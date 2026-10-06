//! Classification des blocs de texte détectés (T2.3).
//!
//! Chaque bloc reçoit un type parmi :
//! - `Dialogue` — bulle de dialogue personnage
//! - `Narration` — bulle de narration / description
//! - `Sfx` — onomatopées, bruitages
//! - `TexteLibre` — texte hors bulle (crédits, signature, etc.)
//! - `Ignore` — filigrane, logo, artefact à ne pas traduire
//!
//! La classification utilise des heuristiques sur la géométrie, la position
//! et le contenu OCR. Elle ne fait pas d'appel à un modèle externe.

use super::tile::Bbox;

/// Type de bloc de texte.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeBloc {
    Dialogue,
    Narration,
    Sfx,
    TexteLibre,
    Ignore,
}

/// Contexte d'une page pour la classification.
#[derive(Debug, Clone, Copy)]
pub struct ContextePage {
    /// Largeur de la page en pixels.
    pub largeur: u32,
    /// Hauteur de la page en pixels.
    pub hauteur: u32,
    /// Position Y de la marge de narration (typiquement en haut ou en bas).
    pub marge_narration_y: Option<u32>,
    /// Zone de crédits en bas de page (y_start, y_end), si connue.
    pub zone_credits: Option<(u32, u32)>,
}

/// Bloc détecté, prêt pour classification.
#[derive(Debug, Clone)]
pub struct Bloc {
    pub bbox: Bbox,
    pub texte: String,
    pub confiance: f32,
    pub masque_pixels: u64,
}

/// Résultat de classification.
#[derive(Debug, Clone)]
pub struct BlocClassifie {
    pub bloc: Bloc,
    pub type_bloc: TypeBloc,
    pub score_dialogue: f32,
    pub score_narration: f32,
    pub score_sfx: f32,
    pub score_ignore: f32,
}

/// Erreurs de classification.
#[derive(Debug, thiserror::Error)]
pub enum ErreurClassification {
    #[error("bbox vide")]
    BboxVide,
    #[error("contexte de page invalide")]
    ContexteInvalide,
}

/// Classifie un bloc selon sa géométrie et son contenu.
///
/// # Règles heuristiques
///
/// 1. **Ignore** : le bloc touche les bords de la page, ou il est dans la zone
///    de crédits, ou il contient moins de 2 caractères non espacés.
/// 2. **Sfx** : le texte est en MAJUSCULES, court (< 30 caractères), et dans
///    une boîte plutôt ronde (ratio w/h proche de 1).
/// 3. **Narration** : le bloc est dans la marge de narration, ou il est large
///    (> 60% de la page) et en haut/bas de page.
/// 4. **Dialogue** : tout le reste (boîte centrée, taille moyenne).
/// 5. **TexteLibre** : texte qui ne ressemble pas à un dialogue ni à du sfx,
///    typiquement des légendes ou des notes.
pub fn classer_bloc(
    bloc: &Bloc,
    ctx: &ContextePage,
) -> Result<BlocClassifie, ErreurClassification> {
    if bloc.bbox.w <= 0 || bloc.bbox.h <= 0 {
        return Err(ErreurClassification::BboxVide);
    }

    let texte = bloc.texte.trim();
    let longueur_texte = texte.chars().filter(|c| !c.is_whitespace()).count();

    // Scores initiaux.
    let mut score_dialogue = 0.0f32;
    let mut score_narration = 0.0f32;
    let mut score_sfx = 0.0f32;
    let mut score_ignore = 0.0f32;

    // Règle 1 : Ignore.
    if touches_bord(&bloc.bbox, ctx.largeur, ctx.hauteur) {
        score_ignore += 0.8;
    }
    if let Some((y_start, y_end)) = ctx.zone_credits {
        if bloc.bbox.y >= y_start as i64 && bloc.bbox.y + bloc.bbox.h <= y_end as i64 {
            score_ignore += 0.9;
        }
    }
    if longueur_texte < 2 && !texte.contains(' ') {
        score_ignore += 0.7;
    }

    // Règle 2 : Sfx.
    let ratio = bloc.bbox.w as f32 / bloc.bbox.h as f32;
    let est_court = texte.len() < 30;
    let est_maj = texte
        .chars()
        .all(|c| c.is_uppercase() || c.is_numeric() || c.is_ascii_punctuation());
    if est_court && est_maj && (0.5..=2.0).contains(&ratio) {
        score_sfx += 0.7;
    }

    // Règle 3 : Narration.
    if let Some(marge_y) = ctx.marge_narration_y {
        if bloc.bbox.y <= marge_y as i64 || bloc.bbox.y >= (ctx.hauteur as i64 - marge_y as i64) {
            score_narration += 0.6;
        }
    }
    let largeur_ratio = bloc.bbox.w as f32 / ctx.largeur as f32;
    if largeur_ratio > 0.6 {
        score_narration += 0.3;
    }
    if bloc.bbox.y < (ctx.hauteur as i64 / 4) || bloc.bbox.y > (ctx.hauteur as i64 * 3 / 4) {
        score_narration += 0.2;
    }

    // Règle 4 : Dialogue (défaut si rien d'autre ne domine).
    if score_dialogue == 0.0 && score_ignore < 0.5 {
        score_dialogue = 0.5;
    }

    // Normaliser les scores pour qu'ils somment à 1.0.
    let total = score_dialogue + score_narration + score_sfx + score_ignore;
    if total > 0.0 {
        score_dialogue /= total;
        score_narration /= total;
        score_sfx /= total;
        score_ignore /= total;
    }

    let type_bloc = if score_ignore >= score_dialogue
        && score_ignore >= score_narration
        && score_ignore >= score_sfx
    {
        TypeBloc::Ignore
    } else if score_sfx >= score_dialogue && score_sfx >= score_narration {
        TypeBloc::Sfx
    } else if score_narration >= score_dialogue {
        TypeBloc::Narration
    } else {
        TypeBloc::Dialogue
    };

    // Ajustement final pour TexteLibre.
    let type_bloc = if type_bloc == TypeBloc::Dialogue && longueur_texte > 100 {
        TypeBloc::TexteLibre
    } else {
        type_bloc
    };

    Ok(BlocClassifie {
        bloc: bloc.clone(),
        type_bloc,
        score_dialogue,
        score_narration,
        score_sfx,
        score_ignore,
    })
}

/// Vérifie si une bbox touche les bords de la page (dans une marge de 5 px).
fn touches_bord(bbox: &Bbox, largeur: u32, hauteur: u32) -> bool {
    const MARGE: i64 = 5;
    bbox.x <= MARGE
        || bbox.y <= MARGE
        || bbox.x + bbox.w >= (largeur as i64 - MARGE)
        || bbox.y + bbox.h >= (hauteur as i64 - MARGE)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn contexte_defaut() -> ContextePage {
        ContextePage {
            largeur: 800,
            hauteur: 1200,
            marge_narration_y: Some(100),
            zone_credits: Some((1100, 1200)),
        }
    }

    #[test]
    fn bloc_central_est_dialogue() {
        let bloc = Bloc {
            bbox: Bbox {
                x: 200,
                y: 400,
                w: 400,
                h: 200,
            },
            texte: "Hello world!".into(),
            confiance: 0.9,
            masque_pixels: 80000,
        };
        let res = classer_bloc(&bloc, &contexte_defaut()).unwrap();
        assert_eq!(res.type_bloc, TypeBloc::Dialogue);
    }

    #[test]
    fn bloc_en_marge_est_narration() {
        let bloc = Bloc {
            bbox: Bbox {
                x: 100,
                y: 20,
                w: 600,
                h: 60,
            },
            texte: "Chapter 1".into(),
            confiance: 0.8,
            masque_pixels: 36000,
        };
        let res = classer_bloc(&bloc, &contexte_defaut()).unwrap();
        assert_eq!(res.type_bloc, TypeBloc::Narration);
    }

    #[test]
    fn bloc_court_majuscule_est_sfx() {
        let bloc = Bloc {
            bbox: Bbox {
                x: 300,
                y: 500,
                w: 200,
                h: 100,
            },
            texte: "BOOM!".into(),
            confiance: 0.7,
            masque_pixels: 20000,
        };
        let res = classer_bloc(&bloc, &contexte_defaut()).unwrap();
        assert_eq!(res.type_bloc, TypeBloc::Sfx);
    }

    #[test]
    fn bloc_dans_credits_est_ignore() {
        let bloc = Bloc {
            bbox: Bbox {
                x: 300,
                y: 1150,
                w: 200,
                h: 40,
            },
            texte: "By Author".into(),
            confiance: 0.6,
            masque_pixels: 8000,
        };
        let res = classer_bloc(&bloc, &contexte_defaut()).unwrap();
        assert_eq!(res.type_bloc, TypeBloc::Ignore);
    }

    #[test]
    fn bloc_tres_long_est_texte_libre() {
        let bloc = Bloc {
            bbox: Bbox { x: 100, y: 400, w: 600, h: 300 },
            texte: "This is a very long text that exceeds one hundred characters and should be classified as free text because it is too long for a single dialogue bubble.".into(),
            confiance: 0.8,
            masque_pixels: 180000,
        };
        let res = classer_bloc(&bloc, &contexte_defaut()).unwrap();
        assert_eq!(res.type_bloc, TypeBloc::TexteLibre);
    }

    #[test]
    fn scores_somment_a_un() {
        let bloc = Bloc {
            bbox: Bbox {
                x: 200,
                y: 400,
                w: 400,
                h: 200,
            },
            texte: "Hello!".into(),
            confiance: 0.9,
            masque_pixels: 80000,
        };
        let res = classer_bloc(&bloc, &contexte_defaut()).unwrap();
        let sum = res.score_dialogue + res.score_narration + res.score_sfx + res.score_ignore;
        assert!(
            (sum - 1.0).abs() < 1e-6,
            "scores somment à {}, pas 1.0",
            sum
        );
    }
}
