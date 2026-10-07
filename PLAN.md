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

**Phase T4 en cours** — mise en page et rendu.

### Next steps
1. **S2.2** — jouer `docs/smoke-test.md` §S (section écrite le 2026-10-07, jamais exécutée).
2. **Retirer la liste blanche S2** de `main.rs:387-393` : les trois commandes sont maintenant
   appelées par l'interface, la porte « commande morte » doit de nouveau mordre.
3. **T4.4** — sélection de police par type de bulle et amélioration de la césure.

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
