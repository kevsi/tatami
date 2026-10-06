//! Effacement du texte dans les bulles (T2.5).
//!
//! Cette module orchestre l'effacement du texte détecté :
//! 1. Pour chaque bloc classifié `Dialogue` ou `Narration` :
//!    - générer un masque érodé
//!    - appeler le sidecar vision `inpaint`
//!    - récupérer l'image nettoyée
//! 2. Pour les blocs `Sfx`, `TexteLibre`, `Ignore` :
//!    - policy ADR 0010 : on n'efface pas, on affiche en légende
//!
//! L'effacement est fait par tuile pour respecter le budget mémoire.


use super::classifier::{BlocClassifie, TypeBloc};
use super::mask::{eroder, masque_bbox, Masque};
use super::tile::Bbox;
use super::vision::InpaintResult;

/// Stratégie d'effacement pour un bloc.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StrategieEffacement {
    /// Bulle de dialogue ou narration : effacer le texte.
    Effacer,
    /// Texte hors bulle (sfx, légende) : conserver, afficher en surcouche.
    Conserver,
    /// Filigrane / artefact : ignorer complètement.
    Ignorer,
}

/// Résultat d'effacement pour un bloc.
#[derive(Debug, Clone)]
pub struct ResultatEffacement {
    pub strategie: StrategieEffacement,
    pub masque: Option<Masque>,
    pub image_nette: Option<Vec<u8>>,
    pub content_type: Option<String>,
}

/// Détermine la stratégie d'effacement selon le type de bloc.
pub fn strategie_pour_type(type_bloc: TypeBloc) -> StrategieEffacement {
    match type_bloc {
        TypeBloc::Dialogue | TypeBloc::Narration => StrategieEffacement::Effacer,
        TypeBloc::Sfx | TypeBloc::TexteLibre => StrategieEffacement::Conserver,
        TypeBloc::Ignore => StrategieEffacement::Ignorer,
    }
}

/// Prépare le masque d'effacement pour un bloc.
///
/// Le masque est érodé de 2 px pour ne pas effacer le contour de la bulle.
pub fn preparer_masque(page_width: u32, page_height: u32, bbox: &Bbox) -> Result<Masque, ErreurEffacement> {
    let masque = masque_bbox(page_width, page_height, bbox);
    eroder(&masque, 2).map_err(|e| ErreurEffacement::Masque(e.to_string()))
}

/// Appelle le sidecar vision pour effacer le texte dans une tuile.
///
/// # Paramètres
/// - `masque` : masque binaire (blanc = à effacer)
/// - `inpaint_fn` : fonction qui appelle le sidecar (pour testabilité)
///
/// # Retour
/// L'image nettoyée (base64 PNG) ou une erreur.
pub async fn effacer_tuile<F>(
    _masque: &Masque,
    inpaint_fn: F,
) -> Result<InpaintResult, ErreurEffacement>
where
    F: std::future::Future<Output = Result<InpaintResult, String>>,
{
    let result = inpaint_fn
        .await
        .map_err(ErreurEffacement::Inpaint)?;

    Ok(result)
}

/// Traite un bloc selon sa stratégie d'effacement.
pub async fn traiter_bloc<F>(
    bloc: &BlocClassifie,
    page_width: u32,
    page_height: u32,
    inpaint_fn: F,
) -> Result<ResultatEffacement, ErreurEffacement>
where
    F: std::future::Future<Output = Result<InpaintResult, String>>,
{
    let strategie = strategie_pour_type(bloc.type_bloc);

    match strategie {
        StrategieEffacement::Effacer => {
            let masque = preparer_masque(page_width, page_height, &bloc.bloc.bbox)?;
            let result = effacer_tuile(&masque, inpaint_fn).await?;
            Ok(ResultatEffacement {
                strategie,
                masque: Some(masque),
                image_nette: Some(result.data.into_bytes()),
                content_type: Some(result.content_type),
            })
        }
        StrategieEffacement::Conserver => Ok(ResultatEffacement {
            strategie,
            masque: None,
            image_nette: None,
            content_type: None,
        }),
        StrategieEffacement::Ignorer => Ok(ResultatEffacement {
            strategie,
            masque: None,
            image_nette: None,
            content_type: None,
        }),
    }
}

/// Erreurs d'effacement.
#[derive(Debug, thiserror::Error)]
pub enum ErreurEffacement {
    #[error("masque invalide : {0}")]
    Masque(String),
    #[error("appel inpaint échoué : {0}")]
    Inpaint(String),
    #[error("base64 invalide")]
    Base64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strategie_dialogue_est_effacer() {
        assert_eq!(
            strategie_pour_type(TypeBloc::Dialogue),
            StrategieEffacement::Effacer
        );
    }

    #[test]
    fn strategie_narration_est_effacer() {
        assert_eq!(
            strategie_pour_type(TypeBloc::Narration),
            StrategieEffacement::Effacer
        );
    }

    #[test]
    fn strategie_sfx_est_conserver() {
        assert_eq!(
            strategie_pour_type(TypeBloc::Sfx),
            StrategieEffacement::Conserver
        );
    }

    #[test]
    fn strategie_ignore_est_ignorer() {
        assert_eq!(
            strategie_pour_type(TypeBloc::Ignore),
            StrategieEffacement::Ignorer
        );
    }

    #[test]
    fn preparer_masque_eroide() {
        let masque = preparer_masque(100, 100, &Bbox { x: 10, y: 10, w: 20, h: 20 }).unwrap();
        // 20x20 - érosion 2px = 16x16 = 256.
        assert_eq!(masque.compter_blancs(), 16 * 16);
    }
}
