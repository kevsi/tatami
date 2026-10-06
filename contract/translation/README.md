# Contrat de traduction — PageTranslation v1

Schéma : `schema.json` (JSON Schema draft 2020-12).
Types générés :
- Rust : `apps/desktop/src-tauri/src/traduction.rs` (`PageTranslation`, `BlocTraduction`, `Bbox`, `TypeBloc`, `StatutBloc`)
- TypeScript : `apps/desktop/app/types/traduction.ts`

## Règles
- `readingOrder` est la clé de tri des blocs sur la page.
- `confidence` = 0 quand inconnu, 1 quand certain.
- `originalText` / `translatedText` sont optionnels : un bloc `ignore` peut ne rien porter.
- `mangaUrl` est optionnel : certaines pages isolées n'ont pas de contexte de série.

## Tests
- Rust : `cargo test contrat_traduction` valide la fixture contre le schéma, puis valide les types Rust sérialisés.
- TypeScript : `pnpm typecheck` valide l'usage de `PageTranslation` dans le front.
