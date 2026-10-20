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
