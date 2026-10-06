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

### En cours
- T2.1 — Assemblage de la bande virtuelle et découpe en tuiles aux lignes uniformes.

### Next steps
1. **T2.1** — Assemblage de la bande virtuelle et découpe en tuiles aux lignes uniformes, avec fusion des détections aux jonctions. Tests sur images synthétiques.
2. **T2.2** — Détection et segmentation des bulles et du texte, avec masque précis de l'intérieur.
3. **T2.3** — Classification des blocs : dialogue, narration, sfx, texte libre, à ignorer.
4. **T2.4** — OCR de l'anglais, regroupement des lignes d'une même bulle, ordre de lecture.

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
- 6 commits : `b6734e7` téléchargements parallèles, `c141377` chargeur animé, `6e25642` molette fix,
  `9bf3fe6` clé Gemini dans le trousseau OS, `f410272` suppression du surlignage "en cours",
  `59526aa` T1 complet (schéma, migrations, vision, packs, tests)
- Toutes les vérifications passent (clippy, typecheck, lint, i18n, hooks, chemins).
- Prochaine phase : **T2 — Pipeline vision**.
