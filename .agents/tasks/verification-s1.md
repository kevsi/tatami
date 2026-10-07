# Vérification S1 — Migration SQLite v9 + commandes sources personnalisées

Date : 2026-10-06 (v2 — corrections review-s1)

## cargo fmt --check

```
cd apps\desktop\src-tauri
cargo fmt --check
```

Résultat : **0 diff — propre**.

---

## cargo clippy --all-targets -- -D warnings

```
cd apps\desktop\src-tauri
cargo clippy --all-targets -- -D warnings
```

Résultat : **0 warning, 0 error** — EXIT 0.

---

## cargo test -- --test-threads=1

```
cd apps\desktop\src-tauri
cargo test -- --test-threads=1
```

### Bibliothèque (lib)

```
test result: ok. 185 passed; 0 failed; 0 ignored
```

Tests S1 spécifiquement :
- `custom_source::tests::importer_refuse_trop_gros` … **ok**
- `custom_source::tests::importer_refuse_traversee` … **ok**
- `custom_source::tests::importer_valide_copie_et_enregistre` … **ok**
- `custom_source::tests::importer_chemin_avec_remonter_copie_dans_dest_dir` … **ok** (nouveau, review finding 2)
- `store::tests::migration_v8_vers_v9_cree_custom_sources` … **ok** (assertion corrigée >= 9, review finding 1)

### Test d'intégration `contrat_moteur` (hors périmètre S1 — pré-existant)

Le test `chaque_methode_du_schema_a_une_reponse_enregistree` échoue sur
`["detect", "ocr", "inpaint", "loadPack"]` — méthodes ajoutées dans les phases T2/T3
sans réponses enregistrées disponibles. **Pré-existant, hors périmètre S1.**

---

## Corrections apportées (itération review-s1)

### Finding 1 — Assertion `== 9` remplacée par `>= 9`

`migration_v8_vers_v9_cree_custom_sources` affirmait `version == 9` alors que
`SCHEMA_VERSION = 10`. Après `migrate()` depuis v8, les blocs v9 ET v10 s'exécutent
et `user_version = 10` est écrit. L'assertion `>= 9` est correcte ; la vérification
de l'existence et de l'utilisabilité de la table (déjà présente dans le test) reste
la vraie preuve.

### Finding 2 — Nouveau test `importer_chemin_avec_remonter_copie_dans_dest_dir`

`importer_refuse_traversee` utilisait un fichier inexistant, donc l'erreur venait de
`fs::metadata`. Ajouté un test complémentaire qui crée un vrai fichier `.js` et passe
un chemin source contenant `..` (via `parent/../parent/real.js`). Le test prouve que
le chemin `..` dans la source ne cause pas de traversal : la copie atterrit dans
`dest_dir/real.js`, jamais ailleurs — conformément à la logique de section 4 qui
reconstruit la destination depuis `dest_dir_canon.join(filename)`.

### Finding 3 — TODO S2 documenté dans custom_source.rs

Ajouté un commentaire `TODO S2 :` explicite à la ligne où `lang = "fr"` est fixé en
dur, indiquant ce que S2 devra faire (ajouter un paramètre `lang: Option<String>`).

---

## Résumé des modifications totales (S1)

1. **`store.rs`** : `SCHEMA_VERSION` 8 → 9 (puis 9 → 10 par fusion v10 préexistante),
   migration v9 (`custom_sources`), struct `CustomSource`, 3 méthodes CRUD,
   test `migration_v8_vers_v9_cree_custom_sources` (assertion corrigée).

2. **`custom_source.rs`** (nouveau) : `importer_source_impl` + 3 commandes Tauri +
   4 tests (dont le nouveau test de traversal avec fichier existant).

3. **`Cargo.toml`** : `dirs-next = "2"`.

4. **`lib.rs`** : `pub mod custom_source;`.

5. **`main.rs`** : import, 3 commandes dans `generate_handler!`, liste blanche S2.
