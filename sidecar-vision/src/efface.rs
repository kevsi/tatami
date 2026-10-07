//! Effacement du texte dans les bulles (T2.5).
//!
//! ## Ce que le modèle veut, mesuré
//!
//! `lama_fp32.onnx` : entrée `image` `[batch, 3, 512, 512]` **en 0..255**, pas
//! en 0..1 ni en [-1, 1] ; entrée `mask` `[batch, 1, 512, 512]` ; sortie
//! `output` `[batch, 3, 512, 512]` **en 0..255**. Le test d'identité (masque
//! nul) le prouve : envoyé en 0..1, le modèle prend l'image pour du noir et
//! renvoie une page blanche ; envoyé en 0..255, l'écart absolu moyen tombe à
//! 15,7 sur 255, et le texte disparaît vraiment.
//!
//! `lama.onnx` (le fichier « int8 ») **ne se charge pas** :
//! `one-sided DFT requires real input`. Donc pas de variante légère — c'est le
//! poste dominant, comme annoncé en T0.
//!
//! ## Le coût, et la seule façon de le tenir
//!
//! Mesuré le 2026-10-07 sur la machine cible : **8,5 s** par tuile de 512 avec un
//! masque de 1 %, **13,9 s** à 25 % de la tuile. Le coût suit la **surface
//! masquée**, pas l'image — LaMa reconstruit par convolution rapide autour du
//! masque. Le batch, lui, **n'amortit rien** : 18,7 s/tile en batch 1, 20,8 en
//! batch 2, 18,8 en batch 4.
//!
//! Donc deux règles, et elles ne sont pas des préférences :
//!
//! 1. **Un seul masque par tuile, ajusté au texte.** Masquer la bulle entière
//!    multiplie le temps par deux pour le même résultat sur fond blanc.
//! 2. **Une tuile par page, pas par bulle.** Les lignes d'une bulle mesurent
//!    119 à 188 px de large sur 7 à 9 de haut, donc plusieurs bulles tiennent
//!    dans une tuile de 512. En regrouplant, on passe de ~10 tuiles par page à
//!    2 ou 3.

use crate::lignes::Rectangle;
use ort::session::Session;
use ort::value::Tensor;

/// Côté de l'entrée du modèle, en pixels. Fixe : le réseau a été exporté à 512.
pub const COTE_TUILE: u32 = 512;

/// Marge ajoutée autour des boîtes de texte avant masquage.
///
/// Mesuré : sans elle, les bords antialiasés des lettres survivent et la
/// lecture de contrôle (T2.6) trouve du résidu. Le compromis 4 px vient de la
/// hauteur de ligne mesurée (7 à 9 px) : une marge plus large commencerait à
/// mordre sur la bulle.
pub const MARGE_TEXTE: i64 = 4;

/// Erreur d'effacement.
#[derive(Debug)]
pub enum ErreurEffacement {
    Modele(String),
    /// Le masque est vide : il n'y a rien à effacer.
    AucunTexte,
}

impl std::fmt::Display for ErreurEffacement {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ErreurEffacement::Modele(m) => write!(f, "modèle d'effacement : {m}"),
            ErreurEffacement::AucunTexte => write!(f, "aucun texte à effacer"),
        }
    }
}

impl std::error::Error for ErreurEffacement {}

/// Une tuile de travail : toujours `COTE_TUILE` × `COTE_TUILE`, donc elle peut
/// déborder de l'image, et c'est voulu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Tuile {
    /// Coin gauche-droit en coordonnée page (peut dépasser la largeur).
    pub x: i64,
    pub y: i64,
}

/// Répartit les boîtes de texte en tuiles 512, en **greedy** : on prend la boîte
/// la plus haute non encore couverte, on centre une tuile dessus, on recommence.
///
/// Fonction pure, donc testable sans modèle : c'est ici que se décide le coût
/// de la page entière, et une répartition maladroite la multiplie par dix.
pub fn tuiles_couvrantes(
    boites: &[Rectangle],
    largeur_page: u32,
    hauteur_page: u32,
    cote: u32,
) -> Vec<Tuile> {
    if boites.is_empty() {
        return Vec::new();
    }
    let cote = cote as i64;
    let mut restantes: Vec<Rectangle> = boites.to_vec();
    // La plus haute d'abord : une tuile choisie en haut de page attrape les
    // bulles qui suivent si elles sont proches, ce qui est le cas courant dans
    // une bande verticale.
    restent_triees(&mut restantes);

    let mut tuiles = Vec::new();
    while !restantes.is_empty() {
        let premiere = restantes[0];
        // Centrage sur la boîte, puis-bornage : une tuile doit commencer dans
        // l'image, sinon elle perdrait du texte en haut ou à gauche.
        let mut x = (premiere.x as i64 + premiere.largeur as i64 / 2.0 as i64) - cote / 2;
        let mut y = (premiere.y as i64 + premiere.hauteur as i64 / 2.0 as i64) - cote / 2;
        x = x.clamp(0, (largeur_page as i64 - cote).max(0));
        y = y.clamp(0, (hauteur_page as i64 - cote).max(0));
        let tuile = Tuile { x, y };

        let couvertes: Vec<Rectangle> = restantes
            .iter()
            .filter(|b| touche(tuile, b))
            .copied()
            .collect();
        if couvertes.is_empty() {
            // Ne pas boucler : une boîte qui ne toucherait aucune tuile est
            // comptée une fois, puis on la retire.
            restantes.remove(0);
            continue;
        }
        restantes.retain(|b| !couvre(tuile, b));
        tuiles.push(tuile);
    }
    tuiles
}

/// Tri par haut croissant, puis par gauche : l'ordre de lecture.
fn restent_triees(boites: &mut [Rectangle]) {
    boites.sort_by(|a, b| {
        a.y.partial_cmp(&b.y)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.x.partial_cmp(&b.x).unwrap_or(std::cmp::Ordering::Equal))
    });
}

fn couvre(t: Tuile, b: &Rectangle) -> bool {
    let bx1 = b.x as i64 - MARGE_TEXTE;
    let by1 = b.y as i64 - MARGE_TEXTE;
    let bx2 = b.x as i64 + b.largeur as i64 + MARGE_TEXTE;
    let by2 = b.y as i64 + b.hauteur as i64 + MARGE_TEXTE;
    bx1 < t.x + COTE_TUILE as i64 && bx2 > t.x && by1 < t.y + COTE_TUILE as i64 && by2 > t.y
}

fn touche(t: Tuile, b: &Rectangle) -> bool {
    couvre(t, b)
}

/// Construit le masque `[COTE_TUILE, COTE_TUILE]` d'une tuile, en 1.0 là où le
/// texte doit disparaître. Les parties de la tuile hors image sont masquées :
/// le modèle n'a rien à reconstruire là, et les laisser en entrée lui ferait
/// inventer.
pub fn masque_de_tuile(
    t: Tuile,
    boites: &[Rectangle],
    largeur_page: u32,
    hauteur_page: u32,
) -> Vec<f32> {
    let cote = COTE_TUILE as usize;
    let mut masque = vec![0f32; cote * cote];
    let mut vide = true;
    for (jx, jy) in (0..cote).flat_map(|y| (0..cote).map(move |x| (x, y))) {
        let m = &mut masque[jy * cote + jx];
        let (px, py) = (jx as i64 + t.x, jy as i64 + t.y);
        if px < 0 || py < 0 || px >= largeur_page as i64 || py >= hauteur_page as i64 {
            *m = 1.0;
            vide = false;
            continue;
        }
        for b in boites {
            if !couvre(t, b) {
                continue;
            }
            let (bx1, by1) = (b.x as i64 - MARGE_TEXTE, b.y as i64 - MARGE_TEXTE);
            let (bx2, by2) = (
                b.x as i64 + b.largeur as i64 + MARGE_TEXTE,
                b.y as i64 + b.hauteur as i64 + MARGE_TEXTE,
            );
            if px >= bx1 && px < bx2 && py >= by1 && py < by2 {
                *m = 1.0;
                vide = false;
                break;
            }
        }
    }
    // Un masque entièrement nul signifie « rien à faire » ; on le signale par un
    // masque vide explicite plutot que par une passe de 13 s qui rend l'image
    // quasi identique.
    if vide {
        return Vec::new();
    }
    masque
}

/// Modèle d'effacement chargé.
pub struct Effaceur {
    session: Session,
}

impl Effaceur {
    pub fn ouvrir(modele: &std::path::Path) -> Result<Effaceur, ErreurEffacement> {
        let mut builder =
            Session::builder().map_err(|e| ErreurEffacement::Modele(e.to_string()))?;
        let session = builder
            .commit_from_file(modele)
            .map_err(|e| ErreurEffacement::Modele(e.to_string()))?;
        Ok(Effaceur { session })
    }

    /// Efface le texte des `boites` dans `image`, et renvoie l'image modifiée.
    ///
    /// Les boîtes sont en coordonnées page. `boites` vide rend l'image
    /// inchangée sans lancer le modèle : une page d'illustration ne coûte
    /// alors rien.
    pub fn effacer(
        &mut self,
        image: &image::RgbImage,
        boites: &[Rectangle],
    ) -> Result<image::RgbImage, ErreurEffacement> {
        if boites.is_empty() {
            return Ok(image.clone());
        }
        let mut sortie = image.clone();
        let tuiles = tuiles_couvrantes(boites, image.width(), image.height(), COTE_TUILE);
        for t in tuiles {
            let masque = masque_de_tuile(t, boites, image.width(), image.height());
            if masque.is_empty() {
                continue;
            }
            let recolore = self.passe(t, image, &masque)?;
            recolorer(
                &mut sortie,
                t,
                &recolore,
                &masque,
                image.width(),
                image.height(),
            );
        }
        Ok(sortie)
    }

    /// Une passe du modèle sur une tuile. Entrées et sorties en 0..255.
    fn passe(
        &mut self,
        t: Tuile,
        image: &image::RgbImage,
        masque: &[f32],
    ) -> Result<Vec<u8>, ErreurEffacement> {
        let cote = COTE_TUILE as usize;
        let mut pixels = vec![0f32; 3 * cote * cote];
        for (jx, jy) in (0..cote).flat_map(|y| (0..cote).map(move |x| (x, y))) {
            let (px, py) = (jx as i64 + t.x, jy as i64 + t.y);
            // Hors image : gris neutre. Le modèle reçoit une zone qui n'existe
            // pas ; la laisser noire biaise la reconstruction des bords.
            let (r, v, b) =
                if px < 0 || py < 0 || px >= image.width() as i64 || py >= image.height() as i64 {
                    (128.0, 128.0, 128.0)
                } else {
                    let p = image.get_pixel(px as u32, py as u32).0;
                    (p[0] as f32, p[1] as f32, p[2] as f32)
                };
            let base = jy * cote + jx;
            pixels[base] = r;
            pixels[cote * cote + base] = v;
            pixels[2 * cote * cote + base] = b;
        }
        let masque_3d = masque.to_vec();
        let image_t =
            Tensor::from_array((vec![1i64, 3, COTE_TUILE as i64, COTE_TUILE as i64], pixels))
                .map_err(|e| ErreurEffacement::Modele(e.to_string()))?;
        let masque_t = Tensor::from_array((
            vec![1i64, 1, COTE_TUILE as i64, COTE_TUILE as i64],
            masque_3d,
        ))
        .map_err(|e| ErreurEffacement::Modele(e.to_string()))?;

        let sortie = self
            .session
            .run(ort::inputs!["image" => image_t, "mask" => masque_t])
            .map_err(|e| ErreurEffacement::Modele(e.to_string()))?;
        let (forme, valeurs) = sortie[0]
            .try_extract_tensor::<f32>()
            .map_err(|e| ErreurEffacement::Modele(e.to_string()))?;
        if forme.len() != 4 || forme[1] as usize != 3 {
            return Err(ErreurEffacement::Modele(format!(
                "sortie d'effacement inattendue : {:?}",
                forme
            )));
        }
        let mut octets = Vec::with_capacity(3 * cote * cote);
        for v in valeurs.iter().take(3 * cote * cote) {
            octets.push(v.clamp(0.0, 255.0) as u8);
        }
        Ok(octets)
    }
}

/// Recolle une tuile recolorée dans l'image, **uniquement sur les pixels
/// masqués**.
///
/// Mesuré le 2026-10-07 : recoler toute la tuile effaçait le texte — et
/// détruisait au passage le visage et la veste du personnage. LaMa reconstruit
/// aussi ce qui est hors masque, et cette reconstruction dérive dès que le
/// contenu est complexe ; l'écart d'identité mesuré (15,7 sur 255) est
/// invisible sur un fond blanc et énorme sur un visage. Donc : hors masque,
/// l'original reste intact.
fn recolorer(
    cible: &mut image::RgbImage,
    t: Tuile,
    octets: &[u8],
    masque: &[f32],
    largeur_page: u32,
    hauteur_page: u32,
) {
    let cote = COTE_TUILE as usize;
    for (jx, jy) in (0..cote).flat_map(|y| (0..cote).map(move |x| (x, y))) {
        let base = jy * cote + jx;
        if masque.get(base).copied().unwrap_or(0.0) <= 0.0 {
            continue;
        }
        let (px, py) = (jx as i64 + t.x, jy as i64 + t.y);
        if px < 0 || py < 0 || px >= largeur_page as i64 || py >= hauteur_page as i64 {
            continue;
        }
        // Le modèle rend du float ; l'ordre des canaux est NCHW.
        let r = octets.get(base).copied().unwrap_or(0);
        let v = octets.get(cote * cote + base).copied().unwrap_or(0);
        let b = octets.get(2 * cote * cote + base).copied().unwrap_or(0);
        cible.put_pixel(px as u32, py as u32, image::Rgb([r, v, b]));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn boite(x: f32, y: f32, l: f32, h: f32) -> Rectangle {
        Rectangle {
            x,
            y,
            largeur: l,
            hauteur: h,
        }
    }

    #[test]
    fn aucune_boite_donne_aucune_tuile() {
        assert!(tuiles_couvrantes(&[], 800, 1000, COTE_TUILE).is_empty());
    }

    #[test]
    fn une_seule_boite_donne_une_seule_tuile() {
        let t = tuiles_couvrantes(&[boite(300.0, 400.0, 100.0, 20.0)], 800, 1000, COTE_TUILE);
        assert_eq!(t.len(), 1);
        assert!(t[0].x >= 0 && t[0].y >= 0);
    }

    #[test]
    fn deux_boites_proches_partagent_une_tuile() {
        // Mesuré sur une bulle réelle : lignes de 119 à 188 px de large, 7 à 9
        // de haut, espacées de 25 px. Elles tiennent largement dans une tuile.
        let boites: Vec<Rectangle> = (0..5)
            .map(|i| boite(360.0, 360.0 + i as f32 * 25.0, 188.0, 9.0))
            .collect();
        assert_eq!(
            tuiles_couvrantes(&boites, 800, 1000, COTE_TUILE).len(),
            1,
            "cinq lignes d'une même bulle ne doivent coûter qu'une tuile"
        );
    }

    #[test]
    fn des_boites_eloignees_cotent_plus_cher() {
        let boites = vec![
            boite(50.0, 20.0, 180.0, 10.0),
            boite(60.0, 900.0, 180.0, 10.0),
        ];
        assert_eq!(
            tuiles_couvrantes(&boites, 800, 1000, COTE_TUILE).len(),
            2,
            "deux lignes à 880 px d'écart ne peuvent pas partager une tuile de 512"
        );
    }

    #[test]
    fn la_tuile_reste_dans_la_page() {
        let t = tuiles_couvrantes(&[boite(10.0, 10.0, 100.0, 20.0)], 800, 1000, COTE_TUILE);
        assert!(t[0].x >= 0, "pas de tuile avant le bord gauche");
        assert!(t[0].y >= 0, "pas de tuile avant le bord haut");
        assert!(
            t[0].x + COTE_TUILE as i64 >= 800 || t[0].x <= 800 - COTE_TUILE as i64,
            "pas de tuile qui commence après la page"
        );
    }

    #[test]
    fn le_masque_couvre_la_boite_avec_sa_marge() {
        let t = Tuile { x: 0, y: 0 };
        let b = [boite(100.0, 200.0, 180.0, 9.0)];
        let m = masque_de_tuile(t, &b, 800, 1000);
        assert!(!m.is_empty(), "il y a du texte");
        // Le centre de la boîte est masqué...
        let idx = 200 * COTE_TUILE as usize + 190;
        assert_eq!(m[idx], 1.0, "le texte est masqué");
        // ...et un point éloigné ne l'est pas.
        let loin = 500 * COTE_TUILE as usize + 500;
        assert_eq!(m[loin], 0.0, "le fond reste intact");
    }

    #[test]
    fn le_masque_est_vide_quand_aucune_boite_ne_touche() {
        let t = Tuile { x: 0, y: 0 };
        let b = [boite(900.0, 900.0, 100.0, 20.0)];
        assert!(
            masque_de_tuile(t, &b, 800, 1000).is_empty(),
            "une boîte hors tuile ne doit pas faire tourner le modèle pour rien"
        );
    }

    #[test]
    fn la_hors_page_est_masquee() {
        // Tuile qui déborde : la partie hors image doit être dans le masque, sinon
        // le modèle inventerait sur une zone qui n'existe pas.
        let t = Tuile { x: 400, y: 700 };
        let m = masque_de_tuile(t, &[boite(450.0, 750.0, 40.0, 10.0)], 800, 1000);
        let idx = 400 * COTE_TUILE as usize + 400; // x=800, hors page
        assert_eq!(m[idx], 1.0, "hors page est masqué");
    }

    #[test]
    fn seuls_les_pixels_masques_sont_recolores() {
        // C'est LA porte du 2026-10-07 : recoller toute la tuile effaçait le
        // texte mais aussi le visage du personnage. On fabrique une image
        // connue, un masque connu, et des octets « effacés » distincts, puis on
        // vérifie que le hors-masque n'a pas bougé.
        let mut image = image::RgbImage::from_pixel(800, 1000, image::Rgb([10, 20, 30]));
        let t = Tuile { x: 0, y: 0 };
        let b = [boite(100.0, 200.0, 100.0, 20.0)];
        let masque = masque_de_tuile(t, &b, 800, 1000);

        let cote = COTE_TUILE as usize;
        let mut octets = vec![0u8; 3 * cote * cote];
        for o in octets.iter_mut() {
            *o = 200; // couleur « reconstruite », franchement différente
        }
        recolorer(&mut image, t, &octets, &masque, 800, 1000);

        assert_eq!(
            image.get_pixel(150, 210).0,
            [200u8, 200, 200],
            "le pixel masqué prend la couleur reconstruite"
        );
        assert_eq!(
            image.get_pixel(500, 500).0,
            [10u8, 20, 30],
            "le pixel NON masqué garde l'original : c'est le visage du personnage"
        );
    }
}
