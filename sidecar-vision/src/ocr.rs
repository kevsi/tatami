//! Lecture d'une **ligne** de texte par PP-OCRv6.
//!
//! ## Ce que le modèle accepte
//!
//! PP-OCRv6 est un CTC de reconnaissance : il lit **une ligne**, pas une bulle.
//! Mesuré le 2026-10-07 : appliqué à une bulle entière de 641×640 px, il rend
//! `LE` puis `pe`. Le découpage en lignes est fait par un autre modèle
//! (`manga_det_v0.2.onnx`), voir le module `lignes`.
//!
//! ## Recette, mesurée et non reconstituée
//!
//! La hauteur est fixée à 48 px et la largeur suit le rapport, l'image est
//! ramenée dans [-1, 1] par `(x / 255 - 0.5) / 0.5`, puis le décodage est un
//! CTC sans répétition. C'est la recette de la sonde Python de l'évaluation,
//! et la parité a été vérifiée : même image, mêmes bandes, **texte identique**
//! entre Python et Rust (51 ms contre 193 ms par ligne).

use crate::dico::{Dictionnaire, ErreurDictionnaire};
use ort::session::Session;
use ort::value::Tensor;

/// Hauteur d'entrée du modèle, en pixels.
pub const HAUTEUR_ENTREE: u32 = 48;

/// Erreur de lecture.
#[derive(Debug)]
pub enum ErreurLecture {
    /// Le modèle n'a pas pu être ouvert.
    Modele(String),
    /// Le dictionnaire ne concorde pas avec le modèle.
    Dictionnaire(ErreurDictionnaire),
    /// La sortie du modèle n'a pas la forme attendue.
    FormeSortie { trouve: usize, attendu: usize },
    /// L'image de la ligne est trop petite pour être lue.
    LigneTropPetite { largeur: u32, hauteur: u32 },
}

impl std::fmt::Display for ErreurLecture {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ErreurLecture::Modele(m) => write!(f, "modèle illisible : {m}"),
            ErreurLecture::Dictionnaire(e) => write!(f, "dictionnaire : {e}"),
            ErreurLecture::FormeSortie { trouve, attendu } => write!(
                f,
                "la sortie du modèle a {trouve} dimensions, {attendu} attendues"
            ),
            ErreurLecture::LigneTropPetite { largeur, hauteur } => write!(
                f,
                "ligne de {largeur}x{hauteur} px : trop petite pour être lue"
            ),
        }
    }
}

impl std::error::Error for ErreurLecture {}

impl From<ErreurDictionnaire> for ErreurLecture {
    fn from(e: ErreurDictionnaire) -> Self {
        ErreurLecture::Dictionnaire(e)
    }
}

/// Prépare une ligne pour le modèle : hauteur 48 px, largeur proportionnelle,
/// valeurs dans [-1, 1].
///
/// Fonction pure, donc testable sans poids : c'est ici que se decides la
/// plupart des défauts d'un OCR mal réglé, et une erreur ici est invisible
/// ensuite.
pub fn preparer(ligne: &image::RgbImage) -> Result<(usize, Vec<f32>), ErreurLecture> {
    let (largeur, hauteur) = (ligne.width(), ligne.height());
    if largeur < 8 || hauteur < 8 {
        return Err(ErreurLecture::LigneTropPetite { largeur, hauteur });
    }
    let entree_w = ((largeur as f32 * HAUTEUR_ENTREE as f32 / hauteur as f32) as u32).max(1);
    let petite = image::imageops::resize(
        ligne,
        entree_w,
        HAUTEUR_ENTREE,
        image::imageops::FilterType::Triangle,
    );
    let (w, h) = (petite.width() as usize, petite.height() as usize);
    let mut data = vec![0f32; 3 * w * h];
    for (x, y, px) in petite.enumerate_pixels() {
        let (x, y) = (x as usize, y as usize);
        for c in 0..3usize {
            data[c * w * h + y * w + x] = (px[c] as f32 / 255.0 - 0.5) / 0.5;
        }
    }
    Ok((w, data))
}

/// Lecteur de lignes : une session ONNX et son dictionnaire.
pub struct Lecteur {
    session: Session,
    dico: Dictionnaire,
}

impl Lecteur {
    /// Ouvre le modèle et construit le dictionnaire à partir du `.txt`.
    ///
    /// Le nombre de classes est **mesuré** par une passe sur une entrée
    /// vide, et non lu dans les métadonnées : c'est le seul moyen d'être sûr
    /// que le dictionnaire comble exactement ce qui manque, y compris si le
    /// fichier est un jour mis à jour.
    pub fn ouvrir(modele: &std::path::Path, dico_txt: &str) -> Result<Lecteur, ErreurLecture> {
        let mut builder = Session::builder().map_err(|e| ErreurLecture::Modele(e.to_string()))?;
        let session = builder
            .commit_from_file(modele)
            .map_err(|e| ErreurLecture::Modele(e.to_string()))?;
        let mut session = session;
        let classes = mesurer_classes(&mut session)?;
        let dico = Dictionnaire::depuis_txt(dico_txt, classes)?;
        Ok(Lecteur { session, dico })
    }

    /// Le dictionnaire effectivement utilisé, avec son nombre de classes.
    pub fn dictionnaire(&self) -> &Dictionnaire {
        &self.dico
    }

    /// Lit une ligne et rend son texte, espaces compris.
    pub fn lire(&mut self, ligne: &image::RgbImage) -> Result<String, ErreurLecture> {
        let (w, data) = preparer(ligne)?;
        let tenseur = Tensor::from_array((vec![1i64, 3, HAUTEUR_ENTREE as i64, w as i64], data))
            .map_err(|e| ErreurLecture::Modele(e.to_string()))?;
        let sortie = self
            .session
            .run(ort::inputs!["x" => tenseur])
            .map_err(|e| ErreurLecture::Modele(e.to_string()))?;
        let (forme, valeurs) = sortie[0]
            .try_extract_tensor::<f32>()
            .map_err(|e| ErreurLecture::Modele(e.to_string()))?;
        let dims: &[i64] = &forme[..];
        // [1, pas_de_temps, classes]
        if dims.len() != 3 {
            return Err(ErreurLecture::FormeSortie {
                trouve: dims.len(),
                attendu: 3,
            });
        }
        let (temps, classes) = (dims[1] as usize, dims[2] as usize);
        let ids: Vec<i64> = (0..temps)
            .map(|t| {
                let ligne = &valeurs[t * classes..(t + 1) * classes];
                ligne
                    .iter()
                    .enumerate()
                    .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
                    .map(|(i, _)| i as i64)
                    .unwrap_or(0)
            })
            .collect();
        Ok(self.dico.decoder(&ids))
    }
}

/// Mesure le nombre de classes du modèle par une passe sur une entrée vide.
fn mesurer_classes(session: &mut Session) -> Result<usize, ErreurLecture> {
    let vide = vec![0f32; 3 * HAUTEUR_ENTREE as usize * 32];
    let tenseur = Tensor::from_array((vec![1i64, 3, HAUTEUR_ENTREE as i64, 32], vide))
        .map_err(|e| ErreurLecture::Modele(e.to_string()))?;
    let sortie = session
        .run(ort::inputs!["x" => tenseur])
        .map_err(|e| ErreurLecture::Modele(e.to_string()))?;
    let (forme, _) = sortie[0]
        .try_extract_tensor::<f32>()
        .map_err(|e| ErreurLecture::Modele(e.to_string()))?;
    let dims: &[i64] = &forme[..];
    if dims.len() != 3 {
        return Err(ErreurLecture::FormeSortie {
            trouve: dims.len(),
            attendu: 3,
        });
    }
    Ok(dims[2] as usize)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{Rgb, RgbImage};

    fn ligne(largeur: u32, hauteur: u32, valeur: u8) -> RgbImage {
        RgbImage::from_pixel(largeur, hauteur, Rgb([valeur; 3]))
    }

    #[test]
    fn la_preparation_donne_la_hauteur_du_modele() {
        let (w, data) = preparer(&ligne(300, 60, 255)).expect("ligne valide");
        assert_eq!(w, 240, "la largeur suit le rapport, 300x60 -> 240x48");
        assert_eq!(data.len(), 3 * 240 * 48, "trois canaux, rien de plus");
    }

    #[test]
    fn les_valeurs_sont_bornees_entre_minus_un_et_un() {
        let (_, noir) = preparer(&ligne(64, 64, 0)).expect("ligne valide");
        let (_, blanc) = preparer(&ligne(64, 64, 255)).expect("ligne valide");
        assert!(
            noir.iter().all(|v| (*v - (-1.0)).abs() < 1e-6),
            "noir -> -1"
        );
        assert!(blanc.iter().all(|v| (*v - 1.0).abs() < 1e-6), "blanc -> +1");
        assert!(
            noir.iter().all(|v| (-1.0..=1.0).contains(v)),
            "aucune valeur hors de [-1, 1] : une entrée hors bornes est un défaut silencieux"
        );
    }

    #[test]
    fn une_ligne_trop_petite_est_refusee_nommer() {
        let err = preparer(&ligne(3, 40, 0)).expect_err("3 px de large");
        assert!(matches!(
            err,
            ErreurLecture::LigneTropPetite { largeur: 3, .. }
        ));
    }

    #[test]
    fn la_largeur_entree_ne_descend_jamois_a_zero() {
        // Une ligne très haute et étroite : l'arrondi donnerait 0, ce qui est
        // une forme invalide pour le modèle.
        let (w, _) = preparer(&ligne(8, 4000, 128)).expect("ligne valide");
        assert!(w >= 1, "largeur minimale respectée");
    }
}
