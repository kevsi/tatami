//! Types du contrat vision sidecar (T1.3).
//!
//! Le sidecar vision expose : `hello`, `capabilities`, `detect`, `ocr`,
//! `inpaint`, `health`, `loadPack`, `shutdown`.
//! Les erreurs typées : `MODEL_MISSING`, `OUT_OF_MEMORY`, `TILE_TOO_LARGE`,
//! `TIMEOUT`, `CANCELLED`, `INTERNAL`.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Types de base
// ---------------------------------------------------------------------------

/// Boîte englobante, en pixels.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VisionBbox {
    pub x: i64,
    pub y: i64,
    pub w: i64,
    pub h: i64,
}

/// Ligne OCR.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VisionOcrLine {
    pub text: String,
    pub bbox: VisionBbox,
}

/// Modèle dans un pack.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VisionModel {
    pub id: String,
    pub path: String,
    pub sha256: String,
}

/// Manifeste d'un pack de modèles.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PackManifest {
    pub pack_id: String,
    pub version: String,
    pub models: Vec<VisionModel>,
    /// Hash du manifeste lui-même.
    pub sha256: String,
}

// ---------------------------------------------------------------------------
// Résultats par méthode
// ---------------------------------------------------------------------------

/// Résultat de `detect`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DetectResult {
    pub boxes: Vec<VisionBbox>,
}

/// Résultat de `ocr`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OcrResult {
    pub lines: Vec<VisionOcrLine>,
}

/// Résultat de `inpaint`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InpaintResult {
    pub data: String,
    pub content_type: String,
}

/// Résultat de `loadPack`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LoadPackResult {
    pub pack_id: String,
    pub version: String,
}

/// Résultat de `health` (vision).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HealthResult {
    pub ok: bool,
    pub pack_id: Option<String>,
    pub version: Option<String>,
    pub models: Vec<String>,
}

// ---------------------------------------------------------------------------
// Erreurs
// ---------------------------------------------------------------------------

/// Erreurs typées du sidecar vision.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum VisionErrorCode {
    ModelMissing,
    OutOfMemory,
    TileTooLarge,
    Timeout,
    Cancelled,
    Internal,
}

impl std::fmt::Display for VisionErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            VisionErrorCode::ModelMissing => "MODEL_MISSING",
            VisionErrorCode::OutOfMemory => "OUT_OF_MEMORY",
            VisionErrorCode::TileTooLarge => "TILE_TOO_LARGE",
            VisionErrorCode::Timeout => "TIMEOUT",
            VisionErrorCode::Cancelled => "CANCELLED",
            VisionErrorCode::Internal => "INTERNAL",
        };
        write!(f, "{s}")
    }
}
