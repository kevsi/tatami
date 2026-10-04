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
4. Smoke test manuel documenté dans `docs/smoke-test.md` (à créer en phase 0), exécuté avant chaque jalon.

**Commandes de vérification** (à confirmer et figer à la fin de la phase 0, puis les garder à jour ici) :

```powershell
pnpm lint
pnpm typecheck
cargo fmt --check
cargo clippy -- -D warnings
cargo test
./gradlew test
```

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
- **Une source qui répond à `pages` peut refuser les images.** REMANGA et SEIMANGA renvoient leurs pages mais `fetchImage` répond 403 (le CDN refuse l'user-agent du moteur). Vérifier l'image, pas seulement la liste.
- **Les nombres flottants des sources sont des identifiants, pas des données.** Une division flottante du site arrivait jusqu'à l'écran (« Ch. 38.09999847… ») : arrondir avant d'afficher, et `Math.round` plutôt que `Math.floor` pour comparer.
- **Depuis Next 16, `next lint` n'existe plus** et `next build` ne lance plus le linter. Il faut `eslint.config.mjs` (flat config : `defineConfig` + `globalIgnores` + `eslint-config-next/core-web-vitals`) et le script `lint` pointant sur le binaire `eslint`.
- **Le peer `eslint >=9` d'`eslint-config-next` est trompeur en pratique** : son `eslint-plugin-react@7.37.5` plante sur ESLint 10 (`contextOrFilename.getFilename is not a function`). Épingler ESLint **9** tant que le plugin n'est pas mis à jour.
- **Une directive `eslint-disable` signalée « inutilisée » peut en masquer une vraie** : ESLint ne signale qu'un avertissement par directive. En supprimer une a fait apparaître 2 vrais `missing dependency`. Lire ce que l'avertissement révélait avant de mass-supprimer.
- **`role="tab"` et `aria-pressed` sont incompatibles** (`jsx-a11y/role-supports-aria-props`). Pour une navigation entre vues, `aria-selected` est le bon attribut — et le sélecteur CSS doit suivre, sinon l'onglet actif cesse d'être mis en valeur.
