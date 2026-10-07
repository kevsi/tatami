//! La chaîne de vision côté application : une page → des blocs de texte.
//!
//! C'est le morceau qui manquait. Le sidecar sait détecter, lire et effacer ;
//! `ocr.rs` sait regrouper les lignes en blocs et les ordonner ; mais personne
//! ne faisait le lien, et l'interface envoyait `blocks: []`.
//!
//! ## L'ordre, et pourquoi il est dans cet ordre
//!
//! ```text
//! page (jeton) → octets → tuiles carrées 640
//!   → detect par tuile → boîtes bornées sur la page
//!   → ocr par bulle → lignes (déjà en repère page)
//!   → traiter_ocr (regroupement + ordre de lecture)
//!   → BlocTraduction avec type_bloc mesuré
//! ```
//!
//! ## Trois décisions mesurées
//!
//! 1. **Les tuiles sont carrées, avec bourrage.** Mesuré le 2026-10-07 : le
//!    détecteur travaille en 640×640, et une page de 800×1000 passée telle
//!    quelle rend des boîtes de 769×645 — plus grandes que la page. Couper en
//!    bandes donnerait une tuile finale de 640×360, de ratio 1,78, que le
//!    détecteur déforme autant. Donc on complète en carré.
//!
//! 2. **Le regroupement se fait par bulle, pas sur toute la page.** `ocr`
//!    rend ses lignes déjà en coordonnées page, donc une culture serait
//!    inutile. Et la classe appartient à la bulle : regrouper par page
//!    mélangerait le type d'un bloc avec celui de son voisin.
//!
//! 3. **Le `status` d'un bloc analysé est `review`, jamais `ok`.** Rien n'a
//!    encore été vérifié : `ok` promet une mise en page validée (T4.4), et un
//!    mensonge là-dessus afficherait du texte qui déborde.

use std::sync::Arc;

use base64::Engine;
use serde_json::Value;

use crate::ocr::traiter_ocr;
use crate::pagecache::PageCache;
use crate::traduction::{Bbox, BlocTraduction, PageTranslation, StatutBloc, TypeBloc};
use crate::vision::{ClasseBloc, DetectResult, OcrResult, VisionBbox};
use crate::vision_superviseur::{SuperviseurVision, DELAI_DETECT, DELAI_OCR};

/// Côté des tuiles envoyées au détecteur.
pub const COTE_TUILE: u32 = 640;

/// Regroupement des lignes d'une bulle, en pixels.
///
/// Mesuré : les lignes d'une bulle réelle sont espacées de 24 à 25 px
/// (y = 509, 533, 557, 581, 604) et font 9 à 14 px de haut. 50 px regroupe la
/// bulle entière sans jamais fusionner deux bulles distinctes.
pub const SEUIL_PROXIMITE: i64 = 50;

/// Côté minimal d'une bulle retenue, en pixels. Sous ce seuil, il s'agit de
/// bruit ou d'un accent isolé.
pub const TAILLE_MIN_BULLE: i64 = 40;

/// Erreur de la chaîne de vision.
#[derive(Debug, thiserror::Error)]
pub enum ErreurVision {
    #[error("page inconnue ou déjà évacuée du cache : {0}")]
    PageAbsente(String),
    #[error("image illisible : {0}")]
    ImageIllisible(String),
    #[error("sidecar vision : {0}")]
    Sidecar(String),
    #[error("aucun texte détecté sur cette page")]
    AucunBloc,
}

// ---------------------------------------------------------------------------
// Partie pure — testable sans modèle, sans sidecar, sans image
// ---------------------------------------------------------------------------

/// Une tuile carrée, ancrée sur un coin de page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tuile {
    pub x: u32,
    pub y: u32,
}

/// Découpe la page en grille de tuiles **carrées** de `cote`.
///
/// La dernière ligne et la dernière colonne débordent : c'est voulu, le
/// bourrage se fait à l'extraction. Une page de 800×1000 donne donc deux tuiles
/// de 640×640, la seconde à moitié vide — mesuré, c'est ce qui garde le ratio
/// à 1,0 et donc des boîtes exploitables.
pub fn tuiles_carrees(largeur: u32, hauteur: u32, cote: u32) -> Vec<Tuile> {
    if largeur == 0 || hauteur == 0 || cote == 0 {
        return Vec::new();
    }
    let mut tuiles = Vec::new();
    let mut y = 0;
    while y < hauteur {
        let mut x = 0;
        while x < largeur {
            tuiles.push(Tuile { x, y });
            x += cote;
        }
        y += cote;
    }
    tuiles
}

/// Ramène une boîte dans la page, et dit si elle survit au filtrage.
///
/// Les boîtes du détecteur débordent **volontairement** de leur tuile : mesuré,
/// une bulle donne `(-144, -46, 359, 214)` sur une tuile de 640. Sans bornage,
/// la culture envoyée à `ocr` sort de l'image et ne rend rien — silencieusement.
/// Une fois bornée, une boîte trop petite est un bruit et se perd.
pub fn borner(b: &VisionBbox, largeur: u32, hauteur: u32, taille_min: i64) -> Option<VisionBbox> {
    let x1 = b.x.max(0);
    let y1 = b.y.max(0);
    let x2 = (b.x + b.w).min(largeur as i64);
    let y2 = (b.y + b.h).min(hauteur as i64);
    let (w, h) = (x2 - x1, y2 - y1);
    if w < taille_min || h < taille_min {
        return None;
    }
    Some(VisionBbox { x: x1, y: y1, w, h })
}

/// Une détection, avec ce qu'il faut pour la dédoublonner sans perdre la classe.
///
/// Pas de `Copy` : `ClasseBloc` ne l'implémente pas. Ce n'est pas un détail —
/// dupliquer la classe par valeur serait exactement le genre de copie qui
/// diverge.
#[derive(Debug, Clone, PartialEq)]
pub struct Detection {
    pub bbox: VisionBbox,
    pub classe: ClasseBloc,
    pub score: f64,
}

/// Distance maximale pour considérer que deux boîtes sont la même bulle.
pub const SEUIL_FUSION: i64 = 24;

/// Fusionne les détections d'une même bulle vues dans deux tuiles.
///
/// Une bulle à cheval sur une jonction est détectée **deux fois** : une fois
/// tronquée à gauche, une fois tronquée à droite. Sans fusion, la page affiche
/// le même texte deux fois, dans deux blocs.
///
/// Pourquoi pas `tile::DetectionFusion` : il ne transporte qu'une `Bbox`, donc
/// il perd la classe — or la classe **est** la classification de T2.3, et la
/// perdre obligerait à la deviner. Ici la classe conservée est celle de la
/// détection la plus sûre du groupe.
///
/// Le tri est par score décroissant, donc le premier élément d'un groupe est le
/// plus fiable et sa classe l'emporte.
pub fn fusionner(mut detections: Vec<Detection>, seuil: i64) -> Vec<Detection> {
    detections.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let mut resultats: Vec<Detection> = Vec::new();
    for d in detections {
        match resultats.last_mut() {
            Some(existant) if se_touchent(&existant.bbox, &d.bbox, seuil) => {
                existant.bbox = englobe(&existant.bbox, &d.bbox);
            }
            _ => resultats.push(d),
        }
    }
    resultats
}

fn se_touchent(a: &VisionBbox, b: &VisionBbox, seuil: i64) -> bool {
    let chevauchement_x = a.x < b.x + b.w + seuil && b.x < a.x + a.w + seuil;
    let chevauchement_y = a.y < b.y + b.h + seuil && b.y < a.y + a.h + seuil;
    chevauchement_x && chevauchement_y
}

fn englobe(a: &VisionBbox, b: &VisionBbox) -> VisionBbox {
    let x1 = a.x.min(b.x);
    let y1 = a.y.min(b.y);
    let x2 = (a.x + a.w).max(b.x + b.w);
    let y2 = (a.y + a.h).max(b.y + b.h);
    VisionBbox {
        x: x1,
        y: y1,
        w: x2 - x1,
        h: y2 - y1,
    }
}

/// Le type de bloc du schéma correspondant à la classe du modèle.
///
/// Le schéma connaît cinq types ; le détecteur n'en donne que trois. `sfx` et
/// `ignore` demandent des critères que T2.3 n'a pas encore posés (ADR 0011),
/// donc **rien ne les invente** : un bloc dont la classe ne suffit pas reste
/// `review` et le contrôle de résidu tranche.
pub fn type_de_bloc(classe: ClasseBloc) -> TypeBloc {
    match classe {
        // `bubble` : une bulle de dialogue.
        ClasseBloc::Bulle => TypeBloc::Dialogue,
        // `text_bubble` : texte posé dans un cartouche — narration, pas parole.
        ClasseBloc::TexteBulle => TypeBloc::Narration,
        // `text_free` : hors bulle. Non effacé (politique prudente, ADR 0011).
        ClasseBloc::TexteLibre => TypeBloc::FreeText,
    }
}

/// Les blocs d'une bulle, prêts pour la traduction.
pub fn blocs_de_bulle(
    lignes: &[crate::vision::VisionOcrLine],
    classe: ClasseBloc,
    ordre_depart: usize,
) -> Vec<BlocTraduction> {
    let Ok(bruts) = traiter_ocr(lignes, SEUIL_PROXIMITE) else {
        return Vec::new();
    };
    let type_bloc = type_de_bloc(classe);
    bruts
        .into_iter()
        .enumerate()
        .filter_map(|(i, b)| {
            let texte = b.texte.trim().to_string();
            if texte.is_empty() {
                return None;
            }
            Some(BlocTraduction {
                id: format!("b{}-{}", ordre_depart, i),
                type_bloc: type_bloc.clone(),
                bbox: Bbox {
                    x: b.bbox.x,
                    y: b.bbox.y,
                    w: b.bbox.w,
                    h: b.bbox.h,
                },
                original_text: Some(texte),
                translated_text: None,
                reading_order: (ordre_depart + i) as i64,
                // `review`, jamais `ok` : rien n'est vérifié tant que la mise en
                // page (T4.4) n'a pas passé. Un `ok` ici afficherait du texte
                // qui peut déborder.
                status: StatutBloc::Review,
                confidence: Some(b.score_confiance as f64),
                font_size: None,
                font_family: None,
                line_breaks: None,
            })
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Partie effectuë
// ---------------------------------------------------------------------------

/// Analyse une page du cache et renvoie ses blocs de texte.
///
/// Le jeton est celui que l'interface affiche déjà : aucune requête réseau, et
/// les octets viennent du cache donc l'analyse est exactement la même que ce
/// que l'utilisateur voit.
pub async fn analyser_page_interne(
    cache: &Arc<PageCache>,
    vision: &Arc<SuperviseurVision>,
    token: &str,
    chapter_url: &str,
    source: &str,
) -> Result<PageTranslation, ErreurVision> {
    let octets = cache
        .octets(token)
        .ok_or_else(|| ErreurVision::PageAbsente(token.to_string()))?;
    let image = image::load_from_memory(&octets)
        .map_err(|e| ErreurVision::ImageIllisible(e.to_string()))?
        .to_rgb8();
    let (largeur, hauteur) = (image.width(), image.height());
    if largeur == 0 || hauteur == 0 {
        return Err(ErreurVision::ImageIllisible("image vide".into()));
    }

    vision
        .assurer_demarrage()
        .await
        .map_err(ErreurVision::Sidecar)?;

    // 1. Détection, tuile par tuile.
    let mut detections: Vec<Detection> = Vec::new();
    for tuile in tuiles_carrees(largeur, hauteur, COTE_TUILE) {
        let morceau = extraire_tuile_carre(&image, tuile);
        let donnees = encoder_png(&morceau)?;
        let reponse = vision
            .demander(
                "detect",
                serde_json::json!({ "image": { "data": donnees } }),
                DELAI_DETECT,
            )
            .await
            .map_err(ErreurVision::Sidecar)?;
        let detect: DetectResult = lire_resultat(&reponse)?;
        for d in detect.detections {
            // La boîte est dans le repère de la tuile : on la translate en
            // repère page, puis on la borne. L'ordre est important — l'inverse
            // donnerait un bornage faux.
            let translated = VisionBbox {
                x: d.bbox.x + tuile.x as i64,
                y: d.bbox.y + tuile.y as i64,
                w: d.bbox.w,
                h: d.bbox.h,
            };
            if let Some(bornée) = borner(&translated, largeur, hauteur, TAILLE_MIN_BULLE) {
                detections.push(Detection {
                    bbox: bornée,
                    classe: ClasseBloc::depuis_id(d.classe),
                    score: d.score,
                });
            }
        }
    }
    if detections.is_empty() {
        return Err(ErreurVision::AucunBloc);
    }
    // Une bulle à cheval sur une jonction a été détectée deux fois : sans cette
    // fusion, la page affiche le même texte deux fois.
    let nb_tuiles = detections.len();
    let detections = fusionner(detections, SEUIL_FUSION);
    tracing::debug!(
        "{nb_tuiles} détections brutes -> {} bulles",
        detections.len()
    );

    // 2. Lecture par bulle, puis regroupement. L'ordre de lecture des bulles est
    //    celui de la page : haut vers bas, puis gauche vers droite.
    let mut detections = detections;
    detections.sort_by(|a, b| a.bbox.y.cmp(&b.bbox.y).then(a.bbox.x.cmp(&b.bbox.x)));

    let mut blocs: Vec<BlocTraduction> = Vec::new();
    let mut echecs = 0usize;
    // La page est encodée **une fois** et réutilisée pour chaque bulle : sinon
    // chaque `ocr` renverrait 600 Ko de PNG base64, soit plusieurs mégaoctets
    // par page pour rien.
    let page_base64 = encoder_png(&image)?;
    for detection in detections {
        let reponse = vision
            .demander(
                "ocr",
                serde_json::json!({
                    "image": { "data": page_base64 },
                    "bbox": detection.bbox,
                }),
                DELAI_OCR,
            )
            .await;
        let reponse = match reponse {
            Ok(r) => r,
            // Une bulle illisible ne doit pas faire perdre la page : on compte
            // et on continue. C'est ce qui distingue « rien lu » de « page
            // entière perdue ».
            Err(e) => {
                tracing::warn!("ocr impossible sur {:?} : {e}", detection.bbox);
                echecs += 1;
                continue;
            }
        };
        let ocr: OcrResult = match lire_resultat(&reponse) {
            Ok(o) => o,
            Err(e) => {
                tracing::warn!("ocr illisible sur {:?} : {e}", detection.bbox);
                echecs += 1;
                continue;
            }
        };
        let avant = blocs.len();
        blocs.extend(blocs_de_bulle(&ocr.lines, detection.classe, blocs.len()));
        if blocs.len() == avant {
            echecs += 1;
        }
    }

    if blocs.is_empty() {
        return Err(ErreurVision::AucunBloc);
    }
    tracing::info!(
        blocs = blocs.len(),
        echecs,
        largeur,
        hauteur,
        "analyse de page terminée"
    );

    Ok(PageTranslation {
        page_id: token.to_string(),
        manga_url: None,
        chapter_url: chapter_url.to_string(),
        source: source.to_string(),
        width: largeur as i64,
        height: hauteur as i64,
        blocks: blocs,
    })
}

/// Extrait une tuile **carrée**, complétée si elle dépasse la page.
///
/// Le bourrage est **blanc** : c'est la couleur d'une bulle et d'une page de
/// webtoon ; du gris créerait un contour que le détecteur pourrait prendre pour
/// un bord de bulle.
fn extraire_tuile_carre(image: &image::RgbImage, tuile: Tuile) -> image::RgbImage {
    let mut out = image::RgbImage::from_pixel(COTE_TUILE, COTE_TUILE, image::Rgb([255, 255, 255]));
    let l = COTE_TUILE.min(image.width().saturating_sub(tuile.x));
    let h = COTE_TUILE.min(image.height().saturating_sub(tuile.y));
    if l == 0 || h == 0 {
        return out;
    }
    // `crop_imm` renvoie une **vue** ; `replace` veut une image. Sans le
    // `to_image()`, la copie serait une vue empruntée à `image`.
    let morceau = image::imageops::crop_imm(image, tuile.x, tuile.y, l, h).to_image();
    image::imageops::replace(&mut out, &morceau, 0, 0);
    out
}

/// Encode une image en PNG base64 pour l'envoyer au sidecar.
fn encoder_png(image: &image::RgbImage) -> Result<String, ErreurVision> {
    let mut buffer = std::io::Cursor::new(Vec::new());
    image::DynamicImage::ImageRgb8(image.clone())
        .write_to(&mut buffer, image::ImageFormat::Png)
        .map_err(|e| ErreurVision::ImageIllisible(e.to_string()))?;
    Ok(base64::engine::general_purpose::STANDARD.encode(buffer.into_inner()))
}

/// Extrait le `result` d'une réponse réussie, ou dit pourquoi elle ne l'est pas.
fn lire_resultat<T: serde::de::DeserializeOwned>(reponse: &Value) -> Result<T, ErreurVision> {
    if reponse.get("ok") != Some(&Value::Bool(true)) {
        let code = reponse
            .get("error")
            .and_then(|e| e.get("code"))
            .and_then(Value::as_str)
            .unwrap_or("INTERNAL");
        let message = reponse
            .get("error")
            .and_then(|e| e.get("message"))
            .and_then(Value::as_str)
            .unwrap_or("sans message");
        return Err(ErreurVision::Sidecar(format!("{code} : {message}")));
    }
    let resultat = reponse
        .get("result")
        .cloned()
        .ok_or_else(|| ErreurVision::Sidecar("réponse sans `result`".into()))?;
    serde_json::from_value(resultat)
        .map_err(|e| ErreurVision::Sidecar(format!("résultat illisible : {e}")))
}

/// Analyse une page et renvoie ses blocs. Commande Tauri : c'est elle que
/// l'interface appelle avant `traduire_page`.
///
/// Les trois paramètres sont explicites et non déduits : l'interface connaît
/// l'URL du chapitre et la source, et le jeton de page est ce qu'elle affiche
/// déjà. Aucune requête réseau ici — les octets viennent du cache.
#[tauri::command]
pub async fn analyser_page(
    cache: tauri::State<'_, Arc<PageCache>>,
    vision: tauri::State<'_, Arc<SuperviseurVision>>,
    token: String,
    chapter_url: String,
    source: String,
) -> Result<PageTranslation, String> {
    analyser_page_interne(&cache, &vision, &token, &chapter_url, &source)
        .await
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn b(x: i64, y: i64, w: i64, h: i64) -> VisionBbox {
        VisionBbox { x, y, w, h }
    }

    #[test]
    fn une_page_de_800_par_1000_donne_quatre_tuiles_carrees() {
        // La grille couvre **toute** la page : 800 px de large = deux colonnes
        // (la seconde de 160 px, bourrée), 1000 px de haut = deux lignes. Mon
        // premier calcul disait 2 tuiles — c'est faux, et l'erreur coûtait une
        // colonne de détection manquée sur le bord droit.
        let t = tuiles_carrees(800, 1000, 640);
        assert_eq!(t.len(), 4, "2 colonnes × 2 lignes");
        assert_eq!(t[0], Tuile { x: 0, y: 0 });
        assert_eq!(t[1], Tuile { x: 640, y: 0 });
        assert_eq!(t[3], Tuile { x: 640, y: 640 });
    }

    #[test]
    fn une_page_plus_large_donne_plus_de_colonnes() {
        let t = tuiles_carrees(1400, 700, 640);
        assert_eq!(t.len(), 6, "3 colonnes × 2 lignes = 6 tuiles");
        assert_eq!(t[2], Tuile { x: 1280, y: 0 });
    }

    #[test]
    fn une_page_minuscule_ne_donne_rien() {
        assert!(tuiles_carrees(0, 100, 640).is_empty());
        assert!(tuiles_carrees(100, 0, 640).is_empty());
    }

    #[test]
    fn une_boite_debordante_est_ramenee_dans_la_page() {
        // Mesuré : le détecteur rend (-144, -46, 359, 214) sur une tuile de 640.
        // Traduite en 179, 104, 359, 214 sur une page de 800×1000.
        let mesure = b(-144 + 179, -46 + 104, 359, 214);
        let r = borner(&mesure, 800, 1000, TAILLE_MIN_BULLE).expect("doit survivre");
        assert_eq!((r.x, r.y), (35, 58), "bornée en haut à gauche");
        assert_eq!((r.w, r.h), (359, 214), "dimensions inchangées");
    }

    #[test]
    fn une_boite_qui_depasse_a_droite_est_coupee_sur_la_page() {
        let r = borner(&b(600, 100, 400, 200), 800, 1000, TAILLE_MIN_BULLE).expect("doit survivre");
        assert_eq!(r.x + r.w, 800, "elle s'arrête au bord droit de la page");
    }

    #[test]
    fn une_boite_entierement_hors_page_disparait() {
        assert!(
            borner(&b(-500, -500, 100, 100), 800, 1000, TAILLE_MIN_BULLE).is_none(),
            "une bulle hors page ne se culture pas"
        );
    }

    #[test]
    fn une_boite_trop_petite_est_du_bruit() {
        assert!(
            borner(&b(10, 10, 20, 20), 800, 1000, TAILLE_MIN_BULLE).is_none(),
            "20x20 px, c'est un accent isolé"
        );
    }

    #[test]
    fn la_classe_du_modele_devient_un_type_du_schema() {
        assert!(matches!(
            type_de_bloc(ClasseBloc::Bulle),
            TypeBloc::Dialogue
        ));
        assert!(matches!(
            type_de_bloc(ClasseBloc::TexteBulle),
            TypeBloc::Narration
        ));
        assert!(matches!(
            type_de_bloc(ClasseBloc::TexteLibre),
            TypeBloc::FreeText
        ));
        // Et surtout : aucune classe ne devient `Sfx` ou `Ignore` par défaut.
        // Ce sont des décisions (ADR 0011), pas des valeurs par défaut.
        assert!(!matches!(
            type_de_bloc(ClasseBloc::TexteLibre),
            TypeBloc::Sfx
        ));
        assert!(!matches!(
            type_de_bloc(ClasseBloc::TexteLibre),
            TypeBloc::Ignore
        ));
    }

    fn lignes_demo() -> Vec<crate::vision::VisionOcrLine> {
        use crate::vision::VisionOcrLine;
        // Positions **réelles** mesurées le 2026-10-07 sur la bulle 0,83 :
        // y = 359, 384, 407 — 25 px d'écart, donc une seule bulle. Ma première
        // version mettait 455 et créait un trou de 71 px, ce qui produisait
        // deux blocs : c'était le test qui mentait sur la page.
        vec![
            VisionOcrLine {
                text: "PROBABLY".into(),
                bbox: b(360, 359, 119, 9),
            },
            VisionOcrLine {
                text: "THE SAME PLACE".into(),
                bbox: b(359, 384, 188, 9),
            },
            VisionOcrLine {
                text: "DOGS".into(),
                bbox: b(418, 407, 69, 10),
            },
        ]
    }

    #[test]
    fn les_lignes_d_une_bulle_deviennent_un_bloc_avec_son_texte() {
        let blocs = blocs_de_bulle(&lignes_demo(), ClasseBloc::Bulle, 0);
        assert_eq!(blocs.len(), 1, "trois lignes d'une bulle = un bloc");
        let bloc = &blocs[0];
        assert!(bloc.original_text.as_deref().unwrap().contains("PROBABLY"));
        assert!(matches!(bloc.type_bloc, TypeBloc::Dialogue));
        assert_eq!(bloc.reading_order, 0, "ordre de lecture démarre à 0");
    }

    #[test]
    fn un_bloc_analyse_n_est_jamais_annonce_ok() {
        // `ok` promet une mise en page vérifiée (T4.4). Sans vérification, c'est
        // un mensonge qui ferait afficher du texte débordant.
        for classe in [
            ClasseBloc::Bulle,
            ClasseBloc::TexteBulle,
            ClasseBloc::TexteLibre,
        ] {
            for bloc in blocs_de_bulle(&lignes_demo(), classe, 0) {
                assert!(
                    matches!(bloc.status, StatutBloc::Review),
                    "un bloc analysé est `review`, jamais `ok`"
                );
            }
        }
    }

    #[test]
    fn l_ordre_de_lecture_continue_d_une_bulle_a_l_autre() {
        let premier = blocs_de_bulle(&lignes_demo(), ClasseBloc::Bulle, 0);
        let second = blocs_de_bulle(&lignes_demo(), ClasseBloc::TexteBulle, premier.len());
        assert_eq!(second[0].reading_order as usize, premier.len());
        assert_ne!(
            second[0].id, premier[0].id,
            "les identifiants ne se réutilisent pas"
        );
    }

    #[test]
    fn une_bulle_sans_texte_ne_donne_aucun_bloc() {
        assert!(blocs_de_bulle(&[], ClasseBloc::Bulle, 0).is_empty());
    }

    #[test]
    fn une_reponse_en_erreur_n_est_pas_lue_comme_un_resultat() {
        let reponse = serde_json::json!({
            "id": "x", "ok": false,
            "error": { "code": "MODEL_MISSING", "message": "pas de pack" }
        });
        let e = lire_resultat::<DetectResult>(&reponse).expect_err("doit échouer");
        assert!(
            matches!(&e, ErreurVision::Sidecar(m) if m.contains("MODEL_MISSING")),
            "le code doit être nommé : {e}"
        );
    }

    #[test]
    fn une_bulle_a_cheval_sur_deux_tuiles_n_est_comptee_qu_une_fois() {
        // Le cas réel : la bulle 0,83 fait 621×640 sur une page de 800, donc
        // elle est coupée par la jonction x=640 et détectée deux fois.
        let gauche = Detection {
            bbox: b(600, 200, 300, 400), // tronquée à droite par la tuile
            classe: ClasseBloc::Bulle,
            score: 0.83,
        };
        let droite = Detection {
            bbox: b(660, 200, 200, 400), // même bulle, vue dans la tuile voisine
            classe: ClasseBloc::Bulle,
            score: 0.80,
        };
        let fusion = fusionner(vec![droite, gauche], SEUIL_FUSION);
        assert_eq!(fusion.len(), 1, "une bulle, pas deux blocs de texte");
        assert_eq!(fusion[0].bbox.x, 600, "la plus petite x des deux");
        // 600..900 englobe 660..860 : la boîte la plus large **déborde** de la
        // tuile, et c'est normal — le détecteur raisonne sur sa fenêtre et ne
        // rogne pas. L'union fait donc 300, pas 260.
        assert_eq!(fusion[0].bbox.w, 300, "l'union des deux moitiés");
    }

    #[test]
    fn la_fusion_garde_la_classe_de_la_detection_la_plus_sure() {
        let sure = Detection {
            bbox: b(100, 100, 80, 40),
            classe: ClasseBloc::TexteBulle,
            score: 0.95,
        };
        let moins_sure = Detection {
            bbox: b(110, 105, 80, 40),
            classe: ClasseBloc::TexteLibre,
            score: 0.55,
        };
        let fusion = fusionner(vec![moins_sure, sure], SEUIL_FUSION);
        assert_eq!(fusion.len(), 1);
        assert_eq!(
            fusion[0].classe,
            ClasseBloc::TexteBulle,
            "la classe la plus sûre gagne : c'est elle qui décide de l'effacement"
        );
    }

    #[test]
    fn deux_bulles_distinctes_ne_sont_pas_fusionnees() {
        // Deux bulles l'une au-dessus de l'autre, séparées de 120 px : au-delà
        // du seuil, ce sont deux blocs. C'est ce qui interdit un seuil trop
        // large, sinon le texte de deux bulles n'en formerait qu'une.
        let haute = Detection {
            bbox: b(100, 100, 200, 80),
            classe: ClasseBloc::Bulle,
            score: 0.9,
        };
        let basse = Detection {
            bbox: b(120, 300, 200, 80),
            classe: ClasseBloc::Bulle,
            score: 0.9,
        };
        assert_eq!(fusionner(vec![haute, basse], SEUIL_FUSION).len(), 2);
    }

    #[test]
    fn une_tuile_carree_est_bourree_de_blanc() {
        // Une tuile qui dépasse la page doit être remplie, pas tronquée : le
        // détecteur verrait sinon une image aux dimensions variables.
        let image = image::RgbImage::from_pixel(100, 100, image::Rgb([10, 20, 30]));
        let tuile = extraire_tuile_carre(&image, Tuile { x: 0, y: 0 });
        assert_eq!(
            (tuile.width(), tuile.height()),
            (640, 640),
            "toujours carrée"
        );
        assert_eq!(
            tuile.get_pixel(600, 600).0,
            [255, 255, 255],
            "hors page : blanc, pas noir"
        );
        assert_eq!(tuile.get_pixel(10, 10).0, [10, 20, 30], "la page est là");
    }
}
