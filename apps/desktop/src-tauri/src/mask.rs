//! Masques de bulles pour le pipeline de traduction (T2.2).
//!
//! Un masque est une image binaire (blanc = intérieur de la bulle, noir =
//! extérieur). Il sert à l'effacement du texte (inpainting) et au contrôle
//! de résidu.
//!
//! Cette module fournit :
//! - création d'un masque grossier depuis une bbox (`masque_bbox`)
//! - érosion du masque pour ne pas effacer le contour de la bulle
//!   (`eroder`)
//! - validation : le masque ne doit pas être vide après érosion,
//!   et il ne doit pas déborder de la page (`valider`)

use std::cmp::{max, min};

use super::tile::Bbox;

/// Masque binaire, stocké comme une image 8-bit (0 = extérieur, 255 = intérieur).
#[derive(Debug, Clone)]
pub struct Masque {
    pub largeur: u32,
    pub hauteur: u32,
    pub donnees: Vec<u8>,
}

impl Masque {
    /// Crée un masque vide (tout noir = rien à effacer).
    pub fn vide(largeur: u32, hauteur: u32) -> Self {
        Self {
            largeur,
            hauteur,
            donnees: vec![0u8; (largeur * hauteur) as usize],
        }
    }

    /// Remplit un rectangle dans le masque.
    pub fn remplire_rect(&mut self, x: i64, y: i64, w: i64, h: i64, valeur: u8) {
        let x1 = max(0, x) as u32;
        let y1 = max(0, y) as u32;
        let x2 = min(self.largeur, max(x1, (x + w) as u32));
        let y2 = min(self.hauteur, max(y1, (y + h) as u32));
        for row in y1..y2 {
            let offset = (row * self.largeur + x1) as usize;
            self.donnees[offset..offset + (x2 - x1) as usize].fill(valeur);
        }
    }

    /// Compte le nombre de pixels blancs (intérieur de la bulle).
    pub fn compter_blancs(&self) -> u64 {
        self.donnees
            .iter()
            .map(|&v| if v > 0 { 1 } else { 0 })
            .sum()
    }

    /// Vérifie que le masque n'est pas vide et ne déborde pas.
    pub fn valider(&self, marge: u32) -> Result<(), ErreurMasque> {
        if self.largeur == 0 || self.hauteur == 0 {
            return Err(ErreurMasque::DimensionsNulles);
        }
        let blancs = self.compter_blancs();
        if blancs == 0 {
            return Err(ErreurMasque::Vide);
        }
        // Vérifier que le masque ne touche pas les bords de la page
        // (hors marge autorisée).
        let mut min_x = self.largeur;
        let mut max_x = 0;
        let mut min_y = self.hauteur;
        let mut max_y = 0;
        for y in 0..self.hauteur {
            for x in 0..self.largeur {
                let idx = (y * self.largeur + x) as usize;
                if self.donnees[idx] > 0 {
                    min_x = min(min_x, x);
                    max_x = max(max_x, x);
                    min_y = min(min_y, y);
                    max_y = max(max_y, y);
                }
            }
        }
        if min_x < marge
            || min_y < marge
            || max_x >= self.largeur - marge
            || max_y >= self.hauteur - marge
        {
            return Err(ErreurMasque::Debordement);
        }
        Ok(())
    }
}

/// Erreurs de masque.
#[derive(Debug, thiserror::Error)]
pub enum ErreurMasque {
    #[error("masque vide")]
    Vide,
    #[error("masque déborde de la page")]
    Debordement,
    #[error("dimensions nulles")]
    DimensionsNulles,
}

/// Crée un masque grossier à partir d'une bbox.
///
/// Le masque est un rectangle blanc sur fond noir, aux dimensions de la page.
pub fn masque_bbox(page_width: u32, page_height: u32, bbox: &Bbox) -> Masque {
    let mut masque = Masque::vide(page_width, page_height);
    masque.remplire_rect(bbox.x, bbox.y, bbox.w, bbox.h, 255);
    masque
}

/// Érode le masque d'une marge en pixels.
///
/// L'érosion réduit le masque du nombre de pixels donné, pour ne pas
/// effacer le contour de la bulle. Une marge de 2–4 px est typique.
///
/// Retourne `Err` si le masque devient vide après érosion.
pub fn eroder(masque: &Masque, marge: u32) -> Result<Masque, ErreurMasque> {
    if marge == 0 {
        return Ok(masque.clone());
    }

    let mut resultat = Masque::vide(masque.largeur, masque.hauteur);

    // Érosion simple : un pixel reste blanc seulement si tous ses voisins
    // dans un carré de (2*marge+1) sont blancs.
    let marge_i = marge as i64;
    for y in 0..masque.hauteur {
        for x in 0..masque.largeur {
            let mut tout_blanc = true;
            'voisins: for dy in -marge_i..=marge_i {
                for dx in -marge_i..=marge_i {
                    let nx = x as i64 + dx;
                    let ny = y as i64 + dy;
                    if nx < 0
                        || ny < 0
                        || nx >= masque.largeur as i64
                        || ny >= masque.hauteur as i64
                    {
                        tout_blanc = false;
                        break 'voisins;
                    }
                    let idx = (ny * masque.largeur as i64 + nx) as usize;
                    if masque.donnees[idx] == 0 {
                        tout_blanc = false;
                        break 'voisins;
                    }
                }
            }
            if tout_blanc {
                let idx = (y * masque.largeur + x) as usize;
                resultat.donnees[idx] = 255;
            }
        }
    }

    if resultat.compter_blancs() == 0 {
        return Err(ErreurMasque::Vide);
    }

    Ok(resultat)
}

/// Crée un masque depuis une liste de boîtes englobantes.
///
/// Les boîtes sont fusionnées en une seule forme rectangulaire, puis érodée.
pub fn masque_depuis_boxes(
    page_width: u32,
    page_height: u32,
    boxes: &[Bbox],
    marge: u32,
) -> Result<Masque, ErreurMasque> {
    if boxes.is_empty() {
        return Err(ErreurMasque::Vide);
    }

    let mut min_x = i64::MAX;
    let mut min_y = i64::MAX;
    let mut max_x = i64::MIN;
    let mut max_y = i64::MIN;

    for b in boxes {
        min_x = min_x.min(b.x);
        min_y = min_y.min(b.y);
        max_x = max_x.max(b.x + b.w);
        max_y = max_y.max(b.y + b.h);
    }

    let bbox = Bbox {
        x: min_x,
        y: min_y,
        w: max_x - min_x,
        h: max_y - min_y,
    };

    let mut masque = masque_bbox(page_width, page_height, &bbox);
    eroder(&masque, marge)?;
    // L'érosion a réussi, on garde le masque érodé.
    masque = eroder(&masque, marge)?;
    Ok(masque)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn masque_bbox_cree_un_rectangle() {
        let masque = masque_bbox(
            100,
            100,
            &Bbox {
                x: 10,
                y: 20,
                w: 30,
                h: 40,
            },
        );
        assert_eq!(masque.compter_blancs(), 30 * 40);
        // Coin hors du rectangle doit être noir.
        assert_eq!(masque.donnees[0], 0);
        // Pixel dans le rectangle doit être blanc.
        let idx = (20 * 100 + 10) as usize;
        assert_eq!(masque.donnees[idx], 255);
    }

    #[test]
    fn eroder_reduit_le_masque() {
        let masque = masque_bbox(
            100,
            100,
            &Bbox {
                x: 10,
                y: 10,
                w: 20,
                h: 20,
            },
        );
        let erode = eroder(&masque, 1).unwrap();
        // 20x20 - contour 1px = 18x18 = 324.
        assert_eq!(erode.compter_blancs(), 18 * 18);
    }

    #[test]
    fn eroder_trop_rend_vide() {
        let masque = masque_bbox(
            10,
            10,
            &Bbox {
                x: 0,
                y: 0,
                w: 2,
                h: 2,
            },
        );
        let err = eroder(&masque, 1).unwrap_err();
        match err {
            ErreurMasque::Vide => {}
            _ => panic!("attendu Vide"),
        }
    }

    #[test]
    fn masque_depuis_boxes_fusionne() {
        let boxes = vec![
            Bbox {
                x: 10,
                y: 10,
                w: 20,
                h: 20,
            },
            Bbox {
                x: 50,
                y: 50,
                w: 20,
                h: 20,
            },
        ];
        let masque = masque_depuis_boxes(200, 200, &boxes, 0).unwrap();
        // Boîtes fusionnées en bbox englobante : (10,10,60,60) = 3600 px.
        assert_eq!(masque.compter_blancs(), 60 * 60);
    }
}
