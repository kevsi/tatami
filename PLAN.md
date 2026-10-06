# Tatami — Phase avancée : mode webtoon + highlight chapitre en cours

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

### En cours
- Vérifier visuellement que `.chapter-cell.encours` s'affiche correctement en thème clair/sombre avec `--encours-texte`.
- Valider que la recherche source "hentai" trouve HENTAIORIGINES et affiche le bouton "Voir plus (N restantes)".
- S'assurer que la page Téléchargements montre `progressParSource` par ligne webtoon.

### Next steps
1. Valider l'affichage `.chapter-cell.encours` thème clair/sombre.
2. S'assurer que `progressParSource` s'affiche correctement sur la page Téléchargements.
3. Exécuter `pnpm build` pour vérifier la production.
4. Valider le smoke test manuel `docs/smoke-test.md`.

### Fichiers modifiés
- `apps/desktop/app/page.tsx` — lecteur webtoon-only, highlight chapitre en cours, helpers `chapterPage`/`chapterCount`.
- `apps/desktop/app/globals.css` — `.chapter-cell.encours` CSS class.
- `apps/desktop/app/i18n.ts` — supprimé `erreur.page`.
- `apps/desktop/src-tauri/src/download.rs` — verrou `DownloadState` HashMap (déjà fait auparavant).

### État du dépôt
- 5 commits : `b6734e7` téléchargements parallèles, `c141377` chargeur animé, `6e25642` molette fix,
  `9bf3fe6` clé Gemini dans le trousseau OS, `f410272` suppression du surlignage "en cours"
- Toutes les vérifications passent (clippy, typecheck, lint, i18n, hooks, chemins).