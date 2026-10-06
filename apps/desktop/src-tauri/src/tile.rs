//! Découpe de la bande virtuelle en tuiles pour le pipeline vision (T2.1).
//!
//! Une bande webtoon est une séquence de pages empilées verticalement.
//! Cette module trouve les lignes uniformes (espace entre cases) et découpe
//! la bande en tuiles qui tiennent dans la mémoire du modèle de détection.
//! Il gère aussi la fusion des détections aux jonctions de tuiles.

use std::cmp::{max, min};

/// Une page dans la bande virtuelle, avec sa position cumulative.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PageRect {
    /// Index de la page dans la bande.
    pub index: usize,
    /// Largeur de la page en pixels.
    pub width: u32,
    /// Hauteur de la page en pixels.
    pub height: u32,
    /// Position verticale cumulative dans la bande (en pixels).
    pub y_offset: u32,
}

/// Une tuile est une portion horizontale de la bande.
#[derive(Debug, Clone, PartialEq)]
pub struct Tile {
    /// Index de la tuile.
    pub index: usize,
    /// Position Y de début dans la bande.
    pub y_start: u32,
    /// Position Y de fin dans la bande (exclue).
    pub y_end: u32,
    /// Hauteur de la tuile.
    pub height: u32,
    /// Pages qui intersectent cette tuile.
    pub pages: Vec<usize>,
}

/// Erreurs de découpe.
#[derive(Debug, thiserror::Error)]
pub enum TileError {
    #[error("bande vide")]
    BandeVide,
    #[error("aucune page fournie")]
    AucunePage,
    #[error("dimensions invalides : {0}")]
    DimensionsInvalides(String),
    #[error("tuile vide : {0}")]
    TuileVide(String),
}

/// Construit les rectangles de pages à partir de leurs dimensions individuelles.
///
/// Les pages sont empilées verticalement dans l'ordre. `y_offset` est
/// calculé cumulativement.
pub fn pages_rectangles(dimensions: &[(usize, u32, u32)]) -> Vec<PageRect> {
    let mut pages = Vec::new();
    let mut y_offset = 0u32;
    for (index, width, height) in dimensions.iter().copied() {
        pages.push(PageRect {
            index,
            width,
            height,
            y_offset,
        });
        y_offset = y_offset.saturating_add(height);
    }
    pages
}

/// Trouve les lignes de coupe candidates entre les pages.
///
/// Une "ligne uniforme" est un espace vertical constant entre deux pages
/// consécutives. On considère qu'un espace est significatif s'il dépasse
/// `seuil_espace` pixels et que les largeurs des pages de part et d'autre
/// sont similaires (écart relatif < `seuil_largeur`).
///
/// Retourne les positions Y des lignes de coupe (début de l'espace).
pub fn lignes_de_coupe(pages: &[PageRect], seuil_espace: u32, seuil_largeur: f32) -> Vec<u32> {
    let mut coupes = Vec::new();
    for i in 1..pages.len() {
        let prev = &pages[i - 1];
        let curr = &pages[i];
        let espace = curr.y_offset.saturating_sub(prev.y_offset + prev.height);
        if espace < seuil_espace {
            continue;
        }
        let largeur_prev = prev.width as f32;
        let largeur_curr = curr.width as f32;
        let ratio = (largeur_prev.max(largeur_curr) - largeur_prev.min(largeur_curr))
            / largeur_prev.max(largeur_curr);
        if ratio < seuil_largeur {
            coupes.push(curr.y_offset);
        }
    }
    coupes
}

/// Découpe la bande en tuiles de hauteur maximale `max_tile_height`.
///
/// Les tuiles sont coupées de préférence aux lignes uniformes détectées
/// (espaces entre panels). Un panel plus grand que `max_tile_height` est
/// découpé en plusieurs tuiles alignées sur des multiples de
/// `max_tile_height`.
pub fn decouper_en_tuiles(
    pages: &[PageRect],
    max_tile_height: u32,
    lignes: &[u32],
) -> Result<Vec<Tile>, TileError> {
    if pages.is_empty() {
        return Err(TileError::AucunePage);
    }

    let hauteur_totale = pages
        .last()
        .map(|p| p.y_offset + p.height)
        .ok_or(TileError::BandeVide)?;

    if hauteur_totale == 0 {
        return Err(TileError::DimensionsInvalides(
            "hauteur totale nulle".into(),
        ));
    }

    if max_tile_height == 0 {
        return Err(TileError::DimensionsInvalides("max_tile_height nul".into()));
    }

    // Construire les segments de panel : chaque panel est une suite de pages
    // séparées par des lignes de coupe.
    let mut segments: Vec<(u32, u32)> = Vec::new(); // (y_start, y_end)
    let mut coupures: Vec<u32> = lignes
        .iter()
        .filter(|&&y| y > 0 && y < hauteur_totale)
        .copied()
        .collect();
    coupures.sort_unstable();
    coupures.dedup();

    let mut debut = 0u32;
    for &coupe in &coupures {
        if coupe > debut {
            segments.push((debut, coupe));
        }
        debut = coupe;
    }
    if debut < hauteur_totale {
        segments.push((debut, hauteur_totale));
    }

    // Découper chaque segment en tuiles de max_tile_height.
    let mut tuiles = Vec::new();
    let mut index_tuile = 0;
    for (seg_start, seg_end) in segments {
        let mut y = seg_start;
        while y < seg_end {
            let fin = min(y + max_tile_height, seg_end);
            let pages_tuile = pages
                .iter()
                .filter(|p| {
                    let page_fin = p.y_offset + p.height;
                    page_fin > y && p.y_offset < fin
                })
                .map(|p| p.index)
                .collect();
            tuiles.push(Tile {
                index: index_tuile,
                y_start: y,
                y_end: fin,
                height: fin - y,
                pages: pages_tuile,
            });
            index_tuile += 1;
            y = fin;
        }
    }

    // Vérifier qu'aucune tuile n'est vide.
    for tuile in &tuiles {
        if tuile.height == 0 {
            return Err(TileError::TuileVide(format!(
                "tuile {} hauteur=0",
                tuile.index
            )));
        }
        if tuile.pages.is_empty() {
            return Err(TileError::TuileVide(format!(
                "tuile {} sans page",
                tuile.index
            )));
        }
    }

    Ok(tuiles)
}

/// Fusionne des détections qui se trouvent de part et d'autre d'une jonction
/// de tuile.
///
/// Quand une bulle est coupée par une ligne de tuile, les deux moitiés
/// doivent être fusionnées en une seule boîte englobante.
///
/// `seuil_fusion` : distance maximale (en pixels) pour considérer que deux
/// boîtes font partie de la même bulle.
pub struct DetectionFusion<'a> {
    pub boxes: &'a [Bbox],
    pub seuil_fusion: i64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Bbox {
    pub x: i64,
    pub y: i64,
    pub w: i64,
    pub h: i64,
}

impl<'a> DetectionFusion<'a> {
    pub fn new(boxes: &'a [Bbox], seuil_fusion: i64) -> Self {
        Self {
            boxes,
            seuil_fusion,
        }
    }

    /// Fusionne les boîtes qui se chevauchent ou sont proches.
    pub fn fusionner(self) -> Vec<Bbox> {
        if self.boxes.is_empty() {
            return vec![];
        }

        // Tri par Y puis X.
        let mut boxes: Vec<Bbox> = self.boxes.to_vec();
        boxes.sort_by(|a, b| a.y.cmp(&b.y).then(a.x.cmp(&b.x)));

        let mut result = Vec::new();
        let mut current = boxes[0];

        for &next in &boxes[1..] {
            if self.chevauche_ou_proche(current, next) {
                current = self.englober(current, next);
            } else {
                result.push(current);
                current = next;
            }
        }
        result.push(current);
        result
    }

    fn chevauche_ou_proche(&self, a: Bbox, b: Bbox) -> bool {
        let ax2 = a.x + a.w;
        let ay2 = a.y + a.h;
        let bx2 = b.x + b.w;
        let by2 = b.y + b.h;

        let chevauche_x = a.x <= bx2 && b.x <= ax2;
        let chevauche_y = a.y <= by2 && b.y <= ay2;

        if chevauche_x && chevauche_y {
            return true;
        }

        // Distance minimale entre les rectangles.
        let dx = if ax2 < b.x {
            b.x - ax2
        } else if bx2 < a.x {
            a.x - bx2
        } else {
            0
        };
        let dy = if ay2 < b.y {
            b.y - ay2
        } else if by2 < a.y {
            a.y - by2
        } else {
            0
        };

        dx <= self.seuil_fusion && dy <= self.seuil_fusion
    }

    fn englober(&self, a: Bbox, b: Bbox) -> Bbox {
        let x1 = min(a.x, b.x);
        let y1 = min(a.y, b.y);
        let x2 = max(a.x + a.w, b.x + b.w);
        let y2 = max(a.y + a.h, b.y + b.h);
        Bbox {
            x: x1,
            y: y1,
            w: x2 - x1,
            h: y2 - y1,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pages_rectangles_calcule_les_y_offset() {
        let dims = vec![(0, 100, 200), (1, 120, 300), (2, 100, 250)];
        let pages = pages_rectangles(&dims);
        assert_eq!(pages[0].y_offset, 0);
        assert_eq!(pages[1].y_offset, 200);
        assert_eq!(pages[2].y_offset, 500);
    }

    #[test]
    fn pages_rectangles_vide_retourne_vide() {
        let pages = pages_rectangles(&[]);
        assert!(pages.is_empty());
    }

    #[test]
    fn lignes_de_coupe_trouve_les_espaces_significatifs() {
        let pages = vec![
            PageRect {
                index: 0,
                width: 800,
                height: 1200,
                y_offset: 0,
            },
            PageRect {
                index: 1,
                width: 800,
                height: 1200,
                y_offset: 1250, // espace de 50px
            },
            PageRect {
                index: 2,
                width: 800,
                height: 1200,
                y_offset: 2500, // espace de 50px
            },
        ];
        let coupes = lignes_de_coupe(&pages, 40, 0.1);
        assert_eq!(coupes, vec![1250, 2500]);
    }

    #[test]
    fn lignes_de_coupe_ignore_les_petits_ecarts() {
        let pages = vec![
            PageRect {
                index: 0,
                width: 800,
                height: 1200,
                y_offset: 0,
            },
            PageRect {
                index: 1,
                width: 800,
                height: 1200,
                y_offset: 1210, // espace de 10px
            },
        ];
        let coupes = lignes_de_coupe(&pages, 40, 0.1);
        assert!(coupes.is_empty());
    }

    #[test]
    fn lignes_de_coupe_ignore_les_largeurs_trop_differentes() {
        let pages = vec![
            PageRect {
                index: 0,
                width: 800,
                height: 1200,
                y_offset: 0,
            },
            PageRect {
                index: 1,
                width: 400, // moitié
                height: 1200,
                y_offset: 1250,
            },
        ];
        let coupes = lignes_de_coupe(&pages, 40, 0.1);
        assert!(coupes.is_empty());
    }

    #[test]
    fn decouper_en_tuiles_sans_coupe_cree_une_tuile_par_bloc() {
        let pages = vec![
            PageRect {
                index: 0,
                width: 800,
                height: 1200,
                y_offset: 0,
            },
            PageRect {
                index: 1,
                width: 800,
                height: 1200,
                y_offset: 1200,
            },
        ];
        let tuiles = decouper_en_tuiles(&pages, 600, &[]).unwrap();
        // 2 pages de 1200px, max 600px → 4 tuiles.
        assert_eq!(tuiles.len(), 4);
        assert_eq!(tuiles[0].y_end, 600);
        assert_eq!(tuiles[1].y_start, 600);
        assert_eq!(tuiles[1].y_end, 1200);
        assert_eq!(tuiles[2].y_start, 1200);
        assert_eq!(tuiles[2].y_end, 1800);
        assert_eq!(tuiles[3].y_start, 1800);
        assert_eq!(tuiles[3].y_end, 2400);
    }

    #[test]
    fn decouper_en_tuiles_utilise_les_lignes() {
        let pages = vec![
            PageRect {
                index: 0,
                width: 800,
                height: 1200,
                y_offset: 0,
            },
            PageRect {
                index: 1,
                width: 800,
                height: 1200,
                y_offset: 1200,
            },
        ];
        // Coupe à 1200 (jonction entre panels), max 600 par tuile.
        // Chaque panel de 1200px est découpé en 2 tuiles → 4 tuiles au total.
        let tuiles = decouper_en_tuiles(&pages, 600, &[1200]).unwrap();
        assert_eq!(tuiles.len(), 4);
        assert_eq!(tuiles[0].y_end, 600);
        assert_eq!(tuiles[0].pages, vec![0]);
        assert_eq!(tuiles[1].y_start, 600);
        assert_eq!(tuiles[1].y_end, 1200);
        assert_eq!(tuiles[1].pages, vec![0]);
        assert_eq!(tuiles[2].y_start, 1200);
        assert_eq!(tuiles[2].y_end, 1800);
        assert_eq!(tuiles[2].pages, vec![1]);
        assert_eq!(tuiles[3].y_start, 1800);
        assert_eq!(tuiles[3].y_end, 2400);
        assert_eq!(tuiles[3].pages, vec![1]);
    }

    #[test]
    fn decouper_bande_vide_erreur() {
        let err = decouper_en_tuiles(&[], 600, &[]).unwrap_err();
        match err {
            TileError::AucunePage => {}
            _ => panic!("attendu AucunePage"),
        }
    }

    #[test]
    fn fusion_chevauchement() {
        let boxes = vec![
            Bbox {
                x: 10,
                y: 10,
                w: 100,
                h: 50,
            },
            Bbox {
                x: 80,
                y: 20,
                w: 100,
                h: 50,
            },
        ];
        let fusion = DetectionFusion::new(&boxes, 10);
        let result = fusion.fusionner();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].x, 10);
        assert_eq!(result[0].y, 10);
        assert_eq!(result[0].w, 170);
        assert_eq!(result[0].h, 60);
    }

    #[test]
    fn fusion_separe_les_boites_loins() {
        let boxes = vec![
            Bbox {
                x: 10,
                y: 10,
                w: 50,
                h: 50,
            },
            Bbox {
                x: 200,
                y: 200,
                w: 50,
                h: 50,
            },
        ];
        let fusion = DetectionFusion::new(&boxes, 10);
        let result = fusion.fusionner();
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn fusion_proche_jonction_tuile() {
        // Deux moitiés d'une bulle coupée par une tuile.
        let boxes = vec![
            Bbox {
                x: 50,
                y: 590,
                w: 100,
                h: 30,
            },
            Bbox {
                x: 50,
                y: 600,
                w: 100,
                h: 40,
            },
        ];
        let fusion = DetectionFusion::new(&boxes, 20);
        let result = fusion.fusionner();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].y, 590);
        assert_eq!(result[0].h, 50);
    }
}
