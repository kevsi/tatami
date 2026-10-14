# AGENTS.md — Directives pour l'agent de développement

> Projet : **Tatami** (nom provisoire), client desktop de lecture de manga avec parité fonctionnelle avec l'app mobile Kotatsu-Redo.
> Ce fichier est lu en premier à chaque session. La feuille de route vivante est dans `PLAN.md`.

---

## 1. Mission

Construire un client desktop (Windows d'abord, puis Linux/macOS) qui permet de faire sur PC tout ce que fait Kotatsu-Redo sur Android : sources, recherche, bibliothèque, lecteur, téléchargements, notifications, suivi, synchronisation.

Stack figée :

| Couche | Techno | Rôle |
|---|---|---|
| Interface | Next.js (export statique) + TypeScript, dans la webview Tauri | Affichage uniquement |
| Cœur | Rust, Tauri v2 | Données, téléchargements, tâches de fond, sync, suivi, supervision du moteur |
| Moteur de sources | Kotlin/JVM, sidecar | Charge `kotatsu-parsers-redo`, fait les requêtes vers les sites |

Licence : **GPLv3** (imposée par les parsers). Tout le code du dépôt est sous GPLv3. Conserver les en-têtes et créditer Kotatsu, Kotatsu-Redo et kotatsu-parsers-redo dans le README.

---

## 2. Règles d'or (non négociables)

1. **Lire `PLAN.md` avant toute action.** Travailler sur une seule tâche à la fois, dans l'ordre des phases. Ne pas commencer une phase tant que le critère de sortie de la précédente n'est pas prouvé.
2. **Ne jamais inventer.** Pour toute API de bibliothèque (Tauri v2, plugins, parsers, Gradle, jlink), lire la documentation officielle ou le code source cloné. Si tu n'as pas vérifié, écris `À VÉRIFIER` dans le code ou la doc et ajoute une tâche dans `PLAN.md`. Un nom de méthode, une coordonnée Maven ou un champ de config "qui a l'air plausible" est une erreur.
3. **Aucune affirmation sans preuve.** "Ça marche", "les tests passent", "c'est corrigé" doivent s'appuyer sur une commande réellement exécutée. Colle la commande et un résumé de la sortie dans le journal de `PLAN.md`.
4. **Petits changements atomiques.** Un commit = une intention. Messages au format Conventional Commits (`feat:`, `fix:`, `refactor:`, `docs:`, `test:`, `chore:`), périmètre optionnel (`feat(engine): ...`).
5. **Ne jamais élargir le périmètre.** Ce qui est hors v1 (voir `PLAN.md`) ne se fait pas sans validation humaine explicite.
6. **Aucun secret dans le dépôt.** Les clés OAuth (AniList, MAL, Shikimori, Kitsu) vont dans des variables d'environnement ou un fichier `.env` ignoré par git. Les jetons utilisateur vont dans le trousseau de l'OS, jamais en clair dans SQLite.
7. **Aucun contenu protégé dans le dépôt.** Pas de pages de manga, de couvertures ni de captures d'œuvres en fixtures. Seules des métadonnées JSON minimales (titres, identifiants, URLs fictives ou anonymisées) sont autorisées.
8. **Ne jamais modifier une migration SQL déjà appliquée.** On en ajoute une nouvelle.
9. **Demander à l'humain** pour toute décision d'architecture, de licence, de sécurité ou de dépendance majeure. Proposer 2 options avec leurs compromis, ne pas trancher seul.

---

## 3. Environnement de développement

La machine de l'humain est un portable Windows 10 (i5-6200U, 12 Go de RAM) avec WSL2/Fedora. Adapte-toi à ces contraintes.

- **Shell** : PowerShell pour toutes les tâches Windows. Pas de commandes bash ou `curl` Unix pour les tâches Windows (utiliser `Invoke-WebRequest` si besoin).
- **Gestion de paquets** : Scoop (winget ne fonctionne pas sur cette machine). Node via nvm4w. Gestionnaire JS : pnpm.
- **Mémoire limitée** : ne lance pas simultanément `next dev`, Gradle, `cargo build` et rust-analyzer lourds. Dans `gradle.properties` : `org.gradle.jvmargs=-Xmx1g`, `org.gradle.workers.max=2`. Arrête le démon avec `./gradlew --stop` après un gros build. Pour Rust, limite les jobs si nécessaire : `cargo build -j 2`.
- **Outils attendus** (à installer s'ils manquent) :

```powershell
scoop bucket add java
scoop install temurin21-jdk gradle
# Rust, Node (nvm4w) et pnpm : vérifier d'abord avec les commandes de version
java -version; gradle -v; rustc --version; node -v; pnpm -v
```

Consigne le résultat de ces vérifications dans le journal de `PLAN.md` (phase 0).

---

## 4. Architecture et règles de dépendance

```
Interface (Next.js statique)
        │  invoke() / événements Tauri uniquement
        ▼
Cœur Rust (Tauri)  ── SQLite, téléchargements, planificateur, sync, suivi
        │  contrat versionné (stdio)
        ▼
Moteur JVM (sidecar) ── kotatsu-parsers-redo ──► sites de manga
```

Règles :

1. **L'interface ne parle qu'à Rust** (commandes et événements Tauri). Jamais directement au sidecar ni au réseau pour les sources.
2. **Rust est le seul client du sidecar.** Il le lance, le supervise, le relance en cas de crash, et lui parle via le contrat.
3. **Le sidecar est sans état persistant.** Seuls des caches jetables sont tolérés. Toute donnée utilisateur vit dans SQLite côté Rust.
4. **Les images passent par le moteur** (en-têtes, cookies et recomposition spécifiques aux sources), sont mises en cache sur disque par Rust, et sont servies à l'interface via un protocole d'URI personnalisé (`tatami://...`). Pas d'appel direct aux hôtes d'images depuis la webview.
5. **Le moteur est remplaçable indépendamment de l'app** (JAR mis à jour séparément du runtime JVM). Toute décision qui rendrait cela impossible doit être remontée à l'humain.

---

## 5. Contrat Rust ↔ moteur (proposition à valider en phase 0)

Le contrat vit dans `contract/` : un schéma JSON versionné, à partir duquel on génère les types Rust et TypeScript. Toute évolution du protocole est documentée dans une ADR (voir section 9).

**Transport proposé** : JSON ligne par ligne (NDJSON) sur stdin/stdout du sidecar. **stdout est réservé au protocole**, tous les logs vont sur stderr. Aucun port réseau ouvert. Si une alternative (HTTP local avec jeton) se révèle nécessaire, créer une ADR avant de changer.

**Poignée de main** : le premier échange est `hello`. Le moteur répond avec `protocolVersion`, `engineVersion` et la version de `kotatsu-parsers-redo` embarquée. Rust refuse de continuer si `protocolVersion` majeure est incompatible.

**Méthodes minimales (noms indicatifs)** : `hello`, `listSources`, `search`, `details`, `chapters`, `pages`, `fetchImage`, `setCookies`, `health`, `shutdown`.

**Erreurs typées** (champ `code` stable, `message` lisible) : `SOURCE_UNAVAILABLE`, `CHALLENGE_REQUIRED`, `PARSE_ERROR`, `RATE_LIMITED`, `TIMEOUT`, `CANCELLED`, `INTERNAL`. `CHALLENGE_REQUIRED` déclenche côté Rust l'ouverture de la fenêtre de résolution (captcha/Cloudflare).

**Règles** :
- Chaque requête a un identifiant, un timeout et peut être annulée.
- Pas de breaking change sans incrément de version majeure du protocole et ADR.
- Les réponses sont validées contre le schéma des deux côtés en mode debug.

**À VÉRIFIER avant d'implémenter** : ce que l'interface `MangaLoaderContext` de `kotatsu-parsers-redo` exige exactement (client HTTP, cookies, exécution de JavaScript, recomposition d'images, configuration par source), et comment l'implémentation non-Android de `kotatsu-dl` y répond. Résumer les conclusions dans `docs/notes/loader-context.md`.

---

## 6. Données

- **SQLite côté Rust** (bibliothèque, favoris, catégories, historique, signets, files de téléchargement, état de sync).
- **Migrations versionnées, sens unique (forward-only)**, jamais éditées après application. Sauvegarde automatique du fichier de base avant chaque migration.
- Tests de migration obligatoires : partir d'une base de la version N-1, migrer, vérifier les données.
- Pas d'IndexedDB pour les données métier. `localStorage` toléré pour de simples préférences d'affichage.
- Les téléchargements vivent sur le système de fichiers (CBZ ou dossier), le chemin est stocké en base. Assainir tous les noms de fichiers dérivés de données distantes (anti path traversal).

---

## 7. Conventions de code

**Rust**
- `cargo fmt` et `cargo clippy -- -D warnings` propres avant chaque commit.
- Erreurs typées (`thiserror`), pas de `unwrap()`/`expect()` dans le code de production sauf invariant documenté.
- Journalisation structurée avec `tracing`.
- Commandes Tauri fines : elles valident, appellent un service, retournent un type sérialisable. La logique vit dans des modules testables.

**TypeScript / Next.js**
- `strict: true`, pas de `any` sans commentaire justificatif.
- Aucun accès réseau direct aux sources. Tout passe par les commandes Tauri.
- Listes longues (chapitres, pages, bibliothèque) virtualisées.
- Textes via un système d'i18n dès le départ (français et anglais), aucune chaîne en dur dans les composants.

**Kotlin (moteur)**
- Coroutines, aucun appel bloquant sans dispatcher adapté.
- Aucune dépendance supplémentaire sans justification dans une ADR (la taille du runtime compte).
- Le moteur ne contient aucune logique métier d'application, seulement l'adaptation aux parsers.

**Tauri**
- Permissions et capabilities **au strict minimum** nécessaire. Toute extension se justifie dans l'ADR ou le commit.
- CSP stricte. Le protocole d'URI personnalisé n'accepte que des identifiants connus, jamais une URL arbitraire fournie par l'interface.

---

## 8. Tests et définition de "terminé"

**Pyramide de tests**
1. Unitaires (Rust, TS, Kotlin).
2. **Tests de contrat** : réponses enregistrées du moteur (métadonnées uniquement, pas d'images d'œuvres) rejouées contre le schéma et contre le code Rust.
3. Intégration : Rust lance le vrai sidecar et exécute un scénario `hello` → `listSources` → `search`.
4. Smoke test manuel documenté dans `docs/smoke-test.md`, exécuté avant chaque jalon.

Quatrième porte de l'interface, à lancer avec les trois autres :
`pnpm hooks:check` (`scripts/check-hooks.mjs`) refuse qu'un `useEffect`
dépende d'une valeur recréée à chaque rendu. Voir la leçon « `t` dans les
dépendances d'un effet » en §14.

Cinquième porte de l'interface : `pnpm chemins:check`
(`scripts/check-chemins.mjs`) refuse qu'un chemin soit affiché sans passer par
`cheminLisible(...)`, c'est-à-dire sans retrait de la forme longue `\\?\`.
Voir la leçon « la forme longue affichée telle quelle » en §14.

Sixième porte de l'interface : `pnpm pages:check`
(`scripts/check-pages-indisponibles.mjs`) refuse que le `<img>` du lecteur
n'ait pas de `onError`, donc qu'une page que la source ne sert pas
s'affiche en image cassée **sans message**. Voir la leçon « un 404 sans corps
n'est pas une panne » en §14.

Septième porte de l'interface : `pnpm perso:check`
(`scripts/check-tuile-perso.mjs`) refuse que les sources personnalisées
occupent toute la largeur (`grid-column: 1 / -1`) au lieu d'être une tuile
comme les 19 autres, et que le bouton « importer » reste dans la tuile — deux
boutons s'imbriqueraient, du HTML invalide.

**Huitième porte de l'interface : `pnpm ipc:check`**
(`scripts/check-type-ipc.mjs`) refuse qu'un type TypeScript déclaré pour la
charge utile d'une commande Tauri nomme des champs que le cœur ne sérialise
pas. Un `invoke<Type>` est une **affirmation** : `tsc` ne la vérifie pas. Mesuré
le 2026-10-09 — `Progression` en camelCase alors que `HistoryEntry` sort en
snake_case, donc ni marqueur de chapitre courant ni bouton « reprendre ». Les
paires type ↔ structure sont **déclarées** dans le script ; son pendant côté
Rust est `les_champs_de_history_entry_sont_en_snake_case`, qui fige les noms
**émis**. Les deux se lisent mutuellement.

**Neuvième porte : `pnpm moteur:check`** (`scripts/check-moteur-js.mjs`) refuse,
dans `docs/*.js`, une construction que le moteur de l'application refuse et que
Node accepte. Le sidecar évalue les sources avec **Nashorn 15.6**, pas avec V8 :
une virgule finale dans les arguments d'un appel est de l'ES2017, donc rejetée
par Nashorn et acceptée par le test hors ligne. Une source ainsi écrite est
rejetée à chaud par le loader, et l'application n'affiche rien.

**Commandes de vérification** (figées le 2026-10-05, tâche 0.12). Chacune
précise son dossier, parce que la plupart échouent depuis la racine :

```powershell
# Interface — dossier apps\desktop
cd apps\desktop
pnpm typecheck          # tsc --noEmit
pnpm lint               # eslint ; 4 warnings exhaustive-deps connus, 0 erreur
pnpm i18n:check         # aucune clé morte ni manquante (152 clés)
pnpm hooks:check        # aucune dépendance d'effet recréée à chaque rendu
pnpm chemins:check      # aucun chemin affiché sans cheminLisible(…)
pnpm pages:check        # le <img> du lecteur gère l'échec et le dit
pnpm perso:check        # les sources perso sont une tuile, pas un encart
pnpm ipc:check          # un type TS nomme les champs que le cœur sérialise
pnpm moteur:check       # un script de source perso est évaluable par Nashorn
pnpm test               # tests unitaires du front (node:test, lib/)
pnpm build              # next build

# Cœur Rust — dossier apps\desktop\src-tauri
cd apps\desktop\src-tauri
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test

# Moteur JVM — dossier engine
cd engine
gradle test             # PAS ./gradlew : il n'y a pas de gradlew.bat dans le dépôt.
                       # Vérifié le 2026-10-05 : `Get-ChildItem gradlew*` ne renvoie rien,
                       # donc la commande écrite ici avant le 2026-10-05 ne pouvait pas tourner.
```

**Le dossier compte.** Depuis `apps\desktop`, `cargo` ne trouve pas le manifeste et
`gradle` ne trouve pas le moteur ; depuis la racine, `pnpm` ne trouve pas
`package.json`. Les chemins ci-dessus sont donc la référence, pas une commodité.

Le smoke test manuel est `docs/smoke-test.md` ; les mesures de démarrage et de
mémoire du moteur sont `docs/spike-phase0.md`.

**Une tâche est terminée quand** :
- le code est écrit **et** les commandes de vérification passent (preuve dans le journal) ;
- les tests nécessaires sont ajoutés ;
- la documentation concernée est à jour (README, `docs/`, ce fichier si une règle change) ;
- la case est cochée dans `PLAN.md` avec une ligne de journal datée ;
- le commit est fait.

---

## 9. Décisions d'architecture (ADR)

Toute décision structurante est consignée dans `docs/decisions/NNNN-titre-court.md` :

```markdown
# NNNN — Titre
- Date :
- Statut : proposée | acceptée | remplacée par NNNN
## Contexte
## Décision
## Alternatives étudiées
## Conséquences
```

Sont concernées : protocole du sidecar, schéma de base de données, stockage des téléchargements, stratégie de mise à jour du moteur, ajout d'une dépendance majeure, changement de permissions Tauri.

---

## 10. Sécurité

- Le moteur et l'app communiquent sans port ouvert (stdio).
- Les mises à jour du moteur sont téléchargées depuis un manifeste **signé** ; l'app vérifie la signature et le hash, remplace de façon atomique, et revient en arrière si le démarrage échoue. Ne jamais exécuter un JAR non vérifié.
- Les jetons de suivi (OAuth) sont stockés dans le trousseau de l'OS.
- Valider toute entrée venant de l'interface ou du réseau (identifiants, URLs, noms de fichiers).
- Limiter le débit par source pour éviter les blocages et rester un client poli.
- Ne jamais désactiver la validation TLS, même "temporairement".
- Aucune exécution de JavaScript arbitraire en dehors du mécanisme explicitement requis par les parsers, et uniquement dans un contexte isolé.

---

## 11. Boucle de travail de l'agent

À chaque session :

1. Lire `AGENTS.md`, puis `PLAN.md` (statut, prochaine tâche, blocages, journal récent).
2. Choisir la **première tâche non cochée** de la phase en cours.
3. Écrire un mini-plan de 3 à 6 lignes dans le journal avant de coder.
4. Implémenter, vérifier (section 8), commiter.
5. Cocher la tâche et ajouter une entrée de journal : date, tâche, ce qui a été fait, preuve (commande + résumé), points ouverts.
6. Si une information manque (`À VÉRIFIER`), créer la tâche de vérification correspondante au lieu de deviner.

**Quand tu es bloqué**
- Après 3 tentatives infructueuses avec la même approche, change d'approche ou documente le blocage dans la section "Blocages" de `PLAN.md` et passe à une tâche indépendante.
- Si le blocage touche une décision d'architecture, de licence ou de sécurité, pose la question à l'humain avec deux options.

**Fin de session** : le dépôt doit être dans un état qui compile, avec `PLAN.md` à jour. Ne jamais laisser du travail non commité sans le noter dans le journal.

---

## 12. Maintenance de ce fichier et de `PLAN.md`

- Tu peux **cocher** des tâches, **ajouter** des entrées de journal, **ajouter** des tâches de vérification ou de dette, et **mettre à jour** les commandes de vérification.
- Tu ne peux **pas**, sans validation humaine : changer le périmètre de la v1, supprimer ou réordonner des phases, modifier les règles d'or, supprimer l'historique du journal.
- Quand une règle de ce fichier se révèle fausse ou inapplicable, signale-le dans "Blocages" avec une proposition de correction plutôt que de l'ignorer en silence.

---

## 13. Interdits

- Réécrire les 1200 sources en Rust ou TypeScript. Le moteur JVM est la source de vérité.
- Ajouter une fonctionnalité hors v1.
- Committer un binaire généré, un runtime JVM ou un JAR dans le dépôt (ils sont produits par les scripts de build).
- Contourner les permissions Tauri ou la CSP pour "faire passer" une fonctionnalité.
- Affirmer qu'une chose fonctionne sans l'avoir exécutée.
- Copier du code d'un autre projet sans vérifier sa licence et sans l'attribuer.

## 14. Leçons de session

Faits mesurés qui ont coûté du temps. À relire avant d'écrire un script de sonde ou du CSS de lecteur.

- **La doc d'un plugin dit ce qu'il expose, pas ce que fait le code.** Le
  plugin `notification` de Tauri v2 expose `sendNotification`, et sa doc montre
  `isPermissionGranted → requestPermission → sendNotification`. Sur desktop,
  `sendNotification` ne fait que `new window.Notification(...)` : **aucun IPC**.
  L'IPC n'est atteint que par la *requête* de permission, et seulement quand
  `window.Notification.permission === 'default'`. Conséquence appliquée : j'ai
  retiré `notification:allow-notify` de la capability, mobile étant hors v1.
  Règle : avant d'ajouter une permission, **lire le code du chunk** — pas seulement
  la liste des commandes exposées.
- **`tauri-build` valide les permissions, et ça se prouve.** Injecter
  `notification:allow-notify-FAUX` fait échouer le build en nommant
  `Permission … not found` et la liste des valides. Un build vert prouve donc que
  les identifiants existent, pas seulement que ça compile. On peut aussi lire le
  `capabilities.json` **généré** dans `target/…/out/` : c'est le jeu effectif,
  horodaté.
- **Une capability n'autorise pas une commande absente.** `sendNotification`
  peut appeler `plugin:notification|notify` sans que le plugin soit enregistré :
  sans `.plugin(tauri_plugin_notification::init())`, la commande n'existe pas et
  la capability est inutile.
- **Composer deux phrases avec `·` casse l'une des deux langues.** « 12 nouveaux
  chapitres » + « ou plus » donnait « 12 nouveaux chapitres · ou plus » : correct
  en anglais, agrammatical en français. Un fragment qui **se tient seul** est la
  seule façon de composer sans connaître la grammaire de l'autre ; un pluriel
  imbriqué dans une clé unique ment toujours sur l'une des deux.
- **Une porte qui ne lit qu'un seul fichier a un angle mort.** `check-i18n.mjs`
  ne lisait que `page.tsx` : dès qu'un écran vit dans un module, ses clés sont
  déclarées mortes. Elle lit maintenant **tous** les modules de `app/` — sauf le
  dictionnaire, sinon toute clé y serait comptée comme utilisée et la porte ne
  crierait plus jamais.
- **Un décompte dépend de la clé choisie.** Une passe peut mentionner la même
  série deux fois ; compter les entrées aurait annoncé « 6 séries » pour 5.
  Compter un `Set` de `(source, url)`.


- **Un dialogue natif n'est pas shimable : `window.__TAURI_INTERNALS__` et sa
  propriété `invoke` sont gelés.** Mesuré : `writable: false, configurable:
  false` sur les deux. Une assignation `invoke = …` échoue **en silence** (le
  code injecté n'est pas strict), donc la sonde croit shimer, le vrai dialogue
  natif s'ouvre, et la commande répond un message **vrai mais sur le mauvais
  dossier** — « aucun fichier .cbz » alors qu'il y en a quatre. Deux fois de
  suite j'ai conclu à un bug d'import. Contournement qui marche : intercepter
  le **chunk** qui contient le plugin (`page.route` + insérer une garde en tête
  de `open`) ; tout le reste du chemin reste réel.
- **Une globale posée avant un rechargement est effacée par celui-ci.** La même
  famille : la garde tombait sur `undefined`, donc le dialogue natif s'ouvrait
  et `open()` ne résolvait jamais — le bouton restait bloqué sur « Importing… ».
  Poser les globales **après** le `reload`.
- **Un `assert` rouge sur un cas limite peut être la règle qui est fausse.**
  J'ai écrit un test attendant le refus d'un nom de fichier, alors que
  `chapter_file_name(titre, "")` retombe **toujours** sur `sanitize(titre).cbz`
  (le repli d'URL appelle `sanitize("")` → `sans-titre`) : l'aller-retour tient
  toujours. Le test attendait une garantie inexistante.
- **Les fixtures hostiles doivent être des noms que la plateforme accepte de
  créer.** `Ch. 1 : la suite.cbz` (`:` interdit) et `CON.cbz` (nom réservé)
  échouent **à la création**, donc avant le test — et le message parle de
  synchronisation, pas du comportement étudié. Restent créables : points de
  tête, points finaux, espaces de bord, plafond de 80 caractères de `sanitize`.
- **Une porte dont une regex n'accepte pas un caractère a un trou
  symétrique.** `check-i18n.mjs` refusait le `-`, donc une clé kebab-case
  disparaissait du dictionnaire **analysé** par la porte : ni déclarée morte, ni
  traduite. Le tester suppose que ses deux moitiés voient les mêmes clés.
- **Une porte qui ne peut pas crier est indiscernable d'une porte verte.**
  La règle « `{placeholder}` sans valeurs » a d'abord passé : le drapeau `m`
  manquait, donc `^` s'ancrait au début du fichier et la table des placeholders
  restait **vide**. Prouver une porte, c'est la faire échouer et **lire le nom
  de ce qu'elle accuse** — pas constater qu'elle est verte.
- **Une correction peut être plus fausse que le bug.** Après « `1/1 chapitres` »,
  j'ai écrit `t('…compte1')` **sans ses paramètres** : l'écran affichait
  littéralement `{n}/{total} chapitre`, et `tsc` comme `i18n:check` ne voyaient
  rien. Après une correction, relire ce qu'elle affiche vraiment.
- **`\\?\` est un chemin Windows valide et illisible.** `canonicalize` le renvoie
  — c'est ce qui débloque les chemins longs — mais l'afficher est du bruit.
  Le retirer à l'affichage seulement, jamais en interne.
- **`lire_entrees` refuse un ZIP sans entrée**, donc `pages_de` ne rend jamais
  une liste vide. Un « `.cbz` sans image » se fabrique avec une archive dont
  l'unique entrée est un `.txt`.
- **Un chemin d'origine et un chemin canonisé ne sont pas la même chaîne** :
  `\\?\C:\…` contre `C:\…`, même fichier. Comparer les deux `canonicalize()`.
- **Un dossier de téléchargement réglé sur le dépôt y pollue le dépôt.** Mes
  fixtures `temp_dir()` écrivent sous `downloads_root()` : vérifier `git status`
  après une série de tests qui touchent le disque.

- **Un `Tauri::command` qui prend `tauri::State` n'est pas appelable depuis
  `tests/`.** Prouver une chaîne de lecture qui finit sur une commande Tauri
  impose d'éprouver la **fonction** que la commande appelle, pas la commande.
  Ici `chapter_file_for` est la partie à risque (le décalage de chemin), et le
  reste de la chaîne est déjà couvert par les tests de 2.3.

- **`TrayIconBuilder::icon()` ne fait rien si on ne lui donne pas d'image** :
  le tray existe alors, sans rien de cliquable. Une icône de zone qui dépend de
  la webview n'est donc pas une question de style : avec « fermer cache la
  fenêtre », elle rend l'application **injoignable**. D'où un verrou en
  fonction pure, testé sur ses quatre cas, et une porte qu'on casse pour la
  prouver. Une commande qui renvoie `Ok` ne prouve rien : `set_libelles`
  renvoie les textes **relus depuis le menu**, ce qui évite en prime d'ajouter
  `core:menu:allow-get-text` aux capabilities — ce qu'interdit « strict
  minimum ».
- **Une sonde refusée par le moteur est une preuve.** `tray.get_by_id`,
  `menu.get`, `window.is_visible` : trois « not allowed ». Ce refus dit que la
  discipline tient, et oblige à trouver une autre voie plutôt qu'à élargir les
  droits pour pouvoir mesurer.
- **Lire la langue au mauvais endroit fait conclure à un bug inexistant.** Une
  sonde lisait `document.documentElement.lang` — exactement l'attribut que le
  script bloquant de `layout.tsx` laisse à `fr`. Elle envoyait donc toujours du
  français et concluait que l'anglais ne passait pas. Le défaut était dans la
  mesure.

- **Un fichier de vérification Gradle généré sans `--refresh-dependencies` passe
  en local et casse la CI.** Mesuré le 2026-10-05 : le fichier couvrait 73
  composants, `gradle clean build` passait… et `--refresh-dependencies` échouait
  sur `kotlinx-coroutines-bom-1.8.0.pom`, que le cache local n’avait jamais
  demandé au dépôt du plugin. La CI fait un checkout neuf, donc elle prend le
  cas froid. Règle : générer **et** prouver avec `--refresh-dependencies`, sinon
  on livre une porte qui ne marche que chez soi.
- **Une porte qui mord se prouve sur l’artefact qu’elle prétend protéger.** Altérer
  **un seul caractère** du sha256 du JAR de parsers fait échouer la construction en
  nommant `kotatsu-parsers-redo-b3c0000.jar`. On ne se contente pas de voir « FAILED »
  : c’est le **nom** dans le message qui prouve que la porte vise la bonne chose.
- **Une tâche peut avoir une prémisse fausse : mesurer la chaîne avant d’implémenter
  la parade.** 5.7 prévoyait un « miroir/fork de secours » et son tableau des
  risques disait « amont archivé ». Mesure : le dépôt archivé (`KotatsuApp/kotatsu-parsers`)
  est le **parent**, et celui qu’on consomme (`Kotatsu-Redo/…`) est **vivant**. Le
  miroir existait déjà, fourni par un tiers, et le pin était la tête de `master`.
  Corollaire : lire la chaîne de dépendance réelle avant de construire une stratégie
  contre elle — sinon on défend le bon endroit avec le mauvais moyen.

- **Une fonction de contexte dans les dépendances d’un `useEffect` déclenche
  l’effet à chaque rendu.** `useI18n()` rend `t` sous forme de fonction
  recréée à chaque rendu, donc `[t]` n’est jamais stable. Mesuré le 2026-10-05
  sur l’effet qui pousse les libellés du menu de zone : **18 appels au seul
  démarrage**, chacun suivi d’une lecture du démarrage automatique. Règle :
  dépendre d’un **primitif** (`langue`), pas de la fonction de traduction. Et
  lire le journal au démarrage est ce qui a révélé le problème — sans lui,
  rien ne se voyait.
- **Une rédaction doit être testée avec la forme que le code écrit vraiment.**
  Deux implémentations de suite ont passé leurs tests et **toutes les deux
  fuitaient** : la première ignorait `manga_url=https://…` (forme `clé=valeur`,
  donc le jeton ne *commence* pas par `https`), la seconde laissait passer
  `\\?\C:\Users\…` (forme longue que `canonicalize` renvoie). Les tests
  utilisaient des URL nues et des chemins courts — jamais les formes réelles.
  Corollaire : écrire un test à partir d’une **ligne de journal relevée**, pas
  à partir de ce qu’on suppose que le code écrit.
- **Une porte qui ne lit qu’un fichier est aveugle sur le reste du dossier — et
  ça arrive deux fois.** `check-i18n.mjs` ne lisait que `page.tsx` ;
  `aucune_commande_tauri_nest_morte` faisait pareil, et a déclaré morts
  `exporter_rapport` et `purger_journal` alors que `app/diagnostic.ts` les
  appelle. Même famille, deux fichiers différents, même date. Règle : dès
  qu’un écran vit dans un module, toute porte qui balaie le front doit balayer
  `app/` entier, et le dire dans son nom de test.
- **Une classe CSS utilisée mais jamais définie ne se voit pas.** `note-ok` était
  employée dans `page.tsx` pour les messages de confirmation, sans aucune règle
  dans `globals.css` : le texte s’affichait en style courant et personne ne l’a
  signalé. Les tokens de succès existaient déjà dans les deux thèmes — seule la
  règle manquait. Chercher les classes **utilisées** contre celles **définies**
  est un test de dix lignes qui trouve ce genre de trou.

- **Restaurer un fichier ne prouve pas que le binaire a été reconstruit.**
  `Move-Item` (et `Copy-Item`) **conservent le `mtime`** de la source : restaurer
  un fichier sauvegardé *avant* un bris réinstalle un fichier dont la date est
  plus ancienne que la construction du binaire cassé, donc cargo réutilise ce
  binaire. Mesuré le 2026-10-05 : le test « rouge puis vert » restait rouge, et
  la cause n’était pas le test mais le compilateur qui n’avait rien reconstruit.
  Règle : après une restauration, **réécrire** le fichier (ou toucher son
  `mtime`) et vérifier que le code restauré est bien celui compilé — relire la
  source ne prouve pas que c’est elle qui tourne.
- **« La lecture échoue quand j’abîme les données » ne prouve pas *quel*
  contrôle l’a arrêtée.** Trois fois le même jour sur le `DEFLATE` : corrompre un
  octet de la charge compressée fait échouer l’**inflate** avant le CRC, donc le
  test passait même avec la vérification du CRC supprimée ; et localiser une
  structure par scan de signature matchait **dans les données compressées**.
  Règle : pour éprouver un contrôle, **corrompre la valeur attendue**, pas les
  données — l’operation doit réussir et seul le contrôle doit refuser. Et pour
  situer une structure dans un binaire, passer par ce qui est non ambigu
  (l’EOCD en fin de fichier), jamais par un scan de signature.
- **Une constante partagée entre deux langages a besoin d’un test qui lit
  l’autre fichier.** La liste des champs signés existe dans `moteur.rs` et dans
  `moteur-sign.mjs` ; aucun compilateur ne compare les deux, et un champ ajouté
  d’un seul côté serait **silencieusement non protégé** — donc glissable par le
  réseau. Règle : dès qu’une valeur doit rester identique entre Rust et un
  script, le test lit le script et compare, comme
  `les_champs_signes_concordent_avec_le_script`.

- **Un second binaire casse `cargo run`, donc `tauri dev`, donc tout.** Cargo ne
  sait plus quel binaire lancer : « could not determine which binary to run ». Le
  correctif est `default-run` dans `[package]` — **et la clé doit être dans
  `[package]`**, sinon Cargo l’ignore silencieusement et l’erreur affichée ne
  parle ni de la clé ni de sa position. Mesuré le 2026-10-05 : première clé posée
  après `[lib]`, donc dans la table de la bibliothèque ; le message ne dit rien. Après tout ajout de `[[bin]]`, `cargo run` **et** `tauri dev` se lancent.
- **Un test qui ne vérifie que des refus ne prouve pas que le cas nominal
  marche.** J’ai interdit le point dans les noms de version pour empêcher une
  traversée — donc `0.2.0`, c’est-à-dire toute version sémantique, était refusée,
  et le test ne le voyait pas parce qu’il ne listait que des noms hostiles. Ajouté
  depuis : `une_version_semantique_est_acceptee`. Corollaire : à chaque test « un X
  est refusé », demander « et un X **légitime** est-il accepté ? ».
- **Les outils Windows écrivent des antislashs dans les archives, et la norme ZIP
  écrit des slashs.** Mesuré le 2026-10-05 : `Compress-Archive`, 180 entrées sur
  181 avec un backslash. Notre lecteur les **écartait**, donc une archive produite
  par l’outil du projet n’installait rien — silencieusement. Règle : **normaliser
  avant de refuser**, sinon le refus porte sur la forme et non sur le danger ;
  `..\..\x` doit être refusé **après** normalisation, pour la bonne raison.

- **Un retour arrière qui n’est jamais appelé ne protège de rien.** 5.1 fournit
  `revenir_en_arriere()` à la partie 1, testée, documentée — et l’appli ne
  l’appelait pas. Une fonction de reprise non branchée est du code qui rassure :
  on lit le nom, on suppose la garantie. Corollaire : nommer la **fonction** ne
  prouve rien ; ce qui compte est de savoir **qui l’appelle**, et à quel moment.
- **Le retour arrière doit s’exécuter avant le choix qu’il contredit.** `sidecar_paths()` lit le pointeur ; sonder **après** laisserait le superviseur sur les chemins
  du moteur mort. Vérifié par les horodatages du journal : la ligne de la sonde
  précède de 5 ms celle du choix. Règle : dès qu’un retour arrière existe, se
  demander **quelle décision rendrait son travail inutile**, et la placer avant.
- **Un test de reprise sans le cas nominal ne prouve qu’une moitié.** La sonde de
  démarrage a été vérifiée sur le moteur **cassé** (`os error 216` → retour arrière) et
  sur le moteur **sain** (`vérifié au démarrage`, pointeur inchangé). Le cas
  cassé seul prouverait que la sonde échoue, pas qu’elle réussit.

- **Tauri ignore silencieusement un paramètre d'écriture en `snake_case`.** Il
  convertit `camelCase` → `snake_case` côté Rust ; une clé déjà snake_case passe
  **sans erreur** et n'est pas prise. La commande répond normalement avec les
  anciennes valeurs, donc un réglage qui ne bouge pas **paraît** réussi. Mesuré
  le 2026-10-05 sur `set_reglages_planificateur` : `periode_s` sans effet,
  `periodeS` pris. Corollaire : envoyer un paramètre qu'on believes écrit, puis
  **relire la valeur** — une commande qui répond n'a rien écrit.
- **`bornes()`-style : une normalisation destructrice n'est pas un calcul.** Pour
  afficher les bornes d'un réglage, j'ai appelé la fonction qui *borne* les
  valeurs — or elle **écrase** (`self.x = x.clamp(...)`). La vue affichait donc
  `max="20"` au lieu de 200, et le champ était bloqué à sa valeur par défaut sans
  le moindre message. Les bornes sont des **constantes nommées**, et la
  normalisation s'y réfère : deux listes de bornes finissent toujours par
  diverger.
- **Une valeur par défaut n'est pas une absence de champ.** Un test passait
  `Default::default()` pour « rien de fourni » — or `actif: true` y est un
  réglage **actif**. Le test confondait « pas fourni » et « revenu par défaut ».
  Pour une écriture partielle, le seul moyen de dire « rien » est `None` ;
  chaque champ doit pouvoir être absent **explicitement**.
- **`exhaustive-deps` signale parfois un vrai oubli.** Les 4 warnings habituels
  sont du bruit (`react-hooks/set-state-in-effect`), mais « missing dependency
  `chargerPlan` » était exact : la fonction était recréée à chaque rendu sans
  être mémorisée. La règle est de **regarder le nom** avant de classer —
  « toujours 4 warnings » fait oublier le cinquième.



- **Un `CustomEvent` sur `window` ne déclenche pas un `listen` Tauri.** Le
  code écoute via IPC, donc la mesure doit émettre par
  `window.__TAURI_INTERNALS__.invoke('plugin:event|emit', { event, payload,
  windowLabel })` — c'est le même chemin que l'émetteur Rust. Une sonde qui
  dispatche un événement DOM passe au vert sans avoir rien testé, et il faut
  **plusieurs tentatives** pour s'apercevoir que rien ne se passe.
- **`t` dans les dépendances d'un effet est une boucle infinie, et `tsc` /
  `lint` ne la voient pas.** Mesuré le 2026-10-05 en pilotant l'app réelle
  (`.mesures/pilot-13.cjs`) : `useI18n()` rend `t` sous forme de fonction
  **recréée à chaque rendu**. L'effet qui charge les « chapitres récents »
  (`page.tsx`, tableau `[latest, showRecent, selected?.name, t, langue]`)
  dépendait de `t` **et** appelait `setCardRecent` / `setCardTotal` /
  `setRecentBusy` → rendu → nouvelle `t` → effet relancé, sans fin. React
  abandonnait après 50 rendus : **« Maximum update depth exceeded » × 12** à
  l'ouverture d'une série. `tsc` : 0 erreur. `eslint` : rien (le warning
  `exhaustive-deps` ne dit pas *quand* la dépendance est instable). La
  correction est de dépendre du **primitif** `langue` — c'est le même fait que
  la 3.4, qui faisait appeler `set_libelles` 18 fois au démarrage. **Porte**
  `pnpm hooks:check` : refusée si `t` (ou un `useCallback([t])`) revient dans un
  tableau de dépendances d'effet ; prouvée **rouge** en remettant `t`, verte
  après. Corollaire : une dépendance d'effet doit être un **primitif** ou une
  fonction **mémorisée sans `t`** ; `useCallback([t])` appelé d'un bouton est
  sans danger (jamais relancé par un effet) — la porte vise donc l'effet, pas le
  rappel.

- **Une porte qui accuse la mauvaise ligne est une porte qu'on finit par ignorer.**
  Première version de `check-hooks.mjs` : remplacer un commentaire de bloc par
  un seul espace **avalait ses sauts de ligne**, donc un `/* … */` de 246 lignes
  décalait tout — la porte nommait `page.tsx:1734` au lieu de `1980`. Même
  famille que `Move-Item` qui conserve le `mtime` : une mesure qui sort un
  numéro faux n'est pas une mesure fausse, c'est une mesure **inutilisable**.
  Règle : quand une porte lit du texte pour en tirer une position, elle doit
  **préserver la numérotation** (`m.replace(/[^\n]/g, ' ')`), et on vérifie que
  la ligne nommée contient bien le motif cherché.

- **`Set-Content -Encoding UTF8` (PowerShell 5.1) corrompt un fichier source.**
  Utilisé pour casser volontairement le code afin de prouver la porte : le
  fichier a reçu un **BOM** (`EF BB BF` avant le `'use client'`) et a perdu les
  caractères non-ASCII (`…` → `.`). Les accents semblaient intacts parce que le
  *console* les affichait en `?` — seul `git diff` a montré la vraie perte.
  Règle : pour **modifier** un fichier, l'outil `edit` ; pour une casse
  volontaire, une **sauvegarde par copie d'octets** (`Copy-Item`) restaurée
  ainsi, avec le `mtime` remis à jour ; et `git diff` comme preuve du contenu
  réel, jamais l'affichage du terminal.

- **Un état résiduel rend un walk muet, et une mesure répétée sur le même état
  rend le walk encore plus muet.** Mesuré le 2026-10-05 : quatre passages de
  pilotage d'affilée ont tous montré « 0 source pour » et une grille
  vide — le champ de recherche **d'un passage** persistait dans l'état de
  l'application et ne correspondait à aucune source. La réponse était dans
  l'écran lui-même : il s'ouvre sur les **langues** (`.lang-tile`), et la grille
  des sources (`.grid .slot`) n'existe qu'**après** avoir cliqué une langue.
  Corollaires : (a) `reload()` en tête de parcours, sinon on mesure l'état du
  passage précédent ; (b) lire le JSX **avant** d'écrire le sélecteur — j'ai
  cliqué `.chapter-cell` (un `<div>` **sans gestionnaire**) alors que le bouton
  est `.chapter-open`, et j'ai conclu « le lecteur ne s'ouvre pas » ; (c) un
  `disabled: true` sur `→` prouve qu'on est déjà à la dernière page, donc
  qu'`ArrowRight` ne fasse rien est **le comportement correct**, pas un bug —
  il faut le lire avant de le signaler.

- **Une correction peut être plus fausse que le défaut : le défaut était un
  artefact du serveur de dev, pas de l'application.** Mesuré le 2026-10-05 : en
  `pnpm tauri dev`, `data-apparence` restait `null` et `lang` restait `fr`, alors
  que `localStorage` portait `sombre` et `en` et que **l'interface affichait
  déjà l'anglais**. J'en ai conclu que le `<script>` bloquant de `layout.tsx`
  ne s'exécutait pas, et j'ai remplacé le `<script>` nu par
  `<Script strategy="beforeInteractive">`. Puis j'ai lu le **HTML servi** : le
  script n'y existait que comme chaîne dans le payload RSC
  (`self.__next_f.push([0,{"children":"(function(){…`), donc inerte — mais
  **seulement en dev**. L'application livrée charge l'**export**, où le script
  est une vraie balise dans le `<head>` : servi en statique avec `clair` et
  `en` posés, il donne `data-apparence="clair"` et `lang="en"` **dès le
  parsing**, thème clair peint (`rgb(244,244,246)` sur `rgb(22,24,29)`). Le
  correctif était donc inutile — et j'ai dû le retirer. Règle : **un défaut
  observé en dev se vérifie sur l'artefact livré** (`out/index.html` + un
  `python -m http.server`) avant d'être cru ; et « l'interface affiche la
  bonne langue alors que `<html lang>` ne suit pas » est déjà la preuve que le
  cœur et l'état React sont bons, donc que le défaut est plus profond qu'un
  attribut — ou alors qu'il n'existe que là où on regarde.
- **La forme longue d'un chemin s'affiche, alors qu'elle est illisible.** Mesuré
  le 2026-10-05 : l'écran affichait `\\?\C:\Users\alexanders\Documents\Tatami`
  au lieu de `C:\Users\alexanders\Documents\Tatami`. Le préfixe vient de
  `canonicalize`, c'est lui qui débloque les chemins de plus de 260 caractères —
  donc il se retire **à l'affichage seulement**. Deux défauts distincts, mesurés
  séparément : (a) la forme longue affichée telle quelle ; (b) le chemin n'était
  affiché **dans aucun** panneau de réglage, seulement dans l'onglet
  Téléchargements, alors que la note du panneau dit « les téléchargements
  existants restent où ils sont » — sans le chemin sous les yeux, impossible de
  savoir où sont ses 20 Mo. Porte `pnpm chemins:check`. Corollaire : le helper
  `cheminLisible` **existait déjà** (l252) et n'était pas appelé — j'en ai écrit
  un second, `tsc` l'a dit (`TS2393`), ce qui a évité de laisser deux versions
  dans le dépôt.
- **Un réglage peut être correct et invisible : chercher la valeur dans le
  mauvais dossier de mesures.** Le dossier de téléchargement vaut
  `C:\Users\alexanders\Documents\Tatami` (choisi par l'utilisateur), alors que
  le **défaut** est `%APPDATA%\Tatami\downloads`. J'ai cherché le `.cbz` dans
  le défaut, conclu « le téléchargement n'a rien écrit », et le journal a dit
  l'inverse : `telechargements : \\?\C:\Users\alexanders\Documents\Tatami`. Le
  téléchargementmarchait depuis le début. Règle : **lire le réglage dans le
  journal avant de chercher sur le disque** — un dossier réglé par l'utilisateur
  Prime sur le défaut du code, et « 0 fichier nouveau » dans le mauvais dossier
  est le pire des résultats, parce qu'il accuse le code.
- **`cargo test` écrivait dans la vraie bibliothèque de l'utilisateur.** Mesuré
  le 2026-10-06 : `%APPDATA%\Tatami\downloads` contenait **34 dossiers
  `tatami-test-*`** et `IMPORT/` avec **48 sous-dossiers** `tatami-import-*`
  (dont un cas hostile `…_.._.._.._evil`, correctement confiné). Les fixtures
  vivent dans la vraie racine **délibérément** : `list_library` n'accepte que
  cette racine, donc une fixture ailleurs ne prouverait rien. Le défaut n'était
  donc pas l'emplacement, c'est l'**absence de nettoyage** : un
  `remove_dir_all` en fin de test est sauté par le `panic!` juste avant, et il
  était **facile à oublier** — 1 seul test d'import sur 13 appelait `nettoyer`.
  Corrigé par un garde `DossierDeTest` dont le `Drop` efface, donc **même en cas
  de panic**. Prouvé : `panic!` délibéré dans un test d'import → test rouge,
  **0 dossier résiduel**.
- **Un garde qui nettoie doit être *transporté* jusqu'à la fin du test, sinon il
  nettoie trop tôt.** Mesuré le 2026-10-06 : `dossier_source()` retournait le
  `PathBuf` et **jettait la garde** → le dossier était effacé avant le premier
  `assert`, soit **14 tests en échec** sur `écrivain cbz: NotFound`, alors que
  le code de production n'avait pas bougé. Le type qui empêche l'erreur est
  `(PathBuf, DossierDeTest)`, pas `PathBuf`. Même famille pour l'import : le
  titre **sanitisé** n'est pas le titre du garde (`sanitize` transforme `/` en
  `_`), donc un dossier hostile restait sur disque malgré le garde.
- **Un nom fixe partagé entre tests parallèles est un défaut invisible.**
  `tatami-cache-test` n'était effacé par personne, et surtout **deux tests se
  partageaient le même dossier** : le jeton écrit par l'un était lisible par
  l'autre, donc un défaut de `list_library` pouvait être masqué par le contenu
  du voisin. Le seul symptôme visible était le dossier vide qui traînait — la
  moitié sérieuse du problème ne se voyait pas du tout. Règle : tout nom de
  fixture porte **pid + compteur**.
- **Un `.cbz` qui existe n'est pas un `.cbz` valide — et le lecteur strict se
  prouve d'abord sur un cas qui doit échouer.** `.mesures/verifier-cbz.py`
  ouvre l'archive, relit les **octets d'origine** de chaque entrée et vérifie
  la magie (JPEG/PNG/WEBP/…), refuse une entrée non-image, refuse toutes les
  tailles identiques (donc la même page écrite N fois), refuse un `.part`. Avant
  de lui faire confiance : il a **accepté** deux vrais téléchargements (4 et 11
  entrées, toutes tailles distinctes) et **refusé** un ZIP malformé en nommant
  la cause (« Overlapped entries… File name in directory and header differ »).
  Mesure obtenue : `The White Devil.cbz`, **20 128 668 o, 37 entrées, 37 tailles
  distinctes, 37 JPEG** — donc pas de page répétée, pas de HTML déguisé.
- **Déclarer une tâche « manquante » en regardant un écran, c'est une
  hypothèse.** Mesuré le 2026-10-06 : l'écran « état des sources » affichait
  `AUTH_REQUIRED 1`, et j'ai conclu dans `PLAN.md` que « le code qui manque au
  contrat apparaît donc pour de vrai, ce qui confirme la tâche ». J'avais donc
  écrit que 0.13 était **à faire**. Le code disait l'inverse, à chaque étage :
  `protocol.json` déclare le code, `Erreurs.kt:50` le détecte dans la chaîne des
  causes, `Server.kt:343` l'émet, `supervisor.rs:66` le traite comme non
  transitoire, `page.tsx:868` lui donne un libellé, et `ErreursTest.kt` compte
  **8 tests** dont 2 sur l'auth (`tests=8 failures=0`). La tâche était **livrée**.
  Corollaire : une case non cochée ou une phrase d'un journal plus ancien ne
  prouvent rien — **ouvrir le code et lire le résultat de test**. Et l'inverse
  vaut aussi : un bug d'affichage qu'on voit à l'écran peut être déjà corrigé
  ailleurs. C'est la deuxième fois de la semaine (cf. « l'écran dit-on que les
  images viennent d'un hôte ? ») où un écran a servi de preuve pour un code
  absent.
- **Une mesure qui doit montrer un *effet* doit d'abord prouver que l'effet a
  eu lieu.** Mesuré le 2026-10-06 : trois fois de suite la purge du cache a
  semblé ne pas fonctionner (« 400 entrées expirées → 400 »), alors que
  **aucune écriture n'avait eu lieu** — donc rien n'avait pu être purgé. Les
  trois causes : (1) la source était **déjà** dans le cache, donc aucune
  requête, donc aucun `saveRecentCache` ; (2) le clic sur la langue n'avait pas
  pris et l'écran était resté sur la liste des **langues**, donc `latest` était
  vide et l'effet ne s'exécutait pas ; (3) le champ de filtre **réduit la liste
  des sources**, il ne charge pas le catalogue — il faut **ensuite cliquer la
  source**. Le témoin décisif a été `maxAt` : **identique → rien n'a été écrit**.
  Règle : capturer un témoin d'activité avant de conclure que l'action a échoué ;
  une écriture qui ne se voit pas n'est pas une écriture ratée.
- **Un gestionnaire d'évènement qui s'applique à tout casse tout, et le défaut
  apparaît dans la fonction voisine.** Mesuré le 2026-10-06 : la 3.4 fait
  qu'une fermeture devient « mise dans la zone ». Le gestionnaire est
  enregistré **sans filtre de fenêtre**, donc la fenêtre de vérification
  Cloudflare était **cachée** au lieu d'être fermée — et `cancel_challenge`
  renvoyait `Ok(())` en croyant l'avoir fermée. Symptôme mesuré : deux annulations
  successives, fenêtre toujours là, cookie encore lisible. Corrigé par une
  décision extraite en fonction **pure sur le label**
  (`est_fenetre_principale_par_label`), testée, **rouge prouvée** en faisant
  renvoyer `true` à tout. Corollaire : quand une tâche touche « toutes les
  fenêtres », se demander lesquelles — et une fenêtre sans icône de zone ne peut
  pas être mise dans la zone, donc la cacher la rend inaccessible à jamais.
  Une fonction pure + testée vaut mieux ici qu'un `if` dans un gestionnaire
  d'évènement, qu'on ne peut éprouver qu'en fermant de vraies fenêtres.
- **`http://tatami.localhost` est un chemin LOCAL, et un classificateur naïf le
  range dans le réseau.** J'ai conclu « la relecture hors ligne va chercher ses
  pages sur https » alors que les quatre URL étaient
  `http://tatami.localhost/page/<jeton>` : le protocole local, servi par Rust
  depuis l'archive. Preuve que c'est réellement hors ligne : pendant la
  navigation, les seules requêtes sont `tatami.localhost/page/…` et
  `ipc.localhost/save_progress` — **aucun** accès à un hôte du site. Corollaire :
  classer par expression régulière demande de connaître le protocole ; sinon
  lister les URL et les lire.
- **Le pluriel se teste avec une seule unité, jamais avec le nombre qu'on a
  sous la main.** Deux fois dans la journée : 5.4 avec 7 pannes et 3.2 avec 2
  entrées — les deux mesures « vert » laissaient le singulier **non testé**, et
  la capture a montré « 1 source(s) » puis « Voir les 1 chapitres ». Corollaire :
  une vérification d'affichage ne vaut que si elle a raté l'état **minimal**.
- **Un test trop lâche apprend à ignorer ses propres alertes.** `/1 autres\b/`
  matche « **11** autres ». Le nombre précède le mot : il faut une borne
  `[^0-9]`. Le premier jet d'un test de pluriel signale donc des fautes
  imaginaires, et il faut le resserrer **avant** de conclure qu'il est cassé.
- **Copier un existant n'est pas réutiliser un existant.** J'ai écrit
  `ouvrirFavori` avec les deux `Promise.allSettled` de `ouvrirSignet`, alors
  que `loadChapters` en faisait autant — et ma version inventait un
  `setChapitresCharges` inexistant. Chercher avant d'écrire : la duplication
  coûte une erreur de compilateur, une divergence silencieuse, ou les deux.

- **Un panneau qui ne s'affiche qu'à vide ne prouve rien.** L'écran 5.4 rendait
  parfaitement son message « aucune source en échec » — avec **0 source en
  échec**, donc **sans jamais avoir affiché la table**. Une capture de cet état
  est indiscernable d'une capture correcte. Il faut **injecter des données
  représentatives** : plusieurs occurrences d'un même code (pour tester le
  regroupement), des âges très différents (pour tester la traduction), un message
  d'erreur long.
- **« n source(s) » ne fonctionne dans aucune langue.** Le français et
  l'anglais n'ont pas la même règle de pluriel, donc **une clé unique ne peut
  pas servir les deux** : mesuré, l'anglais affichait « 1 months ago ». Deux clés,
  ou un choix de pluriel par langue. Corollaire : `tsc` et `i18n:check` ne
  voient **rien** ici — les clés existent, elles sont correctes, et la faute est
  dans le fait d'en choisir une seule.
- **Changer un réglage par le `<select>` puis recharger ne change pas l'état de
  React.** La sonde signalait un défaut de traduction : elle faisait le
  `dispatchEvent` dans un `evaluate`, puis `reload()`, donc le composant
  redémarrait sur l'ancienne langue pendant que `localStorage` portait la
  nouvelle. Stocker **données et langue dans le même `evaluate`** avant le
  rechargement rend le test exact. Troisième fois de la session où la mesure,
  et non le code, était en cause : le réflexe « chercher le défaut » avant
  « douter de la mesure » vaut de l'heure.
- **`read` peut montrer un fichier tandis que le runtime en charge un autre.**
  L'erreur `x is not defined` alors que `const x` était déclaré trois lignes
  au-dessus a coûté plusieurs tentatives. La cause : un `t` s'était glissé dans
  le nom côté usage (`faussataires` contre `faussaires`) — deux noms presque
  identiques, invisibles à la relecture. Examiner la ligne **en octets** tranche
  en une commande ; relire trois fois ne servait à rien.

- **Une estimation de volume est une conjecture, pas une mesure.** La session 3.1
  annonçait « 200 favoris × 2 000 URLs ≈ 8 Mo » pour l'état du planificateur ;
  mesuré avec des URLs réelles, c'est **31,7 Mo**, soit 4× plus. L'écart vient de
  supposer des chaînes « typiques » : une URL de chapitre réelle fait 83 octets,
  et il y en a deux par entrée (la clé du favori + l'URL). Règle : chiffrer un
  volume avec des **données réelles**, jamais avec une longueur moyenne supposée.
- **Le coût dominant d'un état sérialisé n'est pas le stockage, c'est le
  parsing.** 31,7 Mo de JSON ne gênent pas une passe toutes les 6 h ; mais
  `JSON.parse` prend **80 ms**, et le planificateur relit son état **à chaque
  passe** alors qu'une interface le relirait **à chaque navigation**. Une
  latence *constante* se voit moins qu'un pic, et gêne plus.
- **Un pire cas calculé peut être deux fois trop pessimiste.** 20 sources × 20 s
  de délai = 400 s, ce qui suggère une passe de 400 s au démarrage. Mesuré :
  `EXHENTAI` répond en 1 339 ms — les 20 s sont un **délai de politesse**, pas une
  durée d'appel. Vérifier le pire cas avant de le déclarer : ici il n'a nié le
  risque, il l'a mal chiffré.
- **Deux sessions en parallèle se mesurent mutuellement.** Une session a mesuré
  `pnpm typecheck` en échec et l'a consigné comme un défaut de l'autre ; en
  remesurant une heure après : **0 erreur**. Elle avait raison à l'instant t,
  faux au moment de la conclusion. Règle : une mesure qui accuse une autre
  session doit être **reprise** avant d'être écrite dans un journal partagé.

- **« Il nous manque une racine de confiance » est une théorie, pas une
  observation.** C'est celle que j'ai écrite sur 0.16, et elle était fausse —
  et elle se corrigeait par l'ajout d'une racine au magasin du système, donc
  par un élargissement de la confiance. Ce qui l'a renversée : **un témoin**.
  MANGADEX et SEIMANGA sont signés par `YE1`/`YE2`, exactement comme la source
  qui échoue, et sont validés le même jour par le même code. Chaîne identique,
  résultats opposés → la cause est ailleurs. Règle : devant une erreur de
  certificat, trouver **une source qui marche** et comparer ; si elle a la même
  chaîne, l'hypothèse tombe. Ne jamais proposer une action de sécurité avant.
- **La documentation de l'autorité de certification tranche souvent mieux que
  le message d'erreur.** `PKIX path building failed` invite à « ajouter la racine
  manquante ». La page Let's Encrypt dit l'inverse : `Root YE` « is not yet
  included in Root Program Trust Stores » et « not expected to work with any of
  the major trust stores ». Donc **aucun JDK à jour ne l'aura** : la piste du
  « JDK plus récent » était une impasse, et elle sonnait très raisonnable.
- **`getPeerCertificate()` de Node n'est pas une chaîne.** Depuis Node 22,
  `subject` et `issuer` sont des objets : `.match` n'existe pas, et `String()`
  lève `Cannot convert object to primitive value`. Il faut les sérialiser avec
  `Object.entries`. Trois tentatives avant de lire la sortie de la sonde —
  exactement le piège de « deviner une API au lieu de la lire ».
- **Un hôte qui ne résout pas en DNS masque tout le reste.** `mangalesen.com`
  et `mangasoriginesunofficial.com` répondent `ENOTFOUND` : leur échec TLS
  historique n'a plus de cause observable. Les compter comme « échecs de
  certificat » est faux ; ils sont **injoignables**, ce qui est un statut
  différent — et « compter 3 échecs TLS » devient « 1 échec et 2 morts ».

- **Une fixture écrite à la main ne prouve qu'une chose : qu'elle est conforme.**
  Une réponse du moteur truly enregistrée est la seule preuve ; une lue dans
  `Server.kt` et écrite « conforme au schéma » passerait le test même si elle
  était fausse. Mesuré le 2026-10-05 : les 16 fixtures viennent d'une sonde
  NDJSON réelle (`hello` → `protocolVersion: "0.1"`, `listSources` → **1370**
  entrées, `chapters` → **222** chapitres, `fetchImage` → 604 128 octets). Ces
  nombres sont dans les fixtures, pas dans ma mémoire.
- **Une porte qui n'a jamais rien trouvé n'est pas une porte — et le test peut
  passer pour une mauvaise raison.** J'ai voulu prouver `is_transient` en
  retirant son cas `PARSE_ERROR` : **20 tests sont restés verts**, parce que la
  liste de marqueurs finals exclut déjà ces messages. Le code cassé ne changeait
  rien. Il a fallu le casser plus franchement (`PARSE_ERROR` → transitoire) pour
  voir **2 tests échouer**. Règle : une porte se prouve en la cassant, et
  « vert après une modif » ne prouve rien tant qu'on n'a pas vu **le nom du
  test qui échoue**.
- **`gradle test` peut être vert sans exécuter un seul test.** Deux fois de suite
  le 2026-10-05 : `UP-TO-DATE`, puis `FROM-CACHE` (le cache de build est activé
  dans `gradle.properties`). La preuve d'exécution est le compte de
  `build/test-results/test/*.xml`, rien d'autre. Même famille que la porte Gradle
  vide trouvée le 2026-10-04 (0 fichier de résultat).
- **Un test d'intégration ne voit pas le code d'un crate binaire.** Le crate
  était `main.rs` + `mod` : `tests/` ne pouvait valider que le schéma, jamais la
  façon dont Rust lit une réponse. Le découpage `lib.rs` + binaire rend le code
  testable ; côté Tauri, `generate_handler!` résout le chemin jusqu'au dernier
  segment puis invoque le macro `__cmd__x` **au même chemin**, ce qui marche
  depuis un autre crate grâce au `pub use` émis par le wrapper
  (`tauri-macros/src/command/wrapper.rs`). Vérifié dans la source, pas deviné.
- **`cargo clippy` sans `--all-targets` ne regarde pas `tests/`.** Une porte qui
  « vérifie le format du test de contrat » sans `--all-targets` ne le vérifie
  pas. Même piège que `pnpm lint` qui ne lintait que `src/` (§21).
- **Rendre `pub` un élément du cœur demande une raison écrite.** J'ai publié 5
  éléments (`into_result`, `is_transient`, `PROTOCOL_MAJOR`,
  `ajouter_jetons_couverture`, la cible `lib`), chacun avec le défaut qu'il
  rend visible — sinon on refait les commandes mortes supprimées le 2026-10-04.

- **Le `stdout` du moteur arrive en morceaux de 8 Ko.** Un script qui découpe sur `\n` sans buffer vorher jette les fragments et affiche « aucune réponse » alors que la réponse est arrivée. Bufferiser puis découper — une réponse `listSources` fait ~47 Ko.
- **Une image `<img>` cassée est déjà « visible » pour Playwright.** Attendre `img.complete && img.naturalWidth > 0`, sinon on mesure une image de 21 px de haut et on conclut à tort que le CSS est fautif.
- **`page.waitForFunction(fn, options)` n'existe pas.** La signature est `(fn, arg, options)` ; passer l'objet en 2ᵉ argument fait retomber silencieusement sur le délai par défaut de 30 s.
- **Les préférences du lecteur sont relues seulement au montage.** Écrire `localStorage` puis attendre ne change rien à l'état React : un walk doit cliquer les segments.
- **Dans une grille, `grid-template-rows` implicite vaut `auto`.** `height: 100%` et `max-height: 100%` sur un enfant s'y résolvent alors en `auto` et l'image déborde (mesuré : 6 300 px dans un cadre de 685). Écrire `grid-template-rows: minmax(0, 1fr)`.
- **`max-width: 100%` ne borne pas la hauteur.** Sur une page très haute il faut aussi `max-height`.
- **Une valeur servie par le cache mais jamais reportée dans l'état reste invisible.** L'effet voit la cache valide, ne relance aucune requête, et l'affichage reste vide indéfiniment. Reporter la cache dans l'état dans le même effet.
- **Un champ non typé côté TS peut déjà arriver du moteur.** `uploadDate` était émis et transmis par Rust (pass-through) depuis le début, seul le type l'ignorait. Avant d'ajouter une requête, vérifier ce que le moteur renvoie déjà.
- **Les résumés de sources arrivent en HTML.** Stripper balises et entités avant affichage, sinon `<p>…</p>` s'affiche tel quel.
- **« Le filtre réduit le nombre de cellules » peut ne rien prouver.** Sur une série à 286 chapitres, filtrer « 2 » matche `Ch. 2`, `Ch. 20`, `Ch. 21`… Choisir un numéro rare, calculé depuis le DOM.
- **Une source qui répond à `pages` peut refuser les images — mais pas toujours, et pas forcément à cause de l'agent utilisateur.** Version mesurée le 2026-10-04 (`docs/rapport-session-recherche.md` §3, 5 agents utilisateurs × 3 pages) : **REMANGA** renvoie `CHALLENGE_REQUIRED` sur `img.reimg.org` (le CDN exige un challenge Cloudflare) et **aucun** agent utilisateur ne débloque ; **SEIMANGA** sert ses images normalement (444 832 octets WebP), y compris avec `Tatami/0.1` comme agent utilisateur. Donc : ne jamais conclure « c'est l'agent utilisateur » sans matrice de mesure, et vérifier l'image, pas seulement la liste.
- **L'agent utilisateur n'est presque jamais le facteur.** Sur les mesures du 2026-10-04, une source qui bloque le bloque pour tout le monde : soit le domaine est mort, soit Cloudflare est entre les deux, soit c'est le blocage de la source elle-même.
- **Fermer stdin ne tue pas un process JVM.** Mesuré : le moteur est encore vivant 4 s après la fermeture de stdin. La commande `shutdown` donne `exit 0` en 137 ms. Le superviseur doit envoyer `shutdown` **puis** fermer stdin, et garder un `kill` en secours (tâche 0.15).
- **`AuthRequiredException` ≠ challenge Cloudflare.** Elle n'est levée que sur HTTP **401** et arrive comme `INTERNAL` ; `CHALLENGE_REQUIRED` exige 403/503 **et** un marqueur HTML Cloudflare. 16 parseurs lèvent la première. Le contrat n'a aucun code pour « connexion requise » (tâche 0.13).
- **Les nombres flottants des sources sont des identifiants, pas des données.** Une division flottante du site arrivait jusqu'à l'écran (« Ch. 38.09999847… ») : arrondir avant d'afficher, et `Math.round` plutôt que `Math.floor` pour comparer.
- **Depuis Next 16, `next lint` n'existe plus** et `next build` ne lance plus le linter. Il faut `eslint.config.mjs` (flat config : `defineConfig` + `globalIgnores` + `eslint-config-next/core-web-vitals`) et le script `lint` pointant sur le binaire `eslint`.
- **Le peer `eslint >=9` d'`eslint-config-next` est trompeur en pratique** : son `eslint-plugin-react@7.37.5` plante sur ESLint 10 (`contextOrFilename.getFilename is not a function`). Épingler ESLint **9** tant que le plugin n'est pas mis à jour.
- **Une directive `eslint-disable` signalée « inutilisée » peut en masquer une vraie** : ESLint ne signale qu'un avertissement par directive. En supprimer une a fait apparaître 2 vrais `missing dependency`. Lire ce que l'avertissement révélait avant de mass-supprimer.
- **`role="tab"` et `aria-pressed` sont incompatibles** (`jsx-a11y/role-supports-aria-props`). Pour une navigation entre vues, `aria-selected` est le bon attribut — et le sélecteur CSS doit suivre, sinon l'onglet actif cesse d'être mis en valeur.
- **Pousser `SCHEMA_VERSION` et son bloc de migration dans deux éditions différentes casse la base pour toujours.** Le binaire intermédiaire lit la nouvelle version, saute la migration (`current >= SCHEMA_VERSION`) et estampille `user_version`. Mesuré ici : `favorites.content_rating` absente avec `user_version = 3`. Règle : **un numéro de version et son `ALTER TABLE` partent dans la même édition**, et les colonnes s'ajoutent par **présence** (`PRAGMA table_info`) à chaque démarrage, pas seulement par comparaison de version.
- **Un `Option<String>` qui devient `"SAFE"` par défaut casse silencieusement une option de l'interface.** Le réglage « Toutes » envoyait une chaîne vide, interprétée comme SAFE : les sources adultes renvoyaient zéro résultat. Même famille : une **chaîne vide ne doit jamais être un sentinel** ; laisser l'option `None` et décider une fois pour toutes.
- **Les tests Rust quifabricent un fichier avec `now_ms()` se marchent dessus** (les tests sont parallèles, deux démarrent dans la même milliseconde). Ajouter pid + compteur atomique au nom du dossier temporaire.
- **Les deux points d'entrée d'une même donnée peuvent se contredire.** Mesuré : WAMANGA classe `ADULT` dans la liste mais `UNKNOWN` sur la fiche, MANGABUFF l'inverse. Ne jamais choisir « la source de vérité » à l'avance : prendre la première valeur **connue**, et ne jamais écraser une valeur déjà apprise (`COALESCE(NULLIF(?,'UNKNOWN'), col)`).
- **Un test qui lit l'état trop tôt conclut « ça ne marche pas ».** Le moteur répondait 20 titres une seconde après le clic ; le walk lisait 0 et le click suivant expirait. Attendre **l'élément** (`waitFor`), jamais un délai fixe, et afficher l'état (`.count`, `.error`) quand l'attente échoue.
- **Rejouer un walk après un échec : recharger la page d'abord.** Un walk qui rate laisse l'app au milieu d'une fiche ou d'un lecteur, et le suivant clique une vue qui n'existe plus. Corollaire : un walk qui **modifie** un état persistant doit le remettre à zéro au démarrage, sinon il échoue pour une raison absente du code.
- **Une mesure qui passe sur une liste vide ne prouve rien.** Mon sélecteur de titres était faux, le walk renvoyait `[]`, et trois assertions « A→Z = Z→A inversé » passaient parce que deux tableaux vides sont égaux. Toujours mettre un garde-fou du type « au moins N éléments lus dans le DOM » **avant** les mesures dérivées.
- **Une fonction qui sort en silence sur un paramètre manquant est un piège.** `openReaderAt(c, index, sourceName)` fait `if (!src) return;` : ouverte depuis l'accueil (aucune source sélectionnée), elle ne faisait rien du tout, sans erreur. Même famille : toute garde `if (!x) return;` sur un chemin nommé devrait être vérifiée par un test qui ouvre précisément ce chemin.
- **En RTL, la flèche gauche AVANCE au lieu de reculer.** Mesuré : `ArrowLeft` en dernière page ne fait rien. Pour un walk, passer par les boutons « Page précédente » / « Page suivante », dont le sens ne dépend pas du sens de lecture.
- **Chaîner `locator().locator().click()` peut viser une ligne périmée.** Symptôme : le walk rapporte « ouvert à la page 1 » alors que l'app ouvrait la bonne page. Utiliser `.first()` et **afficher le texte de la ligne réellement cliquée** avant de conclure que l'app a tort.
- **Ne jamais écrire des octets distants sur le disque sans vérifier leur nature.** Mesuré : une source répond `200 OK` avec la **page HTML** de son interstitiel de vérification d'âge ; le téléchargeur l'écrivait dans un `.cbz` valide sous le nom `001.jpg`, et l'extension venait du `contentType` **annoncé** — donc un `text/html` devenait `jpg`. 37 chapitres sur 44 illisibles, lecteur affichant du noir. Règle : le type se déduit des **octets** (`image::detect`), l'extension écrite suit ce type, et rien n'est mis en cache ni écrit avant vérification. Corollaire : une API qui dérive l'extension d'un en-tête, doit être supprimée, pas conservée « pour la compatibilité ».
- **Un téléchargement réussi n'est pas un téléchargement valide.** La logique « ne jamais retélécharger ce qui est là » (exigence permanente) reconnaît ce qui est **présent sur le disque** ; si elle ne vérifie pas que ce qui est présent est **juste**, elle garantit l'irréparable. Un fichier corrompu reconnu « téléchargé » est pire qu'un fichier absent.
- **Un garde-fou du moteur ne voit pas ce qui répond `200 OK`.** `CHALLENGE_REQUIRED` n'est levé que sur 403/503 ; or une page de vérification d'âge (mesurée : HENTAIORIGINES, script `ori-age-verif`) est servie avec **200** et un corps HTML. Le moteur n'a donc aucun problème à signaler, et tout ce qui attend ce code attend indéfiniment. Corollaire : quand une requête « réussit » avec un résultat aberrant (ici 45 URLs d'images **vides**), le défaut est rarement dans le parseur — c'est qu'un HTTP 200 SemanticallyIncorrect est traité comme un succès.
- **Un test négatif seul ne prouve pas qu'un correctif est bon.** « Aucun `.cbz` écrit » se satisfied aussi bien si les téléchargements sont cassés *entièrement*. Il faut **deux** contrôles : le négatif (la source visée ne produit rien de mauvais de mauvais) et le positif (une source qui marche produit toujours un fichier valide, relu par un lecteur **strict** — ici `zipfile`, 11 entrées, 11 tailles distinctes : la preuve qu'on n'écrit pas 11 fois la même page). Un artefact n'est valide que si son contenu est varié et volumineux.
- **Mesurer un état avant/après, jamais un absolu.** « 1 `.part` laissé » était un échec trompeur : le fichier datait de 17 minutes avant le walk (un reste de l'ancien bug). Un contrôle d'état persistant se fait en **delta** entre deux relevés dans le même walk.
- **Une commande d'annulation qui réussit toujours ne prouve rien.** `cancel_challenge` fait `if let Some(w) { w.close() }` puis `Ok(())` : elle renvoie donc `Ok` même quand aucune fenêtre n'a jamais existé. L'utiliser comme « l'état est propre » fait passer un test sans vérifier l'état. Prouver l'absence d'une fenêtre = compter les fenêtres réelles (CDP), pas appeler une commande destructive.
- **Avant de conclure « c'est un blocage », lire le HTML d'abord.** Un symptôme « il faut une vérification / une authentification » se lisait sur cette source ; la page **sans cookie** contenait pourtant 53 images, dont 45 en `data-src` (lazy-load), toutes servies sans cookie. Le cookie n'a jamais été la cause. Réflexe : `fetch` la page soi-même, compter les `<img>`, regarder leurs attributs — cela distingue « le site me refuse » de « le client ne lit pas ce qui est là ».
- **Une assertion de test peut cibler la mauvaise chose tout en ayant l'air précise.** `sortie.contains("src=\" https://") == false` échouait parce que `data-src=" https://` contient cette sous-chaîne : le test détectait l'attribut d'origine, intact, et non l'attribut écrit. Vérifier qu'une assertion vise **l'occurrence qu'on vient de produire**, pas une sous-chaîne qui existait déjà.
- **`cancel`/fermeture d'une fenêtre ≠ preuve de l'absence de fenêtre.** cf. ci-dessus : compter, ne pas interroger.
- **Une porte de vérification qui n'exécute rien ne vaut pas une porte.** `gradle test` sur le moteur était vert avec **0 fichier** dans `build/test-results`. Avant de citer une porte, vérifier qu'elle a vraiment exécuté des tests (`testImplementation(kotlin("test"))` manquait).
- **Une webview qui affiche une URL distante perd tout ce que le moteur a mis en place — en silence.** Mesuré le 2026-10-04 : les 6 vignettes faisaient `<img src={coverUrl}>` et la CSP l'autorisait (`img-src ... https:`). Conséquence : pas d'UA, pas de cookies, pas de déblocage Cloudflare, pas de rate-limit — et **aucune erreur console**, juste une vignette vide. Toute image distante passe par le moteur, comme les pages. Corollaire : retirer `https:` de `img-src` pour que l'erreur soit **bloquée** plutôt que silencieuse, et émettre un jeton (`coverToken`) dans la charge utile au lieu de l'URL.
- **Un test qui passe avec l'ancien code ne teste rien.** `FermetureStdinTest` lisait les réponses *avant* de fermer stdin : il passait avec `shutdownNow()` comme avec le correctif. C'est en écrivant le test que l'écart est apparu (mesure séparée : 0 ligne avec fermeture immédiate, 6 avec `shutdown`). Règle : écrire le test depuis **la mesure**, jamais depuis l'explication du code ; puis le faire échouer en remettant l'ancien comportement.
- **Le type est souvent une porte plus fiable qu'une regex.** L'audit conclut « 91 clés i18n mortes » ; `tsc` a refusé 6 d'entre elles, utilisées via `{ cle: 'tri.az' }` ou `t(cond ? 'a' : 'b')`, que la regex ne voyait pas. Corollaire : quand un nettoyage est guidé par une recherche textuelle, **le compilateur tranche** — et la regex doit apprendre ses formes, pas l'inverse.
- **Une commande morte ne casse jamais rien.** `commands::search` et `library::existing_chapters` étaient enregistrées, compilées, testées — et inutilisées depuis la 0.9. `search` portait en plus une sémantique de classification **opposée** à `latest` pour le même réglage. Porte ajoutée : `aucune_commande_tauri_nest_morte`, prouvée ROUGE (front vide) puis VERT.
- **`shutdownNow()` sur un pool qui écrit sur stdout peut tronquer une réponse.** Interrompu entre `println` et `flush`, le thread laisse stdout terminé par une ligne partielle, que le lecteur JSON de Rust rejette — pris pour un moteur corrompu. Mesuré : 5 requêtes d'un bloc + fermeture stdin = **0 ligne**. Utiliser `shutdown()` (n'interrompt rien) puis attendre un compteur de requêtes en vol.
- **Fermer l'application avant de rebuild le moteur.** `scripts/build-engine.ps1` écrit dans `dist/.stage` et refuse d'échanger `dist/` — protection utile, mais l'app ouverte et son superviseur qui relance le sidecar en boucle rendent l'échange impossible. Tuer `tatami-desktop` d'abord, sinon le build échoue sur « Impossible de remplacer dist ».
- **Un test qui lit sa propre source ne prouve rien.** Le test « `note_chapter` n'est plus ignoré » cherchait la chaîne `let _ = super::library::note_chapter` dans `download.rs` — il trouvait **sa propre ligne de test**. Supprimé : ce qui compte est le comportement, pas la présence d'un motif.
- **Mesurer un dictionnaire sur ce qui s'affiche, pas sur ce qu'il contient.** 199 clés i18n pour 108 utilisées : les 91 autres ne cassaient rien et ne se voyaient pas, tout en faisant croire l'écran traduit. `pnpm i18n:check` verrouille désormais « aucune clé morte ».
- **Une porte de code mort ne se déclenche pas au moment où on l'écrit — après.** Les trois commandes `*_downloads_dir` ont passé `cargo test` uniquement parce que le test ne cherchait que le front pour *d'autres* commandes ; il a échoué une fois le front branché. Une porte qui n'a jamais rien trouvé n'est pas une porte : la faire échouer au moins une fois avant de la croire.
- **Un réglage que Rust applique ne peut pas vivre dans `localStorage`.** Le dossier de téléchargements est une donnée que le cœur applique à chaque écriture de fichier ; une webview qui la posséderait diverge de l'état réel au premier redémarrage. Règle générale : `localStorage` pour ce que le lecteur lit (`localStorage` = affichage), la base pour ce que le cœur applique.
- **Valider un chemin en l'écrivant, pas en le Stat.** Un dossier peut exister et être inaccessible (droits, antivirus, disque plein) : la vérification n'échouerait qu'au premier téléchargement, donc au pire moment. `verifier_dossier_ecriture` y crée un vrai fichier, et supprime — c'est une sonde, pas un test.
- **Renommer l'état pour qu'il dise ce qui se passe.** Le même code s'appelait « arrêter » et marquait `annulé`, alors que la file se reprenait : l'utilisateur y lisait un abandon, et c'est ce qu'il a signalé. L'action et l'état doivent dire la même chose que la reprise possible.
- **Créer un token ne suffit pas : lui donner une valeur par thème.** Le token
  `--appbar-fond` a été créé puis laissé **identique dans les deux thèmes** — la
  barre est restée noire en thème clair, et comme le texte des onglets vient de
  `--text` (foncé), il rendait **1,2:1**. Signalé à l'écran, pas par un calcul.
  Corollaire : à l'ajout d'un token, vérifier qu'il apparaît dans **les deux**
  blocs, sinon il hérite du thème sombre par pur oubli.
- **Un module qui écrit dans le DOM au chargement arrive TROP TÔT pour React.**
  `apparence.ts` et `i18n.ts` posaient `data-apparence` et `lang` sur `<html>`
  au chargement du module. C'est avant l'hydratation — donc React signalait
  « some attributes of the server rendered HTML didn't match », en boucle, sur
  **chaque** navigation. La solution documentée Next.js : un script **bloquant**
  dans `layout.tsx` + `suppressHydrationWarning` sur `<html>`. Corollaire : pour
  lire l'attribut fautif, `console.error` ne suffit pas — il faut
  `addInitScript` qui intercepte avant React, la fin du message porte le
  diff (`+ lang="fr"` / `- lang="en"`).
- **Supprimer le fichier de cache ne prouve pas qu'une requête est « froide ».**
  Le cache HTTP de Chromium répond en 4 ms même fichier absent : j'ai conclu
  « la précharge ne fait rien » après 30 s d'attente sur des pages déjà
  Chargees, alors qu'elle écrivait bel et bien. Corollaire : la preuve est dans
  l'**écriture disque** d'une page jamais demandée, ou alors `Network.setCacheDisabled`
  via CDP. Chronométrer ne prouve rien tant qu'un second cache peut répondre.
- **Une sonde qui rapporte « rien ne bouge » peut mesurer le mauvais élément.**
  Quatre sondes de molette ont conclu « la molette est cassée » alors que le
  curseur était resté au-dessus d'un conteneur à défilement propre (`.chip-wrap`).
  Toujours `document.elementFromPoint` avant d'agir de la molette, et comparer au
  max théorique (`scrollHeight - clientHeight`) avant de conclure « figé » :
  173 px de course sur une page de 918 px, c'est le maximum, pas un blocage.
- **Un delta de molette de 600 px traverse un webtoon entier** (mesuré : 71
  pages). Le compteur de page suit la dernière image croisée via
  `IntersectionObserver` — c'est le comportement voulu de 1.9, pas un défaut de
  navigation. Mesurer une navigation à la molette demande un **petit** delta.
- **Un token ne doit pas servir deux usages qui divergent.** `--appbar-fond` sert
  à la barre de titre **et** au compteur posé sur une couverture. Le faire
  basculer aurait éclairci le second — qui est posé sur une image et doit rester
  sombre. Un token par comportement, pas par élément.
- **Comparer un texte à un `rgba` ne veut rien dire.** Il faut composer le voile
  translucide sur le fond **réel** avant de calculer le contraste. Mesuré : un
  chip donnait 1,83:1 sur la valeur brute et 10,7:1 sur le rendu réel. Le même
  piège en version inversée : la barre à 92 % d'opacité, mesurée brute, donne
  des chiffres sans signification.
- **Une mesure qui renvoie 0 élément ne prouve rien.** Trois sondes successives
  sur `.card-count` ont rendu « absent » alors que la pastille existait : elle
  n'apparaît que si `cardTotal` est rempli, ce qui exige de **sélectionner une
  source précise** et d'attendre l'appel `chapters`. Les chips du catalogue sont
  des langues, pas des sources.
- **Un dialogue natif n'est pas testable par CDP.** `Escape` sur le sélecteur
  Windows de dossiers **a confirmé un dossier** au lieu de l'annuler pendant un
  test automatisé. Seule la commande Rust est vérifiable de bout en bout — donc
  c'est elle qu'il faut tester, et le dialogue se vérifie à l'œil.
- **Un test qui appelle la bibliothèque ne prouve pas le chemin de production.** Le test ZENMANGA (0.17) appelait `parser.getPageUrl` puis le client : vert. L'application montrait toujours des images vides, parce que `fetchImage` téléchargeait **en dehors** du `pool.avec` qui pose le parser. Il a fallu passer par `dispatch` — la fonction réellement appelée par `serve()` — pour voir le défaut, puis le corriger. Corollaire : `java.class.path` dans un `ProcessBuilder` passe aussi les classes de test au moteur enfant, qui démarre alors plus lentement ; prendre le classpath de `installDist`.
- **Des threads concurrents pour lire un flux sequentiel font perdre des lignes.** Le test de fermeture stdin démarrait cinq threads pour cinq lignes de stdout ; ils se disputaient le flux, d'où « 0 réponse sur 5 » alors que le moteur en rendait 5/5 hors test. Un `FutureTask` par ligne, séquentiel, et le blocage disparaît — le défaut était dans le test, pas dans le code.
- **Un `Mutex` n'est pas réentrant.** Imbriquer `pool.avec(source)` dans `pool.avec(source)` fige le moteur, et ça se manifeste par un **timeout** du test, pas par un échec propre. Symétriquement, `ThreadLocal` + `Mutex` : deux requêtes sur la même source ne se marchent pas dessus, mais il faut le mutex pour ça — le `ThreadLocal` ne le remplace pas.
- **Une horloge figée au début d'une boucle longue rend le débit FICTIF.** Dans
  le planificateur (3.1), la passe interroge jusqu'à 20 sources à 20 s d'écart :
  elle dure donc 400 s. Horodater toutes les requêtes avec l'heure du **début**
  de passe leur donne le même `t`, donc `prochaine = t + 20 s` est déjà dépassé à
  la deuxième requête et **le délai par source ne s'applique jamais**. La
  politesse était dans le code et absente du comportement. Règle générale : dans
  une boucle qui dure, **relire l'heure à chaque itération**, jamais une fois au
  début. Corollaire de mesure : pour prouver un délai, un compteur d'appels ne
  suffit pas (« 3 appels » peut vouloir dire « 3 appels en 400 ms ») — il faut
  l'**instant** de chaque appel.
- **`tokio::time::interval` a deux surprises, opposées.** Le premier `tick` est
  **immédiat** (pratique : c'est la passe de référence au démarrage, sans attendre
  une période), et les ticks **manqués sont rattrapés par défaut** — donc une
  passe plus longue que la période en déclenche une autre d'affilée, en rafale.
  `MissedTickBehavior::Skip` est obligatoire dès que la période peut être plus
  courte que le travail. Une seule de ces deux surprises se remarque en lisant le
  code.
- **Un état qui ne retient qu'une date ne prouve pas un ORDRE.** Le test
  « aucun favori n'est affamé par le plafond » échouait à tort : il relisait
  `verifie_le` par favori, et cinq favoris ajoutés dans la même milliseconde
  partageaient la même valeur — donc l'« ordre » mesuré était celui du tri de la
  base, pas celui des requêtes. La preuve est le **journal des appels** (l'ordre
  réel), jamais l'état. Même famille que « une mesure qui renvoie 0 élément ne
  prouve rien », mais inversée : une mesure qui renvoie N éléments dans le
  mauvais ordre ne prouve rien non plus.
- **Un test qui change la réponse d'une doublure « juste après » le démarrage est
  ALÉATOIRE.** Le test de boucle posait la réponse mise à jour 1 ms après le
  `spawn` ; la première passe pouvait la voir déjà à jour, mémoriser le chapitre
  comme vu, et ne jamais l'annoncer. Le test passait une fois sur deux, pour une
  raison absente du code. Doublure par **réponses consécutives** (la 1ʳᵉ à l'appel
  1, la 2ᵉ à l'appel 2, la dernière ensuite) : la course disparaît au lieu d'être
  espérée.

- **Un squelette doit emprunter la structure du contenu, sinon il fait pire que
  rien.** Mesuré le 2026-10-06 : le catalogue met 1,3 s et la fiche 2,5 s à se
  remplir, et le squelette (un simple rectangle) mesurait **234 × 351 px** contre
  **188 × 328 px** pour la vraie carte — **46 px d'écart en largeur**, 0 enfant
  contre 2. Le contenu arrivant, toute la grille se réorganisait ; l'œil suit un
  contenu qui bouge, c'est **pire** qu'un écran vide. Le remède n'est pas une
  hauteur en dur : c'est de **recomposer les mêmes classes** que le contenu
  (`.manga` > `.manga-open` > `.cover` + `.label`), pour que la géométrie vienne
  du même CSS et ne puisse pas diverger. Après : 190 × 320 contre 188 × 328, et
  193 × **62** contre 193 × **62** sur la ligne de chapitre. Corollaire, mesuré
  dans la foulée : le nombre de squelettes doit suivre ce qui arrive (8 dessinés
  pour 20 cartes, donc une ligne en plus apparaissait puis disparaissait).

- **Un `<span>` vide dans un conteneur flex mesure 0 px.** La ligne de chapitre
  faisait 46 px au lieu de 62 **après avoir retiré toutes les hauteurs en dur** :
  un élément flex sans contenu n'a pas de boîte de ligne, donc il s'effondre.
  J'avais conclu « la boîte naturelle suffit » — elle n'existait pas. Il faut un
  contenu insécable (`{' '}`) avec `color: transparent` : la hauteur vient alors
  de la même boîte de ligne que le texte réel. Corollaire : « j'ai enlevé la
  valeur en dur » ne prouve pas que la valeur est redevenue correcte — la mesure,
  elle, peut le dire.

- **Un état initial lu dans `localStorage` fait abandonner l'hydratation entière.**
  Mesuré le 2026-10-06, sur l'app réelle : `useState<MajItem[]>(loadMaj())`.
  L'export statique produit un HTML qui ne peut pas contenir le `localStorage` de
  la machine, donc `loadMaj()` renvoyait `[]` côté serveur (son garde
  `typeof window`) et la vraie liste côté client. Diff React :
  `+ <section className="maj">` / `- <div className="band">`. **Ce n'est pas
  cosmétique** : abandonner l'hydratation fait reconstruire la page entière sur
  le client, donc dégrader le chargement qu'on cherche à alléger. Le motif
  correct existe déjà dans ce dépôt : lire dans un **effet de montage** (comme
  les préférences du lecteur, même raison). Corollaire : le même état peut être
  un défaut **latent** — `notif` avait la même cause, invisible parce que
  l'interrupteur n'existe que dans l'écran des réglages. Chercher `useState(` avec
  un appel qui touche à `window` / `localStorage` / `Date.now()`.

- **Un échantillon à un seul instant ne prouve rien.** Mesuré le même jour : « à
  1,2 s il n'y a aucun squelette » — vrai, et sans importance, parce qu'en
  développement React n'a pas encore hydraté. La chronologie complète à 120 ms a
  montré squelette de 0 à 295 ms puis contenu à 436 ms, ce que l'instant unique
  ne montrait pas. Corollaire : afficher la **suite** des états, pas un état.

- **Un relevé fait avant que l'action ait pris mesure l'état précédent.**
  `location.reload()` est asynchrone : le relevé suivant part **avant** la
  navigation, donc il lit l'ancien contenu. J'ai obtenu « 61–72 ms » pour un
  accueil — chiffre plausible et **faux**, écarté du code. Même famille : attendre
  `domcontentloaded` **puis** échantillonner, jamais l'inverse.

- **Un sélecteur fondé sur un libellé traduit casse dès que la langue change.**
  Mesuré le 2026-10-06 : après un `localStorage.clear()`, l'interface repasse en
  français, et « Téléchargements » ne contient pas `download` — mes quatre
  sélecteurs d'onglet par mot anglais sont tombés, et l'écran a semblé « jamais
  prêt » pendant 20 s. Corollaire : sélectionner par **indice** ou par attribut
  stable, jamais par le texte affiché ; et une marche qui remet l'état à zéro
  doit aussi remettre la **langue**.

- **Un temps mesuré dépend de ce qu'on mesure autour.** « L'accueil met
  1 393 ms » incluait le rechargement de la page ; depuis `domcontentloaded`,
  c'est 436 ms. Les deux sont vrais et ne disent pas la même chose. Corollaire :
  dans un commentaire, écrire le chiffre **avec sa condition de mesure** — sinon
  le prochain lit un nombre qu'il ne peut pas reproduire.

- **Normaliser les fins de ligne d'un fichier qu'on n'a pas modifié entièrement
  produit un diff fantôme.** Mesuré le 2026-10-06 : AGENTS.md était en CRLF,
  j'ai converti les 127 LF que mon bloc apportait, et l'outil d'affichage a
  montré 133 lignes modifiées dans une zone intacte. `git diff --numstat` disait
  **64 insertions, 0 suppression** : aucune ligne n'avait bougé. Corollaire : ne
  pas « réparer » les fins de ligne d'un fichier partagé ; s'appuyer sur
  `--numstat` plutôt que sur le diff affiché, qui normalise et ment.
- **Bornez la concurrence ET l'intervalle : ce sont deux choses, et la seconde
  fait la politesse.** Mesuré le 2026-10-06 : `RECENT_CONCURRENCY = 4` existait
  déjà, et quatre appels de 0,5 s se relancent **ensemble**. Le défaut n'était pas
  le débit moyen (1,3 à 2,1 req/s, correct) mais l'**écart minimum nul** : 0 ms
  sur trois rafales mesurées. Un hôte sent les rafales simultanées, pas le débit
  moyen. Ajouté `RECENT_INTERVALLE_MS` : mesuré ensuite, écart minimum **248 ms**,
  médiane 400 — donc toujours aucun départ simultané, qui est le vrai défaut.
  Corollaire de mesure : passer l'intervalle de 800 à 300 ms n'a gagné que
  **0,7 s**, alors que ne demander que les cartes **visibles** en a gagné
  **3,6 s**. Le goulot était la latence des réponses, pas l'intervalle — et la
  correction n'a paru qu'en mesurant **ce que voit l'utilisateur**, jamais le
  débit. **Recommander de renforcer un
  réglage déjà présent sans l'avoir lu est une erreur** — c'est ce que j'ai fait,
  et il a fallu ouvrir `page.tsx` pour le voir.
- **Apparier une file d'attente au DOM par l'index suppose un ordre qu'on n'a
  pas vérifié.** Mesuré le 2026-10-06 : le tri « visible d'abord » appariait le
  nᵉ bouton du DOM au nᵉ élément de la file. Faux : la file est **filtrée** (les
  entrées en cache sont sautées) et la grille est rendue dans l'ordre de
  `latestTrie`, qui est **trié**. Le tri ne triait donc rien — et la mesure
  indiquait pourtant 6/6 visibles remplies, ce qui donnait envie de le croire.
  Corrigé en exposant `data-manga-key` sur la carte : la clé vient du DOM, sans
  supposition. Corollaire : quand une mesure « réussit » alors que le
  mécanisme est faux, c'est le signe qu'elle ne teste pas ce qu'elle croit.
- **Un pic dans une seconde n'est pas un débit, et un débit moyen cache une
  rafale.** J'ai annoncé « 6 requêtes par seconde » en prenant le maximum d'une
  fenêtre glissante d'une seconde ; le débit soutenu était trois fois moindre.
  Pire : je mesurais l'**arrivée** des réponses côté interface, alors que la
  garantie porte sur le **départ**. L'écart minimum mesuré côté interface
  (304 ms) était une contradiction apparente, pas un défaut. Règle : **le journal
  du serveur est la source autoritative du débit** — Rust y écrit une ligne par
  requête réellement partie. Une mesure d'interface mesure la réponse.
- **Un compteur de débit local à un effet n'est pas un compteur de débit.**
  L'effet se relance (changement de source), le compteur repart de zéro, un
  second flux frappe en parallèle : deux horloges, aucune garantie. Mesuré :
  MANGADEX à 800 ms sur une rafale, **200 ms** sur la suivante. Une garantie
  partagée par une ressource externe (un hôte HTTP) appartient au **module**, pas
  au composant : ici une `Map` source → prochain départ.
- **Une porte de politesse ne doit pas retarder un clic.** La fonction
  d'attente ne sert que le trafic de fond. Faire attendre quelqu'un qui vient de
  cliquer, pour être poli envers le site, est l'inverse du but. Cinq sites
  appellent `chapters` ; un seul est automatique, et c'est le seul qui attend.

- **Un export ONNX de détecteur HF ne se devine pas : il se calibre sur une
  mesure.** Mesuré le 2026-10-06 sur `ogkalu/comic-text-and-bubble-detector` :
  les boîtes sortent en `[x1, y1, x2, y2]` (pas `cx,cy,w,h`), et
  `orig_target_sizes` attend **[largeur, hauteur]** (pas l'ordre HF habituel).
  Deux inversions cumulées donnaient des boîtes « lâches » couvrant 90 % de la
  fenêtre et un « IoU 0,16 » qui a coûté une session à chasser un fantôme
  (letterbox). Le calibrage qui tranche en une mesure : la boîte top-score doit
  avoir `x2>x1`, `y2>y1`, une forme de **ligne de texte**, et tomber sur une
  zone dont l'OCR lit quelque chose. Corollaire : quand une métrique de
  qualité est inexplicablement mauvaise, vérifier d'abord la **forme** des
  sorties brutes (plages par colonne), pas le modèle.
- **La langue d'un modèle se lit sur sa carte, pas sur sa licence.** L'audit
  des licences avait retenu `kha-white/manga-ocr-base` (Apache-2.0) pour lire
  l'anglais : sa carte dit « for **Japanese** text ». « HELLO WORLD » passait
  parce que le vocabulaire japonais contient l'ASCII — une fixture qui teste
  la mauvaise chose, deux fois de suite sur le même piège. Pour l'anglais,
  comic-translate utilise `ogkalu/ppocr-v6-onnx` (PP-OCR, CTC ; l'**espace est
  l'indice 18709**, juste après le dictionnaire de 18 708 entrées — mesuré en
  inspectant les indices produits pour un texte connu). Un OCR CTC n'accepte
  qu'**une ligne** : une bulle multi-lignes écrasée à 48 px rend du charabia
  ou du vide ; il faut une découpe en lignes (projection horizontale, 1,2 ms).
- **La langue d'une source n'est pas une garantie.** Mesuré le 2026-10-06 :
  MangaDex a servi du coréen, du hongrois et du portugais — dont un chapitre
  dont **les crédits étaient en anglais** mais le contenu en portugais. Seul
  un OCR d'échantillon tranche ; le titre de la série, le nom de la source et
  la première page ne prouvent rien. (Consigné aussi dans `PLAN.md` §8.)
- **Un tokenizer SentencePiece n'est pas le vocabulaire du modèle.** opus-mt :
  les `.spm` ont 32 000 pièces, le modèle en attend 59 514 — passer les ids
  `.spm` directs donne du charabia qui ressemble à du français. Il faut mapper
  pièce → id via `vocab.json`, dans les deux sens. Et le décodeur ONNX
  `decoder_model` sort **12 pasts encodeur + 12 décodeur**, mais
  `decoder_with_past_model` n'en ressort que 12 (décodeur) : les pasts
  encodeur se réutilisent de la première passe, inchangés.
- **Traduire ligne par ligne détruit le contexte ; par bulle entière le
  restaure.** Mesuré sur opus-mt-en-fr : « OH MY » + « GOSH! » séparés →
  « - Oui ! » ; joints → « Oh mon Dieu ! ». La granularité d'envoi au
  traducteur est une décision de qualité, pas de confort.

- **Un modèle d'API gratuit a trois pièges distincts, tous mesurés le
  2026-10-06.** (1) Le nom **retiré** : `gemini-2.0-flash` → 404 ; la liste
  `GET /v1beta/models` est la seule source fiable. (2) Le nom **interdit aux
  clés récentes** : `gemini-2.5-flash` → 404 « no longer available to new
  users » — listé ≠ utilisable. (3) Le modèle **« à réflexion »** : sans
  `thinkingConfig: {thinkingBudget: 0}`, gemini-3.5 brûle `maxOutputTokens`
  en pensée et la traduction arrive tronquée (« FERME-LA ! JE DO »). Et le
  quota gratuit d'une clé récente est **journalier** (~14 appels) : des pauses
  de 30 s n'y changent rien — un 429 persistant après pause = quota du jour,
  pas de la minute. Mistral gratuit : 429 dès le premier appel (compte), pas
  de contournement côté client.
- **Une clé posée par `setx` n'est pas visible du service déjà lancé.** Les
  processus héritent de l'environnement de démarrage d'OpenCode ; la clé se
  lit alors dans `HKCU:\Environment` et s'injecte dans le seul processus du
  script — jamais affichée, jamais journalisée.
- **Le nom qu'une source porte n'est pas son identifiant, et la tuile
  affiche le mauvais des deux.** Mesuré le 2026-10-07 : le moteur nomme une
  source personnalisée `custom:<id>` (`Server.kt:442`) et porte le nom lisible
  dans un champ séparé, `displayName`. Une tuile qui affiche `s.name` affiche
  donc `custom:mon-source` — et la désactivation, indexée sur ce même nom, doit
  **garder** `s.name` : c'est cet identifiant que le moteur attend en entrée.
  Corollaire : à l'intégration, lire le producteur (`listSources`) avant de
  choisir le champ affiché ; « le nom de la source » n'est pas un champ unique.
- **Des clés i18n déclarées et jamais branchées prouvent qu'un écran manque,
  elles ne prouvent pas qu'une clé est oubliée.** `pnpm i18n:check` sortait les
  **11 noms** de `sources.perso.*` + `catalogue.perso` : ce n'était pas une
  chasse à la clé morte, c'était la mesure exacte de S2.1 (0 composant, 0 appel
  de commande) alors que le type TS et le CSS existaient déjà. Une clé morte
  dans un lot en cours = l'écran n'est pas écrit.
- **Une porte qui compte les clés ne dit pas si l'écran existe.** Corollaire du
  précédent : 281 clés FR = 281 EN et « aucune clé morte » ne disent rien sur
  la moitié fonctionnelle — l'effet, la commande, le clic. C'est le smoke test
  (§S) qui la dit, d'où une section de smoke test écrite **avec** la tâche et
  laissée « jamais exécutée » tant qu'elle ne l'a pas été.
- **Un arbre de travail peut changer sous vous pendant la session.** Mesuré le
  2026-10-07 : `main.rs` est passé *modifié (+4)* → *propre* → *modifié (+4,
  mais 4 autres lignes)* entre deux commandes, sans commit, sans
  `git checkout`, sans stash et sans crochet (`.git/hooks` = 14 `.sample`).
  Explication trouvée : une **autre session** écrivait T3.6 dans le dépôt
  (`traduire_page`, `obtenir_consentement_traduction`…). Règle : ne jamais
  conclure « ces lignes ont disparu » ni « mon commit les a mangées » sans
  relire `git diff` **à l'instant de la conclusion**, et avertir l'humain quand
  deux sessions partagent le même dossier de travail.
- **L'exemple d'un document est un cas de test qui ne tourne jamais.** Mesuré le
  2026-10-07 : le défaut d'identité des sources perso (base = nom de fichier,
  moteur = `SOURCE.id`) a été trouvé **grâce à** `docs/exemple-source.js`, qui
  déclare `id: "exemple-fr"` dans un fichier `exemple-source.js` — l'exemple
  lui-même violait la convention que le code supposait. Règle : écrire
  l'exemple **avant** le reste, puis le faire passer par le chemin réel ; et
  dans un audit, **lire les fixtures et les exemples avant le code**, parce
  qu'ils sont les seules données qui traversent vraiment toutes les couches.
  Corollaire : deux couches qui doivent s'accorder sur un nom ne doivent pas
  le reconstruire chacune de leur côté (le moteur recalculait
  `${meta.id}.js` pour retrouver un fichier supprimé) — elles doivent partager
  la clé qu'elles ont réellement.
- **`:test FROM-CACHE` est un vert qui n'a rien exécuté, et `cleanTest` ne
  suffit pas.** Mesuré le 2026-10-07 : `gradle test` → `BUILD SUCCESSFUL in 5s`
  avec `:test FROM-CACHE` ; `gradle cleanTest test` → toujours `FROM-CACHE` (le
  cache build est actif, il restaure le résultat). La preuve d'exécution est
  `--rerun-tasks` (ou les XML frais) : `gradle test --rerun-tasks` →
  `6 actionable tasks: 6 executed`, 43 tests listés par leur nom, 8 XML
  réécrits. Corollaire : une porte qui rend le même verdict en 3 s qu'en
  1 min 14 n'a rien mesuré.

- **Un compteur qui bouge n'est pas la preuve que l'écran a bougé.** Mesuré le
  2026-10-08 : trois clics sur `→` faisaient passer le compteur de pages de 3 à 6
  pendant que `scrollTop` restait à 0 et que la page 1 restait affichée. La
  navigation « fonctionnait » depuis des semaines parce qu'on mesurait le
  compteur. Mesurer l'**élément visible** (`data-index` de la première image
  dans la bande), jamais l'indicateur.

- **Une garde satisfaite par une valeur nulle rend la bonne branche
  inatteignable.** `if (haut !== undefined)` est vrai pour `0` : la branche de
  repli — celle qui faisait défiler via `scrollIntoView` et qui était la seule
  correcte hors virtualisation — ne pouvait pas s'exécuter. Une garde doit tester
  la **validité** de la valeur, pas sa présence : `bandeVirtualisable ? offsets[i]
  : undefined`.

- **`align-items` du parent ne positionne pas un enfant `position:absolute`.**
  Le flux normal ne le concernait plus, donc le centrage du parent ne
  s'appliquait pas et `left:auto` le posait au bord gauche. Mesuré : l'écart
  valait exactement la marge de centrage — **315 px à 50 % de zoom, 126 px à
  75 %, 0 à 100 %** (là la page déborde). Corollaire de mesure : un défaut
  d'alignement peut n'exister que sur une partie de la plage des réglages, donc
  « ça marche à 100 % » ne prouve rien.

- **Un handle de processus n'est pas un état : `is_some()` reste vrai après la
  mort.** `assurer_demarrage` testait `enfant.is_some()`, donc un sidecar crashé
  était considéré comme démarré et **toute** requête suivante échouait
  définitivement sur `os error 232`. Le test qui compte est `try_wait()`. Mesuré :
  sidecar tué → relance automatique et analyse réussie en 163 s.

- **Un délai doit couvrir l'état le plus lent, pas l'état nominal.** Le délai de
  détection de 10 s (par tuile) était calibré sur 2,3 s par tuile **session
  chaude** ; à froid, la première inférence le dépasse (échec mesuré en 39,7 s),
  et c'est précisément la première page que l'utilisateur veut traduire. La
  parade n'est pas d'allonger le délai du chemin critique, mais de payer le
  préchauffage là où un délai long est normal et invisible.

- **Une file d'attente sans garde de génération fabrique du travail périmé.**
  Comparer le chapitre ne suffit pas : rouvrir **le même** chapitre ré-enregistre
  les jetons, l'URL est identique, et l'analyse en file partait sur un jeton
  évacué (« page inconnue »). Il faut comparer le jeton de la page.

- **Ne pas attribuer un correctif à la première cause plausible.** J'ai écrit que
  trois analyses concurrentes expliquaient le `délai dépassé`, puis mesuré : une
  seule analyse à la fois, et elle aboutit dans les deux cas. La cause réelle
  était le sidecar froid. Corollaire : une explication plausible n'est pas une
  mesure ; et une correction à conserver pour une **autre** raison (ici le débit)
  doit le dire dans le commentaire, sinon elle récupère le mérite d'un défaut
  qu'elle ne corrige pas.

- **Le marqueur d'une preuve rouge se lit dans le bundle compilé, pas dans la
  source.** La forme devinée (`() =>` avec espaces) n'existait pas après
  compilation (`()=>`), donc la réécriture ne mordait pas et le script annonçait
  `chunks reecrits : 0` — un silence indiscernable d'une preuve verte. lire le
  chunk, copier la forme exacte, et **échouer bruyamment** si le marqueur est
  absent (le script le fait désormais).

- **`execSync` passe par cmd.exe** : les cmdlets PowerShell y sont inconnus
  (« Stop-Process n'est pas reconnu »), et `powershell -NoProfile -Command "…"`
  imbriqué perd ses guillemets. Pour une action sur les processus, l'exécuter
  depuis le shell de l'agent, et laisser le script Node ne mesurer que l'app.

- **Un test qui nettoie ce qu'il veut mesurer ne peut plus crier.** Mesuré le
  2026-10-07 : le contrôle OCR de T0 comparait l'attendu et le lu **après avoir
  retiré les espaces et la ponctuation des deux côtés**. Le modèle, lui, lisait
  `YOUGOTTOGETCLOSHESFORDOGS` au lieu de `YOU GOT TO GET CLOTHES FOR DOGS` — et
  le contrôle disait vert. Le défaut n'était pas subtil : il supprimeait chaque
  espace. Corollaire : quand une comparaison « passe », vérifier **qu'elle
  distingue vraiment** les deux cas ; le moyen le plus rapide est de regarder ce
  que le test **retire** avant de comparer.
- **Le nombre de classes du modèle est la vérité, pas le fichier de
  caractères.** Mesuré le 2026-10-07 : `PP-OCRv6_small_rec` a **18 710 classes**,
  le `.txt` qui l'accompagne n'en nomme que **18 708**. L'indice 0 étant le
  blanc CTC, la classe **18709** n'était mappée par rien — et c'est **l'espace**,
  émis entre chaque mot. Corollaire : lire la dimension de sortie du modèle
  (une passe sur une entrée vide suffit) et refuser tout dictionnaire qui ne
  concorde pas, plutôt que de tronquer des indices en silence. Une garde
  `i < len(DICT)` est un suppresseur de caractères.
- **Une page de test sans texte fait accuser le code.** Mesuré le 2026-10-07 : le
  détecteur de bulles rendait 0,33 sur `2-webtoonsen/p081.jpg`, et j'allais
  conclure « le détecteur ne voit rien sur les webtoons ». La page est un plan de
  supermarché **sans une seule bulle** ; elle venait de la sonde d'OCR de T0,
  qui l'utilisait pour une autre raison. Le contrôle positif — une page de
  dialogue, 0,98 — est arrivé ensuite, et le détecteur était sain : int8 vaut
  fp32 (0,332 contre 0,350), l'agrandissement ×2 et ×3 ne change rien. Corollaire
  : **regarder l'image** avant d'accuser un modèle ; et « la page contient-elle ce
  qu'on lui demande de trouver ? » est une question distincte de « le modèle
  trouve-t-il ? ».
- **Une culture d'image ne prend pas un coin, elle prend une largeur et une
  hauteur.** `image::imageops::crop_imm(img, x, y, largeur, hauteur)` : passer
  `x + largeur` en quatrième argument donne une culture plusieurs fois trop
  grande, qui écrase le texte et le rend illisible. Mesuré le 2026-10-07 : des
  cultures de 230×370 px au lieu de 123×14 donnaient « - » et « H » là où le
  texte réel était « PROBABLY » / « THE SAME PLACE ». Corollaire : **écrire les
  cultures sur disque et les relire avec l'autre implémentation** tranche en une
  mesure — sans cela j'ai conclu que les deux implémentations divergeaient, alors
  qu'elles concordaient parfaitement et que le défaut était dans le découpage. Et
  comme rien ne vérifiait la taille de la culture, aucun test ne l'a vu.
- **Une tuile sert à détecter, pas à découper.** Mesuré le 2026-10-07 : sur une
  page de 800×1000, la meilleure bulle va jusqu'à x=819 — donc **aucune** bulle
  n'est entièrement contenue dans une fenêtre de 640, et la culture bornée à la
  fenêtre est tronquée sans le moindre signe. La culture se prend sur la page,
  la tuile ne sert qu'à trouver la boîte.
- **Un test qui compare deux chemins d'exécution différents ne prouve pas la
  parité.** Mesuré le 2026-10-07 : la sonde Python cultivait la bulle depuis la
  page (641×640 px), mon port Rust depuis la fenêtre (462×491 px). Les textes
  divergeaient, et j'ai d'abord cherché la faute dans le redimensionnement du
  modèle — qui n'était pas en cause. La vraie question n'était pas « qui a
  faux ? » mais « les deux ont-ils vu la même image ? », et ça se répond en
  exportant les cultures, pas en comparant des chaînes.
- **Un `Option::None` sérialisé en `null` casse un schéma qui ne l'autorise
  pas.** Mesuré le 2026-10-07 : `BlocTraduction` sortait `"originalText": null`,
  `"confidence": null`, `"lineBreaks": null`, et `contract/translation/schema.json`
  déclarait `"type": "string"` sans `null` — donc `PageTranslation` était
  **invalide**. Le défaut est invisible depuis T1.1 (juin) parce que rien ne
  sérialisait encore un vrai `PageTranslation` : il est apparu au moment du
  branchement, pas au moment de l'écriture du type. Corollaire : **un défaut de
  forme ne se révèle qu'à l'intégration** ; « tous les tests passent » ne dit
  pas « le type est juste », et le test de schéma validait la *fixture* sans
  passer par serde.
- **`rename_all = "lowercase"` manquant fait diverger l'enum et le schéma sans
  bruit.** Même mesure : `StatutBloc::Ok` sortait en `"Ok"`, le schéma attendait
  `"ok"`. `cargo clippy` n'a rien dit, `tsc` n'aurait rien dit, et le validateur
  de fixture — qui teste la fixture, pas le sérialiseur — est resté vert. Il a
  fallu une seconde porte qui sérialise le type Rust lui-même
  (`les_enums_rust_sont_serialisees_comme_le_schema_les_attend`).
- **Une fixture enregistrée comble la porte, mais une méthode du contrat sans
  réponse la fait crier.** Le test `chaque_methode_du_schema_a_une_reponse_enregistree`
  et son inverse `chaque_reponse_enregistree_decrit_une_methode_du_schema`
  formaient une paire : j'ai enregistré `capabilities` sans l'ajouter à l'`enum`
  `method`, et le second l'a nommé. C'est exactement l'écart
  `latest`/`fetchCover` du 2026-10-04, en miroir — et ces deux tests ensemble
  valent mieux qu'un seul.
- **Le nombre de fixtures est une porte à lui-même.** `le_nombre_de_reponses_enregistrees_est_stable`
  attendait 16 ; j'en ai ajouté 6 et il a échoué en nommant 22. Le message
  demande de noter le changement dans `fixtures/README.md` — c'est fait, avec
  un tableau de ce que chaque fixture vision prouve, dont le fait que
  `inpaint` est une **erreur volontaire**.
- **Un schéma déclaré et jamais référencé valide tout.** Mesuré le 2026-10-07 :
  `detectResult`, `ocrResult`, `inpaintResult`, `loadPackResult` et
  `healthResult` étaient dans `$defs` de `contract/protocol.json`, et **rien**
  ne les référençait — `response.result` est un simple `{type: object}`. La
  validation de contrat jugeait donc la forme de ces résultats **sans jamais la
  regarder**, et le test était vert. Comment je l'ai vu : j'ai ajouté
  `maximum` au schéma pour borner le score de détection, la porte
  `le_validateur_couvre_tout_le_schema_du_contrat` a nommé le mot-clé non géré,
  et le **contre-exemple** correspondant (« score 1.5 ») a été **accepté**.
  Corollaire : un contre-exemple qui est accepté n'est pas un test raté, c'est un
  test qui vient de prouver que la porte ne mordait pas. Et une association
  « méthode → schéma de résultat » écrite dans le test est fragile : il y a
  maintenant une porte qui refuse tout `$defs` en `*Result` non attribué.
- **Une méthode peut avoir deux formes réelles, et le schéma n'en décrivait
  qu'une.** Toujours mesuré le 2026-10-07, par le même trou : `health` rend
  `{ok, engineVersion}` pour le global et `{source, reachable}` — **sans `ok`**
  — pour le test d'une source. Les deux réponses sont authentiques et
  enregistrées depuis le 2026-10-05. Le contrat décrit maintenant les deux
  branches en `oneOf`, exclusives par `additionalProperties: false`. Une
  incohérence de contrat peut dormir des mois si personne ne valide le résultat
  correspondant.
- **Perdre une information mesurée à la frontière du contrat, c'est la refaire
  ailleurs.** Mesuré le 2026-10-07 : le détecteur de bulles rend trois classes
  (`bubble`, `text_bubble`, `text_free`), c'est-à-dire **la classification de
  T2.3**, et `detectResult` ne portait que `{boxes}`. L'application aurait donc
  dû deviner `type_bloc` — que le schéma de traduction exige. Le contrat rend
  désormais `detections: [{bbox, classe, score}]`. Corollaire : quand un modèle
  calcule quelque chose, vérifier **jusqu'où** cette information voyage avant
  d'écrire le reste ; une donnée perdue au `$defs` se retrouve réinventée en
  heuristique trois fichiers plus loin.

- **Une dépendance peut écrire « succès » et n'écrire rien.** `keyring = "3"` sans
  la feature `windows-native` compile le magasin `mock` sur Windows
  (`pub use mock as default`) : `set_password` répond `Ok`, rien n'est stocké.
  Mesuré le 2026-10-08 : `definir_cle_gemini` répondait `null`, `cmdkey /list` ne
  montrait **aucune** entrée, et la traduction en ligne retombait sur un stub —
  donc une fonctionnalité « intégrée » depuis plusieurs commits ne pouvait pas
  fonctionner. Corollaires : (a) lire le `#[cfg]` du **backend par défaut** de
  toute dépendance qui touche au système (trousseau, clé, réseau) ; (b) une
  écriture vers un système externe se prouve par un **aller-retour** dans ce
  système, jamais par le code de retour ; (c) un trait « optionnel par défaut »
  n'est pas activé par défaut.
- **Un test qui suppose l'absence d'une donnée de la machine n'est pas un test.**
  `gemini_sans_cle_retourne_local_indisponible` passait tant que le trousseau était
  un mock (toujours vide) et échoue dès qu'il devient réel : 224 passés / 1 échoué
  avec une clé, 225 passés après suppression. Même famille que « une mesure qui
  dépend de l'état du disque » : isoler par injection, pas par dislocation de la
  machine de test.
- **`browser.close()` sur une connexion CDP ferme l'application.** En
  `connectOverCDP` le navigateur distant **est** la webview : le `close()` de mon
  script a tué `tatami-desktop.exe` (mesuré : `ECONNREFUSED` sur 9223 au tour
  suivant). Pour lire une webview, sortir du script sans fermer.
- **Un paramètre nommé `$args` avale les arguments, et cargo répond 0.** Ma porte a
  affiché l'aide de cargo (« See 'cargo help …' ») et renvoyé **0** : une porte
  verte qui n'a rien exécuté. Mon bilan a même annoncé « tout passe », parce que la
  fonction renvoyait aussi ses lignes de log et que la somme portait sur du texte.
  Corollaires : ne pas nommer un paramètre `$args` ; écrire le journal avec
  `Write-Host` (hors pipeline) pour que seule la valeur de retour sorte ; et lire
  **la sortie** d'une porte, pas son seul code.
- **Un exécutable GUI lancé en shell avant-plan ouvre une fenêtre console, et il
  peut ignorer le drapeau qu'on lui donne.** `tatami-desktop.exe --version` n'a pas
  affiché de version : il a lancé l'app, et mon shell l'a attendu 120 s. La cause
  est `main.rs:1`, `#![cfg_attr(not(debug_assertions), windows_subsystem =
  "windows")]` : **en debug la console reste attachée**, donc chaque instance a
  son `conhost.exe`. Pour piloter sans fenêtre : `Start-Process -WindowStyle
  Hidden` avec les journaux redirigés dans des fichiers, puis CDP.
- **Un parent déjà mort ne dit rien du lanceur : il faut l'attraper au démarrage.**
  Trois « Tatami se lance sans mon accord » ont eu des parents différents, tous
  déjà terminés au moment de la mesure ; `Win32_ProcessStartTrace` avec résolution
  immédiate de la chaîne d'ancêtres a, lui, nommé le lanceur. Corollaire de
  méthode : sur une webview, `.first()` et le conteneur d'ancêtres sont peu
  fiables — deux fois le clic a visé la ligne voisine, parce que le plus grand
  ancêtre contenant le titre englobait toute la liste. Partir de l'élément dont
  l'`aria-label` est **unique** et remonter au plus petit conteneur utile.

- **Un titre romanisé, un filtre de langue de source ou un badge « EN » ne prouvent
  pas la langue du contenu.** Mesuré deux fois le 2026-10-08, sur deux sources
  différentes : `MANGADEX` étiquetée `INT` a servi une série **espagnole**, puis
  `TOOMICSEN` étiquetée `EN` a servi un webtoon dont les **cartes de titre sont
  en anglais** et les dialogues en espagnol. J'ai donc pris trois pages « anglaises »
  sur cette base et conclu qu'EN→FR était validé — alors que je n'avais mesuré que
  des cartons de titre. Corollaires : (a) la source sert souvent **plusieurs
  groupes de langue**, l'application en montre un seul, et le groupe affiché n'est
  pas déductible du nom ; (b) **la seule mesure de la langue, c'est la sortie de la
  vision sur un bloc de dialogue** — les titres de série sont bilingues par nature,
  donc inutilisables ; (c) un test de traduction sur une page non vérifiée ne prouve
  rien, et une traduction correcte d'une page espagnole ne prouve pas EN→FR.
  Vérification à faire **avant** de dépenser un appel : lire les blocs `dialogue`
  renvoyés par `analyser_page` et les juger par leurs mots (anglais : *the, and,
  is, you* ; espagnol : *el, la, que, de, por, más*), puis **refuser** la page si le
  verdict n'est pas net. Un contrôle coûtant un appel, il doit précéder l'appel.
- **Une capture peut changer l'état qu'elle ostensibly mesure.** Dans le lecteur,
  la position affichée est lue dans la barre d'outils ; mes captures successives
  l'ont fait reculer (5 → 3 → 1), et les réponses `traduire_page` arrivées entre deux
  captures se retrouvaient donc attribuées à la mauvaise page — jusqu'à afficher une
  traduction dans le rapport qui n'était pas sur la page montrée. Corollaire :
  **lire les mesures avant de capturer**, puis relire l'index après la capture pour
  détecter la dérive ; et corréler par `[data-page="N"]`, jamais en comptant toutes
  les boîtes de l'overlay (qui contient les pages de la fenêtre virtuelle).

- **L'encodeur Marian ne rend qu'UNE sortie, et c'est une correction du modèle
  documenté.** Mesuré le 2026-10-08 sur les trois `.onnx` de
  `Infomaniak-AI/onnx-opus-mt-en-fr` (`examples/sonde_onnx_traduction.rs`) :
  l'encodeur sort `last_hidden_state` [1, n, 512] et **rien d'autre** — les 12
  passés encodeur ne sortent pas de lui mais du **décodeur**, en
  `present.N.encoder.{key,value}`. Et `decoder_with_past` ne rend que **13**
  sorties (`logits` + 12 `present.*.decoder.*`) : les passés encodeur ne sont
  pas réémis, et c'est correct puisque la source ne change pas. Corollaire de
  code : la boucle fusionne les passés **par nom** et ne remplace jamais la
  liste entière ; le nombre de couches est lu dans les sorties, jamais écrit en
  dur (6 ici, mais rien ne l'impose). Généralisation : **la fiche HuggingFace et
  le `.onnx` exporté ne décrivent pas la même chose** — seul le `.onnx` fait foi.

- **`generation_config.json` n'est pas dans le manifeste du pack.** Les valeurs
  (`decoder_start_token_id: 59513` = `<pad>`, `eos_token_id: 0`,
  `bad_words_ids: [[59513]]`, `max_length: 512`, `num_beams: 4`) sont donc des
  **constantes en Rust**, chacune accompagnée de la mesure qui la fonde. Ce
  n'est pas une recopie : c'est un report assumé, parce que le fichier ne sera
  pas installé. Et `bad_words_ids` doit être **appliqué** : sans cette
  interdiction, une boucle gloutonne peut émettre `<pad>` au lieu de `</s>` et
  s'installer au lieu de s'arrêter. Test rouge prouvé : en neutralisant
  l'interdiction, `le_jeton_interdit_ne_gagne_jamais` échoue en nommant
  `left: 1, right: 2`.

- **`cargo check` passe là où `cargo clippy` refuse.** Mesuré le 2026-10-08 :
  un champ `Option<Arc<SuperviseurVision>>` dans une structure
  `#[derive(Debug)]` **compilait** avec `cargo check` et **échouait** avec
  `cargo clippy --all-targets -- -D warnings`, parce que `SuperviseurVision`
  n'implémente pas `Debug`. Corollaire : `check` n'est pas une porte, c'est un
  premier passage ; et plutôt qu'ajouter `#[derive(Debug)]` au superviseur
  (canaux, verrous, processus), on écrit un `Debug` à la main qui dit ce
  qu'on veut savoir — « branché : oui/non ».

- **Une sonde qui écrit les octets de l'appelant attrape ce qu'aucun test
  unitaire n'attrape.** Mesuré le 2026-10-08 : la traduction fonctionnait
  (2/5 bulles identiques, 95 ms/bulle), et la première sonde NDJSON
  `hello → loadPack → capabilities → translate` a répondu « `loadPack` attend
  un paramètre `path` » alors que j'avais écrit `root`. Ni le `cargo test` du
  sidecar, ni la compilation du cœur ne l'auraient vu. Corollaire : quand une
  chaîne traverse un **contrat**, la vérifier par une sonde qui écrit
  exactement le format de l'appelant — pas en appelant la fonction Rust de
  l'autre côté.

- **Une traduction *différente* n'est pas une traduction *fausse*.** Mesuré le
  2026-10-08 sur les 5 bulles de `docs/spike-traduction.md` §8.5 : 2
  identiques, 3 différentes — et les 3 sont des traductions plausibles
  (« Comment pourriez-vous apprendre votre enfant comme ça »). C'est ce qui
  **prouve** que l'alignement des passés est juste : un décalage de tenseur ne
  donne pas du charabia, il donne du bruit. Corollaire de jugement : le signal
  qui tranche n'est pas « identique ou non », c'est « lisible ou non ». Et la
  référence venait d'un décodage **en faisceaux** alors que la boucle est
  gloutonne : un écart y est donc **attendu**, et il faut le dire plutôt que le
  traquer.

- **Le consentement porte sur le réseau, pas sur la fonctionnalité.** Le refus
  de « traduction en ligne » bloquait aussi le modèle local, donc le seul chemin
  qui fonctionne hors connexion était inatteignable. Même famille que le
  réglage que Rust applique : ce que le garde porte decides où se pose la
  question, et une garde appliquée trop large supprime une fonction entière au
  lieu de la protéger.

- **Un refus qui ne dit pas pourquoi fait passer une panne d'accès pour une
  donnée manquante.** Mesuré le 2026-10-09 sur `custom:mangadex-api` : le journal
  portait `HTTP 404 pour cette page`, et j'en ai conclu **trois** fois à une
  panne — le moteur « pas à jour », puis « le CDN ne sert qu'en HTTP/1.1 » (les
  deux fois infirmé, la seconde après avoir écrit le correctif et son test), puis
  « des pages n'existent pas sur le CDN ». Les trois étaient fausses. La
  cause réelle : **MangaDex bloque l'API en TLS depuis cette machine** —
  `api.mangadex.org` et `mangadex.org` en `http=000` avec un handshake
  `SEC_E_ILLEGAL_MESSAGE`, quand le nœud d'images répond 200 et que tous les
  autres sites répondent. Les URL d'images étaient produites par cette API
  morte.
  Corollaire : **`404 sans corps` n'est pas une page d'erreur, c'est un refus de
  connexion** — il se distingue d'une image manquante par son type de contenu
  absent. C'est ce qui a permis de poser la question utile, et il a fallu
  l'ajouter au refus (`DiagnosticImageTest`) : le message seul ne suffisait pas.
  Corollaire général, le plus important des trois : **quand une URL échoue,
  demander d'abord « qui me l'a donnée, et ce producteur répond-il ? »** Une URL
  est une valeur *reçue* ; je l'avais testée avec `curl` sans jamais tester
  l'API. Le piège est que les deux moitiés de la chaîne peuvent être coupées
  séparément — ici l'API morte, les images vivantes — ce qui produit un mélange
  de 404 et de 200 sans qu'aucune donnée ne manque. **Avant d'accuser la
  donnée, vérifier que son producteur répond.**

- **Une porte peut mordre et rester verte sur le mauvais chemin.** Mesuré le
  même jour : le test « le refus ne fuit aucune URL » passait **avant** le
  correctif — il ne pouvait pas échouer, puisque rien n'était encore écrit.
  Il n'a mordu qu'une fois l'assainissement supprimé à la main. Et le test de
  longueur ne mordait qu'en retirant **deux** bornes d'un coup : l'une était
  redondante pour ce test. Corollaire : un test qui passe avant le correctif
  est une porte dont le mors n'est pas encore monté ; la seule façon de savoir
  s'il mord, c'est de casser le code qu'il protège et de lire **le nom du
  test qui échoue** — pas le nombre de tests verts.

- **Le lecteur affichait une image cassée sans rien dire, et c'était le seul
  défaut qui nous restait.** Mesuré le 2026-10-09 sur `custom:mangadex-api`
  (Dragon Ball, chapitres FR), **par le même code et dans la même seconde** :

  | chapitre | pages annoncées | servies |
  |---|---|---|
  | ch. 1 | 32 | **7/8** sondées, ~200 Ko de JPEG chacune |
  | ch. 3 | 15 | **0/8** sondées, toutes refusées en 404 |

  MangaDex sert certains chapitres et en perd d'autres : ce n'est pas notre
  erreur, et **aucun code à écrire ne le changera**. Mais l'absence de message
  en était une : l'utilisateur lisait « la source ne sert pas cette page »
  comme « l'application est cassée », et rien à l'écran ne permettait de les
  distinguer. Corollaire : **dire « je ne sais pas » est un correctif.** Le
  404 est rapporté avec `[type=inconnu] sans corps` (`79d90c8`), ce qui permet
  d'afficher « page indisponible chez la source » — vrai, et vérifiable — au
  lieu d'une `<img>` muette. La distinction compte : un message qui accuse le
  réseau serait faux, et le prochain qui le lirait cherishait cette fausse
  piste comme moi.

- **Cinq tokens CSS que j'ai inventés, et que le navigateur aurait acceptés.**
  J'ai écrit `--t-petit`, `--r2`, `--fond-doux`, `--texte-doux`, `--bordure` :
  aucun n'existait. Le rendu aurait été « correct » — une valeur invalide
  retombe sur l'héritée — et **aucune porte ne l'aurait vu**. Le nom réel
  était `--text-2`, `--surface-2`, `--line`, `--r`, et il existait déjà une
  paire `notice-*` pour un avertissement. Corollaire : un `var(--x)` non
  défini ne casse rien, donc il ne se signale jamais ; la seule vérification est
  de **chercher le token dans `globals.css` avant de l'écrire**, car le grep
  coûte une seconde et la faute coûte une lecture de CSS qui ne mène nulle
  part.

- **Une porte peut rester verte parce que sa regex est fausse, pas parce que
  le code est bon.** Mesuré le même jour : en retirant le `onError`, la porte
  est restée verte — j'ai d'abord conclu « ma neutralisation n'a rien fait »,
  ce qui était vrai, mais j'allais enchaîner sur « la porte ne mord pas ». La
  porte en réalité ne cherchait pas le bon motif : `<img\b[\s\S]*?/>` s'arrêtait
  à la première `/>` **interne**, donc jamais au `/>` de la balise. Elle a été
  réécrite pour signaler séparément « balise de page sans `data-index` », ce
  qui rend le diagnostic exact au lieu de dire « aucun <img> trouvé » alors
  qu'elle en voyait un. Corollaire : **quand une porte reste verte après une
  casse, vérifier d'abord que la casse a eu lieu** (`git diff`, lecture du
  motif) avant de conclure sur la porte. C'est le même réflexe que pour le
  `mtime` conservé par `Move-Item`.

- **Un `invoke<Type>` est une affirmation, que rien ne vérifie.** Mesuré le
  2026-10-09 : `type Progression` déclarait `chapterUrl`, `chapterTitle`,
  `pageIndex`, alors que `HistoryEntry` ne porte qu'un `#[derive(Serialize)]`
  **sans** `rename_all` — serde envoie donc les champs tels qu'écrits, soit
  `chapter_url`, `chapter_title`, `page_index`. `progression.chapterUrl` valait
  `undefined`, `repriseUtile()` renvoyait `false`, et **ni le marqueur de chapitre
  courant ni le bouton « reprendre » n'apparaissaient** — exactement les deux
  défauts que l'utilisateur avait signalés. `tsc` est resté **vert sur les deux
  versions** : le paramètre générique de `invoke` est une déclaration de
  l'auteur, jamais une conversion. Corollaire : `catch(() => setX(null))` rend
  un échec d'IPC **indistinguable** d'une valeur légitimement absente — d'où les
  deux hypothèses concurrentes (écriture fausse / lecture fausse), tranchées en
  consultant la base directement (`node:sqlite` en builtin de Node 26) plutôt
  qu'en devinant. Gardes ajoutées : `pnpm ipc:check` (champs d'un type TS
  comparés à ceux de la struct Rust qu'il décrit, **paires déclarées** et non
  devinées : une heuristique sur tous les `invoke<T>` ne distingue pas un type de
  charge utile d'un type local, et produirait des faux positifs qu'on finirait
  par ignorer) et `les_champs_de_history_entry_sont_en_snake_case` (les noms
  **émis**, côté producteur). Les deux se lisent mutuellement — même famille que
  `les_champs_signes_concordent_avec_le_script` : une valeur partagée entre deux
  langages a besoin d'un test qui lit l'autre fichier.

- **Un attribut `#[test]` en double ne fait pas perdre un test, il n'en ajoute
  aucun — et `cargo test` le voit vert.** Mesuré le 2026-10-09 : deux `#[test]`
  sur la même fonction, laissés par une insertion de bloc précédente. Résultat :
  **162 tests passants**, comme si de rien n'était, et
  `cargo clippy --all-targets -- -D warnings` qui refuse de compiler sur
  `duplicated attribute`. Corollaire général : **`cargo test` n'est pas une porte
  de compilation du code de test** — c'est `clippy --all-targets` (cf. la leçon
  où `check` n'est pas une porte). Et l'attribut parasite venait d'une écriture
  par index : relire la ligne **suivante** du bloc inséré.

- **Arrondir une géométrie ne la rend pas plus propre : cela déplace l'erreur
  *dedans*, où elle devient visible.** Mesuré le 2026-10-09 sur le lecteur : la
  boîte d'une image faisait `height: 438px` alors que le ratio exact à 630 px
  de large valait **437,5**. `object-fit: contain` centre l'image dans la boîte
  et laisse donc **0,25 px de fond en haut et en bas** — un trait horizontal fin
  **à chaque frontière de page**, visible sur fond sombre. Ce n'était **pas** un
  écart entre deux images : la mesure d'« écart entre images » valait 0, parce
  qu'il n'y en avait pas. Corollaire : **une mesure d'écart entre deux boîtes ne
  voit pas une lame *à l'intérieur* d'une boîte** — il faut comparer la boîte à
  ce qu'elle devrait contenir, donc au ratio. Et l'invariant qui protège le
  remède vaut mieux que le remède :
  `hauteurRendue(p, l) === hauteurPage(p, l)`, testé sur 25 couples. Si les deux
  divergeaient, le demi-pixel irait ailleurs — mais il resterait visible.
  Dernière leçon : ici, la **capture d'écran de l'utilisateur** a montré ce
  que mes deux mesures avaient manqué ; un cadre (`écart`, `largeur`) ne dit pas
  la **forme** du défaut.

- **Le badge de développement se lit « footer inutile » dans une capture.**
  Mesuré le 2026-10-09 : il n'y a aucun `<footer>` dans le lecteur ; le « footer
  » en bas à gauche est l'indicateur de `next dev`, posé **par-dessus la page de
  lecture**. Il n'existe pas dans l'application livrée — donc le signaler comme
  un défaut de l'application aurait été faux, et le corriger en CSS l'aurait
  laissé intact. Corollaire : **une capture prise en dev ne décrit pas
  l'application**, et ce qu'elle montre peut être un outil de développement.
  Le lire avant d'en faire un défaut de mise en page.

- **Une flèche de navigation et une flèche de défilement ne sont pas le même
  geste, et les mettre dans le même tableau rend les deux mauvais.** Mesuré le
  2026-10-09 : `ArrowDown`/`ArrowUp` étaient rangés avec `ArrowLeft`/`ArrowRight`
  et appelaient `scrollToPage(index ± 1)`. Sur une bande continue, un appui
  valait **une page entière** (18 629 px) — et le premier ne déplaçait **rien** :
  `scrollTop` restait à 0 pendant que le compteur passait de 1 à 2, parce que
  l'ancrage de page ne peut pas se satisfaire quand la page courante est plus
  haute que le cadre. Un appui qui consomme un chapitre sans le faire bouger est
  pire qu'une touche morte : on croit avoir lu. Corollaire : **un `preventDefault`
  suivi d'un `scrollTo` qui ne bouge pas est un défaut, pas un cas limite** — le
  mesurer au `scrollTop`, jamais au compteur de pages (cf. « un compteur qui
  bouge n'est pas la preuve que l'écran a bougé »).

- **Une hauteur de repli affichée en bas d'une liste est un footer, et elle n'est
  visible que parce qu'elle est réservée *avant* d'être mesurée.** Mesuré le
  2026-10-09 : la bande réservait `HAUTEUR_REPLI` (1 115 px) aux pages de queue
  qu'elle n'avait pas encore montées ; au fond, **1 062 px de vide** sous la
  dernière image. Ce n'était pas un calcul faux mais un **artefact de
  chargement** — 6 s plus tard la bande retombait de 57 314 à 55 440 px et le
  vide disparaissait seul. Remède : la fenêtre de virtualisation **monte toujours
  la dernière page**, donc elle décode tôt et `onLoad` la mesure ; une image de
  plus, pas la queue. Corollaire de mesure : une sonde qui **saute** au fond
  reproduit l'artefact, pas l'usage — il faut descendre **par paliers**. C'est
  pour ça que la règle vaut aussi pour le `loading="lazy"` : « monté tôt » est une
  garantie vérifiable (un test sur `fenetreBande`), « invisible » ne l'est pas.

- **Un garde qu'aucun test ne fait échouer n'est pas un garde — parfois il
  est du code superflu.** Mesuré le 2026-10-09 : j'ai ajouté
  `if (premier >= nbPages)` pour couvrir l'ancre hors bande, puis je l'ai
  désactivé pour prouver qu'il mordait. **Les tests sont restés verts** : la
  garantie voisine couvrait déjà le cas. J'ai donc **supprimé** le garde, plutôt
  que de garder une protection que rien ne vérifie. Corollaire : « ça ne mord pas »
  ne dit pas seulement *que* la porte est inutile — ici, la bonne conclusion était
  *supprimer*, pas *ajouter un test qui morde*. Une porte verte après une casse
  est un signal, et il faut lui demander ce qu'il désigne.

- **Un journal est cumulé : relire « les 20 dernières lignes utiles » peut
  afficher un résultat d'il y a deux heures.** Mesuré le 2026-10-11, **trois fois
  de suite** : mon script de verdict a lu un `403 cf-mitigated` écrit à 12h48 —
  une heure avant le lancement de l'app à 13h22 — et a conclu « le rejeu est
  refusé ». Le rejeu n'avait jamais eu lieu. Les deux tells étaient là : le
  `cf-ray` datait de la veille, et la requête n'avait **aucun** `cf_clearance`.
  Corollaire : **noter la position de départ** (ligne, octet ou horodatage) et
  n'afficher que ce qui est écrit après. Une mesure sans fenêtre temporelle
  n'est pas une mesure, c'est une recherche dans le passé.

- **« Refusé » et « la porte d'entrée » donnent le même statut HTTP.** Mesuré le
  2026-10-11 : la requête qui **déclenche** le challenge répond
  `403 cf-mitigated=challenge`, et celle qui **rejoue** la clearance répond
  exactement la même chose. En se fiant au statut seul, on annonce un refus
  après avoir seulement vu s'ouvrir la fenêtre — ce que j'ai fait. Ce qui
  distingue les deux n'est pas le code de retour : c'est **ce que la requête
  porte** (`cf_clearance` dans le Jar, UA de la fenêtre qui a résolu). Règle
  générale : devant un verdict, écrire d'abord **ce qui doit être vrai pour
  qu'il soit vrai**, et le vérifier dans la trace. Sinon on mesure la porte
  d'entrée et on conclut sur la sortie.

- **Un test de câblage doit lire l'objet qui travaille, pas la fonction qui
  l'a construit.** En posant Conscrypt (`PileTls`) : les 2 tests qui appellent
  `PileTls.construire()` restent verts même si le builder **oublie** la
  fabrique — c'est le repli silencieux, il ne lève rien. Les 3 qui lisent
  `client.sslSocketFactory` tombent rouges. C'est cette différence qui prouve
  que la porte voit le **câblage**, et pas seulement la présence du JAR dans le
  classpath. Même famille que « une porte qui ne lit qu'un seul fichier » : lire
  le symptôme, pas le constructeur.

- **`gradle build` ne produit pas `engine/dist/`.** Le plugin `application` écrit
  dans `build/install/<nom>` ; c'est `scripts/build-engine.ps1` qui copie vers
  `dist/` après un `jlink`. Un `gradle test` vert laisse donc `dist/` avec les
  JAR **de la compilation précédente** : après avoir ajouté une dépendance, le
  seul moyen de savoir ce que l'application embarque est de regarder
  `dist/tatami-engine/lib`. C'est encore « corriger la source ne prouve pas que
  le binaire l'exécute », mais par un chemin plus discret.

- **Un marqueur peut vivre dans une lambda, pas dans la classe englobante.**
  Mesuré le 2026-10-11, deuxième fois dans la même séance : le marqueur
  `enTetesDeclares` était cherché dans `EngineContext.class`, et « absent ». Il
  était dans `EngineContext$special$$inlined$-addInterceptor$1.class` — le code
  est dans une **lambda d'intercepteur**. Conclusion « pas dans le JAR », fausse,
  et elle a failli faire réécrire du code déjà livré. Corollaire : chercher un
  marqueur dans **toutes** les classes du dossier, pas dans la classe dont le nom
  est celui du fichier source. Et rester attentif au **deuxième** problème de la
  même mesure : le JAR que j'avais ouvert datait de 45 minutes avant la dernière
  compilation.

- **Une porte élargie à un nouveau dossier se prouve en retirant l'usage du
  module concerné.** `check-i18n.mjs` ne lisait que `app/` ; `describeError` a
  été extrait vers `lib/` et ses huit clés `erreur.*` ont été déclarées mortes.
  Première tentative de preuve rouge ratée : la clé restait citée dans la
  **union de type** `CleErreur`, que la passe `viaTable` compte comme usage. Il a
  fallu retirer les **deux** occurrences — union et appel — pour que la porte
  crie `erreur.verification.non.rejouable`. Même famille que « vérifier que la
  casse a eu lieu avant de conclure sur la porte », mais ici la casse partielle
  est **silencieusement insuffisante**.

- **Un test qui dit « le test lit ce que le moteur lira » alors qu'il le prouve
  pas.** Mesuré le 2026-10-14 : `scripts/test-source.mjs` chargeait une source
  paresseuse avec `vm.runInContext`, et son commentaire affirmait « `vm` plutôt
  qu'un `eval` bricolé, pour que le test lise exactement ce que le moteur
  lira ». Le moteur est **Nashorn 15.6** (`engine/build.gradle.kts`) ; `vm` est
  **V8**. Les trois sources écrites ce jour-là ont donc passé le test et ont été
  **rejetées au chargement**, sur `<eval>:115:8 Expected an operand but found )`
  — une virgule finale dans les arguments d'un appel, de l'ES2017. Corollaire :
  un test qui rejoue du code hors de son moteur d'exécution **ne prouve que le
  code**, jamais son évaluation ; et « le moteur recharge les sources à chaud »
  est faux ici aussi — il y a bien un `boucleWatcher`, ce qui a rendu le défaut
  visible dans le journal au lieu de le laisser silencieux. La seconde moitié de
  la leçon : une **regex qui exige un `href` relatif** ne peut pas mordre un site
  qui sert des URL absolues. Les trois sites servent `href="https://…/manga/…"`,
  la fixture portait la forme relative, et le test hors ligne fut vert. Mesure :
  les trois catalogues répondent 200 avec 24 / 13 / 24 liens, et 0 match.

- **Le nom de contrat d'une source perso est `custom:` + le nom de fichier, et
  la sonde ne pouvait pas en sonder une.** `Server.kt` compose `CUSTOM_PREFIX`
  avec `identiteDe(fichier)` (le nom de fichier sans `.js`) ;
  `dispatchCustom` retire le préfixe, donc un nom sans `custom:` part sur le
  chemin **natif** et répond « unknown custom source ». Or `sonde-sources.mjs`
  filtrait `custom:` — y compris ceux passés explicitement par `--sources`, donc
  `--sources custom:x` sondait **0 source** en Silence. Corollaire : une liste
  demandée puis vidée par un filtre est le piège « 0 résultat ne prouve rien » ;
  la sonde le crie désormais. Et `SOURCE.id` n'est **pas** l'identité : il est lu
  et validé, mais c'est le nom de fichier que la base et l'interface connaissent.

- **Un rejeu supposé peut être la valeur réelle du site, et le « byte à byte » ne
  le démontrait pas.** Mesuré le 2026-10-14 : après redémarrage de
  l'application **et** du sidecar, `webview_lire` sur HENTAIZONE rendait
  **exactement 6 201 octets**, comme six heures plus tôt — ce qui accablait la
  mémoire du moteur. Or trois `fetch` hors application donnent **403 « Just a
  moment… », 5 710 octets**, et la webview en a 6 201 : deux identités de client,
  deux variantes de la même page de vérification. La réponse était donc la
  **vraie réponse du réseau**, à cet instant. Corollaires : (a) une valeur
  constante n'est pas la preuve d'un rejeu, c'est la preuve d'un *déterminisme*,
  et il faut demander de quoi ; (b) le témoin qui tranche est la mesure
  **hors application** du même endpoint, pas une deuxième exécution de la même
  chaîne ; (c) `cf_clearance` était **présente** (295 cookies, dont
  `.hentaizone.xyz`) et Cloudflare refusait quand même — l'absence de cookie
  n'était donc pas la cause, contrairement à ce que la lecture du journal
  suggérait.

- **Le chemin du transport n'est pas celui qu'on croit, et le log le dit.** Les
  mesures de 14 h 20 sortaient de `challenge::webview_lire`, une **commande
  Tauri** qui appelle `lire_par_transport` directement. Or `latest` passe par
  `Supervisor::request_avec_transport`, qui **journalise** `transport: …` dans
  ses deux branches (succès et échec). Aucune ligne de ce type n'existe dans le
  journal, jamais — donc ces mesures ne passaient pas par ce chemin, et un
  `invoke('latest')` de sondage n'y entre pas. Règle : avant de sonder une
  chaîne, vérifier **quelle ligne de log elle écrit** et la retrouver dans le
  journal ; si elle n'y est pas, on ne mesure pas le chemin qu'on croit.

- **`transport_a_tenter` est testée et verte, et `latest` ne l'a pas empruntée.**
  La fonction est **pure**, ses tests passent sur le message exact du journal —
  et pourtant `latest` a rendu l'erreur d'origine en 516 ms, sans passer par le
  transport. Ce n'est donc pas « la fonction a été oubliée » : c'est une
  **contradiction mesurée** entre un test vert et un comportement réel. Elle
  mérite un nom et une cause avant toute correction, pas un `if` de plus.

- **Une app en `cargo run` garde son serveur de dev par un autre arbre de
  processus.** Mesuré le 2026-10-14 : en arrêtant `tatami-desktop.exe` **et** son
  parent `cargo.exe`, le port 3001 est tombé aussi — la webview a alors chargé
  `chrome-error://chromewebdata/`, dont `location.origin` vaut **`null`**, et
  tout `invoke` répondait « Origin header is not a valid URL ». Une erreur
  d'IPC qui accuse l'origine est souvent une **page morte** : lire
  `location.href` avant de conclure quoi que ce soit sur l'IPC.
