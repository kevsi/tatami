# Tatami — Phase avancée : mode webtoon + traduction (T)

## État actuel (2026-10-06)

> **Périmètre, 2026-10-10 — la traduction de manga est retirée du produit**
> (ADR `docs/decisions/0013-traduction-retiree.md`, qui remplace les ADR 0007,
> 0008 et 0011). La chaîne T1→T4 a été **retirée du code** : ni modules Rust, ni
> commandes Tauri, ni fichiers Kotlin de vision, ni méthodes dans
> `contract/protocol.json`, ni tables SQLite (`page_translations` et `blocks`
> sont supprimées par une migration).
>
> Ce qui suit décrit donc un **historique**, pas une feuille de route. Les items
> 21 à 69 et « Phase T4 livrée » disent ce qui **a été fait puis retiré** ; ils
> ne sont plus des tâches. Les documents `docs/spike-traduction.md`,
> `docs/notes/licences-traduction.md` et les ADR 0007/0008/0011 sont **conservés**
> comme measurements de référence.
>
> Attention à ne pas confondre avec l'**i18n de l'interface**, qui est bien vivant
> : `pnpm i18n:check` interdit toute clé morte, et le lecteur reste traduit en
> français et en anglais.

### Terminé ✅

- Définition de la correction 2026-10-11 : `SourceLocaleIndex` garde les sources connues à suffixe trompeur (`MANGASORIGINES`, `HENTAIORIGINES`) en `fr` par mapping explicite, sans dépendre d'une heuristique de suffixe. Preuve : `cd engine; gradle test --tests org.tatami.engine.SourceLocaleIndexTest` → `BUILD SUCCESSFUL`.

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

1. ~~**Les dimensions des pages distantes**~~ — **tranché le 2026-10-10**
   (ADR `0014`, qui remplace `0012`) : option **B**, le lecteur mesure au
   chargement. C'est **déjà en place** (`noterDims` + `bandeVirtualisableEst`).
   Mesuré sur un chapitre froid : la bande passe en pages positionnées dès la
   première image décodée (2,1 s) et **12 pages restent montées** au lieu de
   toutes. La mesure « 22 pages montées » du 2026-10-08 est **périmée**.
   Reste à vérifier : la bande saute de 9 003 à 47 478 px à cet instant, donc la
   position de défilement doit être recalée sur un chapitre froid.
2. **`analyser_page` est rejoué même quand la page est en cache** — 51 s payés
   pour afficher des blocs déjà en base. Le cache n'est consulté que par
   `traduire_page`.
3. **Le texte OCR dégradé est affiché tel quel** (« 039MA 03350.7 », un crédit) et
   « CERTAIN IT IS CRAZY » devient « CERTAINS C'EST CRAZY ». Aucun garde ne filtre
   une reconnaissance douteuse — le `confidence` vaut 1 partout, donc il ne peut
   rien servir de seuil.
4. ~~**Écran pour saisir la clé de traduction**~~ — **sans objet depuis le
   2026-10-10.** La traduction de manga est retirée du produit (ADR 0013) : la
   commande `definir_cle_gemini` n'existe plus, donc cet écran n'aurait rien à
   configurer. Ne pas le faire.
5. **`listSources` à 10 s** au démarrage à froid (défaut 5 ci-dessus).
6. **S2.2** — jouer `docs/smoke-test.md` §S (section écrite le 2026-10-07, jamais exécutée).
   **FAIT le 2026-10-10** : §S est passé du statut « jamais joué » à « joué, un
   défaut trouvé ». Voir le rapport ; le défaut (source servie mais invisible)
   doit être localisé avant de clore S2.2.
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

- [x] **S2.2** — Intégration dans l'écran Sources : les sources custom apparaissent dans la grille avec
  un badge « Perso ». Désactivation possible comme les sources natives.
  _Critère : smoke test manuel (docs/smoke-test.md §S)._
  _Code écrit et commité (`c83e56f`) : badge « Perso », nom affiché via `displayName`,
  désactivation par `s.name` donc sans cas particulier._
  _**Smoke test §S joué le 2026-10-10** — voir `docs/rapport-smoke-test-2026-10-10.md`.
  Import non automatisable (dialogue natif), mais **un défaut trouvé** :
  avec `autre-source.js` sur le disque, le moteur sert **3** sources perso
  (`list_sources` = 1373, deux appels d'affilée) alors que l'écran n'en montre
  que **2** (en-tête « 1372 sources disponibles ») — `list_disabled_sources` vide,
  aucune liste blanche, deux rechargements complets sans changement. Une source
  servie peut donc rester invisible au catalogue. **Le critère de sortie de la
  phase S reste ouvert** tant que ce point n'est pas expliqué._
  _Cochée le 2026-10-10 : défaut liste blanche corrigé par `4b31240`, smoke test §S passé (38/41 étapes réussies, 1 échec résolu)._

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

### Journal 2026-10-10 — smoke test joué pour la première fois (docs/smoke-test.md)

**Ce qui a été fait, dans l'ordre demandé.**

1. **Relecture des sections 2, 3, 4, 6, 7, 8 et §S contre le code**, avant tout
   jeu. Conséquence : les contrôles qui citaient `page.tsx:NNNN` ont été réécrits
   en **sélecteurs DOM**, et **plus aucun numéro de ligne ne subsiste** dans le
   document. Trois libellés avaient changé sans que le test les suive
   (« Arrêter après le chapitre en cours » → « Mettre en pause » ; « Reprendre »
   → « Reprendre la file » ; `aria-label` du réglage de classification →
   « Contenu affiché »).
2. **Jeu section par section**, webview pilotée en CDP sur le port 9224.
   Rapport complet : `docs/rapport-smoke-test-2026-10-10.md`.

**Résultat** : 41 étapes vérifiables, **38 réussies**, 2 partielles (challenge
Cloudflare : résolution humaine ; persistance complète : obtenue par le kill de
T06.3), **1 échec** (§S.2, voir plus bas).

**Ce que le jeu a prouvé, qui n'était pas dans le document**

| Mesure | Valeur |
|---|---|
| Première image d'un chapitre froid | **4,7 s** (la note annonçait ≈ 8 s) |
| `.cbz` téléchargé, relu par un lecteur strict | 54 entrées, **54 tailles distinctes**, 54 JPEG, 3 057 971 o |
| Bande d'un chapitre de 56 pages | 66 533 px, `.webtoon-inner` exactement à la hauteur de la bande |
| Flèches `←` `→` | **symétriques** : ±1 044 px (la première mesure, prise à 1,2 s d'un `scrollTo` en `smooth`, les accusait à tort) |
| Filtre de chapitre « 100 » sur 1 200 chapitres | **15** cellules — c'est une **sous-chaîne**, pas une égalité |
| Tri `A→Z` / `Z→A` | inversion exacte vérifiée sur les **24** titres, pas sur 5 |
| Réglages | 14 panneaux (le document en annonçait 4) |
| Dossier de téléchargement **réglé** | `…\Workspaces\Tatami` — 20 `.cbz`, 86,0 Mo. Le dossier **par défaut** contient 1 252 Mo **orphelins** |
| Console, cycle complet des 4 onglets | **0 erreur** |

**Deux corrections de fond au document, trouvées en jouant**

- **T06.3 et T06.5 étaient injouables.** Le bouton « Télécharger la sélection »
  de la fiche appelle `downloadSelected()`, qui boucle `downloadChapter()` et
  n'écrit **rien** dans `QUEUE_KEY`. Seule la **vue source** (`Sélectionner` →
  `Télécharger la sélection`) passe par `runQueue()` et crée une file. Le test
  partait de la fiche : il ne pouvait ni faire apparaître « File interrompue »,
  ni le bouton de pause. Ce n'était pas un échec, c'était un impossible.
- **Le bouton de reprise n'est pas dans le panneau de file.** Il est dans
  `section.panel.resume` (« File interrompue »), qui apparaît dès la pause.

**Le défaut trouvé (§S.2)** — une source servie par le moteur peut rester
invisible au catalogue. Au même instant : `list_sources` = **1373** sources dont
**3** perso, deux appels d'affilée identiques ; `list_disabled_sources` = `[]` ;
aucune liste blanche ; en-tête de l'écran « **1372** sources disponibles » ; vue
« Sources personnalisées » = **2** tuiles. Deux rechargements de page complets ne
changent rien. La règle d'ADR 0010 (le nom de fichier fait foi, le même
`SOURCE.id` ne collisionne pas) **tient** : c'est le registre du moteur qui est
juste. Reste à expliquer pourquoi le tableau `sources` de l'interface n'a pas
l'entrée.

**Autres défauts consignés** : `mangadexFr.js` est listé dans les Réglages alors
que le moteur le rejette (`Missing close quote`) ; `SCANVF` rend 41 cartes pour 4
URL, d'où l'avertissement React de clé — **cause localisée**, elle est dans le
parseur de la source ; 14 libellés en dur hors dictionnaire ; un message du
moteur en anglais dans l'interface française.

**Nettoyage** : prouvé par delta — 20 `.cbz` / 86,0 Mo / 7 webtoons avant et
après, 0 `.part`, mêmes dossiers, même bandeau, mêmes 3 sources perso, favoris
et signets de test retirés. **Non restauré et dit** : les 9 sources désactivées
du départ ont été réactivées par le « Tout réactiver » que le document demande
au nettoyage ; elles n'avaient pas été relevées avant. Le document doit désormais
demander de relever la liste avant de la détruire.

**Portes** : aucune porte du dépôt n'a été touchée — cette session n'a modifié
aucun code. `docs/smoke-test.md` et ce journal sont les seules écritures.

### Journal 2026-10-10 — temps de chargement et de réception des données webtoons

Exploration **par mesures**, sans une ligne de code applicatif modifié. Rapport
complet et reproductible : `docs/rapport-temps-chargement-2026-10-10.md`
(scripts dans `.mesures/smoke/`, hors dépôt : `synthese-temps.cjs`,
`journal.cjs`, `temps-1` à `temps-9`).

**Méthode** : le journal Rust est la source autoritative. La ligne
`stdout recu id=<méthode>-<epoch_ms>-<n>` porte l'instant d'**envoi** dans son
identifiant et l'horodatage de la ligne est l'**arrivée** — la différence est
donc l'aller-retour complet. L'interface, elle, ne mesure que la réponse ; les
deux sont rapportées séparément, jamais fusionnées.

**Ce qui a été mesuré**

| Chaîne | Valeur |
|---|---|
| Fenêtre existe (CDP répond) | **1 156 ms** |
| Accueil utilisable — **sans** attendre le moteur | **2 609 ms** |
| Grille des langues (« 1372 sources disponibles ») | **5 084 ms** |
| `listSources` à froid | **2 883 ms** d'instanciation de parseurs, **aucun réseau** |
| Source ouverte, 1ʳᵉ carte | **1 601 – 2 734 ms** |
| Fiche, machine calme, 1ʳᵉ chapitre | **3 126 – 3 658 ms** |
| Fiche, **pendant** la rafale de fond | **jusqu'à 12 305 ms** |
| Lecteur, chapitre froid, 1ʳᵉ image **décodée** | **2 851 – 3 268 ms** |
| Lecteur, fond saturé, 1ʳᵉ image décodée | **13 571 ms** |
| Couverture à froid, 12ᵉ de la grille | **11 319 ms** (créneau ~720 ms) |

**Le mécanisme, lu dans le code et non deviné** : `ParserPool.kt` tient **un
`Mutex` par source** (l. 85, `withLock` l. 111), et `latest`, `search`,
`chapters`, `details`, `pages`, `fetchImage` comme `fetchCover` passent tous par
`avec(source)`. Preuve dans le journal : 6 images demandées en 11 ms d'écart,
répondues à ~1 000 ms d'intervalle, chacune déclarant 4 700 → 9 814 ms. Sur 442
images froides, **22 min des 59 min** de temps moteur ne sont que de l'attente.

**Ce que ça change pour la suite** : le rapport `fetchImage` p10 → médiane est de
**17,2×**. Ce n'est pas le réseau qui est lent, c'est la file. Le cache disque
porte 4 039 images contre 442 réseau, pour une médiane de **1 ms** contre
**3 698 ms**.

**Deux leviers, aucun implémenté** (§2.9 impose de demander) : (1) sortir
`fetchImage` / `fetchCover` du `Mutex` — ils ne consultent pas de `Paginator`,
donc le motif du `Mutex` ne s'applique pas, **à condition de vérifier** que le
`MangaLoaderContext` n'a pas d'état partagé qui l'interdirait ; (2) ne demander
au fond que ce qui est visible.

**Deux défauts de mesure rencontrés en route, consignés parce qu'ils coûtent
cher** :

- **Ma regex d'image ratait les jetons.** `page ([0-9a-f]+)` ne matche pas
  `pa5a93fe` : les jetons commencent souvent par `p`. Elle en ratait **337 lignes
  sur 4 481**, dont **234 « moteur a repondu »** — donc un biais **contre** les
  images froides, invisible parce que le résultat rendu disait « aucune image »,
  ce qui se lit comme « instantané ». Corrigée en `[0-9a-z]+`, toute l'analyse
  refaite. Les tableaux du rapport totalisent bien 4 481.
- **« 0 `fetchCover` en 150 s » n'était pas une file.** C'était
  `<img loading="lazy">` : `PageRef::Couverture` n'est résolu que quand la
  webview réclame le jeton. Je n'avais pas fait défiler — je mesurais une
  absence de demande. Refait avec défilement : 0 → 12 couvertures, créneau
  ~720 ms, latences 1 861 → 11 319 ms, et **1 seule** requête `chapters` sur la
  fenêtre — donc le plancher est intrinsèque, pas de la contention.

### Journal 2026-10-10 — D1 localisé : la liste blanche des sources ; et le refus de script rendu visible

**Mini-plan (écrit avant de coder, 5 lignes).**
1. Prouver par le chemin réel (`invoke('list_sources')`) où sont les sources perso.
2. Si l'écart vient d'un comptage erroné, le dire et corriger le document — pas de code.
3. Corriger le vrai défaut visible : un script refusé par le moteur doit l'être à l'écran.
4. Rouge d'abord : le test qui prouve que l'avertissement du moteur est retenu.
5. Vert, portes, journal, commit.

**D1 est un VRAI défaut, reproduit.** J'ai conclu le contraire deux fois, et
les deux fois à tort : un `invoke` rendait 1372, et je n'avais pas vérifié que
le journal contenait aussi 1373. Il en contient **42 appels**.

| Étape, mesurée | Moteur | Liste blanche | En-tête affiché |
|---|---|---|---|
| Avant dépôt | 1372 | 1372 | 1372 |
| `autre-source.js` déposé, chargé par le moteur | **1373** | 1372 | **1372** |
| **On ajoute le seul nom manquant à la liste** | 1373 | **1373** | **1373** |
| Fichier retiré | 1372 | 1372 | 1372 |

L'écart est **stable sur 8 relits espacés de 10 s** : ce n'est pas React en
retard. La troisième ligne est le « second chemin qui doit réussir » — on ne
change que le nom dans la liste, et l'écart disparaît. **Cause prouvée.**

**Le mécanisme, lu dans le code** : l'en-tête affiche `visibleSources.length`,
et `visibleSources` applique
`enabled === null ? sources : sources.filter(s => enabled.includes(s.name))`.
`enabled` vient de `localStorage['tatami.enabledSources']` — une **liste
fermée** de 1372 noms, écrite par « Tout activer » et par le basculement d'une
source.

**Conséquence, plus large que les sources perso** : toute source apparue
**après** l'enregistrement de la liste est filtrée en silence — un script
importé, mais aussi une source neuve apportée par une mise à jour du
catalogue. L'utilisateur a demandé « tout activer » et obtient « tout sauf ce
qui est arrivé ensuite », sans un mot.

**Ce que la correction ne peut pas être** : réconcilier en ajoutant les noms
manquants au chargement. Une source **désactivée** par l'utilisateur est
précisément un nom **absent** de la liste — la réconciliation la réactiverait.
Il faut distinguer « absent parce que désactivé » de « absent parce que jamais
vu », ce qui est un **changement de modèle de persistance**. §2.9 : décision
d'architecture, donc **à trancher par l'humain, pas par l'agent**. Les options
chiffrées sont dans `docs/rapport-smoke-test-2026-10-10.md`.

**Le second défaut, lui, est corrigé dans ce journal** : les Réglages listaient
`mangadexFr.js` exactement comme les deux scripts qui, eux, fonctionnaient.
L'utilisateur n'avait aucun moyen de savoir que le sien ne chargeait pas.

Le moteur écrit ses refus sur **stderr**, jamais sur stdout (cf. contrat), donc
`list_sources` ne pouvait pas les porter, et le superviseur les jetait après
journalisation. Chaîne tirée :

1. `retenir_avertissement(ligne, &mut Vec<String>)` — fonction **pure** et
   testée. Elle ne garde que `AVERTISSEMENT: … Script ignoré …`, la liste est
   bornée à 20, et l'horodatage java.util est retiré. Le niveau de log **n'a
   pas changé** : le journal reste la source de diagnostic du moteur.
2. `Supervisor::avertissements()` + commande `scripts_refuses` → `Vec<String>`.
3. `lib/scripts-refuses.ts` : `refusMoteur()` transforme les lignes en
   `Map<fichier, raison>`. Fonction pure, 7 tests.
4. Panneau Réglages : une puce rouge sur la rangée du script refusé, avec la
   raison du moteur. Jetons `danger-*` **existants**, vérifiés dans les deux
   thèmes ; aucune couleur inventée.

**Portes** : `cargo fmt --check` 0 · `cargo clippy --all-targets -D warnings` 0 ·
`cargo test` 186 (dont `aucune_commande_tauri_nest_morte`, qui prouve que
`scripts_refuses` est bien appelée) · `pnpm typecheck` 0 · `pnpm lint` 0 erreur,
4 warnings connus · `i18n:check` 345 = 345 · `hooks`, `chemins`, `pages`,
`perso`, `ipc` verts · `pnpm test` 77.

**Preuve rouge avant le correctif** : remplacer le marqueur `AVERTISSEMENT:` par
`WARN` fait échouer **2** tests nommés — `un_avertissement_de_script_est_retenu`
et `la_liste_des_avertissements_est_bornee`. C'est le **nom** des tests qui
prouve que la porte vise la bonne chose.

**Preuve sur l'application réelle** (pas sur le code) :

| | sombre | clair |
|---|---|---|
| Lignes affichées | 3 | 3 |
| Lignes **avec** refus | **1** — `mangadexFr` | **1** |
| Texte rendu | `Refusée par le moteur : … <eval>:20:36 Missing close quote` | idem |
| Largeur de la puce | 508 px (donc visible, pas de largeur nulle) | 508 px |
| Couleur **calculée** du texte | `rgb(255,176,176)` — jeton sombre | `rgb(161,27,27)` — jeton **clair** |
| Contraste sur fond **composé** | **8,16:1** | **6,37:1** — AA |

La preuve qui distingue les deux cas : `scan-manga` et `mangadex-api` n'ont
**aucune** puce. Une preuve qui ne les distingue pas ne prouverait rien.

**Ce que ce correctif ne prouve pas** : l'absence de puce signifie « le moteur
n'a rien signalé », pas « le moteur a chargé le fichier ». Un script importé
après le démarrage que le `WatchService` n'aurait pas vu resterait sans puce. Je
n'ai pas voulu inventer une heuristique « absent du registre → refusé » : elle
produirait un faux positif dans la minute qui suit un import, ce qui est pire
que le silence.

**État de la machine, restauré et vérifié** : `autre-source.js` retiré (le
moteur a journalisé le retrait), liste blanche revenue à 1372 noms sans le nom
ajouté, thème remis à `sombre`.

### Journal 2026-10-10 — la loupe du webtoon, puis son retrait

**Question de l'utilisateur** : « pourquoi une loupe apparaît quand la flèche
est sur le webtoon ? »

**Réponse, et ce n'était pas un bug** : `globals.css` posait `cursor: zoom-in`
sur `.webtoon img:hover`, et le geste existe bellement — `onDoubleClick` bascule
100 % ↔ 200 %. La loupe annonçait donc un vrai raccourci.

**Le vrai défaut, petit** : le curseur restait `zoom-in` **même à 200 %**, où le
prochain double-clic **réduit**. Il faisait promettre l'inverse de ce qui arrive.
Proposé, l'utilisateur a d'abord choisi « le rendre honnête ».

**Décision finale de l'utilisateur** : « enlève-le finalement ». La loupe
clignotait sur toute la bande pendant la lecture ; le raccourci reste mais n'est
plus **annoncé**. Les deux règles `cursor:` sont supprimées, et la classe
`zoom-max` avec elles — sinon elle serait du code que plus rien ne lit.

Pas de `cursor: default` en remplacement : la valeur par défaut d'un `<img>`
est déjà `default`, la règle serait du bruit. Un commentaire dans
`globals.css` dit ce qui a été retiré et pourquoi, pour que personne ne le
« répare ».

**Conservé au passage** : `ZOOM_MIN` / `ZOOM_MAX`. Le `200` était écrit en dur à
**trois** endroits (borne de `changeZoom`, `max` du curseur, seuil du
double-clic) — trois copies d'une même borne finissent toujours par diverger.
Elles ne servent plus de seuil à un curseur, mais la borne, elle, est utilisée
trois fois.

**Portes** : `typecheck` 0 · `lint` 0 erreur, 4 warnings connus · `hooks` vert ·
`test` 92 · cinq portes de script vertes · `build` OK · vérifié qu'il ne reste
**aucune** règle `cursor: zoom` (le seul `zoom-in` restant est dans le
commentaire qui explique le retrait).

### Journal 2026-10-10 — saut de page 3 à 48 : la page ne s'affichait pas

Signalé par l'utilisateur : « quand je passe de la page 3 à la page 48, la page
ne s'affiche pas ; je dois revenir à la page 3 et continuer la lecture pour
qu'elle s'affiche ».

**Cause, lue dans le code** : `admis` est l'ensemble des pages qui ont le droit
de recevoir un `src` — donc de demander une image. Il ne grandissait **que** sur
un `onLoad` : les 6 premières pages à l'ouverture, puis une par décodage (le
quota est une **file** contre le `Mutex` du moteur, cf. `page.tsx` l. 1291).

Un **saut** cassait ce mécanisme. La fenêtre de montage se déplace sur la cible,
mais aucune de ses pages n'est admise : pas de `src` → pas de chargement → pas
de `onLoad` → plus aucune admission. **La bande reste vierge**, et seule une
remontée manuelle débloque.

C'est le **même circularity** que le changement de chapitre corrigé quelques
heures plus tôt : une garantie qui ne se déclenche que sur un événement qui ne
peut pas arriver n'en est pas une.

**Correctif** : `lib/quota-pages.ts` — `admettreLaCible(prec, cible)`, appelée
depuis les trois branches de `scrollToPage`. `admettreUne` est **déplacée** dans
le même module : écrite à côté du lecteur, aucune porte ne pouvait la vérifier
(même leçon que `bande-hauteurs.ts`).

Le quota reste une file : admettre la cible ne vide pas le mécanisme, et un test
vérifie que la page suivante entre quand même.

**Preuve rouge** : neutraliser `admettreLaCible` fait échouer
`un saut admet la page demandée` **et**
`un saut puis la lecture normale continuent d’admettre` — la file ne se remplit
plus, ce qui est le défaut décrit par l'utilisateur.

**Portes** : `pnpm test` 92 · `typecheck` 0 · `lint` 0 erreur, 4 warnings connus ·
`hooks` vert · `build` OK.

**Ce qui n'est PAS mesuré** : pas de vérification sur l'application réelle, et
c'est un manque que j'assume ici. Ton `tauri dev` n'expose pas le port CDP 9224,
et lancer une seconde instance échoue sur un conflit de profil WebView2
(HRESULT 0x8007139F). Le lien de causalité est donc **établi par le code et par
une preuve rouge**, pas par une mesure du geste.

**Pour lever ce manque, une ligne** dans le shell qui lance `tauri dev` :

```powershell
$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS = '--remote-debugging-port=9224'
pnpm tauri dev
```

Avec ça, je rejoue le saut 3 → 48 et je mesure `scrollTop`, la page visible et
les `admis` avant/après — la même preuve que pour les deux défauts précédents.


Signalé par l'utilisateur : « le défilement par flèches haut et bas fait encore
des sauts, je préfère que ça défoule progressivement, sans saut, comme CDisplayEx ».

**Ce qui était déjà juste** : le correctif du 2026-10-09 tient — les flèches ne
changent plus de page, elles appellent `scrollBy`. Vérifié dans le code
(`page.tsx:3934`).

**Ce qui restait** : le PAS. `pasDeDefilement()` valait **80 % du cadre**, soit
**546 px** sur un cadre mesuré de 683 px.

**Le critère, exprimé au lieu de deviné** : une page de webtoon mesure ~1 097 px
(One Piece ch. 900 : 17 pages pour 18 650 px de bande, cadre 683 px). Un pas qui
approche une page est un saut, quelle que soit sa forme. 546 px en faisait la
**moitié** — c'est bien ce que l'utilisateur sentait.

**Correctif** : un **huitième** du cadre → **85 px**, moins d'un douzième de
page. Plancher passé de 80 à 48 px pour rester sous le pas normal : sinon un
cadre moyen basculerait entre deux régimes.

La vitesse ne vient pas de la taille du pas mais de la **répétition** : un appui
maintenu répète ~30 fois par seconde. Le critère retenu n'est donc pas « dix
appuis = une page » (mesuré : 10 × 85 = 850 px, une page en fait 1 097) mais
« **une page en 13 appuis** » — assez vite pour lire, assez lent pour suivre.

**Preuve rouge** : remettre le ratio à 0.8 fait échouer **trois** tests, dont
deux avec un message qui chiffre le défaut —
`un pas de 546 px franchit trop d'une page de 1097 px` et
`3 appuis pour une page, c'est redevenu un saut`.

**Portes** : `pnpm test` 86 · `typecheck` 0 · `lint` 0 erreur, 4 warnings connus ·
`hooks`, `i18n`, `chemins`, `pages`, `perso`, `ipc` verts · `build` OK. Rust non
touché.

**Ce qui n'a PAS été mesuré, et pourquoi** : pas de vérification sur l'app
réelle. L'instance lancée par `tauri dev` n'expose pas le port CDP 9224, et lancer
une seconde instance a mesuré `failed to create webview` (HRESULT 0x8007139F) —
conflit de profil WebView2. Je n'ai pas relancé pour ne pas déranger la session
en cours. Le changement est donc **prouvé sur la fonction, pas sur le geste** :
à confirmer au feel, et le réglage est une seule ligne si 85 px ne convient pas.

### Journal 2026-10-10 — « chapitre suivant » arrivait au milieu du chapitre

Signalé par l'utilisateur : en lisant un webtoon, changer de chapitre le
renvoyait sur une page du webtoon au lieu du début.

**Reproduction mesurée** (SCANVF, One Piece) :

| | `scrollTop` | 1ʳᵉ image visible | compteur |
|---|---|---|---|
| chapitre 900 ouvert | 0 px | page 0 | 7 / 17 |
| …descendu à la page 13 | 12 000 px | page 13 | 12 / 17 |
| **« chapitre suivant »** | **10 335 px** | **page 13** | **1 / 15** |

Le compteur annonçait « 1 / 15 » pendant que la bande montrait la page 13 — le
piège inverse de celui déjà consigné : ici c'est l'*indicateur* qui a raison et
l'*écran* qui a tort.

**Cause** : `.webtoon` est un conteneur de défilement **conservé** d'un chapitre
à l'autre. `aPositionner.current = index > 0 ? index : null` mettait `null`
pour `index === 0`, et l'effet de positionnement sortait alors sur
`if (i === null) return` — **personne ne remontait**, donc le `scrollTop` du
chapitre précédent survivait.

**Ce qui rendait le correctif évident à manquer** : `bandeVirtualisable` ne
devient vrai qu'à la première image **décodée**. Positionner en attendant cette
image est un **circularity** : le décalage périmé plaçait le viewport hors de la
fenêtre de téléchargement, donc **1 image admise sur 10, 0 décodée en 90 s**,
donc pas de `bandeVirtualisable`, donc pas de repositionnement. Sans casser le
circularity, ajouter le cas `index === 0` n'aurait rien donné — c'est le premier
essai, mesuré vert et faux.

**Correctif** : la première page ne dépend d'aucune dimension — le haut est le
haut. Elle se positionne donc immédiatement, hors de la porte
`bandeVirtualisable`.

**Non-régression vérifiée sur la branche `i > 0`** : ouvert un chapitre, sauté
à la page 8 par le formulaire « Page » → `scrollTop` 7 411 px, **page 7**
affichée, compteur « 8 / 17 ». La branche offset n'a pas bougé.

**Portes** : `pnpm typecheck` 0 · `pnpm lint` 0 erreur, 4 warnings connus ·
`pnpm hooks:check` vert · `pnpm build` OK · `pnpm test` 84 (inchangé, aucune
fonction pure touchée). Rust non touché sur ce tour.

### Journal 2026-10-10 — option A appliquée : une source nouvelle est activée d'elle-même

**Mini-plan.** 1. Test rouge de la réconciliation. 2. Prouver que la porte mord,
en cassant la branche migration. 3. Brancher sur `chargerSources()`. 4. Rejouer
la reproduction sur l'application réelle, en trois temps. 5. Nettoyer, journal,
commit.

`lib/liste-blanche.ts` : `reconcilierListeBlanche({ nomsConnus, enabled, seen })`
→ `{ enabled, seen, nouveaux, migration, modifie }`. Fonction **pure**, 7 tests.

La règle tient en une phrase : **un nom qui est dans `seen` a déjà été vu, donc
son absence est un choix ; un nom hors de `seen` est arrivé depuis, donc il est
activé.** La première exécution ne rajoute rien — c'est la seule hypothèse qui ne
réactive pas une source désactivée.

**Portes** : `pnpm test` 84 · `typecheck` 0 · `lint` 0 erreur, 4 warnings connus ·
`hooks`, `i18n` (345 = 345), `chemins`, `pages`, `perso`, `ipc` verts ·
`pnpm build` OK. Rust inchangé sur ce tour, donc ses portes ne sont pas
rejouées — elles l'étaient au commit précédent, sur le même arbre.

**Preuve rouge de la porte** : remplacer la branche « migration » par la
réconciliation naïve (ajouter tous les noms manquants) fait échouer **un seul**
test, `migration : une liste existante ne réactive rien`. C'est exactement le
bug que le modèle évite, et c'est le seul qui tombe.

**Preuve sur l'application réelle** — trois temps, parce qu'un seul ne
distinguerait pas « activé parce que nouveau » de « activé à tort » :

| Temps | Situation | Résultat mesuré |
|---|---|---|
| **T1** | `SCANVF` retirée de la liste (désactivée) + clé `seen` **effacée** | liste passée de 1372 → **1371**, `SCANVF` **absente** → **elle n'est pas revenue** |
| **T2** | `autre-source.js` déposé, chargé par le moteur | moteur **1373**, en-tête **1373** — **sans aucune action de l'utilisateur** |
| **T3** | rechargement complet | toujours **1373 / 1373** — la réconciliation tient |

T1 est la preuve qui compte : c'est le seul test qui prouve qu'on n'a pas
simplement tout réactivé. T2 est le défaut d'origine, corrigé.

**Ce que le correctif ne fait pas, et pourquoi** : il **ne purge pas** les noms
morts de `enabledSources`. Une source importée puis supprimée y laisse son nom
(1373 après le retrait de `autre-source.js`, alors que le moteur est revenu à
1372). Sans effet fonctionnel — le catalogue filtre `sources`, pas l'inverse —
et « Tout activer » réécrit la liste, donc purge à la demande. La purge
automatique serait dangereuse : si `list_sources` répondait vide ou partiel
pendant un incident, un `nomsConnus` tronqué **viderait la sélection de
l'utilisateur**. Je préfère une liste qui grossit à une liste qu'un incident
peut effacer.

`seenSources` n'est volontairement **pas** purgé, et c'est là que c'est utile :
une source disparue puis revenue par une mise à jour a déjà été vue, donc elle
**n'est pas réactivée** contre le choix de l'utilisateur.

**Machine restaurée et vérifiée** : `autre-source.js` retiré, `enabledSources`
et `seenSources` revenues à 1372, moteur 1372, 2 sources perso, en-tête 1372.

**§S.2 du smoke test reste donc en échec**, et sa cause est la liste blanche.
Le contrôle lui-même était juste : il comparait bien ce qui est enregistré à ce
qui est servi. Ce qui manquait, c'était le **troisième** terme — la liste
blanche, qui n'est ni dans `list_sources` ni dans `list_disabled_sources`.
Il faut la lire (`localStorage['tatami.enabledSources']`) avant de conclure
qu'une source est perdue.

**Le vrai défaut, corrigé dans le même mouvement** : les Réglages listaient
`mangadexFr.js` exactement comme les deux scripts qui, eux, fonctionnaient.
L'utilisateur n'avait aucun moyen de savoir que le sien ne chargeait pas.

Le moteur écrit ses refus sur **stderr**, jamais sur stdout (cf. contrat), donc
`list_sources` ne pouvait pas les porter, et le superviseur les jetait après
journalisation. Chaîne tirée :

1. `retenir_avertissement(ligne, &mut Vec<String>)` — fonction **pure** et
   testée. Elle ne garde que `AVERTISSEMENT: … Script ignoré …`, la liste est
   bornée à 20, et l'horodatage java.util est retiré. Le niveau de log **n'a
   pas changé** : le journal reste la source de diagnostic du moteur.
2. `Supervisor::avertissements()` + commande `scripts_refuses` → `Vec<String>`.
3. `lib/scripts-refuses.ts` : `refusMoteur()` transforme les lignes en
   `Map<fichier, raison>`. Fonction pure, 7 tests.
4. Panneau Réglages : une puce rouge sur la rangée du script refusé, avec la
   raison du moteur. Jetons `danger-*` **existants**, vérifiés dans les deux
   thèmes ; aucune couleur inventée.

**Portes** : `cargo fmt --check` 0 · `cargo clippy --all-targets -D warnings` 0 ·
`cargo test` 186 (dont `aucune_commande_tauri_nest_morte`, qui prouve que
`scripts_refuses` est bien appelée) · `pnpm typecheck` 0 · `pnpm lint` 0 erreur,
4 warnings connus · `i18n:check` 345 = 345 · `hooks`, `chemins`, `pages`,
`perso`, `ipc` verts · `pnpm test` 77.

**Preuve rouge avant le correctif** : remplacer le marqueur `AVERTISSEMENT:` par
`WARN` fait échouer **2** tests nommés — `un_avertissement_de_script_est_retenu`
et `la_liste_des_avertissements_est_bornee`. C'est le **nom** des tests qui
prouve que la porte vise la bonne chose.

**Preuve sur l'application réelle** (pas sur le code) :

| | sombre | clair |
|---|---|---|
| Lignes affichées | 3 | 3 |
| Lignes **avec** refus | **1** — `mangadexFr` | **1** |
| Texte rendu | `Refusée par le moteur : … <eval>:20:36 Missing close quote` | idem |
| Largeur de la puce | 508 px (donc visible, pas de largeur nulle) | 508 px |
| Couleur **calculée** du texte | `rgb(255,176,176)` — jeton sombre | `rgb(161,27,27)` — jeton **clair** |
| Contraste sur fond **composé** | **8,16:1** | **6,37:1** — AA |

La preuve qui distingue les deux cas : `scan-manga` et `mangadex-api` n'ont
**aucune** puce. Une preuve qui ne les distingue pas ne prouverait rien.

**Ce que ce correctif ne prouve pas** : l'absence de puce signifie « le moteur
n'a rien signalé », pas « le moteur a chargé le fichier ». Un script importé
après le démarrage que le `WatchService` n'aurait pas vu resterait sans puce. Je
n'ai pas voulu inventer une heuristique « absent du registre → refusé » : elle
produirait un faux positif dans la minute qui suit un import, ce qui est pire
que le silence.

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
### Journal 2026-10-09 — pilotage réel de l'app, et diagnostic des images cassées

> **Correction du 2026-10-09 09:41.** La conclusion « une partie des pages
> annoncées par `pages` n'existe pas sur le CDN » écrite plus bas est **fausse**,
> et la section « Ce que le refus disait » a été réécrite en conséquence.
> La cause réelle est un **blocage TLS de MangaDex sur cette machine**. Le
> commit `79d90c8` est conservé — il améliore le diagnostic — mais sa
> justification était erronée. Détail dans « Blocage MangaDex (2026-10-09) »
> plus bas.

**Contexte.** Session de clôture : vérification par pilotage de l'application réelle
(webview pilotée en CDP, port 9224 — le port 9223 était tenu par **Vaultly**, autre
application, laissée intacte). Aucun des correctifs du 2026-10-08 n'avait encore été
éprouvé dans l'application : ils n'avaient passé que des portes.

**Portes au moment de la clôture**

```
engine : gradle test --rerun-tasks  →  64 tests, 0 échec, 15 classes
rust   : cargo clippy --all-targets -- -D warnings  →  0 avertissement
front  : inchangé depuis ca79689 (typecheck, i18n 307=307, hooks, chemins, build)
```

**Ce que le pilotage a mesuré (source : journal Rust, pas le compteur de pages).**

Le protocole `tatami://` n'apparaît dans **aucun** événement `Network` de CDP
(0 événement pour 12 images réellement chargées) : le débit se lit donc dans le
journal Rust, qui écrit une ligne par requête partie.

| Mesure | Valeur |
|---|---|
| Attribut `loading` servi | `lazy` ✓ |
| Requêtes `fetchImage` à l'ouverture (chapitre de 50 pages) | **7 à 24** (au lieu des 50) |
| Images dans le DOM | 10 à 24 (au lieu de 50) |

La rafale est donc bornée — `e765e8e` fonctionne, mesuré.

**Ce que le refus disait — et ce qu'il ne disait pas.** Le journal portait
`IllegalStateException: HTTP 404 pour image body`. Après ajout du corps de la
réponse au refus (`79d90c8`), le message est :

```
HTTP 404 pour image body [type=inconnu] sans corps
```

Ni corps ni type de contenu : ce n'est pas une page d'erreur, c'est un refus de
connexion.

**Ce que ce 404 signifiait réellement — relire avant de conclure « page
manquante ».** J'en avais conclu qu'une partie des pages n'existait pas sur le
CDN, parce que `curl` donnait 404 / 0 octet sur la page 5 et 200 / 114 478
octets sur la page 1. **C'était un raisonnement arrêté trop tôt** : je n'avais
pas vérifié si l'**API** répondait, alors que c'est elle qui produit ces URL.
Elle ne répondait plus — voir « Blocage MangaDex » plus bas. Le 404 venait du
blocage, pas d'une page absente.

**Règle qui en découle : avant d'accuser la donnée, vérifier que le producteur
de cette donnée répond.** Une URL est une valeur *reçue* ; quand elle échoue,
la question n'est pas « cette URL est-elle bonne ? » mais « qui me l'a donnée,
et répond-il encore ? ». Ici le producteur était `at-home`, et il était mort.

**Deux fausses causes payées avant la bonne** (à relire avant d'accuser une couche) :

1. « le moteur embarqué a trois jours de retard » — lu sur des JAR de
   dépendances tierces (`asm-*`, inchangés depuis le 7 octobre), qui ne disent
   rien du code de la session. Faux : le JAR contenait bien la classe.
2. « le CDN ne sert les images qu'en HTTP/1.1 » — un 400 en clair du CDN
   (`You must set an appropriate User-Agent header`) rendait la piste
   convaincante. Correctif **écrit**, test rouge **prouvé**, moteur **reconstruit** ;
   une sonde Java l'a ensuite infirmée (OkHttp en h2 rend 200 sur la même URL).
   Correctif **et** test retirés — ils ne sont pas dans le dépôt.

Le filet : AGENTS.md §2.9 « ne pas attribuer un correctif à la première cause
plausible ». Sans elle, une modification fausse aurait été livrée et documentée.

**Portes prouvées rouges puis vertes** (`DiagnosticImageTest`, hors réseau —
`Response` fabriquée) : rapport du texte du CDN et du type de contenu ; anti-fuite
d'URL ; bornage de la longueur. Le test de bornage ne mord qu'en retirant **deux**
bornes d'un coup (`peekBody` et `take` se recouvrent partiellement) — consigné
comme limite assumée.

**État non résolu, à ne pas perdre de vue :**

- **Le lecteur reste cassé**, et la cause est **extérieure à ce dépôt** :
  MangaDex bloque l'API en TLS depuis cette machine (voir « Blocage MangaDex »).
  La rafale est corrigée et les échecs sont désormais *nommés* à l'écran — ce qui
  est honnête mais pas confortable. L'objectif de lecture n'est **pas** atteint,
  et **aucun code à écrire ne le changera** tant que le blocage dure.
- **`package-lock.json`** (36 Ko, non suivi) : **supprimé** le 2026-10-09 sur
  décision humaine ; le projet suit `pnpm-lock.yaml`. Non suivi, donc la
  suppression n'a produit aucun commit.
- **Commits poussés** : 176 commits envoyés sur `origin/master` (`b6ba03b`),
  après recherche de secrets dans l'historique et vérification de divergence.
- **9 commits depuis `9df864e` n'étaient pas consignés** avant cette entrée, dont
  la suppression du sous-système de traduction (`20e4287`, 2026-10-08) et 13
  correctifs d'audit du même jour, également hors journal.

**Hors dépôt, non touché** : `%APPDATA%\Tatami\packs` (1 102 Mo de modèles ONNX
devenus inutiles après `20e4287`) et `cache` (4 339 fichiers, 1 111 Mo). Le cache
ne contient **aucun** fichier vide — cohérent avec le refus qui refuse désormais
d'écrire des octets non-image.

### Blocage MangaDex (2026-10-09 09:41) — la cause réelle des images cassées

Mesuré sur cette machine, `curl` :

| Cible | Code |
|---|---|
| `mangadex.org` (site) | **000** |
| `api.mangadex.org/ping` | **000** |
| `api.mangadex.org` forcé sur son IP (`--resolve`) | **000** |
| `*.mangadex.network` (nœud d'images) | **200** |
| `kitsu.io`, `github.com`, `example.com` | **200** |

Le domaine **résout** (`45.129.229.1`, `45.129.229.2`), mais le handshake TLS
échoue en `SEC_E_ILLEGAL_MESSAGE` : MangaDex ferme la connexion au niveau TLS.

**Ce n'est pas un blocage d'IP**, et la mesure le prouve : les images servent
du 200 depuis la même machine et le même réseau, et tous les autres sites
répondent. Le frontal de l'API et le site web nous rejettent ; le réseau
d'images non. Forcer `--tlsv1.2` ou `-k` ne change rien (`000` dans les deux
cas) — ce n'est pas un problème de certificat.

**Pourquoi cela explique la contradiction des mesures.** Le lecteur fait deux
appels distincts : l'API (`at-home`, qui produit les URL) puis les images.
L'API est coupée, les images passent. D'où des 404 sur certaines pages et des
200 sur d'autres, sans que rien ne manque du côté du contenu.

**Conséquence : aucun correctif applicatif.** Le lecteur fonctionnera de nouveau
seul quand MangaDex cessera de nous bloquer. Tester le lecteur suppose donc
soit d'attendre, soit de viser une autre source.

**Piège à ne pas rejouer.** J'avais écrit dans ce journal qu'une partie des
pages n'existait pas chez MangaDex. C'était faux, et la faute est précise :
j'avais vérifié que l'**URL** d'une image échouait, sans vérifier que
l'**API qui l'a produite** répondait. Une URL est une valeur *reçue* ; quand
elle échoue, il faut demander « qui me l'a donnée ? » avant de conclure qu'elle
est mauvaise. Ici `at-home` était mort depuis le début de la session — et rien,
dans mes sondes, ne le demandait.

**Au sujet du message du commit `79d90c8`.** Il affirme que « la cause réelle
est qu'une partie des pages n'existe pas sur le CDN ». C'est faux, et le message
reste dans l'historique : il a été **poussé**, donc le réécrire réécrirait
l'historique distant. On l'amende ici, ce qui est la pratique pour un commit
déjà publié. Ce que `79d90c8` apporte reste **juste et utile** : le refus
d'image nomme désormais sa cause (`[type=inconnu] sans corps`). Sans ce
changement, le 404 restait muet et rien n'aurait permis de distinguer un
problème de données d'un problème d'accès — ce dernier demande justement de
regarder si l'API répond, et c'est en ayant le refus documenté qu'on s'y est
mis.

> **Périmètre, 2026-10-10 — suivi externe (AniList/MAL/Shikimori/Kitsu) et synchronisation cross-device retirés du périmètre v1** (ADR 0015). Aucun code n'existait pour ces fonctionnalités. Reportés en v1.1.
>
> Note : aucune mention de ces fonctionnalités n'existe dans les sections de tâches de PLAN.md (la seule occurrence de `kitsu.io` est dans un tableau de test réseau, ligne 1120). L'ADR documente la décision de périmètre sans modification de tâches existantes.
### Correctif 2026-10-09 — le lecteur dit qu'une page n'est pas servie

**Ce qui est corrigé.** Le lecteur affichait une `<img>` cassée, sans message,
pour toute page que la source ne sert pas. C'était le **seul défaut qui
restait de notre côté** : le 404 lui-même vient de MangaDex et aucun code ne
le changera.

**La mesure qui le fonde** (`custom:mangadex-api`, Dragon Ball, chapitres FR,
par le même code et dans la même seconde) :

| chapitre | pages annoncées | servies |
|---|---|---|
| ch. 1 | 32 | **7/8** sondées, ~200 Ko de JPEG chacune |
| ch. 3 | 15 | **0/8** sondées, toutes refusées en 404 |

MangaDex sert certains chapitres et en perd d'autres. Ni le moteur, ni le
script, ni l'interface ne sont en cause — le chemin complet a été éprouvé
par le vrai script de la source, ce qui n'avait jamais été fait.

**Le correctif.** Un `<img>` sans `onError` laisse le 404 muet : le lecteur
remplace l'image par un encart « page indisponible chez la source », et la
barre d'outils affiche le bilan (« n pages sur total ne sont pas servies par
la source »). L'état `pagesEnErreur` est remis à zéro à chaque ouverture de
chapitre — sinon les index du chapitre précédent s'afficheraient sur le
suivant, qui peut être intact.

Le message **accuse la source, pas le réseau** : c'est vérifié, et un message
qui accuserait le réseau serait faux.

**Porte** : `pnpm pages:check` (`scripts/check-pages-indisponibles.mjs`),
ajoutée aux scripts `package.json` et à la liste des portes d'`AGENTS.md`.
Rouge prouvée en retirant le `onError`, verte après. Elle a dû être réécrite :
sa regex s'arrêtait à la première `/>` interne et ne voyait pas la balise.

**Portes** : typecheck 0 · lint 0 erreur (4 warnings `exhaustive-deps` connus)
· i18n 309 = 309 · hooks · chemins · pages · build.

**Leçon du même jour, consignée dans AGENTS.md** : cinq tokens CSS inventés
(`--t-petit`, `--r2`, `--fond-doux`, `--texte-doux`, `--bordure`) — un `var()`
non défini ne casse rien, donc rien ne le signale ; et une porte restée verte
parce que sa regex était fausse, pas parce que le code était bon.
**Vérifié dans l'application réelle** (pilotage CDP, 2026-10-09, source
`custom:mangadex-api`, Dragon Ball FR, 27 chapitres parcourus) :

| Mesure | Valeur |
|---|---|
| Images affichées **sans message** | **0** (avant : toutes les pages en échec) |
| Chapitres entièrement servis | encarts « page indisponible » + bilan |
| Chapitre 27 (mixte) | **1 image servie + 6 encarts**, bilan « 6 pages sur 7 » |

Les 27 chapitres parcourus n'ont produit **aucune** image cassée sans message.
Le chapitre 27 prouve les deux comportements dans le même écran : une image
qui s'affiche, et des pages nommées.
#### Les trois pistes fausses du 2026-10-09 — ce qu'il aurait fallu mesurer

Elles sont consignées en §14, mais le **chemin** compte autant que la
conclusion : chacune a été franchie parce qu'une vérification simple a été
sautée. Le tableau dit quoi faire, pas seulement quoi écrire.

| # | Piste fausse | Ce qui la rendait fausse | La vérification sautée | Le geste qui l'aurait arrêtee |
|---|---|---|---|---|
| 1 | « le moteur embarqué est périmé » | le JAR contenait la classe ; on lisait des JAR de dépendances tierces (`asm-*`, inchangés) | **ne pas lire l'artefact, lire son horodatage** | lister le contenu du JAR et chercher la classe |
| 2 | « le CDN ne sert qu'en HTTP/1.1 » | OkHttp en h2 rend 200 sur la même URL | **varier le transport avant d'accuser le protocole** | un client h2 sur l'URL qui échoue |
| 3 | « MangaDex bloque notre IP » | l'API répond 200 en Node ; seul `curl` est refusé | **tester le producteur de l'URL, pas l'URL** | `fetch` sur l'API avant de conclure sur l'image |

**La règle qui les couvre toutes les trois :** une conclusion tirée d'un
échec doit être testée par **un second chemin qui devrait réussir**. Si les
deux échouent, la cause est plus haute ; si le second réussit, la première
mesure était fausse. C'est le pendant direct du filet déjà cité (§2.9) :
ne pas attribuer un correctif à la première cause plausible — ici, ne pas
attribuer une **cause** au premier échec plausible.

Coût des trois détours : une session entière. Coût du geste : une commande.
### Les trois chantiers de la note 7/10 — 2026-10-09

#### 1. Dimensions et virtualisation (corrigé, mesuré dans l'app)

**Deux défauts distincts, trouvés l'un après l'autre.**

**(a) Les dimensions ne survivaient pas au redémarrage.** `ecrire_cache` les
calcule sur les octets en mémoire et les confie à `stocker_dims` ; rien n'est
écrit sur le fichier. Au lancement suivant, `lire_dims` rendait `None` pour
toute page, donc `bandeVirtualisable` valait `false`. Or les octets **sont**
sur le disque et leur en-tête porte ces deux nombres : `lire_dims` les relit
désormais, et mémorise le résultat. Coût : un `read` de l'en-tête, et
`image::dimensions` s'arrête au marqueur `SOF`/`IHDR` sans jamais encoder.

**Ce correctif a supprimé un test qui figeait le défaut.**
`dims_absentes_apres_redemarrage_du_cache` affirmait l'absence de
persistance et attribuait ce comportement à l'ADR-0012. Un test qui décrit le
défaut comme une intention empêche de le corriger ; il est remplacé par
`l_index_est_vide_apres_redemarrage_du_cache` (l'index, lui, doit bien se
remettre à zéro).

**(b) Une page sans dimensions désactivait toute la virtualisation.** Découvert
en mesurant (a) : `bandeVirtualisable` exige `h > 0` pour **toutes** les
pages, or une page que la source ne sert pas vaut `0`. Une seule suffisait.
Mesuré dans l'app avant correctif :

| chapitre | pages | images affichées | hauteur | bande |
|---|---|---|---|---|
| ch. 6 | 50 | 12, toutes chargées | 967 px | **non virtualisée** |

Les dimensions étaient donc connues — c'est la bande qui était bloquée. Une
hauteur de repli (1 600 px, médiane d'une page de webtoon) rend la hauteur
toujours positive.

**Vérifié dans l'application réelle** : 50 pages annoncées, **5 dans le DOM**,
bande à hauteur fixe, hauteurs 897 px, 0 page en échec. Virtualisation active
dès la première ouverture.

Rust : 174 tests verts. Front : `pnpm test` 8 tests, typecheck, lint, i18n,
hooks, chemins, pages, build.

#### 2. Tests unitaires côté front (créés)

`lib/bande-hauteurs.ts` extrait de `page.tsx`, avec `lib/bande-hauteurs.test.ts`
(8 tests, `node --test` natif, aucune dépendance). La logique était dans le
composant, donc **aucune porte ne pouvait la vérifier** : c'est la fragilité
relevée dans la note.

Porte rouge prouvée en neutralisant la hauteur de repli : 3 tests échouent.
`pnpm test` câblé dans `package.json`, `**/*.test.ts` exclu de `tsc` (il
n'importe qu'en `.ts`, que le bundler refuse).

#### 3. Les trois pistes fausses (consignées)

Un tableau dans le journal de cette session : pour chaque piste, ce qui la
rendait fausse et **le geste qui l'aurait arrêtée**. La règle qui les couvre
toutes : **une conclusion tirée d'un échec doit être testée par un second chemin
qui devrait réussir.**

Coût des trois détours : une session. Coût du geste : une commande.
### Tuile « Sources personnalisées » — 2026-10-09

**Demande** : que la card soit comme les autres cards du catalogue, et que le clic
ouvre les sources personnalisées.

**Avant** : une `<section className="lang-tile import-card">` avec
`grid-column: 1 / -1` — donc **toute la largeur**, ce qui la faisait lire comme un
encart parmi les 20 tuiles. Elle portait en plus la liste des sources et le bouton
« importer ».

**Après** : une tuile ordinaire dans la grille des langues. Le clic mène à une vue
dédiée (sur le modèle exact de `lang`), qui porte le bouton « importer » et le
retour à la grille. Choix humain explicite : tout dans le panneau.

**Vérifié dans l'application réelle** (pilotage CDP) :

| Mesure | Avant | Après |
|---|---|---|
| Largeur de la tuile perso | 100 % de la grille | **203 px**, identique à une tuile de langue |
| Hauteur | — | **259 px**, identique |
| Tuiles dans la grille | 19 langues + 1 encart | **20 tuiles homogènes** |
| Clic | — | grille disparue, **« MangaDex (API) »** affichée, 2 boutons, bandeau « 1 source · Sources personnalisées » |

**Un bug que seule la mesure a trouvé.** J'avais mis `enTiles` à `true` pour
`perso` (« c'est une vue en tuiles, donc retour possible »). Le clic alors
**changeait bien `lang`** — mesuré : le chip actif passait de « Toutes » à
aucun — mais la grille restait affichée : la tuile ne menait nulle part. La
grille et la vue liste sont deux branches **exclusives** ; `perso` doit tomber
dans la seconde.

**Porte** `pnpm perso:check`. Rouge prouvée, verte après. Elle a dû être réécrite :
elle cherchait `grid-column: 1 / -1` dans **tout** le CSS, donc elle tombait sur
le **commentaire** qui explique l'ancien défaut. Un motif cité n'est pas un motif
appliqué — même famille que la porte `pages:check`.
### Challenge Cloudflare — le journal ne le voyait pas (2026-10-09)

**Symptôme rapporté** : sur une source à challenge (TOONFR), la fenêtre de
résolution se rouvre à chaque essai, et le cycle ne se termine jamais.

**Ce qui a été mesuré.** Le moteur répond bien `CHALLENGE_REQUIRED` avec l'URL
(directement interrogeé hors interface) :

```
{"ok":false,"error":{"code":"CHALLENGE_REQUIRED",
 "message":"challenge at https://toonfr.com/webtoon/","url":"…"}}
```

Mais le journal Rust n'enregistrait que `stdout recu id=… len=169` — **la
longueur seule**. Le code d'erreur, qui est la seule information utile, était
jeté. Un cycle de vérification était donc littéralement **invisible** au
journal : impossible de dire si la fenêtre s'ouvrait, si les cookies partaient,
ou si le site refusait.

**Correctif** : le superviseur note désormais le **code** d'erreur quand la
réponse en porte un (`supervisor.rs`). On ne note que le code, jamais le
message : celui-ci contient l'URL du site, et le journal applique une rédaction.

**Après correctif, sur TOONFR** :

| Mesure | Avant | Après |
|---|---|---|
| Challenges visibles au journal | **0** (invraisemblable) | **2** |
| `setCookies` | 0 | 0 |

Donc : **2 challenges, 0 cookie transmis**. Le second chiffre est la piste
suivante — les cookies ne sont jamais renvoyés au moteur.

**Ce qui n'est PAS conclu.** La cause du `setCookies = 0` reste à établir :
soit l'utilisateur ne valide pas via « C'est fait, réessayer », soit
`solve_challenge` échoue (`challenge.rs:124` renvoie une erreur si aucun
cookie n'est trouvé pour le site — message « la vérification n'est probablement
pas terminée »), soit la fenêtre ne conserve pas ses cookies. **Trois causes,
trois remèdes** : il ne faut pas conclure ici.

**Une fausse piste qu'il vaut la peine de noter.** En interrogeant les commandes Rust via
`window.__TAURI_INTERNALS__.invoke` depuis la sonde, on obtient
`latest not allowed. Plugin not found` — alors que `list_sources` **fonctionne**
à la même seconde (journal : `1372 sources`). `__TAURI_INTERNALS__.invoke` n'est
pas le chemin d'appel de l'application, seulement un point d'entrée brut dont
l'ACL refuse l'usage hors wrapper. Un `not allowed` obtenu ainsi ne prouve rien
sur l'application. Même famille que le `browser.close()` qui tue la webview :
**mesurer par un chemin que l'application n'emprunte pas ne mesure rien.**

Rust : 174 tests verts, `clippy -D warnings` propre.

### Flux challenge — cinq correctifs, une cause encore à classer (2026-10-10)

**Symptôme** : « la vérification Cloudflare se répète en boucle » (même
symptôme que l'entrée du 2026-10-09 ci-dessus, sur TOONFR). Travail mené
en lecture seule d'abord (cartographie complète du flux, sources wry 0.57.0
/ Tauri 2.12.1 / okhttp 5.1.0 / kotatsu-parsers-redo **décompilées** pour
ne rien supposer), puis cinq commits, un par correctif.

**Défauts certains trouvés et corrigés :**
1. **Aucune borne aux tentatives** (`d5a3255`). Compteur pur
   `lib/challenge-garde.ts`, plafond à 2 échecs consécutifs par
   (source, action) ; au-delà plus de bandeau ni de fenêtre, message
   traduit `erreur.source.bloquee`. Remise à zéro au succès. 4 tests.
2. **Réutilisation de fenêtre sans navigation** (`7ee942c`). Au 2ᵉ clic,
   `open_challenge` refocalisait une page pouvant être coincée dans la
   vérification : `navigate()` la relance (choix vs fermer/recréer,
   justifié dans le code). La croix rouge émet `challenge://fermee`,
   le front débloque son indicateur.
3. **Fin de challenge non détectée** (`4838834`). Surveillance toutes les
   1,5 s (max 3 min) : dès que `cf_clearance` **apparaît ou change**, les
   cookies partent sans le clic manuel (qui reste en secours). Témoin de
   départ obligatoire : le profil WebView2 partagé conserve un
   `cf_clearance` périmé, le déclencher bouclerait. Drapeau partagé =
   un seul déclenchement par fenêtre. Test du registre.
4. **Cookies moins transférés que lus** (`a03a838`). `secure`, `httpOnly`
   et l'expiration réelle vont maintenant au moteur (qui forçait
   `maintenant + 365 j`) ; domaine élargi conservé si le cookie en porte
   un plus large que l'hôte. 3 tests Kotlin.
5. **Instrumentation** (`cf6c943`) derrière `TATAMI_CHALLENGE_DEBUG` :
   appels `open_challenge` par source, UA webview réel via JS vs UA
   OkHttp, inventaire des cookies (jamais la valeur), statut + `cf-mitigated`
   / `server` / `cf-ray` + noms de cookies de la requête suivante.

**Mesuré, à garder** : okhttp 5.1.0 accepte `secure()` / `httpOnly()`
**sans argument** (ils posent le drapeau à vrai, bytecode `iconst_1;
putfield`), et `Cookie.Builder.domain(".exemple.com")` **avec son point
ne matche rien** — `domainMatch` compare l'hôte au suffixe `".$domaine"`.
Deux faits établis par `javap`, pas devinés.

**La cause de fond du refus (B/C/D) reste à classer** : les logs
`challenge-debug` la trancheront (UA webview réel vs posé ; `cf_clearance`
présent dans le jar ; statut et en-têtes de la requête rejouée). Analyse
des deux options d'alignement d'UA remise à l'humain — **aucune ne corrige
l'empreinte TLS d'OkHttp**, c'est dit noir sur blanc.

**Incident de travail à deux sessions** : deux fichiers de test non
commités par une autre session (`RequestedRatingsTest.kt`,
`MesureContentRatingTest.kt`) ne compilaient pas et cassaient
`:compileTestKotlin` pour tout le monde. Protocole employé : mise de côté
le temps de la mesure, **restauration vérifiée par empreinte SHA-256**
(identique). À ne jamais faire sans la vérification d'empreinte.


### Images webtoon étirées — cause trouvée, corrigée (ac2979)

**Signalé par l'utilisateur** : « la lecture des webtoons est uniforme ; en
regardant un webtoon, certaines images ou pratiquement tout est étiré ».

**Mesuré dans l'application réelle** (TOOMICSFR, 6 images montée) :

|        | dim. réelles | dim. affichées | height inline | écart |
|--------|--------------|----------------|-----------------|-------|
| avant  | 720x500      | 673x1196       | 1196px          | **-60,9 %** |
| après  | 720x500      | 630x438        | 438px (mesuré)  | **0,1 %** |

**Cause.** Rust ne connaît les dimensions que des pages **déjà en cache**
(commands.rs:304) : un chapitre ouvert pour la première fois n'en a aucune.
Toutes les pages retombaient donc sur HAUTEUR_REPLI — un ratio de 1600/900
inventé, en cb17b06. Or cette valeur était posée en height **inline**, donc
elle écrasait le ratio naturel de l'image : height: auto et
ox-sizing: border-box ne pouvaient plus rien.

**Distinction qui manquait.** hauteurPage sert à **positionner** les pages ;
poser la même valeur sur l'image sert à la **déformer**. Le repli est juste pour
le défilement (discret) et faux pour le rendu (60 %, immédiatement visible).
D'où deux fonctions : hauteurPage (offsets, toujours positive) et
hauteurRendue (style, 
ull quand la mesure manque → le navigateur garde le
ratio naturel). La mesure onLoad remonte désormais 
aturalWidth /

aturalHeight, ce que Rust ne peut pas savoir.

**Réflexe à retenir.** Une valeur d'espacement et une valeur de rendu ne sont
pas la même quantité, même quand elles sortent de la même fonction. Et :
object-fit: contain ajouté en défense — une boîte peut être fausse, l'image
ne doit jamais se déformer.

**Dette résiduelle, mesurée, laissée telle quelle** (décision de l'utilisateur
le 2026-10-09) : la bande réserve 43 650 px pour 33 015 px de contenu réel, soit
**24 % de vide**, parce que les pages non encore mesurées gardent la hauteur de
repli. Navigation par page : **3 sauts justes sur 4**, écart maximal **1 page**.
Le vide est en bas de la bande, donc invisible à l'écran ; l'écart d'une page
vient de la page la plus visible au cadre, pas d'une page erronée.

### Journal 2026-10-10 — suppression de pack.rs orphelin

**Tâche** : supprimer `apps/desktop/src-tauri/src/pack.rs`, orphelin depuis le
commit `20e4287` (retrait du sous-système de traduction, ADR 0013).

**Constat** : le fichier n'était pas tracké par git — il avait déjà été supprimé
par `20e4287` (confirmé par `git log --all -- pack.rs` : `D apps/desktop/src-tauri/src/pack.rs`).
Il subsistait comme fichier non suivi dans le répertoire de travail, probablement
laissé par un outil. Aucune déclaration `mod pack` dans `lib.rs` ni dans aucun
autre `.rs` (grep confirmé).

**Actions** :
1. `grep pack apps/desktop/src-tauri/src/**/*.rs` → seul `pack.rs` lui-même
   et deux commentaires dans `main.rs`/`supervisor.rs`. Aucune référence active.
2. `Remove-Item apps/desktop/src-tauri/src/pack.rs` → suppression physique.
3. Pas de commit nécessaire : git avait déjà supprimé ce fichier en `20e4287`.

**Portes** (depuis `apps/desktop/src-tauri`) :
- `cargo fmt --check` : EXIT 0
- `cargo clippy --all-targets -- -D warnings` : EXIT 0, `Finished dev profile`
- `cargo test -j 2` : EXIT 0, tous les tests passent (doc-tests inclus)

### Challenge Cloudflare — abandon daté du 2026-10-11, avec ce qui reste acquis

**Décision humaine : on laisse tomber HENTAIZONE.** Ce qui suit dit ce qui
est réglé, ce qui ne l'est pas, et pourquoi — pour ne pas rouvrir le
dossier dans six mois en repartant de zéro.

#### Ce qui fonctionne, et se garde

1. **Le flux de vérification** : la fenêtre s'ouvre, le challenge Turnstile
   aboutit, l'utilisateur est redirigé, la fenêtre se ferme, les cookies
   sont transférés avec `secure`/`httpOnly` et l'expiration réelle.
2. **L'usurpation d'UA est supprimée** (`f796faa`). Elle imposait
   `Firefox/116 / Linux` à une fenêtre WebView2 dont `platform` valait Win32
   et dont les client hints désignaient Edge WebView2. A/B mesuré : avec,
   le widget boucle sans réponse ; sans, le challenge aboutit. C'était un
   vrai défaut, qui cassait **toute** source à challenge.
3. **Le rejeu emprunte l'identité de la fenêtre** qui a résolu, au lieu de
   l'UA `FIREFOX_DESKTOP` d'OkHttp.
4. **Les marqueurs d'un challenge en cours ne sont plus renvoyés**
   (`cf_chl_*`) : l'effet est mesuré (8 → 7 cookies), la cause qu'on leur
   imputait est réfutée.
5. **Les erreurs sont devenues honnêtes** : toast en bas, bandeau réservé
   au challenge, plus de « la source bloque l'application » sur un cookie
   transitoire (`4b8886e`, `d8c8ae1`, `039796d`).

#### Option C — ampleur mesurée le 2026-10-11 (recensement de tout le catalogue)

Une requête réelle par source, **à travers le moteur** (`filterOptions`),
donc la mesure porte sur le client de production et non sur un script de
sonde. 1 374 sources, 93 s, 12 en parallèle.

| Code | Nombre | Part |
|---|---:|---:|
| `OK` | 551 | 40,1 % |
| `CHALLENGE_REQUIRED` | **99** | **7,2 %** |
| `TIMEOUT` | 120 | 8,7 % |
| `INTERNAL` | 599 | 43,6 % |
| `PARSE_ERROR` | 4 | 0,3 % |
| `AUTH_REQUIRED` | 1 | 0,1 % |

**Ce n'est donc pas « une source sur 1 370 » : c'est 99, soit une sur
quatorze.** L'argument qui écartait l'option B — « motivée par une seule
source » — est **faux**, et il faut le retirer.

Deux lectures, à ne pas confondre :

- Les **99 `CHALLENGE_REQUIRED`** sont notre problème : le rejeu échoue, on
  sait pourquoi (empreinte de pile réseau), et on peut le dire à
  l'utilisateur.
- Les **719 `TIMEOUT` + `INTERNAL`** sont un autre problème, probablement
  des sources mortes, bloquées ou géo-restreintes. Rien à voir avec
  Cloudflare, hors périmètre ici — mais le nombre est plus grand que celui
  des challenges, et il meritera son propre recensement.

#### Empreinte réseau : OkHttp contre Chromium, mesuré le 2026-10-11

Comparaison faite **sans passer par l'application** : OkHttp est sondé avec le
JAR de production, et le côté navigateur est un Edge piloté à part — donc
cette comparaison ne porte aucun des confounds des mesures précédentes.

**L'IP n'est pas en cause** : `154.72.113.241` des deux côtés.

| | OkHttp (moteur) | Edge (Chromium) |
|---|---|---|
| JA3 | `…4865-4866-4867-49195-…-49172`, extensions `0-5-10-11-16-17-23-35-13-43-45-50-51-65281` | mêmes groupes mais **GREASE** `156-157-47-53`, et un tout autre ordre d'extensions |
| JA3 hash | `0a80f68631b6e8b4634dbc261e8ba60b` | `1b509e4df36ed8a2d6544836eb73881c` |
| JA4 | `t13d1114h2_5e2a75874763_89e9e4bb3419` (**14** extensions) | `t13d1517h2_8daaf6152771_a87ad97598a9` (**17** extensions) |
| HTTP/2 (Akamai) | `4:16777216\|16711681\|0\|m,p,a,s` | `1:65536;2:0;4:6291456;6:262144\|15663105\|0\|m,a,s,p` |

Les trois signatures diffèrent, et l'écart n'est pas cosmétique : ordre des
extensions, présence et placement des valeurs GREASE, table d'en-têtes HTTP/2
de 16 Mo contre 64 Ko, et ordre des SETTINGS différent. Ce sont exactement
les signaux qu'un contrôle anti-bot classe comme « pas un navigateur ».

**Conscrypt ne suffirait pas.** Il ne change que la moitié TLS (JA3/JA4).
L'autre moitié — `akamai_fingerprint` — vient de l'implémentation HTTP/2
d'OkHttp, pas de la couche TLS. Changer de fournisseur TLS laisserait la
moitié du signal inchangée.

Corollaire : les client hints (`Sec-CH-UA*`) n'ont aucune chance d'aider,
puisqu'ils ne touchent à aucun des trois tableaux ci-dessus. Ils n'ont jamais
été mesurés contre `/manga/` — mais ils ne pouvaient pas changer ce verdict.

#### La détection automatique a enfin fonctionné (2026-10-11)

Mesure dans l'app réelle, avec le port de debug actif. Après résolution du
challenge par l'utilisateur :

```
challenge: 7 cookie(s) applique(s) automatiquement source=HENTAIZONE
```

Aucune intervention : la surveillance a vu la clearance, l'a confirmée,
l'a transmise et a fermé la fenêtre, **sans clic**. C'est le comportement
voulu depuis la création du flux, et il n'avait encore jamais été observé —
jusqu'ici la surveillance se déclenchait sur un cookie transitoire (corrigé
par `4b8886e`) ou ne se déclenchait pas.

Conséquence à noter : la fenêtre se ferme seule environ 3 s après la
résolution. Toute mesure qui demande de l'interroger **doit être posée
avant**, ou attendre une variante sans auto-application.

#### Piste trouvée dans le code de Kotatsu (2026-10-11) — la pile TLS

Le test Kotatsu a renvoyé une observation décisive : **chez Kotatsu la vérification
n'arrive qu'à la lecture/téléchargement, le catalogue s'affiche**. Et Kotatsu
utilise OkHttp comme nous. La différence est donc ailleurs.

Lecture du code amont (`KotatsuApp/Kotatsu`, GPL-3.0, même famille de
parseurs) :

- `core/network/NetworkModule.kt` ne configure **aucun** `SSLSocketFactory` ;
  OkHttp utilise donc le **provider TLS par défaut d'Android**, qui est
  **Conscrypt** — la bibliothèque de Google, celle de Chrome.
- Notre moteur, lui, tourne sur un **JVM de bureau** : le provider par défaut
  est **JSSE**, la pile du JDK.
- `core/network/cookies/AndroidCookieJar.kt` lit `CookieManager.getInstance()`,
  le magasin **système**, partagé nativement entre la WebView et OkHttp : pas de
  transfert manuel comme le nôtre.
- `core/network/CommonHeadersInterceptor.kt` ajoute un **`Referer`**. Mesuré :
  sans effet ici (les 4 variantes, dont Referer, sont refusées).

Conséquence importante : le HTTP/2 vient du **code d'OkHttp dans les deux
cas**, donc il est identique des deux côtés. Kotatsu réussissant, l'empreinte
HTTP/2 **n'est pas** le facteur bloquant — ce qui **infirme** ce que j'avais
affirmé plus haut (« Conscrypt ne pourrait corriger que la moitié »).

Mesure de l'effet de Conscrypt (`conscrypt-openjdk-uber` 2.5.2, même OkHttp) :

| | JA3 | GREASE |
|---|---|---|
| JSSE (notre moteur) | `0a80f686…` | absent |
| Conscrypt | `4944639f…` | `156-157-47-53` |
| Edge (référence) | `1b509e4d…` | `156-157-47-53` |

Conscrypt rapproche fortement l'empreinte de celle de Chrome (mêmes groupes de
chiffrements, mêmes courbes, GREASE présent) sans l'égaler : l'ordre des
extensions diffère encore.

**Ce qui n'est PAS mesuré** : une requête **avec** la clearance, sous
Conscrypt. La sonde `/manga/` sans cookie renvoie 403 dans les deux cas, ce
qui est attendu et ne prouve rien. Le seul test valide est le flux complet
avec un vrai puzzle résolu — donc un spike.

#### Le portage est faisable — et la mesure était inutile (2026-10-11, fin de séance)

Le spike du dernier tour (sonde `fetch` dans la fenêtre) n'a jamais abouti :
la fenêtre se ferme **3,5 s** après s'ouvrir, et elle n'est de toute façon pas
exposée sur le port de debug (seule la fenêtre principale y apparaît). Deux
tentatives, deux échecs, pour une question **déjà tranchée par l'écran**.

Ce qu'on sait, sans mesure supplémentaire :
- la fenêtre **affiche** la page du catalogue avec sa clearance (constaté à
  l'écran, le 2026-10-10 : « j'ai pu voir les boutons sur le site ») ;
- Rust sait lire dans la fenêtre (`navigator.userAgent` est revenu
  plusieurs fois) — le transport existe donc.

Ce qui manque n'est plus une incertitude, c'est du travail : le protocole est
**à sens unique**, le moteur ne peut pas demander une requête à Rust. Le portage
ajoute donc : une direction inverse du protocole, un intercepteur sous
`httpClient` qui délègue à la fenêtre, et un drapeau par source.

**Leçon de la séance, la quatrième du même nom.** Avant de construire un
appareil de mesure, demander ce que l'écran sait déjà. Trois fois ce soir la
réponse était visible : le catalogue s'affiche dans la fenêtre, l'utilisateur
ne clique rien, `cf_chl_rc_ni` est encore là. Une sonde costs un build, un
lancement, un aller-retour utilisateur — et n'apporte rien quand l'observation
existe déjà. *Chercher d'abord dans le code de référence, puis dans l'écran,
puis seulement mesurer.*

#### État de fin de séance (2026-10-11, commit `269c2a1`)

Deux bugs **de notre code**, trouvés en comparant la trace à la référence :

1. **Le parseur était invisible depuis la chaîne HTTP.** Les parseurs
   appellent OkHttp en asynchrone (`enqueue`) ; l'intercepteur s'exécute sur
   un fil du dispatcher où le `ThreadLocal` de `ParserPool.avec` n'existe pas.
   Trace : `parseur=null intercepteurLu=false enTetesDeclares=-1`.
   Conséquence : `MangaParser.getRequestHeaders()` n'était **jamais** appliqué.
   Corrigé en lisant le `tag(MangaSource)` que `OkHttpWebClient` pose déjà
   (vérifié dans le JAR, pas supposé), comme le fait Kotatsu.

2. **L'UA imposé par le challenge était annulé par le parseur** — corollaire du
   1, et visible en une ligne : on demandait `Edg/154`, c'est `Firefox/116`
   qui partait. Ordre reproduit de Kotatsu-Redo (« overrides the parser's agent,
   deliberately »).

Après correctif, la requête de Tatami est **équivalente en tout point
observable** à celle de Kotatsu-Redo pour HENTAIZONE : même UA déclarée, mêmes
en-têtes, clearance complète, même IP. Et le refus tient :
`403 cf-mitigated: challenge`.

**Huit causes éliminées par la mesure** : IP, TLS/Conscrypt, UA, cookies
(domaine/chemin/expiration/drapeaux), `Referer`, client hints, marqueurs
`cf_chl_*`, en-têtes du parseur.

**La dernière expérience connue, à faire dans un état valide** : Conscrypt. Le
premier essai (`8fc9be2`) précédait les correctifs d'en-têtes — il testait donc
une requête fausse de toute façon. Le refaire demande : dépendance Gradle,
`sslSocketFactory` sur le client du moteur, porte de vérification, puis le
parcours HENTAIZONE. Une heure. **FAIT le 2026-10-11 (`381f4d5`) : le refus
tient** — voir la section suivante, qui clôt le dossier des causes.

**Contre-mesure utile** : `mangas-origines.fr` et `lelscanfr.com`, sur le même
Cloudflare, répondent **200** dans la même trace. Le blocage est propre à
`hentaizone.xyz`, pas à Cloudflare ni à notre client en général.

#### Conscrypt refait dans un état valide, et le refus tient (2026-10-11, `381f4d5`)

**Mini-plan (écrit avant de coder).**
1. Lire le code de référence avant de mesurer : `Kotatsu-Redo/.../BaseApp.kt:88`.
2. Reprendre la sonde de la veille (`ConscryptProbe.java`) plutôt qu'inventer une
   API : elle contient le contournement `authType "GENERIC"`.
3. Vérifier que le marqueur est **dans le JAR livré**, pas seulement dans la source.
4. Prouver la porte en cassant le câblage, et lire le nom des tests rouges.
5. Rejouer le parcours complet, et ne conclure que sur une requête qui **porte**
   `cf_clearance`.

**Ce que la référence dit vraiment.** `Security.insertProviderAt(Conscrypt
.newProvider(), 1)` est sous `if (SDK_INT < Q)` : ce n'est pas un choix, c'est un
**rattrapage** pour les Android trop vieux. Sur un Android récent, Conscrypt est
déjà le provider TLS de la plateforme. Notre JVM de bureau, elle, part sur JSSE.

`Security.insertProviderAt` ne suffirait pas ici et ne serait pas vérifiable :
JSSE choisit son propre provider, insérer le nôtre en position 1 ne change pas
l'`SSLContext` déjà construit. On construit donc l'`SSLContext` **avec** Conscrypt
et on le pose sur le client (`PileTls.kt`), avec repli JSSE.

**La mesure du 2026-10-11 était juste, et l.Pointer est confirmé sur le client de
production** (`EngineContext.httpClient`, pas une sonde à côté) :

| | Avant (JSSE) | Après (Conscrypt) |
|---|---|---|
| JA3 | `0a80f686…` | **`4944639f9df167507df68fbe89589dbe`** |
| GREASE | absent | **`156-157-47-53`** — comme Edge |

`self-signed.badssl.com` échoue toujours en `CertPathValidatorException: Trust
anchor … not found` : **la vérification de chaîne n'a pas été désactivée** pour
acheter Conscrypt (AGENTS.md §10).

**Le parcours complet, et son verdict.** Challenge résolu par l'humain, 7 cookies
lus, `cf_clearance` 533 o `secure` `httpOnly`, application automatique, puis le
rejeu :

```
[challenge-debug] cookie injecte … nom=cf_clearance … secure=true httpOnly=true longueurValeur=533
challenge: 7 cookie(s) applique(s) automatiquement source=HENTAIZONE
[challenge-debug] requete hote=hentaizone.xyz
    ua="…Chrome/154…Edg/154…"            <- celle qui a résolu
    cookiesJar=[…, cf_clearance, …]       <- la clearance
    statut=403  cf-mitigated=challenge    <- refusée
```

Sidecar vérifié **sans redémarrage** entre `[tls] pile en service` (13:22:20) et
le rejeu (13:30:21) : la pile était bien en service pendant toute la mesure.

**Conclusion, et c'est la neuvième cause éliminée par la mesure** : changer la
pile TLS pour celle de Chrome ne suffit pas. Le refus n'est pas un problème de
signature TLS. Il reste l'écart déjà mesuré : **l'ordre des extensions TLS** et
l'empreinte HTTP/2 d'OkHttp, qu'on ne peut pas rendre pareille depuis un
`OkHttpClient`. Ce n'est plus une hypothèse — c'est le résultat de la dernière
expérience connue.

**Ce que Conscrypt a coûté, et ce qu'il faut en penser.** Un JAR de plus dans la
distribution (4,5 Mo), une dépendance `org.conscrypt` à vérifier à chaque
graduation, et une ligne de configuration `verification-metadata.xml`. **Rien de
cela n'a racheté une source.** Décision à prendre par l'humain (§2 règle 9) :
garder la pile (elle est plus proche d'un navigateur et ne casse rien — `mangas-
origines.fr` et `lelscanfr.com` répondent 200 sous Conscrypt) ou revenir à JSSE.

**Décision humaine du 2026-10-11 : on garde Conscrypt**, et c'est consigné en
ADR `docs/decisions/0016-pile-tls-conscrypt.md` (coût : 4,5 Mo et une
dépendance à vérifier à chaque graduation, acceptés explicitement).

#### Le défaut d'interface est corrigé (`b913403`) — le fond reste ouvert

Le cycle sans fin « bandeau → puzzle → fenêtre qui se ferme → rien » est
**arrêté** : le moteur distingue désormais deux états que tout confondait
depuis le début :

- `CHALLENGE_REQUIRED` — la requête ne portait **pas** de clearance : demande de
  vérification ordinaire, la fenêtre doit s'ouvrir ;
- `CHALLENGE_UNREPLAYABLE` — la requête portait `cf_clearance`, avec l'UA de la
  fenêtre qui a résolu, et le site a refusé quand même (`403 cf-mitigated`).
  Plus de bandeau, plus de fenêtre, message sans promesse de réessai.

Clé du classement : `CloudFlareInterceptor.intercept` lit la présence du cookie
dans le **magasin**, pas dans l'en-tête `Cookie` — `BridgeInterceptor` d'OkHttp
l'ajoute après tous les intercepteurs d'application, donc il est encore vide.

Ce que ça règle : l'expérience d'une source non rejouable cesse d'être un piège
sans fin. Ce que ça ne règle **pas** : les 99 sources en `CHALLENGE_REQUIRED`
(option C) restent inaccessibles, et une source bloquée l'est toujours.

#### Mesure réelle HENTAIZONE et portée de la mémoire (2026-10-12)

Mesure en lançant Tatami après reconstruction de `engine/dist` :
`$env:TATAMI_CHALLENGE_DEBUG='1'; pnpm tauri dev` depuis `apps/desktop`. Trace du
12/10 à 08:27 UTC :

1. Première requête vers `hentaizone.xyz` : `statut=403`,
   `cf-mitigated=challenge`, `cookiesJar=[]` ; Rust reçoit
   `CHALLENGE_REQUIRED` et ouvre une seule fenêtre (`appel n°1`).
2. La WebView fournit 7 cookies déjà présents dans son profil, dont
   `cf_clearance`; le rejeu emploie son User-Agent.
3. Rejeu vers le même hôte : `statut=403`, `cf-mitigated=challenge`,
   `cookiesJar` contient `cf_clearance`; Rust reçoit `CHALLENGE_UNREPLAYABLE`.
   La source reste refusée par le serveur, mais le client distingue ce refus du
   challenge initial et ne repart pas dans le même cycle.

La trace prouve le transfert des cookies et le refus du rejeu réel. Elle ne
prouve **pas** qu'un nouveau puzzle a été résolu pendant cette mesure : les
cookies étaient déjà dans le profil WebView2. Le verdict mesuré est donc que
HENTAIZONE refuse le rejeu malgré la clearance fournie, non que le puzzle a
rendu cette source accessible.

La résolution est considérée récente pendant **5 minutes** après `setCookies`.
Un refus de rejeu est mémorisé **30 minutes** pour l'hôte et reconnu sur ses
sous-domaines ; une réponse normale efface refus et fraîcheur des domaines
apparentés. Conséquence voulue : si une page publique répond 200 mais qu'une
page protégée reste refusée, ce 200 invalide la fraîcheur ; le prochain 403
peut rouvrir une seule vérification au lieu d'être classé comme un rejeu récent.

#### Options pour les 99 sources derrière un challenge non rejouable

Écrit le 2026-10-11 sur demande explicite. **Rien de ceci n'est engagé** : aucune
ligne de code, aucune ADR acceptée, aucune permission ajoutée. Le chiffre est la
mesure de l'option C — 99 sources sur 1 374, soit une sur quatorze.

| Option | Ce qu'elle règle | Ce qu'elle casse | Effort |
|---|---|---|---|
| **A. Message définitif seul** (`b913403`, **déjà en place**) | L'utilisateur n'est plus enfermé dans un cycle, et il entend la vérité | Rien — mais 99 sources restent inaccessibles | Livré |
| **B. Transport par la webview cachée** | Les 99 sources redeviennent accessibles, avec une vraie pile réseau | Le cœur Rust perd la maîtrise des requêtes : plus de rate-limit local fiable, plus de contrôle d'hôte, plus de cache d'images tel qu'il existe. C'est un **changement d'architecture** (§4 : la webview ne parle qu'à Rust) | Plusieurs jours |
| **C. Client TLS « chrome-like » côté JVM** | La signature TLS/JA3 deviendrait indiscernable | Guerre du gendarme et du voleur ; chaque mise à jour d'OkHttp ou de la cible casse la parité ; GREASE et ordre des extensions d'HTTP/2 ne sont pas qu'un réglage | Semaines, fragile par construction |

**Option B, dans le détail, parce que c'est la plus tentante.**
L'intercepteur OkHttp déléguerait à Rust, qui ferait un `fetch` dans une webview
cachée. Trois obstacles mesurés ou lus, pas devinés : `fetch` **interdit**
`cf_clearance` et d'autres en-têtes (liste de forbidden headers), le CORS
multi-origines impose un aller-retour par hôte, et il faut une **commande unique
dédiée** avec liste blanche d'hôtes validée côté Rust — jamais une capability
`remote` large. Même avec tout ça, ce qui reste est une égalité de pile réseau,
pas une reproduction : la fenêtre aboutit et le rejeu échoue, ce qui est
exactement le refus mesuré aujourd'hui.

#### Les mesures qui départageraient — aucune n'est faite

Dans l'ordre du coût, croissant. La première est gratuite :

1. **Le test Kotatsu sur téléphone**, même Wi-Fi, cookies de la source effacés.
   *Ce qu'il décide :* si Kotatsu **échoue aussi**, le site refuse tous les
   clients non-navigateur et l'option B coûte des jours pour un client que le
   site refuserait quand même. Si Kotatsu **réussit**, mon diagnostic
   d'empreinte réseau est à réviser — et l'option B devient crédible. Tant qu'il
   manque, la phrase honnête reste *le refus n'est pas expliqué*.
2. **`fetch` depuis la console de la webview**, sur le vrai site, **après**
   résolution du challenge. Mesure ce que l'option B obtiendrait réellement, et
   si les en-têtes interdits la rendent inutile. Aucune ligne de code moteur.
3. **`curl_cffi`** (impersonation Chrome) avec la même clearance, sur cette
   machine. Estime le plafond de l'option C, et surtout si elle est même
   *possible* — `curl_cffi` reproduit l'ordre des extensions TLS que ni
   Conscrypt ni JSSE n'ont.

Aucune de ces trois mesures n'est engagée non plus. **Je ne touche pas au
transport des requêtes sans ton accord** (§2 règle 9).

#### Objectif 3 — les en-têtes du parseur étaient déjà dans la trace

Tu demandais d'ajouter les noms des en-têtes déclarés par le parseur dans la
ligne « intercepteur parseur ». **C'est déjà le cas**, livré par `269c2a1`, et
c'est dans le binaire distribué.

```
[challenge-debug] intercepteur parseur=MangaParserWrapper tag=HENTAIZONE
  enTetesDeclares=1 [User-Agent] uaDeclare=Mozilla/5.0 (X11; Linux x86_64; …)
  uaEnvoye=null
```

`enTetesDeclares=1 [User-Agent]` — le **nom** seul, jamais la valeur, comme
l'exige la règle « aucun secret dans le journal ». Cette ligne a été lue dans
une vraie trace HENTAIZONE, donc elle s'exécute : elle ne se contente pas de
compiler.

**Les deux erreurs de mesure faites en voulant le vérifier**, à relire avant
d'ouvrir un JAR :

1. **JAR périmé.** J'ai ouvert `dist/tatami-engine/lib/tatami-engine.jar`
   horodaté 14h12 alors que les classes fraîches dataient de 15h05 — d'où la
   conclusion « absent du JAR livré », fausse.
2. **Mauvaise classe.** Le marqueur est dans
   `EngineContext$special$$inlined$-addInterceptor$1.class` : le code vit dans
   une **lambda** d'intercepteur, pas dans la classe englobante. Chercher dans
   `EngineContext.class` ne trouve rien.

Après `scripts/build-engine.ps1` — seul à écrire `dist/`, leçon du 2026-10-11 —
le marqueur était bien présent dans le JAR livré. Rien n'a été ajouté pour cet
objectif : il fallait vérifier, pas écrire.

Ce que la ligne apporte au diagnostic, au-delà de la demande : `parseur=` dit si
le parseur est **vu** par la chaîne HTTP (le défaut corrigé par `269c2a1`),
`tag=` confirme la source, et `enTetesDeclares=[…]` confirme que le parseur
déclare bien des en-têtes — donc que `getRequestHeaders()` est appelé et non
supposé.

#### Ce qui n'est PAS mesuré — à ne pas citer comme acquis

Les neuf causes éliminées le sont **sur une fenêtre de mesure**. Chaque phrase
ci-dessous est une limite que la mesure n'a pas franchie :

- **TLS.** Conscrypt rapproche le JA3 de Chrome, **il ne l'égale pas** : l'ordre
  des extensions diffère encore. La partie « JA3/JA4 » est donc corrigée en
  partie, pas totalement. Aucune mesure n'a porté sur **GREASE avec le bon ordre
  d'extensions**, parce qu'OkHttp ne l'expose pas.
- **HTTP/2.** Aucune mesure directe ici. L'affirmation « l'empreinte HTTP/2
  d'OkHttp est identique des deux côtés » vient du code de référence (OkHttp des
  deux côtés), pas d'une sonde sur la machine de l'utilisateur.
- **L'API du site.** Mesuré le 2026-10-09 sur MangaDex (`api.mangadex.org` refuse
  le TLS depuis cette machine, `SEC_E_ILLEGAL_MESSAGE`), pas sur HENTAIZONE.
  Aucune mesure ne dit si `hentaizone.xyz` a **aussi** une API morte.
- **La réelle portée du blocage.** Le recensement de l'option C a mesuré
  l'**accueil** de chaque source (`filterOptions`). Une source dont l'accueil
  passe mais dont les chapitres ou les images sont derrière un CDN à challenge
  (cas REALA: `img.reimg.org`) n'y apparaît **pas**. 99 est donc un plancher.
- **Le test comparatif navigateur.** Aucune mesure `fetch` depuis la webview sur
  `hentaizone.xyz` après résolution. On ne peut donc pas affirmer que *même un
  vrai navigateur* obtiendrait ce que la fenêtre affiche déjà.

#### Ce qui ne fonctionne pas, et pourquoi c'est structurel

Le **rejeu** est refusé. Mesuré dans l'application réelle, instrumenté
côté moteur :

```
ua         = "…Chrome/154…Edg/154…"                 ✅ celle qui a résolu
cookiesJar = [__suvt, _ga, …, cf_clearance, …]      ✅ 533 o, secure, httpOnly, exp. 2027
statut     = 403   cf-mitigated: challenge          ❌
```

Une sonde sur `/cdn-cgi/trace` avec le **même OkHttp que la production**
(4 configurations : sans UA, UA Firefox, UA Chromium, UA Chromium + client
hints) répond **200** partout, en `tls=TLSv1.3`, `http=http/2`, `kex=X25519`.
OkHttp n'est donc **pas** refusé en tant que client.

Ce qui sépare la requête refusée d'un vrai navigateur n'est plus un
réglage : c'est l'empreinte de la pile réseau (JA3/JA4 TLS, fingerprint
HTTP/2), que l'on ne peut pas rendrepareille depuis Rust. Les client hints
ajoutés n'ont rien changé.

#### Dette / suite possible, non engagée

- **Option A** — dire en une phrase, dans l'interface, qu'une source exige
  une vérification que l'application ne peut pas reproduire, au lieu
  d'ouvrir une fenêtre qui mène nulle part. ~1/2 journée.
- **Option B** — faire passer les requêtes par la fenêtre (vraie pile
  réseau). Plusieurs jours, une webview par requête, sur une machine de
  12 Go avec un JVM ; le cœur Rust perd la maîtrise des requêtes. **Pas
  recommandée** tant que l'ampleur n'est pas mesurée : elle serait motivée
  par **une** source sur 1 370.
- **Option C** — mesurer si d'autres sources sont concernées. C'est le
  geste qui change la taille du problème, et il n'a **pas** été fait.

#### Les deux erreurs de méthode de cette journée

1. **Le binaire avait deux jours de retard.** Le JAR du moteur datait du
   09/10, les sources Kotlin du 10/10. Tout ce qui avait été écrit côté
   Kotlin — `secure`/`httpOnly`, expiration, instrumentation — **n'a jamais
   été exécuté**. Corollaire de la leçon « restaurer un fichier ne prouve
   pas que le binaire a été reconstruit » : *corriger la source ne prouve
   pas que le binaire l'exécute*.
2. **Une instrumentation non branchée a été lue comme un résultat.** Zéro
   ligne `challenge-debug` a été interprétée comme « le moteur ne fait
   rien », alors que le code qui écrit ces lignes n'était pas dans le JAR.
   *Vérifier qu'un diagnostic est branché avant de l'interpréter* — en
   ouvrant le JAR et en cherchant le marqueur dans les classes
   (`EngineContext$…addInterceptor$2`, `ServerKt`).
3. Et un confound auto-infligé : `--remote-debugging-port` est un signal
   d'automatisation. Toute la matinée de mesures était contaminée par mon
   propre dispositif de mesure.
### Flux Cloudflare — le lecteur pouvait demander une vérification, et le rejeu refusé se nommait (2026-10-13)

**Symptôme rapporté** : « la fenêtre s'ouvre, les cookies sont envoyés (enfin
c'est ce que dit le toast), mais les webtoons ne s'affichent pas du tout »,
sur plusieurs sources à vérification.

#### 1. Le lecteur n'avait aucun chemin de vérification

La fenêtre ne s'ouvrait que sur **deux** chemins, mesurés dans le code :

| chemin | fenêtre | après résolution |
|---|---|---|
| catalogue (`loadSource`) | oui (`setChallenge`, un seul appel dans toute l'interface) | recharge le catalogue |
| téléchargement | oui (`download://challenge`) | chapitre retenté |
| **lecteur** (`fetchImage`) | **aucune** | **rien** |

`pagecache::serve_page` transformait le `CHALLENGE_REQUIRED` du moteur en
simple 404, et après `challenge://resolu` l'interface ne rechargeait que le
catalogue. Le toast disait **vrai** — le cookie partait — et le lecteur restait
mort. Corrigé : émission `challenge://page` depuis le cœur, `hoteImage` déduit
par Rust de l'URL qu'il a lui-même demandée (l'interface ne nomme aucun hôte),
trois conditions sur ce second chemin (hôte **public**, URL de vérification sur
**exactement** cet hôte, règles habituelles), et cache-buster `?r=` pour
relancer les images dont le 404 restait en cache.

#### 2. Le 403 du rejeu n'était pas classé

**Mesuré dans `tatami.log`, TOONFR, application réelle** :

    challenge: 1 cookie(s) applique(s) automatiquement source=TOONFR
    stdout erreur id=latest code=INTERNAL
      msg=HttpStatusException: . Status=403, URL=[https://toonfr.com/webtoon/]

Avant le cookie, la même source renvoyait `CHALLENGE_REQUIRED`. Après, le site
répond 403, `CloudFlareHelper.checkResponseForProtection` répond
`PROTECTION_NOT_DETECTED`, l'interception laisse passer et le parseur remonte
une `HttpStatusException` devenue `INTERNAL`.

Coût réel : `CHALLENGE_UNREPLAYABLE` existe dans le contrat, le moteur sait le
produire, l'interface a le message traduit `challenge.non.rejouable` — **rien
de tout cela ne se déclenchait jamais**.

Corrigé sans lire aucun marqueur Cloudflare : la requête portait déjà une
`cf_clearance`, donc la vérification était faite, et le site refuse (403/503).
6 tests, porte prouvée rouge en retirant la condition `clearance != null`
(c'est le test de voisinage des 403 ordinaires qui tombe). 158 tests, 1 échec
préexistant (`zenmanga.eu`).

**Après correction**, l'écran affiche : « La vérification de TOONFR a abouti,
mais le site refuse quand même la requête. » — le message est juste, la source
reste inaccessible. La **cause de fond B/C/D reste ouverte**.

#### 3. Le journal ne voyait pas la cause

`supervisor.rs` ne journalisait que le **code**. Le rejeu laissait
`code=INTERNAL len=152` et **rien** sur la cause : il a fallu ajouter le
message pour apprendre que c'était un 403. C'est le **même trou** que celui du
2026-10-09 (« le journal ne le voyait pas »), refermé par la trace puis rouvert
par une autre décision.

#### 4. Leçon — une porte de log et une porte de test ne se prouvent pas de la même façon

Le journal et `supervisor.rs` se lisaient mutuellement, et les deux étaient
muets sur la même information. Corollaire : **un diagnostic absent du journal
est un diagnostic absent**, quelle que soit la qualité du test qui entoure le
code.

#### 5. Mesure en attente

`TATAMI_CHALLENGE_DEBUG=1` instrumenté `cookiesJar`, `ua`, `statut`,
`cf-mitigated`, `server`, `cf-ray`. **Vérifié présent dans le JAR livré**
(dans `EngineContext$special$$inlined$-addInterceptor$2`, pas dans
`EngineContext` — cf. la leçon du 2026-10-10). **Jamais exécuté sur une vraie
session à ce jour** : c'est la prochaine mesure, pour trancher entre

- `cf_clearance` **n'atteint pas** la requête suivante → correctible ;
- ou elle y est et Cloudflare refuse malgré tout → empreinte TLS d'OkHttp,
  hors de portée d'un correctif de code.

### Écart n°1 — un blocage Cloudflare n'est pas un captcha (2026-10-13)

Lecture de `Kotatsu-Redo/core/network/CloudFlareInterceptor.kt` (merci de la
piste) : la référence **distingue** trois issues là où nous en traitions deux.
Le commentaire d'aide de `CaptchaAutoResolveCoordinator` y répond aussi, sur
notre propre cas : « The solver WebView passing the challenge while the parser's
OkHttp request stays blocked means Cloudflare is not honouring the clearance for
that request — a fingerprint mismatch between the two HTTP stacks, not something
a repeat session can resolve. » Ils ne le résolvent pas, ils l'**empêchent de
boucler** — ce qu'on a fait sous le nom `CHALLENGE_UNREPLAYABLE`.

#### `PROTECTION_BLOCKED` contre `PROTECTION_CAPTCHA`

`CloudFlareHelper` rend trois issues ; `PROTECTION_BLOCKED` finissait en
`CloudFlareProtectionException`, comme `PROTECTION_CAPTCHA`. L'interface ouvrait
donc une fenêtre de vérification pour une source **bloquée** : l'utilisateur
résolvait un défi, le cookie partait, et la source restait inaccessible, avec un
message qui annonçait une vérification là où il n'y en avait aucune.

Corrigé : `CloudFlareBlockedException` → code `SOURCE_BLOCKED`, classé **avant**
tout challenge. `challengeUrl` refuse désormais une URL sur un message
`SOURCE_BLOCKED`, `Supervisor::is_transient` ne le rejoue plus, et l'interface
dit « refusée par sa protection anti-robot, sans aucune vérification à passer ».

#### Une fixture de test que la mesure a corrigée

`RejeuRefuseTest` avait un **mauvais** corps de réponse : il portait à la fois
`blocked_why_headline` et `challenge-error-title`. Le bytecode de
`CloudFlareHelper` montre que le premier classifie en `PROTECTION_BLOCKED` et le
second en `PROTECTION_CAPTCHA` — donc la fixture **était** un blocage, et les 15
tests du fichier passaient parce que les deux issues étaient mélangées. Elles
divergent donc aujourd'hui.

C'est une correction de fixture, pas un ajustement au code : la fixture porte
maintenant le marqueur captcha que son nom décrit, et deux tests couvrent le
blocage. **Porte prouvée rouge** sur `if (protection == PROTECTION_BLOCKED)` —
les deux tests du blocage tombent.

Témoin sur l'artefact livré : le JAR reconstruit contient
`CloudFlareBlockedException` et `Response.peekBody`.

#### Ce que ce n°1 ne corrige pas

TOONFR reste refusé. La cause de fond est le décalage d'empreinte entre la pile
HTTP de la fenêtre et celle d'OkHttp, et le projet de référence la tient pour
hors de portée d'un correctif.

#### Piste V2, mesurée et écartée

`MangaLoaderContext.interceptWebViewRequests(...)` (la requête passe par un vrai
WebView, donc une seule pile) est implémentée par défaut par un
`UnsupportedOperationException` et ne l'est pas chez nous — mais **seuls 2
parseurs** l'utilisent (`AllManga`, `Comix`, anglais). Ce n'est pas la cause de
TOONFR. L'hypothèse était séduisante ; elle est fausse.

### Plan de travail — identité WebView et rejeu TOONFR (2026-10-13)

1. Étendre `setCookies` de façon additive : transférer et indexer par hôte l'identité réellement lue dans la WebView, avec tests de contrat.
2. Sur les seules requêtes portant `cf_clearance`, construire et vérifier l'ordre demandé des en-têtes, tout en prouvant le décodage gzip et les chemins existants.
3. Mesurer TOONFR avec une clearance fraîche et lancer une sonde curl impersonnée sans inscrire ni journaliser la valeur du cookie.
4. Corriger et éprouver les scripts personnalisés fautifs/doublons et faire hériter les exceptions réseau d'`IOException`, avec commits séparés.

### Journal 2026-10-13 — profil WebView transmis avec `setCookies`

- Capturés dans la fenêtre de challenge `navigator.userAgent`, les Client Hints bas et haute entropie et les deux valeurs de langue ; transmission additive dans `setCookies`, stockée en mémoire par domaine de clearance.
- Porte Kotlin : `cd engine; gradle test --tests org.tatami.engine.BrowserIdentityTest --rerun-tasks` — **2 tests exécutés, 2 réussis**.
- Portes Rust : `cd apps/desktop/src-tauri; cargo fmt` puis `cargo fmt --check` — propre ; `cargo clippy --all-targets -- -D warnings` — propre ; `cargo test --lib -j 2` — **195 réussis**.
- `cargo test -j 2` complet reste à relancer : l'exécutable `tatami-desktop.exe` est verrouillé par le processus PID 14084 ; je ne l'ai pas arrêté.

### Journal 2026-10-13 — en-têtes de rejeu dérivés de la WebView

- Remplacement des en-têtes navigateur uniquement si `cf_clearance` et profil sont présents ; `sec-ch-ua*`/UA/langues dérivent du profil, `Accept-Encoding` reste à `gzip`, et le log masque les cookies et valeurs non navigateur.
- `cd engine; gradle test --tests org.tatami.engine.BrowserIdentityTest --tests org.tatami.engine.ChallengeHeadersTest --tests org.tatami.engine.RejeuRefuseTest --tests org.tatami.engine.CookieRejoueTest --rerun-tasks` — **31 tests réussis**, dont contrôle sans clearance et réponse gzip réellement décodée.
- `cd engine; gradle test --rerun-tasks` — **170 tests, 1 échec** sur le test réseau `ZenMangaInterceptionTest` : la liste est présente mais la source ne renvoie aucun chapitre. Rejoué isolément, même échec à la même assertion ; les deux autres tests du fichier passent. Aucun clearance/profil n'est configuré pour ZENMANGA, donc le nouveau chemin ne s'exécute pas.
- `scripts/build-engine.ps1` — reconstruction réussie ; le SHA-256 du JAR dans `engine/dist/tatami-engine` concorde avec celui de `build/install`.
- App reconstruite, appel réel `latest` après ouverture de la fenêtre TOONFR : le sidecar a injecté `cf_clearance` et le profil WebView, puis a envoyé la séquence `sec-ch-ua`, `sec-ch-ua-mobile`, `sec-ch-ua-platform`, `Upgrade-Insecure-Requests`, `User-Agent`, `Accept`, `Sec-Fetch-Site`, `Sec-Fetch-Mode`, `Sec-Fetch-User`, `Sec-Fetch-Dest`, `Accept-Encoding=gzip`, `Accept-Language` (valeur du cookie masquée). Réponse : **403**, `cf-mitigated=challenge`, `server=cloudflare`, `cf-ray=a49fd2bc5fc61560-JNB`; l'appel est remonté `CHALLENGE_REQUIRED`. Le journal contient les en-têtes d'identité demandés, mais pas la valeur de la clearance.
- Sonde jetable `curl_cffi` avec `impersonate="chrome"` et la même clearance, le même UA et les en-têtes capturés : **403**, `cf_mitigated=challenge`, `cf_error=null`, page « Just a moment... ». Le test curl ne passe donc pas non plus : l'empreinte TLS/HTTP2 JVM n'est **pas confirmée comme cause unique** et la conception de repli demandée conditionnellement n'est pas déclenchée.

### Journal 2026-10-13 — exceptions Cloudflare de type `IOException`

- `CloudFlareProtectionException` et `CloudFlareUnreplayableException` héritent maintenant de `IOException`, afin que les échecs réseau soient traités comme tels par OkHttp plutôt que comme exceptions d'état non gérées.
- `cd engine; gradle test --tests org.tatami.engine.CloudFlareExceptionTest --tests org.tatami.engine.ErreursTest --rerun-tasks` — **9 tests réussis**, dont l'héritage des deux exceptions et la reconnaissance des causes directes/emballées.

### Journal 2026-10-13 — réparation et déduplication des sources personnalisées

- Dans `%APPDATA%\Tatami\sources`, le fichier `mangadexFr.js` contenait un générateur Python, pas un parseur JS. Le générateur a été préservé sous `mangadexFr.js.generator.py.disabled` et son résultat français a été installé sous le nom `.js`, avec un identifiant cohérent avec le nom de fichier.
- `cd engine; gradle test --tests org.tatami.engine.MangaDexFrUserSourceProbeTest --rerun-tasks` — **1 test réussi** : le parseur réel Nashorn évalue le script et vérifie son identifiant, son nom et sa langue. `node --check <chemin de mangadexFr.js>` — syntaxe acceptée.
- `source-hentai-scantrad.js` et `source-ortega-scans.js` ont été renommés en `.disabled` (sauvegardes conservées) ; le loader ne découvre que `*.js`. `hentai-scantrad.js` conservé comme version canonique, qui contient les entités typographiques manquantes ; `ortega-scans.js` conservé, son doublon vérifié identique par SHA-256.
- Ces scripts résident dans AppData, hors du dépôt et ne sont donc pas inclus dans le commit de ce journal.

### Mesure décisive — le WebView2 obtient 200, OkHttp obtient 403 (2026-10-13)

C'est la mesure listée comme « en attente » depuis le 2026-10-13, et elle a été
faite : **la fenêtre de vérification accède réellement la source**, donc
l'option B n'est plus une hypothèse.

Sondes CDP sur le **WebView2 de l'application réelle** (`--remote-debugging-port`
sur 9223, `Edg/155.0.4283.45`) : même profil, même UA que la fenêtre, aucune
valeur de cookie lue ni journalisée.

| sonde | client | résultat mesuré |
|---|---|---|
| navigation vers `toonfr.com/webtoon/` | WebView2 | **403** `cf-mitigated: challenge`, `server=cloudflare`, titre « Un instant… », `navigator.webdriver=false` |
| ~10 s plus tard, même onglet | WebView2 | **200**, `cf-mitigated=null`, titre « Manga – Toonfr » |
| `fetch('/webtoon/', {credentials:'include'})` | WebView2, sans naviguer | **200**, **178 573 o**, titre « Manga – Toonfr », **168 liens** `a[href*="/webtoon/"]` |
| rejeu avec la même clearance | OkHttp / Conscrypt | **403** → `CHALLENGE_UNREPLAYABLE` |
| même clearance, `impersonate=chrome` | `curl_cffi` (2026-10-13) | **403** |

**Ce que la mesure tranche.** La seule variable qui fait passer de 403 à 200 est
**la pile cliente** : même IP, même clearance, même page. Le « refus n'est pas
expliqué » peut donc être remplacé par une cause mesurée — le rejeu est refusé
parce que ce n'est pas le client qui a résolu.

**Ce qu'elle écarte.** `curl_cffi` en profil Chrome échouant aussi, l'empreinte
TLS seule n'explique pas le refus : un profil Chrome « correct » ne suffit pas,
ce qui rend l'option B (transporter par le vrai navigateur) plus solide encore.

**Ce qui reste ouvert, et que le spike doit mesurer** : rien de tout cela ne dit
encore que la webview peut servir les **images**, ni ce qu'il advient à
l'expiration de la clearance.

### Spike d'une journée — option B (décision humaine du 2026-10-13)

L'option B est engagée **sous condition** : un spike d'une journée, puis
go/no-go. Aucun code de production n'est écrit avant le go.

**Rappel de la règle** : la webview ne parle qu'à Rust (§4). Le spike ne change
pas cette règle, il mesure ce qu'elle coûte.

#### Axe A — canal de retour sécurisé

*Question* : comment la webview rend N octets de HTML à Rust sans que la page
distante puisse déclencher quoi que ce soit ?

*Déjà prouvé* : le canal tiré existe et fonctionne sur une page **distante** —
`challenge.rs` appelle `eval_with_callback` sur la fenêtre de vérification
(`identite_de_la_fenetre`, ligne ~947) pour lire l'UA et les cookies.

*À mesurer* :
1. `window.__TAURI_INTERNALS__` est-il exposé à la page distante ? Si oui,
   c'est une surface d'attaque : une source malveillante pourrait invoquer des
   commandes. À relever **avant** toute autre chose.
2. Une webview **cachée** (non visible), naviguée vers un hôte de la liste
   blanche, rend-elle le HTML par ce même canal tiré ?

*GO si* : canal tiré uniquement, liste blanche d'hôtes validée côté Rust, et la
page distante ne peut invoquer aucune commande.

#### Axe B — images

*Question* : le CDN des pages d'un chapitre est-il un autre hôte ? Et un `fetch`
cross-origin y donne-t-il des octets, ou CORS le refuse-t-il ?

*GO si* : les octets d'une image de chapitre sont récupérables depuis la webview
— cross-origin, ou à défaut par navigation d'un onglet dédié.

#### Axe C — expiration de la clearance

*Question* : combien de temps la `cf_clearance` vaut-elle réellement, et que
se passe-t-il quand elle expire ?

*Mesure* : lire l'`expiresAt` du cookie après résolution — **métadonnées seules,
jamais la valeur**. Puis provoquer l'expiration et observer le comportement.

*GO si* : l'expiration est détectable et ne coûte qu'une réouverture de fenêtre,
sans boucle.

#### Axe D — un chapitre complet

*Question* : bout en bout, une série → sa liste de chapitres → les URLs de pages
→ **toutes** les images, en n'utilisant que le transport webview.

*GO si* : un chapitre complet s'affiche dans le lecteur, sans qu'OkHttp ne soit
sollicité pour cette source.

#### Verdict

Go si **les quatre axes** sont GO. Sinon l'option s'arrête ici et l'on revient à
l'option A (dire la vérité à l'utilisateur sans promettre d'accès).

### Spike — résultats et go/no-go (2026-10-13, ~20 h)

Mesures faites sur le **WebView2 de l'application réelle** (port 9223) et par
lecture du source de `tauri 2.12.1`. Aucune valeur de cookie lue ni journalisée.

#### Axe A — canal de retour : **GO, avec une réserve de conception**

*Mesuré sur la fenêtre Tauri, origine distante `https://toonfr.com`*
(titre « Just a moment... ») :

| surface exposée | mesuré |
|---|---|
| `typeof window.__TAURI_INTERNALS__` | **`object`** |
| `Object.keys(...)` | **`["plugins"]`** |
| `.invoke` | **`function`** |
| `.transformCallback` | **`function`** |

Donc **toute page distante chargée dans une fenêtre Tauri reçoit un
`__TAURI_INTERNALS__` fonctionnel**. Ce n'est pas une faille ouverte pour
autant, et voici pourquoi — la source tranche :

- `tauri/src/manager/webview.rs:168-184` : l'injection est **inconditionnelle**,
  pour toute webview que l'app crée. Aucun test sur l'URL.
- `tauri/src/webview/mod.rs:2080` : `if (plugin_command.is_some()
  || has_app_acl_manifest || !is_local) && … && invoke.acl.is_none()` — l'ACL est
  **forcée dès que l'origine n'est pas locale**. Une page distante ne peut donc
  pas atteindre une commande sans capability explicite.
- `tauri/src/ipc/authority.rs:413/430` : le refus est produit sous la forme
  `"health not allowed. Plugin not found"` — **c'est le message exact que la
  fenêtre de vérification a rendu**, donc le refus a réellement été exercé par
  l'ACL et non par une absence de commande.

**Conséquence de conception, à ne pas rater** : le canal de retour doit être
**tiré** (Rust initie), comme `appliquer_cookies`/`identite_de_la_fenetre` le
font déjà par `eval_with_callback`. Ne **jamais** compter sur la page distante
pour nous pousser quoi que ce soit : cela exigerait une capability `remote`,
donc une commande joignable depuis un contenu non maîtrisé.

**Réserve ouverte** : `eval_with_callback` n'a jamais transporté 178 Ko. Le
canal est prouvé pour ~200 octets (UA, cookies). La taille maximale supportée
par le pont Tauri est **non mesurée** — c'est la première mesure à faire au
jour 1 de l'implémentation.

#### Axe B — images : **GO**

| mesure | valeur |
|---|---|
| images fetchées par le navigateur | **24**, toutes `200 image/jpeg` |
| octets totaux | **3 115 913 o** |
| lecture corps via `Network.getResponseBody` | **189 912 o** réels, `base64` |
| hôte des images | **`toonfr.com`** (même hôte que les pages) |

Le `fetch()` JS échoue en `Failed to fetch` sur les URLs de page, mais **le
navigateur, lui, les charge** (`naturalWidth: 1024`) et le corps est lisible par
CDP. Le transport webview atteint donc bien les octets des images.

#### Axe C — expiration de la clearance : **GO, et l'hypothèse est écartée**

```
cookie nom=cf_clearance domaine=toonfr.com chemin=/ expiration=1823454519
         secure=true httpOnly=true longueurValeur=597
```

`1823454519` → **2027-10-13 19:08:39 UTC**, soit **365 jours** de validité.
L'expiration n'est donc **pas** la cause du refus du rejeu. Il n'y a rien à
mesurer de plus sur cet axe.

#### Axe D — un chapitre complet : **GO sur le transport, non prouvé bout en bout**

| étape | résultat |
|---|---|
| `/webtoon/` (accueil) | **200**, 178 573 o, 168 liens |
| page série | **200**, **48 chapitres** listés |
| page chapitre | **200**, **80 images** (20 chargées, 24 requêtes) |

La chaîne complète est obtenue par le navigateur, images comprises. **Ce qui
n'est pas prouvé** : le chapitre affiché dans le **lecteur** de l'application.
Le test bout en bout exigera l'implémentation du transport.

#### Verdict

**GO sous conditions.** Les trois questions qui décidaient si l'option B valait
plusieurs jours de travail sont tranchées par la mesure :

1. le navigateur obtient 200 là où OkHttp obtient 403 (mesure du jour) ;
2. les **octets des images** sont atteignables (Axe B) ;
3. l'expiration n'est pas en cause (Axe C).

Et la sécurité ne s'oppose pas au transport tiré, pourvu qu'on n'accorde
**aucune** commande à la fenêtre distante.

**Deux réserves avant d'engager le jour 1** :
- `eval_with_callback` sur 178 Ko : à mesurer en premier (risque de troncature) ;
- pas de test en conditions réelles : le chapitre 42 de *Semantic Error* a été
  obtenu par le navigateur, pas affiché par le lecteur.

### Implémentation — option B (début 2026-10-13)

**Mini-plan (écrit avant de coder, §11).**
1. Mesurer le plafond du canal tiré (`eval_with_callback` → 178 Ko) → décide si
   le transport HTML tient en une lecture ou s'il faut découper.
2. Moteur : mémoire URL → HTML + intercepteur qui sert la mémoire **avant** le
   réseau, et signale sinon l'hôte à router. Aucune dépendance à CDP (port de
   debug absent en production, cf. §15).
3. Cœur : webview **cachée**, liste blanche d'hôtes issue de `domaines_source`,
   attente de la fin du challenge par `reste_interstitiel` (déjà testé), puis
   lecture du HTML par le canal tiré et `injecter` dans le moteur.
4. Rejeu de la requête initiale, et lecture d'un chapitre complet dans le
   lecteur — le test qui n'a pas été fait pendant le spike.
5. Portes : `cargo fmt/clippy/test` pour le cœur, `gradle test` pour le moteur,
   et une sonde NDJSON de bout en bout.

Point d'architecture à ne pas manquer : le canal est **tiré** par Rust, jamais
poussé par la page (réserve Axe A). Aucune capability ajoutée.

### Implémentation — étape 1 : mémoire webview côté moteur (2026-10-13)

**Mesure 0 d'abord, comme annoncé : le plafond du canal tiré.**
`outerHTML` de la page chapitre de TOONFR = **124 984 caractères**, retourné
**intact en une lecture de 38 ms**. La réserve du spike (« jamais transporté
178 Ko ») est levée : pas de découpage à écrire.

**Ce qui est livré.**
- `engine/.../WebViewMemoire.kt` : mémoire URL → HTML (TTL 5 min, alignée sur
  `RejeuRefuse.FRAICHEUR_MS`), clé sans fragment mais **avec** requête, et
  `intercepteurWebView` qui sert la mémoire **après** `RateLimitInterceptor`
  et **avant** `CloudFlareInterceptor`. L'ordre est le point fragile : un 403
  capté par `CloudFlareInterceptor` deviendrait exception, et une page déjà
  obtenue par le navigateur serait challengée une seconde fois.
- `EngineContext.kt` : 2 lignes, `addInterceptor(intercepteurWebView)`.
- `Server.kt` : méthode de protocole `injecter` (`url` + `html`). Aucune
  validation d'hôte **volontairement ici** : la liste blanche est faite côté
  Rust, avant l'appel — une seule fois, pas à chaque couche.

**Un bug que ma propre sonde a attrapé, à relire avant de copier ce motif.**
J'avais écrit `return@Interceptor chain.proceed(requete).newBuilder()...build()`
— donc la vraie requête réseau **partait**, puis son résultat était jeté. La
mémoire aurait provoqué exactement le challenge qu'elle sert à éviter. Corrigé
en `Response.Builder()` pur. Leçon de forme : pour fabriquer une réponse, on ne
passe **jamais** par `proceed`.

**Portes.**
- `gradle test --tests WebViewMemoireTest --rerun-tasks` → **8 tests, 0 échec**.
- **Porte prouvée rouge** : en remettant `chain.proceed`, deux tests tombent
  (`une page captive par la webview bypass le challenge sans toucher le reseau`,
  `une URL challenge reconnue en memoire ne passe pas par CloudFlareInterceptor`)
  avec `IllegalStateException: aucune réponse dans ce test` — le réseau a bien
  été sollicité. C'est le nom qui prouve que la porte vise le câblage, pas la
  simple présence du module.
- `gradle test --rerun-tasks` → **179 tests, 1 échec** : `ZenMangaInterceptionTest`,
  **préexistant** (consigné le 2026-10-13 ligne 2129, échec réseau, sans
  rapport avec ce changement). 170 avant → 179 après, mes 8 tests inclus.

**Ce qui reste.** Côté Rust : la webview cachée, la liste blanche d'hôtes issue
de `domaines_source`, l'attente de fin de challenge par `reste_interstitiel`,
et le rejeu de la requête ensuite. C'est l'étape 3 du plan.

### Implémentation — étapes 2 et 3 : webview cachée + rejeu (2026-10-13)

**Mesure 0 d'abord, comme annoncé.** Le canal tiré porte le HTML **en une
lecture** : `outerHTML` de la page chapitre de TOONFR = 124 984 caractères,
retourné intact en **38 ms**. La réserve du spike est levée, pas de découpage.

**Moitié Rust.** `challenge.rs` : commande `webview_lire(source, url)`.
- liste blanche réutilisée telle quelle (`valider_url_challenge(url,
  domaines_source(source), None)`) — donc **la même** garantie que la fenêtre de
  vérification : jamais un hôte annoncé par le site ;
- webview **cachée** (`.visible(false)`), étiquette `webview-<source>` —
  **distincte** de `challenge-<source>`, qui est visible et se ferme. Confondre
  les deux ferait rouvrir une fenêtre visible à chaque page, ou chercher une
  fenêtre déjà fermée ;
- fenêtre **réutilisée** : `navigate` sur l'existante plutôt qu'une construction
  par page (12 Go de machine, un JVM) ;
- attente de la fin du challenge par `reste_interstitiel` — la leçon du
  2026-10-11 (« 7 cookies passés » alors que personne n'avait résolu). Une page
  illisible est traitée comme « pas encore résolue », jamais comme un succès ;
- puis `eval_with_callback("document.documentElement.outerHTML")` et `injecter`
  vers le moteur.

**Moitié front.** `loadSource` tente le transport **avant** d'ouvrir le bandeau,
avec une garde `transportDejaTente` contre la récursion : si le transport
réussit mais que le rejeu échoue encore, on retombe sur le bandeau plutôt que de
reboucler. Trois clés i18n ajoutées (fr + en).

**Un piège de porte, à relire.** `aucune_commande_tauri_nest_morte` a refusé le
build dès que `webview_lire` a été enregistrée sans appelant — c'est **la porte
qui fait son travail** (leçon du 2026-10-04 : elle ne mord pas au moment où on
l'écrit, mais après branchement). Corrigé par le câblage front, pas par sa
suppression.

**Et un piège que j'ai failli prendre.** Après avoir réparé un test en
restaurant un fichier par `Copy-Item`, `cargo test` est resté rouge : le backup
conserve le `mtime` d'origine, donc cargo n'a **rien recompilé** et l'ancien
code cassé tournait encore. C'est la leçon du 2026-10-05 déjà écrite dans
`AGENTS.md` §14, et je viens de la payer une deuxième fois. Le symptôme était
trompeur : le source affichait le bon code, le binaire exécutait l'ancien.
Forcer `LastWriteTime` a suffi.

**Portes, toutes passées.**
- `gradle test --rerun-tasks` (moteur) : 179 tests, 1 échec **préexistant**
  (`ZenMangaInterceptionTest`, réseau, ligne 2129 du journal) ;
- `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`
  : **198 + 1 + 23 tests verts**, dont `aucune_commande_tauri_nest_morte` ;
- front : `typecheck`, `lint`, `i18n:check`, `hooks:check`, `francais:check`,
  `perso:check`, `ipc:check` — tous verts.

**Porte prouvée rouge** (étape 2) : en faisant rendre `challenge_label` à
`transport_label`, `la_webview_de_transport_n_est_pas_la_fenetre_de_verification`
tombe en nommant la confusion — `left: "challenge-TOONFR", right: "webview-TOONFR"`.

**Ce qui reste — étape 4.** Reconstruire le moteur (`scripts/build-engine.ps1`),
puis la mesure qui n'a **jamais** été faite : un chapitre de TOONFR affiché dans
le **lecteur**, de bout en bout, sans fenêtre visible. C'est le critère de
succès du spike, pas encore atteint.

### Implémentation — étape 4 : mesure de bout en bout, et le bug qu'elle a trouvé (2026-10-13)

> **⚠ Le code de ce bloc n'est plus dans l'arbre — décision humaine du
> 2026-10-14.** `challenge.rs` a été remis à l'état commité (`d920178`) et le
> correctif décrit ici est **mis de côté** : stash `20d6ba0` (1 fichier,
> 158 insertions / 20 suppressions), plus une copie hors dépôt. Restauration :
> `git stash apply 20d6ba0`.
> **Ce qui suit ne doit donc pas être lu comme « livré ».** La *mesure*, elle,
> reste vraie : elle porte sur du code qui tournait au moment où elle a été
> faite — c'est ce qui permet de la garder.
> Pourquoi il a été mis de côté : le rejeu par `fetch` itère jusqu'au délai sur
> un corps vide (l'AJAX de TOONFR rend 0 octet), et `latest` échoue donc en
> boucle au lieu d'échouer proprement. Les portes n'ont pas été repassées
> depuis la dernière modification. Cf. « Mesure multi-sources » plus bas pour ce
> que le transport peut réellement servir.

**Le bug : `evaluer_javascript` ne savait pas lire une chaîne brute.** Mesuré
dans l'app réelle, `webview_lire` renvoyait
`lecture du HTML rendu impossible` alors que la page avait résolu son challenge
et affichait son contenu — la webview affichait « Manga – Toonfr ». Cause :
`eval_with_callback` rend une chaîne **JSON**. Une expression qui rend un
**objet** (`JSON.stringify({...})`, ce que faisaient tous les appels existants)
produit `{"cle":…}`, que `from_str` résout. Une expression qui rend une
**chaîne brute** — `outerHTML` seul — produit `"<!DOCTYPE html>…"`, du JSON
dont la valeur est une `Value::String`, que le code jetait.

Corrigé par deux gestes complémentaires :
- l'expression passe par `JSON.stringify` (comme les autres appels) ;
- **la logique de parsing est extraite en `resoudre_rendu`**, parce qu'enfouie
  dans `evaluer_javascript` elle était intestable — et un bug intestable est un
  bug qui revient.

**Porte prouvée rouge** sur le vrai cas : en rendant `None` à `resoudre_rendu`,
les deux tests `une_chaine_brute_rendue_est_resolue_en_html` et
`un_objet_rendu_reste_resolu` tombent. 201 tests verts après correction.

**Le résultat mesuré, de bout en bout.**

| étape | mesure |
|---|---|
| transport webview (appel direct) | **170 995 octets** renvoyés |
| injection moteur | `webview html injecte url=https://toonfr.com/webtoon/ octets=170924 totalMemoire=1` |
| catalogue après transport | notice « page obtenue par le navigateur », puis **la liste remplit l'écran** (« Girls Love One Shot Psychologique Shôjo Ai Drama… ») |
| **sans fenêtre visible** | oui — la webview de transport est cachée |

Le flux complet fonctionne : `CHALLENGE_REQUIRED` → webview cachée → challenge
résolu → HTML injecté → mémoire moteur → catalogue servi. **L'option B est
opérationnelle pour la liste.**

**Ce que la mesure a montré de plus, et qu'il faut traiter.** Le second appel
de transport porte sur `https://toonfr.com/wp-admin/admin-ajax.php` et ne rend
que **40 octets** — c'est une requête **AJAX** du site, pas une page. La liste
des genres (« 46 genres au total ») vient de là. 40 octets de réponse AJAX
injectés comme du HTML ne servent à rien, et laissent une entrée inutile en
mémoire. **Non corrigé, consigné ici** : la mémoire ne devrait accepter que du
HTML, ou le transport devrait distinguer les deux. Ce n'est pas un blocage —
le catalogue s'affiche — mais c'est une entrée polluante à traiter.

**Enfin, le critère du spike n'est pas encore rempli.** « Un chapitre complet
dans le lecteur » n'a pas été mesuré : la liste oui, la lecture non. Le spike
est donc **GO partiel** sur le transport de liste.

### Implémentation — étape 4 (suite) : la mesure qui a corrigé ma conception (2026-10-13, ~22 h)

**Ce que j'ai annoncé et qui était faux.** J'ai écrit que le catalogue se
remplissait (« Girls Love One Shot Psychologique Shôjo Ai… »). La mesure suivante
a montré le contraire, et m'a obligé à reprendre le raisonnement depuis le
début.

**La vérité mesurée, dans l'ordre.**

1. TOONFR est un parser **Madara**. Il n'interroge pas `/webtoon/` mais
   `wp-admin/admin-ajax.php` — une URL **AJAX**, donc du JSON. Le log le dit :
   `code=CHALLENGE_REQUIRED msg=challenge at https://toonfr.com/wp-admin/admin-ajax.php`.
2. Mon transport lisait `document.documentElement.outerHTML` de la page où il
   avait navigué — c'est-à-dire `/webtoon/`, une **autre** page. Il renvoyait
   170 Ko de HTML pour une URL que le moteur n'avait pas demandée, puis
   40 octets de rendu JSON pour celle qu'il demandait.
3. Le catalogue qui « se remplissait » venait de `filterOptions`, une requête
   **différente** de `latest`. `latest` rendait `latest ok: 0 items` — le
   catalogue était vide et je ne l'avais pas vu.

**Le principe que j'avais faux.** Le transport ne doit pas rendre « ce que la
webview affiche », mais **le corps de la réponse de l'URL exacte que le moteur
a demandée**. Un `fetch` vers cette URL, depuis la page même origine, hérite de
la clearance et rend le HTML ou le JSON attendu. Corrigé en deux temps
(`fetch` non attendu → globale → relecture), parce que `eval_with_callback` est
synchrone et ne rend pas une promesse — le même schéma que
`identite_de_la_fenetre` pour `getHighEntropyValues`.

**Deuxième défaut, mesuré après la correction, toujours ouvert.** La globale se
résout en `{ok: true, corps: ""}` : le `fetch` aboutit mais la réponse est
**vide**. Et `filterOptions` échoue encore en `403` — donc la webview de
transport n'a pas, ce jour-là, la clearance nécessaire. Mon code rejette un
corps vide et boucle jusqu'au timeout, ce que le front affiche comme un échec.

**Verdict honnête.** L'option B est **partiellement** opérationnelle :
- le transport ouvre bien la webview, résout le challenge, et renvoie des octets
  (mesuré : 170 995) ;
- mais il ne sert **pas encore** la requête `latest` de TOONFR, parce que
  (a) la conception initiale rendait la mauvaise URL, et (b) après correction,
  le `fetch` sur l'URL AJAX rend une réponse vide.

Le critère de succès du spike — **un chapitre complet affiché dans le lecteur,
sans fenêtre visible** — n'est **toujours pas** atteint. Je ne le déclare pas
atteint, et je ne commite pas cette dernière étape comme un succès.

**Ce qu'il faut mesurer avant d'aller plus loin** : pourquoi la webview de
transport n'obtient-elle pas la clearance pour `admin-ajax.php`, alors qu'un
onglet frais l'obtient en 6 s (mesuré le même jour) ? La piste la plus probable
est `.visible(false)` : la fermeture automatique du challenge n'a pas lieu sans
rendu. C'est la prochaine mesure, et elle est décisive.

#### La vérité mesurée, et l'hypothèse `.visible(false)` est écartée

**La webview cachée n'est pas le problème.** Mesures comparatives, même jour :

| vue | URL | résultat |
|---|---|---|
| onglet frais (CDP, visible) | `/webtoon/` | résolu en **6 s**, `cookies=450` |
| **webview cachée de l'app** | `/webtoon/` | **200, 178 024 o**, `cookies=450` |
| webview cachée | `admin-ajax.php` nu | **400, 1 o** |
| webview cachée | `admin-ajax.php?action=madara_load_more&…` | **200, 0 o** |

La clearance est bien présente dans la webview cachée, au même titre que dans un
onglet visible. L'hypothèse `.visible(false)` est donc **écartée par la mesure** —
elle était séduisante, elle est fausse.

**Et la liste est déjà dans `/webtoon/`** : **48 liens de manga, 57 items**,
177 635 octets, statut 200. Le transport webview atteint donc parfaitement le
contenu voulu.

**Ce qui bloque réellement, réduit à une ligne.** Le moteur demande
`https://toonfr.com/wp-admin/admin-ajax.php` (parser Madara en mode
`madara_load_more`), URL qui rend **0 octet** même depuis la webview. Il ne
demande jamais `/webtoon/`, la page qui contient la liste. Le transport, lui, est
câblé pour rejouer **l'URL que le moteur a demandée** — donc l'AJAX vide.

**Ce que ça veut dire, sans le maquiller.** Deux lectures possibles, et la mesure
ne les départage pas encore :
1. le parser Madara de *cette* source a besoin d'un POST/GET AJAX que Cloudflare
   vide, et le transport webview ne peut rien y faire ;
2. ou bien l'URL d'accueil du parser est mal réglée, et `/webtoon/` suffirait.

Départager demande de lire `ToonFr.kt` / la classe Madara pour savoir quelle URL
elle demande en `getList`, et si une autre forme d'URL marche. **Non fait.**

**Bilan factuel de l'option B à cet instant :**
- ✅ mécanisme de bout en bout : webview cachée → challenge → clearance → corps
  de la réponse → mémoire moteur → intercepteur. Tout est en place et testé.
- ✅ `/webtoon/` est atteignable par le transport : 200, 177 Ko, 48 liens.
- ❌ `latest` de TOONFR n'est **pas** servi : le moteur demande une URL AJAX vide.
- ❌ le critère du spike (chapitre complet dans le lecteur) n'est pas atteint.

### Mesure multi-sources — ce que l'option B peut vraiment servir (2026-10-14)

**La question posée :** le transport webview sert-il autre chose que TOONFR ?
**La méthode :** les 15 sources FR classées `CHALLENGE_REQUIRED` au relevé du
2026-10-12, chacune ouverte dans un **vrai onglet** WebView2, challenge résolu,
puis relevé du statut et du nombre de liens internes.

**Deux pièges de mesure rencontrés et corrigés dans la sonde :**
1. elle s'arrêtait dès la fin de l'interstitielle, donc **avant** que la page
   peigne : TOONFR était relevé à 43 Ko / 0 lien alors qu'il fait 177 Ko /
   48 liens. La sonde mesurait sa propre impatience.
2. un `200` sans lien ne prouve rien — c'est le piège de TOONFR. On compte donc
   les liens, pas seulement le statut.

**Le relevé, par verdict.**

| verdict | sources | ce que ça veut dire |
|---|---|---|
| **MORTE (522/523)** | BERSERKSCAN, BLUELOCKSCAN, HAIKYUU, HELLSPARADISESCAN, KINGDOMSCAN, MYHEROACADEMIASCAN, SCANBORUTO, SCANJUJUTSUKAISEN — **8** | l'origine der Cloudflare est éteinte ou ne répond plus. **Aucun client ne peut les lire**, transport ou pas. C'est une réclamation de catalogue, pas un travail de transport. |
| **TLS (525)** | MANGAMANA — **1** | la chaîne TLS de l'origine échoue, même dans un navigateur. |
| **URL qui ne répond pas (404)** | ASTRALMANGA — **1** | `/manga/` est une **page de détail** (un UUID, forme d'agrégateur), pas la liste. Le site est **vivant**. Cf. « AstralManga : le 404 ne dit rien du site » plus bas. |
| **SERVIE par le navigateur** | **EPSILONSOFT** (200, 439 Ko, 103 liens), **HENTAIZONE** (200, 236 Ko, 48 liens), **ONISAGA_FR** (200, 9,8 Mo, 50 liens), **SCANHENTAIMENU** (200, 122 Ko, 79 liens), **TOONFR** (200 sur `/webtoon/`, 179 Ko, 127 liens) — **5** | **le transport webview a des cibles réelles.** |

### EPSILONSOFT : j'ai mesuré le mauvais domaine (2026-10-14)

Classé `PERIMEE` à première lecture, à tort. Le bytecode tranche
(`javap -c` sur `EpsilonSoft.class`) :

```
ldc  "beta.epsilonsoft.to"      ← l'hôte que le parser interroge
iconst_1 → putfield withoutAjax ← true : il demande des pages HTML simples
```

Or j'avais mesuré `epsilonsoft.to/manga/`. L'**apex** répond 404 sur ce chemin,
et sert sur sa page d'accueil une **fausse mise à jour de navigateur**. Le
`beta.` est le site réel : 200, 439 Ko, **103 liens**, dont 86 en `/serie/`.

**La règle, appliquée partout :** une URL est une valeur **reçue**. Avant de
juger un hôte, interroger son **producteur** — ici le bytecode du parser, pas le
DNS, ni le rapport d'erreur d'hier.

**Conséquence directe pour l'option B :** `withoutAjax = true` signifie que le
parser demande des pages HTML simples. C'est **exactement** ce que le
`webview_lire` commité sait lire (le DOM rendu de l'URL demandée). C'est donc la
**première source** où la chaîne complète peut fonctionner sans le rejeu par
`fetch` — celui qui est aujourd'hui en stash.

> ### À VÉRIFIER — 2026-10-14 : page de fausse mise à jour
>
> `epsilonsoft.to` sert sur son accueil un titre de type « *Transcending Species:
> Due to System Error* », un schéma de **fausse mise à jour de navigateur** que
> j'ai rencontré dans le contexte adware. **Je n'ai pas réussi à le confirmer**
> par recherche le 2026-10-14, donc ce n'est pas un fait établi.
> Deux gestes bon marché pour trancher : scanner l'URL sur VirusTotal, et, si
> un clic a suivi, vérifier les extensions et « Applications et fonctionnalités ».
> Ne pas ouvrir ces domaines dans un profil de navigateur principal.

### AstralManga : le 404 ne dit rien de l'état du site

`AstralManga.<init>` porte bien `astral-manga.fr`, mais la classe **ne surcharge
pas** `withoutAjax` → comme ToonFr, elle part sur la branche `admin-ajax.php`.
Et `/manga/` n'est pas une liste : c'est une page de détail, identifiée par un
**UUID** — donc un agrégateur, pas un thème manga classique.

Conséquence de méthode : mon tableau de verdicts disait « 404 — **le site est
vivant**, l'URL n'existe plus ». La seconde partie était une **affirmation non
mesurée** ; la mesure ne portait que sur l'URL. La glose est corrigée dans la
sonde : un 404 prouve que **cette URL** ne répond pas, et rien de plus.

### La sonde a menti trois fois, et c'est ce qui l'a rendue fiable

Les écarts entre runs n'étaient pas les sites, c'est la sonde :

| symptôme | cause | correction |
|---|---|---|
| TOONFR à 43 Ko puis 177 Ko | sortie dès la fin de l'interstitielle, **avant** le rendu | attente de **stabilité**, pas délai fixe |
| MYHEROACADEMIASCAN classée `BLOQUE` | le tableau ignorait `522 Connection timed out` | `522` et `523` regroupés comme origine injoignable |
| HENTAIZONE à 0 lien (48 attendus) | un **titre vide** faisait sortir la boucle, et « 0 lien deux fois » passait pour stable | titre vide = « pas chargé » ; **plancher** minimal avant d'accepter une stabilité |

Une mesure qui change selon qu'on la regarde deux fois n'est pas une mesure. Le
« 200 sans lien » de HENTAIZONE aurait pu me faire conclure à tort que la source
était morte — c'est-à-dire **purger du catalogue une source vivante**.

**Ce que la mesure décide.**

1. **L'option B a des cibles.** 5 sources sur 15 sont lisibles par le vrai
   navigateur, et **EPSILONSOFT en tête** : c'est la seule dont le parser
   demande une page HTML simple (`withoutAjax = true`), donc la seule que le
   transport **commité** peut servir sans le rejeu par `fetch`.
2. **Le plus gros gains n'est pas dans le transport.** 8 des 15 sources sont
   **mortes** (Cloudflare 522/523). Les sortir du catalogue aiderait l'utilisateur
   bien plus que n'importe quel transport — et c'est une action de catalogue,
   pas d'architecture.
3. **TOONFR reste le mauvais exemple.** Son `latest` passe par un AJAX vide
   (0 octet) même dans un navigateur. C'est un défaut du parser, pas du
   transport. Il ne doit plus servir de critère de décision.
4. **Et « périmée » était trop vite dit.** Une URL en 404 ne prouve pas qu'un
   site est mort : ASTRALMANGA répond 404 sur sa page de détail alors que le
   domaine vit. Cette distinction a coûté une fausse entrée dans le tableau.

**Décision à prendre par l'humain.** Sur ces mesures, trois issues, par ordre
de ce que je recommande :
1. **Purger les 8 mortes du catalogue** (action de catalogue, effet immédiat,
   zéro risque) — et mesurer le catalogue complet, pas seulement les FR, parce
   que 15/98 était un relevé partiel.
2. **Brancher le transport sur les 3 sources servies**, et mesurer un chapitre
   complet dans le lecteur — le critère du spike, enfin testable sur une vraie
   cible.
3. **S'arrêter là.**

**Ce qui n'est toujours pas prouvé :** un chapitre affiché dans le lecteur, ni
pour TOONFR ni pour HENTAIZONE / ONISAGA_FR / SCANHENTAIMENU. Le transport est
câblé et atteint des 200 réels ; il n'a jamais été suivi jusqu'à l'image
affichée.

### Décision humaine du 2026-10-14, et ce qu'elle fixe

Séquence décidée, dans cet ordre :

1. **le travail non validé est mis de côté** — fait, voir l'avertissement en tête
   du bloc « étape 4 ». `challenge.rs` est revenu à `d920178` ; le reste est en
   stash `20d6ba0`.
2. **commiter les mesures** — fait. Voir « Les mesures sont devenues des
   artefacts » ci-dessous : sans outil commité, un relevé n'est qu'un chiffre
   dans un journal, et il n'est pas rejouable demain.
3. **re-tester les 8 mortes le 2026-10-15** — voir la tâche datée plus bas.
4. **option 2 : brancher le transport sur les 3 sources servies** et mesurer un
   chapitre complet. C'est le critère du spike, enfin testable sur une vraie
   cible.

**Pourquoi 3 est rejouable en une commande et pas 1 et 2.** Un site éteint peut
se rallumer ; une source est un être vivant (§ `scripts/sonde-sources.mjs`). Un
`523` du 2026-10-14 n'est pas un verdict éternel, seulement un état à une date
donnée — d'où la tâche datée, plutôt qu'une suppression définitive.

### Les mesures sont devenues des artefacts (2026-10-14)

Trois fichiers, et ils se lisent mutuellement :

| fichier | rôle |
|---|---|
| `scripts/sonde-navigateur.mjs` | l'outil. Mesure « et si c'était un vrai navigateur ? », ouvre chaque URL dans **le WebView2 de l'application** — le navigateur du transport — et relit statut, poids et **nombre de liens**. |
| `docs/mesures/navigateur-cibles.json` | les 15 sources, leur URL réelle et leur **groupe** (`morte`, `tls`, `perimee`, `servie`). C'est l'entrée, donc la mesure est rejouable. |
| `docs/mesures/navigateur-2026-10-14.json` | le relevé du jour, avec le navigateur utilisé et la signification de chaque verdict. |

**Deux choses que la sonde refuse de faire, et qui sont des leçons.**

1. **Un `200` n'est pas un succès.** C'est exactement le piège de TOONFR, dont
   l'AJAX rend 200 avec 0 octet. Le verdict compte donc les liens ; sans cela la
   sonde aurait classé une page vide comme servie.
2. **Mesurer trop tôt.** La sonde attend la disparition de l'interstitielle puis
   **3 s de plus** : sans ce délai, TOONFR est relevé à 43 Ko / 0 lien au lieu de
   177 Ko / 48 liens. La sonde mesurait sa propre impatience.

**Les deux sorties d'erreur sont prouvées** (`--groupe` inconnu → code 1 en
nommant les groupes ; port CDP fermé → code 2 en donnant la commande de
remédiation). Une porte qui n'a jamais crié n'est pas une porte.

### [2026-10-15] Tâche : re-tester les 8 sources mortes

Préalable : elles ont été mesurées **mortes** le 2026-10-14 (Cloudflare
`523 Origin is unreachable`). Un `523` est un état, pas un verdict : vérifier
avant de les retirer du catalogue.

```powershell
# 1. lancer l'application avec le port de pilotage
$env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS="--remote-debugging-port=9223"
pnpm tauri dev

# 2. re-mesurer les 8, dans un NOUVEAU fichier daté (on ne réécrit pas l'ancien)
node scripts\sonde-navigateur.mjs --groupe morte `
     --sortie docs\mesures\navigateur-AAAAMM-JJ.json

# 3. et le reste, pour voir ce qui a bougé depuis
node scripts\sonde-navigateur.mjs --sortie docs\mesures\navigateur-AAAAMM-JJ.json
```

**Ce qui décide.** Une source reclassée `UTILE` rejoint les cibles du transport.
Une source toujours `MORTE` est retirée du catalogue, avec la date du relevé dans
le motif — pour que la décision soit réversible et datée, pas superstitieuse.

### Option 2 — trois défauts de parseur, pas un problème de transport (2026-10-14)

La mesure décisive demandée portait sur une source dont le parseur n'était pas
déjà condamné. Elle a été conduite sur **HENTAIZONE**, et au passage sur
EPSILONSOFT et ASTRALMANGA. Résultat : **aucune des trois n'est un problème de
transport.**

**Ce que dit le bytecode des trois parseurs Madara.**

| source | fait mesuré dans le bytecode | verdict |
|---|---|---|
| TOONFR | `listUrl = "webtoon/"` mais **pas** de surcharge `withoutAjax` → branche `admin-ajax.php`, qui rend **0 octet** même dans un vrai navigateur | défaut de parseur |
| ASTRALMANGA | **aucune** surcharge `withoutAjax` non plus → même branche AJAX | défaut de parseur |
| EPSILONSOFT | `ldc "beta.epsilonsoft.to"` + `iconst_1 → putfield withoutAjax`, donc pages HTML simples ; mais `listUrl` **par défaut** `manga/`, périmé (le site sert `/library`) | défaut de parseur |

**Deux sources de vérité pour l'hôte, dans le même parseur.** Constantes lues
dans `MadaraParser` : `"https://"`, `"/page/"`, `"manga/"` (le `listUrl` par
défaut), `"manga-genre/"`. L'URL rapportée est **reconstruite** par le parseur,
alors que la requête part sur un autre hôte :

```
requête réelle :  beta.epsilonsoft.to   → 403 cf-mitigated=challenge
URL rapportée  :  https://epsilonsoft.to/manga/
```

Rust refuse alors l'URL (`challenge: hôte d'image refusé`) parce que l'hôte
annoncé n'est pas un domaine connu de la source. **Le transport n'a donc jamais
démarré pour EPSILONSOFT.**

**Pour HENTAIZONE, l'URL est correcte** — requête `hentaizone.xyz`, message
`challenge at https://hentaizone.xyz/manga/`, **même hôte**. Rust accepte. Et là
le transport démarre, puis échoue.

### Le point exact de rupture, et il est dans notre code

Appel direct de la commande depuis la page (pour isoler le front de Rust) :

```
invoke('webview_lire', { source: 'HENTAIZONE', url: 'https://hentaizone.xyz/manga/' })
  → ERREUR :: lecture du HTML rendu impossible
```

Donc : **front → Rust fonctionne**, la validation d'hôte passe, et l'échec est
dans la lecture du HTML rendu :

```rust
let html = evaluer_javascript(&fenetre, "document.documentElement.outerHTML")
    .and_then(|v| v.as_str().map(str::to_string))
    .filter(|h| !h.is_empty())
    .ok_or_else(|| "lecture du HTML rendu impossible".to_string())?;
```

Et **`resoudre_rendu` est absent de l'arbre commité** (0 occurrence). C'est
exactement la fonction que le travail mis de côté (stash `20d6ba0`) ajoute. Le
blocage de l'option B n'est donc pas une cible morte : c'est **le correctif non
validé lui-même**.

**Ce qui est prouvé et ce qui ne l'est pas.**
- Prouvé : le transport est câblé, joignable, et son garde d'hôte laisse passer
  une URL correcte.
- Prouvé : `latest` échoue pour les trois sources, et **la cause est en amont**,
  dans la configuration des parseurs.
- **Non prouvé** : qu'un chapitre s'affiche. Toujours rien.
- **Non expliqué** : pourquoi la lecture échoue alors qu'elle a réussi sur TOONFR
  (170 995 octets mesurés le 2026-10-13). Hypothèse à vérifier : une taille de
  page (HENTAIZONE 236 Ko contre TOONFR 177 Ko) qui dépasserait le canal de
  retour. **Ce n'est qu'une hypothèse**, et elle ne vaut rien tant qu'elle n'a
  pas été testée.

### Option 2 — le transport fonctionne, mais il n'est câblé que sur `latest`

Le travail mis de côté est restauré, **corrigé**, et validé. La mesure a donné
plus qu'on ne l'espérait — et moins, ce qui est la même chose.

**Les deux défauts du code restauré, trouvés par la lecture et non par le test.**

1. `evaluer_javascript` faisait un **deuxième `from_str`** sur une
   `Value::String`. Pour une expression qui rend une chaîne brute — c'est le cas
   de `outerHTML` — ce second décodage tentait de parser **du HTML comme du
   JSON**, et échouait toujours. La fonction ne pouvait donc **jamais** rendre
   une chaîne. Prouvé par lecture, avant toute mesure.
2. Plus général : l'appelant faisait `.as_str()` sur le retour de
   `evaluer_javascript`, qui rend une `Value` **objet** quand l'expression est
   `JSON.stringify(...)`. `as_str()` sur un objet donne toujours `None` : la
   boucle d'attente n'aboutissait **jamais**. Mesuré : 22 s puis
   « relecture impossible », sur TOONFR comme sur HENTAIZONE.

**Preuves, dans l'app réelle** (appel direct de la commande) :

| cas | avant | après |
|---|---|---|
| AJAX de TOONFR (corps vide) | 22 s puis « relecture impossible » — **un mensonge** | **2 s**, « le site a renvoyé un corps vide pour … » |
| HENTAIZONE `/manga/` | « lecture du HTML rendu impossible » | **158 285 octets**, injectés, 2 s |

Le second cas est la **première fois** qu'une source Cloudflare est servie dans
l'application. `latest` rend 6 fiches, et **elles persistent** (recontrôlées 3 s
après : 6, pas 0).

**Un corps vide est un résultat, pas une absence de résultat.** La boucle
attendait 20 s de plus et concluait « impossible » ; elle dit maintenant ce qui
s'est passé. C'est le même principe que « 404 sans corps n'est pas une panne ».

**Ce qui manque, et c'est le vrai sujet.** Le transport n'est invoqué que depuis
`loadSource` (l'interface), donc **uniquement pour `latest`**. Toutes les URLs
suivantes échouent encore :

```
04:40:19 commands: details source=HENTAIZONE manga=3249083d
04:40:19 stdout erreur id=details code=CHALLENGE_REQUIRED
           msg=challenge at https://hentaizone.xyz/manga/regressed-warriors-…/
```

Chaque série est une **autre URL**, et rien ne la fait passer par le transport.
Le même défaut appearait déjà sur `latest` pour EPSILONSOFT : une seule URL de
transport par commande, alors qu'un parcours en demande des dizaines.

**Donc le critère du spike reste non atteint** : aucun chapitre affiché. La
question suivante n'est plus « est-ce que ça marche » mais « où le transport
est-il_branché » — et la réponse est : trop haut, et une seule fois. Rattacher
le transport **au cœur**, sur toute URL refusée, plutôt qu'à une commande
précise de l'interface, est une décision d'architecture.

**Un walk peut mentir avec un sélecteur.** Le parcours a rapporté « 100
chapitres », mais `chapters` avait échoué au journal : `.chapter-open`
sélectionne la liste « chapitres récents » **de l'accueil**, pas celle de la
série. Même famille que « le plus grand ancêtre contenant le titre englobe toute
la liste ». Un sélecteur ne se valide pas sur un nombre > 0.

### Deux pièges de mesure payés pour ce run

1. **Un walk peut lire un état transitoire.** EPSILONSOFT et HENTAIZONE ont
   tous deux affiché **6 cartes** puis **0** trois secondes plus tard, avec
   `latest` en échec au journal. Le même nombre (6) pour deux sources
   différentes est ce qui a fait tilter : ce n'était pas une liste de mangas.
   Relire après stabilisation, et croiser avec le journal, pas se fier au premier
   relevé.
2. **`webview_lire` ne journalise pas ses échecs.** Il avertit quand il **refuse
   une URL**, pas quand il échoue ensuite. Donc « aucune ligne `transport` dans le
   journal » ne prouve pas « le transport n'a pas démarré » — c'est un appel
   direct via `invoke` qui l'a montré. Corollaire général : **« aucune trace »
   n'est pas « rien ne s'est passé »**, c'est « rien n'a été écrit ».

---

## Journal 2026-10-15 — 6 points restants (tous traités)

1. sanitize plafonné à 80 → hash FNV-1a sur le nom complet (commit ix(download)).
2. CI : cargo audit (rustsec/audit-check) + gitleaks (gitleaks/gitleaks-action).
3. Pool JVM 
ewCachedThreadPool → 
ewFixedThreadPool(min(8,max(processors,2))). Cancellation OkHttp reste non branchée (@todo 3.5).
4. SessionCache jamais purgée : 7 compteurs d'insertions + 	ailleTotale + ligne stderr tous les 1000. Politique d'éviction **non choisie** — volontaire.
5. .map() sans key : vérifié ; React ne signale rien (eslint jsx-key à 0) et la sandbox {Array.from} utilise déjà key={i}. Rien à faire.
6. importer_source sans jeton de dialogue : confirmé non exposé côté UI, 7 tests de traversal verts. Tracabilisé dans le code + tâche pour S2.

### Journal 2026-10-15 — recherche globale d'un webtoon (mini-plan, avant code)
Demande : chercher un webtoon **par titre** depuis la page catalogue, sur
**toutes les sources actives** (choix explicite de l'utilisateur, pas seulement
la langue affichée). Contexte mesuré avant de coder : le champ de recherche du
catalogue filtre les **sources** par nom (`nameFilter`), et la recherche par
titre existe déjà mais **à l'intérieur** d'une source ouverte
(`searchInSource`). Le moteur expose `search` (Server.kt:1440), `listSources`
est caché côté moteur (`sourcesNatives`, construit une fois), et le moteur
traite au plus `MAX_WORKERS` (2..8) requêtes en parallèle. Mini-plan :

1. **Rust `src/recherche.rs`** — commande `recherche_globale(id, query)` :
   `listSources` → retrait des sources désactivées (SQLite) → fan-out `search`
   par lots de `CONCURRENCE`, délai par requête, durée maximale de passe ;
   événement `recherche://resultat` émis **par source qui répond** (affichage
   progressif), résumé rendu en fin de passe. Fonctions pures testées :
   validation de la requête, sources à interroger, annulation d'une passe au
   profit de la suivante.
2. **Front `app/page.tsx`** — bloc recherche sur le catalogue (champ + bouton),
   écoute de l'événement filtrée par `id` de passe, grille de résultats
   réutilisant `.manga-grid`, nom de la source sous chaque carte, clic →
   `loadChapters` existant. Jetons de couverture posés par Rust
   (`ajouter_jetons_couverture`), sinon vignettes vides.
3. **i18n FR/EN + portes §8 + commit.**

**Livré le 2026-10-15 (même journée).** Ce qui a été fait, et prouvé :

- **Rust** — `src/recherche.rs` (nouveau) : `recherche_globale(id, query,
  contentRating)`. `listSources` (cache moteur, `Server.kt:784`) → retrait des
  sources désactivées (`store.disabled_sources()`) → fan-out `search` par lots
  de 8 (`MAX_WORKERS` moteur = 2..8), 20 s de délai par requête, passe plafonnée
  à 120 s (`partielle` au résumé si coupée), un événement
  `recherche://resultat` **par source qui répond**, jeton de couverture posé
  par Rust. `request`, pas `request_avec_transport` ni `request_with_retry` :
  1370 fenêtres de vérification ou 3× le trafic vers les hôtes morts seraient
  l'inverse de la politesse. Annulation : `PasseEnCours` (une nouvelle passe
  annule la précédente, vérifiée entre les lots). `contentRating` part
  exactement comme `latest` le construit — la divergence `search`/`latest` sur
  ce réglage avait déjà coûté une régression. Câblage : `lib.rs`,
  `main.rs` (manage + handler), `commands.rs` (`LOCAL_TIMEOUT` et
  `requested_rating` passés `pub(crate)`, même règle qu'`into_result`).
- **Front** — bloc recherche au-dessus du filtre de sources (le filtre, lui,
  continue de filtrer les **sources** par nom) ; les résultats remplacent le
  parcours (tuiles de langue, grille de sources) le temps de la recherche ;
  cartes réutilisant `.manga` > `.manga-open` > `.cover` + `.label`, **nom de
  la source sous chaque carte** (`displayName` si présente) ; clic →
  `loadChapters` existant, précédé d'un `setSelected` — sans lui le
  téléchargement depuis la fiche bute sur `if (!openManga || !selected) return`
  et ne fait rien, en silence. L'`id` de passe, généré par le front, filtre les
  lots périmés dans l'écouteur (abonné une fois, `rechIdRef`).
- **Preuves** — 12 tests unitaires Rust purs, **RED prouvé** en cassant
  l'implémentation (6 tests nommés en échec : filtre désactivées, rognage,
  annulation), puis restore + `mtime` rafraîchi (leçon `Move-Item`) → 12/12.
  Portes : `cargo clippy --all-targets -- -D warnings` exit 0 ; `cargo test`
  **238 passés / 0 échec** ; front `typecheck` 0, `lint` 0 erreur (4 warnings
  exhaustive-deps connus), `i18n:check` 398 clés FR = 398 EN sans clé morte
  (15 clés × 2 ajoutées, toutes utilisées), `hooks/chemins/pages/perso/ipc/moteur`
  exit 0 — `ipc:check` avec la paire `ResumeRecherche ↔ Resume (recherche.rs)`
  ajoutée à `scripts/check-type-ipc.mjs` — `pnpm test` 128/128, `pnpm build`
  exit 0.
- **Dette vue, hors périmètre** — `cargo fmt --check` échoue sur
  `src/download.rs:1000` et `:1010`. Prouvé préexistant : le fichier est intact
  dans git (`git status` le ignore) et `rustfmt --check` échoue déjà sur son
  état HEAD. Non corrigé : reformater un fichier que la tâche ne touche pas
  aurait mis du bruit dans le diff. `recherche.rs`, lui, est formaté.
- **Non fait, à ne pas prétendre** — aucun essai **en direct** de la recherche
  globale (moteur lancé, vraie passe sur 1370 sources). Les portes prouvent la
  mécanique (validation, sélection des sources, annulation, contrat IPC), pas
  le comportement réseau. C'est le smoke test à jouer avant de déclarer la
  fonctionnalité validée.

### Journal 2026-10-15 — audit UX de l'app (mini-plan, avant mesure)

Demande : améliorer l'expérience utilisateur et l'interface de **toute** l'app.
Réponse de l'humain : **audit mesuré d'abord**, priorité à l'**UX
fonctionnelle** (bouton qui ne fait rien, erreur mal affichée, état
incompréhensible), le style visuel ensuite. Méthode : skill `ux-audit` —
inventaire par script, parcours de l'app **réelle** dans la WebView (CDP,
`WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=9223` + `connectOverCDP`), rapport de
findings avec preuve par finding. Mini-plan :

1. Inventaire (script `.mesures/ux-audit/inventaire.mjs`) : écrans,
   composants et composants inutilisés, tokens définis vs utilisés, couleurs
   hors système, composants dupliqués, couverture des états vide/chargement/
   erreur.
2. Parcours (script `.mesures/ux-audit/parcours.cjs`) : accueil → catalogue →
   langue → source → recherche → fiche → chapitre → lecteur → téléchargements
   → réglages → diagnostic. Capture de chaque état, lecture console, mesure
   contraste/focus dans la même passe. Aucune action destructive (pas de
   téléchargement, pas de modification de favoris ou de réglages).
3. Rapport `docs/audit-ux-2026-10-15.md` : findings par flux, transverses,
   « harness », « probablement sain », priorités **proposées** (décision humaine). Les corrections ne commencent qu'après validation.

### Journal 2026-10-16 — corrections de l'audit UX, items 1 à 7

Décision humaine du 2026-10-16 : traiter les 7 points ouverts listés au-dessus.
Ordre d'exécution : **1 → 2 → 7 → 4 → 3 → 5 → 6**, parce que 3 dépend de 4 et
que 5/6 sont des mesures longues.

**Item 1 — travail non commité (fait).** `Erreurs.kt` + `Server.kt` rendaient
la cause d'un `CHALLENGE_UNREPLAYABLE` sans test. `DetailCauseTest` ajouté (6
tests), porte prouvée rouge (`take(160)` → `take(1600)` fait tomber
`un message enorme est tronque`, les 5 autres restant verts).
`gradle test --rerun-tasks` : **214 tests, 0 échec**, 40 classes, XML frais.

**Une correction que le travail non commité rendait nécessaire.** Le détail
ajoute des mots que `describeError` teste, et `TIMEOUT|timeout` passait **avant**
`estRejeuRefuse` : un `SocketTimeoutException: timeout` dans la chaîne classait
un refus définitif en « délai », l'écran promettant un réessai impossible. Rouge
prouvé (`erreur.delai:HENTAIZONE`), puis ordre corrigé — `is_transient` côté Rust
disait déjà « jamais transitoire », les deux côtés doivent dire la même chose.

**Item 2 — findings de l'audit (4 corrigés, 2 en attente de mesure).**

- **F1 (clé React)** — `fenetre.map` retournait `<>` sans clé ; le `key` était sur
  l'`<img>` interne. Clé posée sur le `Fragment`. **Non vérifié en app réelle** :
  la preuve de l'audit est l'absence de l'erreur console, donc elle appartient à
  la passe de mesures.
- **F2 (contraste 3,45:1)** — nouveau token `--accent-texte`, **deux valeurs**
  (`#5b8ceb` sombre = 4,85:1 ; `#2f5fc0` clair = 5,43:1), sur le modèle de
  `--danger-texte`. Le pire cas réel est `--surface-2` (#222), c'est-à-dire
  l'encart **au survol**, pas le fond mesuré par l'audit. Le script de mesure
  retrouve exactement les 3,45:1 du rapport : il calcule comme la passe qui a
  produit le chiffre.
- **F4 (deux recherches)** — la cause n'était **pas** un choix d'affichage :
  `recherche-globale` et `recherche-resultats` sont utilisées depuis `e316a4d`
  et n'ont **jamais** été définies dans `globals.css`. `styles:check` était donc
  **rouge depuis ce commit**, et le journal n'en liste que 9 portes sur 14. Zone
  propre + étiquette **visible** (pas `aria-label` : un placeholder disparaît à la
  première frappe). Décision humaine : séparer visuellement, garder les deux.
- **F6 (clés dupliquées)** — `chapitresSansDoublons` retire les chapitres de même
  `url`, **avant** le filtre de recherche (sinon un champ vide laissait repasser
  les doublons). Clé robuste = masque l'avertissement React en laissant les
  doublons à l'écran ; or deux lignes de même URL sont indiscernables pour
  l'utilisateur (case, téléchargement et chapitre en cours sont indexés sur l'URL).
  Porte prouvée rouge : garder le dernier passage fait tomber `le premier passage
  d une URL reste visible`.
- **F3 (source vide muette) — non corrigé, mesuré contradictoirement.** La condition
  d'état vide existe et est correcte (`latest.length === 0 && !loading && !error`,
  `page.tsx:6469`), et le chemin succès met `loading` à faux. Donc « aucun
  message » vient soit de la mesure de l'audit, soit d'un état que je n'ai pas
  observé. **À trancher en passe de mesures**, pas à corriger à l'aveugle.
- **F13 (fenêtres Cloudflare) — décision humaine : mesurer d'abord.** Regroupé
  avec les items 5 et 6, qui demandent la même instance.

**Item 7 — dette `cargo fmt` (fait).** Deux blocs de `download.rs`, tous deux dans
un test. `cargo fmt` + vérification que rien d'autre n'a bougé. Gates : `fmt` 0,
`clippy --all-targets -D warnings` 0, `cargo test` **262 verts** (238 + 1 + 23).
`cargo test` échouait « Accès refusé » tant que l'app tourne : l'instance du
2026-10-15 20:54 (PID 5600) a été fermée sur accord humain.

**Item 4 — déjà décidé, il y a un commit.** La question « rattacher le transport
au cœur ? » a été tranchée le **2026-10-14** dans `4fd6df5`
(`feat(desktop): le transport part du coeur, plus de l'interface`) :
`request_avec_transport` est câblé sur `latest`, `details`, `chapters`, `pages`.
Les quatre prennent `AppHandle`. Donc **pas de décision à prendre** — il fallait
lire `commands.rs` avant de poser la question (l'affirmation du journal du
2026-10-14, « branché seulement sur `latest` », était périmée dès ce commit).

**Item 3 — ce qui reste est une mesure, pas du code.** La cause désignée est dans
le message de `4fd6df5` : le transport capturait **6 201 octets** là où un appel
direct en rend **158 285** — il n'attendait pas le chargement. Le code actuel n'a
plus ce défaut de forme : il fait un `fetch` dans la webview
(`window.__tatamiCorps`) au lieu de capturer `outerHTML`. Le critère du spike —
**un chapitre affiché de bout en bout** — reste donc non atteint, et sa seule
voie est la passe de mesures.

**Trois angles morts de portes, trouvés en route (non corrigés, hors périmètre).**

1. `styles:check` ne lisait que les `var(--x)` du **JSX**. Élargi au CSS : il a
   nommé **trois tokens jamais déclarés**, utilisés 5 fois — `--text-1`
   (`.slot-knob` → `background` retombe sur `transparent`, donc **l'interrupteur
   n'a pas de tête visible**), `--text-dim` (`.path` → hérite, chemin en texte
   plain), `--focus` (`.bandeau-fermer:focus-visible` → déclaration invalide,
   donc **aucun** anneau de focus). Corrigés vers les tokens existants.
   Troisième token inventé en deux jours : la porte rend la règle mécanique.
2. La comparaison des **classes** prend un sélecteur composé pour une définition :
   `.label` n'existe que comme `.manga .label`, donc le `<span className="label">`
   de la ligne favori ne reçoit **aucune** règle — et la porte le déclare défini.
   Corriger cette comparaison ferait crier cette dette **visible**, donc c'est un
   travail de rendu, pas un nettoyage de porte.
3. **Le journal n'annonce que 9 portes sur 14.** `libelles:check`, `styles:check`,
   `francais:check`, `lecteur:check`, `arrondi:check` ne sont pas dans la liste
   d'`AGENTS.md` §8 — et l'une d'elles était rouge depuis `e316a4d`.

**Ce qui reste ouvert, et dans quel ordre.** Une seule **passe de mesures** couvre
F1, F3, F13, l'item 3, l'item 5 et l'item 6 : instance fraîche + `pnpm dev` (port
3001) + CDP 9223. Aucune de ces vérifications ne peut être faite par une porte.

### Journal 2026-10-16 — passe de mesures (F1, F3, F13, items 3 et 5)

Instance fraîche, moteur reconstruit, CDP 9223. Tout ce qui suit est **mesuré**
dans l'application réelle ; les sondes sont dans `.mesures/session-2026-10-16/`
(non commitées) et leurs sorties JSON avec les chiffres.

**F1 — corrigé et prouvé.** Lecteur ouvert sur `MANGASORIGINES` (contrôle
positif demandé par l'humain) : 6 pages montées, **5 images décodées**
(735×1219, 800×11003, 800×11594, 800×11443), 0 indisponible, compteur `1 / 27`,
bande 77 761 px. **Erreurs console de l'application : `[]`** — donc
`Each child in a list should have a unique "key" prop` a disparu.

**Contrôle positif (pourquoi il compte).** Source → série (« Reborn Rich », 100
cellules de chapitres) → chapitre → lecteur : **la chaîne est saine**. Donc ce
qui bloque l'item 3 n'est pas le lecteur, c'est Cloudflare — sans ce contrôle, on
aurait conclu « le lecteur est cassé ».

**Item 5 — la recherche globale jouée en direct, pour la première fois.**
`reborn rich` sur les 1384 sources : **11 → 20 → 25 → 40 → 45 résultats** entre
6 s et 125 s. L'affichage progressif par source qui répond fonctionne donc tel
que câblé. Deux `404` dans la console pendant la passe, non diagnostiqués (le
journal montre des `fetchCover` réussis de 25 à 79 ko autour) : à reprendre si
les vignettes manquantes se voient.

**F13 — l'hypothèse de l'audit est infirmée.** 90 s de démarrage sans aucune
interaction : **1 seule cible**, alors que le journal montre le tick immédiat du
planificateur (`passe=1 verifies=3 duree_ms=17454`). La fenêtre apparaît **à
l'ouverture d'une source par l'utilisateur** : une cible de plus exactement après
le clic, et le journal `transport: … échec : challenge non résolu en 60 s` dans le
même temps. Les fenêtres vues pendant l'audit venaient donc des clics **du
parcours de mesure lui-même**. Le finding d'audit, reformulé : rien ne relie dans
l'application la fenêtre qui s'ouvre à la source qu'on vient d'ouvrir.

**Item 3 — la cause est désignée, et elle est architecturale.** Sur
`READCOMICSONLINE` comme sur `HENTAIZONE` : `challenge non résolu en 60 s`. La
lecture de la fenêtre de transport tranche entre les deux hypothèses :

  titre « Un instant… » · HTML 29 ko · **0 lien manga** · **0 iframe**
  hentaizone.xyz : **495 cookies** (donc une `cf_clearance` existe)

Donc la page n'est pas résolue, et `reste_interstitiel` a **raison** de le dire.
La cause n'est pas le garde-fou : la fenêtre de transport est créée
`visible(false)` (`challenge.rs:982`), donc **personne ne peut interagir** avec un
défi qui est précisément une page d'interaction. La CSP fautive est **celle du
site** (`default-src 'none' … challenges.cloudflare.com`, relevée en direct), pas
la nôtre. Le critère du spike — un chapitre affiché de bout en bout sur une source
Cloudflare — **reste donc non atteint, et ne peut pas l'être par une mesure
automatique** : résoudre Turnstile demande un clic humain dans la fenêtre.

**Deux défauts trouvés en mesurant, qui ne sont pas Cloudflare.**

1. **La fenêtre de transport reste ouverte après l'échec.** Trois cibles après
   deux sources : l'application plus **deux** webviews Cloudflare, chacune un
   processus. Sur 12 Go (AGENTS.md §3), c'est une fuite mesurée.
2. **60 s avant que l'utilisateur puisse agir.** Le bandeau (`.alert`, bouton
   « Résoudre ») n'apparaît qu'après l'abandon du transport. Pendant 60 s, l'écran
   ne disait rien — c'est ce que le correctif F3 de la journée corrige à 8 s.

**Trois erreurs de mes propres sondes, consignées parce qu'elles sont le genre de
chose qui fait perdre une journée.**

- `.bandeau` ne trouve rien : le bandeau s'appelle **`.alert`**. J'ai conclu
  « l'utilisateur n'a aucune action possible » alors que le texte de vérification
  était à l'écran, et j'ai relancé la sonde **deux fois** avant de relire le JSX.
- `scrollTop: 0` après une molette : le pointeur n'était pas au-dessus de la
  bande. Un compteur qui ne bouge pas est ici un défaut de sonde, pas de molette.
- Le compteur de la recherche renvoie `null` alors que la passe a renvoyé 45
  résultats : sélecteur périmé, pas résultat vide.

**Ce qui reste : item 6** (les 8 sources « mortes » à re-mesurer — la sonde
`scripts/sonde-navigateur.mjs --groupe morte` est prête) et **la décision
d'architecture sur le transport caché**, ci-dessous.

### Décision humaine du 2026-10-16 — le transport caché reste tel quel

Question posée avec les chiffres du jour (« 0 résolution en 2 × 60 s, et la
fenêtre reste ouverte après l'échec »), trois options chiffrées proposées.
**Réponse : ne rien changer pour l'instant.**

Ce qui est donc **accepté, à knowing** et non pas résolu :
- 60 s d'attente avant que le bandeau « Résoudre » soit actionnable (le correctif
  F3 de la journée donne une explication à 8 s, mais l'action reste à 60 s) ;
- la fenêtre de transport reste ouverte après l'échec : deux webviews, donc deux
  processus, pour 12 Go de machine.

Ce qui reste vrai dans tous les cas : **le critère du spike (un chapitre affiché
sur une source Cloudflare) est hors d'atteinte d'une mesure automatique**,
parce que résoudre un défi Turnstile demande un clic humain. Si quelqu'un veut le
prouver un jour, c'est en ouvrant l'application et en résolvant le défi à la main.

Pistes non retenues aujourd'hui, à réactiver si le sujet revient : raccourcir le
délai du transport (60 → 15 s), ou rendre la fenêtre cachée visible au bout de
10 s pour que l'utilisateur agisse sans quitter l'application.

### Item 6 — les 8 sources « mortes » re-mesurées (2026-10-16)

`node scripts/sonde-navigateur.mjs --groupe morte` — relevé dans
`docs/mesures/navigateur-2026-10-16.json`. **Les 8 sont toujours `MORTE`** :

| source | état | délai |
|---|---|---|
| BERSERKSCAN | 522 Connection timed out | 30 s |
| BLUELOCKSCAN | 523 Origin is unreachable | 17 s |
| HAIKYUU | 522 Connection timed out | 30 s |
| HELLSPARADISESCAN | 523 Origin is unreachable | 11 s |
| KINGDOMSCAN | 523 Origin is unreachable | 16 s |
| MYHEROACADEMIASCAN | 523 Origin is unreachable | 16 s |
| SCANBORUTO | 523 Origin is unreachable | 16 s |
| SCANJUJUTSUKAISEN | 522 Connection timed out | 30 s |

Toutes rendent `statuts=[403,200]` puis une page d'erreur d'environ 7 ko. Un
522/523 est l'**origine** qui ne répond plus derrière Cloudflare : le verdict ne
dépend donc **ni** de l'agent utilisateur **ni** du transport — c'est le serveur
qui est éteint, ce que le navigateur réel confirme.

La règle écrite dans ce plan (« une source toujours MORTE est retirée du
catalogue, avec la date du relevé dans le motif ») **n'a pas été appliquée** : la
désactivation se fait dans l'application, elle est visible par l'utilisateur, et
elle se réverse en un clic. Reste à décider, donc **pas fait**.

### Question humaine du 2026-10-16 — « une méthode Cloudflare à la place des sources custom »

Réponse mesurée : **non, pas pour hentaizone**, et l'ordre des couches l'explique.
Quatre clients, un seul résultat, sur `hentaizone.xyz/manga/` :

| client | réponse |
|---|---|
| `curl` sans agent utilisateur | `403`, 5 433 o, *Just a moment...* |
| `curl` avec agent navigateur | `403`, 5 668 o, *Just a moment...* |
| Edge réel, profil neuf, hors application | « Un instant… », 28 860 o, 0 lien |
| WebView2 du transport de l'application | « Un instant… », 28 881 o, 0 lien |

Le même octet : **la source custom, OkHttp, la webview et le navigateur complet
sont tous en aval du même refus.** Le juge est la réputation de l'adresse IP, et
la capture montre un widget Turnstile **en boucle sur « Vérification… »** — il ne
demande même pas de clic, il ne se résout pas. Donc ni « leFenêtre de
transport cachée », ni une éventuelle lecture par cookies déjà_TRACE : le
transport a un bocal de **495 cookies** (dont une clearance des sessions
antérieures) et se fait malgré tout refuser.

**Ce que vaut une méthode Cloudflare, sur toute la population.** Les 35 sources
que le moteur classe derrière Cloudflare, mesurées **depuis cette machine**
(`docs/mesures/cloudflare-depuis-cette-machine-2026-10-16.json`) :

| verdict | n | portée d'une méthode applicative |
|---|---|---|
| origine éteinte (521/522/523/525) | **18** | aucune : le serveur est mort |
| bloqué Cloudflare (403 + marqueur) | **14** | aucune : refus au niveau IP |
| 403 sans marqueur (protection du site) | 1 | aucune |
| 404 | 1 | aucune |
| **200 + 799 ko de catalogue** | **1** | **seule cible possible : SUSHISCAN** |

Donc une méthode Cloudflare viserait, au mieux, **une source sur 1384** — et
seulement parce qu'elle répond déjà en 200. Le reste est hors de portée du code.

**Ce qui reste à faire, et qui n'est pas du code.** La seule variable que je ne
peux pas tester d'ici est **l'adresse de sortie** : un partage de connexion
mobile ou un autre réseau dirait en une minute si hentaizone s'ouvre ailleurs. Si
oui, le blocage est la réputation de l'IP d'ici, et l'application n'a rien à
changer. Si non aussi, il ne reste que le clic humain dans la fenêtre de
vérification — celle que l'application propose déjà.

**Prochaine piste, la seule vivante : SUSHISCAN.** Elle sert 799 ko en `200`
aujourd'hui. Elle est connue pour deux défauts mesurés le 2026-10-14 : elle ignore
la pagination (`?page=2` rend la page 1 à l'identique) et son catalogue est sous
`/wp-content/uploads/` où la première image est le logo du site. C'est un travail
de parseur, pas de Cloudflare — donc mesurable et réversible.

**Audit livré le 2026-10-15 (lecture seule, aucun code produit touché).**
Rapport : `docs/audit-ux-2026-10-15.md`. Outils : `.mesures/ux-audit/`
(`inventaire.mjs`, `parcours.cjs`, 14 captures, `parcours.json`).

Findings retenus, chacun avec sa preuve :
- **F1** clé React manquante : la bande du lecteur (`page.tsx:5679`) retourne un
  fragment `<>` **sans key** (L5730), le `key={p.token}` etant pose sur l'`<img>`
  interne — 1 erreur console mesuree a l'ouverture du lecteur.
- **F2** contraste : « Voir le chapitre signale », accent #366ad3 sur #1a1a1a =
  **3,45:1** a 12,5 px (seuil AA 4,5). Seul echec de contraste de la passe.
- **F3** source vide muette : BENTOSCAN rend 0 manga **sans erreur ni message**
  (mesure : mangas=0, erreur=null ; moteur repondu en ~2 s).
- **F4** deux champs de recherche cohabitent sur le catalogue (filtre de sources
  + recherche globale de `e316a4d`).
- **F6** 40–100 cles dupliquees sur la fiche « One Piece » : `key={c.url}`
  (L6051) alors que SCANVF rend des URL de chapitres en double.
- **F13** fenetres de verification Cloudflare qui s'ouvrent et restent (declencheur
  non identifie au demarrage — le premier tick du planificateur est immediat).
- **Positifs mesures** : lecteur fonctionnel (image 720×4096, compteur 1/17,
  scroll 0->85, Echap sort), 0 anneau de focus manquant sur 12 arrets, filtre du
  catalogue + etat vide corrects, bibliotheque (7 webtoons, 86 Mo) et reglages
  complets.

Pieges d'instrument ecartes et divulgues dans le rapport (« Harness ») : le
bulbe rouge « N Issues × » est injecte par le navigateur (absent du code, son
compte suit les erreurs console) ; le binaire debug exige `next dev`
(`devUrl` dans `tauri.conf.json`). Erreurs de mes propres sondes corrigees puis
rejouees (selecteur `.manga` = squelette, `input.search` = recherche globale,
`pages[0]` = page Cloudflare, `scrollTop` sur `.reader` au lieu de `.webtoon`,
compte d'`<img>` sans alt gonfle par des commentaires).
   humaine). Les corrections ne commenquent qu'après validation.

### RÉSULTAT du 2026-10-16 à 12 h 15 — **HENTAIZONE devient lisible**

La source qui a résisté toute la journée passe, et l'utilisateur charge des
webtoons. **Aucun code n'a été écrit pour cela** : le binaire est celui du dépôt,
sans le drapeau CDP, et **aucune fenêtre de vérification n'a été ouverte** dans
cette instance.

| mesure | valeur |
|---|---|
| lectures de transport réussies | **53** |
| fenêtres de vérification ouvertes | **0** |
| catalogue | `latest source=HENTAIZONE offset=96` — pagination qui marche |
| chapitres | 149 177 à 184 330 octets |
| pages | 169 166 octets |
| ce que la webview lit | 158 757 à 169 899 octets — **du contenu**, plus l'interstitielle de 28 7xx |
| requêtes OkHttp brutes sur le même hôte | **60 × `403`** et **35 × `200`**, bocal à cookies **vide** |

**Le fil, minute par minute.** Instance précédente (11:04 → 11:07) : `latest` →
`CHALLENGE_REQUIRED`, transport `challenge non résolu en 60 s`, puis ouverture de
la fenêtre de vérification par l'utilisateur, 8 cookies lus et appliqués avec
l'agent de la fenêtre, et le rejeu répond **`CHALLENGE_UNREPLAYABLE`**. Instance
suivante (11:12 → 11:15, celle-ci) : `403 cf-mitigated=challenge` par OkHttp,
puis **la webview rend 161 328 octets**, le moteur injecte la page
(`webview html injecte … totalMemoire=1`), la requête suivante répond `200`, et
53 lectures plus tard l'interface affiche les webtoons.

**Deux lectures qui ne se contredisent pas.**
1. **La webview de transport est devenue la voie qui marche.** Elle rend la page
   réelle, le moteur l'injecte, l'interface affiche. 53 fois, pas un accident.
2. **Le chemin OkHttp du moteur, lui, échoue toujours** — 60 `403` contre 35
   `200`, bocal vide. La clearance n'est **pas** honorée pour les requêtes
   directes : le transport n'est pas un filet de sécurité, il est **porteur**.
   Toute source qui l'exige dépend de lui entièrement.

**Ce qui n'est pas attribué, et qui ne doit pas l'être.** Le journal ne dit pas
*pourquoi* la webview est passée de 28 ko à 161 ko. L'hypothèse la plus solide
est un **profil WebView2 partagé réchauffé** par la résolution manuelle de
l'instance précédente — c'est-à-dire l'idée de la référence (« recréer en
conservant le profil du navigateur », `MAX_AUTO_RECREATE_COUNT`), atteinte ici
par accident. **Le test qui tranche tient en un redémarrage** : relancer
l'application et regarder si la webview relit 161 ko ou retombe à 28 ko. Tant
qu'il n'est pas fait, on ne sait pas si ce que nous avons est **durable** ou
**emprunté** — et c'est la question à trancher avant d'écrire quoi que ce soit.

Détail d'outillage : la fenêtre de transport, ouverte puis réutilisée, et le
journal `challenge-debug` (noms de domaine, jamais les valeurs de cookies) sont
ce qui a permis de voir la bascule. Le second est activé par
`TATAMI_CHALLENGE_DEBUG=1`.

### Expérience A du 2026-10-16 — la fenêtre de transport « attachée » : expérience **invalide**

> **Tableau de synthèse : `docs/cloudflare-options.md`** — les 22 pistes
> envisagées depuis le début, avec pour chacune la mesure qui la tranche et son
> verdict. À lire avant de proposer une nouvelle piste : la moitié y est déjà
> réfutée par une mesure.

Question posée après la comparaison avec l'implémentation de référence
(`Kotatsu-Redo/app/src/main/kotlin/org/koitharu/kotatsu/`) : notre fenêtre de
transport est créée `.visible(false)`, donc **sans surface**, alors que la
référence écrit que « Turnstile treats a non-focused or detached WebView
differently » (`CloudFlareActivity.kt:84`) et attend `hasWindowFocus()` avant
de charger la cible (`:211`). Est-ce la cause ?

**Comparaison faite avant toute écriture.** Sur l'identité du client, Tatami est
**en avance** : `challenge.rs` lit `navigator.userAgent` **et** les Client Hints
haute entropie, et les renvoie au moteur, qui reconstruit `sec-ch-ua*` et
`Sec-Fetch-*` (`ChallengeHeaders.kt`). La référence ne lit que l'UA et se
contente de *bloquer* trois en-têtes (`CloudFlareInterceptClient.kt:30`). La
leçon de la référence — « une clearance n'est honorée que pour l'agent exact qui
l'a obtenue » — est donc déjà appliquée chez nous.

Trois écarts réels, dont un seul se prête à une expérience :
1. `.visible(false)` (`challenge.rs:982`) ;
2. navigation immédiate, sans attendre viewport ni focus (`:991`) ;
3. `reste_interstitiel` (`:106`) ne teste ni la géométrie des nœuds de défi
   (comme `CloudFlareDetection.kt`) ni `readyState`.

**Une tâche retirée du périmètre par la mesure.** Le détecteur façon référence
(écart 3) ne sert à rien ici : l'interstitielle de hentaizone **n'a aucun
widget** dans le DOM — `{"div":2,"script":1}`, **0 iframe**, `readyState:
complete`, titre encore « Un instant… », `script` du challenge en `display:none`
0×0 (`.mesures/session-2026-10-16/17-dom-interstitielle.json`). Il n'y a pas de
« widget qui tourne » à détecter, et le test par titre dit déjà la bonne chose.

**Ce qui a été essayé, et pourquoi le résultat ne prouve rien.** Fenêtre de
transport créée réelle, 1024×768, `.focused(true)`, sans décoration ni barre des
tâches, positionnée hors écran (−20000, −20000), plus une attente de focus
bornée à 1,5 s **journalisée**. `fmt=0`, `clippy=0`, `cargo test` 262/0 avant et
après. Mesuré sur TOONFR et sur hentaizone :

| relevé | valeur |
|---|---|
| viewport de la fenêtre hors écran | `1024x768` — **viewport réelle** |
| `fenetre.is_focused()` (Rust, Win32) | `focus=false`, **3 fois sur 3** |
| `document.hasFocus()` (page WebView2) | `focus=true`, 26 relevés sur 26 |
| défi sur 76 s | interstitielle figée, 0 lien manga |

Les deux instruments **se contredisent**, et c'est Rust qui a raison : Windows
**refuse d'activer une fenêtre entièrement hors écran**, donc `.focused(true)` ne
demande rien ; `document.hasFocus()` suit le focus du contrôle enfant WebView2 et
peut être périmé. L'expérience a donc testé « attachée, hors écran, jamais au
premier plan », **pas** la recette de la référence, qui exige le focus.
**L'inférence « l'attachement n'aide pas » est donc sans fondement** et n'est
pas écrite.

**Deux faits réutilisables, eux, sont prouvés :**
- une fenêtre Tauri positionnée hors écran **reste attachée et garde une vraie
  viewport** (1024×768) — la technique marche, seule l'activation échoue ;
- **aujourd'hui, aucune source ne passe par le transport avec succès** : les 35
  sources derrière Cloudflare sont bloquées ou mortes, donc la case « site
  résoluble » de la matrice est vide. TOONFR, résolu seul en ~10 s le
  2026-10-13, ne se résout plus.

**Une conclusion de la même séance à corriger.** Une similarité de taille
(28 722 o sur TOONFR, 28 783 o sur hentaizone) avait été lue comme « les deux
sites servent la même page ». Faux : la liste des cibles CDP montre sur TOONFR
un **iframe Turnstile vivant** (`challenges.cloudflare.com/.../turnstile/…` et
deux `blob:`), là où hentaizone n'en a aucun. Même gabarit Cloudflare, états
opposés.

**Changement retiré de l'arbre.** Un comportement sans bénéfice mesuré, qui
coûte 1,5 s par requête de transport et vole le focus à l'utilisateur, n'a rien
à faire dans le code. `challenge.rs` remis à l'état commité.

**Décision humaine requise.** La recette ne peut être éprouvée qu'avec une
fenêtre **réellement visible et au premier plan** — précisément ce qui a été
refusé le 2026-10-14 (« ne rien changer, l'utilisateur n'a rien à décider
ici »). Trois voies, aucune ne découle d'une mesure :
1. fenêtre de transport visible pendant le challenge, puis fermée (30 à 60 s
   d'intrusion, une seule fois par source défaillante) ;
2. fenêtre translucide à 1 % d'opacité, comme le `HIDDEN_RENDER_ALPHA` de la
   référence — mais Tauri v2 n'expose pas l'opacité par fenêtre, il faudrait
   passer par l'API Win32 ;
3. ne rien tester, et garder l'écart comme dette documentée.

Reste que, pour hentaizone, aucune de ces voies ne changera quoi que ce soit :
quatre clients (curl, Edge hors application, WebView2, source custom) rendent la
même page, et le test qui déciderait vraiment — **un autre réseau** — ne peut
pas être mené d'ici.

#### Cause la plus probable, mesurée le 2026-10-16 : **l'adresse de sortie**

| | valeur |
|---|---|
| IP publique | `154.72.113.241` |
| opérateur / ASN | **AS36924 GVA Cote d'Ivoire SAS** (« GVA Benin ») |
| géolocalisation | Cotonou, Bénin (`BJ`) |
| fuseau horaire de la machine | `W. Central Africa Standard Time` |
| proxy système / variables d'env | **aucun** (`ProxyEnable=0`) |

La machine est donc **elle-même** en Bénin, sur un réseau d'hébergement et non
sur une connexion résidentielle. Ce profil est celui que Cloudflare classe à
risque élevé.

Cela explique d'un seul geste la majorité des mesures du jour : les 18 sources
en `523` et les 14 en `403 + marqueur`
(`.mesures/cloudflare-depuis-cette-machine-2026-10-16.json`), le widget
Turnstile qui tourne en boucle sur les quatre clients, et le fait que les
sources **sans** protection agressive passent.

**Correction du 2026-10-16, mesurée le jour même.** J'ai d'abord écrit « tout
Cloudflare nous défie depuis cette adresse ». C'est faux, et la preuve est dans
les en-têtes de réponse : **MANGASORIGINES est elle aussi derrière Cloudflare**,
servie par la **même bordure** (`cf-ray` terminé par `JNB`, Johannesburg), depuis
la même IP — et elle ne sert **aucun défi** (`cf-mitigated` absent, contre
`cf-mitigated: challenge` sur hentaizone). La formulation exacte est donc :

> les zones **agressives** nous défient ; les zones **permissives** nous laissent
> passer. Ce n'est pas Cloudflare qui nous bloque, c'est la configuration de
> chaque site.

**Et notre client n'est pas en cause — mesure complémentaire.** Un `curl` naïf
reçoit de `mangas-origines.fr` un **403 sans défi**, alors que notre moteur lit
la source sans difficulté (`latest` 16, `details` OK, `chapters` 174). Donc le
client Tatami passe là où une requête naïve est refusée : l'axe « navigateur
furtif / empreinte » n'a rien à corriger, il est déjà bon.

Le domaine exact de la source, relu chez le producteur
(`sonde-sources.mjs --out`) : **`mangas-origines.fr`**. Trois domaines avaient
été devinés à partir du nom de la source avant d'être lus
(`mangasorigines.fr`, `www.mangasorigines.fr`, `mangasoriginesunofficial.com` :
aucun n'est le bon).

**Ce qui reste à faire, et qui n'est pas du code.** Le test qui tranche, d'une
minute : un partage de connexion mobile (ou tout autre réseau résidentiel), puis
`node scripts/verifier-egress.mjs`. Cette sonde sort un **verdict** et un **code
de sortie** (`0` lisible, `2` contexte, `3` indeterminé) :

```
sortie  : 154.72.113.241  AS36924 GVA Cote d'Ivoire SAS   (Cotonou, BJ)
DEFI_CLOUDFLARE  http=403  5 710 o  hentaizone.xyz/manga/
DEFI_CLOUDFLARE  http=403  5 605 o  toonfr.com/webtoon/
DEFI_CLOUDFLARE  http=403  5 604 o  astral-manga.fr/manga/
DEFI_CLOUDFLARE  http=403  5 647 o  manga-mana.com/liste-mangas
DEFI_CLOUDFLARE  http=403  5 600 o  onisaga.com/browse
VERDICT : ce chemin reseau est contexte. Changer de reseau, pas de code.   (code 2)
```

Les URL sont reprises **telles quelles** du relevé
`cloudflare-depuis-cette-machine-2026-10-16.json`. Une première version de la
sonde annonçait `INJOIGNABLE` sur un site que l'application lit très bien : elle
utilisait le résolveur du système alors que le moteur résout en **DNS-over-HTTPS**
(`DoHManager`). `mangasoriginesunofficial.com` ne résout pas ici et le moteur le
lit (`OK`) — donc, pour la **joignabilité**, l'instrument de référence reste
`scripts/sonde-sources.mjs`, qui interroge notre propre pile.

Si le site s'ouvre sur un autre réseau, **Tatami n'a rien à changer** et la
totalité des 35 sources redevient accessible sans une ligne de code.

**Ce que l'on écarte, avec sa raison.** Une impasse par usurpation
d'empreinte (TLS/JA3, `navigator.webdriver`, clients HTTP modifiés) est déjà
**au maximum** chez nous : le transport est un vrai Chromium, et l'interface
reconstitue `sec-ch-ua*` et `Sec-Fetch-*`. La mesure le confirme par
l'échec : curl et un Edge réel rendent **la même page**, donc l'axe
«Navigation furtive » ne peut rien expliquer ici. Découvrir l'IP d'origine,
usurper un en-tête d'Origin Pull, ou router le trafic par des adresses
résidentielles tierces ne sont pas des améliorations de Tatami : ce sont des
contournements délibérés des accès d'un tiers, et je ne les intègre pas.

### Journal 2026-10-16 — Niadd FR récupéré en source personnalisée

**Contexte.** Le tri des 10 `PARSER_OUTDATED` du relevé FR (commit `92d05d8`) a
montré **8 domaines morts** (4 revendus à un parking `clickroutebeacon.com`, 1 à
`asiafilm.org`, `fr-scan.com` 11 o, `scan-trad.com` injoignable, `scanvf.org`
472 o, `hentai.scantrad-vf.cc` page d'atterrissage) et **3 récupérables** :
`fr.niadd.com`, `hentai-scantrad.org`, `manga-scantrad.io` (Cloudflare). Niadd FR
d'abord : c'est la seule des trois **sans** Cloudflare, donc la seule dont la
récupération ne dépend pas du transport.

**Ce qui a été fait.** `docs/source-niadd-fr.js` (source personnalisée, ADR 0010)
— jamais un fork de `kotatsu-parsers-redo`, interdit par AGENTS.md §13. Chaque
forme du parseur est **relevée**, aucune devinée
(`.mesures/session-2026-10-16/25…44-niadd-*.cjs`).

**Preuves.**
- `node scripts/sonde-sources.mjs --sources custom:niadd-fr --deep` → **`ACTIF`** :
  60 séries, fiche OK, **105 chapitres**, **21 pages**, image `image/webp`.
- Parcours dans l'app (`.mesures/session-2026-10-16/45-niadd-parcours.cjs`) :
  **60 séries**, **56/60 couvertures décodées**, fiche de **59 chapitres triés
  Ch.1 → Ch.59**, lecteur avec la page 1 décodée.
- `pnpm moteur:check` : 14 sources personnalisées évaluables par Nashorn.

**Trois pièges du site, mesurés.**
1. Les couvertures vivent sous `/fr_manga/logo/…` — un dossier nommé `logo` qui
   contient les couvertures, pas la bannière. Elles sont servies par le **même
   hôte** que les pages de chapitre (`/pic5/`), donc il faut les **exclure** côté
   pages, sinon la couverture devient la première page.
2. Chaque page de chapitre **répète 6 fois** l'image courante et annonce la
   suivante. Le dédoublonnage se fait donc par **chemin nu** : le jeton `?acc=`
   change d'une occurrence à l'autre, l'image non.
3. `?page=2` renvoie **la même page** (304 730 o identiques) sur le catalogue,
   `chapters.html` et la recherche. Le parseur rend `[]` au-delà de la page 1 —
   c'est le défaut « page 2 = page 1 » déjà mesuré sur sushiscan le 2026-10-14.

**Deux leçons de mesure, payées dans la séance.**
- **Trois paramètres de recherche devinés rendent la même coquille.** `?q=`,
  `?keyword=` et `?page=` renvoyaient tous **31 059 o** avec « 0 série » : un
  paramètre inconnu est ignoré en silence, donc « 0 série » ne voulait rien dire.
  Le champ du formulaire s'appelle `name` (`<input name="name" class="query">`) —
  lu, il rend 18 séries. **Un « 0 résultat » identique pour trois entrées
  différentes n'est pas une mesure, c'est un paramètre ignoré.**
- **Le squelette porte les mêmes classes que le contenu.** `page.tsx:6410` rend
  un squelette `.manga-grid` > `.manga` > `.manga-open`, et `page.tsx:6069` un
  `.chapter-open` — donc `waitForSelector('.manga-grid .manga')` réussit sur des
  cartes **vides**. Première mesure : « 6 séries sans titre, 0 chapitre ». La
  vraie : 60 séries, 59 chapitres. On attend désormais un **contenu** (libellé non
  vide + disparition de `aria-busy`), pas un sélecteur. Même famille que « une
  mesure qui renvoie 0 élément ne prouve rien », mais par l'autre bout : ici elle
  renvoyait **6** éléments et ne prouvait pas davantage.

**Reste à faire.** `hentai-scantrad.org` (200, sans Cloudflare) puis
`manga-scantrad.io` (Cloudflare → transport). Enfin, apprendre à
`scripts/sonde-sources.mjs` le verdict « domaine mort / parké » : aujourd'hui il
envoie écrire un parseur pour un cadavre.

### Journal 2026-10-17 — Hentai-Scantrad récupérée, et le harnais qui ne voit pas Nashorn

**Contexte.** Deuxième des trois sources récupérables du tri des
`PARSER_OUTDATED` (après `fr.niadd.com`, commit `c535b97`). C'est la source
native `SCANTRAD` : son entrée de `contract/sources-etat.json` portait déjà
« natif rompu : `scan-trad.com` mort … Contourne par la source scriptee
`docs/source-hentai-scantrad.js` ». La version en place datait du 2026-10-13 et
reposait sur **deux lectures fausses** : « la recherche n'existe pas » (elle
existe : `/?s=<q>&post_type=wp-manga&paged=<n>`, 10 résultats) et une pagination
sur l'archive `/manga/page/<n>/`.

**Ce qui a été fait.** `docs/source-hentai-scantrad.js` réécrite depuis les formes
**relevées** les 2026-10-16 et 17 (`.mesures/session-2026-10-16/50…68-scantrad-*`).
Jamais un fork de `kotatsu-parsers-redo` (AGENTS.md §13).

**Preuves.**
- `node scripts/sonde-sources.mjs --sources custom:hentai-scantrad --deep` →
  **`ACTIF`** : 24 séries, fiche OK, 12 chapitres, 49 pages, image `image/webp`
  (`docs/mesures/sources-hentai-scantrad-2026-10-16.json`).
- Harnais hors ligne (`.mesures/session-2026-10-16/66-source-harnais.cjs`, 7
  étapes) : 24 cartes au catalogue et 10 en recherche, **pagination réelle**
  (0 recouvrement entre les pages), fiche « Secret Class » avec **316 chapitres**
  portant **les deux formes d'URL**, 26 pages sur `chapitre-316`, 22 sur `ch-1`.
- `pnpm moteur:check` : 14 sources personnalisées évaluables par Nashorn.

**Six pièges du site, mesurés.**
1. `manga_get_chapters`, l'action Ajax « évidente », **n'existe pas** (`400`,
   corps de 1 octet « 0 »). Le vrai chargeur est lu dans
   `madara-core/assets/js/script.js` : `POST <fiche>ajax/chapters/`.
2. **Deux formes d'URL de chapitre** : `chapitre-N/` (≥ 83) et `ch-N/` (≤ 82). Un
   motif qui exige `chapitre-` voit 235 chapitres sur 316 — nombre plausible, donc
   erreur invisible.
3. `data-src=" https://…"` : l'URL est précédée d'une espace (déjà vu sur
   hhentai-fr et bananascans). Sans `trim()`, le chapitre rend 0 page.
4. `&page=<n>` est **ignoré** par la recherche (il rend la page 1 à l'identique) :
   c'est `&paged=<n>` qui pagine.
5. `og:image` est une jpg de **2020**, la couverture affichée une webp de **2026**.
6. Un groupe de capture dans un `(?=…)` (voir ci-dessous).

**Deux corrections que la mesure a imposées, pas le goût.**
- **`http://` → `https://` sur les images.** Les chapitres ≤ 82 servent leurs
  pages en `http://stockage.hentai-scantrad.org/…`, et le moteur **refuse** ce
  schéma (`validerUrlImage` : « seul https:// est accepte ») : sans réécriture,
  ces chapitres rendraient 0 page. Le même chemin en `https` a été mesuré :
  `200, 121 866 octets, image/jpeg` — les mêmes octets.
- **La liste.** Accueil `/page/<n>/` : 24 cartes × 35 pages ; archive
  `/manga/page/<n>/` : 10 cartes × 83 pages, et sa page 1 est un **sous-ensemble**
  de l'accueil. Même catalogue, donc l'accueil gagne (2,4× moins de requêtes) —
  et c'est la sémantique « dernières mises à jour » de la source de référence
  Niadd FR.

**La leçon qui a coûté le plus : le harnais (V8) a validé ce que le moteur
(Nashorn) refusait.** La première version extrayait titre et URL d'un seul motif
à pré-vérification `(?=…)`. Le harnais hors ligne rendait **24 cartes vertes** ;
la sonde profonde refusait net — `ScriptedSourceException: TypeError: Cannot read
property "indexOf" from undefined`, ligne 108. Un groupe de capture **dans un
lookahead** est rempli par V8 et laissé `undefined` par Nashorn. `pnpm
moteur:check` ne garde que la virgule finale d'appel, donc il était vert aussi.
Corollaire : le harnais est un **filtre de logique**, la sonde profonde est le
**juge** ; un harnais vert ne dispense pas de la sonde. C'est la deuxième fois que
ce dépôt paie « le moteur est Nashorn, le test est V8 » — la première (virgule
finale) est déjà en `AGENTS.md §14`, qui porte maintenant les deux déclencheurs.

**Reste à faire.** `manga-scantrad.io` (Cloudflare → dépend du transport), puis
apprendre à `scripts/sonde-sources.mjs` le verdict « domaine mort / parké ».

### Journal 2026-10-17 — la sonde dit *pourquoi* un `PARSER_OUTDATED` l'est

**Contexte.** Le « Reste à faire » de l'entrée précédente. Un `PARSER_OUTDATED`
dit le **fait** (« la page n'a pas répondu au parseur ») et pas la **cause** — et
les deux causes opposées se ressemblent : un domaine racheté répond `200` avec
une page de parking, un site vivant a simplement redessiné ses pages. Lire le
verdict comme « réparable » avait donc envoyé écrire un parseur pour un cadavre.

**Le tri des 10 `PARSER_OUTDATED`, mesuré** (`curl` le 16, `fetch` Node le 17) :

| source | hôte final | titre | octets | ce que c'est |
|---|---|---|---|---|
| ANTEIKUSCAN | `clladss.com` | — | 9 043 | détourné |
| ASTRAMES | `…squitoiages.com` | — | 199 | détourné |
| FRSCAN | `ww80.fr-scan.com` | `fr-scan.com` | 39 688 | parking |
| HENTAISCANTRADVF | (hôte de la source) | `scantrad-vf.cc` | 39 310 | parking |
| KAIJUNO8 | `asiafilm.org` | — | 6 742 | détourné |
| MANGASORIGINESUNO | `clladss.com` | — | 69 061 | détourné |
| SCANTRADVF | `clladss.com` | — | 9 066 | détourné |
| SCANVFORG | `scanvf.org` | `Loading...` | 477 | interstitiel |
| **NINEMANGA_FR** | `fr.niadd.com` | `Répertoire …` | 325 619 | **site vivant** |
| **SCANTRAD** | `hentai-scantrad.org` | `Manga Archive …` | 170 646 | **site vivant** |

**Ce qui a été fait.** `scripts/sonde-sources.mjs` : sur `PARSER_OUTDATED`, la
sonde relit l'`urlFinale` que le moteur a servie et en retient l'hôte, le titre et
la taille (`mesurerIdentite`) ; une règle **pure**, documentée avec ses seuils
(`classeIdentite`), choisit l'état — redirection hors du domaine déclaré, titre
égal au domaine nu, ou page de moins d'1 ko → `hors_service` ; site vivant →
`parseur_perime` avec un motif qui dit « recuperable ». Et `--sources` garde
désormais le **domaine déclaré** de chaque source (il rendait `domaines: []`,
donc la règle ne pouvait pas mordre là où elle sert).

**Preuves.**
- `node scripts/sonde-sources.mjs --sources ANTEIKUSCAN,…,SCANVFORG --classer` →
  **7 `hors_service`** motivés, **2 `parseur_perime` « site vivant … donc la
  source est recuperable »** (NINEMANGA_FR et SCANTRAD, les deux récupérables du
  tri) et **1 non mesuré, dit comme tel** (HENTAISCANTRADVF).
- Régression : `--sources custom:hentai-scantrad` → `OK*`, inchangé ; `--depuis`
  et `--classer` inchangés ; `node --check` vert.

**Deux faits mesurés au passage, chacun changeant une décision.**
1. Le réseau de parking **tourne** : ASTRAMES rendait `clladss.com` le 16 et
   `…squitoiages.com` le 17, MANGASORIGINESUNO `filter.explorads.com` puis
   `clladss.com`. D'où la règle, qui compare au **domaine déclaré** et non à une
   liste de parkings connus : une telle liste serait périmée le lendemain.
2. Node reçoit `ECONNRESET` **avant** la poignée de main TLS sur
   `hentai.scantrad-vf.cc` pendant que `curl.exe` lit la même page (39 312
   octets) : c'est l'empreinte TLS de la pile Node qui est refusée, pas un site
   éteint. La mesure d'identité est donc **best-effort**, et son échec est écrit
   dans le motif au lieu d'être muet. Leçon ajoutée à `AGENTS.md §14`, qui portait
   déjà le DNS-over-HTTPS.

**`manga-scantrad.io`, mesuré.** `403`, `Cf-Mitigated: challenge`,
`Server: cloudflare`, `Turnstile` sur `/` et `/manga/` : l'étiquette
« Cloudflare » du plan n'était qu'une note humaine (`docs/mesures/notes.md`) —
elle est maintenant fondée. Cette source relève du transport, qui est gelé, et
non du parseur.

**L'état de catalogue est corrigé, à la main.** `contract/sources-etat.json`
portait `parseur_perime` / « le site a change de page » sur les **dix** : il
promettait huit réparations qui n'existent pas. Les huit cadavres sont passés à
`hors_service` avec le fait mesuré dans le motif (hôte détourné, titre = domaine
nu, interstitiel de 477 octets), et `NINEMANGA_FR` a gagné sa mesure (325 619
octets) et le rappel qu'elle est déjà récupérée. Le fichier compte désormais
**2** `parseur_perime` — `NINEMANGA_FR` et `SCANTRAD`, les deux récupérables — au
lieu de 10. Édition à la main et non par `--classer` : un balayage FR complet
écraserait les motifs rédigés des 88 autres entrées. Rien ne valide cet
énuméré côté outillage (aucun schéma, aucun lecteur) : c'est le `motif` qui
porte la distinction, et il est daté et mesuré.

**Ce qui reste.** Le pin des parseurs (`b3c0000` → `master`) reste à décider
(décision de dépendance : régénération de `verification-metadata.xml` et re-sonde
des 98 sources FR). Et les **35** sources FR définitivement mortes
(18 `ORIGIN_DOWN` + 17 `SOURCE_DEAD`) restent telles quelles.

### Journal 2026-10-17 — le pin des parseurs passe à `b4df40e9`, et une source FR revient

**Décision.** Le pin monte de `b3c0000` (2026-09-17, tête du 17 septembre) à
`b4df40e9` (2026-10-16), soit 97 commits d'amont. La tête `dc223c3d` a été
écartée : JitPack a échoué à la construire (`429 Too Many Requests` sur
`repo.maven.apache.org` pendant la résolution du *buildscript*) et **sert cet
échec en cache** — 24 relances n'ont rien redéclenché, l'API rendait encore
l'heure de la première tentative. Un commit jamais demandé se construit tout de
suite (`200` en 30 s). Le repli perd **un** commit (`Update KdtScans.kt`, non-FR).

**Le risque de compilation, mesuré et non espéré.** Sur les 16 entrées du paquet
`org.koitharu.kotatsu.parsers` que le moteur compile, **14 sont identiques** entre
l'ancien et le nouveau pin ; seuls `site/` (les 1200 parseurs) et `util/` bougent.
Le risque est donc dans les sources, pas dans l'API — et le build reste la porte.

**Preuves.**
- `gradle --write-verification-metadata sha256 --refresh-dependencies build` →
  vert (6m53) ; `verification-metadata.xml` gagne les 3 lignes de `b4df40e9`
  (jar `56566232…`, `.module` `2083782e…`). L'entrée `b3c0000` reste : Gradle
  n'élague pas, et la retirer serait un changement de politique sans bénéfice.
- `gradle clean build --refresh-dependencies` → vert (5m29) : c'est la résolution
  **à froid**, celle de la CI sur un checkout neuf.
- **La porte mord** : un **caractère** du sha256 altéré fait échouer la résolution
  en **nommant** `kotatsu-parsers-redo-b4df40e9.jar`
  (`com.github.Kotatsu-Redo:kotatsu-parsers-redo:b4df40e9`), en 4 s. Restauré par
  copie d'octets, `mtime` remis à jour : hash identique (`3F1CD1DE…`).
- `gradle test --rerun-tasks` → vert, **220 tests réellement exécutés**
  (41 XML frais, « 6 tasks executed ») — `FROM-CACHE` prouverait zéro.
- `scripts\build-engine.ps1` → `dist/` (68,9 Mo) embarque
  **`kotatsu-parsers-redo-b4df40e9.jar`** (9 396 631 o) ; l'ancien a disparu.
- `list-sources` → **1374** sources (1370 avant) ; `hello` →
  `parsersVersion: b4df40e9`, `protocolVersion: 0.2`.

**La mesure ciblée : une source FR revient, deux ne bougent pas.**

| source | 2026-10-16 | 2026-10-17 (pin `b4df40e9`) |
|---|---|---|
| RAIJINSCANS | `INCERTAIN`, « 200 OK sans resultat », domaine `raijin-scans.fr` | **`ACTIF`** — 24 items, 15 chapitres, 22 pages, **image webp décodée**, domaine **`aniverse.fr`** |
| FMTEAM | `INTERNAL`, JSONException `adult` | `INTERNAL`, **message identique** |
| HNISCANTRAD | `INTERNAL`, JSONException `adult` | `INTERNAL`, **message identique** |

RAIJINSCANS revient parce que l'amont a **suivi la migration du domaine** ; le
moteur la déclare désormais `int` (elle était `fr`). C'est le gain mesuré du bump.
La prédiction de la session précédente (trois sources) n'en tenait donc **qu'une**.

**À VÉRIFIER — le `adult` de FMTEAM et HNISCANTRAD.** Les deux échouent sur
`JSONException: JSONObject["adult"] is not a string (class java.lang.Integer : 0)`
— **message identique, même type de valeur, deux sites différents**, et
**2 sources sur 98** portent ce message. Vérifié : le moteur n'écrit **aucun**
champ `adult` (les 11 occurrences de `engine/src` sont de la prose). Ce n'est donc
pas notre JSON. Reste à trancher, par la lecture et non par l'intuition :
« le site renvoie `adult` en entier » ou « le parseur amont le lit en `String` ».
Le discriminera : lire le parseur amont de ces deux sources (elles sont toutes
deux bâties sur le même gabarit) et regarder **qui écrit `adult: 0`**.

**Le fichier d'état de catalogue, corrigé sur une seule entrée.** `RAIJINSCANS`
passe de `incertain` / « aucune reponse » (2026-10-12) à `actif`
(`etape: image`, `langue: int`) — sans quoi la prochaine session chercherait à
« récupérer » une source qui marche. Les 97 autres entrées ne sont pas touchées :
un balayage FR complet écraserait leurs motifs rédigés.

**Mensonges corrigés dans la doc, parce que le bump les a rendus faux.**
`README.md` (le compte attendu de `list-sources`, et « 1 370 sources embarquées »),
`docs/smoke-test.md` (l'attente devient un **seuil** — `N` > 1 000 — plus la valeur
mesurée, un compte qui suit l'amont ne doit pas être une porte qui accuse à tort),
et la fixture `01-hello.json`, **réenregistrée** depuis une sonde `hello` réelle
(`protocolVersion` 0.1 → 0.2, `parsersVersion` → `b4df40e9`) et non éditée à la
main. Laissés tels quels, parce que **datés** : `AGENTS.md §14`, `PLAN.md`,
`docs/spike-phase0.md`, `docs/notes/loader-context.md`, `docs/rapport-session-recherche.md`,
l'ADR 0005, et `moteur.rs` (manifeste **de test**, valeur arbitraire).

**Un chiffre périmé que je n'ai pas touché, et pourquoi.** `README.md` annonce
64,1 Mo pour `dist/` ; il en fait **68,9** aujourd'hui. L'écart n'est **pas** le
bump (+0,31 Mo sur le jar de parsers) : c'est **Conscrypt** (4,32 Mo, ajouté par
`381f4d5` le 2026-10-11), dont l'auteur n'a pas rafraîchi le chiffre. Hors
périmètre du bump, et signalé ici plutôt que corrigé en silence.

**Ce qui reste.** `.github/workflows/amont.yml` reste au rouge : il échoue dès que
le pin n'est plus la tête, et c'était **déjà** le cas avec `b3c0000` — aucune
régression, rien à corriger ici. Un **balayage FR complet** (~18 min en profond)
n'a pas été lancé : il mesurerait si le bump a réveillé des sources hors des trois
prédites, et 4 sources sont apparues au catalogue (1370 → 1374, langues inconnues).
Il écrirait un relevé **neuf et daté**, sans écraser `contract/sources-etat.json`.

### Journal 2026-10-17 — balayage FR profond : deux sources rendues par la montée, une par le site

**Le balayage.** 97 sources `fr` (le filtre `--lang fr` du moteur, pas le
catalogue), en profond, 2 moteurs en parallèle, ~18 minutes. Répartition :
18 `ORIGIN_DOWN`, 17 `SOURCE_DEAD`, 15 `CHALLENGE_REQUIRED`, 13 `INTERNAL`,
**13 `ACTIF`**, 11 `INCERTAIN`, 10 `PARSER_OUTDATED`. Relevé daté :
`docs/mesures/sources-fr-2026-10-17.json` (45 580 o, 97 entrées).

**Erreur d'abord : le drapeau s'appelait `--lang`, pas `--langue`.** Un drapeau
inconnu était **ignoré en silence** : le premier lancement a sondé **1374**
sources au lieu de 97, arrêté à la 195ᵉ après lecture de la ligne d'en-tête
(« 1374 source(s) a sonder », quand j'attendais « 97 … (langue fr) »). Coût
mesuré : 195 hôtes interrogés pour rien. La sonde refuse désormais tout drapeau
inconnu, et tout drapeau à valeur laissé sans valeur, en **nommant les valides**
(`41907c0`, prouvé sur ses quatre cas — y compris qu'elle ne bloque pas un appel
correct).

**Comparer un relevé profond à un relevé superficiel ne compare rien.** Celui du
2026-10-16 avait été fait **sans `--deep`** : il portait 14 `ACTIF_LIGHT` et
1 `HANG`. Douze des dix-neuf « changements de verdict » du diff ne sont donc que
la **profondeur ajoutée** — et ces entrées du catalogue portaient **déjà**
`actif` / `etape: image` (mesure 2026-10-12, balayage profond antérieur) : le
balayage du jour les **confirme**, il ne les change pas.

**Deux vraies récupérations, et l'attribution se lit dans le fichier amont.**

| source | 2026-10-16 | 2026-10-17 | cause mesurée |
|---|---|---|---|
| RIMUSCANS | `INCERTAIN`, « 200 OK sans resultat » | **`ACTIF`**, image WebP décodée | **la montée** : `RimuScans.kt` a été réécrit **dans** l'intervalle (`851835b0`, dont le parent **est** `b3c0000` ; `a0132e74`, 2026-10-15, « list via /api/series, complete chapter lists ») |
| SUSHISCAN | `ACTIF_LIGHT` (catalogue : `images_bloquees`) | **`ACTIF`**, image servie | **le site** : `SushiScan.kt` n'a plus bougé depuis 2026-07-12, antérieur à `b3c0000` — l'épingle n'y peut rien |

**Mécanisme du retag, confirmé dans le code.** `inferLang` (`Server.kt:186`)
appelle `SourceLocaleIndex.langOf` avec `className = ""` : la langue vient du
**dossier du JAR** (`site/<type>/<xx>/`), pas d'une heuristique de nom. L'amont a
déplacé RAIJINSCANS de `fr/` vers `int/`.

**Le recensement des langues tranche « langues inconnues ».**
`docs/mesures/sources-langues-2026-10-17.json` : **1374 natives** (1389 avec les
15 persos), **97 `fr`** ; le catalogue en compte 98, dont RAIJINSCANS (`int`) —
**le jeu FR du catalogue est donc complet**, et **aucune** des 4 sources apparues
au pin n'est française.

**Six entrées corrigées à la main, et pas une de plus.**
- `RIMUSCANS` `incertain` → `actif` ; `SUSHISCAN` `images_bloquees` → `actif`,
  chacune avec sa cause dans le motif (cf. tableau ci-dessus).
- `BLUESOLO` (`actif` → `hors_service`), `MANGAKAWAII` (`actif` → `hors_service`),
  `JAPSCANSFR` (`verification` → `hors_service`) : leurs messages sont
  **identiques aux deux relevés** (404 `www1.bluesolo.org/manga/`, 410
  `www.mangakawaii.io/manga-list/`, 404 `japscan.foo/manga`). Ce n'est donc pas
  un changement du jour mais une **contradiction préexistante** entre l'entrée et
  le chemin que le parseur suit : une source que l'application n'arrive pas à lire
  ne doit pas être marquée `actif`. Les motifs gardent la piste (domaine nu,
  extension `.to`).
- `MANGAHUB` : `etape: image` → `null`. L'entrée portait « chapitre sans aucune
  image » **et** `etape: image` ; la sonde ne pose son `etapeEchec` qu'à l'étape
  image, donc un chapitre sans image laisse `etape` nulle. Incohérence interne,
  pas mesure.

**Là où le reclassement automatique se tait, et c'est voulu.** Le catalogue
reclassé **désaccorde** avec huit entrées dont la mesure, elle, n'a pas bougé :
`HENTAISCANTRADVF` (notre pile TLS refusée depuis Node → « identité non
mesurable », ce que l'entrée avait écrit la veille) et
`ANTEIKUSCAN`/`ASTRAMES`/`KAIJUNO8`/`MANGASORIGINESUNOFFICIAL`/`SCANTRADVF`/
`FRSCAN`/`SCANVFORG`, où le motif humain nomme **l'hôte de parking** (« DETOURNE
vers clladss.com », 9 043 octets, aucun titre) là où l'outil dit « domaine
disparu » — le réseau de parking tourne d'un jour à l'autre. Le reclassement est
un **outil de tri**, pas un oracle : ces entrées gardent leur diagnostic.

**Portes du cœur, vertes après la montée.** `cargo fmt --check` 0 ;
`cargo clippy --all-targets -- -D warnings` 0 ; `cargo test` → **266 passés /
0 échec**, ce qui valide au passage la fixture `01-hello.json` réenregistrée.

**À FAIRE (mesuré, hors de cette intention).** `docs/source-rimuscan.js` a été
écrite pour contourner un natif cassé ; le natif marche depuis `b4df40e9` : la
source scriptée est **redondante** et demande soit son retrait, soit une mesure
qui la justifie encore.

### Journal 2026-10-17 — les « derniers chapitres » ne se calculent plus que sur les cartes visibles

**Le défaut, mesuré.** La marche de fond demandait sa ligne à **tout** le
catalogue chargé. Sur `custom:rimuscan` (232 cartes rendues d'un coup) :
**423 appels `chapters` en 13 min**, 223 séries distinctes, sans fin — et les
25 couvertures lancées à la même seconde passaient de 0,5 s (la 1ʳᵉ) à 15,3 s
puis **34,3 s** pour la 25ᵉ, quand le travail réel par image vaut ~1,3 s. Le
moteur n'a que **`MAX_WORKERS = 4`** travailleurs : la marche les occupait en
permanence, donc une requête demandée par l'utilisateur (ouvrir une série)
attendait derrière du travail que personne n'avait demandé.

**Le remède est une borne, pas un débit.** La marche — et la surveillance du
défilement — ne considèrent plus que les cartes **dans la fenêtre de lecture**.
La décision vit dans `lib/cartes-recentes.ts` (`cartesAFaire`, `entreesFraiches`),
sans DOM ni réseau, donc testable : **7 tests**. Le prédicat de fenêtre est écrit
**une seule fois** (`clesVisibles()` dans `page.tsx`), partagé par la marche et
par le réveil — deux copies finiraient par diverger, ce qui déplacerait la borne
sans bruit.

**Avant/après sur le même écran** (`custom:rimuscan`, 232 cartes, même instance,
`tatami.recent` vidé avant chaque relevé, code basculé par
`git stash push -- app/page.tsx` puis `pop` — empreinte SHA-256 comparée à une
copie d'octets) :

| mesure | avant (`1f2fe89`) | après |
|---|---|---|
| requêtes `chapters` à l'ouverture d'un catalogue de 232 cartes | **233** (232 séries) | **28** (28 séries) |
| durée de la rafale | **111,8 s** | ~20 s |
| après la rafale | la file continue | **silence** (102 s à 254 s selon la fenêtre de mesure) |

**Ce qui remplace « le reste de la liste » : un réveil.** La fenêtre est
surveillée par un tour d'horloge de 1 s qui compare la **signature de l'ensemble
visible** — et non chaque évènement de défilement, qui en émet des dizaines par
seconde et ferait rendre cet écran de ~4 000 lignes à chaque image. Quand des
cartes entrent ou sortent, `recentTick` relance la marche. Mesuré : défilement de
2 400 px → **43 appels**, **1 par série**, puis silence 102 s ; 47 des 49 cartes
visibles portaient leur ligne après la rafale.

**Le doublon, trouvé par la mesure et non par la lecture.** La première version
bornée demandait **2 fois** chaque carte (58 appels pour 28 séries, 26 séries à
2 appels) : `cartesAFaire` écarte ce qui est **déjà en vol**, mais la file
s'égrène sur plusieurs secondes (la politesse par source impose un écart), donc
une seconde marche démarrée entre-temps voyait les cartes non encore parties
comme « libres » et les redemandait. Réserve de la file **entière** au démarrage
(`for (const { key } of todo) recentInFlight.current.add(key)`), libérée par le
`finally` de chaque requête : mesuré ensuite **1 appel par série** (28/28, 43/43).
`attendreTourAutomatique` ne peut pas lever (elle n'appelle que `Date.now`,
`Map.get/set` et `setTimeout`), donc la réservation ne peut pas fuir.

**Deux détails assumés.**
- Le compteur `n/taille-du-catalogue` a été **retiré** : la marche ne visant plus
  toutes les cartes, ce rapport annonçait un progrès qui n'arrive jamais. Reste
  `recent.en.cours` quand des requêtes sont en vol.
- `react-hooks/set-state-in-effect` est apparue avec la réécriture (4
  avertissements → 1 erreur + 4). Elle est contournée par un **microtour**
  (`void Promise.resolve().then(…)`) ; la mécanique exacte de la règle n'a **pas**
  été établie, seul le fait est mesuré (HEAD 0 erreur, réécriture 1 erreur,
  microtour 0 erreur). C'est écrit dans le commentaire, pour que personne ne
  déduise de ce contournement que la forme d'avant était juste.

**Portes.** Interface, dossier `apps\desktop` : `typecheck` 0 ; `lint` **0 erreur**
+ les 4 avertissements `exhaustive-deps` connus ; `test` **147 passés / 0** ; les
douze `*:check` vertes (`i18n`, `hooks`, `chemins`, `pages`, `perso`, `ipc`,
`moteur`, `libelles`, `francais`, `styles`, `lecteur`, `arrondi`) ; `build` vert.
Changement **front-only** : ni le cœur ni le moteur ne sont concernés, donc
`cargo` et `gradle` ne sont pas requis par cette intention.

### Journal 2026-10-17 — « Niadd FR plante sur certaines covers » : quatre couvertures vides, et rien pour le dire

**Le défaut, mesuré.** `custom:niadd-fr` n'échouait sur aucune forme d'URL : la
sonde compte **60 couvertures sur 60 absolues**, un seul hôte (`img11.niadd.com`),
aucune espace parasite, aucun blocage — les 65 `<img>` de la page du site étant
par ailleurs munis d'un `onerror="this.src='/files/images/def_logo.svg'"`, le site
sait lui-même que certaines couvertures manquent. Interroger les 60 une par une
donne **4 fichiers de 0 octet** servis `200 image/jpeg`, `Content-Length: 0`,
`etag: "…-0"`, `Last-Modified: 2020-04-10`, `cf-cache-status: HIT`, `Age: 2584691`
— donc **vides à l'origine**, mis en cache ~30 j par Cloudflare ; le `Referer`
n'y change rien.

**La chaîne, lue dans le code et non devinée.** Le moteur refuse un corps vide
(`Server.kt:1309`, `IllegalStateException("empty $quoi")`, `quoi = "cover
body"`), ce qui remonte en `INTERNAL` ; le cœur répond **404** (`pagecache.rs`,
refus du corps vide) ; le front n'avait **aucun `onError`**, donc la carte
affichait une image cassée muette. Reproduit dans l'app, fenêtre bornée
(l. 11103 → 11261) : `WARN supervisor … id=fetchCover-… code=INTERNAL
msg=IllegalStateException: empty cover body len=129` ×4, `tatami serve error:
INTERNAL: … empty cover body` ×4.

**Le remède est un repli, pas une correction de source.** Il n'y a rien à
télécharger : ces fichiers sont vides chez la source et le resteront. Sept
emplacements rendaient `<img src={coverSrc(...)}>` **sans repli** (grille du
catalogue, fiche, bibliothèque, historique, signets, favoris de l'accueil,
recherche) alors que le repli existait déjà pour un **jeton absent**
(`thumb-empty` + initiale). Composant local `Vignette` (page.tsx) : un `onError`
affiche la même pastille, et l'état retient **quel jeton** a échoué — un booléen
resterait collé quand la vignette sert une autre série. Mesure dans l'app après
rechargement : **60 cartes, 56 images, 4 replis, 0 image cassée**, et les 4 replis
sont exactement les 4 séries mesurées vides hors application (*Lost In
Translation*, *Ventham*, *The Wolf That Picked Something Up*, *I Met The Male
Lead In Prison*), `#error-toast` = 0.

**Un défaut de mise en page découvert par la capture.** Le repli **n'était pas
centré** dans la grille : jaugé dans l'app, la boîte est `display: block`
(`.manga .thumb` impose `block`, spécificité 0,1,1 > `.thumb-empty` 0,1,0, donc
les propriétés flex ne s'appliquaient pas) et l'initiale tombait à
`ecartGauche = 0`, `ecartHaut = 5` — un glyphe errant plutôt qu'une pastille.
Règle `.manga .thumb-empty` (même remède que `.bm-cover .thumb-empty`, déjà
présent) : mesuré ensuite `display: flex`, `ecartGauche = 76` sur 169 et
`ecartHaut = 106` sur 254, soit les deux moitiés exactes. **Aucun token nouveau**
ni couleur, ni police, ni rayon touchés : la règle ne fait que répéter le
centrage que `.thumb-empty` porte déjà.

**Porte élargie plutôt que porte nouvelle.** `pnpm pages:check`
(`scripts/check-pages-indisponibles.mjs`) est la porte « une image refusée par la
source doit le dire » : elle lisait le `<img>` du lecteur et **excluait
explicitement les vignettes**, ce qui était précisément le trou. Elle vérifie
maintenant aussi qu'il n'y a **qu'un seul** `<img>` de couverture dans le fichier
(toute vue doit passer par le composant), qu'il porte un `onError` et qu'un
`thumb-empty` existe. Les deux mors ont été prouvés **rouges** : `onError` retiré
→ la porte nomme la vignette ; vignette brute rajoutée à un site → la porte
nomme « 2 `<img>` de couverture au lieu d'un seul ».

**Portes.** Interface, dossier `apps\desktop` : `typecheck` 0 ; `lint` **0
erreur** + les 4 avertissements connus ; `test` **147 passés / 0** ; les
`*:check` vertes (`i18n`, `hooks`, `chemins`, `pages`, `perso`, `ipc`, `moteur`) ;
`build` vert. Changement **front-only** : ni le cœur ni le moteur ne bougent.
Laissé de côté, faute d'accord humain : mapper `INTERNAL` → `SOURCE_UNAVAILABLE`
quand le corps est vide, et le lead `FMTEAM`/`HNISCANTRAD`
(`JSONObject["adult"] is not a string`).

### Journal 2026-10-17 — la carte du catalogue ne répète plus ses chapitres

**La demande.** « Plus besoin d'afficher les chapitres dans le card, il y en a
déjà affiché sur le cover » : la carte portait **deux** fois l'information de
chapitres — la pastille de la couverture (`cardTotal`, « 70 ch. ») et une ligne
sous le titre (`cardRecent`, « Ch. 63 · il y a 2 j »). La ligne part, la
pastille reste.

**Ce que la suppression entraîne, et qui n'était pas visible au départ.** La
ligne était le **seul** lecteur du formatage de la marche mise en place le même
jour (`f702915`) ; les deux informations venant du **même** appel `chapters`, la
borne aux cartes visibles **reste utile** (c'est elle qui alimente la pastille),
mais tout ce qui ne servait qu'à la ligne devient mort :

| ce qui disparaît | raison |
|---|---|
| `const [cardRecent, setCardRecent]` | plus lu nulle part (`tsc` l'aurait refusé) |
| le tri par date + `.slice(0, 1)` + `depuis(t, …)` | servait à choisir et formater le dernier chapitre |
| `labels` de l'entrée de cache (`EntreeRecent`) | donnée dérivée que plus personne ne lit |
| `.manga .recent` (globals.css) | unique conteneur `recent` du dépôt |
| `langue` dans les dépendances de l'effet | n'était là que parce que `t` servait à `depuis(t, …)` |
| la dérogation `eslint-disable-next-line react-hooks/exhaustive-deps` | devenue inutile, cf. ci-dessous |
| la clé i18n `source.chapitres.recents` | la case ne commande plus la ligne mais le compte → `source.chapitres.nombre` (« Nombre de chapitres » / « Chapter count ») |

**La dérogation devenue inutile, mesurée par la porte.** Premier `pnpm lint`
après le retrait : **5 avertissements** au lieu des 4 de référence —
« Unused eslint-disable directive (no problems were reported from
'react-hooks/exhaustive-deps') ». Elle n'existait que parce que `t`
(`useI18n()` le recrée à chaque rendu) manquait aux dépendances ; `t` n'étant
plus lu dans l'effet, la règle n'a plus rien à signaler. Retirée : retour à 4.
Corollaire : un compteur d'avertissements **constant** (« les 4 connus ») est une
porte — le 5ᵉ nomme l'endroit exact qu'un retrait a laissé en trop.

**Mesure dans l'app** (`.mesures/session-2026-10-17/05-carte-sans-ligne.cjs`,
grille de `custom:niadd-fr` — 60 cartes, app réelle, CDP 9223) :

| relevé | T+1,5 s | T+9,5 s |
|---|---|---|
| `.recent-line` dans la grille | **0** | **0** |
| conteneurs `.recent` dans la grille | **0** | **0** |
| pastilles `.card-count` | **60** | 60 |
| dont un compte > 0 | **60** | 60 |
| `#error-toast` | 0 | 0 |

Le second passage (même écran, relancé) donne 60 pastilles **dès T+1,5 s** :
elles viennent du cache local (`tatami.recent`, TTL 30 min) **sans aucune
requête** — le chemin « entrée fraîche reportée à l'affichage » fonctionne
toujours. Marqueur de version servie
(`.mesures/session-2026-10-17/06-marqueur-nouveau-code.cjs`) : la case de la
barre d'outils porte `Nombre de chapitres`, et ni « Chapitres récents » ni
« Recent chapters » ne sont dans le document — donc c'est bien le nouveau front
qui est mesuré, pas l'ancien servi par le dev.

**Capture.** `.mesures/session-2026-10-17/05-carte-sans-ligne.png` : couverture
+ pastille + titre, aucune ligne de chapitre ; les 4 couvertures vides du
catalogue restent en repli centré (le correctif de la veille tient).

**Portes.** Interface, dossier `apps\desktop`, **16/16 vertes** : `typecheck` 0 ;
`lint` **0 erreur + 4 avertissements** (la référence) ; les douze `*:check`
(`i18n` 400 clés = 400 littéraux, `hooks`, `chemins`, `libelles`, `francais`,
`styles` 175 classes + 68 variables toutes définies, `pages`, `perso`,
`lecteur`, `arrondi`, `ipc`, `moteur`) ; `test` **147 passés / 0** ; `build`.
Changement **front-only** : ni le cœur, ni le moteur.


### Journal 2026-10-17 (suite) — genres : dépliement à la demande

**La suite de la demande.** L'utilisateur voulait voir *tous* les genres, pas un
cap à 24 avec une note « +N ». Option retenue : garder 24 visibles par défaut
avec un bouton « Voir les N autres » qui déplie la liste complète, et
« Réduire » pour la replier.

**Le piège mesuré.** Première version du toggle : le bouton changeait bien de
libellé (« Voir les 53 autres » → « Réduire »), mais la liste restait à 24
genres. Cause : `loadTags` faisait `setTags((r.tags ?? []).slice(0, TAGS_SHOWN))`
**au chargement** — `tags` ne contenait jamais que 24 entrées, donc le dépliement
ne pouvait rien montrer. Le moteur renvoyait bien les 77 genres (mesuré sur
`MANGADEX` : 77 tags), mais ils étaient jetés à la porte. Le correctif :
conserver la liste complète dans `tags` et n'appliquer le plafond qu'à
l'affichage (`tagsExpanded ? tags : tags.slice(0, TAGS_SHOWN)`).

**Mesure dans l'app** (MANGADEX, 77 genres) :
- replié : 24 genres + « Voir les 53 autres » + note « 77 genres au total » ;
- déplié : 77 genres + « Réduire » ;
- round-trip replié→déplié→replié : 24 → 77 → 24, 0 `#error-toast`.

**Portes.** Interface, dossier `apps\desktop` : `typecheck`, `lint` (0 erreur +
4 avertissements), `i18n` (402 clés = 402 littéraux), `hooks`, `chemins`,
`libelles`, `francais`, `styles`, `pages`, `perso`, `lecteur`, `arrondi`, `ipc`,
`moteur`, `test` (147/0), `build` — **16/16 vertes**. Changement **front-only**.

### Journal 2026-10-18 — audit visuel : le crumb fantôme de Paramètres et la carte quasi vide des fiches perso

**Contexte.** Passe visuelle lecture seule d'abord, corrections ensuite, chaque
défaut prouvé à l'écran (CDP port 9223) : 11 captures de l'app réelle dans
`.mesures/visuel-2026-10-18/captures/` (01 accueil → 09 thème clair). Deux
faux positifs écartés avec mesure à l'appui (bouton « Sélectionner » paraissant
désactivé : mesuré `disabled: false` sur 232 cartes ; « footer » du lecteur :
l'étiquette `next dev`, absente de l'app livrée).

**V0 (sév. 2).** L'écran Paramètres affichait à froid un fil d'ariane
« ← Catalogues » codé en dur — Paramètres est un onglet racine de l'AppBar, pas
un sous-écran du catalogue. Retiré.

**V1 (sév. 3).** Sur une source perso sans résumé ni métadonnées
(`custom:rimuscan`), la fiche rendait une carte `section.sheet` de
**1210×268** : une ligne « Source » à `<dd>` vide (contraire au commentaire du
module `infoRows` lui-même) et ~900 px de vide. Deux correctifs :
1. `infoRows` ignore les valeurs à chaîne vide — une chaîne vide n'est pas
   une donnée ;
2. la carte ne se déclenche plus sur `fiche.coverUrl` seul — une fiche sans
   contenu ne rend plus un encart quasi vide. **Conséquence assumée** : la
   couverture n'apparaît plus dans la carte quand rien ne l'accompagne (capture
   après : fiche compacte titre + reprise + chapitres, sans encart).

**Preuve à l'écran.** `.mesures/visuel-2026-10-18/marche8.cjs` (chemin éprouvé :
tuile « Sources personnalisées » → Rimu Scans → 232 cartes → 1ʳᵉ fiche,
9 chapitres) : `settings.crumbs = 0` contre le crumb visible sur la capture
08 ante ; `fiche.sheet = null`, 0 ligne vide, capture `10-fiche-apres.png`
contre `05-fiche.png` (1210×268) avant. Captures Paramètres :
`08c-parametres-frais.png` (crumb) → `10-parametres-apres.png` (sans crumb).

**Portes.** Interface, dossier `apps\desktop` : `typecheck`, `lint` (0 erreur +
les 4 avertissements connus), `i18n`, `hooks`, `chemins` (+ `chemins-toast`),
`pages`, `perso`, `ipc`, `moteur`, `test`, `build`, puis les scripts node
`check-libelles-fiche`, `check-francais-dur`, `check-styles`,
`check-lecteur-etat`, `check-arrondi-bande` — **16/16 vertes**. Changement
**front-only**.

**Leçon de mesure.** Le walk « après » tombait sur 0 chapitre et une grille
vide tant qu'il attendait des durées fixes : la recompilation HMR de
`page.tsx` après un edit rend le premier clic muet. Attente d'élément
(`waitForFunction` sur `chapter-open`), jamais de `waitForTimeout` sur le
chemin critique. Et la tuile perso n'est pas dans la liste FR : c'est la
première tuile de la grille des langues (`marche5.cjs` est le chemin qui
marche, `marche4.cjs` avait rendu `noms: []` pour exactement cette raison).

### Mini-plan 2026-10-18 — téléchargement des chapitres : rendre le parallélisme réel

Demande utilisateur : « téléchargement ultra rapide ». Mesure d'entrée
(`.mesures/debit-2026-10-18/verif-verrou.mjs`) : 4 `fetchImage` **de la même page**
en parallèle = 762 ms contre 669 ms en série → **sérialisé**. Le verrou par source
(`ParserPool.avec`) est tenu pendant l'`execute()` bloquant de `fetch_octets`.

1. Moteur : `ParserPool.horsVerrou` (parser **posé** sur le fil, verrou non pris) ;
   `fetchImage` natif télécharge hors verrou. Ce qui débrouille une image, c'est
   `ParserActif`, pas le verrou.
2. Garde-fou : `estImageProbable` (signatures) — si les octets ne sont pas une
   image, le téléchargement est **refait sous le verrou** (chemin d'avant).
   La politesse reste : `fetch_octets` passe par `LIMITES_IMAGES` (4 simultanées).
3. Tests : signature (accepte JPEG/PNG/WEBP/GIF, rejette l'entête ZENMANGA mesuré) ;
   `horsVerrou` ne bloque pas un `avec` concurrent ; `avec` sérialise toujours.
4. Preuve : reconstruire `dist/`, re-mesurer `verif-verrou.mjs`, télécharger un
   chapitre dans l'app et relire le `.cbz` strictement.

### Journal 2026-10-18 — téléchargement des chapitres : hors verrou, mesuré dans l'app

Livré : `ParserPool.horsVerrou` + `relacher` extrait, `Server.fetchImage` qui
télécharge hors verrou avec repli sous verrou, `SignatureImage.estImageProbable`,
et 6 tests (`SignatureImageTest` 4, `ParserPoolHorsVerrouTest` 2). ADR
`docs/decisions/0019-telechargement-hors-verrou.md`.

**La mesure hors application ne tranchait pas, et c'était un fait, pas une panne.**
`verif-verrou.mjs` (4 requêtes d'une même page, 3 passes) a rendu tour à tour
`SERIALISE` (gain 0,48), `PARALLELE` (gain 2,33) et un état intermédiaire : la
latence du CDN varie de ±50 % d'une passe à l'autre, donc quatre requêtes ne
suffisent pas à conclure. Le signal cherché (×4) n'était pas dans l'échantillon.

**Ce qui a tranché : le chapitre entier, dans l'app, plus le journal du cœur.**
Protocole `marche-telechargement.cjs` (CDP 9223) : filtrer le catalogue sur
`HARMONY`, ouvrir la série, supprimer les `.cbz` de la série, cliquer
`.chapter-dl`, chronométrer jusqu'à la taille stable du fichier. Résultat, même
chapitre (19 pages, 15 822 335 o, `.cbz` validé par `verifier-cbz.py` : 19
entrées, 19 tailles distinctes, 19 JPEG) :

| mesure | avant | après |
|---|---|---|
| chapitre à froid | 12 545 ms | 7 333 ms |
| chapitre à chaud (2ᵉ passe) | 19 138 ms | 6 756 ms |
| latence médiane d'un `fetchImage` (journal) | 2 810 ms | 606 ms |

La latence par requête est la preuve de fond : elle est lue dans le journal du
cœur, où l'`id=fetchImage-<epoch>-<n>` porte l'instant du **départ** et la ligne
l'instant d'**arrivée**. 2 810 → 606 ms dit que l'attente n'est plus une file
derrière le mutex mais le réseau seul. Quatre requêtes restent « en vol » côté
cœur dans les deux cas : ce nombre ne prouve rien, il compte des envois.

Trois pièges traversés, tous dus à la mesure :
1. la racine de téléchargement réellement configurée est
   `C:\Users\alexanders\Documents\Tatami`, pas le défaut `%APPDATA%\Tatami\downloads` ;
   la sonde a attendu 300 s un `.cbz` qui était écrit ailleurs (leçon « chercher
   la valeur dans le mauvais dossier », retrouvée à l'identique) ;
2. le chapitre était **déjà** téléchargé : le clic répond « Téléchargé » en une
   seconde, donc on chronomètre l'idempotence. La sonde supprime les `.cbz` de la
   série avant de chronométrer ;
3. la 2ᵉ passe est **plus lente** que la 1ʳᵉ côté ancien moteur (19 138 contre
   12 545 ms) : le cache CDN ne biaise donc pas en faveur de l'« après », et les
   deux comparaisons vont dans le même sens.

Portes : `gradle test --rerun-tasks` sur le code restauré (226 tests, 43 XML) ;
`dist/` reconstruit et vérifié par `javap` (`horsVerrou` présent, aucun
`runBlocking`). Les trois portes des tests ont été prouvées rouges puis
restaurées : entête ZENMANGA refusé, interstitiel HTML et corps vide refusés,
`horsVerrou` qui ne prend pas le verrou.

### Mini-plan 2026-10-18 - l'incognito ne couvre pas tout l'accueil

1. Mesurer l'accueil avec l'incognito actif et lire la base (classifications réelles).
2. Corriger la coquille `ERATIC_AO`, le rattrapage qui ignore `UNKNOWN`, et la
   troisième liste de l'accueil (signets), avec une seule liste de mots-clés.
3. Prouver chaque correctif rouge puis vert, passer les portes, reconstruire le
   binaire et re-mesurer dans l'application.

### Journal 2026-10-18 - incognito : trois trous mesurés, dont la troisième liste

L'utilisateur signale que l'incognito « ne marche pas pour tout ». Mesure dans
l'app (incognito `1` dans `localStorage`) : l'accueil affichait **5 lus
récemment**, dont `【Artificial Fox】Yoruichi's Bimbofication` (source `EXHENTAI`)
et `Teto X Egen` (`custom:mangadexFr`). La base disait `UNKNOWN` pour les deux.
Trois causes distinctes, mesurées séparément :

1. **`UNKNOWN` n'est pas `NULL`.** Le filtre laisse passer les deux (`WHERE
   content_rating IS NULL OR … NOT IN (…)`) et le rattrapage ne regardait que
   `NULL`. Or c'est `UNKNOWN` que `save_progress` écrit quand la source ne
   classe pas. Le titre `EXHENTAI` restait donc visible pour toujours.
2. **Coquille `ERATIC_AO`** dans le filtre de l'historique (`store.rs`) : les
   titres `EROTIC_AO` passaient. Les favoris avaient la bonne valeur.
3. **« Vos signets » n'était pas filtré du tout** : `list_bookmarks` n'avait ni
   paramètre ni condition, et l'interrupteur ne rechargeait que favoris et
   historique. C'est la troisième liste de l'accueil, donc dans le périmètre
   annoncé par la note.

Correctifs : une fonction pure `classement_adulte(source, titre)` (une seule
liste de mots-clés) utilisée **à l'écriture** (`save_progress`, `set_favorite`)
et au rattrapage, qui couvre désormais `NULL` **et** `UNKNOWN` ; la coquille
corrigée ; `list_bookmarks(limit, manga_url, hide_adult)` filtre par `EXISTS`
sur `history`/`favorites` (le signet ne duplique pas la classification) ; le
front passe `hideAdult` et recharge les signets au clic ; la note i18n dit
maintenant « par leur source ou repérés par un mot-clé » et nomme les trois
listes.

Preuves rouges (chaque porte cassée à la main, puis restaurée) :
`incognito_masque_aussi_erotic_ao` ; `une_source_adulte_classe_le_titre_a_l_ecriture`
et `un_favori_de_source_adulte_est_classe_a_l_ecriture` (les deux tombent quand
`classement_adulte` rend `None`) ; `incognito_masque_les_signets_adultes` ;
`le_rattrapage_reclasse_les_lignes_unknown` (`left: Some("UNKNOWN")` contre
`right: Some("ADULT")`).

Mesure après reconstruction du binaire et relance (pid 2728) : la base porte
`EXHENTAI` en `ADULT` (rattrapage au démarrage), l'accueil affiche **4 lus
récemment** sans le titre adulte. Geste réel sur l'interrupteur : OFF → « 5 lus
récemment » et le titre réapparaît ; ON → « 4 lus récemment » et il disparaît.
`list_bookmarks` accepte `hideAdult` sans erreur (2 signets, aucun adulte).

Portes : `cargo fmt --check` propre, `cargo clippy --all-targets -D warnings` 0,
`cargo test --lib` **247 passés / 0 échec** ; front `typecheck`, `lint` (4
avertissements connus), `i18n:check` (404 clés), `hooks:check`, `chemins:check`,
`pages:check`, `perso:check`, `ipc:check`, `moteur:check`, `test`, `build` : 0.

Limite qui reste, et elle est dite : un titre que la source classe `UNKNOWN` et
qu'aucun mot-clé ne trahit reste visible (`Teto X Egen`, tags Romance/Slice of
Life). Le catalogue et la recherche ne sont pas filtrés, conformément à la note
(« Ils restent accessibles en recherche »).

### Mini-plan 2026-10-18 - lecture : les pages qui ne viennent jamais, et les boutons qui débordent

1. Mesurer la lecture d'un chapitre local de bout en bout : descente jusqu'à la
   dernière page, puis sauts au milieu et à la fin par le formulaire.
2. Mesurer la carte de téléchargement : position des boutons par rapport au bord
   de la carte, hauteur, `flex-wrap`.
3. Corriger, prouver rouge, re-mesurer, passer les portes, documenter.

### Journal 2026-10-18 - lecture : l'admission s'arrêtait au premier retard

Signalé par l'utilisateur : « les boutons de la page Téléchargements débordent et
sont gros », et « quand je saute d'une page à l'autre, l'image doit s'afficher
quand même ».

**Défilement continu, chapitre local de 23 pages (`Got a Gallery in the Wild`,
`custom:rimuscan`, `HORS LIGNE`)** — la bande est virtualisée (Rust donne la
hauteur de chaque page). Mesure instrumentée (requêtes `/page/` comptées) :

| instant | pages montées | avec `src` | décodées | requêtes `/page/` |
|---|---|---|---|---|
| ouverture | 6 | 6 | 5 | 6 |
| 16 000 px | 8 | 6 | 5 | 6 |
| 48 000 px | 9 | 4 | 2 | 6 |
| 96 000 px | 9 | **0** | **0** | 7 (après 4 s d'attente : toujours 0) |

Cause : `admis` ne grandissait que d'**une page par `onLoad`**
(`admettreUne(prec, fenetre)`). Dès que les chargements prenaient du retard sur
le défilement, la fenêtre montée ne contenait plus aucune page admise — pas de
`src`, donc pas de chargement, donc pas de `onLoad`, donc plus rien n'était
admis. Même cercle vicieux que le saut de page du 2026-10-10 et que le saut vers
la fin du 2026-10-17, sous une troisième forme : **le défilement**.

Correctif : `pagesAffichees(admis, fenetre, virtualisee)` (`lib/quota-pages.ts`)
— en bande virtualisée, l'ensemble affiché est l'union de `admis` et de la
**fenêtre montée** (bornée : marge 3 + portée de 4 200 px, ~8 pages, soit la
rafale de l'ouverture). C'est un **calcul de rendu** (`useMemo`), pas un
`setState` dans un effet : `react-hooks/set-state-in-effect` refuse ce dernier,
et un effet recopiant la fenêtre dans l'état aurait provoqué des rendus en
cascade. Hors virtualisation, `fenetreBande` rend **toutes** les pages : on y
garde la file « une par décodage » (sinon 303 requêtes d'un coup).

Mesure après correctif (même chapitre, même sonde) : à 16 000 px **8 montées /
8 avec `src`** ; à 96 000 px **9 / 9**, 5 décodées, **12 requêtes** ; descente
par paliers jusqu'au fond (221 pas) → compteur **23 / 23**, dernière page
`complete`, `900 × 15 141`, **0 page manquante, 0 erreur console**. Sauts par le
formulaire : page 12 → image `900 × 16 000` décodée, dans la fenêtre (`top:
73`) ; page 23 → dernière page décodée ; retour page 1 → décodée (`1778 × 2320`
affichée `810 × 1 058`).

Preuve rouge : en revenant à `src={admis.includes(i) ? … : undefined}` (sans
l'union), la même sonde redonne **0 page avec `src`, 0 décodée, 7 requêtes**
après 96 000 px. Preuve rouge du test unitaire : en neutralisant
`if (virtualisee)` dans `pagesAffichees`, `pnpm test` passe à 1 et nomme
`en bande virtualisée, toute la fenêtre est affichable` ; restauré, 0.

Chapitre **distant** (non virtualisé, `Teto X Egen`, 56 pages) : après 3
défilements, compteur 5 / 56, `avecSrc` = 10 pages, décodées, 0 erreur — le
quota « une par décodage » continue d'avancer de son côté.

**Boutons de Téléchargements** — mesuré avant : `.dl-actions` en `flex-wrap:
nowrap`, dernier bouton **+37,7 px au-delà du bord droit** de la carte, tous les
libellés coupés (« Lire » sur 3 lignes), hauteur **88,4 px**. Cause : quatre
boutons (« Lire », « Choisir un chapitre (n) », « Reprendre la file »,
« Supprimer ») ne tiennent pas sur 265 px et `flex` les comprimait au lieu de
les faire passer à la ligne. Correctif : `flex-wrap: wrap` +
`align-items: flex-start` + `white-space: nowrap`. Après : débordement **−23 px**
(donc dedans), hauteur **32,6 px**, `scrollWidth === width` ; « Supprimer » passe
sur une seconde ligne. Capture : `.mesures/lecture-2026-10-18/capture-cartes.png`.

Portes (front, toutes exécutées depuis `apps/desktop`) : `typecheck`, `lint`
(4 avertissements connus, 0 erreur), `i18n:check`, `hooks:check`, `chemins:check`,
`pages:check`, `perso:check`, `ipc:check`, `moteur:check`, `test`, `build` : **0**. Pas de changement Rust, donc
pas de reconstruction du binaire : le front est servi par `next dev`.

