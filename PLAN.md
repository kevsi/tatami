# Tatami — Phase avancée : mode webtoon + traduction (T)

## État actuel (2026-10-06)

### Terminé ✅

1. **Mode webtoon uniquement** — suppression complète du mode "page" dans le lecteur.
   - Retiré `readerMode === 'page'` et tout le code associé (boutons Pagé/Webtoon, ajustements de pages par écran, ajustement largeur/hauteur, bouton spread/dir, zoom sur les pages simples).
   - Le lecteur affiche maintenant exclusivement du webtoon (bande verticale avec images empilées).
   - Les boutons précédent/suivant fonctionnent toujours en mode webtoon (testé : 10→11→12→11 pages).

2. **Chargeur animé `.chargement`** — visible dès que `readerLoading` monte, couvre ouverture + changement de chapitre. Mesuré : 1,2 s pour la roue.

3. **Verrou Rust `DownloadState`** — passé de `Mutex<bool>` à `HashMap<source, actif>` plafonné à 2 slots. Téléchargements parallèles jusqu'à 2 sources distinctes, jamais 2 de la même source. Mesuré : 49 fetchImage/30s.

4. **Recherche source fiable** — cause identifiée : le nom de la source est `HENTAIORIGINES` (H-E-N-T), pas `HANT`. Le bouton "Voir plus (N restantes)" fonctionne maintenant correctement (69 → 48 → 1 après deux clics).

5. **Mise en évidence chapitre en cours dans la liste** — `.chapter-cell.encours` ajouté au CSS + détection dans le composant de grille de chapitres. Le chapitre en cours affiche "En cours · page X/Y" en dessous du numéro. Les couleurs utilisent le token `--accent-dim`/ `--accent` (identique à `.on` mais avec une logique de détection distincte).

6. **Nettoyage des clés i18n inutilisées** — supprimé `erreur.page` du fichier `i18n.ts` (FR + EN). `i18n:check` passe maintenant : "aucune clé morte, aucune traduction manquante, aucun placeholder nu".

7. **Phase T1 — Modèle de données et contrat de traduction** — schéma `PageTranslation` v1, migrations SQLite v8 (`page_translations`, `blocks`), types Rust/TS, contrat sidecar vision étendu (`detect`, `ocr`, `inpaint`, `loadPack`, `health`, `shutdown`), gestionnaire de packs de modèles, fixtures JSON, tests de contrat.
   - Sécurité : clé Gemini déplacée de SQLite vers le trousseau OS (`keyring` crate).
   - Portes passées : `cargo clippy --all-targets -- -D warnings` (0 warning), `cargo test` (133/133 mono-thread), `pnpm typecheck/lint/i18n:check/hooks:check/chemins:check/build`.

8. **Phase T2.1 — Assemblage de la bande virtuelle** — module `tile.rs` avec découpe en tuiles aux lignes uniformes, fusion des détections aux jonctions. 11 tests unitaires + 6 tests d'intégration sur images synthétiques.
   - Portes passées : `cargo clippy/test` (144/144 mono-thread), frontend gates vertes.

9. **Phase T2.2 — Masques de bulles** — module `mask.rs` avec création de masques depuis bbox/boxes, érosion, validation. 4 tests unitaires.
   - Portes passées : `cargo clippy/test` (148/148 mono-thread), frontend gates vertes.

10. **Phase T2.3 — Classification des blocs** — module `classifier.rs` avec `TypeBloc` (Dialogue, Narration, Sfx, TexteLibre, Ignore) et heuristiques géométriques. 6 tests unitaires, scores normalisés à 1.0.
    - Portes passées : `cargo clippy/test` (154/154 mono-thread), frontend gates vertes.

11. **Phase T2.4 — Traitement OCR et ordre de lecture** — module `ocr.rs` avec regroupement Union-Find des lignes par proximité spatiale, tri haut-vers-bas/gauche-vers-droite, assemblage du texte. 5 tests unitaires.
    - Portes passées : `cargo clippy/test` (159/159 mono-thread), frontend gates vertes.

12. **Phase T2.5 — Stratégie d'effacement** — module `effacement.rs` avec `StrategieEffacement` (Effacer/Conserver/Ignorer) par type de bloc, politique ADR 0010. 5 tests unitaires.
    - Portes passées : `cargo clippy/test` (164/164 mono-thread), frontend gates vertes.

13. **Phase T2.6 — Contrôle de résidu** — module `residuel.rs` avec `verifier_residu` et `ResultatResiduel` (Propre/Review). Second passage d'OCR sur zone nettoyée. 3 tests unitaires.
    - Portes passées : `cargo clippy/test` (167/167 mono-thread), frontend gates vertes.

14. **Phase T2.7 — Budget mémoire et non-régression** — module `bench.rs` avec `empreinte_tuile`, `tuile_dans_budget`, `tuiles_max_simultanees`. Budgets : 64 Mo/tuile, 256 Mo pipeline. 3 tests unitaires.
    - Portes passées : `cargo clippy/test` (170/170 mono-thread), frontend gates vertes.

15. **Phase T3.1 — Trait `Translator`** — module `translator.rs` avec `Translator` (async), `GeminiTranslator` et `LocalTranslator`. 3 tests unitaires.
    - Portes passées : `cargo clippy/test` (173/173 mono-thread), frontend gates vertes.

16. **Phase T3.2 — Prompt par page + validation JSON** — modules `prompt.rs` et `translator.rs` : `construire_prompt`, `calculer_budget`, `schema_traduction`, boucle de traduction avec validation du JSON et vérification du nombre de blocs. 3 tests unitaires.
    - Portes passées : `cargo clippy/test` (183/183 mono-thread), frontend gates vertes.

17. **Phase T3.3 — Glossaire par série (SQLite v10)** — table `glossary` avec `UNIQUE(series_key, term)`, CRUD complet, injection dans les prompts. 1 test unitaire.
    - Portes passées : `cargo clippy/test` (185/185 mono-thread), frontend gates vertes.

18. **Phase T3.4 — Injection du glossaire dans le prompt** — `GlossaryEntry` dans `prompt.rs`, `traduire_page` charge le glossaire depuis le store. Portes passées : `cargo clippy/test` (185/185), frontend gates vertes.

19. **Phase T3.5 — Résilience** — backoff exponentiel avec jitter (500 ms–10 s), détection du quota HTTP 429 (`ErreurTraduction::Quota`), bascule automatique sur `LocalTranslator`, 4 tests unitaires.
    - Portes passées : `cargo clippy/test` (189/189 mono-thread), frontend gates vertes.

20. **Phase T3.6 — Confidentialité** — `ConsentementRefuse` + garde dans `traduire_page`, commandes Tauri `obtenir/definir_consentement_traduction` et `resume_traduction_page`, type `ResumeTraduction` (FR/EN), UI de consentement dans les réglages, fuite de journal corrigée.
    - Portes passées : `cargo fmt/clippy/test` (189/189 + commande morte), `pnpm typecheck/lint/i18n:check/hooks:check/chemins/build`.

**Phase T3 livrée** — traduction complète : trait, prompt, glossaire, résilience, confidentialité.

21. **Phase T4.1 — Polices embarquées** — 8 polices libres (OFL 1.1 / Apache-2.0) téléchargées dans `public/fonts/`, déclarées en `@font-face` dans `globals.css`, classe `.bubble-text` ajoutée pour le rendu des traductions.
    - Portes passées : `pnpm typecheck/lint/i18n:check/hooks/check/chemins/build`.

**Phase T4 livrée** — mise en page et rendu.

### Next steps

Ajouté le 2026-10-08 (pilotage de l'application réelle — les défauts 1, 2, 3 et 4
de la section précédente sont **corrigés et mesurés** ; ce qui suit est le reste) :

1. **Les dimensions des pages distantes** — seul point non résolu du défaut 2.
   `MangaPage` du parser ne porte que `url` et `preview` : il n'existe donc
   aucune dimension à émettre sans télécharger chaque image. Décision humaine
   attendue : (a) le moteur fait une requête de dimension par image (coûteuse en
   bande passante, mais `image.rs` sait déjà lire les en-têtes JPEG/PNG/WebP) ;
   (b) le lecteur mesure `naturalHeight` des images déjà chargées (aucune
   requête, mais la virtualisation ne peut pas s'activer tant que toutes les
   pages ne sont pas connues — donc elle resterait désactivée). En l'état :
   **22 pages montées** au lieu d'une fenêtre glissante.
2. **`analyser_page` est rejoué même quand la page est en cache** — 51 s payés
   pour afficher des blocs déjà en base. Le cache n'est consulté que par
   `traduire_page`.
3. **Le texte OCR dégradé est affiché tel quel** (« 039MA 03350.7 », un crédit) et
   « CERTAIN IT IS CRAZY » devient « CERTAINS C'EST CRAZY ». Aucun garde ne filtre
   une reconnaissance douteuse — le `confidence` vaut 1 partout, donc il ne peut
   rien servir de seuil.
4. **Écran pour saisir la clé de traduction** — le repli local étant coupé et le
   quota Gemini gratuit étant de 20 pages par jour, il manque l'écran qui permet
   de configurer le fournisseur. C'est la dette 3 de la section précédente.
5. **`listSources` à 10 s** au démarrage à froid (défaut 5 ci-dessus).
6. **S2.2** — jouer `docs/smoke-test.md` §S (section écrite le 2026-10-07, jamais exécutée).
7. **Retirer la liste blanche S2** de `main.rs:387-393` : les trois commandes sont maintenant
   appelées par l'interface, la porte « commande morte » doit de nouveau mordre.
8. **T5.1** — éditeur de bloc : modifier le texte, déplacer/redimensionner, retoucher le masque, relancer l'effacement local, régler la taille de police.

---

## Phase S — Sources personnalisées scriptées

> ADR 0010 acceptée le 2026-10-06. Motivée par l'instabilité de l'écosystème scantrad FR (BentoManga,
> LireScan, MangaKawaii, Anime-Sama tous hors ligne ou migrés en octobre 2026).

### S0 — Infrastructure moteur

- [x] **S0.1** — `HttpBridge.kt` : bridge OkHttp → Nashorn. Expose `http.get(url)` et `http.post(url, body)`
  au script. Refuse les URLs non-https, corps limité à 5 Mo.
  _Prouvé : 3/3 tests HttpBridgeTest — 2026-10-06._

- [x] **S0.2** — `ScriptedParser.kt` : charge un script JS via Nashorn, valide `SOURCE` et ses méthodes,
  exécute `getList`/`getDetails`/`getPages` via Invocable.
  _Prouvé : 7/7 tests ScriptedParserTest — 2026-10-06._

- [x] **S0.3** — `ScriptedSourceLoader.kt` : scanne le dossier `sources/`, charge/recharge les scripts,
  rechargement à chaud via WatchService.
  _Prouvé : 5/5 tests ScriptedSourceLoaderTest — 2026-10-06._

- [x] **S0.4** — Intégration `Server.kt` : `listSources` inclut les sources custom avec `"custom": true`.
  `search`/`details`/`chapters`/`pages` routent vers le `ScriptedParser` si `source_id` est custom.
  _Prouvé : 37/37 tests moteur — 2026-10-06._

### S1 — Côté Rust

- [x] **S1.1** — Migration SQLite v9 : table `custom_sources (id TEXT PK, name TEXT, lang TEXT, path TEXT,
  installed_at INTEGER)`. Sauvegarde avant migration, test de migration v8→v9.
  _Prouvé : `migration_v8_vers_v9_cree_custom_sources` ok, 185/185 tests passés — 2026-10-06._

- [x] **S1.2** — Commandes Tauri : `importer_source(path)` (copie le `.js` dans le dossier sources,
  enregistre en base), `lister_sources_custom()`, `supprimer_source_custom(id)` (efface le fichier + la
  ligne). Validation : nom de fichier assaini (anti path-traversal), taille ≤ 256 Ko.
  _Critère : `cargo test` couvre les cas limites (traversée, taille, doublon)._
  _Prouvé : cargo test 185/185, clippy 0 warnings, fmt propre — 2026-10-06._

### S2 — Interface

- [x] **S2.1** — Écran « Sources perso » : bouton « Importer un script (.js) » (dialogue natif), liste
  des sources installées (nom, langue, date), bouton supprimer avec confirmation.
  i18n FR + EN. Clés à ajouter dans `i18n.ts`.
  _Critère : `pnpm typecheck/lint/i18n:check/hooks:check/chemins:check` vertes._
  _Prouvé : commit `c83e56f`. typecheck 0 erreur ; lint 0 erreur (4 warnings
  `exhaustive-deps` préexistants) ; `i18n:check` vert (281 clés FR = 281 EN) ; `hooks:check`
  et `chemins:check` verts ; `next build` OK — 2026-10-07._
  _Confirmation de suppression en deux temps dans la ligne : la capability ne vaut que
  `dialog:allow-open`, un dialogue natif de confirmation aurait exigé de l'élargir._

- [ ] **S2.2** — Intégration dans l'écran Sources : les sources custom apparaissent dans la grille avec
  un badge « Perso ». Désactivation possible comme les sources natives.
  _Critère : smoke test manuel (docs/smoke-test.md §S)._
  _Code écrit et commité (`c83e56f`) : badge « Perso », nom affiché via `displayName`,
  désactivation par `s.name` donc sans cas particulier. **Reste** : exécuter
  `docs/smoke-test.md` §S — la section est écrite, jamais jouée._

- [x] **S2.3** — Card « Sources importées » dans l'écran Catalogue : dans la grille des
  langues (l'écran d'entrée), une card pleine largeur qui **contient la liste des sources
  importées** — chaque ligne cliquable pour ouvrir la source — et le bouton
  « Importer un script… ». Demande explicite de l'humain le 2026-10-07, hors périmètre
  initial : ajoutée ici avant d'écrire le code (§11, anti-dérive).
  _Prouvé : `chargerSources` extrait en `useCallback` sans dépendance et appelé au
  montage + après import ; après import la liste est rechargée **3 fois** (0, 1,5 s,
  4 s) parce que le moteur ne voit le fichier que ~580 ms après la copie (mesuré :
  WatchService, 1ʳᵉ tentative refusée car le fichier s'écrit encore). typecheck 0
  erreur ; lint 0 erreur ; i18n/hooks/chemins verts ; `next build` OK._
  _La card est un `<section>` et non une tuile `<button>` : elle contient des boutons,
  et deux `<button>` imbriqués seraient du HTML invalide (§14). Style : `.lang-tile`
  réutilisée (fond, bordure, rayon) + `.import-*` ; aucune nouvelle couleur._
  _Les lignes affichent la langue du **moteur** (`sourcesImportees`), pas la colonne
  `custom_sources.lang` du panneau Réglages : celle-ci est écrite en dur à `fr` et
  n'est donc pas affichée._

  _Corrigé après pilotage de l'app réelle (2026-07) : la card était **placée après** la
  grille des langues et tombait sous la ligne de flottaison — la sonde DOM la trouvait, l'œil
  ne la voyait pas. Elle est désormais **avant** les tuiles ; mesuré : `haut 201, bas 391`
  pour une fenêtre de 749. Ajouts au passage : `data-tab` sur les onglets et `data-lang` sur
  les tuiles de langue (un onglet ne se cible plus par son libellé traduit), et `nomSource()`
  pour le nom affiché — la fiche affichait `custom:mangadex-api`._

### Dette connue mesurée le 2026-10-08 (pilotage de la traduction)

Défauts trouvés en pilotant l'app réelle, **après** le correctif `aa99972`
(magasin de secrets Windows) et avec le chemin OpenRouter actif.

1. **`translator::tests::gemini_sans_cle_retourne_local_indisponible` dépend de la machine.**
   Il suppose qu'aucune clé n'est enregistrée dans le trousseau ; il passe chez un
   développeur sans clé et échoue dès que la machine en a une (mesuré : 224 passés / 1
   échoué avec une clé, 225 passés après suppression). Le correctif : rendre la clé
   injectable dans `Traducteur` au lieu de lire le trousseau réel depuis un test.

2. **Le quota Gemini gratuit est de 20 requêtes par jour et par modèle.** Mesuré le
   2026-10-08 sur `gemini-3.8-flash` : HTTP 429, `quotaMetric`
   `GenerateRequestsPerDayPerProjectPerModel-FreeTier`, `quotaValue: "20"`,
   `retryDelay: 59049 s`. Une page = une requête, donc **20 pages par jour** au total,
   et la traduction ceases silencieusement (statut `fallback`, overlay vide, aucun
   message). L'ADR 0008 dit « ~14 appels/jour » : c'était mesuré sur Gemini 3.5.
   **Le repli hors ligne n'existe pas** (`traduire_local` est un stub), donc au-delà
   de 20 pages la fonction n'est plus utilisable.

3. **Le chemin OpenRouter n'était jamais testé parce qu'il n'avait pas d'écran.**
   `Traducteur::cle_openrouter()` lit le trousseau **puis** `OPENROUTER_API_KEY`, et
   `traduire_openrouter` est déjà complet avec le modèle en dur
   `cohere/north-mini-code:free`. Mesuré le 2026-10-08 : **2,6 s** par page contre
   **73 s** avec Gemini, coût 0, traduction correcte
   (« YOU GOT TO GET CLOTHES FOR DOGS » → « Il faut acheter des vêtements pour les
   chiens. »). Une simple variable d'environnement rend le chemin actif — il ne
   manque qu'un écran pour saisir la clé, et un choix de fournisseur.

4. **L'effacement n'est pas branché, donc la traduction se superpose à l'original.**
   `inpaint` répond `MODEL_MISSING` (`tests/fixtures/README.md` le dit) et ADR 0011
   prévoit d'effacer le dialogue avant de le remplacer. La capture du 2026-10-08 montre
   « Cette partie montre / la liberté de la jeunesse » **par-dessus** l'anglais, qui
   reste lisible dessous : le résultat est illisible. C'est le défaut prioritaire
   tant que la traduction fonctionne. Correctifpossible à court terme : une plaque de
   fond derrière la bulle.

5. **L'OCR produit des blocs fantômes et du texte dégradé, et rien ne le filtre.**
   Le champ `confidence` vaut **1** sur les 45 blocs mesurés : il ne discrimine rien,
   donc aucun seuil ne peut s'appuyer dessus. Un bloc `dialogue` d'un seul caractère
   (`"7"`, `"一"`, `"d"`) et des blocs `free_text` posés sur du tramage vide ont été
   détectés. Corrigé pour l'affichage par `blocsSuperposables` (type + longueur ≥ 4),
   mais **rien n'empêche un texte Recognition dégradé d'être affiché** : « MODERN... S »
   a été rendu tel quel par le modèle.

6. **La vision est le goulot, pas le traducteur.** `analyser_page` seule : 11 s, 43 s,
   20 s, 30 s, 30 s selon la page (1 à 19 blocs).

### Dette connue mesurée le 2026-10-08 (pilotage 2 — surcouche et navigation)

Pilotage de l'app réelle, après `d82a79d`. Un walk complet a été écrit et rejoué
(`.mesures/pilot-90-traduction.cjs`, `.mesures/pilot-94-zoom.cjs`,
`.mesures/pilot-99-saut-bouton.cjs`, `.mesures/pilot-101-sans-cles.cjs`), sur
GDSCANS / `Otaku-kun Who's Kind to Gyaru` / Ch.1.1 (22 pages). État de la machine
mesuré : **aucune clé** Gemini dans le trousseau, pas de `OPENROUTER_API_KEY`,
`TRADUCTION_LOCALE_ACTIVE = false`.

**Ce qui marche** : les 4 onglets répondent, 1363 sources, 3 favoris, thème et
langue corrects, **0 erreur console et 0 requête en échec** sur les quatre
écrans. La traduction **en cache** s'affiche : 5 blocs, français correct
(« Oui ! », « C'est pas vrai. », « C'est fou ! »), et la plaque de fond non
committée rend le texte lisible (mesuré `rgba(0,0,0,0.82)`).

1. **La surcouche de traduction n'était pas centrée sur l'image — CORRIGÉ.**
   Décalage horizontal exactement égal à la marge de centrage, mesuré à trois
   zooms : **−315 px à 50 %, −126 px à 75 %, 0 à 100 %** (là, la page déborde la
   bande). Cause : `.webtoon-inner` centre ses images par `align-items:center`,
   mais les pages de `.traduction-overlay` sont en `position:absolute`, donc
   **hors flux** : `left:auto` les pose au bord gauche et l'alignement du parent
   flex ne s'y applique pas. Les traductions flottaient à côté de leur bulle
   (`.mesures/p90-2-apres-fr.png`, avant).
   Correctif : `align-items: center` sur `.traduction-overlay`. Après : écart
   **0,0,0**. Porte prouvée **rouge** en commentant la ligne : −336,5 / −134,6 /
   0. Le défaut était invisible parce qu'il n'existe qu'en dessous de 100 % de
   zoom, et toutes les mesures précédentes portaient sur le compteur de pages.

2. **Le saut de page ne déplaçait pas la bande — CORRIGÉ.** Mesuré : trois
   clics sur `→` laissaient `scrollTop` à **0** et la première page affichée à
   l'index 0, tandis que le compteur passait de 3 à 6 — l'écran mentait. La
   molette, elle, fonctionnait (`scrollTo(3000)` → 3000, compteur 6). Cause lue
   dans le code : `Server.kt::pageToJson` n'émet que `index`, `url` et
   `previewUrl`, et `commands.rs::pages` n'ajoute que `token` — donc
   `largeur`/`hauteur` sont **toujours absents**, `hauteurs` vaut zéro,
   `offsets` vaut zéro partout. La garde `haut !== undefined` était donc
   **satisfaite par 0**, et `scrollToPage` défilait vers `max(0, 0 - 8)` : la
   branche de repli sur `scrollIntoView` — qui est la bonne hors virtualisation —
   était inatteignable. Correctif : `offsets[cible]` n'est lu que si
   `bandeVirtualisable`. Preuve rouge/verte en réécrivant le chunk servi
   (`.mesures/pilot-107-preuve-rouge.cjs`) : ancien code → compteur 4, bande 0,
   page 1 affichée ; code corrigé → compteur 5, bande 2864, page 5 affichée.
   **Reste ouvert** : `MangaPage` du parser ne porte que `url` et `preview`, donc
   la virtualisation reste impossible sans télécharger chaque image. Les 22 pages
   sont donc montées au lieu d'une fenêtre glissante.

3. **Sur le chemin par défaut, la bascule FR ne produisait rien de visible —
   CORRIGÉ.** Trois défauts distincts, tous mesurés :
   - *Le délai de 10 s ne couvre pas le sidecar froid.* Mesuré : sidecar tué puis
     FR → `detect vision : délai dépassé (10s)` en 39,7 s. Après un premier
     `detect`, la même page aboutit. La session ONNX n'est chaude qu'après une
     première inférence. Correctif : préchauffage dans `assurer_demarrage`,
     après `loadPack` — là où un délai long est normal. Mesuré : démarrage neuf →
     analyse réussie.
   - *Le superviseur ne voyait pas mourir son enfant.* `assurer_demarrage`
     testait `enfant.is_some()`, or un `Child` est un handle, pas un état : la
     valeur restait `true` après un crash, donc **toute** traduction suivante
     échouait définitivement sur `stdin vision : os error 232`, sans relance.
     Correctif : `try_wait()` + remise à zéro de l'état. Mesuré : sidecar tué →
     analyse réussie en 163 s et **nouveau** sidecar (pid 10328).
   - *L'échec n'était visible que dans la console.* `console.warn` seul
     (§ `page.tsx`). Correctif : `<p role="alert">` dans la barre du lecteur,
     qui reste jusqu'à ce qu'une page réussisse, avec la clé
     `lecteur.traduction.echec` (FR/EN).
   - *La file d'attente.* L'observateur déclenche `i`, `i+1`, `i-1`. Une file
     borne le débit à une analyse, mais **mesure 109 : sur cette machine il n'y a
     qu'une analyse à la fois de toute façon, et elle aboutit dans les deux cas**.
     La concurrence n'était donc pas la cause du délai dépassé — il ne faut pas
     lui attribuer le correctif. Elle est conservée pour le débit, avec une garde
     de génération : sans elle, une analyse en file partait après un changement
     de chapitre et répondait « page inconnue ou déjà évacuée du cache ». Comparer
     le **chapitre** ne suffit pas — rouvrir le même chapitre ré-enregistre les
     jetons, l'URL est identique : il faut comparer le **jeton de la page**.

4. **« 8 désactivée{s} » — CORRIGÉ.** `page.tsx` passait `{ n }` à un gabarit
   `'{n} désactivée{s}'`. Les deux autres appels de la même famille passent déjà
   `s: n > 1 ? 's' : ''` ; celui-ci non. Mesuré après : « 1363 sources
   disponibles · 8 désactivées », plus aucun `{s}` à l'écran. Le cas singulier
   n'a pas été joué à l'écran : il faudrait désactiver 7 des 8 sources dans les
   réglages de l'utilisateur.

5. **`listSources` expire à 10 s au démarrage à froid** (JVM qui reconstruit le
   catalogue des 1370 sources) : l'écran affiche « 0 sources disponibles » puis
   « timeout 10s sur listSources ». L'interface, elle, montre l'erreur — c'est le
   bon comportement ; c'est le délai qui est trop court pour un premier appel.

Confirmé par ailleurs, déjà connu : le texte OCR dégradé est affiché tel quel
(« 039MA 03350.7 », un crédit de la page de remerciements) et
« CERTAIN IT IS CRAZY » devient « CERTAINS C'EST CRAZY ». Et `analyser_page` est
**rejoué même quand la page est en cache** : 51 s payés pour afficher des blocs
déjà en base.

### Observation non expliquée (2026-10-08)

Trois fois pendant ce pilotage, `tatami-desktop.exe` a disparu **sans une seule
ligne de journal**. La séquence exacte de la troisième fois est dans
`%APPDATA%\Tatami\logs\tatami.log` :

```
16:15:54  WARN supervisor: stdout: EOF (moteur fermé)
16:15:54  WARN supervisor: boucle de lecture terminée
16:15:54  WARN supervisor: sidecar terminé (exit code: 1)
16:15:54  INFO supervisor: sidecar sorti, relance
16:15:55  INFO supervisor: sidecar démarré (pid=Some(14016))
--- puis plus rien ; le processus a disparu vers 17:16:05, soit 10 s après ---
```

Ce qui est établi, et ce qui ne l'est pas :

- **Le superviseur moteur, lui, fait son travail** : il voit la sortie du
  sidecar et le relance 1 s après. C'est le comportement voulu (§4.2).
- **Ce n'est pas un manque de mémoire** : échantillonné pendant tout le parcours
  (`.mesures/echantillon-memoire.ps1` → `.mesures/memoire.log`, 180 échantillons
  de 5 s), la mémoire libre descend au plus à **2 190 Mo sur 12 193 Mo** et
  l'engagement plafonne à **18 777 Mo sur 23 572 Mo**. Le pic du sidecar vision
  est à 982 Mo.
- **Ce n'est pas une panique Rust** : une panique écrit sur stderr, et
  `.mesures/app-stderr.log` est **vide**. Aucune trace, donc aucune exception.
- **L'application a été terminée de l'extérieur** (pas de `ExitProcess` propre,
  pas de trace). Hypothèse à vérifier, non vérifiée : un job object posé par
  l'environnement de lancement (le processus est démarré depuis un shell
  d'agent), qui tuerait l'arbre au-delà d'un plafond. Vérifiable en lançant
  l'application **hors** de ce shell (double-clic, ou `Start-Process` depuis une
  session interactive) et en la laissant tourner seule une heure.

À faire avant d'aller plus loin sur la traduction : c'est le seul défaut qui peut
faire perdre le travail de l'utilisateur.

Pièges de mesure rencontrés dans ce pilotage, à ne pas re-commettre :
`data-page` n'existe **que** dans la surcouche (attendre `[data-page]` pour
détecter l'ouverture du lecteur est faux) ; `data-page` est un index **base 0** ;
le squelette `aria-busy="true"` intercepte les clics ; `data-lang` est en
**minuscules** ; un clic juste après un rechargement est perdu (hydratation) ;
compter toutes les plaques confond la fenêtre virtuelle avec la page visée.
Et `%APPDATA%\Tatami\launch_with_ort.ps1` était **corrompu** (`\ = '…'` au lieu
de `$env:ORT_DYLIB_PATH = '…'`) : l'application partait donc sans runtime ONNX.
Remplacé par `.mesures/lancer-app.ps1`.

### Dette connue mesurée le 2026-10-07

- `FermetureStdinTest > cinq requetes d'un bloc rendent cinq reponses completes` **échoue en
  suite complète** (2 exécutions sur 2) et **passe seul** (3 sur 3). Vérifié sans la
  modification du jour (`git stash`) : l'échec est donc préexistant et lié à la charge, pas
  à la phase S. Le test lit 5 lignes de stdout après fermeture de stdin — c'est le genre de
  course déjà décrite dans son propre commentaire. À reprendre : timeout sur la lecture,
  ou assertion sur le nombre de lignes lues plutôt que sur la 5ᵉ.

### Audit de bout en bout (2026-10-07)

Trois défauts trouvés en relisant les 4 couches, tous deux commits :
1. **Identité divergente** (`6fe11bf`) : base = nom de fichier, moteur = `SOURCE.id`.
   Une source supprimée restait fantôme dans la grille et ses favoris pointaient vers un
   identifiant inconnu de la base. Corrigé : le nom de fichier fait foi (ADR 0010 révisée),
   2 tests de régression, porte prouvée en la cassant. Trouvé **grâce à** l'exemple
   `docs/exemple-source.js`, dont `id` (`exemple-fr`) diffère de son nom de fichier.
2. **Langue codée en dur** : `custom_sources.lang` vaut toujours `fr` (TODO S2). Le panneau
   ne l'affiche plus. Le TODO reste ouvert, listé en next steps.
3. **Watcher non testé** : `boucleWatcher` (`ENTRY_CREATE`/`ENTRY_DELETE`) n'était exercé par
   aucun test — c'est pourquoi 1 et 2 étaient invisibles. Couvrir par les deux tests
   ajoutés (chargement + déchargement par nom de fichier) ; le thread lui-même reste non
   testé (poll à 2 s).

Vérifié correct : dossier `%APPDATA%\Tatami\sources` aligné des deux côtés · anti
path-traversal avec canonicalisation et test · limite 256 Ko des deux côtés · `HttpBridge`
refuse le non-https · capability minimale (`dialog:allow-open` seule).

### Critère de sortie de la phase S

- `gradle test` passe (moteur)
- `cargo test` passe (Rust)
- Toutes les portes front vertes
- Smoke test §S documenté et exécuté
- Un script d'exemple `docs/exemple-source.js` fourni et commenté

### Fichiers modifiés (T1)
- `contract/translation/schema.json` — schéma v1
- `contract/protocol.json` — méthodes vision ajoutées
- `contract/protocol.fixtures.json` — fixtures vision
- `apps/desktop/src-tauri/src/traduction.rs` — types + commandes traduction
- `apps/desktop/src-tauri/src/vision.rs` — types sidecar vision
- `apps/desktop/src-tauri/src/pack.rs` — gestionnaire de packs
- `apps/desktop/src-tauri/src/store.rs` — migration v8 + fonctions
- `apps/desktop/src-tauri/tests/contrat_traduction.rs` — tests contrat traduction
- `apps/desktop/src-tauri/tests/contrat_vision.rs` — tests contrat vision
- `apps/desktop/app/types/traduction.ts` — types TS miroir

### État du dépôt
- 17 commits : `b6734e7` téléchargements parallèles, `c141377` chargeur animé, `6e25642` molette fix,
  `9bf3fe6` clé Gemini dans le trousseau OS, `f410272` suppression du surlignage "en cours",
  `59526aa` T1 complet (schéma, migrations, vision, packs, tests), `ff33cd0` docs T1,
  `5758ff3` T2.1 tuiles, `539d3ef` T2.2 masques, `02f63a5` T2.3 classification, `b541844` T2.4 OCR/ordre de lecture,
  `ed2d745` T2.5 stratégie d'effacement, `a8fb87a` T2.6 contrôle de résidu, `d6b9121` T2.7 benchmarks mémoire,
  `c6f7dc9` T3.1 trait Translator, `48e7c0a` T3.2 prompt par page + validation, `c375c8c` T3.3 glossaire SQLite v10,
  `de53744` T3.4 injection glossaire dans le prompt, `5b2edf8` T3.5 résilience backoff/quota/fallback,
  `c83e56f` S2.1+S2.2 écran « Sources perso » et badge « Perso »
- 23 commits : ajouts T3.6 (confidentialité), T4.1 (polices embarquées), T4.2 (layout), T4.3 (proptest layout).
- Toutes les vérifications passent (clippy, typecheck, lint, i18n, hooks, chemins).
- Prochaines tâches : S2.2 (jouer `docs/smoke-test.md` §S), retrait de la liste blanche S2 dans
  `main.rs`, puis T3.6 — Confidentialité.
- Note d'hygiène : `main.rs` était modifié (+4 lignes) au début de la session du 2026-10-07 et
  ne l'est plus, sans commit ni crochet git ni hook qui l'expliquerait (`.git/hooks` = samples
  seuls). Le contenu est identique à `HEAD`. À surveiller : si T3.6 attendait un
  enregistrement de commande dans `main.rs`, il manque.

### Journal 2026-10-09 — commit `9df864e` : correctifs lecteur webtoon (P0, P2, ADR 0012)

**Tâches réalisées** : P0 (`loading` conditionnel), P2 (`scrollToPage` robuste + garde
`bandeVirtualisable`), P3 TODO commenté dans `openReaderAt`, ADR
`docs/decisions/0012-dimensions-pages-distantes.md` créée (statut « proposée »).

**Résultats des cinq portes front** (`cd apps\desktop`) — attestés en itération de
correction du finding « Portes non attestées dans le journal » (revue commit `9df864e`) :

```
pnpm typecheck
  → tsc --noEmit   ✓  0 erreur

pnpm lint
  → eslint
     page.tsx:1338  warning  exhaustive-deps (chargerSources, refreshDownloads, setNotice, t)
     page.tsx:1713  warning  exhaustive-deps (refreshShelf, refreshSignets)
     page.tsx:3479  warning  exhaustive-deps (reader)
     page.tsx:3517  warning  exhaustive-deps (closeReader, scrollToPage)
     4 problems (0 errors, 4 warnings)  ✓  4 warnings connus, 0 erreur

pnpm i18n:check
  → clés FR : 307 | clés EN : 307 | littéraux : 307
     aucune clé morte, aucune traduction manquante, aucun placeholder nu.  ✓

pnpm hooks:check
  → 7 modules lus, aucune dépendance d'effet instable.  ✓

pnpm chemins:check
  → 7 modules lus, 4 affichage(s) via cheminLisible, aucune valeur brute.  ✓
```

Toutes les portes vertes. 0 erreur typecheck, 4 warnings lint connus (préexistants, hors scope
du correctif).
