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
