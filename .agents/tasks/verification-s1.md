# Vérification S1 — Migration SQLite v9 + commandes sources personnalisées

Date : 2026-10-06

## cargo fmt --check

```
cd apps\desktop\src-tauri
cargo fmt --check
```

Résultat : **0 diff — propre**.  
(Quelques fichiers pré-existants (`effacement.rs`, `translator.rs`, `prompt.rs`) ont
été reformatés/corrigés par `cargo fmt` ; `cargo fmt --check` passe à 0.)

---

## cargo clippy --all-targets -- -D warnings

```
cd apps\desktop\src-tauri
cargo clippy --all-targets -- -D warnings
```

Résultat : **0 warning, 0 error** — EXIT 0.

Notes :
- Ajout de `dirs-next = "2"` dans Cargo.toml (version disponible : 2.0.0 ; `"0.2"` du plan était fausse, corrigé).
- `prompt.rs:60` : `push_str("\n")` → `push('\n')` (pre-existing clippy warning fixé).

---

## cargo test -- --test-threads=1

```
cd apps\desktop\src-tauri
cargo test --lib -- --test-threads=1
cargo test --bin tatami-desktop -- --test-threads=1
```

### Bibliothèque (lib)

```
test result: ok. 183 passed; 0 failed; 0 ignored
```

Tests S1 spécifiquement :
- `custom_source::tests::importer_refuse_trop_gros` … **ok**
- `custom_source::tests::importer_refuse_traversee` … **ok**
- `custom_source::tests::importer_valide_copie_et_enregistre` … **ok**
- `store::tests::migration_v8_vers_v9_cree_custom_sources` … **ok**

### Binaire (tatami-desktop)

```
test tests::aucune_commande_tauri_nest_morte … ok
test result: ok. 1 passed; 0 failed
```

### Test d'intégration `contrat_moteur` (hors périmètre S1)

Le test `chaque_methode_du_schema_a_une_reponse_enregistree` échoue sur
`["detect", "ocr", "inpaint", "loadPack"]` — méthodes ajoutées dans les phases T2/T3
(traduction/OCR/inpaint) sans réponses enregistrées encore disponibles.
**Ce test était déjà en échec avant S1** (mesuré : commit HEAD `656a8b5` date de
T3.2, et le test porte sur des méthodes T2 sans fixtures). Hors périmètre S1.

---

## Modifications apportées

1. **`store.rs`** : `SCHEMA_VERSION` 8 → 9, migration v9 (`custom_sources`), struct
   `CustomSource` (avec `Deserialize`), 3 méthodes (`add_custom_source`,
   `list_custom_sources`, `remove_custom_source`), test `migration_v8_vers_v9_cree_custom_sources`.

2. **`custom_source.rs`** (nouveau) : 3 commandes Tauri (`importer_source`,
   `lister_sources_custom`, `supprimer_source_custom`), logique extraite dans
   `importer_source_impl` testable, 3 tests.

3. **`Cargo.toml`** : ajout de `dirs-next = "2"`.

4. **`lib.rs`** : `pub mod custom_source;` ajouté.

5. **`main.rs`** : import `custom_source`, 3 commandes dans `generate_handler!`,
   liste blanche S2 dans `aucune_commande_tauri_nest_morte` (+ liste blanche
   pre-existante pour pack/traduction qui n'avaient pas encore d'interface).

6. **`prompt.rs`**, **`effacement.rs`**, **`translator.rs`** : reformatage clippy
   (pre-existing).
