# PLAN.md — Tatami (nom provisoire)

> Feuille de route vivante. L'agent la met à jour selon les règles de `AGENTS.md` (section 12).
> Créé le 2026-10-02. Tailles relatives : S, M, L (pas des durées).

---

## 1. Vision

Un client desktop qui fait sur PC tout ce que fait l'app mobile **Kotatsu-Redo** (lecteur de manga Android open source, https://github.com/Kotatsu-Redo/Kotatsu-Redo), en réutilisant ses sources via la librairie `kotatsu-parsers-redo` (Kotlin/JVM, 1200+ sources).

Principe directeur : **séparer ce qui change tout le temps (les sources) de ce qui doit rester stable (l'app et les données)**. Les sites cassent chaque semaine ; le moteur de sources doit pouvoir être mis à jour sans republier l'app.

## 2. Architecture

```
Interface (Next.js export statique, dans la webview Tauri)
        │  commandes / événements Tauri
        ▼
Cœur Rust (Tauri v2)
   ├─ Données        SQLite (bibliothèque, favoris, historique, signets)
   ├─ Hors ligne     téléchargements, CBZ
   ├─ Planificateur  mises à jour de chapitres, notifications
   └─ Sync / suivi   AniList, MAL, Shikimori, Kitsu, sync entre appareils
        │  contrat versionné (NDJSON sur stdio)
        ▼
Moteur de sources (sidecar JVM) ── kotatsu-parsers-redo ──► sites de manga
```

Structure du dépôt :

```
tatami/
  AGENTS.md  PLAN.md  README.md  LICENSE (GPLv3)
  apps/desktop/    Next.js (export statique) + src-tauri (Rust)
  engine/          Projet Gradle Kotlin : le sidecar
  contract/        Schéma JSON partagé -> types Rust et TypeScript
  scripts/         Scripts PowerShell de build (jlink, packaging)
  docs/            notes/, decisions/ (ADR), smoke-test.md, spike-phase0.md
```

## 3. Périmètre

> **Exigence permanente (demandée par l'utilisateur le 2026-10-02) :** tout
> téléchargement doit être **reprenable**. Un webtoon entamé doit pouvoir être
> repris là où il s'est arrêté, sans jamais retélécharger ce qui est déjà
> présent — y compris après un redémarrage de l'application ou une coupure
> réseau. Un téléchargement non reprenable est un **défaut**, pas une limite.
> Critères de vérif : `missing_chapters` ne renvoie que l'absent (test
> unitaire), un `.part` compte comme absent, et l'interface expose « Reprendre »
> sur tout webtoon partiel.

**Dans la v1** : sources, recherche et filtres ; bibliothèque, favoris, catégories, historique, signets ; lecteur paginé et webtoon ; téléchargements et CBZ ; notifications de nouveaux chapitres ; suivi AniList/MAL/Shikimori/Kitsu ; sync ; import d'un backup mobile.

**Ordre v1 (décision humain, 2026-10-02)** : catalogue (1.1→1.7) puis téléchargements (phase 2, en bloc ou par sélection) puis lecteur (1.8/1.9) en dernier. La lecture est différée, pas supprimée.

**Hors v1** (ne pas faire sans validation) : recommandations, verrou de l'app, builds macOS/Linux (Windows d'abord), tout nouveau service de suivi, thèmes avancés.

## 4. Statut des phases

| Phase | Titre | Taille | Statut |
|---|---|---|---|
| 0 | Fondations et spike des risques | M | **En cours** — 0.1 à 0.11 faits, 0.12 partiel, 0.13 à 0.17 à faire |
| 1 | Bibliothèque et lecteur | L | **Finie** — 1.1 à 1.10, dont 1.8 et 1.10 livrés le 2026-10-05 |
| 2 | Hors ligne | M | **Faite** — file, reprise, CBZ, lecture hors ligne, assainissement et stats ; durcie le 2026-10-04 (un `.cbz` par chapitre, `sync_all`, verrou RAII) ; dossier de téléchargement au choix et pause de la file le 2026-10-05 |
| 3 | Tâches de fond et notifications | M | **Pas commencée** |
| 4 | Comptes et synchronisation | L | **Pas commencée** |
| 5 | Durcissement et distribution | M | **Partiellement entamée** — audit de code du 2026-10-04 et ses 5 correctifs livrés |

Phase en cours : **1 et 2 en parallèle**, la phase 2 ayant un manque bloquant.
Prochaine tâche : 1.10 (thèmes clair/sombre), puis la dette de phase 0 (0.13 → 0.17).

**Correction du 2026-10-04** : le tableau disait « phase 2 pas commencée ». C'était
faux, et c'est vérifiable dans le code — `download.rs` (file séquentielle,
`missing_chapters` pour la reprise, `clean_partials`, `sanitize`), `cbz.rs`
(écriture ZIP sans dépendance), `library.rs` (`existing_chapters`,
`remove_download`, tailles, `meta.json`). Un inventaire doit être **lu dans le
code**, jamais déduit de l'absence de cases cochées.

---

## 5. Phases détaillées

### Phase 0 — Fondations et spike des risques (M)

Objectif : prouver que les trois risques majeurs sont maîtrisables avant de construire le reste (sidecar JVM empaquetable, images servies correctement, sources protégées par Cloudflare).

- [x] **0.1** Initialiser le dépôt : git, workspace pnpm, `LICENSE` (GPLv3), `README.md` avec crédits (Kotatsu, Kotatsu-Redo, kotatsu-parsers-redo) et avertissement (l'app est un navigateur, elle n'héberge aucun contenu).
- [x] **0.2** Vérifier l'outillage (`java -version`, `gradle -v`, `rustc --version`, `node -v`, `pnpm -v`) et installer ce qui manque (voir `AGENTS.md` section 3). Consigner les versions dans le journal.
- [x] **0.3** Lire le code de `kotatsu-parsers-redo` et de l'implémentation non-Android de `kotatsu-dl`. Écrire `docs/notes/loader-context.md` : méthodes de `MangaLoaderContext` à implémenter, celles qui posent problème sur desktop (exécution de JavaScript, recomposition d'images), coordonnées de dépendance **exactes** (À VÉRIFIER dans le README du dépôt, ne pas les deviner).
- [x] **0.4** Créer `engine/` (Gradle Kotlin) avec la dépendance sur une version **épinglée** des parsers. CLI de test : lister les sources, rechercher dans une source. Vérifier avec une source ouverte (une API officielle comme MangaDex est un bon candidat).
- [x] **0.5** Rédiger `contract/` v0 (schéma JSON : `hello`, `listSources`, `search`, `details`, `chapters`, `pages`, `fetchImage`, `setCookies`, `health`, `shutdown`, erreurs typées) et l'ADR `0001-transport-sidecar.md` (NDJSON sur stdio vs HTTP local avec jeton, avec compromis).
- [x] **0.6** Implémenter le mode serveur du moteur selon le contrat (stdout = protocole, stderr = logs). Test d'intégration `hello` → `listSources` → `search`.
- [x] **0.7** Empaqueter un runtime réduit avec `jdeps` + `jlink` (script `scripts/build-engine.ps1`). Mesurer et noter : taille du runtime, temps de démarrage à froid, mémoire au repos et en usage (viser un tas limité, par ex. `-Xmx256m`, GC série, à valider par mesure).
- [x] **0.8** Scaffolder Tauri v2 + Next.js (export statique). Déclarer le sidecar (`externalBin`, plugin shell : vérifier la configuration et les permissions dans la doc Tauri v2). Superviseur minimal : démarrage, détection de crash, relance.
- [x] **0.9** Commandes Tauri `search` / `chapters` / `pages` + page d'interface minimale qui affiche une liste de résultats.
- [x] **0.10** Protocole d'URI personnalisé `tatami://page/...` : Rust demande `fetchImage` au moteur, met en cache sur disque, sert l'image à la webview (vérifier l'API Tauri v2 des protocoles asynchrones dans la doc). Afficher une page de chapitre dans l'interface.
- [x] **0.11** Source protégée (Cloudflare/captcha) : si le moteur renvoie `CHALLENGE_REQUIRED`, ouvrir une fenêtre webview, laisser l'utilisateur résoudre, récupérer cookies et user-agent (vérifier ce que l'API de Tauri v2 permet pour les cookies), les transmettre via `setCookies`, rejouer la requête. Documenter les limites observées (liaison à l'IP, expiration).
- [x] **0.12** **Livré** le 2026-10-05. `docs/spike-phase0.md` écrit, `docs/smoke-test.md` déjà livré, commandes de vérification figées dans `AGENTS.md` §8. Première mesure sérieuse du moteur (jamais faite) : bannière 880 ms, `hello` 104 ms, `listSources` 180 ms pour 1 370 sources, arrêt propre 1 208 ms ; RAM 68 Mo au démarrage, 136 Mo après le premier appel réseau puis plate — `-Xmx256m` serré mais tenable. **Trouvé en passant** : `gradlew test` était une commande **fictive** (§2 le prohibait) — pas de `gradlew.bat` dans le dépôt ; et le cache d'images est borné en **nombre** d'entrées (2048) mais pas en **volume**, soit ~521 Mo possibles pour 260 Ko de moyenne sur une machine de 12 Go. Ce plafond rejoint la tâche 2.5.
- [x] **0.13 (nouveau)** Contrat : ajouter un code d'erreur `AUTH_REQUIRED`. Mesuré : `AuthRequiredException` n'est levée que sur HTTP 401 par 16 parseurs, arrive aujourd'hui comme `INTERNAL`, et ne peut pas correspondre à `CHALLENGE_REQUIRED` (qui exige 403/503 + marqueur Cloudflare). **Livré** : `AUTH_REQUIRED` au contrat, déballage de l'exception dans le moteur, message i18n dédié, non transitoire côté superviseur.
- [x] **0.14 (nouveau)** Moteur : ne pas perdre `CHALLENGE_REQUIRED` quand le parser emballe l'exception. Mesuré : MANGANATO et KAGANE renvoient `INTERNAL: IOException: canceled due to CloudFlareProtectionException` — un vrai challenge Cloudflare rapporté comme panne interne, donc la fenêtre de résolution de 0.11 ne s'ouvrirait pas. **Livré** : `Erreurs.kt` déballe la chaîne de causes *et* le message (OkHttp 5 KMP perd parfois la cause). Mesuré : les deux sources renvoient `CHALLENGE_REQUIRED` + URL.
- [x] **0.15 (nouveau)** Superviseur : envoyer `shutdown` avant de fermer stdin. Mesuré : fermer stdin ne tue pas le moteur (process encore vivant 4 s plus tard) ; `shutdown` donne `exit 0` en 137 ms. Garder le `kill` en secours. **Livré** dans le correctif du 2026-10-04 : `workers.shutdown()` + compteur de requêtes en vol au lieu de `shutdownNow()`.
- [ ] **0.16 (nouveau)** 3 sources sur 31 échouent en `PKIX path building failed`. **Le diagnostic d'origine était faux** : ce n'est pas le runtime `jlink`. Mesuré le 2026-10-05, le runtime embarqué **et** le JDK complet échouent identiquement, et le magasin de confiance embarqué contient bien `cacerts`. Vraie cause, deux manques indépendants : (a) le serveur n'envoie que la feuille et l'intermédiaire Let's Encrypt `YE1`/`YE2` est absent — la JVM ne fait pas de récupération AIA ; (b) la chaîne se termine sur une racine Let's Encrypt `Root YE` absente du `cacerts` embarqué, de celui du JDK Temurin 21 **et** du magasin Windows de cette machine (qui ne contient que `ISRG Root X1`). Ne **jamais** résoudre en désactivant la validation TLS (AGENTS.md §10) : la voie est un JDK plus récent ou l'ajout explicite de la racine — décision de sécurité qui demande l'accord du projet.
- [x] **0.17 (nouveau)** ZENMANGA servait du chiffrement, pas des images. **Mesuré** : 832 513 octets en `application/octet-stream`, dont les **16 premiers octets identiques sur deux pages différentes** (`AA A1 AC 8B 70 20 08 3C 19 2E 77 6D 77 50 32 57`) — ni JPEG, ni WEBP, ni PNG, ni HTML. Le parser porte `ImageCodec { SEC, XOR }` et `detectImageCodec(url)` ; le déchiffrement se fait dans `MangaParser.intercept`, **jamais appelé**. **Livré** : `ParserActif` (ThreadLocal) + `intercepteurParser`, ajouté **en dernier** dans la chaîne (le plus proche du réseau) pour transformer la réponse avant que CloudFlare et le lazy-load ne la voient. Résultat : les mêmes octets commencent par `FF D8 FF E0`, un JPEG valide. **Piège** : `ParserPool.avec` n'est pas réentrant (Mutex) — un appel imbriqué sur la même source fige le moteur.
- [x] **0.19 (nouveau)** La fenêtre de vérification s'ouvre quand le moteur ne signale **aucun** challenge. Deux déclencheurs côté Rust (URL d'image vide, octets non-image), fenêtre ouverte sur `manga.publicUrl`, panneau dans la vue Téléchargements, 9 clés i18n. Walk 17/17, contrôle négatif (0 `.cbz` écrit) et contrôle positif (`.cbz` MANGADEX relu par un lecteur strict). **Résolu en 0.20** : sur HENTAIORIGINES le cookie n'était pas la cause, et le panneau était invisible depuis l'onglet Catalogues.
- [x] **0.20 (nouveau)** Images lazy-loadées illisibles par les parsers : l'URL est dans `data-src` et non `src`. `LazyLoadInterceptor` côté moteur (9 tests), garde `fetchImage` sur URL vide, `testImplementation(kotlin("test"))` — le moteur n'avait aucun test. **Résultat mesuré** : 45 URL remplies, `.cbz` de 9,3 Mo relu par un lecteur strict (45 entrées, 45 WEBP, 45 tailles distinctes), MANGADEX non régressé, aucune fenêtre de vérification. Résiduel : `032.webp` = placeholder 413×42 servi par le site.
- [x] **0.18 (nouveau)** Un chapitre sans titre (`title: null`) donne un `.cbz` nommé `22d20970-38d1-48aa-bbd3-9ca9afbbb3dc.cbz` (mesuré sur MANGADEX). Le fichier est valide mais inutilisable dans un lecteur externe, ce qui vide l'exigence permanente « un `.cbz` par chapitre ». Corriger `chapterLabel` / le nom de fichier : retomber sur un numéro de chapitre lisible (« Chapitre 1.cbz »), jamais sur l'URL ni un UUID. Vérifier aussi l'assainissement du nom (AGENTS.md §6).

### Phase 0 bis — Adaptation de l'interface d'après l'analyse de sushiscan.net (M)

Origine : analyse de structure et de style de `sushiscan.net` (WordPress + thème custom dérivé Madara, LiteSpeed + Cloudflare, 4 écrans, lecteur à 4 contrôles). Demande humaine : 5 chantiers retenus le 2026-10-03. Ces tâches affinent l'interface déjà livrée en 0.9–0.11 ; elles ne remplacent pas la phase 1.

- [x] **0.13** Lecteur : ajustement à la largeur ou à la hauteur, double page, sens de lecture. Critères : `Par défaut` / `Ajusté à la hauteur` → vérifié par `getComputedStyle(img).width/height` distincts entre les deux modes ; double page → vérifié par 2 `<img>` dans `.reader-stage` et un pas de 2 pages ; RTL par défaut.
- [x] **0.14** Lecteur : sélecteur de chapitre + saut de page (`n/N` cliquable). Critères : `<select>` de chapitres dans `.reader-bar` × `aria-label="Changer de chapitre"` ; champ numérique × `aria-label="Aller à la page"` qui positionne l'index.
- [x] **0.15** Fiche série : chapitres en grille 5 colonnes (numéro + date de publication) + filtre par numéro/titre. Critères : `.chapter-grid` à `repeat(auto-fill, minmax(...))` avec ≥ 5 colonnes à 1440 px ; date affichée → vérifié par sonde moteurconfirmant `uploadDate` non nul ; filtre → vérifié par capture.
- [x] **0.16** Catalogue : derniers chapitres dans la carte (comme « Ch. 392 · 5 mois »). Critères : concurrence bornée (4), cache localStorage 30 min, interrupteur on/off, compteur `x/y` chargés. Limite assumée : 1 requête `chapters` par carte affichée.
- [x] **0.17** Style : bandeaux de section, table d'informations 2 colonnes sur la fiche série, badges. La table exige la commande Rust `details` + `mangaToJson` étendu (`description`, `state`, `tags`) → rebuild du moteur + redémarrage de l'app. Critères : `cargo clippy -D warnings` 0, `gradle installDist` OK, table rendue avec ≥ 1 ligne.

**Critère de sortie** : lire un chapitre complet d'une source ouverte dans l'interface Tauri avec le moteur empaqueté, et un résultat documenté (succès ou limites claires) pour une source protégée.

### Phase 1 — Bibliothèque et lecteur (L)

Objectif : pouvoir remplacer le mobile pour lire en ligne.

- [x] **1.1** SQLite côté Rust, système de migrations forward-only, sauvegarde avant migration, tests de migration.
- [x] **1.2** Catalogue de sources : liste, activation/désactivation, langue, état (alimenté par `health`).
- [x] **1.3** Recherche et filtres (nom, genres, tri) avec pagination.
- [x] **1.4** Page détail d'un manga : infos, liste des chapitres, ajout aux favoris.
- [x] **1.5** Favoris organisés en catégories définies par l'utilisateur.
- [x] **1.6** Historique de lecture, reprise au bon chapitre et à la bonne page, mode incognito.
- [x] **1.7** Signets.
- [x] **1.8** Lecteur paginé : sens de lecture, zoom, préchargement via le cache Rust, raccourcis clavier et souris. **Livré** le 2026-10-05. Sens de lecture, zoom, double page et RTL étaient déjà faits ; il restait deux choses, et **les deux n'étaient pas ce qu'elles semblaient être**. La précharge fonctionnait (elle écrit bien dans le cache Rust) mais ne couvrait qu'une page → désormais 4. La molette ne faisait rien en mode paginé → ajoutée, avec seuil de 40 px et verrou de 300 ms par sens (un geste émet 43 événements : mesuré).
- [x] **1.9** Mode webtoon (défilement continu, liste virtualisée).
- [x] **1.10** Thèmes clair/sombre, i18n français et anglais en place. **Livré** le 2026-10-05 : l'i18n existait déjà (152 clés) ; le thème clair, non. Le travail réel n'était pas d'ajouter une palette mais de **tokeniser les 69 couleurs écrites en dur** hors tokens — sans quoi rien n'aurait basculé. `--reader-bg` reste identique dans les deux thèmes (pages blanches, un fond clair ferait disparaître leurs bords).

**Critère de sortie** : recherche → ajout en favori → lecture → fermeture → reprise exacte à la réouverture, sur au moins 3 sources différentes, avec tests passants.

### Phase 2 — Hors ligne (M)

- [x] **2.1** Gestionnaire de téléchargements : file d'attente, reprise après interruption, concurrence limitée. *(fait sauf « débit limité par source » : la file est séquentielle globalement, ce qui est plus strict qu'un débit mais pas équivalent)*
- [x] **2.2** Stockage CBZ, noms de fichiers assainis. *(fait sauf « chemin en base » : le chemin est dérivé de `downloads_root` + source + titre et un `meta.json` fait foi — cf. dérogation signalée au journal, à trancher)*
- [x] **2.3** Lecture hors ligne des chapitres téléchargés. **Bloquant** : `cbz.rs` sait écrire un ZIP mais rien ne sait le lire, le lecteur passe toujours par le moteur.
- [ ] **2.4** Import de CBZ tiers et lecture locale ; export d'un chapitre en CBZ.
- [x] **2.5** Gestion de l'espace : taille utilisée, suppression. *(emplacement configurable pas fait)*

**Critère de sortie** : télécharger un chapitre, couper le réseau, le lire ; reprendre un téléchargement interrompu ; importer un CBZ externe.

### Phase 3 — Tâches de fond et notifications (M)

- [ ] **3.1** Planificateur Rust (tokio) : vérification périodique des nouveaux chapitres des favoris, avec limites de débit.
- [ ] **3.2** Flux "mises à jour" dans l'interface.
- [ ] **3.3** Notifications système (plugin Tauri, à vérifier dans la doc).
- [ ] **3.4** Icône dans la zone de notification, fermeture dans la zone de notification, démarrage automatique optionnel.
- [ ] **3.5** Réglages : fréquence, sources concernées, activation.

**Critère de sortie** : un nouveau chapitre (simulé ou réel) déclenche une notification pendant que l'app est réduite dans la zone de notification.

### Phase 4 — Comptes et synchronisation (L)

- [ ] **4.1** Étude (docs/notes/) : protocole de `kotatsu-syncserver` (dépôt archivé, GPL-3.0) et format du backup mobile. Ne rien implémenter avant d'avoir lu le code.
- [ ] **4.2** Suivi AniList (OAuth avec redirection locale ou deep link, à vérifier), stockage des jetons dans le trousseau OS.
- [ ] **4.3** Suivi MyAnimeList, puis Shikimori, puis Kitsu.
- [ ] **4.4** Synchronisation de la progression sur chaque chapitre lu.
- [ ] **4.5** Client de sync compatible avec le serveur de sync de Kotatsu (si l'étude 4.1 montre que c'est faisable) ; sinon ADR avec alternative.
- [ ] **4.6** Import du backup mobile (favoris, catégories, historique).

**Critère de sortie** : suivi mis à jour après lecture, import d'un backup mobile réel sans perte de favoris ni d'historique, sync fonctionnelle entre l'app et un second appareil ou une instance de test.

### Phase 5 — Durcissement et distribution (M)

- [ ] **5.1** Mise à jour du moteur : manifeste signé, vérification de signature et de hash, remplacement atomique, retour arrière si échec au démarrage.
- [ ] **5.2** Mise à jour de l'app via l'updater Tauri.
- [ ] **5.3** Tests de contrat sur réponses enregistrées, exécutés en intégration continue.
- [ ] **5.4** Écran "état des sources" (dernière réussite, dernière erreur, version du parser) pour repérer ce qui est cassé.
- [ ] **5.5** Journaux de diagnostic exportables, sans données personnelles.
- [ ] **5.6** Installeurs Windows (`.msi`), signature du code, documentation utilisateur et contributeur.
- [ ] **5.7** Politique de suivi de l'amont : procédure pour intégrer les nouvelles versions de `kotatsu-parsers-redo`, miroir/fork de secours.

**Critère de sortie** : installation propre sur une machine neuve, mise à jour du moteur sans republier l'app, suite de tests verte en CI.

---

## 6. Questions ouvertes / À VÉRIFIER

- [ ] Coordonnées de dépendance exactes de `kotatsu-parsers-redo` (README du dépôt).
- [ ] Ce que `MangaLoaderContext` exige (exécution de JS, recomposition d'images, config par source) et comment `kotatsu-dl` le gère.
- [ ] API Tauri v2 pour : sidecar `externalBin` + permissions, protocole d'URI asynchrone, cookies d'une webview, notifications, deep link / redirection locale OAuth, updater.
- [ ] Protocole de `kotatsu-syncserver` et format du backup mobile.
- [ ] Disponibilité du nom "Tatami" (GitHub, stores, domaine) ; alternatives : Irori, Mikan.
- [ ] Empreinte mémoire réelle du sidecar sur la machine cible (12 Go de RAM, outils de dev ouverts).
- [ ] **0.11 de bout en bout** : confirmer sur un vrai site protege que la fenetre de resolution + `setCookies` debloque la source. Aucune source de l'echantillon (MANGALIB, BAOZIMH) ne renvoie `CHALLENGE_REQUIRED` (403 classiques). Vigilance : clearance liee a l'IP et expirante, empreinte TLS WebView2 differente d'OkHttp.

## 7. Risques

| Risque | Impact | Parade |
|---|---|---|
| Cloudflare/captchas sur certaines sources | Sources inutilisables | Fenêtre de résolution + cookies (0.11), documenter les limites |
| Parsers qui cassent souvent | Perte de confiance | Moteur mis à jour séparément (5.1), écran d'état (5.4) |
| Amont archivé (dépôts KotatsuApp) | Dépendance fragile | Fork/miroir (5.7), version épinglée |
| JVM lourde sur machine modeste | Lenteur | Runtime `jlink`, tas limité, mesures en 0.7 |
| Périmètre trop large pour un seul développeur | Épuisement, abandon | Phases courtes, critères de sortie, hors v1 explicite |
| Divergence de protocole de sync avec le mobile | Pas de sync | Étude 4.1 avant tout code, ADR |

## 8. Blocages

_Aucun pour l'instant. L'agent ajoute ici : date, tâche, description, ce qui a été essayé, option proposée._

## 8 bis. Travail réparti sur plusieurs sessions (depuis le 2026-10-04)

Décision de l'utilisateur : l'avancement est réparti sur **trois sessions OpenCode
sur le même dossier**. Contraintes mesurées qui ont dicté la répartition :

- **4 Go de RAM libres** sur 11,9 : une seule instance de `pnpm tauri dev`
  (next + cargo + JVM + WebView2) tient à peine ;
- ports **3000** et **9223** déjà pris par l'app de la session 1 ;
- `apps/desktop/app/page.tsx` fait ~3 000 lignes, un seul fichier : toute
  fonction le touche ;
- `SCHEMA_VERSION` est une constante unique dans `store.rs`.

Le **code n'est donc pas partageable**, le reste si.

| Session | Fichiers qu'elle seule peut modifier | Mission |
|---|---|---|
| **1 — code** | `apps/`, `engine/`, `AGENTS.md`, **`PLAN.md`**, **seule à commiter** | 1.5 catégories, 1.9 virtualisation webtoon, 1.10 thèmes + i18n |
| **2 — docs** | `docs/`, `README.md` | 0.12 `docs/smoke-test.md`, ADR de la §9, `README.md` |
| **3 — recherche** | `docs/notes/` uniquement | trancher les « À VÉRIFIER » par la mesure |

Règles données aux trois sessions :

1. **Jamais** de `pnpm dev`, `pnpm tauri dev`, `next dev`, `cargo`, `gradle`
   ailleurs que dans la session 1.
2. **Jamais** de commande git ailleurs que dans la session 1 : un `git add -A`
   prendrait les fichiers des autres dans le même commit.
3. **Jamais** de modification de `PLAN.md` ailleurs que dans la session 1 :
   c'est le tableau de coordination, il est recalculé à partir des sous-tâches.
4. La session 3 n'utilise **ni Playwright ni le port 9223** : elle mesurerait
   l'app pendant que la session 1 s'en sert. Elle parle au moteur seul en
   NDJSON.
5. Chaque session laisse son bilan dans `docs/rapport-session-*.md` ; la session 1
   récupère ces fichiers dans un commit « docs » dédié.

## 9. Journal

### 2026-10-05 — Spike phase 0 (0.12)

- Plan : écrire `docs/spike-phase0.md` et figer les commandes de vérification.
  Choisi pour cette période parce que c'est le seul travail **sans fichier
  partagé** — les deux autres sessions écrivent dans `src-tauri`, et un doc ne
  peut pas entrer en conflit avec elles.
- **Commande fictive trouvée.** `AGENTS.md` §8 disait `./gradlew test`. Vérifié :
  `Get-ChildItem gradlew*` ne renvoie rien — Gradle est installé globalement via
  Scoop, il n'y a pas de `gradlew.bat` dans le dépôt. La commande n'avait **jamais
  pu tourner**, et figurait dans le document de règles depuis le début. C'est
  exactement ce que la règle d'or n° 2 interdit. Corrigée : `gradle test` depuis
  `engine/`, et chaque commande porte maintenant son dossier.
- **Moteur mesuré pour la première fois**, en le lançant exactement comme le
  superviseur (mêmes options JVM — j'avais d'abord mis `-Xmx1g` et oublié
  `serve`, le moteur sortait en 323 ms sans répondre et la mesure ne voulait rien
  dire) : bannière 880 ms, `hello` 104 ms, `listSources` 180 ms pour 1 370
  sources (47 417 octets), arrêt propre 1 208 ms, code de sortie 0. RAM 68,5 Mo au
  démarrage → **136,5 Mo après le premier appel réseau**, puis plate.
- **Ce que je n'ai pas mesuré, et dit comme tel** : la *cause* des 57 Mo ajoutés.
  L'hypothèse (3 844 classes de parseurs) est plausible mais demanderait
  `-verbose:class`. Et MANGAFR a renvoyé 0 octet, MANHWA 0,1 Ko : ces deux
  appels ne disent rien sur un vrai parcours, donc je ne les cite pas comme
  « source rapide ».
- **Trouvé en passant** : le cache d'images est borné en nombre (`CAP_PAGE =
  2048`, éviction qui supprime bien le fichier) mais **pas en volume**. Mesure :
  260,5 Ko de moyenne, 6 121 Ko max → **~521 Mo possibles** pour une base de
  64 Ko. Croissance non bornée et silencieuse ; rejoint 2.5.
- Porte vérifiée réellement exécutée : `gradle test` = **24 tests, 0 échec**
  (5 fichiers XML). Un `UP-TO-DATE` seul ne prouve rien.
- `docs/spike-phase0.md` : 215 lignes, aucune mesure inventée, chaque limite
  énoncée.

### 2026-10-05 — Lecteur paginé (1.8)

- Plan : auditer ce qu'il restait vraiment de la 1.8 avant de coder. Sens de
  lecture, zoom, double page et RTL étaient déjà faits ; le `PLAN.md` disait
  « la précharge ne couvre que la page suivante », et la molette n'était pas
  mentionnée du tout.
- **Ce que j'ai d'abord conclu, et qui était faux** : « la précharge ne fait
  rien ». Mesuré après 30 s d'attente, zéro écriture sur le disque. Cause : je
  supprimais le fichier de cache **après** que Chromium l'avait déjà en
  mémoire, donc son cache HTTP répondait en 4 ms et Rust n'était jamais appelé.
  La preuve était dans l'**écriture disque** d'une page jamais demandée, pas
  dans un chronomètre — avec `Network.setCacheDisabled` pour écarter le second
  cache, la balise a écrit 147 Kio en moins de 2 s. Même famille d'erreur que le
  scroll : 173 px de course sur une page de 918 px, c'est le maximum, pas un
  blocage.
- Codé : `PRECHARGE_PAGES = 4` (une balise par page, plafond volontaire pour ne
  pas télécharger ce que l'utilisateur n'atteindra pas), et `onMoletteStage` sur
  `.reader-stage` avec seuil de 40 px, verrou de 300 ms par sens, et `Ctrl`
  réservé au zoom. En webtoon la molette n'est pas interceptée.
- Preuve (sur l'app réelle, port CDP 9223) : 4 balises distinctes, aucune visible
  (`getBoundingClientRect` 1×1), image affichée 800×4375 ; molette delta 300 →
  page change, delta 12 → rien, delta −300 → recul ; **contrôle négatif webtoon** :
  la bande défile de 660 px et le compteur ne bouge pas.
- **Bug trouvé en passant, non demandé** : `apparence.ts` et `i18n.ts` posaient
  `data-apparence` et `lang` sur `<html>` au chargement du module, donc **avant**
  l'hydratation de React, qui signalait l'écart en boucle à chaque navigation.
  Corrigé par un script bloquant dans `layout.tsx` + `suppressHydrationWarning`.
  Pour le lire, `console.error` ne suffit pas (message tronqué) : il faut un
  `addInitScript` qui intercepte avant React — la fin du message porte le diff.
- **Non vérifié** : la lecture de ce correctif d'hydratation *sur l'app*. La
  session 2 écrit dans `src-tauri`, ce qui redémarre Tauri à chaque modification
  et fait tomber le port 9223. Vérifié sans elle : le script est bien dans le
  HTML produit et précède React (les 5 bundles qui le précèdent portent
  `async=""`). **À confirmer à l'œil dès que les sessions Rust s'arrêtent.**

### 2026-10-05 — Thème clair (1.10)

- Plan : ajouter le thème clair et le sélecteur d'apparence. Deux surprises
  mesurées avant de coder, qui ont changé le contenu de la tâche.
- **Le vrai travail était la tokenisation.** La feuille contenait **69 couleurs
  écrites en dur** hors des tokens ; un bloc `[data-apparence='clair']` qui ne
  surcharge que des tokens ne les aurait pas fait basculer. Il a fallu créer les
  tokens manquants (`--line-strong`, `--accent-hover`, `--accent-dim-texte`,
  `--danger-*`, `--succes-*`, `--favori`, `--voile`, `--sur-voile`…), puis
  remplacer chaque usage. Contrôle final : **0** couleur en dur hors tokens (les
  8 couleurs de catégories sont des données, pas du style, et restent).
- **L'état devait vivre dans le module.** Première version : `useState` +
  appel à `setApparence`. Le setter du composant était **écrasé par l'import du
  module** — React n'était jamais prévenu, et le `<select>` affichait une valeur
  que rien ne portait. Mesuré : `localStorage` restait vide après un clic. Corrigé
  par `useSyncExternalStore`, comme la langue.
- **Deux contrastes trouvés en mesurant, pas en relisant le CSS :**
  - le texte posé sur `--accent-dim` : `--text` ne donnait que **2,98:1** en
    thème clair. Il fallait son propre token (`--accent-dim-texte`), car un voile
    à 12 % n'a pas la même profondeur sur fond noir et sur fond blanc → **9,22:1** ;
  - la bordure de champ : `#b4b8c2` ne faisait que **1,99:1** sur blanc, la
    bordure était invisible. Une bordure demande 3:1, pas 4,5:1 — ce n'est pas
    du texte (WCAG 1.4.11) → `#8b8f99`, **3,24:1**.
- Le **lecteur reste sombre dans les deux thèmes**, par choix : les pages sont
  majoritairement blanches, un fond clair ferait disparaître leurs bords. Les
  voiles posés sur les images aussi — ils sont faits pour être sombres.
- Corrigé au passage : `document.documentElement.lang` valait `fr` pendant que
  l'écran affichait l'anglais. L'attribut n'était posé que dans `setLang`, jamais
  au démarrage ; un lecteur d'écran annonçait donc la mauvaise langue.
- Preuve : bascule depuis l'interface, les deux thèmes, quatre vues capturées
  plus le lecteur sur un chapitre réel. Contrastes mesurés —
  texte/fond **21,00 → 16,17**, muted **7,37 → 4,59**, bouton primaire
  **5,05 → 5,97**, accent **17,40 → 10,94**, chip pressé **14,97 → 9,22**,
  bordure de champ **1,79 → 3,24**. `localStorage tatami.apparence = clair`
  survit au rechargement. `tsc` 0, `lint` 0 erreur, `build` OK, `i18n:check` 152/152.

### 2026-10-05 — Dossier de téléchargement au choix + pause de la file

- Plan : deux demandes de l'utilisateur. (1) Pouvoir choisir le dossier de
  téléchargements. (2) Pouvoir mettre en pause un téléchargement en cours. La
  première touche au stockage et aux permissions, la seconde à la file : deux
  questions d'architecture posées **avant** de coder, l'utilisateur ayant tranché
  « sélecteur natif » et « pause au chapitre ».
- **Dossier de téléchargement.** Le chemin était figé dans
  `downloads_root()` (`%APPDATA%\Tatami\downloads`). Il est maintenant un
  réglage en base, appliqué par Rust :
  - table `settings` (`store.rs`, migration v7, `CREATE TABLE IF NOT EXISTS` —
    et non `if current < 7`, pour qu'un binaire plus ancien n'écrase jamais une
    base estampillée par un plus récent) + `get_setting` / `set_setting` ;
  - `downloads_root()` lit un global, initialisé **une fois** au démarrage
    depuis la base. Une lecture SQLite dans un chemin appelé des dizaines de
    fois par téléchargement n'apportait rien, et le rendait intestable ;
  - `verifier_dossier_ecriture` **essaie vraiment** d'écrire un fichier :
    un dossier existe sans pour autant être accessible, et l'échec
    n'apparaîtrait sinon qu'au premier téléchargement ;
  - `set_downloads_dir` valide **avant** d'enregistrer. Enregistrer un chemin
    invalide ferait échouer tous les téléchargements ensuite, sans retour
    possible depuis l'interface ;
  - sélecteur natif `tauri-plugin-dialog` + **seule** permission
    `dialog:allow-open` (pas `allow-save`, pas `allow-message`, pas le lot
    `dialog:default`). ADR 0004 révisée : elle interdisait tout plugin.
- **Pause.** La logique existait déjà (arrêt au chapitre suivant, file
  persistée et reprise) mais se lisait comme un abandon : le bouton disait
  « Arrêter après le chapitre en cours » et les éléments restants étaient
  marqués `annulé`. C'est trompeur, parce que la file **se reprend** — c'est
  exactement la différence entre un abandon et une pause. Libellé
  « Mettre en pause », état `pause` affiché « en pause », info-bulle qui
  explique la sémantique (« le chapitre en cours est terminé, puis la file
  s'arrête ; reprendre la reprendra où elle s'est arrêtée »). Au passage :
  « Effacer la file » était du français en dur, hors i18n.
- Preuve :
  - `pnpm typecheck` 0 · `pnpm lint` 0 erreur (4 warnings `exhaustive-deps`
    antérieurs) · `pnpm build` OK · `pnpm i18n:check` **138/138**
  - `cargo fmt --check` 0 · `cargo clippy --all-targets -D warnings` 0 ·
    `cargo test` **60 passés, 0 échec** (dont `un_reglage_ecrit_relu_apres_redemarrage`)
  - Démarrage : `telechargements : \\?\C:\Users\alexanders\AppData\Roaming\Tatami\downloads`
    — la racine vient bien de la base, canonisée.
  - Interface réelle (CDP sur WebView2) : panneau « Dossier de téléchargement »
    présent, chemin affiché, aller-retour `get_downloads_dir` →
    `set_downloads_dir` → `get_downloads_dir` correct, `reset_downloads_dir`
    ramène à 100 % (13 webtoons de nouveau visibles).
  - Pause mesurée de bout en bout : `Mettre en pause` + badge
    `téléchargement` → 10 s plus tard badge **`en pause`** + panneau
    « File interrompue » ; `Reprendre la file` → badge `téléchargement`,
    bannière disparue. File persistée dans `localStorage` entre les deux.
  - La porte `aucune_commande_tauri_nest_morte` a **échoué** juste après
    l'ajout des trois commandes : elles n'étaient pas encore appelées par le
    front. Elle a fait son travail avant même que je pense à m'en servir.
- Points ouverts :
  - Changer de dossier **ne déplace rien** : les `.cbz` déjà écrits restent
    dans l'ancien dossier et la bibliothèque n'affiche plus que ceux du
    nouveau. C'est dit dans l'interface, mais un « migrer les fichiers
    existants » reste à faire si l'utilisateur le veut.
  - La granularité de la pause est le **chapitre**, par choix humain. Une
    pause à la page exigerait un `.part` réellement reprenable (offsets et
    entrées persistés) — chantier distinct, avec un risque sur les `.cbz`.
  - `escape` sur le dialogue natif **a confirmé** un dossier au lieu de
    l'annuler pendant un test CDP, ce qui a déplacé la racine le temps d'une
    mesure. Détournement de test, pas défaut de l'app, mais cela veut dire que
    le dialogue natif n'est pas testable par CDP : seul l'aller-retour de la
    commande l'est.
  - La file affichée n'a aucun réglage « arrêter après N chapitres » ; le seul
    contrôle est la pause.

### 2026-10-04 — Audit du code réel et correction des 5 constats

- Plan : audit du dépôt **sans se fier aux `.md`**, puis correction de chaque
  constat mesuré. Périmètre validé par l'utilisateur : le suivi AniList/MAL est
  **hors v1** (decision humaine : l'app sera terminee). L'audit a fait
  ressortir deux constats d'architecture et trois de robustesse.
- **Constat 1 — les couvertures contournaient le moteur.** Les 6 vignettes
  faisaient `<img src={coverUrl}>` sur une adresse de site, et la CSP
  l'autorisait (`img-src ... https:`) : violation directe du §4 règle 4, avec
  un échec silencieux (vignette vide, console propre). Corrigé de bout en bout :
  `fetchCover` côté moteur, `PageRef::Couverture` + `tatami://cover/<jeton>`
  côté Rust, `coverToken` dans toutes les réponses (moteur **et** base), `https:`
  retiré de la CSP. Au passage : index O(1) au lieu d'un balayage O(n²),
  plafonds d'éviction, écriture de cache atomique.
- **Constat 2 — le contrat était en retard de deux méthodes.** `latest` (qui
  porte l'essentiel du parcours) et `filterOptions` étaient implémentées et
  utilisées mais absentes de l'`enum` `method`. Une porte de validation contre
  le schéma serait passée à côté. `fetchCover` ajoutée, formes de `result`
  décrites, points ouverts honnêtes (`health` n'est appelé par personne).
- **Constat 3 — deux commandes mortes et deux sémantiques opposées.**
  `commands::search` (jamais appelée depuis la 0.9) envoyait `SAFE` par défaut
  quand `latest` n'en envoie aucun : même réglage, deux comportements
  contraires. `library::existing_chapters` remplacée par `missing_chapters`.
  Retirées. Nouvelle porte `aucune_commande_tauri_nest_morte`.
- **Constat 4 — cinq défauts sur le chemin de production d'un `.cbz`.**
  Collision de nom entre chapitres homonymes (« un `.cbz` par chapitre » n'était
  pas tenu), `note_chapter` ignoré en silence, noms réservés Windows non
  gérés, débordement `u32` tronqué sans bruit dans l'écrivain ZIP, `flush` au
  lieu de `sync_all` (donc un `.cbz` tronqué portant le nom définitif). Plus le
  verrou de téléchargement qui restait pris sur une panique.
- **Constat 5 — i18n mesuré sur le dictionnaire, pas sur l'affichage.**
  199 clés pour 108 utilisées : **91 clés jamais affichées**. Sept libellés
  restaient en français en dur. Nettoyé, plus `pnpm i18n:check`.
- **Bonus mesuré — le moteur jetait les requêtes en vol.** Cinq requêtes d'un
  bloc puis fermeture de stdin : **0 ligne sur stdout** (`shutdownNow()`
  interrompait les workers). Corrigé, avec test.
- Preuve :
  - `pnpm typecheck` 0 · `pnpm lint` 0 erreur (4 warnings `exhaustive-deps`
    antérieurs) · `pnpm build` OK · `pnpm i18n:check` 127/127
  - `cargo fmt --check` 0 · `cargo clippy --all-targets -D warnings` 0 ·
    `cargo test` **56 passés, 0 échec**
  - `gradle test` **10 passés, 0 échec** (dont `FermetureStdinTest`)
  - `fetchCover` sur MangaDex : **43 981 octets, signature `FF D8 FF E0`** ;
    `file://` refusé ; repli `coverUrl` OK
  - Fermeture stdin : **0 → 5 réponses** ; `shutdown` : 6/6
  - 4 commits : `7bab520`, `2d26a45`, `df77322`, `a7e83bb`, `4eedc99`
- Points ouverts :
  - **Suivi AniList/MAL, sync et notifications : hors v1** (décision humaine du
    2026-10-04). Le tableau des phases les garde visibles, pas engagements.
  - Aucune porte ne couvre encore le rendu visuel : `page.tsx` fait 3750 lignes
    et n'a aucun test. Une régression d'affichage ne serait vue qu'à l'écran.
  - `store.rs` : `history_title` est la seule brique encore verrouillée par
    version alors que le fichier prône la réparation par présence.
  - Un commit (`5be3849`) est apparu pendant l'audit sans que je l'aie lancé.

_Format d'une entrée :_

```
### AAAA-MM-JJ — Tâche X.Y
- Plan : (3 à 6 lignes)
- Fait :
- Preuve : commande + résumé de la sortie
- Points ouverts :
```

### 2026-10-04 — Tâche 0.19 : la fenêtre de vérification se déclenche vraiment

**Problème mesuré, pas déduit.** Le moteur ne signale `CHALLENGE_REQUIRED` que sur HTTP **403/503** avec un marqueur Cloudflare. Or HENTAIORIGINES répond **200 OK** avec la page HTML de son interstitiel de vérification d'âge (`document.documentElement.className += document.cookie.indexOf('ori_majeur=1')>-1?'':' ori-age-verif'`). Aucun challenge n'était donc jamais signalé, et rien ne s'ouvrait.

Sonde NDJSON directe sur le moteur (`sonde-ho.js`) :
```
pages     = 45
url image = { previewUrl: null, index: 0, url: '' }   ← VIDE, et non pas relative
contentType= text/html  |  taille = 193 283 octets
```
Les 45 URLs d'images sont **vides** : le téléchargement partait chercher l'adresse `""` et écrivait la page d'accueil du site 45 fois. Les 37 entrées identiques de 193 043 octets vues au diagnostic s'expliquent exactement.

**Correctif.** Rust déclenche la fenêtre de vérification (tâche 0.11, déjà construite) dans **deux** cas que le moteur ne voit pas :
1. une page sur N a une URL d'image vide → échec propre, aucun octet écrit ;
2. les octets reçus ne sont pas une image (`image::detect`) → échec propre, fenêtre ouverte.

Le chapitre reste **absent** du disque, donc la règle « ne jamais retélécharger ce qui est là » le retentera au lieu de le croire terminé. L'URL ouverte est `details.result.manga.publicUrl` (mesuré présent sur cette source : `https://hentai-origines.com/manga/i-love-you-very-much/`), sinon `mangaUrl`. Côté interface : un état `verif` distinct de `challenge` (celui-ci est une URL rattachée à la source ouverte, alors que l'évènement porte source + URL + motif et que l'utilisateur peut être sur la vue Téléchargements sans source sélectionnée), un panneau dans la vue Téléchargements, et 9 clés i18n FR/EN.

**Ce que ça ne fait PAS** : rien n'est contourné. L'utilisateur clique lui-même dans une vraie fenêtre de navigateur, sur le parcours prévu du site. La fenêtre utilise déjà l'agent utilisateur **du moteur** (`challenge.rs`), ce qui limite le risque de cookie refusé.

**Preuves — walk sur l'app réelle, 17/17** (`walk-verif.js`, deux contrôles) :
- **Négatif** (HENTAIORIGINES) : échec en 2 359 ms avec `« HENTAIORIGINES demande une vérification (45 page(s) sur 45 sans URL d'image). Une fenêtre va s'ouvrir : confirmez-y vous-même, puis relancez le téléchargement. »` · **0 `.cbz` écrit** (44 avant, 44 après) · 0 nouveau `.part` · fenêtre ouverte (`cancel_challenge` accepte) · panneau présent, visible dans la vue Téléchargements, avec « C'est fait, appliquer ».
- **Positif** (MANGADEX) : téléchargement **réussi** en 16 453 ms, `.cbz` de 8 099 239 octets. Contrôle par un lecteur ZIP **strict** (`zipfile`) : intégrité OK, 11 entrées, genres `{PNG: 2, JPEG: 9}`, **11 tailles distinctes** (donc 11 vraies pages différentes, pas 11 copies du même HTML), 0 entrée vide, médiane 761 959 octets. Sans ce contrôle, le test négatif ne prouverait que « rien de mauvais n'arrive » — pas que la correction n'a pas cassé ce qui marchait.
- Portes : `pnpm typecheck` / `pnpm lint` / `pnpm build` = 0 · `cargo test` **29 passés** · `cargo clippy -D warnings` = 0 · `cargo fmt --check` = 0.

**Limite honnête** : la dernière étape est manuelle et non automatisable (le clic « j'ai plus de 18 ans »). Que la pose du cookie rende les URLs d'images réelles **n'est pas encore prouvé** — c'est la prochaine étape, à faire à la main. Risque documenté dans `challenge.rs` : le cookie est lié au navigateur, et le moteur parle en agent Firefox pendant que la fenêtre est en Chromium.

### 2026-10-04 — Défaut trouvé par le walk : nom de `.cbz` en UUID

Mesuré : quand la source ne renvoie **pas** de titre de chapitre (`title: null`, cas de MANGADEX ici), `chapterLabel` retombe sur l'URL et le fichier s'appelle `22d20970-38d1-48aa-bbd3-9ca9afbbb3dc.cbz`. Le `.cbz` est valide et lisible par Tatami, mais **inutilisable** dans un lecteur externe, ce qui contredit l'intention de l'exigence permanente (« un `.cbz` par chapitre »). Nouvelle tâche 0.18.

### 2026-10-04 — Trait horizontal entre les pages du webtoon

Signalé par l'utilisateur : « il y a des espèces de trait horizontal quand je lis le webtoon ».

Cause : `.webtoon img` portait `border-bottom: 1px solid #0e0d13`, un liseré sombre ajouté en bas de chaque page.

**Cette bordure cassait aussi ma virtualisation, et c'est le plus important.** La virtualisation calcule la position de chaque page à partir d'une hauteur ; en `box-sizing: content-box` (le défaut CSS), toute bordure s'ajoute à cette hauteur. Mesuré : **1 px de dérive par page**, soit **303 px** sur Killer Peter — la fenêtre affichée finissait par ne plus correspondre à son contenu. Le correctif est `box-sizing: border-box` sur `.webtoon img`, plus la suppression du liseré que l'utilisateur ne veut pas.

Mesuré après correction (5/5) : `box-sizing` = `border-box`, 0 bordure, **écart entre hauteur rendue et hauteur calculée = 0** sur chaque image, **écart entre position rendue et offset calculé = 0**, et **aucun espace ni chevauchement aux jointures** entre pages.

L'écran ne peut pas confirmer visuellement : la capture CDP rend la bande noire dès qu'on descend dans le chapitre (au sommet elle fonctionne). C'est l'artefact déjà documenté, et il grandit avec la distance de défilement. Les mesures ci-dessus valent la preuve ; un coup d'œil de l'utilisateur confirmera.

### 2026-10-04 — Navigation dans la barre du lecteur + mode plein écran

**Demande de l'utilisateur** : « en haut, à côté de Signet, dans le même header ». La navigation webtoon occupait une **ligne séparée sous la barre**. Elle est désormais dans `.reader-bar`, juste après le bouton Signet, sous forme de deux flèches `←` `→` discretes. La ligne dédiée disparaît, ce qui rend aussi de la hauteur à l'image. Mesuré : Signet en x = 728, navigation en x = 802, plein écran en x = 900, tous dans la barre de 62 px ; la flèche `→` avance la bande (page 3, `scrollTop` 6 808) ; plus aucune navigation en bas.

**Plein écran.** Le plein écran porte sur l'élément `.reader`, pas sur `document` : la fenêtre de l'application reste visible si l'utilisateur en sort, et la navigation de Tatami ne disparaît pas définitivement. `:fullscreen` est une pseudo-classe CSS, donc l'état est lu **depuis le DOM** au lieu d'être deviné — c'est la seule source fiable, `Échap` et le bouton système pouvant sortir du plein écran sans passer par notre handler. Deux règles CSS étaient nécessaires : sans elles, `.reader` gardait `height: calc(100vh - s5*2)` et `margin: -s5`, ce qui laissait une bande vide en bas et un débordement latéral. Mesuré : hauteur 900 px pour une fenêtre de 900 px, bouton à `aria-pressed="true"`.

**Un vrai défaut trouvé par le walk : `Échap` fermait le lecteur ET le plein écran d'un coup.** Le handler faisait `closeReader()` inconditionnellement. En plein écran, `Échap` doit d'abord sortir du plein écran, sinon l'utilisateur perd sa page sans l'avoir voulu. Corrigé : si `document.fullscreenElement` est présent, on sort du plein écran et on s'arrête là. Mesuré : après `Échap`, plus de plein écran, lecteur **toujours ouvert**, bouton resynchronisé à `aria-pressed="false"`.

**Zone d'ombre de la tache 1.9 — tranchee.** Les captures de la bande webtoon revenaient noires depuis la tâche 1.9 ; tout indiquait un artefact (géométrie exacte, images décodées, pointage correct, fond rouge non peint, même jetons corrects en mode pagé). **Confirmé par l'œil** : la capture en plein écran montre l'image pleine largeur, correctement peinte. C'était bien la capture CDP au-dessus d'un conteneur de 686 000 px. Aucune correction de code n'était nécessaire — l'écart était dans la mesure, pas dans l'application. Leçon : devant une contradiction entre mesure et doute, chercher un troisième témoin (ici la capture plein écran, qui change le mode de rendu) avant de casser du code.

Portes : `pnpm typecheck` / `lint` / `build` = 0. Walk 11/11.

### 2026-10-04 — Demande utilisateur : bande webtoon pleine largeur, navigation en haut

**Pleine largeur.** Mesuré avant : 1 200 px de bande dans une fenêtre de 1 440, donc 120 px de marge de chaque côté — le lecteur était encadré. `.reader.pleine-largeur` retire le padding horizontal en mode webtoon ; les barres (titre, navigation) gardent leur marge, sinon leurs boutons colleraient aux bords. Après : bande à **1 420 px** (les 20 px restants sont la gouttière de la barre de défilement, `scrollbar-gutter: stable`) et image à 1 420 px.

**Navigation en haut.** En mode webtoon, la navigation était rendue **après** la bande : revenir en haut de l'écran à chaque changement de page revenait à parcourir 650 000 px sur un chapitre de 303 pages. Elle est désormais rendue juste sous la barre de titre. Un `order: -1` a été retiré après coup : `.reader-bar` est le premier enfant de `.reader` (`order: 0`), donc un `order: -1` aurait placé la navigation **au-dessus** d'elle. L'ordre du DOM suffit.

Mesuré : navigation à `top = 62 px`, sous la barre de titre (`top = 0`), les deux boutons présents, aucune navigation restante en bas, le bouton « suivante » avance bien la bande (page 3, `scrollTop` 6 808), et le **mode pagé garde sa navigation en bas** (non régressé). 9/9.

**Zone d'ombre non tranchée, à l'œil humain.** Les captures de la bande webtoon reviennent **noires**, alors que tout le reste est correct : géométrie vérifiée (boîtes 1 420 × 2 272 aux offsets 0 / 2 272 / 4 544), images **décodées** (`naturalWidth 800 × 1280`), `elementFromPoint` renvoie bien l'`IMG`, et **le fond rouge posé sur l'image ne peint pas non plus** — même en `position: static`, donc pas un problème de positionnement. Aucun `content-visibility` résiduel dans le CSS servi. Les mêmes jetons locaux s'affichent correctement en mode pagé. Tout penche vers un artefact de capture CDP au-dessus d'un conteneur de 686 000 px, mais **ce n'est pas prouvé** : il faut l'œil de l'utilisateur.

### 2026-10-04 — Tâches 1.9, 2.3 (suite), et les trois défauts que j'avais laissés

**1.9 — bande webtoon virtualisée.** Rust lit désormais la **taille réelle** de chaque page dans l'en-tête (`image::dimensions` : JPEG par parcours des marqueurs SOF, PNG via IHDR, WebP VP8/VP8L/VP8X), en ne lisant que **512 octets** par entrée — lire les images entières aurait relu ~90 Mo pour rien. L'interface en déduit la hauteur affichée, la position de chaque page, et ne monte qu'une **fenêtre glissante**. `flex: none` sur `.webtoon-inner` était indispensable : sans lui, le `height: 657 485px` posé en style inline était réduit par `flex-shrink` à 540 px, la barre de défilement mentait et la position sautait.

Preuves sur Killer Peter (303 pages dans un seul `.cbz`) : bande **657 485 px exacte**, **3 nœuds à l'ouverture et 6 en défilement au lieu de 303**, chaque cible atteinte au pixel près (50 000 → page 23, 200 000 → 92, 450 000 → 207, 650 000 → 299), position monotone, aucun trou blanc, compteur de page suit. Mode pagé non régressé (647 px dans un cadre de 691). 7/7.

Limite assumée : un chapitre lu **en ligne** n'a pas de dimensions (le moteur ne les renvoie pas encore), donc la virtualisation ne s'y applique pas et l'interface rend toutes les pages. Aucune estimation n'est faite dans ce cas — measured : une estimation fait perdre la position de lecture.

**2.3 — sélecteur de chapitre** dans la vue Téléchargements. Le bouton « Lire » ouvrait toujours le premier chapitre, ce qui est inutilisable sur une série de 137. Mesuré : Petite Rebelle affiche « Choisir un chapitre (37) », le dialogue liste 37 chapitres et tient dans l'écran (liste bornée à 52vh).

**Défaut d'interface corrigé** : le panneau de vérification n'existait que dans la vue Téléchargements, alors qu'un téléchargement peut être lancé depuis n'importe quelle vue — c'est exactement le « rien ne se passe »  experienced par l'utilisateur. Il est désormais rendu après la barre de navigation dans les **six** points d'entrée de vue.

**0.18 — nom de fichier lisible.** Un chapitre sans titre produisait `22d20970-38d1-48aa-bbd3-9ca9afbbb3dc.cbz` : valide, mais inutilisable dans un lecteur externe. Le nom retient maintenant le **numéro** de chapitre trouvé dans l'URL (`Chapitre 12.cbz`), et un UUID est **détecté explicitement** (`segments_uuid`, 8-4-4-4-12) avant tout repli. Ce n'était pas suffisant de filtrer les groupes de chiffres : l'UUID en contient, et le premier essai produisait `Chapitre 48.cbz` — échec révélé par le test.

Portes : `pnpm typecheck` / `lint` / `build` = 0 · `cargo test` **43 passés** · clippy 0 · fmt 0 · `gradle test` 9/9.

### 2026-10-04 — Tâche 2.3 : lecture hors ligne (le trou le plus visible de la v1)

L'application écrivait des `.cbz` qu'elle ne savait pas **lire**. Le lecteur passait toujours par le moteur, donc le seul moyen de lire un téléchargement était d'ouvrir un autre logiciel — exactement ce que l'utilisateur a fait.

**Rust** : `zipread.rs` (le pendant de `cbz.rs`), sans dépendance — `Cargo.toml` n'a aucun crate ZIP et en ajouter un est une décision d'architecture (AGENTS.md §9). Nos fichiers sont en `STORED` : lire ce qu'on écrit ne demande que le répertoire central. Une entrée `DEFLATE` (beaucoup de `.cbz` tiers) rend une erreur explicite au lieu de renvoyer des octets faux. **CRC32 vérifié à la lecture** : un `.cbz` corrompu est refusé, pas affiché. `PageRef` devient un enum `Moteur` / `Local`, donc **le lecteur n'a pas eu à changer** — il manipule toujours des jetons. Deux commandes : `local_chapter_pages` (recalcule le chemin avec la **même** fonction que `download_chapter`, pour que les deux règles ne puissent pas diverger) et `local_file_pages` (chemin fourni par l'interface, avec **confinement vérifié** sous le dossier de téléchargement, sinon l'interface pourrait faire lire n'importe quel fichier — AGENTS.md §7). Tri des pages par **numéro** et non alphabétiquement : `page_10` après `page_9`, sinon l'ordre s'inverse dès la dixième page, invisible sur une série de 9.

**Défaut structurel trouvé en route, préexistant.** Chaque vue s'affiche par un `return` conditionnel (`if (tab === 'downloads') { … return … }`), donc le composant retournait **avant** d'atteindre `if (reader)`. Le lecteur était structurellement inatteignable depuis la vue Téléchargements : le clic « Lire » ne faisait strictement rien — aucun message, aucune erreur console. Même famille que la leçon « une fonction qui sort en silence est un piège ».

**Preuves — 8/8 sur l'app réelle** : 14 images décodées, `src = http://tatami.localhost/page/l3713806…` (le préfixe `l` identifie un jeton local, donc aucun moteur impliqué) · mention « Hors ligne » · **moteur tué** (`taskkill /F /IM java.exe`) puis navigation : les images restent décodées. C'est cette dernière mesure qui distingue une vraie lecture locale d'un simple cache. Portes : `pnpm typecheck` / `lint` / `build` = 0 · `cargo test` **37 passés** · clippy 0 · fmt 0 · `gradle test` 9/9.

**Reste pour 2.3** : un sélecteur de chapitre dans la vue Téléchargements (le bouton « Lire » ouvre le premier chapitre téléchargé).

### 2026-10-04 — Tâche 1.9 (diagnostic) : le mode webtoon fonctionne mais n'est pas virtualisé

**Plainte de l'utilisateur** : « la page de lecture n'est pas vraiment longue ». Mesuré sur le plus long chapitre réel du disque — **Killer Peter, 303 pages dans UN seul `.cbz`**.

**Mode Webtoon, état réel** : bande de **657 788 px** de haut, images empilées à `width: 100 %` / `height: auto`, conteneur borné et défilant, `defile: true`. `scrollTop` = 48 000 avant **et** après un défilement de 12 × 4 000 px → **la position ne saute pas**. Coût : 174 ms pour ce défilement. **Le mode webtoon est donc correct et la bande est bien longue** : le reproche ne porte pas sur la longueur.

**Le vrai défaut** : `noeudsDom: 303` et `imagesChargees: 303` — le lecteur rend **toutes** les pages en DOM et le navigateur **charge les 303 images** à l'ouverture. Un chapitre de 303 pages coûte ainsi 211 requêtes `tatami://` dès le premier affichage. Ça passe sur cette machine, pas sur un portable modeste.

**Une tentative retirée, et pourquoi.** J'ai d'abord posé `content-visibility: auto` + `contain-intrinsic-size: auto 1200px` sur `.webtoon img`. Mesure : la bande annonçait **2 479 px** au lieu de 657 788, les 303 images se chargeaient **quand même**, et surtout `scrollTop` **reculait de 3 964 à 1 788** pendant le défilement — la position de lecture était perdue. Cause : la hauteur de réserve est une estimation, donc la barre de défilement est fausse, `loading="lazy"` croit que tout est proche, et la position est recalculée quand la vraie hauteur arrive. **CSS retiré**, retour mesuré au comportement correct et stable. Une virtualisation qui perd la position de lecture est pire que pas de virtualisation.

**La bonne voie, donc** : que Rust **renvoie les dimensions réelles** de chaque page (en-tête JPEG/PNG/WebP, déjà savoir le lire — `image::detect` le fait), pour que l'interface sache la hauteur exacte de chaque page. La hauteur de la bande devient juste dès le premier rendu, `loading="lazy"` fonctionne, et une **fenêtre glissante** peut alors ne rendre qu'une dizaine de nœuds au lieu de 303. C'est la suite de 1.9.

### 2026-10-04 — Tâche 0.20 : les images lazy-loadées sont illisibles par les parsers

**L'utilisateur a testé la tâche 0.19 et le résultat a été « rien ne se passe ».** Deux causes, mesurées séparément.

**1. Le cookie n'a jamais été transmis — défaut d'interface, mien.** Le panneau de vérification n'existait que dans la vue Téléchargements. L'utilisateur était dans l'onglet **Catalogues** : relevé sur l'app vivante, `panneauxH2: []` et `boutonsVisibles: []`. Il n'y avait rien à cliquer. Le panneau doit être global, pas lié à une vue.

**2. Mais surtout : le cookie n'aurait rien donné.** La source ne demande aucune vérification d'âge. Mesuré en récupérant la page du chapitre **sans aucun cookie** :
```
HTTP 200 | 162 740 caractères | 54 balises <img>
dont 45 balises WP-manga = exactement les 45 pages annoncées par le parser
< img id="image-0" data-src=" https://…/page_0001.webp"
      class="wp-manga-chapter-img img-responsive lazyload-ordered effect-fade" >
```
Les URL sont **dans `data-src`**, `src` est absent : le thème sert ses images en lazy-load. Et ces URL servent de **vraies WebP sans cookie** (HTTP 200, `image/webp`, 535 200 octets). Le HTML porte les URL ; c'est la lecture qui est en retard. Deux pièges mesurés : l'espace après `data-src="` (45/45) et le nom d'attribut `data-src`, pas `src`.

**Correctif : intercepteur HTTP du moteur**, `LazyLoadInterceptor`, branché dans `EngineContext`. Il fait ce qu'un navigateur fait : il place l'URL réelle dans `src` avant que le parser ne lise la page. Il ne touche que les `<img>` dont `src` est absent, vide ou placeholder ; une source qui fonctionne garde donc exactement son comportement. Il s'appuie sur `peekBody`, donc une page sans image lazy est renvoyée **octet pour octet**, charset d'origine compris. Attributs couverts : `data-src`, `data-lazy-src`, `data-original`, `data-original-src`, `data-echo`, `data-url`, `data-full-src`, `data-image`, plus `data-srcset`/`srcset` (plus grande variante).

Le correctif est **générique etfondu dans le moteur**, pas dans un parser : corriger le parser supposerait de forker `kotatsu-parsers-redo` (interdit par AGENTS.md §13), alors qu'adapter le contenu HTTP reçu est exactement le rôle du moteur. Plusieurs thèmes WordPress de lecture (Madara, MangaThemesia…) sont concernés.

Garde-fou ajouté dans `fetchImage` : une URL d'image vide lève désormais `ParamException` au lieu de partir sur l'origine du site. C'est ce silence qui avait produit les 37 `.cbz` pleins de HTML ; et le cache était en prime indexé sur cette chaîne vide, donc **les 45 pages partageaient la même clé**.

**Le moteur n'avait aucun test** : `build/test-results` était vide, donc la porte `gradle test` passait sans rien vérifier. Ajout de `testImplementation(kotlin("test"))` (test seul, sansJUnit) et de **9 tests** sur du HTML réel extrait de la page du 2026-10-04, dont les 45 balises. Un des tests a d'abord échoué **à tort** : il cherchait la sous-chaîne `src=" https://`, que `data-src=" https://` contient aussi — l'assertion visait l'attribut d'origine, intact.

**Preuves — 9/9 tests moteur, puis walk sur l'app réelle 9/10 (le 10ᵉ était une assertion fausse, voir plus bas).**
- Les 45 URL ne sont plus vides : `page_0001.webp` → `page_0045.webp`.
- Téléchargement **réussi en 12 750 ms** : `HENTAIORIGINES\I Love You Very Much\Chapitre 1.cbz`, 9 330 396 octets.
- Relecture par un lecteur ZIP **strict** : intégrité OK, **45 entrées, toutes WEBP**, **45 tailles distinctes**, 0 entrée vide, médiane 234 594 octets, premier octet `52494646…57454250` = `RIFF`/`WEBP`.
- **Non-régression** : MANGADEX télécharge toujours.
- Aucune fenêtre de vérification ne s'ouvre plus (1 seule fenêtre réelle, comptée via CDP).

**Ce qui reste imparfait, mesuré :** `032.webp` fait 310 octets. Décodé, c'est un WebP valide de **413×42 px** — un placeholder servi par le site lui-même. 44 pages réelles sur 45. Résiduel côté site, pas côté Tatami.

**Une assertion fausse à signaler :** j'ai voulu prouver « plus aucune fenêtre de vérification », en appelant `cancel_challenge`. Or cette commande fait `if let Some(w) = … { w.close() }` puis `Ok(())` **inconditionnellement** : elle réussit donc toujours, même sans fenêtre. Le test passait sans prouver rien. Remplacé par un comptage des fenêtres réelles via CDP (`pages.length === 1`). Retenir : **une commande d'annulation qui réussit toujours ne peut pas servir de preuve d'état**.

### 2026-10-04 — Correctif : des `.cbz` illisibles (signalé par l'utilisateur)

- Symptôme : « j'ai téléchargé des webtoon, ouvert le `.cbz` avec un logiciel dédié, je ne voyais rien — tout noir ».
- Diagnostic (mesuré, pas supposé) : les 44 `.cbz` du dossier de téléchargement ouverts avec un lecteur **strict** (`zipfile` de Python). **37 sont corrompus**, tous de HENTAIORIGINES. Les 7 autres (MANGADEX, MANGASORIGINES) sont des ZIP valides, intégrité vérifiée, vraies images JPEG (`ffd8ffe0`). Dans les 37, **toutes** les entrées contiennent la même chose : `202020200a3c21444f4f435459` = `"    \n<!DOC TY` — du **HTML**. Contenu lu : la page du site, avec `ori-age-verif`, son **interstitiel de vérification d'âge**.
- Cause : trois défauts enchaînés.
  1. `fetchImage` renvoie `200 OK` avec le HTML de l'interstitiel : aucune erreur, donc rien ne signale le problème.
  2. `download.rs` écrivait les octets **sans jamais vérifier que c'est une image**.
  3. `cbz.rs` déduisait l'extension du `contentType` annoncé, et `extension()` retombait sur `jpg` pour tout ce qui n'était pas png/webp/gif : le HTML partait donc sous le nom `001.jpg`.
  Aggravant : la logique « ne jamais retélécharger ce qui est là » voyait ces fichiers comme téléchargés — **ces chapitres étaient irrécupérables** sans suppression.
- Correctif : nouveau module `image.rs` avec `detect()` (signature sur les octets : JPEG, PNG, WebP, GIF, BMP) et un message d'erreur parlant. Appliqué aux **deux** sites d'écriture : le `.cbz` et le cache du lecteur (`pagecache.rs`, qui lui aussi écrivait sans vérifier). L'extension écrite vient désormais des **octets**, jamais du `contentType`. `add_image` (basé sur le `contentType`) est **supprimé** : c'était précisément l'API fautive, la garder aurait laissé le pied de gun. Le doublon `guess_ct` de `pagecache.rs` a disparu au profit du détecteur partagé.
- Preuve : `cargo test` → **29 passés** (dont `une_page_web_nest_pas_une_image`, qui rejoue le cas réel : l'interstitiel mesuré) · `cargo clippy -D warnings` → 0 · `cargo fmt --check` → 0.
- **En attente de décision humaine** : les 37 fichiers corrompus sont toujours sur le disque. Ils sont reconnus comme téléchargés, donc il faut les supprimer pour pouvoir retélécharger — mais HENTAIORIGINES est **derrière une vérification d'âge**, donc le retéléchargement échouera tant que la vérification n'est pas faite (l'interface doit le dire au lieu d'écrire du noir).

### 2026-10-04 — Tâche 1.10 (partiel) : i18n français / anglais
- Plan : (1) construire la couche i18n avec une **garantie de compilation**, pas seulement un dictionnaire ; (2) convertir écran par écran ; (3) prouver sur l'app réelle que la bascule est immédiate, complète et persistante.
- Fait : `app/i18n.ts`, sans dépendance. Deux propriétés voulues :
  1. **Exhaustivité vérifiée par le compilateur** — `EN` est typé `Record<Cle, string>`, donc une clé française sans traduction fait échouer `pnpm typecheck`. Aucun trou silencieux possible, contrairement à un `JSON` que rien ne vérifie.
  2. **Conversion possible par écrans** — `t()` retombe sur le français, donc convertir l'interface morceau par morceau n'affiche jamais une clé brute.
  La langue est une préférence d'affichage : `localStorage` (`tatami.lang`), conformément à AGENTS.md §6. Sélecteur dans les Réglages, `document.documentElement.lang` suit.
- Converti : navigation, Réglages (contenu, catégories, langue, réseau), accueil (continuer, signets, favoris, filtres), catalogues, téléchargements, fiche, lecteur, et tous les messages d'erreur.
- **Une fuite que seule la capture a révélée** : le walk annonçait 15/15 alors que le titre de l'accueil et le compteur étaient restés en français — mes assertions regardaient les bandeaux et les boutons, pas le `h1`. Les deux ont été convertis **et** ajoutés au walk. Un test qui passe peut masquer ce qu'il ne regarde pas.
- Preuve :
  - `pnpm typecheck` / `pnpm lint` / `pnpm build` → 0 / 0 / 0.
  - Parcours sur l'app réelle via CDP (`walk-i18n.js`) : **17 mesures, 0 échec** — onglets FR puis EN, titre de vue, sections, `html lang`, les trois écrans, retour au français, survie au rechargement, et **aucune clé brute affichée** (détectée par balayage du DOM).
  - Captures : `audit/tatami4/23-anglais-reglages.png`, `audit/tatami4/24-anglais-accueil.png`.
- Points ouverts : les **titres de mangas restent dans leur langue d'origine** (volontaire : on ne traduit pas les œuvres). Restent à convertir les libellés menores : sélection de chapitres, fiche détaillée, invite de vérification Cloudflare.

### 2026-10-04 — Tâche 1.5 : catégories de favoris
- Plan : (1) demander à l'humain une catégorie ou plusieurs, plutôt que de deviner (deux applications de référence ne font pas la même chose) ; (2) table `categories` + `favorites.category_id` ; (3) le prouver sur l'app réelle, y compris la suppression d'une catégorie.
- Décision humaine : **une seule catégorie par favori**, comme dans Kotatsu-Redo, l'app dont on vise la parité.
- Fait : table `categories` (schéma **v6**) avec nom unique, index de couleur et position ; `favorites.category_id` en clé étrangère `ON DELETE SET NULL`. Commandes `list_categories`, `add_category`, `rename_category`, `delete_category`, `reorder_categories`, `set_favorite_category`. Interface : sélecteur sur la fiche, section de gestion dans les Réglages (renommage, 8 couleurs, monter/descendre, supprimer, ajout), pastille de couleur sur la carte, et filtre sur l'accueil (« Toutes », une pastille par catégorie avec son compte, « Sans catégorie »).
- Choix : la base ne stocke qu'un **index** de couleur (1..8), jamais une valeur CSS ; l'interface la traduit en variables `--cat-1`…`--cat-8`. Respecte AGENTS.md §9.2 (aucune valeur magique dans le code). L'ordre est réécrit en renvoyant **tous** les identifiants, pas un échange de deux lignes : l'ordre final est alors exactement celui demandé.
- **Un défaut trouvé en mesurant** : les Réglages n'affichaient aucune erreur. Un échec déclenché dans cet écran (catégorie en double) était totalement invisible — l'utilisateur cliquait et rien ne se passait. Les quatre autres vues affichaient déjà cette erreur ; les Réglages ont été alignés.
- Preuve :
  - `cargo test` → **26 passés, 0 échec** (dont `supprimer_une_categorie_ne_supprime_pas_ses_favoris`, `affecter_une_categorie_inexistante_echoue`, `les_categories_ordonnees_et_sans_doublon_de_nom`).
  - `cargo clippy -D warnings` → 0 · `cargo fmt --check` → 0 · `pnpm typecheck` / `pnpm lint` / `pnpm build` → 0.
  - Parcours sur l'app réelle via CDP (`walk-categories.js`) : **20 mesures, 0 échec** — création, doublon refusé **avec message**, réordonnancement, affectation depuis la fiche, filtre exact, pastille réellement peinte (`rgb(54, 106, 211)`), catégorie vide qui le dit, survie au rechargement, et suppression qui **conserve** le favori.
  - Captures : `audit/tatami4/20-reglages-categories.png`, `audit/tatami4/21-favori-filtre.png`.
- Note de méthode : une mesure échouait alors que l'application était correcte — mon assertion cherchait `existe deja` sans accent alors que le message dit « existe déjà ». Un walk qui coule sur une regex accentuée prouve un défaut de regex, pas un défaut de l'app.

### 2026-10-04 — Tâche 1.7 : signets
- Plan : (1) table `bookmarks` (schéma **v5**) avec la page dans la clé, pour pouvoir marquer plusieurs pages d'un même chapitre ; (2) bouton dans le lecteur + section sur l'accueil avec réouverture au bon endroit ; (3) prouver la persistance et l'exactitude de la page.
- Fait : `add_bookmark` / `remove_bookmark` / `list_bookmarks` (+ filtre par webtoon et limite). Bouton « Signet » dans la barre du lecteur (état actif + libellé « Signet ✓ », `aria-pressed`). Section « Vos signets » sur l'accueil : couverture, titre, source, chapitre, page, ancienneté, « Rouvrir » et « Retirer ». `ouvrirSignet` recharge la fiche, les chapitres, puis rouvre à la page exacte.
- Choix : pas de commande `has_bookmark`. Un seul appel `list_bookmarks` alimente l'état du bouton du lecteur et l'affichage ; une commande de plus aurait été du poids mort (`clippy` l'a d'ailleurs signalé comme morte). L'état du bouton du lecteur est donc dérivé d'une liste — si l'utilisateur dépasse 200 signets, les plus anciens ne sont plus en mémoire (limite documentée dans la commande).
- **Deux bugs réels trouvés en mesurant :**
  1. `ouvrirSignet` n'ouvrait **rien** : `openReaderAt(c, index, sourceName)` sort en silence quand aucune source n'est sélectionnée, ce qui est exactement le cas depuis l'accueil. Le nom de source doit être passé explicitement, comme le fait déjà `resumeReading`.
  2. Un walk précédent mesurait « ouvert à la page 1 » alors que l'app ouvrait bien la page 9 : la forme chaînée `locator(...).locator(...).click()` visait une ligne périmée. `.first()` + un affichage de la ligne réellement cliquée ont rendu la mesure fiable. **Encore un cas où le test, pas l'app, était faux.**
- Preuve :
  - `cargo test` → **23 passés, 0 échec** (dont `un_signet_memorise_la_page_exacte`, `les_signets_se_lisent_du_plus_recent_au_plus_ancien`, `un_signet_ne_touche_pas_l_historique` — marquer une page ne fabrique pas d'historique, retirer un signet n'efface rien).
  - `cargo clippy -D warnings` → 0 · `cargo fmt --check` → 0 · `pnpm typecheck` / `pnpm lint` / `pnpm build` → 0.
  - Parcours sur l'app réelle via CDP (`walk-signets.js`) : **22 mesures, 0 échec** — marquage, libellé, page exacte, pas de doublon en marquer/retirer/marquer, deux pages distinctes, section d'accueil, réouverture à la **page 9/9**, bouton déjà marqué, retrait, survie au rechargement, et table vide après nettoyage.
  - Captures : `audit/tatami4/18-signets-accueil.png`, `audit/tatami4/19-signet-ouvert.png`.
- Points ouverts : pas encore de pastille de signet sur les chapitres de la fiche (un signet n'est donc visible que depuis l'accueil et le lecteur) ; les signets ne sont pas encore exportables ni attachés à une catégorie (1.5).

### 2026-10-04 — Tâche 1.3 : tri des titres (alphabétique, choisi avec l'utilisateur)
- Plan : (1) demander à l'humain quel tri, plutôt que d'inventer (le moteur n'expose pas de tri « dernière mise à jour ») ; (2) implémenter alphabétique ; (3) prouver sur l'app réelle que l'ordre est bien appliqué et qu'il revient à l'ordre du site.
- Décision humaine : **alphabétique seul** (ordre du site / A → Z / Z → A). Écarté : « dernier chapitre en premier », trop approximatif sur les sources sans dates.
- Fait : barre de liste (nombre de webtoons à gauche, tris à droite), tri appliqué à **tout ce qui est chargé** et non à la seule page courante — sinon l'ordre changerait à chaque « Voir plus ». `localeCompare` sans locale explicite (celle du navigateur) range correctement les accents, `numeric: true` place « Ch. 2 » avant « Ch. 10 ». L'état du tri est global, il survit donc au changement de source.
- Preuve :
  - `pnpm typecheck` / `pnpm lint` / `pnpm build` → 0 / 0 / 0. Aucun changement Rust.
  - Parcours sur l'app réelle via CDP (`walk-tri.js`) : **11 mesures, 0 échec** — les 3 tris présents, « Ordre du site » par défaut, A→Z effectivement trié, Z→A = inverse **exact** d'A→Z, retour à l'ordre du moteur, et conservation après changement de source.
  - Capture : `audit/tatami4/17-tri-az.png`.
- Point de méthode : ma première version du walk lisait les titres avec un sélecteur faux et renvoyait des listes **vides** — trois mesures passaient alors à vide sans rien prouver. Un garde-fou « les titres sont bien lus dans le DOM (≥ 3) » a été ajouté avant de croire le résultat. Une mesure qui passe sur un tableau vide ne prouve rien.

### 2026-10-04 — Tâche 1.2 : activation/désactivation des sources
- Plan : (1) auditer 1.2 et 1.3 avant d'ajouter 1.7, dans l'ordre du plan ; (2) combler le seul manque de 1.2 (activation/désactivation) ; (3) le prouver sur l'app réelle, persistance comprise.
- Fait : table `sources_state` (schéma **v4**, sauvegarde avant migration vérifiée dans le journal : `tatami-v3-1791099395803.db`). Seules les sources **désactivées** sont écrites : l'absence de ligne signifie « active », donc pas de `DEFAULT` à maintenir. Commandes `list_disabled_sources`, `set_source_enabled`, `enable_all_sources`. Côté interface : interrupteur sur chaque tuile de source, compteur « n désactivée(s) », « Afficher les n sources désactivées », « Tout réactiver », et un **état vide** explicite quand la recherche ne donne rien (avant : une grille vide sous un bandeau vide, capturé à l'écran).
- Choix d'implémentation : la tuile est passée de `<button>` à `<div class="slot">` contenant le bouton d'ouverture et, en **frère**, l'interrupteur — un `<button>` dans un `<button>` est du HTML invalide (cf. AGENTS.md §14). Le CSS de `.card` est resté intact, la grille garde la même largeur (266 px mesurés avant/après).
- Preuve :
  - `cargo test` → **20 passés, 0 échec** (dont `les_sources_desactivees_sont_memorisees` et `desactiver_une_source_ne_touche_pas_les_favoris` — écarter une source ne perd ni l'historique ni les favoris).
  - `cargo clippy -D warnings` → 0 · `pnpm typecheck` / `pnpm lint` / `pnpm build` → 0.
  - Parcours sur l'app réelle via CDP (`walk-sources.js`) : **16 mesures, 0 échec** — désactivation, disparition de la liste, compteur, survie au rechargement complet, réapparition atténuée à la demande, « Tout réactiver », base vide à la fin.
  - Audit d'accessibilité : 48 interrupteurs sur 48 tuiles, tous `role="switch"` et nommés (« Désactiver la source MANGABUFF »).
  - Captures : `audit/tatami4/14-source-desactivee.png`, `15-sources-actives.png`, `16-grille-sources.png`.
- Points ouverts : **le tri de 1.3 reste à faire** — quel tri ? (ordre du site / alphabétique / dernière mise à jour) : choix à confirmer avant de coder, le moteur n'exposant pas de tri « dernière mise à jour » sans requête supplémentaire par source.

### 2026-10-04 — Tâches 1.1, 1.4, 1.6 : favoris, accueil, incognito, reprise
- Plan : (1) prouver que favori + historique survivent à un vrai redémarrage de l'application ; (2) corriger ce qui casse l'incognito ; (3) ne déclarer 1.1/1.4/1.6 fait qu'avec une preuve exécutée.
- Fait : base SQLite (schéma v3) avec `favorites` et `history` ; accueil par défaut avec « Continuer la lecture », « Vos favoris », interrupteur Incognito et « Vider l'historique » ; `resumeReading` rouvre le dernier chapitre à la dernière page ; `note_content_rating` apprend le classement d'un titre déjà connu (historique **et** favori) sans jamais créer de ligne.
- **5 défauts trouvés en mesurant, tous corrigés :**
  1. l'accueil ne chargeait rien au démarrage (`tab` vaut « home » sans passer par `goTab`) ;
  2. la reprise écrasait le classement connu par `NULL` → `COALESCE(NULLIF(?5,'UNKNOWN'), …)` ;
  3. les deux sources de classification se contredisent (WAMANGA : `ADULT` dans la liste, `UNKNOWN` sur la fiche ; MANGABUFF l'inverse) → on prend la première valeur connue, quel que soit l'endroit ;
  4. le réglage « Toutes » envoyait `SAFE` (chaîne vide interprétée comme SAFE côté Rust) : WAMANGA renvoyait 0 manga. Corrigé par `safeOnly: false` + `requested_rating` ;
  5. `user_version` estampillée sans la colonne : un binaire intermédiaire (version poussée dans une édition, migration dans l'édition suivante) a laissé la base réelle cassée définitivement. Les colonnes sont désormais ajoutées par **présence** (`PRAGMA table_info`), pas seulement par version.
- Preuve :
  - `cargo test --manifest-path apps\desktop\src-tauri\Cargo.toml` → **18 passés, 0 échec** (dont `une_version_stampillee_sans_la_colonne_est_reparee`, `la_migration_idempotente_ne_casse_rien`, `une_chaine_vide_ne_signifie_pas_safe`), stable sur 2 exécutions.
  - `cargo clippy --all-targets -- -D warnings` → 0 · `cargo fmt --check` → 0 · `pnpm typecheck` / `pnpm lint` / `pnpm build` → 0.
  - Parcours complet sur l'app réelle via CDP (`walk-final.js`) : catalogue → fiche → lecture → favori → retour accueil → **19 mesures, 0 échec**. Dont `l'incognito retire exactement les titres adultes = 1`.
  - **Redémarrage réel** (`redemarrage.js`, processus tué puis relancé) : **9 mesures, 0 échec** — le favori revient avec son classement `ADULT`, l'historique revient avec `3/3` et `1/9`, l'incognito reste actif, le titre adulte reste masqué de « Continuer » et le favori reste visible.
  - Captures : `audit/tatami4/11-accueil-incognito.png`, `audit/tatami4/13-redemarrage.png`.
- Points ouverts : 1.5 reste à faire pour les **catégories** (le favori simple est en place) ; le classement d'un titre jamais ouvert et jamais mis en favori reste `UNKNOWN` et donc visible sous incognito — choix prudent assumé, documenté dans l'interface.

### 2026-10-03 — Analyse de sushiscan.net (site de référence, aucune source ajoutée)
- Plan : (1) ne PAS toucher aux sources malgré la présence de `SushiScan` dans le JAR ; (2) analyser l'architecture, les écrans et le style pour adapter l'interface de Tatami.
- Fait : site mesuré au navigateur (4 captures). **Architecture** : WordPress + thème custom `wp-content/themes/sushiscan` (dérivé Madara) + LiteSpeed Cache + Cloudflare devant (`cf-cache-status: HIT`, `cdn-cgi/challenge-platform` = détection active). Pas d'API REST de contenu : la 1re page du chapitre est dans le HTML (`img.ts-main-image`), les suivantes par `POST /wp-admin/admin-ajax.php`. **Images** : CDN séparé `c.sushiscan.net`, WebP, nommage `OPChap1180-01.webp`. **Routes** : `/catalogue/<slug>/`, `/<slug>-chapitre-<N>/`, `/catalogue/?order=update`, `/random`, plus Liste de Lecture et Historique.
- **Lecteur mesuré** : 4 contrôles (`chapter`, `readingmode` = *Lecture Verticale* | *Lecture par Page*, `customreader` = *Par défaut* | *Ajusté à la Hauteur*, `select-paged` = `1/15`). En mode page 1 seule image dans le DOM ; en mode vertical 11 images sur 15, page de 17 885 px de haut.
- **Tokens mesurés** : fond accueil `#000000`, fond lecteur `#16151D`, texte `#B8B8B8`, Roboto 14px, contrôles `bg #111` + `border 2px #444` + `radius 20px` + `13px/600`, couverture `radius 6px` en `loading=lazy` affichée 167×228, bandeau de section `bg #111` + `radius 5px 5px 0 0` + `padding 5px 20px` + `14px/600`.
- Preuve : captures `audit/sushi/{1-accueil,2b-detail-complet,3-lecteur,4-catalogue}.png` ; mesures dans `audit/sushi/mesures.txt` et `livraison.txt` (scripts `analyse-sushi*.js`).
- **À retenir** : le parser `mangareader/fr/SushiScan` du JAR cible la famille MangaReader.to, **pas** ce site (schéma d'URL différent). Le DOM est bien dérivé Madara, donc un parser Madara sur mesure fonctionnerait. Non fait, hors demande.
- Points ouverts : les pubs (3 emplacements dans le flux) et le logo servi en 3965×660 pour 180×30 sont des défauts, pas des références à copier.

### 2026-10-04 — Résolution des deux points ouverts (lint, couvertures absentes)
- Plan : (1) remettre `pnpm lint` en service selon la doc Next 16 ; (2) supprimer les 2 erreurs et les avertissements réels ; (3) remplacer les rectangles de couverture vides par une pastille d'initiale ; (4) repasser la passe visuelle.
- **Lint** : `next lint` a été retiré de Next 16 et `next build` ne lance plus le linter. Installation de `eslint` + `eslint-config-next@16.3.8`, création de `apps/desktop/eslint.config.mjs` (`defineConfig` + `globalIgnores` + `eslint-config-next/core-web-vitals`), script passé à `eslint`. **Piège mesuré** : `eslint-config-next@16.3.8` déclare le peer `eslint >=9.0.0` mais son `eslint-plugin-react@7.37.5` échoue sur ESLint 10 (`contextOrFilename.getFilename is not a function`) → **ESLint 9.39.5 épinglé**.
- **2 erreurs corrigées** : `react-hooks/set-state-in-effect` sur l'effet de montage (bloqué par un `eslint-disable` borné à cet effet, avec le motif : en export statique le HTML est produit au build, les préférences ne peuvent pas venir d'un initialiseur paresseux sans créer un écart serveur/client) et `react/no-unescaped-entities` (apostrophe échappée en `&apos;`).
- **Avertissements traités** : `role="tab"` portait `aria-pressed`, interdit par `jsx-a11y/role-supports-aria-props` → passé à `aria-selected`, sélecteur CSS aligné (mesuré : 1 seul onglet actif, fond `rgb(51,51,51)`). 6 directives `eslint-disable` inutiles supprimées — dont 2 qui masquaient de vrais avertissements `exhaustive-deps`, remis avec justification. `@next/next/no-img-element` désactivé **avec motif écrit** : en webview Tauri les images viennent du protocole `tatami://` ou de CDN, `images.unoptimized` est déjà actif, `next/image` n'apporterait rien et casserait le protocole.
- **Couvertures absentes** : les téléchargements antérieurs à la colonne `coverUrl` de `meta.json` affichaient un rectangle vide qui se lisait comme une image cassée. Remplacé par une pastille portant l'initiale du titre, à la taille exacte de la couverture (84 px).
- Preuve : `pnpm lint` **exit 0, 0 erreur 0 avertissement** · `pnpm typecheck` exit 0 · `pnpm build` `✓ Compiled successfully` · passe visuelle **6 mesures, 0 échec** (8 cartes, 3 couvertures réelles, 5 pastilles, 0 image cassée, pastille de 84 px, 1 onglet actif).
- Points ouverts : `pnpm add` renvoie le code 1 à cause de `ERR_PNPM_IGNORED_BUILDS` sur `unrs-resolver@1.12.2` (script natif non approuvé) — le linter fonctionne malgré tout ; je n'approuve pas un script de build natif sans décision humaine. Les couvertures réellement absentes ne sont pas récupérées : une commande de réparation qui reconsulte la source reste possible, non faite.

### 2026-10-04 — Retours humains sur la vague 0.13–0.17 (style + 5 défauts)
- Plan : (1) mesurer les VRAIS tokens du site dans sa feuille de style ; (2) réécrire le thème sur ces tokens ; (3) derniers chapitres et non premiers ; (4) page Téléchargements aérée ; (5) sélection en masse de chapitres ; (6) page des sources refaite ; (7) repasser la passe visuelle.
- **Retour 1 — le style n'était pas appliqué.** Je n'avais repris que la *structure* du site (grille, table, bandeau), pas son *look*. Tokens lus dans la feuille `litespeed/css/2d73d5a6….css` du site : fonds `#000` / `#16151d` / `#1a1a1a` / `#222` / `#333`, textes `#fff` / `#ddd` / `#ccc` / `#999`, bordures `#333` / `#323232` / `#383838`, **accent `#366ad3`** (46 fonds, 22 textes, 5 bordures), danger `#f13e3e`, Roboto 14 px, titre **32 px/600**, contrôles `bg #111` + `border 2px #444` + `radius 20 px` + `13 px/600`, couverture `radius 6 px`. `globals.css` réécrit sur cette base : noir, Roboto en tête (Segoe UI en repli, aucune police téléchargée), titres 32 px, pilules sur les contrôles, rayons 6/8/12, espacement grossi d'un cran. L'accent bleu reste celui de Tatami par identité.
- **Retour 2 — les cartes affichaient les PREMIERS chapitres.** `slice(0, 3)` corrigé en tri explicite décroissant (date, puis numéro) puis `slice(0, 3)`. Mesuré : Ch. 255/254/253, Ch. 7/6/5, Ch. 45/44/43…
- **Retour 3 — page Téléchargements vide et serrée.** Bandeau de chiffres (webtoons / chapitres / poids / à compléter), cartes à 3 colonnes de 340 px, écart 22 px, couverture passée de 56 à 84 px, bandeau « Vos webtoons ».
- **Retour 4 — cocher 286 chapitres un par un.** Barre de sélection rapide : champ N + `N derniers` / `N premiers` / `Tout` / `Aucun`. `Tout` et `Aucun` portent sur les chapitres **filtrés**, pas sur toute la série.
- **Retour 5 — page des sources fade.** Le mur de 1 370 entrées est remplacé par un écran de **19 tuiles de langue** (code, nom, volume) triées par volume ; la liste n'apparaît qu'une fois une langue choisie ou une recherche lancée.
- **Preuve** : `pnpm typecheck` exit 0 · `pnpm build` `✓ Compiled successfully` · `cargo clippy -D warnings` exit 0 · `cargo test` `7 passed` · **21 mesures visuelles, 0 échec**. Thème mesuré dans le navigateur : `rgb(0,0,0)`, `rgb(255,255,255)`, `Roboto`, `h1 = 32px/600/rgb(255,255,255)`, 19 tuiles, 0 carte de source sur l'accueil, 48 dans une langue, 21 lignes de chapitres récents toutes décroissantes, 5 cases cochées par « N derniers », bandeau de 4 chiffres, écart 22 px, couverture 84 px.
- **Bug trouvé par cette passe** : « N derniers » cochait bien les 5 derniers chapitres mais **hors de l'écran** (la grille affichait 100 cellules sur 286) et le défilement restait en tête de série. Corrigé : la grille se déplie jusqu'à la fin *et* un `scrollIntoView` sur la première case cochée — mesuré à `top = 703` dans une fenêtre de 900 px.
- **Deux erreurs de harnais, pas de code** : mes prédicats de test étaient passés en arguments inversés, ce qui affichait 8 « échecs » sur des mesures justes. Corrigé. Leçon : un rapport de vérification qui affiche des faux échecs est pire qu'aucun rapport.
- Points ouverts : les couvertures absentes des téléchargements anciens laissent un bloc vide (donnée réellement manquante) ; `pnpm lint` toujours cassé.

### 2026-10-03 — Tâches 0.13 à 0.17 (adaptation de l'interface)
- Plan : (1) 0.13+0.14 lecteur (TS/CSS seuls) ; (2) 0.15 grille de chapitres + date ; (3) 0.16 chapitres récents sur les cartes (TS, concurrence bornée + cache) ; (4) 0.17 bandeaux + table d'informations (CSS + commande Rust `details` + `mangaToJson` étendu, donc rebuild moteur).
- **Fait 0.13** : types `ReaderFit` (width/height) et `ReaderDir` (rtl/ltr), état `fit`/`spread`/`readerDir` persistés (`tatami.readerFit`, `tatami.readerSpread`, `tatami.readerDir`). Un cran de navigation = 2 pages en double page (`step()`), utilisé par le clavier comme par les boutons. Le cadre porte `fit-width` (page pleine largeur, défilement vertical) ou `fit-height` (page entière visible), et `spread` (grille 2 colonnes, ordre DOM inversé en RTL — la page courante est à droite, comme un manga).
- **Fait 0.14** : sélecteur de chapitre (`aria-label="Changer de chapitre"`) et champ de saut de page (`#reader-page`, min 1 max N, validation à la soumission) dans la barre du lecteur. La barre passe en `flex-wrap` pour tenir sur une fenêtre étroite.
- **Fait 0.15** : `.chapter-list` (liste verticale) remplacée par `.chapter-grid` (`auto-fill minmax(170px, 1fr)`). Chaque cellule = case à cocher + numéro + date + bouton de téléchargement révélé au survol **ou au focus** (donc atteignable au clavier). Champ de filtre : un terme numérique compare aussi les numéros, « 1180 » trouve le chapitre 1180 même si son titre n'a que des lettres.
- **Fait 0.16** : les 3 derniers chapitres + leur ancienneté (« Ch. 1 · 2 ans ») dans la carte du catalogue, et le nombre total de chapitres en pastille sur la couverture. Coût assumé : 1 requête `chapters` par carte, concurrence bornée à 4, cache `localStorage` 30 min, interrupteur activable, compteur `x/y` affiché.
- **Fait 0.17** : bandeau de section sur tous les `.page-head h1` ; fiche série avec couverture + résumé + table d'informations 2 colonnes (`.info-table`) alimentée par une nouvelle commande Rust `details`. `mangaToJson` étendu côté Kotlin (`state`, `rating`, `description`, `tags`, `altTitle` via `altTitles.firstOrNull()` — `altTitle` est déprécié côté parsers).
- **Preuve** : `pnpm typecheck` → exit 0. `pnpm build` → `✓ Compiled successfully in 10.4s`. `cargo clippy --all-targets -- -D warnings` → exit 0, aucune alerte. `cargo test` → `7 passed; 0 failed`. `gradle installDist` → `BUILD SUCCESSFUL in 14s` (1er essai en échec : `MangaState?` nullable sur `.state.name`, corrigé en `m.state?.name ?: "UNKNOWN"`), 18 JAR copiés dans `engine/dist/tatami-engine/lib`.
- **Preuve moteur** : `details` sur MANGABUFF → `state="ONGOING"`, `altTitle="Tears on withered flowers"`, `rating=0.89`, `tags` = 9 genres, `description` = 1470 caractères. `chapters` → 121 chapitres, `uploadDate` non nul sur **121/121**, lu et affiché « 22 juin 2024 ». Le champ était donc déjà transmis (Rust fait du pass-through), seul le type TS l'ignorait.
- **Passe visuelle (app réelle via CDP, WebView2 port 9223, source WAMANGA)** : **20 mesures, 0 échec**. Bandeau `rgb(19,22,30)` + rayon `7px 7px 0 0`. Cartes : 60 lignes de chapitres récents, 20 pastilles de compte (`286 ch.`, `8 ch.`…), compteur `20/20`. Fiche : 5 lignes d'infos, grille à **7 colonnes** à 1440 px, 100 cellules dont **100 avec date**, filtre « 145 » → 1 cellule. Lecteur : 1 image en simple, **2** en double page (`676px 676px`), ajusté hauteur 70×653 dans un cadre de 685 contre 1344×12525 en largeur, saut de page et bascule RTL/LTR fonctionnels.
- **Trois bugs trouvés par ces mesures, corrigés** : (1) double page + ajusté à la hauteur débordait (6 300 px dans un cadre de 685) — `grid-template-rows` valait `auto`, donc `max-height: 100%` se résolvait en `auto`, ajouté `grid-template-rows: minmax(0, 1fr)` ; (2) une carte servie par le cache restait vide (aucune requête ne restait à faire pour la remplir) — le cache est maintenant reporté dans l'état ; (3) les numéros de chapitre flottants fuitaient (« Ch. 38.09999847… », division flottante du site) — arrondis à 2 décimales, et `Math.floor` → `Math.round` dans le filtre.
- Deux autres faits mesurés : les résumés arrivent **en HTML** (REMANGA : `<p>…</p>`) → `plainText()` retire balises et entités ; les préférences du lecteur sont relues **seulement au montage** de l'app, un walk qui veut des valeurs connues doit cliquer les segments, pas écrire `localStorage`.
- Limite mesurée sur les sources : `pages` répond sur REMANGA/SEIMANGA mais `fetchImage` renvoie **HTTP 403** (CDN `img.reimg2.org` qui refuse l'user-agent du moteur) → le lecteur reste vide sur ces deux-là. Images réellement servies : WAMANGA, SEIMANGA, ANZMANGASHD. MANGABUFF demande une authentification dès `pages`.
- Preuve gates après correctifs : `pnpm typecheck` exit 0 · `pnpm build` `✓ Compiled successfully` · `cargo clippy --all-targets -- -D warnings` exit 0 · `cargo test` `7 passed; 0 failed`.
- Commit `7efdc1f` poussé sur `origin/master`. Points ouverts : `pnpm lint` toujours cassé (`next lint` n'existe plus en Next 16) ; 0.16 ajoute 1 requête `chapters` par carte affichée, mesuré à 20/20 en 14 s sur WAMANGA puis servi par le cache 30 min — à surveiller sur une source lente ; le lecteur reste non vérifié sur REMANGA/SEIMANGA à cause du 403 d'images.

### 2026-10-02 — Création
- Plan et directives créés. Nom provisoire "Tatami". Aucune tâche commencée.

### 2026-10-02 — Tâche 0.1
- Plan : (1) git init + identité locale → vérif rev-parse ; (2) LICENSE GPLv3 depuis gnu.org + README crédits/avertissement + workspace pnpm + .gitignore → vérif pnpm install ; (3) commit + cocher 0.1.
- Fait : dépôt init (root-commit `4c19a26`), 8 fichiers (`LICENSE`, `README.md`, `package.json` avec `packageManager pnpm@12.8.1`, `pnpm-workspace.yaml` sur `apps/*`, `pnpm-lock.yaml`, `.gitignore`, `AGENTS.md`, `PLAN.md`). Identité locale : kevsi roughi <kev@mail.com>.
- Preuve : `git log --oneline` → `4c19a26 chore: initialise le depot` ; `git ls-files` → 8 fichiers ; `pnpm install` → `Already up to date / Done in 754ms` ; `LICENSE` = 35149 octets, tête `GNU GENERAL PUBLIC LICENSE / Version 3, 29 June 2007`.
- Points ouverts : `packageManager` épinglé à la version constatée (12.8.1) ; structure `apps/desktop`, `engine/`, `contract/`, `scripts/`, `docs/` à créer dans les tâches suivantes (0.4–0.8).

### 2026-10-02 — Tâche 0.2
- Plan : (1) mesurer le présent (5 versions) ; (2) installer le manquant via scoop ; (3) prouver la chaîne Rust jusqu'au binaire ; (4) consigner + cocher.
- Fait : manquaient Java, Gradle, Rust. Installés via scoop (`bucket add java` + `install temurin21-jdk gradle rustup`). Scoop vit en `~\scoop` mais ses shims ne sont pas sur le PATH du shell → préfixe manuel à chaque appel. MSVC déjà présent (BuildTools 18 + MSVC 14.51.36231 + WinSDK 10), `link.exe` trouvé, chaîne prouvée par un hello-world compilé et exécuté.
- Preuve : `java -version` → Temurin 21.0.12.1 LTS ; `gradle -v` → 9.8.0 ; `rustc --version` → 1.99.0 ; `cargo --version` → 1.99.0 ; `node -v` → v26.10.0 ; `pnpm -v` → 12.8.1 ; `cargo build` hello-world → `Finished dev in 3.03s` + `Hello, world!`. Install : `scoop install` → `temurin21-jdk`, `gradle`, `rustup` = `installed successfully`, EXIT 0.
- Points ouverts : shells futurs doivent charger `vcvars64.bat` avant `cargo build` (link.exe hors PATH) ; `JAVA_HOME`, `CARGO_HOME`, `RUSTUP_HOME` persistés en vars user par scoop ; `gradle-all` (252 MB) a fait timeouter une extraction à 10 min → relance OK depuis le cache.

### 2026-10-02 — Tâche 0.3
- Plan : (1) localiser les 2 dépôts amont ; (2) lire README + `MangaLoaderContext.kt` + `MangaLoaderContextImpl.kt` (dl) + `build.gradle.kts` (dl) ; (3) écrire `docs/notes/loader-context.md` ; (4) cocher + commiter.
- Fait : note écrite (`docs/notes/loader-context.md`, 53 lignes) : coords exactes `com.github.Kotatsu-Redo:kotatsu-parsers-redo:<commit>` via JitPack (README du fork périmé, cite encore KotatsuApp), inventaire abstract/open du contexte, 5 points durs desktop avec réponses (nashorn-core:15.6, Bitmap sur BufferedImage, fenêtre 0.11 pour WebView/Cloudflare, cookies en Rust), 5 À VÉRIFIER pour 0.4.
- Preuve : lectures via API GitHub — redo HEAD `b3c0000` (2026-09-17, 0 tags, 0 releases), dl `build.gradle.kts` (parsers `14cff0d651`, okhttp 4.12.0, nashorn 15.6), `MangaLoaderContext.kt` (5217 o), `Bitmap.kt` (3 membres) ; fichier relu sur disque (53 lignes, 7 marqueurs clés).
- Points ouverts : JitPack doit encore construire le SHA redo (0.4) ; `evaluateJs` 3 args du fork vs 2 args chez dl — implémenter la 3 args.

### 2026-10-02 — Tâche 0.4
- Plan : (1) lire build amont (versions) + helpers dl ; (2) scaffold `engine/` + CLI ; (3) build, `list-sources`, `search MANGADEX` ; (4) stop daemon, journal, commit.
- Fait : `engine/` créé (settings/build/gradle.properties + 5 sources : Main, EngineContext, BitmapImpl, InMemoryCookieJar, DefaultMangaSourceConfig). Parsers `b3c0000` via JitPack, okhttp 5.1.0/okio 3.16.0/kotlin 2.2.10/coroutines 1.10.2/nashorn 15.6. Daemon stoppé.
- Preuve : `gradle run list-sources` → BUILD SUCCESSFUL, `sources: 1370`, MANGADEX listé ; `gradle run search MANGADEX naruto` → BUILD SUCCESSFUL, `results: 20` (ex. `Naruto: The Whorl within the Spiral`). 1er build JitPack : timeout Gradle → build serveur OK (pom 200, 2619 o). 2 incidents fixés : `google()` manquant (androidx.collection) + `requireBody()` déprécié sous OkHttp 5.
- Points ouverts : `evaluateJs`/Nashorn non exercé (MangaDex sans JS) ; usage d'`interceptWebViewRequests` par les sources non inventorié ; pas de wrapper Gradle (système 9.8.0) ; sorties capturées dans `AppData\Local\Temp\opencode\listsources.txt` et `searchmdx.txt`.

### 2026-10-02 — Tâche 0.5
- Plan : (1) écrire `protocol.json` + `contract/README.md` (10 méthodes, 7 codes) ; (2) ADR 0001 stdio vs HTTP ; (3) valider le JSON par exécution, journal, commit.
- Fait : `contract/protocol.json` (enveloppe + 8 defs), `contract/README.md` (table méthodes/params/result + points ouverts v1), `docs/decisions/0001-transport-sidecar.md` (statut acceptée). Incident : `node -e` inline cassé par l'expansion `$` du shell → script fichier (règle connue).
- Preuve : script node → `methods=10` (hello..shutdown), `codes=7`, `defs=method,errorCode,request,response,manga,chapter,page,sourceInfo`.
- Points ouverts : annulation par requête reportée (v0 = fermer stdin) ; `fetchImage` base64 à réévaluer si pages > ~10 Mo.

### 2026-10-02 — Tâche 0.6
- Plan : (1) lire modèles Manga/Chapter/Page + CloudFlareHelper ; (2) écrire Server.kt (10 méthodes, cache jetable, timeouts) ; (3) installDist + tests pipés ; (4) stop daemon, journal, commit.
- Fait : `Server.kt` (dispatch + cache session + `withTimeout`), `CloudFlareInterceptor.kt` + `CloudFlareProtectionException.kt` (→ CHALLENGE_REQUIRED), `putCookies` + `userAgentOverride`, commande `serve`. CloudFlareHelper confirmé présent dans le fork.
- Preuve : 4 runs pipés via `installDist`, EXIT 0 partout — hello ✓, listSources=1370 ✓, search=3 ✓, details ✓, chapters=40 ✓, pages=19 ✓, fetchImage=2393492 o image/jpeg ✓, setCookies ✓, health ×2 ✓, shutdown ✓ (exit 0). Soit 10/10 méthodes exercées. Sorties : `AppData\Local\Temp\opencode\out{1,2a,2b,2c,2d}.ndjson`.
- Points ouverts : `fetchImage` sans désembrouillage (note dans le code, MangaDex direct) ; cache session = erreur PARSE_ERROR « call search first » si process neuf (comportement voulu, vu en run2a).

### 2026-10-02 — Tâche 0.7
- Plan : (1) inspecter installDist ; (2) jdeps → modules ; (3) script build-engine.ps1 ; (4) mesurer froid + RAM sur le runtime ; (5) journal + commit.
- Fait : `scripts/build-engine.ps1` (installDist → jdeps → jlink strip/compress → launcher `-Xmx256m -XX:+UseSerialGC`). Modules : base, desktop, instrument, logging, net.http, scripting, dynalink, unsupported + `jdk.crypto.ec` ajouté à la main. Incidents : `java -version` sur stderr tuait le script sous `Stop` → `Continue` + `$LASTEXITCODE` ; 1re mesure RAM = wrapper cmd.exe → harness v2 sur le java.exe enfant ; TLS `handshake_failure` sur le runtime nu (témoin JDK complet OK) → SunEC manquant, fix vérifié.
- Preuve (runtime jlink, harnais flux redirigés) : dist = **64.1 MB** (runtime 47.2 + lib 16.8) ; démarrage à froid **1576 ms** (lancement → hello) ; idle WS **71.9** / privée 231.7 MB ; search 2047 ms, pic WS **114.5** / privée 256.2 MB (≈ plafond Xmx, respecté) ; search OK 3 résultats, shutdown exit 0. Private ≈ réserve adresses (tas), WorkingSet = RAM réelle.
- Points ouverts : `--compress=2` déprécié (warning, non bloquant) ; revue du plafond Xmx si sources lourdes en 0.11.

### 2026-10-02 — Tâche 0.8
- Plan : (1) docs Tauri v2 (Next export, sidecar, shell) ; (2) scaffold apps/desktop + superviseur ; (3) install/build/typecheck/check/clippy ; (4) preuve dev live crash→relance ; (5) journal + commit.
- Fait : Next 16.0.10 + React 19 (export `out/`, `GET / 200`), tauri 2.12.1, `supervisor.rs` (spawn java serve, hello MAJEUR=0, watchdog backoff 1s→30s, kill à la sortie), `main.rs` (ExitRequested → shutdown, pas de java orphelin), capabilities vides (0 permission : Rust spawn direct, pas de plugin shell). Icônes placeholder générées (`tauri icon`, logo réel à valider).
- Preuve : `cargo check`/`clippy -D warnings`/`fmt --check` verts ; `pnpm build` → `out/index.html` ; `pnpm typecheck` vert ; `tauri dev` live — `sidecar démarré (pid=16540)`, `hello OK (protocol=0.1)`, `taskkill 16540` → `sidecar sorti, relance` → pid 11844 → `hello OK`. Logs : `AppData\Local\Temp\opencode\tauridev2.txt`.
- Points ouverts : `externalBin` NON déclaré (binaire JVM, pas natif → ira en `resources` en 0.9+, cf. commentaire `supervisor.rs`) ; versions `@tauri-apps/*` corrigées `^2.12.1` (registre) ; `allowBuilds: sharp` persisté ; bug chemin dev `../../` → `../../../` trouvé par l'exécution ; fenêtre non capturée (preuves = logs).

### 2026-10-02 — Audit UX + refonte visuelle + correctifs (non commité)
- Plan : (1) audit UX piloté (captures + console + mesures) ; (2) corriger chaque finding ; (3) refonte « cinéma sombre » + grille compacte ; (4) refaire le runtime jlink sans casser le dist précédent.
- Fait : **encodage** — `page.tsx` ré-écrit en double UTF-8 par un `-replace` PowerShell (42 occurrences) : réparé, plus aucun U+FFFD. **Filtre genre inerte** — le `onClick` déclenchait un second `loadSource` dans un `setTimeout` qui relisait l'état *ancien* ; supprimé, la liste change vraiment (preuve ci-dessous). **Progression** — `capabilities/default.json` avait `permissions: []` → `listen()` levait `core:event:allow-listen` ; `core:event:default` ajouté. **Titres de cartes** — `overflow:hidden` clippe sur la *padding-box* : la 3ᵉ ligne se peignait dans le padding bas ; borné par `max-height: 2 lignes + padding haut`. **Barre d'onglets** extraite en `AppBar` (présente dans les 4 vues, avant : absente hors catalogue). **Chapitres** — 778 lignes rendues d'un bloc, plafonnées à 100 + « Afficher plus ». **Contenu adulte** — `safeOnly` (défaut vrai) → `MangaListFilter.contentRating = SAFE/SUGGESTIVE`, seulement si la source expose une classification ; réglage exposed dans Paramètres. **Bandeau tips rotatif** supprimé (incohérent : présent uniquement sur le catalogue). **`.gitignore` corrompu** — la ligne `*.tsbuildinfo` était devenue `*` : *tout* nouveau fichier du dépôt était ignoré (8 fichiers, dont `cbz.rs`, `download.rs`, `commands.rs`, `globals.css`). **Script build** — jlink + swap versés dans `dist\.stage`, échange en fin de script : un build raté ne détruit plus un dist valide.
- Preuve : `pnpm typecheck` EXIT 0 ; `cargo clippy --all-targets -- -D warnings` EXIT 0 ; `cargo test` 1 passed EXIT 0 ; `gradle installDist` BUILD SUCCESSFUL ; runtime reconstruit **64.1 MB**, `latest` répond sur le dist posé (n=5). Parcours CDP (walks `walk-fix.js`, `walk-filtre.js`, `walk-download.js`, `walk-refonte.js`, `%TEMP%\opencode\audit\*.png`) : **filtre genre** `liste_identique=false` puis « Effacer » restaure la liste initiale ; **titres** `hauteur_anormale=0` ; **progression** `Quiero vivir — préparation…` puis `Téléchargé : …\Chiikawa_ Nanka Chiisakute Kawaii Yatsu\Quiero vivir.cbz` ; **chapitres** `chapitres_rendus=100` + bouton « Afficher plus » ; **0 erreur console** sur tous les écrans. **A/B moteur** (`safeOnly` true/false sur MANGADEX via `tatami-engine.bat serve`) : `true` → `Fukushuu Doumei… | Yowamushi Pedal | Drive My Heart!`, `false` → `Chiikawa… | Yumeneconnect | Dragon Quest…` : le filtre change bien la source, mesuré.
- Points ouverts : `pnpm lint` est cassé (`next lint` n'existe plus dans Next 16) → à remplacer (eslint direct) ; `contract/protocol.json` n'énumère pas `latest`/`filterOptions` dans son enum de méthodes ; adult filtering réservé aux sources qui exposent `availableContentRating` (MangaDex oui, Mes sources non) ; rien n'est commité, en attente de validation visuelle à la relance.

### 2026-10-02 — Sources vides / sources bloquées (non commité)
- Plan : (1) sonder un échantillon de sources sur le moteur installé ; (2) corriger la cause des 0 résultat et des attentes ; (3) échouer vite et lisiblement.
- Fait : **`Multiple Content ratings`** — le moteur renvoyait `SAFE+SUGGESTIVE`, or les parsers n'en acceptent qu'un (`setOf(SAFE)`). Réglage passé à 3 états, défaut **SUGGESTIVE** (mesuré : SELFMANGA renvoie 0 résultat en SAFE — c'est ce qui vidait les sources). **Attentes** — deux causes distinctes : (a) le moteur traitait les requêtes **séquentiellement**, donc une source morte gelait toutes les autres ; (b) `timeoutMs` n'était pas transmis, le moteur ignorait le timeout de Rust et tenait 120 s. Corrigé : un thread par requête (`Executors.newCachedThreadPool`, écritures stdout synchronisées), `ConcurrentHashMap` pour les cookies, `@Volatile` sur le user-agent, `timeoutMs` dans l'enveloppe, timeouts par méthode (listes 30 s, détails/chapitres 60 s, `listSources` 10 s). **UI** — erreurs traduites en français court (`describeError` : 404 / hôte inconnu / 403 / timeout / anti-bot) avec « Réessayer » et « Changer de catalogue », plus d'attente muette.
- Preuve : sondage de 24 sources sur le runtime installé → `MANGADEX 12/2634 ms`, `MANGASORIGINES 12/3279 ms`, `MANGABUFF 12/1026 ms`, `SEIMANGA/ZENMANGA/WAMANGA 12` ; échecs réels : `COMX 404`, `NUDEMOON` hôte inconnu, `MINTMANGA 404`, `MANGALIB 403`, `BAOZIMH 403`, `SELFMANGA 0 résultat en SAFE`. Concurrence (6 requêtes simultanées dont 3 sources mortes) : **6/6 réponses entre 7 170 et 8 640 ms**, les sources mortes n'ont pas retenu les autres. `contentRating` SAFE / SUGGESTIVE / vide : 3 réponses distinctes, aucune erreur. Gates : `clippy -D warnings` 0 · `cargo test` 1 passed · `tsc` 0 · `gradle installDist` OK.
- Points ouverts : les sources mortes (404 / domaine disparu / 403 anti-bot) sont un fait du fork `kotatsu-parsers-redo b3c0000` — aucun correctif applicatif possible, il faudra une liste « hors service » ou un tri par échec ; `CHALLENGE_REQUIRED` (fenêtre de résolution) toujours non implémentée ; le timeout annule la requête mais le thread du parser peut rester occupé jusqu'à son propre délai OkHttp.

### 2026-10-02 — Téléchargements par webtoon + sélection multiple (8045ea2)
- Plan : (1) persister l'état d'avancement par webtoon ; (2) refaire la vue Téléchargements en webtoons ; (3) sélection multiple + file d'attente ; (4) mesurer le cas « source incomplète ».
- Fait : `library.rs` (nouveau) — `meta.json` par webtoon (titre, source, url, couverture, total, chapitres) écrit à chaque `.cbz`, commande `list_library` (vue webtoon : téléchargés, total, complet, poids) et `existing_chapters` pour ignorer ce qui est déjà là ; état reconstruit quand le meta est absent. Vue Téléchargements en cartes webtoon (couverture, `x/y chapitres`, poids, badge). Mode sélection dans la vue source + file séquentielle annulable, chapitres présents ignorés, bascule automatique vers Téléchargements. Titre de chapitre UUID remplacé par son numéro. Délai d'abandon 60 s, réglable (30/60/120).
- Preuve : `cargo test` 3 passed (2 nouveaux : meta idempotent, reconstruction sans meta), `clippy -D warnings` 0, `tsc` 0. Parcours CDP : `8 webtoons · 15 chapitres`, cartes `曼波～曼波～ part 1 · EXHENTAI · 9/99 · 43,9 Mo` et `No. 6 · MANGADEX · 2/3 · 64,2 Mo`, 0 erreur console, un vrai `.cbz` de 43 Mo produit par la file. **Pagination mesurée** : EXHENTAI offset 10 renvoie 0 résultat (le parser ignore l'offset) alors que MANGADEX, MANGASORIGINES et SEIMANGA renvoient 10 puis 10 ; EXHENTAI met 35,7 s pour une page. D'où : « Voir plus » disparaît avec une explication, et le délai est passé de 30 à 60 s.
- Points ouverts : une file de 99 chapitres tourne des heures et n'est pas reprise après redémarrage (les chapitres déjà là sont ignorés, donc un simple clic repart, mais l'état de la file est perdu) ; pas de suppression d'un webtoon téléchargé depuis l'interface ; le poids affiché est celui des `.cbz`, pas des images décompressées.

### 2026-10-03 — Taches 0.9 et 0.10 : listes + lecteur pagine
- Plan : (1) cocher 0.9 (couvert par les commandes existantes) ; (2) 0.10, afficher une page de chapitre ; (3) prouver par capture et navigation clavier.
- Fait : bouton **Lire** sur chaque chapitre ; la commande `pages` renvoie des jetons et l'image arrive par le protocole `tatami://page/<token>` (Rust : `fetchImage` -> cache disque -> webview, jamais d'appel direct aux hotes distants). Lecteur plein ecran : compteur de pages, titre webtoon + chapitre, boutons et clavier (gauche, droite, espace, Debut, Fin, Echap), prechargement de la page suivante.
- Preuve : parcours CDP sur MANGADEX — `src = http://tatami.localhost/page/2f7a4dbe...`, image naturelle **800x1148** affichee, `1/33`, deux fleches -> `3/33` avec une **seconde image differente** (800x1149), Echap referme, **0 erreur console et 0 erreur d'image**. Hauteur du lecteur 701 px pour une fenetre de 745 px (plus de debordement). `tsc` 0, `clippy` 0, `cargo test` 6 passes.
- Points ouverts : pas de zoom ni de sens de lecture (1.8), pas de reprise de position par chapitre (1.6), pas de mode webtoon defilant (1.9) ; l'image est bornee par la fenetre sans etre agrandiee, donc les petites pages restent petites ; la precharge ne couvre que la page suivante.

### 2026-10-03 — Tache 0.11 : sources protegees (fenetre de verification)
- Plan : (1) verifier que les cookies HttpOnly sont recuperables sur Windows ; (2) ouvrir une fenetre de resolution, transmettre cookies + user-agent au moteur, rejouer ; (3) mesurer.
- Fait : challenge.rs (nouveau) — open_challenge ouvre une WebView2 sur l'URL du challenge avec **le user-agent du moteur** (le moteur l'expose desormais dans `hello`), solve_challenge lit les cookies **HttpOnly** via `WebviewWindow::cookies_for_url` et les pousse via `setCookies`, cancel_challenge ferme sans appliquer. Cote interface : « Resoudre la verification », « C'est fait, reessayer », « Annuler » ; une demande de verification n'est **pas** marquée source hors service.
- Preuve : appel reel des trois commandes via l'IPC — `open_challenge` ok et fenetre listee par CDP (`http://localhost:3000/` + `https://mangadex.org/`), `solve_challenge` = **4 cookies** transmis, fermeture propre ; UA de la fenetre = `Mozilla/5.0 (X11; Linux x86_64; rv:109.0) Gecko/20100101 Firefox/116.0`, identique au `defaultUserAgent` du moteur ; 0 erreur console. Bug trouve par ce test : un second clic **refermait** la fenetre au lieu de la reutiliser (`close()` est asynchrone, le label existe encore) -> desormais on la reutilise. `clippy` 0, `cargo test` 7 passes, `tsc` 0.
- Points ouverts : **aucune source de l'echantillon ne renvoie reellement `CHALLENGE_REQUIRED`** (MANGALIB et BAOZIMH renvoient un 403 classique, pas un challenge Cloudflare) : la fin de chaine reste donc a confirmer sur un vrai site protege, cf. §6. Limites connues : la clearance est liee a l'IP et expire ; l'empreinte TLS de WebView2 differe de celle d'OkHttp, donc certains sites peuvent refuser la clearance malgre tout.
