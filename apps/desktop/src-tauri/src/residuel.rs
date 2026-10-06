//! Contrôle de résidu après effacement (T2.6).
//!
//! Cette module vérifie que le texte a bien été effacé en faisant un
//! second passage d'OCR sur la zone nettoyée. Si du texte est encore
//! détecté, le bloc passe en `review` pour validation humaine.

use super::ocr::BlocOcr;
use super::tile::Bbox;

/// Résultat du contrôle de résidu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResultatResiduel {
    /// La zone est propre, aucun texte détecté.
    Propre,
    /// Du texte résiduel a été détecté, nécessite une validation humaine.
    Review,
}

/// Vérifie le résidu OCR sur une zone nettoyée.
///
/// # Paramètres
/// - `lignes_apres` : lignes OCR détectées sur la zone nettoyée
/// - `bbox_originale` : la bbox de la bulle avant effacement
///
/// # Retour
/// `Propre` si aucune ligne n'est détectée, `Review` sinon.
pub fn verifier_residu(lignes_apres: &[BlocOcr], bbox_originale: &Bbox) -> ResultatResiduel {
    if lignes_apres.is_empty() {
        return ResultatResiduel::Propre;
    }

    // Vérifier que les lignes détectées sont bien dans la zone effacée.
    for ligne in lignes_apres {
        if ligne.bbox.x >= bbox_originale.x
            && ligne.bbox.y >= bbox_originale.y
            && ligne.bbox.x + ligne.bbox.w <= bbox_originale.x + bbox_originale.w
            && ligne.bbox.y + ligne.bbox.h <= bbox_originale.y + bbox_originale.h
        {
            // Une ligne a été détectée dans la zone effacée = résidu.
            return ResultatResiduel::Review;
        }
    }

    ResultatResiduel::Propre
}

/// Erreurs de contrôle de résidu.
#[derive(Debug, thiserror::Error)]
pub enum ErreurResiduel {
    #[error("bbox invalide")]
    BboxInvalide,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zone_propre_retourne_propre() {
        let lignes: Vec<BlocOcr> = vec![];
        let bbox = Bbox {
            x: 10,
            y: 10,
            w: 100,
            h: 50,
        };
        assert_eq!(verifier_residu(&lignes, &bbox), ResultatResiduel::Propre);
    }

    #[test]
    fn ligne_dans_zone_retourne_review() {
        let lignes = vec![BlocOcr {
            texte: "RESIDUAL".into(),
            bbox: Bbox {
                x: 20,
                y: 20,
                w: 50,
                h: 20,
            },
            lignes: vec![],
            score_confiance: 1.0,
        }];
        let bbox = Bbox {
            x: 10,
            y: 10,
            w: 100,
            h: 50,
        };
        assert_eq!(verifier_residu(&lignes, &bbox), ResultatResiduel::Review);
    }

    #[test]
    fn ligne_hors_zone_retourne_propre() {
        let lignes = vec![BlocOcr {
            texte: "OUTSIDE".into(),
            bbox: Bbox {
                x: 200,
                y: 200,
                w: 50,
                h: 20,
            },
            lignes: vec![],
            score_confiance: 1.0,
        }];
        let bbox = Bbox {
            x: 10,
            y: 10,
            w: 100,
            h: 50,
        };
        assert_eq!(verifier_residu(&lignes, &bbox), ResultatResiduel::Propre);
    }
}
