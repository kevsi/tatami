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
///
/// `Copy` : quatre `i64`, donc la copie est triviale — et son absence coûte
/// cher. Sans lui, un test qui veut regarder `fusion[0].bbox` doit cloner, et
/// le clone se glisse dans un message d'assertion… où il masque la valeur
/// réelle. Le type qui empêche la faute vaut mieux que le `clone()` de
/// précaution.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
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

/// Une détection : sa boîte, sa **classe** et son score.
///
/// La classe est la classification de T2.3 et elle est **mesurée** par le
/// modèle : la jeter au contrat obligerait l'application à deviner `type_bloc`,
/// que le schéma de traduction exige. Mesuré le 2026-10-07.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Detection {
    pub bbox: VisionBbox,
    /// `0` bulle, `1` texte dans une bulle, `2` texte hors bulle.
    pub classe: i64,
    pub score: f64,
}

/// Ce que le détecteur distingue. Miroir de `detect::Classe` côté sidecar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClasseBloc {
    /// Une bulle de dialogue : effaçable et traduisible en surcouche.
    Bulle,
    /// Du texte posé dans une bulle : même traitement.
    TexteBulle,
    /// Hors bulle (narration, effets sonores, crédits) : la politique d'ADR 0011
    /// dira s'il est effacé ; en attendant il **n'est pas** effacé, ce qui est
    /// la valeur prudente.
    TexteLibre,
}

impl ClasseBloc {
    /// Convertit l'identifiant du modèle. Une classe inconnue devient
    /// `TexteLibre` : prudent, et jamais un effacement.
    pub fn depuis_id(id: i64) -> ClasseBloc {
        match id {
            0 => ClasseBloc::Bulle,
            1 => ClasseBloc::TexteBulle,
            _ => ClasseBloc::TexteLibre,
        }
    }

    pub fn id(self) -> i64 {
        match self {
            ClasseBloc::Bulle => 0,
            ClasseBloc::TexteBulle => 1,
            ClasseBloc::TexteLibre => 2,
        }
    }

    /// Vrai si le texte est dans une bulle.
    pub fn dans_bulle(self) -> bool {
        matches!(self, ClasseBloc::Bulle | ClasseBloc::TexteBulle)
    }
}

/// Résultat de `detect`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DetectResult {
    pub detections: Vec<Detection>,
}

/// Résultat de `ocr`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OcrResult {
    pub lines: Vec<VisionOcrLine>,
}

/// Résultat de `inpaint`.
///
/// L'image est en base64 **PNG** : le JPEG repasserait par une compression qui
/// bavouille précisément les bords de texte qu'on vient d'effacer.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InpaintResult {
    pub data: String,
    pub content_type: String,
    /// Nombre de tuiles de 512 utilisées, pour le journal et le diagnostic.
    #[serde(default)]
    pub tiles: usize,
    #[serde(default)]
    pub ms: u64,
}

/// Résultat de `loadPack`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LoadPackResult {
    #[serde(rename = "packId")]
    pub pack_id: String,
    pub version: String,
}

/// Résultat de `capabilities`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CapabilitiesResult {
    #[serde(rename = "packId")]
    pub pack_id: String,
    pub version: String,
    pub methods: Vec<String>,
    #[serde(rename = "inpaintDisponible")]
    pub inpaint_disponible: bool,
}

/// Résultat de `health` (vision).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HealthResult {
    pub ok: bool,
    #[serde(default)]
    pub pack_id: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Une réponse **enregistrée** du sidecar, pas une écriture à la main.
    ///
    /// Mesuré le 2026-10-07 : ces types n'avaient **aucun** test, donc rien ne
    /// prouvait que le Rust lit ce que le sidecar écrit. Un champ mal nommé
    /// (ou une classe perdue) serait resté invisible jusqu'à la première
    /// traduction, c'est-à-dire chez l'utilisateur.
    fn fixture(nom: &str) -> serde_json::Value {
        let chemin = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join(nom);
        let texte = std::fs::read_to_string(&chemin)
            .unwrap_or_else(|e| panic!("fixture absente {} : {e}", chemin.display()));
        serde_json::from_str(&texte).expect("fixture JSON invalide")
    }

    #[test]
    fn le_type_rust_lit_la_detection_enregistree() {
        let v = fixture("23-vision-detect.json");
        let r: DetectResult = serde_json::from_value(v["result"].clone())
            .expect("DetectResult ne lit pas la fixture");
        assert!(!r.detections.is_empty(), "la fixture porte des détections");
        let premiere = &r.detections[0];
        // La classe survit au contrat : c'est toute la raison du changement.
        assert!(
            (0..=2).contains(&premiere.classe),
            "classe hors contrat : {}",
            premiere.classe
        );
        assert!(
            (0.0..=1.0).contains(&premiere.score),
            "score hors intervalle : {}",
            premiere.score
        );
        assert!(premiere.bbox.w > 0 && premiere.bbox.h > 0, "boîte non vide");
    }

    #[test]
    fn le_type_rust_lit_les_capacites_enregistrees() {
        let v = fixture("22-vision-capabilities.json");
        let r: CapabilitiesResult = serde_json::from_value(v["result"].clone())
            .expect("CapabilitiesResult ne lit pas la fixture");
        assert!(r.methods.contains(&"detect".to_string()));
        assert!(r.methods.contains(&"ocr".to_string()));
        // Cohérence interne de la réponse enregistrée : `inpaintDisponible`
        // doit correspondre à la présence de `inpaint` dans `methods`. C'est
        // cette cohérence que l'application lit pour afficher « effacement
        // indisponible » au lieu d'appeler une méthode qui échouera.
        assert_eq!(
            r.inpaint_disponible,
            r.methods.contains(&"inpaint".to_string()),
            "`inpaintDisponible` et `methods` se contredisent"
        );
    }

    #[test]
    fn le_type_rust_lit_l_ocr_enregistre_avec_ses_boites() {
        let v = fixture("24-vision-ocr.json");
        let r: OcrResult =
            serde_json::from_value(v["result"].clone()).expect("OcrResult ne lit pas la fixture");
        assert!(!r.lines.is_empty(), "la fixture porte des lignes");
        assert!(
            r.lines.iter().all(|l| !l.text.trim().is_empty()),
            "une ligne vide ne devrait pas avoir été enregistrée"
        );
        assert!(
            r.lines.iter().any(|l| l.text.contains(' ')),
            "au moins une ligne doit contenir une espace : c'est la classe 18709 du \
             dictionnaire, celle que le fichier .txt ne nomme pas. Sans elle on lirait \
             YOUGOTTOGET. (Pas **toutes** : « THES? » est un seul mot, et c'est correct.)"
        );
    }

    #[test]
    fn la_classe_du_modele_se_convertit_sans_inventeur() {
        assert_eq!(ClasseBloc::depuis_id(0), ClasseBloc::Bulle);
        assert_eq!(ClasseBloc::depuis_id(1), ClasseBloc::TexteBulle);
        assert_eq!(ClasseBloc::depuis_id(2), ClasseBloc::TexteLibre);
        // Une classe inconnue ne doit jamais devenir une bulle : ce serait un
        // effacement et une traduction d'un texte qu'on n'a pas identifié.
        assert_eq!(ClasseBloc::depuis_id(99), ClasseBloc::TexteLibre);
        assert!(!ClasseBloc::depuis_id(99).dans_bulle());
        assert!(ClasseBloc::depuis_id(0).dans_bulle());
        for id in 0..3 {
            assert_eq!(ClasseBloc::depuis_id(id).id(), id);
        }
    }
}
