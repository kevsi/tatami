//! Découpe d'une bulle en **lignes** de texte.
//!
//! ## Pourquoi ce module existe
//!
//! PP-OCRv6 est un CTC de reconnaissance : il lit une ligne, pas une bulle.
//! Mesuré le 2026-10-07 sur la bulle `(178,149)-(819,789)` d'une page de
//! dialogue : appliquée à la bulle entière de 641×640 px, elle rend `LE` puis
//! `pe`. Avec ce module devant, la même bulle donne sept lignes et le texte
//! réel : `PROBABLY`, `THE SAME PLACE`, `YOU GOT TO GET`, `CLOTHES FOR`, `DOGS`.
//!
//! Le modèle est `manga_det_v0.2.onnx` (1,7 Mo, Apache-2.0), qui sort une carte
//! de probabilité style DBNet. Recette PP-OCR : hauteur et largeur multiples de
//! 32, division par 255, centrage, seuil 0,3, composantes connexes.
//!
//! Le seuil 0,3 et la taille minimale sont **constantes nommées** : une valeur
//! écrite en dur à deux endroits finit toujours par diverger (leçon AGENTS.md,
//! « `bornes()`-style »).

use ort::session::Session;
use ort::value::Tensor;

/// Seuil de probabilité au-dessus duquel un pixel appartient à du texte.
pub const SEUIL_PROBABILITE: f32 = 0.3;

/// Largeur minimale d'une ligne, en pixels de l'image d'origine. Ce seuil écarte
/// le bruit : mesuré, un composant de 1×1 px sortait du seuil, alors que les
/// lignes réelles mesurées le 2026-10-07 font de 70 à 188 px de large.
pub const LARGEUR_MINIMALE: i64 = 8;

/// Hauteur minimale d'une ligne, en pixels. **Plus petit que la largeur, sur
/// mesure** : les lignes réelles relevées sur une bulle de webtoon font 7 à 9 px
/// de haut (119×8, 188×8, 155×7, 70×9). Un seuil unique de 8 px en hauteur
/// aurait supprimé la moitié du texte ; les tests l'ont montré avant qu'un
/// modèle ne le fasse.
pub const HAUTEUR_MINIMALE: i64 = 4;

/// Les trois seuils de la détection de lignes, regroupés : écrits à deux
/// endroits, ils finissent toujours par diverger.
#[derive(Debug, Clone, Copy)]
pub struct Seuils {
    /// Probabilité minimale pour qu'un pixel compte comme du texte.
    pub probabilite: f32,
    /// Largeur minimale d'une ligne, en pixels.
    pub largeur_minimale: i64,
    /// Hauteur minimale d'une ligne, en pixels.
    pub hauteur_minimale: i64,
}

impl Default for Seuils {
    /// Les valeurs mesurées le 2026-10-07 sur une bulle de webtoon.
    fn default() -> Self {
        Seuils {
            probabilite: SEUIL_PROBABILITE,
            largeur_minimale: LARGEUR_MINIMALE,
            hauteur_minimale: HAUTEUR_MINIMALE,
        }
    }
}

/// multiple de 32 exigé par l'entrée du modèle.
const MULTIPLE: u32 = 32;

/// Rectangle en coordonnées de l'image d'origine.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rectangle {
    pub x: f32,
    pub y: f32,
    pub largeur: f32,
    pub hauteur: f32,
}

impl Rectangle {
    /// Centre du rectangle, utile pour l'ordre de lecture.
    pub fn centre_y(&self) -> f32 {
        self.y + self.hauteur / 2.0
    }
}

/// Erreur de détection des lignes.
#[derive(Debug)]
pub enum ErreurLignes {
    /// Le modèle n'a pas pu être ouvert ou a échoué.
    Modele(String),
    /// La carte de probabilité n'a pas la forme attendue.
    FormeCarte { largeur: usize, hauteur: usize },
    /// L'image ne contient aucune zone de texte.
    AucuneLigne,
}

impl std::fmt::Display for ErreurLignes {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ErreurLignes::Modele(m) => write!(f, "détecteur de lignes : {m}"),
            ErreurLignes::FormeCarte { largeur, hauteur } => write!(
                f,
                "carte de probabilité de {largeur}x{hauteur}, la largeur est nulle"
            ),
            ErreurLignes::AucuneLigne => write!(f, "aucune ligne de texte détectée"),
        }
    }
}

impl std::error::Error for ErreurLignes {}

/// Composantes connexes 8-connexes d'un masque binaire.
///
/// Fonction pure : c'est ici que se perdent les lignes si l'on se trompe de
/// voisinage, et une erreur ici est invisible sur le texte final. Le
/// voisinage à 8 connexe est celui de PP-OCR ; à 4, une lettre en diagonale
/// scinderait sa ligne en deux morceaux.
pub fn composantes(masque: &[bool], largeur: usize, hauteur: usize) -> Vec<Rectangle> {
    let mut visites = vec![false; largeur * hauteur];
    let mut boîtes = Vec::new();
    let mut pile: Vec<usize> = Vec::new();

    for depart in 0..(largeur * hauteur) {
        if !masque[depart] || visites[depart] {
            continue;
        }
        visites[depart] = true;
        pile.clear();
        pile.push(depart);
        let mut min_x = largeur;
        let mut min_y = hauteur;
        let mut max_x = 0usize;
        let mut max_y = 0usize;

        while let Some(courant) = pile.pop() {
            let (x, y) = (courant % largeur, courant / largeur);
            min_x = min_x.min(x);
            min_y = min_y.min(y);
            max_x = max_x.max(x);
            max_y = max_y.max(y);

            for dy in -1i64..=1 {
                for dx in -1i64..=1 {
                    if dx == 0 && dy == 0 {
                        continue;
                    }
                    let nx = x as i64 + dx;
                    let ny = y as i64 + dy;
                    if nx < 0 || ny < 0 || nx >= largeur as i64 || ny >= hauteur as i64 {
                        continue;
                    }
                    let voisin = ny as usize * largeur + nx as usize;
                    if masque[voisin] && !visites[voisin] {
                        visites[voisin] = true;
                        pile.push(voisin);
                    }
                }
            }
        }
        boîtes.push(Rectangle {
            x: min_x as f32,
            y: min_y as f32,
            largeur: (max_x - min_x + 1) as f32,
            hauteur: (max_y - min_y + 1) as f32,
        });
    }
    boîtes
}

/// Boîtes de lignes dans les coordonnées de l'image d'origine.
///
/// `facteur_x` et `facteur_y` sont les rapports entre la taille d'origine et la
/// taille de la carte (l'image a été agrandie pour tomber sur un multiple de
/// 32, donc ces rapports sont presque toujours < 1).
pub fn boites_de_lignes(
    probabilites: &[f32],
    largeur_carte: usize,
    hauteur_carte: usize,
    largeur_orig: u32,
    hauteur_orig: u32,
    seuils: Seuils,
) -> Vec<Rectangle> {
    let masque: Vec<bool> = probabilites
        .iter()
        .map(|p| *p > seuils.probabilite)
        .collect();
    let facteur_x = largeur_orig as f32 / largeur_carte as f32;
    let facteur_y = hauteur_orig as f32 / hauteur_carte as f32;

    let mut lignes: Vec<Rectangle> = composantes(&masque, largeur_carte, hauteur_carte)
        .into_iter()
        .filter(|r| {
            (r.largeur as i64) >= seuils.largeur_minimale
                && (r.hauteur as i64) >= seuils.hauteur_minimale
        })
        .map(|r| Rectangle {
            x: r.x * facteur_x,
            y: r.y * facteur_y,
            largeur: r.largeur * facteur_x,
            hauteur: r.hauteur * facteur_y,
        })
        .collect();

    // Ordre de lecture : de haut en bas. Les lignes d'une même bulle se lisent
    // dans cet ordre, et `ocr.rs` côté application regroupe ensuite ce qui
    // appartient au même bloc.
    lignes.sort_by(|a, b| {
        a.centre_y()
            .partial_cmp(&b.centre_y())
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    lignes
}

/// Détecteur de lignes chargé en mémoire.
pub struct DetecteurLignes {
    session: Session,
}

impl DetecteurLignes {
    pub fn ouvrir(modele: &std::path::Path) -> Result<DetecteurLignes, ErreurLignes> {
        let mut builder = Session::builder().map_err(|e| ErreurLignes::Modele(e.to_string()))?;
        let session = builder
            .commit_from_file(modele)
            .map_err(|e| ErreurLignes::Modele(e.to_string()))?;
        Ok(DetecteurLignes { session })
    }

    /// Détecte les lignes d'une culture et les rend dans ses coordonnées.
    pub fn detecter(
        &mut self,
        image: &image::RgbImage,
        seuils: Seuils,
    ) -> Result<Vec<Rectangle>, ErreurLignes> {
        let (largeur_orig, hauteur_orig) = (image.width(), image.height());
        if largeur_orig == 0 || hauteur_orig == 0 {
            return Err(ErreurLignes::AucuneLigne);
        }
        let entree_w = arrondi_au_multiple_32(largeur_orig);
        let entree_h = arrondi_au_multiple_32(hauteur_orig);

        let resize = image::imageops::resize(
            image,
            entree_w,
            entree_h,
            image::imageops::FilterType::Triangle,
        );
        let (w, h) = (resize.width() as usize, resize.height() as usize);
        let mut data = vec![0f32; 3 * w * h];
        for (x, y, px) in resize.enumerate_pixels() {
            let (x, y) = (x as usize, y as usize);
            for c in 0..3usize {
                data[c * w * h + y * w + x] = (px[c] as f32 / 255.0 - 0.5) / 0.5;
            }
        }
        let tenseur = Tensor::from_array((vec![1i64, 3, h as i64, w as i64], data))
            .map_err(|e| ErreurLignes::Modele(e.to_string()))?;
        let sortie = self
            .session
            .run(ort::inputs!["x" => tenseur])
            .map_err(|e| ErreurLignes::Modele(e.to_string()))?;
        let (forme, valeurs) = sortie[0]
            .try_extract_tensor::<f32>()
            .map_err(|e| ErreurLignes::Modele(e.to_string()))?;
        // [1, 1, h, w]
        if forme.len() != 4 || forme[2] as usize != h || forme[3] as usize != w {
            return Err(ErreurLignes::FormeCarte {
                largeur: forme.get(3).copied().unwrap_or(0) as usize,
                hauteur: forme.get(2).copied().unwrap_or(0) as usize,
            });
        }

        let lignes = boites_de_lignes(valeurs, w, h, largeur_orig, hauteur_orig, seuils);
        if lignes.is_empty() {
            Err(ErreurLignes::AucuneLigne)
        } else {
            Ok(lignes)
        }
    }

    /// Détecte les lignes avec les seuils mesurés par défaut.
    pub fn detecter_par_defaut(
        &mut self,
        image: &image::RgbImage,
    ) -> Result<Vec<Rectangle>, ErreurLignes> {
        self.detecter(image, Seuils::default())
    }
}

/// Arrondit une dimension au multiple de 32 **le plus proche**, avec un minimum
/// d'un multiple : une dimension nulle est une forme invalide pour le modèle.
///
/// Arrondi et non arrondi supérieur, **pour être identique à la recette de
/// référence** : la sonde Python utilisait `round()`, donc 462 px donnaient 448.
/// Les deux formes donnaient le même texte une fois le reste corrigé, mais
/// la référence évite une divergence qui n'apparaîtra qu'un jour, sur une autre
/// page. Le charabia rencontré au passage venait d'ailleurs : `crop_imm` attend
/// une largeur et une hauteur, pas un coin.
fn arrondi_au_multiple_32(valeur: u32) -> u32 {
    let demi = MULTIPLE / 2;
    (((valeur.max(1) + demi) / MULTIPLE).max(1)) * MULTIPLE
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Un composant de 119×8 px : c'est la taille **réelle** d'une ligne de webtoon
    /// relevée le 2026-10-07 sur une bulle de dialogue.
    const LIGNE_L: f32 = 119.0;
    const LIGNE_H: f32 = 8.0;

    fn rectangle_dans(m: &mut [bool], largeur: usize, x: usize, y: usize, l: usize, h: usize) {
        for yy in y..(y + h) {
            for xx in x..(x + l) {
                m[yy * largeur + xx] = true;
            }
        }
    }

    /// Deux lignes de taille réelle, plus un pixel isolé comme bruit.
    fn masque_de_test(largeur: usize, hauteur: usize) -> Vec<bool> {
        let mut m = vec![false; largeur * hauteur];
        rectangle_dans(&mut m, largeur, 5, 10, 119, 8);
        rectangle_dans(&mut m, largeur, 20, 40, 188, 8);
        m[100 * largeur + 100] = true;
        m
    }

    fn probas(masque: &[bool]) -> Vec<f32> {
        masque.iter().map(|b| if *b { 1.0 } else { 0.0 }).collect()
    }

    #[test]
    fn deux_blocs_donnes_dont_le_bruit_est_isole() {
        let (l, h) = (256usize, 256usize);
        let trouvees = composantes(&masque_de_test(l, h), l, h);
        assert_eq!(trouvees.len(), 3, "deux blocs et un pixel isolé");
        assert!(
            trouvees
                .iter()
                .all(|c| c.largeur >= 1.0 && c.hauteur >= 1.0),
            "aucun composant vide"
        );
    }

    #[test]
    fn le_bruit_d_un_seul_pixel_est_ecarte_mais_les_lignes_reelles_survivent() {
        let (l, h) = (256usize, 256usize);
        let lignes = boites_de_lignes(
            &probas(&masque_de_test(l, h)),
            l,
            h,
            l as u32,
            h as u32,
            Seuils::default(),
        );
        assert_eq!(lignes.len(), 2, "le pixel isolé ne devient pas une ligne");
        assert!(
            (lignes[0].hauteur - LIGNE_H).abs() < 0.5 && (lignes[0].largeur - LIGNE_L).abs() < 0.5,
            "une ligne de 119x8 px survit : {:?}",
            lignes[0]
        );
    }

    #[test]
    fn les_lignes_sont_renvoyees_de_haut_en_bas() {
        let (l, h) = (256usize, 256usize);
        let lignes = boites_de_lignes(
            &probas(&masque_de_test(l, h)),
            l,
            h,
            l as u32,
            h as u32,
            Seuils::default(),
        );
        assert_eq!(lignes.len(), 2);
        assert!(
            lignes[0].y < lignes[1].y,
            "ordre de lecture : la première ligne est la plus haute ({:?})",
            lignes.iter().map(|r| r.y).collect::<Vec<f32>>()
        );
    }

    #[test]
    fn les_boites_sont_remises_a_l_echelle_de_l_image_d_origine() {
        // Carte deux fois plus petite que l'image : chaque coordonnée doit être
        // doublée en remontant à l'original.
        let (l, h) = (128usize, 64usize);
        let mut masque = vec![false; l * h];
        rectangle_dans(&mut masque, l, 2, 5, 8, 5);
        let lignes = boites_de_lignes(&probas(&masque), l, h, 256, 128, Seuils::default());
        assert_eq!(lignes.len(), 1);
        assert_eq!(lignes[0].x, 4.0, "x doublé");
        assert_eq!(lignes[0].y, 10.0, "y doublé");
        assert_eq!(lignes[0].largeur, 16.0, "largeur doublée");
        assert_eq!(lignes[0].hauteur, 10.0, "hauteur doublée");
    }

    #[test]
    fn le_seuil_decide_de_ce_qui_est_du_texte() {
        let (l, h) = (128usize, 128usize);
        let mut probs = vec![0.0f32; l * h];
        probs[5 * l + 5] = 0.29; // sous le seuil
        probs[6 * l + 6] = 0.31; // au-dessus, mais seul : écarté par la taille
        for y in 10..18 {
            for x in 10..110 {
                probs[y * l + x] = 0.9;
            }
        }
        let lignes = boites_de_lignes(&probs, l, h, l as u32, h as u32, Seuils::default());
        assert_eq!(lignes.len(), 1, "seul le bloc de 100x8 px survit");
        assert!(
            lignes.iter().all(|r| r.x != 5.0 && r.y != 5.0),
            "les pixels sous le seuil ne deviennent pas une ligne"
        );
    }

    #[test]
    fn les_dimensions_entree_tombent_sur_un_multiple_de_32_au_plus_proche() {
        // Valeurs relevées sur la bulle de référence : 462x491 px.
        assert_eq!(arrondi_au_multiple_32(462), 448, "comme round() en Python");
        assert_eq!(arrondi_au_multiple_32(491), 480, "comme round() en Python");
        assert_eq!(arrondi_au_multiple_32(641), 640);
        assert_eq!(arrondi_au_multiple_32(0), 32, "zéro reste valide");
        assert_eq!(arrondi_au_multiple_32(1), 32);
        assert_eq!(arrondi_au_multiple_32(32), 32);
        assert_eq!(arrondi_au_multiple_32(33), 32);
        assert_eq!(
            arrondi_au_multiple_32(100) % MULTIPLE,
            0,
            "toute dimension d'entrée est un multiple de 32, sinon le modèle refuse"
        );
    }

    #[test]
    fn le_seuil_de_hauteur_laisse_passer_une_ligne_de_sept_pixels() {
        // Une ligne mesurée à 7 px de haut (155x7). Avec un seuil de 8, elle
        // disparaissait : c'est le défaut que les tests ont fait apparaître.
        let (l, h) = (256usize, 128usize);
        let mut masque = vec![false; l * h];
        rectangle_dans(&mut masque, l, 10, 10, 155, 7);
        let lignes = boites_de_lignes(
            &probas(&masque),
            l,
            h,
            l as u32,
            h as u32,
            Seuils::default(),
        );
        assert_eq!(lignes.len(), 1, "une ligne de 7 px de haut est du texte");
    }
}
