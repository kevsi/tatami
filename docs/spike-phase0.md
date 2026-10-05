# Spike phase 0 — mesures, décisions, ajustements

Ce document répond à la tâche 0.12 du `PLAN.md` : **mesurer** avant de figer la
v1. Toutes les valeurs ci-dessous ont été obtenues en exécutant la commande
indiquée, sur la machine cible (portable Windows 10, i5-6200U, 12 Go de RAM,
WSL2/Fedora). Les scripts sont dans `.mesures/` (ignoré par git, donc
reproductibles uniquement en local).

Une mesure sans sa commande et son contexte ne vaut rien : chaque tableau
donne la méthode, et les limites sont dites explicitement.

---

## 1. Ce qui marche, avec les chiffres

### 1.1 Le moteur démarre vite et tient en peu de mémoire

Mesure faite en lançant le sidecar **exactement comme le
superviseur** (mêmes options JVM : `-Xmx256m`, `-XX:+UseSerialGC`,
`-Djava.net.preferIPv4Stack=true`, encodage verrouillé, `serve`).

| Étape | Temps |
|---|---|
| JVM → bannière (`tatami-engine serve protocol=0.1 …`) | **880 ms** |
| poignée de main `hello` | **104 ms** |
| `listSources` — 1 370 sources, 47 417 octets NDJSON | **180 ms** |
| `shutdown` propre (entrée + sortie) | **1 208 ms** au total |
| Code de sortie | **0** |

Côté mémoire du processus `java` :

| Étape | RAM résidente | RAM privée |
|---|---|---|
| démarrage | 68,5 Mo | 230,5 Mo |
| après `hello` | 73,1 Mo | 232,0 Mo |
| après `listSources` (1 370 sources) | 79,8 Mo | 233,9 Mo |
| **après le 1ᵉʳ appel réseau réel** | **136,5 Mo** | **261,4 Mo** |
| après 2 appels de plus | 135,5 Mo | 260,2 Mo |

**Le premier appel réseau coûte +57 Mo** (79,8 → 136,5 Mo). Ce que la mesure
établit : l'écart apparaît au **premier appel seulement**, le reste est plat.

Ce que je **n'ai pas** mesuré, et qu'il ne faut donc pas affirmer : *la cause*
de ces 57 Mo. L'hypothèse est le chargement des parseurs Kotatsu — le JAR en
contient 3 844 classes pour 8,7 Mo, et le catalogue en compte 1 370 — mais cela
demanderait un relevé de classes chargées (`-verbose:class` ou un heap dump), pas
une lecture de `WorkingSet64`. À confirmer si la mémoire devient un sujet.

Le pic ensuite est **plat** : 2 120 ms pour MANGADEX avec 25,8 Ko de réponse,
puis 720 ms et 3 ms pour les deux sources suivantes.

**Réserve sur ces deux appels** : MANGAFR a renvoyé **0 octet** et MANHWA
**0,1 Ko**. Une réponse vide est ambiguë — source vide, source désactivée, ou
erreur — et je ne l'ai pas tranchée. Ces 720 ms et 3 ms ne disent donc rien sur
le coût d'un vrai parcours. Seul MANGADEX a renvoyé des données.

Conclusion : **`-Xmx256m` est serré mais tenable**. La RAM privée (261 Mo) dépasse
`-Xmx` parce que cette option ne couvre que le tas, pas le metaspace ni les
buffers natifs d'OkHttp. Sur 12 Go de RAM, avec le navigateur et les outils de
dev ouverts, il reste de la marge.

**Ajustement proposé.** Rien d'urgent. Si le planificateur (3.1) ajoute de la
concurrence, surveiller la RAM privée : c'est elle qui plafonnera en premier, pas
la résidente.

### 1.2 L'app et les données sont légères

| Élément | Taille |
|---|---|
| `engine/dist/tatami-engine` complet | **64,1 Mo** (180 fichiers) |
| dont runtime `jlink` embarqué | **47,2 Mo** (162 fichiers) |
| base `tatami.db` (favoris, historique, réglages) | **64 Ko** |
| cache images `%APPDATA%\Tatami\cache` | **388,4 Mo** (1 527 fichiers) |

La base est négligeable : tout l'état utilisateur tient dans 64 Ko. C'est le bon
choix pour la v1.

---

## 2. Ce qui bloque, ou nearly bloque

### 2.1 Le cache d'images n'est borné qu'en nombre, pas en volume — **à traiter**

`CAP_PAGE = 2048` borne l'**index en mémoire**, et l'éviction supprime bien le
fichier correspondant (`pagecache.rs`, `remove_file` sur la victime). Mais rien
ne borne le **volume**.

Mesuré : moyenne **260,5 Ko** par page, maximum **6 121 Ko**.

> **2048 entrées × 260 Ko ≈ 521 Mo de cache disque possible**, sur une machine
> de 12 Go. La base est à 64 Ko ; le cache est à 388 Mo. Le rapport est de
> 6 000 à 1.

Ce n'est pas encore un problème — 388 Mo après des semaines d'usage — mais c'est
une **croissance non bornée** qui finira par occuper un disque, sans que rien ne
l'annonce. Un cache d'images de lecture est jetable par nature : c'est le
seul élément du projet qui mérite une limite en octets.

**Ajustement proposé** : ajouter un plafond en volume ( proposition : **1 Go**,
avec éviction des plus anciens par date d'écriture) et l'exposer dans l'écran
« espace utilisé ». La tâche 2.5 dit « gestion de l'espace : taille utilisée,
suppression » — c'est le bon endroit, mais le plafond doit exister **avant** que
l'interface ne promette de maîtriser une taille.

### 2.2 Trois sources sur 31 échouent en TLS — **bloqué, décision de sécurité**

`PKIX path building failed`. Le diagnostic d'origine (le runtime `jlink`) était
faux : le runtime embarqué **et** le JDK complet échouent identiquement.

Deux manques distincts, tous deux mesurés :

1. Le serveur n'envoie que la feuille, et l'intermédiaire Let's Encrypt
   `YE1`/`YE2` est absent — **la JVM ne fait pas de récupération AIA**.
2. La chaîne se termine sur une racine Let's Encrypt **`Root YE`**, absente du
   `cacerts` embarqué, de celui du JDK Temurin 21 **et** du magasin Windows de
   cette machine (qui ne contient que `ISRG Root X1`).

**Jamais** de contournement par désactivation de la validation TLS (`AGENTS.md`
§10). La voie propre est un JDK plus récent, ou l'ajout explicite de la racine —
ce qui **élargit la confiance du système** et relève donc d'une décision humaine.
Voir la tâche 0.16, non close.

### 2.3 La commande de vérification `gradlew test` ne pouvait pas fonctionner

`AGENTS.md` §8 indiquait `./gradlew test`. Vérifié le 2026-10-05 :
`Get-ChildItem gradlew*` ne renvoie **rien** — il n'y a pas de `gradlew.bat` dans
le dépôt (Gradle est installé globalement via Scoop). La commande écrite ne
pouvait donc pas tourner. Corrigée : `gradle test` depuis `engine/`, avec le
dossier de chaque commande explicité.

C'est exactement le piège que `AGENTS.md` §2 signale (« ne jamais inventer ») :
une commande de vérification qui n'a jamais été exécutée est une commande
fictive. Elle figurait dans le document de règles depuis le début.

### 2.4 Le port CDP 9223 tombe dès que Tauri rebuild

Mesuré pendant la tâche 1.8 : écrire dans `src-tauri/` déclenche un rebuild, donc
l'app **redémarre** et le port de débogage distant disparaît. Une mesure en cours
échoue alors avec `ECONNREFUSED`, sans que rien ne soit cassé.

Corollaire d'organisation : **une seule session peut mesurer l'app**. Les autres
peuvent écrire du code et lancer les tests, mais pas piloter l'application. Trois
sessions en parallèle ont produit des redémarrages en série qui ont coûté
plusieurs minutes de mesures perdues.

---

## 3. Décisions prises pendant la phase 0

Les quatre ADR sont dans `docs/decisions/`. Résumé et **raison** :

| ADR | Décision | Pourquoi celle-là |
|---|---|---|
| `0001-transport-sidecar` | NDJSON sur stdio, aucun port ouvert | Un port ouvert dans une webview est une surface d'attaque ; stdio n'en est pas une. `stdout` est réservé au protocole, les logs vont sur stderr — sinon le lecteur JSON rejette une ligne partielle et prend le moteur pour corrompu. |
| `0002-schema-sqlite-migrations` | Migrations *forward-only* + sauvegarde avant migration | Réécrire une migration appliquée **casse la base définitivement** (mesuré : `favorites.content_rating` absente avec `user_version = 3`, parce que deux éditions avaient séparé le numéro de version de son `ALTER TABLE`). |
| `0003-images-protocole-tatami` | Les images passent par le moteur, jamais par la webview | Une webview qui appelle un site de manga perd l'user-agent, les cookies et le déblocage Cloudflare — **et l'échec est silencieux** : vignette vide, console propre. Aucun appel direct, et `https:` retiré de `img-src` pour que l'erreur soit bloquée au lieu d'être invisible. |
| `0004-permissions-tauri` | Permissions au strict minimum, révisée pour `dialog:allow-open` seule | Le sélecteur de dossier de téléchargement n'a demandé qu'une permission : pas d'écriture, pas d'accès large au système de fichiers. |

---

## 4. Ce que les mesures ont changé dans le plan

Une mesure qui ne modifie aucune décision ne servait à rien. Voici ce qui a bougé.

1. **Le cache d'images est devenu une tâche à part** (2.5, plafond en volume).
   Tant qu'il n'était qu'un « cache », personne n'avait mesuré 388 Mo.
2. **Le cache HTTP de Chromium a été identifié comme un second cache**, non
   documenté, qui répond en quelques millisecondes même fichier absent du
   disque. Toute mesure de performance d'image doit le neutraliser
   (`Network.setCacheDisabled`) — sans quoi elle ne prouve rien. La leçon est
   consignée dans `AGENTS.md`, section « Leçons de session ».
3. **La tâche 0.16 a été requalifiée** : le runtime `jlink` n'était pas en cause,
   et l'origine du problème est un **défaut de chaîne de confiance**, pas un
   problème d'empaquetage. Deux causes distinctes, pas une.
4. **Le dénominateur des proportions est explicité.** Le catalogue compte
   **1 370 sources** (`listSources: 1370 sources` dans les journaux du
   superviseur, mesuré le 2026-10-05), mais les mesures de la phase 0 portent
   sur **31 sources en chaîne complète** (`docs/rapport-session-recherche.md` :
   « 31 sources en chaîne complète, +8 … »). Un « 3 sources sur 31 » de la
   tâche 0.16 décrit donc un **échantillon**, pas une proportion du catalogue.
   La phrase est correcte tant qu'elle le dit ; à l'écrire sans le dire, elle
   ferait croire à un taux de 9,7 % alors qu'on n'a rien mesuré sur 1 339
   sources. Toute proportion doit nommer son dénominateur.

---

## 5. Limites de ces mesures, énoncées

Rien de ce qui suit n'a été fait, donc rien n'est affirmé :

- **Le `shutdown` du moteur n'a pas été mesuré en conditions réelles** de
  fermeture d'application (il l'est en 0.15 via les tests).
- **La tenue en charge** (10 chapitres ouverts, 300 pages, plusieurs sources en
  parallèle) n'a pas été mesurée. Le pic de 136 Mo est un cas simple.
- **Le disque** : aucune mesure d'E/S, alors que le cache écrit 260 Ko par page.
  Sur un disque mécanique, un chapitre de 300 pages représente 78 Mo d'écritures
  au premier parcours.
- **`i18n:check`** : la porte existe (152 clés), mais elle vérifie les clés, pas
  la **qualité** des traductions. Une traduction littérale passe.
- **L'écriture de `Root YE`** (2.2) reste sans solution tant que la décision de
  sécurité n'est pas prise.

---

## 6. Commandes de vérification figées

Elles sont dans `AGENTS.md` §8, avec leur dossier. En résumé :

```powershell
cd apps\desktop        && pnpm typecheck && pnpm lint && pnpm i18n:check && pnpm build
cd apps\desktop\src-tauri && cargo fmt --check && cargo clippy -- -D warnings && cargo test
cd engine              && gradle test
```

Le smoke test manuel est `docs/smoke-test.md` (539 lignes).
