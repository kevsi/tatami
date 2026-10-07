//! Détection des bulles et du texte libre sur une fenêtre de page.
//!
//! ## Mesures
//!
//! RT-DETR-V2 (`detector_int8.onnx`), trois classes : `bubble`, `text_bubble`,
//! `text_free`. Sur le corpus : 0,98 sur une page de dialogue, 0,33 sur une page
//! d'illustration — et cette dernière valeur est **correcte**, la page ne
//! contient aucune bulle.
//!
//! Deux causes ont été écartées **par la mesure**, pas par le raisonnement :
//!
//! - la **quantification** : fp32 0,350 contre int8 0,332 sur la même fenêtre ;
//! - l'**échelle** : agrandir ×2 puis ×3 ne change rien (0,308 → 0,364).
//!
//! Donc int8, qui est 3,8× plus léger à qualité égale.
//!
//! ## Forme des sorties
//!
//! Le modèle rend `labels`, `boxes` et `scores`. Les boîtes sont en
//! **centre / largeur / hauteur, en pixels de l'entrée 640×640** — pas
//! normalisées, et pas en x1,y1,x2,y2. Mesuré : la conversion se fait en
//! multipliant par `largeur_orig / 640`, et c'est ce qui donne les boîtes
//! plausibles relevées (178,149)-(819,789) sur une fenêtre de 800 px. Les deux
//! autres conventions ont déjà coûté une session à T0 : l'IoU 0,16 venait de
//! deux bugs de convention dans la sonde, pas d'un détecteur malade.

use crate::lignes::Rectangle;
use ort::session::Session;
use ort::value::Tensor;

/// Côté de la fenêtre d'entrée du détecteur, en pixels.
pub const COTE_FENETRE: u32 = 640;

/// Un côté d'image au-delà duquel le détecteur déforme et rend n'importe quoi.
///
/// Mesuré le 2026-10-07 : le détecteur travaille en 640×640. Une page de
/// 800×1000 est ramenée à cette taille, et les boîtes remontées font alors
/// 769×645 — plus grandes que la page. Le ratio compte donc autant que le côté.
pub const COTE_MAXIMAL: u32 = 1024;

/// Tolérance sur le rapport d'aspect. Une tuile est carrée ; une page de webtoon
/// ne l'est pas (mesuré : 800×1000, 690×1280).
pub const RATIO_MIN: f32 = 0.75;
pub const RATIO_MAX: f32 = 1.40;

/// Vrai si une image peut être passée au détecteur telle quelle.
///
/// Fonction pure : la règle est testée sans modèle, parce que le défaut qu'elle
/// évite — des boîtes absurdes, sans erreur — ne se voit qu'en regardant
/// l'image.
pub fn tuile_acceptable(largeur: u32, hauteur: u32) -> bool {
    if largeur == 0 || hauteur == 0 || largeur > COTE_MAXIMAL || hauteur > COTE_MAXIMAL {
        return false;
    }
    let ratio = largeur as f32 / hauteur as f32;
    ratio >= RATIO_MIN && ratio <= RATIO_MAX
}

/// Score minimal pour garder une détection.
///
/// Le triage des 36 pages du corpus a donné six pages au-dessus de 0,97 et une
/// page d'illustration à 0,33 : 0,50 sépare les deux groupes sans effort.
pub const SCORE_MINIMAL: f32 = 0.50;

/// Ce que le détecteur distingue. C'est la **classification de T2.3**, fournie
/// par le modèle : inutile de la réinventer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Classe {
    /// Une bulle de dialogue.
    Bulle,
    /// Du texte posé dans une bulle.
    TexteBulle,
    /// Du texte hors bulle : narration, effets sonores, crédits.
    TexteLibre,
}

impl Classe {
    /// Convertit l'identifiant de classe du modèle.
    ///
    /// Une classe inconnue devient `TexteLibre` : c'est la valeur **prudente**,
    /// car la politique retenue est de ne pas effacer hors bulle.
    pub fn depuis_id(id: i64) -> Classe {
        match id {
            0 => Classe::Bulle,
            1 => Classe::TexteBulle,
            _ => Classe::TexteLibre,
        }
    }

    /// L'identifiant de classe du modèle.
    pub fn id(self) -> i64 {
        match self {
            Classe::Bulle => 0,
            Classe::TexteBulle => 1,
            Classe::TexteLibre => 2,
        }
    }

    /// Vrai si le texte est dans une bulle, donc effaçable et traduisible en
    /// surcouche.
    pub fn dans_bulle(self) -> bool {
        matches!(self, Classe::Bulle | Classe::TexteBulle)
    }
}

/// Une détection.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Detection {
    pub boite: Rectangle,
    pub classe: Classe,
    pub score: f32,
}

/// Erreur de détection.
#[derive(Debug)]
pub enum ErreurDetection {
    Modele(String),
    FormeSortie {
        trouve: usize,
        attendu: usize,
    },
    /// Le nombre de détections ne concorde pas entre les trois sorties.
    SortiesIncoherentes {
        labels: usize,
        boxes: usize,
        scores: usize,
    },
}

impl std::fmt::Display for ErreurDetection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ErreurDetection::Modele(m) => write!(f, "détecteur de bulles : {m}"),
            ErreurDetection::FormeSortie { trouve, attendu } => write!(
                f,
                "la sortie du détecteur a {trouve} dimensions, {attendu} attendues"
            ),
            ErreurDetection::SortiesIncoherentes {
                labels,
                boxes,
                scores,
            } => write!(
                f,
                "sorties incohérentes : {labels} labels, {boxes} boîtes, {scores} scores"
            ),
        }
    }
}

impl std::error::Error for ErreurDetection {}

/// Les deux tailles mises en jeu : celle que le modèle voit, et celle de la
/// fenêtre d'origine. Les garde dans une structure, sinon les quatre paramètres
/// se mélangent d'un appel à l'autre — et une inversion ne se voit pas.
#[derive(Debug, Clone, Copy)]
pub struct Geometrie {
    pub largeur_entree: u32,
    pub hauteur_entree: u32,
    pub largeur_orig: u32,
    pub hauteur_orig: u32,
}

/// Convertit les boîtes centrées normalisées du modèle en rectangles dans les
/// coordonnées de l'image.
///
/// Fonction pure : c'est la conversion **`[cx, cy, w, h]` → `[x1, y1, x2, y2]`**
/// qui a été confondue avec l'autre convention, donc elle est testée seule.
pub fn boites_vers_rectangles(centres: &[f32], geo: Geometrie) -> Vec<Rectangle> {
    let facteur_x = geo.largeur_orig as f32 / geo.largeur_entree as f32;
    let facteur_y = geo.hauteur_orig as f32 / geo.hauteur_entree as f32;
    centres
        .as_chunks::<4>()
        .0
        .iter()
        .map(|b| {
            let (cx, cy, bw, bh) = (b[0], b[1], b[2], b[3]);
            Rectangle {
                x: (cx - bw / 2.0) * facteur_x,
                y: (cy - bh / 2.0) * facteur_y,
                largeur: bw * facteur_x,
                hauteur: bh * facteur_y,
            }
        })
        .collect()
}

/// Assamble les trois sorties du modèle en détections, filtrées et triées.
pub fn detections_depuis_sorties(
    labels: &[i64],
    boxes: &[f32],
    scores: &[f32],
    geo: Geometrie,
    score_minimal: f32,
) -> Result<Vec<Detection>, ErreurDetection> {
    if labels.len() != boxes.len() / 4 || labels.len() != scores.len() {
        return Err(ErreurDetection::SortiesIncoherentes {
            labels: labels.len(),
            boxes: boxes.len() / 4,
            scores: scores.len(),
        });
    }
    let rectangles = boites_vers_rectangles(boxes, geo);
    let mut detections: Vec<Detection> = labels
        .iter()
        .zip(rectangles)
        .zip(scores)
        .filter(|((_, r), s)| **s >= score_minimal && r.largeur > 0.0 && r.hauteur > 0.0)
        .map(|((&label, boite), &score)| Detection {
            boite,
            classe: Classe::depuis_id(label),
            score,
        })
        .collect();
    // Les plus sûrs d'abord : si le pipeline doit s'arrêter, il s'arrête sur le
    // meilleur plutôt que sur l'ordre du modèle, qui n'est pas défini.
    detections.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    Ok(detections)
}

/// Détecteur de bulles chargé en mémoire.
pub struct DetecteurBulles {
    session: Session,
}

impl DetecteurBulles {
    pub fn ouvrir(modele: &std::path::Path) -> Result<DetecteurBulles, ErreurDetection> {
        let mut builder = Session::builder().map_err(|e| ErreurDetection::Modele(e.to_string()))?;
        let session = builder
            .commit_from_file(modele)
            .map_err(|e| ErreurDetection::Modele(e.to_string()))?;
        Ok(DetecteurBulles { session })
    }

    /// Détecte sur une fenêtre de page et rend les détections dans ses
    /// coordonnées, triées par score décroissant.
    pub fn detecter(
        &mut self,
        fenetre: &image::RgbImage,
        score_minimal: f32,
    ) -> Result<Vec<Detection>, ErreurDetection> {
        let (largeur_orig, hauteur_orig) = (fenetre.width(), fenetre.height());
        if largeur_orig == 0 || hauteur_orig == 0 {
            return Ok(Vec::new());
        }
        let entree = image::imageops::resize(
            fenetre,
            COTE_FENETRE,
            COTE_FENETRE,
            image::imageops::FilterType::Triangle,
        );
        let mut data = vec![0f32; 3 * COTE_FENETRE as usize * COTE_FENETRE as usize];
        let cote = COTE_FENETRE as usize;
        // `do_normalize: false` dans le `preprocessor_config.json` du modèle :
        // une simple division par 255, sans moyenne ni écart-type. Mesuré :
        // appliquer ici la normalisation ImageNet fausseit toutes les boîtes.
        for (x, y, px) in entree.enumerate_pixels() {
            let (x, y) = (x as usize, y as usize);
            for c in 0..3usize {
                data[c * cote * cote + y * cote + x] = px[c] as f32 / 255.0;
            }
        }
        let images = Tensor::from_array((vec![1i64, 3, cote as i64, cote as i64], data))
            .map_err(|e| ErreurDetection::Modele(e.to_string()))?;
        // L'ordre de `orig_target_sizes` n'est pas observable quand la fenêtre
        // est carrée, donc il est transmis tel quel, sans être interprété.
        let tailles = Tensor::from_array((
            vec![1i64, 2],
            vec![COTE_FENETRE as i64, COTE_FENETRE as i64],
        ))
        .map_err(|e| ErreurDetection::Modele(e.to_string()))?;

        let sortie = self
            .session
            .run(ort::inputs!["images" => images, "orig_target_sizes" => tailles])
            .map_err(|e| ErreurDetection::Modele(e.to_string()))?;
        if sortie.len() < 3 {
            return Err(ErreurDetection::FormeSortie {
                trouve: sortie.len(),
                attendu: 3,
            });
        }

        let labels = sortie[0]
            .try_extract_tensor::<i64>()
            .map_err(|e| ErreurDetection::Modele(e.to_string()))?;
        let boxes = sortie[1]
            .try_extract_tensor::<f32>()
            .map_err(|e| ErreurDetection::Modele(e.to_string()))?;
        let scores = sortie[2]
            .try_extract_tensor::<f32>()
            .map_err(|e| ErreurDetection::Modele(e.to_string()))?;
        let (forme_labels, _) = labels;
        let (forme_boxes, _) = boxes;
        let (forme_scores, _) = scores;
        if forme_labels.len() != 2 || forme_boxes.len() != 3 || forme_scores.len() != 2 {
            return Err(ErreurDetection::FormeSortie {
                trouve: forme_labels.len(),
                attendu: 2,
            });
        }

        let n = forme_scores[1] as usize;
        detections_depuis_sorties(
            &labels.1[..n],
            &boxes.1[..n * 4],
            &scores.1[..n],
            Geometrie {
                largeur_entree: COTE_FENETRE,
                hauteur_entree: COTE_FENETRE,
                largeur_orig,
                hauteur_orig,
            },
            score_minimal,
        )
    }

    /// Détecte avec le score minimal mesuré.
    pub fn detecter_par_defaut(
        &mut self,
        fenetre: &image::RgbImage,
    ) -> Result<Vec<Detection>, ErreurDetection> {
        self.detecter(fenetre, SCORE_MINIMAL)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fenêtre carrée de 640 px vue par le modèle : le cas le plus courant,
    /// puisque les tuiles font 640×640 (T2.1).
    fn geo(largeur_orig: u32, hauteur_orig: u32) -> Geometrie {
        Geometrie {
            largeur_entree: 640,
            hauteur_entree: 640,
            largeur_orig,
            hauteur_orig,
        }
    }

    #[test]
    fn les_boites_centrees_sont_converties_en_coin_haut_gauche() {
        // Boîte de 200x200 px centrée en (320, 320) dans l'entrée 640 : elle
        // couvre (220,220)-(420,420). Si la conversion prenait le centre pour
        // un coin, on obtiendrait (320,320) — c'est exactement la confusion qui
        // a coûté une session à T0.
        let r = boites_vers_rectangles(&[320.0, 320.0, 200.0, 200.0], geo(640, 640));
        assert_eq!(r.len(), 1);
        assert!((r[0].x - 220.0).abs() < 1e-3, "x = 220");
        assert!((r[0].y - 220.0).abs() < 1e-3, "y = 220");
        assert!((r[0].largeur - 200.0).abs() < 1e-3, "largeur = 200");
        assert!((r[0].hauteur - 200.0).abs() < 1e-3, "hauteur = 200");
    }

    #[test]
    fn la_boite_est_remise_a_l_echelle_de_la_fenetre() {
        // Fenêtre d'origine 1280 pour une entrée de 640 : tout est doublé.
        let r = boites_vers_rectangles(&[320.0, 320.0, 200.0, 100.0], geo(1280, 1280));
        assert!((r[0].x - 440.0).abs() < 1e-3, "x = 440");
        assert!((r[0].y - 540.0).abs() < 1e-3, "y = 540");
        assert!((r[0].largeur - 400.0).abs() < 1e-3, "largeur = 400");
        assert!((r[0].hauteur - 200.0).abs() < 1e-3, "hauteur = 200");
    }

    #[test]
    fn la_classe_inconnue_devient_du_texte_libre() {
        // Prudence : hors bulle, la politique est de ne pas effacer.
        assert_eq!(Classe::depuis_id(0), Classe::Bulle);
        assert_eq!(Classe::depuis_id(1), Classe::TexteBulle);
        assert_eq!(Classe::depuis_id(2), Classe::TexteLibre);
        assert_eq!(Classe::depuis_id(99), Classe::TexteLibre);
        assert_eq!(Classe::depuis_id(-1), Classe::TexteLibre);
    }

    #[test]
    fn seule_la_classe_bulle_est_effacable() {
        assert!(Classe::Bulle.dans_bulle());
        assert!(Classe::TexteBulle.dans_bulle());
        assert!(!Classe::TexteLibre.dans_bulle());
    }

    #[test]
    fn les_scores_faibles_sont_ecartes_et_les_autres_tries() {
        let detections = detections_depuis_sorties(
            &[0, 2, 1],
            &[
                0.5, 0.5, 0.4, 0.4, // 0.62
                0.2, 0.2, 0.1, 0.1, // 0.98
                0.8, 0.8, 0.2, 0.2, // 0.31
            ],
            &[0.62, 0.98, 0.31],
            geo(640, 640),
            SCORE_MINIMAL,
        )
        .expect("sorties cohérentes");
        assert_eq!(detections.len(), 2, "le score de 0,31 est écarté");
        assert!(
            (detections[0].score - 0.98).abs() < 1e-6,
            "le meilleur d'abord"
        );
        assert_eq!(detections[0].classe, Classe::TexteLibre);
        assert_eq!(detections[1].classe, Classe::Bulle);
    }

    #[test]
    fn des_sorties_incoherentes_sont_refusees() {
        let err = detections_depuis_sorties(
            &[0, 1],
            &[0.5, 0.5, 0.1, 0.1], // 1 boîte pour 2 labels
            &[0.9, 0.9],
            geo(640, 640),
            SCORE_MINIMAL,
        )
        .expect_err("incohérent");
        assert!(matches!(err, ErreurDetection::SortiesIncoherentes { .. }));
    }

    #[test]
    fn une_boite_de_largeur_nulle_est_ecartee() {
        let detections = detections_depuis_sorties(
            &[0],
            &[0.5, 0.5, 0.0, 0.4],
            &[0.95],
            geo(640, 640),
            SCORE_MINIMAL,
        )
        .expect("sorties cohérentes");
        assert!(
            detections.is_empty(),
            "une boîte dégénérée n'est pas une bulle"
        );
    }

    #[test]
    fn la_conversion_des_classes_est_bijective() {
        for id in 0..3 {
            assert_eq!(Classe::depuis_id(id).id(), id);
        }
    }
}
