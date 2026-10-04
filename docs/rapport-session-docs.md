# Rapport de session — documentation

Date : 2026-10-04. Périmètre strict : `docs/` et `README.md`.
Aucune commande de build, de dev server, de Gradle, de cargo ou de git n'a été lancée (autre
session propriétaire des ports 3000/9223, du dossier `target` et de l'historique). Tout ce qui
est écrit ci-dessous a été **lu** dans le dépôt, avec le fichier et la ligne.

## Ce qui a été écrit

| Fichier | Statut | Contenu |
|---|---|---|
| `docs/smoke-test.md` | **créé** (~330 lignes) | Test de fumée manuel exigé par `AGENTS.md` §8, niveau 4. 11 blocs (T01→T11) + un bloc conditionnel T09. Chaque étape dit quoi cliquer, quoi vérifier, et **quelle preuve attendre** (valeur relevée, capture, sortie de log, fichier sur disque). |
| `docs/decisions/0002-schema-sqlite-migrations.md` | **créé** | SQLite + `PRAGMA user_version`, migrations forward-only, sauvegarde avant migration, colonnes ajoutées par présence, repli en mémoire. Schéma v5 documenté table par table. |
| `docs/decisions/0003-images-protocole-tatami.md` | **créé** | Protocole `tatami://` : jeton opaque, table de résolution en mémoire, cache disque `%APPDATA%\Tatami\cache`, `404` sur jeton inconnu, CSP `img-src`. |
| `docs/decisions/0004-permissions-tauri.md` | **créé** | Aucun plugin Tauri, une seule capability `core:event:default` limitée à `main`, sidecar lancé en Rust (donc pas de `shell`), fenêtre de vérification créée en Rust et donc sans permission, CSP sans `https:` dans `connect-src`. |
| `README.md` | **réécrit** | Présentation, état réel du projet, architecture, dépendances amont épinglées, construire/lancer/vérifier, chemins de données, licence GPLv3, crédits Kotatsu / Kotatsu-Redo / kotatsu-parsers-redo, table documentaire. |

### Points de méthode

- **Format ADR** : celui de `AGENTS.md` §9, à l'identique (`# NNNN — Titre`, `- Date :`,
  `- Statut :`, `## Contexte`, `## Décision`, `## Alternatives étudiées`, `## Conséquences`).
  Le statut est `acceptée` avec la mention « consignée après coup » : le code applique déjà ces
  décisions, il ne s'agit donc pas d'un nouveau choix à valider.
- **`docs/decisions/0001-transport-sidecar.md` n'a pas été touché** (voir « Ce qui n'a pas été
  fait »).
- **Aucune affirmation sans preuve** : chaque énoncé de l'interface porte son
  `page.tsx:<ligne>`, chaque énoncé Rust son `store.rs:` / `main.rs:` / `pagecache.rs:` /
  `challenge.rs:` / `download.rs:` / `commands.rs:` / `supervisor.rs:`, chaque fait mesuré son
  entrée de journal `PLAN.md`.
- **Ce qui n'est pas fait est écrit dans les docs** : `docs/smoke-test.md` §13 liste 10 limites
  attendues (images 403 sur REMANGA/SEIMANGA, sources mortes, EXHENTAI non paginé, pas de
  lecture hors ligne, pas de catégories…), §14 liste ce que le test ne couvre pas (phases 3, 4, 5).

## Ce qui a été lu pour le faire

`AGENTS.md` (259 lignes), `PLAN.md` (382 lignes), `README.md` avant réécriture, `LICENSE`,
`.gitignore`, `package.json` racine et `apps/desktop/package.json`, `pnpm-workspace.yaml`,
`scripts/build-engine.ps1`, `contract/protocol.json`, `contract/README.md`,
`docs/decisions/0001-transport-sidecar.md`, `docs/notes/loader-context.md`,
`apps/desktop/next.config.mjs`, `apps/desktop/eslint.config.mjs` (non lu en détail),
`apps/desktop/src-tauri/tauri.conf.json`, `capabilities/default.json`, `Cargo.toml`, `Cargo.lock`
(vérification de `tauri = 2.12.1`), et dans `src-tauri/src/` : `main.rs` (complet),
`pagecache.rs` (complet), `challenge.rs` (complet), `commands.rs` (les commandes),
`store.rs` (lignes 1-300, 876-915, 1172-1201), `download.rs` (lignes 30-109, 258-317),
`library.rs` (signatures), `supervisor.rs` (en-tête, constantes, `dev_sidecar_paths`),
`cbz.rs` (signatures). Côté interface : `apps/desktop/app/page.tsx` par zones (constantes
1-215, barre d'application, `describeError`, clavier, réglages, téléchargements, lecteur, fiche,
source, accueil, catalogues), `apps/desktop/app/layout.tsx` non lu.
Côté moteur : `engine/build.gradle.kts`, `engine/src/main/kotlin/org/tatami/engine/Main.kt`,
`engine/src/main/kotlin/org/tatami/engine/Server.kt` (dispatch des méthodes).

## Ce qui n'a pas pu être fait

1. **`docs/decisions/0001-transport-sidecar.md` (protocole NDJSON) n'a pas été réécrit.**
   La mission listait 4 ADR ; l'ADR du protocole NDJSON existe déjà, statut `acceptée`, daté du
   2026-10-02, et il est **conforme** à `AGENTS.md` §5 et §9. Le réécrire aurait créé deux
   enregistrements d'une même décision. Le code lui correspond : `supervisor.rs:1` référence
   l'ADR 0001, `stdout` est bien réservé au protocole (`supervisor.rs:8`), la poignée de main
   `hello` refuse un MAJEUR différent (`supervisor.rs:21,212-224`).
2. **Les ADR n'ont pas de statut « à valider » formel.** `AGENTS.md` §2.9 veut l'humain consulté
   pour toute décision d'architecture. Ici il s'agit de décisions **déjà appliquées et
   vérifiées** ; la validation porte sur la documentation, pas sur un choix technique nouveau.
   Aucune nouvelle décision n'a été prise : chaque ADR décrit ce que le code fait déjà.
3. **`docs/spike-phase0.md` n'a pas été écrit.** Il fait partie de la tâche `PLAN.md` **0.12**,
   dont le périmètre est « mesures + smoke test + figer les commandes de vérification ». Il
   faudrait les mesures de la phase 0.7 (runtime 64,1 Mo, démarrage 1 576 ms, RAM), qui
   n'existent que dans le journal de `PLAN.md` : les recopier serait un travail de copie, pas une
   mesure, et 0.12 demande des mesures à jour. **De plus, cette session n'a pas le droit de
   cocher 0.12 ni d'ajouter une ligne de journal** (contrainte de périmètre : ni `PLAN.md` ni
   `AGENTS.md`).
4. **Les « commandes de vérification » d'`AGENTS.md` §8 n'ont pas été figées/mises à jour.**
   C'est explicitement la tâche 0.12 et ça demande de modifier `AGENTS.md` — interdit ici.
   Le README documente en revanche les commandes réellement utilisées, avec leur forme exacte et
   leurs pièges (`--manifest-path`, ESLint 9 épinglé).
5. **Aucun test de fumée n'a été exécuté.** Interdit (ports et RAM occupés par l'autre session).
   `docs/smoke-test.md` est donc **un protocole à exécuter, pas un résultat**. La section §12
   fournit le gabarit de rapport à coller, et §13 la liste des échecs attendus pour ne pas
   les confondre avec des bugs. Chaque étape est appuyée sur le code, pas sur une exécution.
6. **Rien n'est commité.** Interdit : une autre session est seule propriétaire de l'historique et
   un `git add -A` aurait mêlé les fichiers. **Les 5 fichiers ci-dessus sont non commités** ; la
   session propriétaire de git devra les ajouter (`docs/smoke-test.md`,
   `docs/decisions/0002-*`, `0003-*`, `0004-*`, `docs/rapport-session-docs.md`) et `README.md`.
   Un commit `docs: ajoute le smoke test, les ADR et réécrit le README` conviendrait.
7. **`docs/notes/` non touché.** `loader-context.md` a 2 points `À VÉRIFIER` encore ouverts
   (`nashorn-core` jamais exercé par un parser à JS, sources utilisant
   `interceptWebViewRequests` non inventoriées) : ce sont des tâches de `PLAN.md`, pas de la doc.

## Défauts trouvés en documente (signalés, **pas corrigés** — code hors périmètre)

1. **`contract/protocol.json` est en retard sur le moteur.** L'enum `method` liste 10 méthodes
   (`contract/protocol.json:14-25`) et `contract/README.md` aussi, mais le moteur en implémente
   **12** : `latest` et `filterOptions` sont dans le dispatch (`Server.kt:290,301`) et sont
   appelées par les commandes Rust `commands::latest` et `commands::filter_options`
   (`main.rs:97-98`). Une validation contre le schéma rejetterait donc des réponses légitimes.
   *Déjà signalé* dans le journal `PLAN.md` du 2026-10-02 (« Sources vides / sources
   bloquées »), toujours pas corrigé. **Correctif** : ajouter `latest` et `filterOptions` à
   l'enum et au tableau de `contract/README.md`. Non fait ici (`contract/` hors périmètre).
2. **Un message d'erreur contredit le code.** `describeError` renvoie « <source> demande une
   vérification anti-robot, **pas encore gérée** » pour `CHALLENGE_REQUIRED`
   (`page.tsx:403`), alors que la gestion existe : bouton « Résoudre la vérification » et fenêtre
   de résolution (`page.tsx:2566-2592`, `challenge.rs`). C'est une chaîne de la phase 0.2 jamais
   mise à jour après 0.11. **Signalé dans `docs/smoke-test.md` §13** pour que le smoke test
   ne le rapporte pas comme un nouveau bug.
3. **Cache d'images non borné.** `%APPDATA%\Tatami\cache` accumule un fichier par page vue, sans
   quota ni expiration (`pagecache.rs:24-31,63-65`). Documenté comme conséquence dans l'ADR
   0003, avec le rendez-vous `PLAN.md` 2.5.
4. **`page.url` reste transmis à l'interface** alors que seul le `token` est utilisé
   (`commands.rs:240-248`). Ce n'est pas une faille — la résolution passe par la table de
   jetons (`pagecache.rs:55-61`) — mais le contrat expose une donnée inutile. Documenté
   honnêtement dans l'ADR 0003, point 1.
5. **`connect-src` interdit `https:`, mais `img-src` l'autorise** à cause des couvertures
   (`tauri.conf.json:21`). Asymétrie assumée, documentée dans l'ADR 0003 et rappelée en
   surveillance dans l'ADR 0004.

## Vérifications faites sur mes propres fichiers

- Chaque étape du smoke test renvoie à un `page.tsx:<ligne>` relu dans cette session.
- Les 28 commandes Tauri de l'ADR 0004 ont été recomptées à partir de `main.rs:91-121`.
- La taille du runtime (64,1 Mo), le démarrage (1 576 ms) et la RAM viennent de l'entrée de
  journal `PLAN.md` du 2026-10-02, tâche 0.7 — ils ne sont pas re-mesurés ici.
- Aucune commande n'a été exécutée : ce rapport décrit **ce qui a été écrit et lu**, pas un
  résultat d'exécution.
