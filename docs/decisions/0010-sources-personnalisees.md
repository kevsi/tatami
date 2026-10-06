# 0010 — Sources personnalisées scriptées (JavaScript)
- Date : 2026-10-06
- Statut : acceptée

## Contexte

L'écosystème scantrad français est instable : en octobre 2026, BentoManga, LireScan, InovaScanManga et
RimuScans sont hors ligne ; MangaKawaii et Anime-Sama ont changé de domaine. Les parsers de
`kotatsu-parsers-redo` couvrent ~1370 sources, mais aucune mise à jour du JAR ne peut suivre le rythme
des fermetures/migrations. L'utilisateur a besoin de pouvoir brancher une source sans recompiler l'app.

## Décision

Introduire un mécanisme de **sources personnalisées scriptées** : l'utilisateur dépose un fichier `.js`
dans un dossier dédié (`$APPDATA/Tatami/sources/`). Le moteur JVM charge ce script via Nashorn (déjà
présent pour `evaluateJs`) et l'expose comme une source ordinaire au reste de Tatami.

**Contrat d'un script source** — le script doit exporter un objet global `SOURCE` avec :

```js
const SOURCE = {
  id:       "mon-site",          // identifiant unique, lettres/chiffres/tirets
  name:     "Mon Site VF",       // nom affiché
  lang:     "fr",                // code BCP-47
  baseUrl:  "https://...",

  // Retourne [{id, title, coverUrl, url}] — page 1-based, "" si vide
  async getList(page, query) { ... },

  // Retourne {title, description, coverUrl, status, authors, tags, chapters}
  // chapters : [{id, title, number, url}]
  async getDetails(mangaUrl) { ... },

  // Retourne [{id, url}] — URLs d'images de la page
  async getPages(chapterUrl) { ... },
};
```

Le moteur fournit au script un objet `http` (GET/POST → texte) et `html` (parseur CSS léger) via le
contexte d'exécution Nashorn. Les appels réseau passent par l'OkHttpClient existant (rate-limiter,
Cloudflare interceptor, cookies de session) — le script ne fait jamais de requête directe.

**Stockage** : les sources perso sont enregistrées dans SQLite (`custom_sources`) comme les sources
normales. Les favoris, l'historique et les téléchargements référencent le `source_id` du script.

**Sécurité** :
- Le script s'exécute dans un contexte Nashorn restreint (pas d'accès fichier, pas de `java.lang.Runtime`).
- `http.get` et `http.post` n'acceptent que des URLs `https://` (pas d'accès réseau local).
- La taille maximale d'un script est 256 Ko.
- L'utilisateur importe explicitement le fichier — pas de téléchargement automatique non signé.

## Alternatives étudiées

- **Source JSON déclarative** : plus simple, mais ne peut pas gérer les APIs avec logique conditionnelle
  (tokens, pagination variable, décodage base64). Rejeté : couvre trop peu de cas réels.
- **Extension JAR Kotlin signée** (style Tachiyomi) : le plus puissant, mais nécessite un écosystème de
  build + signature + distribution. Hors portée v1.
- **HTTP local (serveur proxy utilisateur)** : l'utilisateur expose une API REST locale que Tatami
  interroge. Élégant mais exige que l'utilisateur maintienne un serveur. Rejeté pour la friction.

## Conséquences

- Côté moteur : `ScriptedParser.kt` + `ScriptedSourceLoader.kt` + `HttpBridge.kt` (bridge OkHttp→Nashorn).
- Côté Rust : table `custom_sources` (migration v9), commandes `importer_source`, `lister_sources_custom`,
  `supprimer_source_custom`.
- Côté UI : écran de gestion des sources perso (import par fichier, liste, suppression).
- Le moteur expose les sources perso via `listSources` comme n'importe quelle autre source, avec un flag
  `custom: true` dans la réponse.
- Une source perso dont le script lève une exception renvoie `PARSE_ERROR` ou `SOURCE_UNAVAILABLE`
  selon le cas — même traitement que les sources natives en erreur.
- Le moteur ne met **jamais** à jour un script automatiquement. C'est toujours un geste explicite de
  l'utilisateur.
