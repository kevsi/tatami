//! Sidecar vision de Tatami : `detect` → `ocr` → `inpaint`.
//!
//! Ce binaire est le pendant du moteur de sources (`engine/`, Kotlin) pour la
//! vision. `PLAN-TRADUCTION.md` §2.2 : « Un sidecar dédié à la vision, séparé
//! du moteur de sources. Rust l'orchestre (file de jobs, cache, packs de
//! modèles). »
//!
//! ## Runtime
//!
//! ONNX Runtime appelé **depuis Rust**, sans Python dans l'application livrée
//! (`docs/decisions/0007-runtime-vision-onnx.md`, acceptée le 2026-10-07 sur
//! une parité mesurée). Le crate `ort` est en `load-dynamic` : le chemin de la
//! DLL vient de `ORT_DYLIB_PATH`, ce qui évite le « shared library hell »
//! documenté pour Windows et permet d'utiliser un runtime déjà présent.
//!
//! ## État
//!
//! - `dico` : dictionnaire et décodage CTC — **fait et testé**, avec le défaut
//!   d'espace mesuré le 2026-10-07 couvert par une régression.
//! - `ocr` : lecture d'une ligne — **fait**, pré-traitement testé sans poids.
//! - `lignes`, `detect`, `inpaint`, protocole NDJSON : à faire.
//!
//! Les poids ne sont **jamais** dans le dépôt : ils arrivent par packs
//! versionnés et vérifiés (`pack.rs` côté application).

pub mod dico;
pub mod ocr;
