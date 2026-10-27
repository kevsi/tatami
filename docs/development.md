# Développement

Informations techniques pour construire, lancer et vérifier Tatami. Le README est centré sur les
utilisateurs ; ce document reprend les sections techniques qui y étaient le 2026-10-20.

## Architecture

| Couche | Techno | Rôle |
|---|---|---|
| Interface | Next.js 16 (export statique) + TypeScript, dans la webview Tauri | Affichage uniquement |
| Cœur | Rust, Tauri 2 | Données, téléchargements, cache d'images, sync, suivi, supervision du moteur |
| Moteur de sources | Kotlin/JVM, sidecar | Charge `kotatsu-parsers-redo`, interroge les sites |

```
Interface (Next.js statique)
        │  commandes / événements Tauri uniquement
        ▼
Cœur Rust (Tauri)  ── SQLite, téléchargements, cache tatami://, planificateur, sync, suivi
        │  contrat versionné (NDJSON sur stdio, aucun port ouvert)
        ▼
Moteur JVM (sidecar) ── kotatsu-parsers-redo ──► sites de manga
```

Quatre règles structurantes, détaillées dans [`AGENTS.md`](../AGENTS.md) §4 :

1. l'interface ne parle qu'à Rust, jamais au sidecar ni au réseau des sources ;
2. Rust est le seul client du sidecar — il le lance, le supervise, le relance après un crash ;
3. le sidecar est sans état persistant, toute donnée utilisateur vit dans SQLite côté Rust ;
4. les images passent par le moteur, sont mises en cache par Rust et servies via le protocole
   `tatami://` — jamais d'appel direct à un hôte d'images depuis la webview.

## Dépendances amont

| Composant | Provenance | Version |
|---|---|---|
| `kotatsu-parsers-redo` | [Kotatsu-Redo/kotatsu-parsers-redo](https://github.com/Kotatsu-Redo/kotatsu-parsers-redo), via JitPack | **commit `b4df40e9`** (2026-10-16, aucun tag ni release) |
| Kotlin | Gradle | 2.2.10, `jvmToolchain(21)` |
| OkHttp / Okio | `gradle/libs.versions.toml` du fork | 5.1.0 / 3.16.0 |
| Nashorn | `org.openjdk.nashorn:nashorn-core` | 15.6 — le JDK 21 ne fournit plus de moteur JS |
| Tauri | crates.io | 2.12.1 |

Le commit est **épinglé** parce que le fork n'a ni tag ni release : sans épinglage, le build
changerait de sources à chaque résolution. Voir [`docs/notes/loader-context.md`](notes/loader-context.md)
pour les coordonnées exactes et les 5 points durs desktop de `MangaLoaderContext`.

## Structure du dépôt

```
tatami/
  AGENTS.md          directives de travail de l'agent
  PLAN.md            feuille de route vivante (phases, journal, blocages)
  README.md          présentation pour les utilisateurs
  CODE_SIGNING.md    politique de signature et de confidentialité
  LICENSE            GPLv3
  apps/desktop/      Next.js (export statique) + src-tauri (Rust)
  engine/            projet Gradle Kotlin : le sidecar JVM
  contract/          protocole.json : schéma du contrat Rust ↔ moteur
  scripts/           build-engine.ps1 (installDist + jlink + lanceur)
  docs/              smoke-test.md, development.md, decisions/ (ADR), notes/, ...
```

Chemins de données à l'exécution (Windows) :

| Donnée | Emplacement |
|---|---|
| Base utilisateur | `%APPDATA%\Tatami\tatami.db` |
| Sauvegardes avant migration | `%APPDATA%\Tatami\tatami-v<N>-<horodatage>.db` |
| Cache d'images (`tatami://`) | `%APPDATA%\Tatami\cache\` |
| Téléchargements | `%APPDATA%\Tatami\downloads\<source>\<webtoon>\*.cbz` |

## Construire et lancer

### Prérequis

Installés avec Scoop (winget ne fonctionne pas sur la machine cible) :

```powershell
scoop bucket add java
scoop install temurin21-jdk gradle rustup
```

Node via nvm4w, gestionnaire JS pnpm. Versions utilisées pendant le développement : Temurin
21.0.12.1 LTS, Gradle 9.8.0, rustc 1.99.0, Node 26.10.0, pnpm 12.8.1.

Deux pièges d'environnement mesurés sur cette machine :

- les **shims Scoop** ne sont pas dans le PATH du shell → préfixer `$env:USERPROFILE\scoop\shims` ;
- `link.exe` (MSVC) est hors PATH → **charger `vcvars64.bat` avant tout `cargo`**.

### 1. Dépendances JS

```powershell
pnpm install
```

### 2. Moteur (obligatoire avant de lancer l'application)

```powershell
pnpm engine
```

`scripts/build-engine.ps1` enchaîne `gradle installDist`, `jdeps`, puis `jlink` pour produire un
runtime réduit dans `engine/dist/tatami-engine/` avec un lanceur `tatami-engine.bat`
(`-Xmx256m -XX:+UseSerialGC`). Taille mesurée : **64,1 Mo** (runtime 47,2 + 18 JAR 16,8),
démarrage à froid **1 576 ms** jusqu'à `hello`, mémoire privée au repos 231,7 Mo, pic à
256 Mo pendant une recherche. Le script affiche `OK: <chemin> (<taille> MB)` en fin de course.

> Le runtime et les JAR **ne sont pas commités** (`.gitignore` : `dist/`, `build/`). Il faut donc
> cette étape sur chaque poste, après chaque changement de moteur.

Contrôle du moteur sans l'application :

```powershell
engine\dist\tatami-engine\tatami-engine.bat list-sources        # attendu : sources: 1374
engine\dist\tatami-engine\tatami-engine.bat search MANGADEX naruto
```

### 3. Lancer l'application

```powershell
pnpm tauri dev
```

`tauri dev` démarre le serveur Next sur `http://localhost:3001` puis ouvre la fenêtre (1280 × 800).

Le moteur est **empaqueté avec l'application** depuis le 2026-10-20 : `tauri.conf.json` déclare
`bundle.resources` (`engine/dist/tatami-engine` → `engine/tatami-engine`). Le MSI 0.1.0 contient
le moteur (extraction par `msiexec /a` : 184 fichiers, dont `PFiles\Tatami\engine\tatami-engine\`)
et le `sha256` du JAR du paquet est identique à celui du `dist` utilisé en vol — voir `PLAN.md`,
journal du 2026-10-20. Un binaire release lancé depuis `target\release` lit, lui, l'arbre du
dépôt (Tauri résout `bundle.resources` relativement à `src-tauri`) : c'est ce binaire-là que
pilote la vérification CDP.

### 4. Vérifications

Les commandes `cargo` sont lancées depuis la **racine du dépôt** avec le chemin du manifeste :
`apps/desktop/src-tauri` n'est pas membre d'un workspace Cargo racine.

```powershell
pnpm lint          # ESLint 9 flat config (next lint n'existe plus en Next 16)
pnpm typecheck     # tsc --noEmit
pnpm build         # export statique -> apps/desktop/out
cargo fmt --check --manifest-path apps\desktop\src-tauri\Cargo.toml
cargo clippy --manifest-path apps\desktop\src-tauri\Cargo.toml --all-targets -- -D warnings
cargo test  --manifest-path apps\desktop\src-tauri\Cargo.toml
```

ESLint est **épinglé en 9** : `eslint-config-next` déclare un peer `eslint >= 9`, mais son
`eslint-plugin-react@7.37.5` plante sur ESLint 10.

Ces commandes ne remplacent pas le test de fumée : elles prouvent que le code compile, pas que
l'application fait ce qu'on croit. Voir [`docs/smoke-test.md`](smoke-test.md).

### 5. Construire un installeur de test signé (updater)

Le build ordinaire (`pnpm tauri build`) n'active **pas** les artefacts de mise à
jour : `createUpdaterArtifacts` vit dans `src-tauri/tauri.test.conf.json`, qui
porte aussi la publique de **test** et un endpoint local. Sans cette
configuration, aucun `.sig` n'est produit — et les builds existants continuent
de fonctionner sans clé.

```powershell
$dir = "$env:USERPROFILE\.tatami-keys\test"
$env:TAURI_SIGNING_PRIVATE_KEY = "$dir\tatami-test.key"
$env:TAURI_SIGNING_PRIVATE_KEY_PASSWORD = (Get-Content "$dir\mot-de-passe.txt" -Raw).Trim()
pnpm tauri build --config src-tauri/tauri.test.conf.json
```

La clé privée et son mot de passe vivent **hors du dépôt** (`.gitignore` refuse
`*.key` en ceinture). La CLI Tauri ne lit **pas** `.env` : les deux variables
doivent être posées dans le shell du build. Résultat attendu : un `.sig` à côté
de l'installeur NSIS, dont le contenu va dans
`platforms.windows-x86_64.signature` du `latest.json` de test.

Le fichier `tauri.test.conf.json` active `dangerousInsecureTransportProtocol`
**uniquement** parce que son endpoint est en `http://127.0.0.1` : le plugin
refuse un endpoint non HTTPS en build release (`tauri-plugin-updater` 2.11.0,
`src/config.rs`). Aucune configuration de production ne doit porter ce drapeau.

### 6. Publier une prérelease de test (workflow)

Le workflow `.github/workflows/release.yml` construit, signe et publie une
**prérelease** GitHub. Il ne modifie aucune version : il vérifie que le tag
annonce la même chose que les trois fichiers qui en portent une.

```powershell
# 1. Bumper la version aux trois endroits (le workflow refuse un décalage).
#    apps/desktop/package.json, apps/desktop/src-tauri/tauri.conf.json,
#    apps/desktop/src-tauri/Cargo.toml
# 2. Commiter, puis taguer et pousser le tag.
git tag -a v0.2.0-rc.1 -m "Tatami 0.2.0-rc.1 (test)"
git push origin v0.2.0-rc.1
```

Une prérelease n'est **jamais** servie par `releases/latest` : GitHub l'en
exclut, et le job le vérifie explicitement après publication. Ses assets
restent téléchargeables par l'URL du tag.

**Le binaire de test n'interroge pas le tag de sa propre version**, mais le
canal : `https://github.com/kevsi/tatami/releases/download/canal-rc/latest.json`.
Le canal est une release à tag fixe qui ne porte qu'un fichier, `latest.json`,
réécrit à chaque build. Un endpoint figé sur son propre tag se comparerait à son
propre manifeste et se croirait à jour pour toujours — mesuré le 2026-10-25 sur
rc.4, qui annonçait `0.2.0-rc.4` et n'aurait jamais vu rc.5. L'endpoint du mode
stable reste celui de l'ADR 0020 (`releases/latest/download/latest.json`).

Le manifeste du canal n'est pas signé, et ce n'est pas ce qui protège : chaque
entrée porte la signature minisign de son installeur, vérifiée par le plugin
avec la clé publique compilée dans le binaire **avant** toute exécution. Un
manifeste altéré ne peut donc désigner qu'un installeur déjà signé par nous.

Les deux secrets de signature vivent dans l'environnement GitHub
`release-test` (Settings → Environments), pas au niveau du dépôt : une
prérelease de test ne doit pas pouvoir signer avec la clé de production. Le
job échoue en nommant le secret absent s'il manque.

Ce que le workflow prouve, dans l'ordre : les portes du dépôt (workflow
réutilisable `verifications.yml`) ; la concordance tag/versions ; la
construction du moteur ; le build signé (NSIS + MSI + `.sig`) ; `latest.json`
et `SHA256SUMS.txt` (`scripts/preparer-release.ps1`) ; la **correspondance
cryptographique** entre le `.sig` et l'installeur, par le test Rust
`signature_installeur` (même crate `minisign-verify` que le plugin) ; la
publication en brouillon puis en prérelease ; l'absence de service par
`releases/latest`.

La même préparation peut être rejouée à la main après un build local :

```powershell
.\scripts\preparer-release.ps1 -Version 0.1.0 -Tag v0.1.0 -Depot kevsi/tatami
$env:TATAMI_INSTALLEUR = (Resolve-Path "dist-release\Tatami_0.1.0_x64-setup.exe").Path
$env:TATAMI_INSTALLEUR_SIG = "$env:TATAMI_INSTALLEUR.sig"
$env:TATAMI_MANIFESTE = (Resolve-Path "dist-release\latest.json").Path
$env:TATAMI_CLE_PUBLIQUE = (Get-Content apps\desktop\src-tauri\tauri.test.conf.json -Raw |
  ConvertFrom-Json).plugins.updater.pubkey
cargo test --manifest-path apps\desktop\src-tauri\Cargo.toml `
  --test signature_installeur -- --ignored --nocapture
```

Limite connue : sur un dépôt **privé**, l'application ne peut pas télécharger
un asset de release sans authentification (`404` anonyme). Le test de bout en
bout de M5 se fait donc avec le binaire de test et un manifeste servi
localement, ou après passage du dépôt en public (décision utilisateur, D3).

Les fichiers de workflow se vérifient avec `actionlint` (voir `AGENTS.md` §8) :
une clé mal placée y rend le fichier invalide, et GitHub ne crée alors **aucun
job** — le run échoue en 0 s sans message lisible.

### 7. Sauvegarde de la bibliothèque

Deux boutons dans **Paramètres → Sauvegarde de la bibliothèque** :
`Exporter…` écrit un fichier JSON, `Importer…` le fusionne dans la base
courante. Les deux dialogues sont ouverts **par Rust** (`backup.rs`), donc aucun
chemin ne traverse l'IPC — même raison que l'import d'une source personnalisée.

Ce qui voyage : catégories, favoris, historique, signets. Ce qui ne voyage
**pas**, et c'est un choix : les téléchargements (des `.cbz` sur le disque, pas
des lignes), les réglages (`dossier_telechargements` pointe vers un chemin de
**cette** machine) et les scripts de sources personnalisées (des fichiers `.js`).

Le format porte `version: 1` (`store::VERSION_SAUVEGARDE`) et `restaurer`
**refuse** une version qu'il ne connaît pas — jamais « au mieux ». Les favoris
référencent leur catégorie **par nom**, parce que deux bases n'ont jamais les
mêmes `category_id`.

L'import ne supprime rien et ne remonte jamais dans le temps :

| Donnée | Règle de fusion |
|---|---|
| Favori | date d'ajout la plus **ancienne** (`MIN`), classification connue non écrasée par `UNKNOWN`, couverture gardée si le fichier n'en a pas |
| Historique | `read_at` le plus récent gagne ; une ligne du fichier plus ancienne est ignorée |
| Signet | `INSERT OR IGNORE` sur la clé primaire (source, manga, chapitre, page) |
| Catégorie | recréée par nom si absente ; couleur et place locales conservées sinon |

Les comptes affichés à l'import sont des **deltas** : restaurer deux fois la
même sauvegarde rend zéro la seconde fois. C'est `Store::restaurer` qui les
calcule, pas une prédiction en Rust — la condition `WHERE` de chaque
`DO UPDATE` répète les expressions du `SET`, donc c'est la même formule qui
décide du changement et qui l'écrit.

Preuves : `cargo test` (9 tests de sauvegarde, dont
`restaurer_ne_remonte_jamais_l_historique`, prouvé rouge en retirant la garde
`WHERE`) ; côté écran, T08.6 de [`smoke-test.md`](smoke-test.md).

## Documentation

| Document | Contenu |
|---|---|
| [`AGENTS.md`](../AGENTS.md) | règles de travail, architecture, sécurité, leçons de session |
| [`PLAN.md`](../PLAN.md) | phases, critères de sortie, questions ouvertes, journal |
| [`docs/smoke-test.md`](smoke-test.md) | test de fumée manuel, exécuté avant chaque jalon |
| [`docs/notes/loader-context.md`](notes/loader-context.md) | ce qu'exige `MangaLoaderContext` sur desktop |
| [`contract/README.md`](../contract/README.md) | les 10 méthodes du contrat et les 7 codes d'erreur |
| [`docs/decisions/`](decisions) | ADR : transport du sidecar, schéma SQLite, protocole d'images, permissions Tauri |
| [`docs/rapport-session-docs.md`](rapport-session-docs.md) | dernier rapport de session documentation |
