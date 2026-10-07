//! Chaîne complète sur une vraie page, pour la parité et pour la preuve.
//!
//! Ne fait pas partie des tests unitaires : il a besoin des poids, qui ne sont
//! **jamais** dans le dépôt. Il sert à comparer la sortie de ce sidecar à celle
//! de la sonde Python de l'évaluation (ADR 0007 exige cette parité).
//!
//! Usage : `cargo run --example chain -- <dossier-modeles> <image> [y0]`
//!
//! Le dossier doit contenir :
//!   detector_int8.onnx
//!   ocr-en/PP-OCRv6_small_rec.onnx
//!   ocr-en/PP-OCRv6_small_rec.txt
//!   ocr-en/manga_det_v0.2.onnx

use std::path::PathBuf;

use tatami_vision::detect::{Classe, DetecteurBulles, SCORE_MINIMAL};
use tatami_vision::lignes::DetecteurLignes;
use tatami_vision::ocr::Lecteur;

/// Recadre une boîte **bornée à l'image**.
///
/// `image::imageops::crop_imm` prend une largeur et une hauteur, pas un coin
/// bas-droit. Passer `x + largeur` là où il faut `largeur` donne une culture
/// plusieurs fois trop grande — mesuré ici : 230×370 px au lieu de 123×14, le
/// texte écrasé à 48 px de haut, et une lecture qui rendait « - » et « H ».
fn culture_bornee(
    image: &image::RgbImage,
    x: f32,
    y: f32,
    largeur: f32,
    hauteur: f32,
    marge: f32,
) -> image::RgbImage {
    let (iw, ih) = (image.width() as i64, image.height() as i64);
    let x1 = (x - marge).round().max(0.0).min(iw as f32) as i64;
    let y1 = (y - marge).round().max(0.0).min(ih as f32) as i64;
    let x2 = ((x + largeur + marge).round() as i64).clamp(x1, iw);
    let y2 = ((y + hauteur + marge).round() as i64).clamp(y1, ih);
    image::imageops::crop_imm(
        image,
        x1 as u32,
        y1 as u32,
        (x2 - x1) as u32,
        (y2 - y1) as u32,
    )
    .to_image()
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("usage: chain <dossier-modeles> <image> [y0]");
        std::process::exit(2);
    }
    let modeles = PathBuf::from(&args[1]);
    let image = image::open(&args[2])?.to_rgb8();
    let y0: u32 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(0);
    let fenetre = image::imageops::crop_imm(
        &image,
        0,
        y0,
        image.width().min(640),
        (image.height() - y0).min(640),
    )
    .to_image();
    println!(
        "image {}x{} | fenetre {}x{} a y={}",
        image.width(),
        image.height(),
        fenetre.width(),
        fenetre.height(),
        y0
    );

    let mut detecteur = DetecteurBulles::ouvrir(&modeles.join("detector_int8.onnx"))?;
    let t0 = std::time::Instant::now();
    let detections = detecteur.detecter(&fenetre, SCORE_MINIMAL)?;
    println!(
        "detection : {} boites en {} ms",
        detections.len(),
        t0.elapsed().as_millis()
    );
    for d in detections.iter().take(6) {
        println!(
            "  score={:.2} classe={:?} boite=({:.0},{:.0},{:.0},{:.0})",
            d.score,
            d.classe,
            d.boite.x,
            d.boite.y,
            d.boite.x + d.boite.largeur,
            d.boite.y + d.boite.hauteur
        );
    }

    let mut lignes = DetecteurLignes::ouvrir(&modeles.join("ocr-en").join("manga_det_v0.2.onnx"))?;
    let mut lecteur = Lecteur::ouvrir(
        &modeles.join("ocr-en").join("PP-OCRv6_small_rec.onnx"),
        &std::fs::read_to_string(modeles.join("ocr-en").join("PP-OCRv6_small_rec.txt"))?,
    )?;
    println!(
        "dictionnaire : {} classes pour un modele de {} (mesure a l'ouverture)",
        lecteur.dictionnaire().nb_classes(),
        18710
    );

    // On lit la **plus grande** bulle, pas la mieux notée : mesuré, la mieux
    // notée (0,98) est souvent un fragment de 215×167 px au bord de la
    // fenêtre, alors que la plus grande (0,83, 462×491 px) porte le texte
    // lisible. Le score mesure la certitude du détecteur, pas la quantité de
    // texte.
    let contenue = |d: &tatami_vision::detect::Detection| {
        d.boite.x >= 0.0
            && d.boite.y >= 0.0
            && d.boite.x + d.boite.largeur <= fenetre.width() as f32
            && d.boite.y + d.boite.hauteur <= fenetre.height() as f32
    };
    let bulles: Vec<&tatami_vision::detect::Detection> = detections
        .iter()
        .filter(|d| matches!(d.classe, Classe::Bulle | Classe::TexteBulle))
        .collect();
    let (cible, tronquee) = match bulles
        .iter()
        .filter(|d| contenue(d))
        .max_by(|a, b| {
            (a.boite.largeur * a.boite.hauteur)
                .partial_cmp(&(b.boite.largeur * b.boite.hauteur))
                .unwrap_or(std::cmp::Ordering::Equal)
        })
        .copied()
        .or_else(|| {
            bulles
                .iter()
                .max_by(|a, b| {
                    (a.boite.largeur * a.boite.hauteur)
                        .partial_cmp(&(b.boite.largeur * b.boite.hauteur))
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .copied()
        }) {
        Some(d) => (d, !contenue(d)),
        None => {
            println!("aucune bulle detectee : rien a comparer");
            return Ok(());
        }
    };
    if tronquee {
        println!(
            "note : aucune bulle n'est entierement contenue dans la fenetre {}x{}, la culture est donc prise sur la page",
            fenetre.width(),
            fenetre.height()
        );
    }

    // La culture vient de la **page**, pas de la fenêtre. Mesuré : la fenêtre
    // 640x640 coupe les bulles (la meilleure va jusqu'à x=819 sur une page de
    // 800), donc borner à la fenêtre ne donnait qu'un texte partiel — et la
    // comparaison avec la sonde Python, qui cultivait depuis la page, n'était
    // alors pas à pareille. C'est une leçon pour le pipeline : les tuiles
    // servent à **détecter**, les cultures se font sur la page.
    let culture = culture_bornee(
        &image,
        cible.boite.x,
        y0 as f32 + cible.boite.y,
        cible.boite.largeur,
        cible.boite.hauteur,
        0.0,
    );
    println!(
        "\nbulle choisie : score={:.2} classe={:?} {}x{} px cultivee sur la page",
        cible.score,
        cible.classe,
        culture.width(),
        culture.height()
    );

    let t1 = std::time::Instant::now();
    let boxes = lignes.detecter_par_defaut(&culture)?;
    println!(
        "lignes : {} en {} ms",
        boxes.len(),
        t1.elapsed().as_millis()
    );

    let t2 = std::time::Instant::now();
    let mut total = 0usize;
    // Les cultures sont écrites sur disque : c'est ce qui permet de relire les
    // **mêmes pixels** avec la sonde Python et de trancher où les deux
    // implémentations divergent, au lieu de comparer deux textes qui n'ont
    // peut-être rien à voir.
    let sortie = std::env::var("TATAMI_CROPS")
        .unwrap_or_else(|_| "C:/Users/alexanders/AppData/Local/Temp/opencode/crops".to_string());
    let _ = std::fs::create_dir_all(&sortie);
    for (i, b) in boxes.iter().enumerate() {
        let crop = culture_bornee(&culture, b.x, b.y, b.largeur, b.hauteur, 2.0);
        let chemin = format!("{sortie}/ligne{i:02}.png");
        let _ = crop.save(&chemin);
        let texte = lecteur.lire(&crop)?;
        if !texte.is_empty() {
            println!(
                "  ({:3.0},{:3.0}) {:3.0}x{:2.0} px -> {}",
                b.x, b.y, b.largeur, b.hauteur, texte
            );
            total += 1;
        }
    }
    println!(
        "--- {} lignes avec du texte, OCR en {} ms | cultures dans {sortie} ---",
        total,
        t2.elapsed().as_millis()
    );
    Ok(())
}
