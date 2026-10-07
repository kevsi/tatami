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

**Phase T2 livrée** — pipeline vision complet : tuiles → masques → classification → OCR → effacement → contrôle de résidu → benchmarks mémoire.

### En cours
- (aucune)

### Next steps
1. **T3.1** — Trait `Translator` côté Rust. Fournisseurs : LLM en ligne (clé de l'utilisateur dans le trousseau de l'OS) et modèle local si l'ADR 0008 le retient.

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

- [ ] **S2.1** — Écran « Sources perso » : bouton « Importer un script (.js) » (dialogue natif), liste
  des sources installées (nom, langue, date), bouton supprimer avec confirmation.
  i18n FR + EN. Clés à ajouter dans `i18n.ts`.
  _Critère : `pnpm typecheck/lint/i18n:check/hooks:check/chemins:check` vertes._

- [ ] **S2.2** — Intégration dans l'écran Sources : les sources custom apparaissent dans la grille avec
  un badge « Perso ». Désactivation possible comme les sources natives.
  _Critère : smoke test manuel (docs/smoke-test.md §S)._

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
- 16 commits : `b6734e7` téléchargements parallèles, `c141377` chargeur animé, `6e25642` molette fix,
  `9bf3fe6` clé Gemini dans le trousseau OS, `f410272` suppression du surlignage "en cours",
  `59526aa` T1 complet (schéma, migrations, vision, packs, tests), `ff33cd0` docs T1,
  `5758ff3` T2.1 tuiles, `539d3ef` T2.2 masques, `02f63a5` T2.3 classification, `b541844` T2.4 OCR/ordre de lecture,
  `ed2d745` T2.5 stratégie d'effacement, `a8fb87a` T2.6 contrôle de résidu, `d6b9121` T2.7 benchmarks mémoire,
  `c6f7dc9` T3.1 trait Translator, `48e7c0a` T3.2 prompt par page + validation, `c375c8c` T3.3 glossaire SQLite v10,
  `de53744` T3.4 injection glossaire dans le prompt
- Toutes les vérifications passent (clippy, typecheck, lint, i18n, hooks, chemins).
- Prochaine phase : **T3.5 — Résilience** (file d'attente, backoff, quotas, mode hors ligne).
