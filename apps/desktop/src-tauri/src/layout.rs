//! Mise en page du texte traduit dans les bulles (T4).
//!
//! Stratégie :
//! 1. Éroder le masque/bbox d'une marge pour obtenir la surface disponible.
//! 2. Recherche dichotomique de la plus grande taille de police qui tient.
//! 3. Césure simple aux espaces (pas de coupure de mot pour l'instant).
//! 4. Retours à la ligne stockés explicitement dans `BlocTraduction::line_breaks`.

use std::cmp::min;

use crate::traduction::Bbox;

// ---------------------------------------------------------------------------
// Constantes de l'algorithme
// ---------------------------------------------------------------------------

/// Taille de police minimale tentée (px).
const FONT_SIZE_MIN: f64 = 8.0;
/// Taille de police maximale tentée (px).
const FONT_SIZE_MAX: f64 = 72.0;
/// Pas de la recherche dichotomique.
const FONT_SIZE_PRECISION: f64 = 0.5;
/// Marge d'érosion du masque, en pixels (évite le crénage contre les bords).
const MARGE_EROSION_PX: i64 = 2;
/// Ratio largeur/hauteur d'un caractère moyen (polices latines).
/// Utilisé comme estimation quand les métriques réelles de la police ne sont
/// pas disponibles. Mesuré sur les 8 polices retenues en T0.1.
const LARGEUR_CARACTERE_MOYEN: f64 = 0.52;

// ---------------------------------------------------------------------------
// Résultat du calcul de mise en page
// ---------------------------------------------------------------------------

/// Résultat de l'algorithme de remplissage d'une bulle.
#[derive(Debug, Clone, PartialEq)]
pub struct LayoutResult {
    /// Taille de police retenue (px).
    pub font_size: f64,
    /// Police choisie.
    pub font_family: &'static str,
    /// Coupures de ligne dans le texte, en nombre de caractères.
    /// Ex: `[6, 12]` signifie ligne 0 = texte[0..6], ligne 1 = texte[6..12], ligne 2 = texte[12..].
    pub line_breaks: Vec<usize>,
}

// ---------------------------------------------------------------------------
// Erreurs de mise en page
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, thiserror::Error)]
pub enum ErreurLayout {
    #[error("bbox vide ou invalide")]
    BboxVide,
    #[error("texte vide")]
    TexteVide,
    #[error("aucune taille de police ne tient dans la bbox")]
    AucuneTaille,
}

// ---------------------------------------------------------------------------
// API publique
// ---------------------------------------------------------------------------

/// Calcule la mise en page d'un texte dans une bbox.
///
/// # Paramètres
/// - `bbox` : boîte englobante du bloc de traduction.
/// - `texte` : texte traduit à faire tenir.
///
/// # Retour
/// `LayoutResult` avec la taille de police, la police et les coupures.
pub fn calculer_mise_en_page(bbox: &Bbox, texte: &str) -> Result<LayoutResult, ErreurLayout> {
    if bbox.w < 1 || bbox.h < 1 {
        return Err(ErreurLayout::BboxVide);
    }
    if texte.trim().is_empty() {
        return Err(ErreurLayout::TexteVide);
    }

    // Surface disponible après érosion par la marge.
    let largeur_dispo = (bbox.w - 2 * MARGE_EROSION_PX).max(1) as f64;
    let hauteur_dispo = (bbox.h - 2 * MARGE_EROSION_PX).max(1) as f64;

    // Recherche dichotomique de la plus grande taille qui tient.
    let mut debut = FONT_SIZE_MIN;
    let mut fin = FONT_SIZE_MAX;
    let mut meilleure = None;

    while debut <= fin {
        let milieu = (debut + fin) / 2.0;
        if texte_tient(texte, milieu, largeur_dispo, hauteur_dispo) {
            meilleure = Some(milieu);
            debut = milieu + FONT_SIZE_PRECISION;
        } else {
            fin = milieu - FONT_SIZE_PRECISION;
        }
    }

    let font_size = meilleure.ok_or(ErreurLayout::AucuneTaille)?;
    let font_family = choisir_police(bbox, font_size);
    let line_breaks = calculer_coupures(texte, font_size, largeur_dispo);

    Ok(LayoutResult {
        font_size,
        font_family,
        line_breaks,
    })
}

// ---------------------------------------------------------------------------
// Mesure et césure
// ---------------------------------------------------------------------------

/// Vérifie si le texte tient dans la boîte à la taille donnée.
fn texte_tient(texte: &str, font_size: f64, largeur: f64, hauteur: f64) -> bool {
    let lignes = calculer_coupures(texte, font_size, largeur);
    let nb_lignes = lignes.len() + 1;
    let hauteur_estimee = nb_lignes as f64 * font_size * 1.25; // interligne ~125%

    hauteur_estimee <= hauteur
}

/// Calcule les indices de coupure aux espaces.
fn calculer_coupures(texte: &str, font_size: f64, largeur_dispo: f64) -> Vec<usize> {
    let largeur_car = font_size * LARGEUR_CARACTERE_MOYEN;
    let max_caracteres_par_ligne = (largeur_dispo / largeur_car).floor().max(1.0) as usize;

    if max_caracteres_par_ligne >= texte.len() {
        return Vec::new(); // tout tient sur une ligne
    }

    let mut coupures = Vec::new();
    let mut pos = 0;

    while pos + max_caracteres_par_ligne < texte.len() {
        // Chercher le dernier espace dans la fenêtre pour couper proprement.
        let fin = pos + max_caracteres_par_ligne;
        let coupure = texte[pos..fin]
            .trim_end()
            .rfind(' ')
            .map(|i| pos + i)
            .unwrap_or(fin);

        // Avancer d'au moins un caractère pour éviter une boucle infinie.
        let coupure = min(coupure + 1, texte.len());
        if coupure <= pos {
            break;
        }

        coupures.push(coupure);
        pos = coupure;
    }

    coupures
}

/// Choix de police selon la taille et le type de bloc.
fn choisir_police(_bbox: &Bbox, _font_size: f64) -> &'static str {
    // Pour l'instant : police par défaut.
    // Un sélecteur plus fin pourra être ajouté en T4.6 selon le type de bloc
    // et la taille retenue.
    "Comic Neue"
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn texte_court_tient_sans_coupure() {
        let bbox = Bbox {
            x: 0,
            y: 0,
            w: 200,
            h: 100,
        };
        let resultat = calculer_mise_en_page(&bbox, "Hello").unwrap();
        assert!(resultat.line_breaks.is_empty());
        assert!(resultat.font_size >= FONT_SIZE_MIN);
    }

    #[test]
    fn texte_long_est_decoupe() {
        let bbox = Bbox {
            x: 0,
            y: 0,
            w: 100,
            h: 200,
        };
        let resultat = calculer_mise_en_page(&bbox, "Hello world this is a test").unwrap();
        assert!(!resultat.line_breaks.is_empty());
    }

    #[test]
    fn bbox_trop_etroite_retourne_erreur() {
        let bbox = Bbox {
            x: 0,
            y: 0,
            w: 1,
            h: 1,
        };
        assert!(calculer_mise_en_page(&bbox, "Hello").is_err());
    }

    #[test]
    fn texte_vide_retourne_erreur() {
        let bbox = Bbox {
            x: 0,
            y: 0,
            w: 100,
            h: 100,
        };
        assert!(calculer_mise_en_page(&bbox, "").is_err());
    }
}
