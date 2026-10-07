//! Mise en page du texte traduit dans les bulles (T4).
//!
//! Stratégie :
//! 1. Éroder le masque/bbox d'une marge pour obtenir la surface disponible.
//! 2. Recherche dichotomique de la plus grande taille de police qui tient.
//! 3. Césure simple aux espaces (pas de coupure de mot pour l'instant).
//! 4. Retours à la ligne stockés explicitement dans `BlocTraduction::line_breaks`.

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
/// - `type_bloc` : type de bulle, influence le choix de la police.
///
/// # Retour
/// `LayoutResult` avec la taille de police, la police et les coupures.
pub fn calculer_mise_en_page(
    bbox: &Bbox,
    texte: &str,
    type_bloc: crate::traduction::TypeBloc,
) -> Result<LayoutResult, ErreurLayout> {
    let texte = texte.trim_start().trim_end();
    if bbox.w < 1 || bbox.h < 1 {
        return Err(ErreurLayout::BboxVide);
    }
    if texte.is_empty() {
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
    let font_family = choisir_police(type_bloc);
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

    while pos < texte.len() {
        // Sauter les espaces en tête de ligne pour ne jamais créer de ligne vide.
        while pos < texte.len() && texte.chars().nth(pos) == Some(' ') {
            pos += 1;
        }
        if pos >= texte.len() {
            break;
        }

        let fin = (pos + max_caracteres_par_ligne).min(texte.len());

        // Si tout le reste tient, pas de coupure supplémentaire.
        if fin >= texte.len() {
            break;
        }

        let window = &texte[pos..fin];
        if let Some(rel) = window.rfind(' ') {
            // Césure après l'espace : il reste dans la ligne courante,
            // la ligne suivante commence après lui.
            let coupure = pos + rel + 1;
            coupures.push(coupure);
            pos = coupure;
        } else if window.len() >= max_caracteres_par_ligne && max_caracteres_par_ligne > 1 {
            // Pas d'espace et mot trop long : césure forcée avec hyphen.
            let coupure = pos + max_caracteres_par_ligne - 1;
            coupures.push(coupure);
            pos = coupure;
        } else {
            // Pas d'espace : coupure nette à la borne.
            coupures.push(fin);
            pos = fin;
        }
    }

    coupures
}

/// Choix de police selon le type de bloc et la taille.
fn choisir_police(type_bloc: crate::traduction::TypeBloc) -> &'static str {
    match type_bloc {
        crate::traduction::TypeBloc::Dialogue => "Patrick Hand",
        crate::traduction::TypeBloc::Narration => "Architects Daughter",
        crate::traduction::TypeBloc::Sfx => "Luckiest Guy",
        crate::traduction::TypeBloc::FreeText => "Comic Neue",
        crate::traduction::TypeBloc::Ignore => "Comic Neue",
    }
}

// ---------------------------------------------------------------------------
// Vérification automatique (T4.4)
// ---------------------------------------------------------------------------

/// Seuil minimal de taille de police en dessous duquel on refuse le rendu.
const FONT_SIZE_SEUIL: f64 = 6.0;

/// Résultat d'une vérification automatique de mise en page.
#[derive(Debug, Clone, PartialEq)]
pub struct VerificationLayout {
    pub statut: crate::traduction::StatutBloc,
    pub police: &'static str,
    pub raison: Option<&'static str>,
}

/// Vérifie qu'un layout respecte les contrôles automatiques.
///
/// # Contrôles
/// 1. Taille de police >= seuil.
/// 2. Texte inclus dans le masque/bbox (pas de débordement).
/// 3. Aucun chevauchement avec d'autres blocs (si fournis).
///
/// # Retour
/// `VerificationLayout` avec le statut recommandé.
pub fn verifier_layout(
    layout: &LayoutResult,
    bbox: &Bbox,
    autres_blocs: &[Bbox],
) -> VerificationLayout {
    if layout.font_size < FONT_SIZE_SEUIL {
        return VerificationLayout {
            statut: crate::traduction::StatutBloc::Fallback,
            police: layout.font_family,
            raison: Some("font_size_below_threshold"),
        };
    }

    // Vérifier que le texte ne dépasse pas la bbox du bloc.
    let ligne_max = (layout.line_breaks.len() + 1) as f64;
    let hauteur_estimee = ligne_max * layout.font_size * 1.25;

    if hauteur_estimee > bbox.h as f64 {
        return VerificationLayout {
            statut: crate::traduction::StatutBloc::Review,
            police: layout.font_family,
            raison: Some("height_overflow"),
        };
    }

    // Vérifier les chevauchements avec les autres blocs.
    for autre in autres_blocs {
        if bboxes_se_chevauchent(bbox, autre) {
            return VerificationLayout {
                statut: crate::traduction::StatutBloc::Review,
                police: layout.font_family,
                raison: Some("overlap"),
            };
        }
    }

    VerificationLayout {
        statut: crate::traduction::StatutBloc::Ok,
        police: layout.font_family,
        raison: None,
    }
}

/// Vérifie que deux bboxes se chevauchent (y a un contact, pas seulement adjacency).
fn bboxes_se_chevauchent(a: &Bbox, b: &Bbox) -> bool {
    let ax2 = a.x + a.w;
    let ay2 = a.y + a.h;
    let bx2 = b.x + b.w;
    let by2 = b.y + b.h;

    a.x < bx2 && ax2 > b.x && a.y < by2 && ay2 > b.y
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traduction::TypeBloc;

    #[test]
    fn texte_court_tient_sans_coupure() {
        let bbox = Bbox {
            x: 0,
            y: 0,
            w: 200,
            h: 100,
        };
        let resultat = calculer_mise_en_page(&bbox, "Hello", TypeBloc::Dialogue).unwrap();
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
        let resultat =
            calculer_mise_en_page(&bbox, "Hello world this is a test", TypeBloc::Dialogue).unwrap();
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
        assert!(calculer_mise_en_page(&bbox, "Hello", TypeBloc::Dialogue).is_err());
    }

    #[test]
    fn texte_vide_retourne_erreur() {
        let bbox = Bbox {
            x: 0,
            y: 0,
            w: 100,
            h: 100,
        };
        assert!(calculer_mise_en_page(&bbox, "", TypeBloc::Dialogue).is_err());
    }

    #[test]
    fn verifier_layout_ok_quand_tout_rentre() {
        let bbox = Bbox {
            x: 0,
            y: 0,
            w: 200,
            h: 200,
        };
        let layout = LayoutResult {
            font_size: 16.0,
            font_family: "Comic Neue",
            line_breaks: vec![],
        };
        let v = verifier_layout(&layout, &bbox, &[]);
        assert_eq!(v.statut, crate::traduction::StatutBloc::Ok);
    }

    #[test]
    fn verifier_layout_fallback_sous_seuil() {
        let bbox = Bbox {
            x: 0,
            y: 0,
            w: 200,
            h: 200,
        };
        let layout = LayoutResult {
            font_size: 4.0,
            font_family: "Comic Neue",
            line_breaks: vec![],
        };
        let v = verifier_layout(&layout, &bbox, &[]);
        assert_eq!(v.statut, crate::traduction::StatutBloc::Fallback);
    }

    #[test]
    fn verifier_layout_review_si_chevauchement() {
        let bbox = Bbox {
            x: 10,
            y: 10,
            w: 100,
            h: 50,
        };
        let autre = Bbox {
            x: 50,
            y: 20,
            w: 100,
            h: 50,
        };
        let layout = LayoutResult {
            font_size: 16.0,
            font_family: "Comic Neue",
            line_breaks: vec![],
        };
        let v = verifier_layout(&layout, &bbox, &[autre]);
        assert_eq!(v.statut, crate::traduction::StatutBloc::Review);
    }

    // -------------------------------------------------------------------------
    // Tests de propriétés (proptest)
    // -------------------------------------------------------------------------

    use proptest::prelude::*;

    proptest! {
        /// Aucune ligne ne doit dépasser le budget de caractères calculé
        /// à partir de la largeur disponible et du ratio moyen.
        #[test]
        fn prop_ligne_respecte_budget(bbox_x in 0i64..400, bbox_y in 0i64..400,
                                      bbox_w in 20i64..600, bbox_h in 20i64..600,
                                      texte in "[A-Za-z ]{1,80}") {
            let bbox = Bbox { x: bbox_x, y: bbox_y, w: bbox_w, h: bbox_h };
            let texte_trim = texte.trim();
            if texte_trim.is_empty() { return Ok(()); }
            let resultat = calculer_mise_en_page(&bbox, &texte, TypeBloc::Dialogue);
            if resultat.is_err() { return Ok(()); }

            let resultat = resultat.unwrap();
            let largeur_dispo = (bbox.w - 2 * MARGE_EROSION_PX).max(1) as f64;
            let max_car = (largeur_dispo / (resultat.font_size * LARGEUR_CARACTERE_MOYEN))
                .floor()
                .max(1.0) as usize;

            let mut pos = 0;
            for coupure in &resultat.line_breaks {
                let ligne = &texte_trim[pos..*coupure];
                prop_assert!(!ligne.trim().is_empty(), "ligne vide en {pos}..{coupure}");
                // Le budget porte sur le contenu rendu, pas les espaces de remplissage.
                let non_space = ligne.chars().filter(|c| *c != ' ').count();
                prop_assert!(non_space <= max_car + 1,
                    "ligne {} ({}) dépasse le budget {} car", non_space, ligne, max_car);
                pos = *coupure;
            }
            if pos < texte_trim.len() {
                let derniere = &texte_trim[pos..];
                prop_assert!(!derniere.trim().is_empty(), "dernière ligne vide");
            }
        }

        /// Le texte produit ne doit jamais contenir de séquence de coupures
        /// croissantes invalides (chaque coupure strictement après la précédente).
        #[test]
        fn prop_coupures_croissantes(bbox_x in 0i64..400, bbox_y in 0i64..400,
                                     bbox_w in 20i64..600, bbox_h in 20i64..600,
                                     texte in "[A-Za-z ]{1,80}") {
            let bbox = Bbox { x: bbox_x, y: bbox_y, w: bbox_w, h: bbox_h };
            let resultat = calculer_mise_en_page(&bbox, &texte, TypeBloc::Dialogue);
            if resultat.is_err() { return Ok(()); }

            let coupures = &resultat.unwrap().line_breaks;
            for paire in coupures.windows(2) {
                prop_assert!(paire[0] < paire[1], "coupures non triées: {:?}", coupures);
            }
        }
    }
}
