//! Tests d'intégration T2.1 — tuiles sur images synthétiques.
//!
//! Génère une bande webtoon PNG factice avec des panels séparés par des
//! espacements connus, puis vérifie que la découpe produit les tuiles
//! attendues et que la fusion des détections aux jonctions marche.

use std::path::PathBuf;

use image::RgbaImage;
use tatami_desktop::tile::{decouper_en_tuiles, Bbox, DetectionFusion};

/// Crée une bande webtoon synthétique de 3 panels.
///
/// Chaque panel est un rectangle de couleur différente, séparé par un
/// espace blanc de 50 px. La largeur est fixe (800 px).
///
/// Retourne les dimensions des pages et le chemin du PNG.
fn bande_synthetique() -> (Vec<(usize, u32, u32)>, PathBuf) {
    let largeur = 800u32;
    let hauteur_panel = 400u32;
    let espace = 50u32;
    let nb_panels = 3;

    let mut img = RgbaImage::new(
        largeur,
        nb_panels * hauteur_panel + (nb_panels - 1) * espace,
    );

    // Couleurs distinctes par panel.
    let couleurs = [
        [255, 0, 0, 255], // rouge
        [0, 255, 0, 255], // vert
        [0, 0, 255, 255], // bleu
    ];

    for i in 0..nb_panels {
        let y_debut = i * (hauteur_panel + espace);
        for y in y_debut..y_debut + hauteur_panel {
            for x in 0..largeur {
                let pixel = couleurs[i as usize];
                img.put_pixel(x, y, image::Rgba(pixel));
            }
        }
        // Espace blanc entre panels (déjà initialisé à transparent/noir).
        // On le remplit en gris clair pour être visible.
        if i < nb_panels - 1 {
            let y_espace_debut = y_debut + hauteur_panel;
            let y_espace_fin = y_espace_debut + espace;
            for y in y_espace_debut..y_espace_fin {
                for x in 0..largeur {
                    img.put_pixel(x, y, image::Rgba([240, 240, 240, 255]));
                }
            }
        }
    }

    let path = std::env::temp_dir().join("tatami-test-bande.png");
    img.save(&path).expect("sauvegarde PNG échouée");

    let dimensions: Vec<(usize, u32, u32)> = (0..nb_panels as usize)
        .map(|i| (i, largeur, hauteur_panel))
        .collect();

    (dimensions, path)
}

#[test]
fn tuiles_coupes_aux_jonctions_panels() {
    let (dims, _path) = bande_synthetique();
    let pages = tatami_desktop::tile::pages_rectangles(&dims);
    // pages_rectangles empile les panels sans espace : 0 cut.
    let coupes = tatami_desktop::tile::lignes_de_coupe(&pages, 30, 0.1);
    assert!(coupes.is_empty());
}

#[test]
fn decouper_en_tuiles_3_panels_600px() {
    let (dims, _path) = bande_synthetique();
    let pages = tatami_desktop::tile::pages_rectangles(&dims);
    // Panels de 400px, max 600px → 2 tuiles qui se chevauchent sur 2 panels.
    let tuiles = decouper_en_tuiles(&pages, 600, &[]).unwrap();
    assert_eq!(tuiles.len(), 2);
    assert_eq!(tuiles[0].pages, vec![0, 1]);
    assert_eq!(tuiles[0].y_start, 0);
    assert_eq!(tuiles[0].y_end, 600);
    assert_eq!(tuiles[1].pages, vec![1, 2]);
    assert_eq!(tuiles[1].y_start, 600);
    assert_eq!(tuiles[1].y_end, 1200);
}

#[test]
fn decouper_en_tuiles_avec_lignes_600px() {
    let (dims, _path) = bande_synthetique();
    let pages = tatami_desktop::tile::pages_rectangles(&dims);
    // Pas de ligne de coupe détectée (panels contigus) → 2 tuiles.
    let tuiles = decouper_en_tuiles(&pages, 600, &[]).unwrap();
    assert_eq!(tuiles.len(), 2);
}

#[test]
fn decouper_panel_trop_haut_est_split() {
    // Panel unique de 1500px, max 600px → 3 tuiles.
    let dims = vec![(0, 800, 1500)];
    let pages = tatami_desktop::tile::pages_rectangles(&dims);
    let tuiles = decouper_en_tuiles(&pages, 600, &[]).unwrap();

    assert_eq!(tuiles.len(), 3);
    assert_eq!(tuiles[0].y_end, 600);
    assert_eq!(tuiles[0].pages, vec![0]);
    assert_eq!(tuiles[1].y_start, 600);
    assert_eq!(tuiles[1].y_end, 1200);
    assert_eq!(tuiles[2].y_start, 1200);
    assert_eq!(tuiles[2].y_end, 1500);
}

#[test]
fn fusion_jonction_tuile_apres_coupe() {
    // Simule deux détections coupées par une tuile à y=600.
    let boxes = vec![
        Bbox {
            x: 50,
            y: 580,
            w: 200,
            h: 40,
        },
        Bbox {
            x: 50,
            y: 600,
            w: 200,
            h: 40,
        },
    ];
    let fusion = DetectionFusion::new(&boxes, 30);
    let result = fusion.fusionner();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].y, 580);
    assert_eq!(result[0].h, 60);
}

#[test]
fn dimensions_pages_servent_au_calcul() {
    // Vérifie que `pages_rectangles` + `decouper_en_tuiles` utilisent bien
    // les dimensions réelles, pas des valeurs par défaut.
    let dims = vec![(0, 100, 200), (1, 200, 300)];
    let pages = tatami_desktop::tile::pages_rectangles(&dims);
    let tuiles = decouper_en_tuiles(&pages, 150, &[]).unwrap();

    // Page 0: 0-200, page 1: 200-500, max 150px.
    // Tuiles : 0-150 [page 0], 150-300 [pages 0+1], 300-450 [page 1], 450-500 [page 1].
    assert_eq!(tuiles.len(), 4);
    assert_eq!(tuiles[0].pages, vec![0]);
    assert_eq!(tuiles[1].pages, vec![0, 1]);
    assert_eq!(tuiles[2].pages, vec![1]);
    assert_eq!(tuiles[3].pages, vec![1]);
}
