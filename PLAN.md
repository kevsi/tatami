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
| 0 | Fondations et spike des risques | M | À faire |
| 1 | Bibliothèque et lecteur | L | À faire |
| 2 | Hors ligne | M | À faire |
| 3 | Tâches de fond et notifications | M | À faire |
| 4 | Comptes et synchronisation | L | À faire |
| 5 | Durcissement et distribution | M | À faire |

Phase en cours : **0**. Prochaine tâche : **0.9**.

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
- [ ] **0.12** Écrire `docs/spike-phase0.md` : mesures (taille, démarrage, RAM), ce qui marche, ce qui bloque, décisions, ajustements proposés au plan. Créer `docs/smoke-test.md`. Figer et mettre à jour les commandes de vérification dans `AGENTS.md`.

**Critère de sortie** : lire un chapitre complet d'une source ouverte dans l'interface Tauri avec le moteur empaqueté, et un résultat documenté (succès ou limites claires) pour une source protégée.

### Phase 1 — Bibliothèque et lecteur (L)

Objectif : pouvoir remplacer le mobile pour lire en ligne.

- [ ] **1.1** SQLite côté Rust, système de migrations forward-only, sauvegarde avant migration, tests de migration.
- [ ] **1.2** Catalogue de sources : liste, activation/désactivation, langue, état (alimenté par `health`).
- [ ] **1.3** Recherche et filtres (nom, genres, tri) avec pagination.
- [ ] **1.4** Page détail d'un manga : infos, liste des chapitres, ajout aux favoris.
- [ ] **1.5** Favoris organisés en catégories définies par l'utilisateur.
- [ ] **1.6** Historique de lecture, reprise au bon chapitre et à la bonne page, mode incognito.
- [ ] **1.7** Signets.
- [ ] **1.8** Lecteur paginé : sens de lecture, zoom, préchargement via le cache Rust, raccourcis clavier et souris.
- [ ] **1.9** Mode webtoon (défilement continu, liste virtualisée).
- [ ] **1.10** Thèmes clair/sombre, i18n français et anglais en place.

**Critère de sortie** : recherche → ajout en favori → lecture → fermeture → reprise exacte à la réouverture, sur au moins 3 sources différentes, avec tests passants.

### Phase 2 — Hors ligne (M)

- [ ] **2.1** Gestionnaire de téléchargements : file d'attente, reprise après interruption, concurrence limitée, débit limité par source.
- [ ] **2.2** Stockage CBZ ou dossier, chemin en base, noms de fichiers assainis.
- [ ] **2.3** Lecture hors ligne des chapitres téléchargés.
- [ ] **2.4** Import de CBZ tiers et lecture locale ; export d'un chapitre en CBZ.
- [ ] **2.5** Gestion de l'espace : taille utilisée, suppression, emplacement configurable.

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

## 9. Journal

_Format d'une entrée :_

```
### AAAA-MM-JJ — Tâche X.Y
- Plan : (3 à 6 lignes)
- Fait :
- Preuve : commande + résumé de la sortie
- Points ouverts :
```

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