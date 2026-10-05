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
