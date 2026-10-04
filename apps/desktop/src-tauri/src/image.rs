//! Détection du type réel d'une image, **d'après ses octets**.
//!
//! Pourquoi ne pas faire confiance au `contentType` annoncé : mesuré, une
//! source (HENTAIORIGINES) répond `200 OK` avec la **page HTML** de son
//! interstitiel de vérification d'âge, et l'en-tête.de type est trompeur. Avant
//! cette vérification, 37 chapitres sur 44 ont été écrits dans des `.cbz`
//! valides contenant du HTML sous le nom `001.jpg` : les lecteurs de CBZ
//! affichaient du noir, et la logique « ne jamais retélécharger ce qui est là »
//! les croyait corrects.
//!
//! La règle est donc simple : **on n'écrit jamais des octets dont on n'a pas
//! vérifié qu'ils sont une image**, et l'extension écrite vient de ce que les
//! octets disent, pas de ce que le serveur déclare.

/// Type d'image reconnu. Le vocabulaire est volontairement minimal : ce sont
/// les formats que les sources servent réellement.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageKind {
    Jpeg,
    Png,
    Webp,
    Gif,
    Bmp,
}

impl ImageKind {
    /// Extension de fichier, sans le point.
    pub fn extension(self) -> &'static str {
        match self {
            ImageKind::Jpeg => "jpg",
            ImageKind::Png => "png",
            ImageKind::Webp => "webp",
            ImageKind::Gif => "gif",
            ImageKind::Bmp => "bmp",
        }
    }

    pub fn content_type(self) -> &'static str {
        match self {
            ImageKind::Jpeg => "image/jpeg",
            ImageKind::Png => "image/png",
            ImageKind::Webp => "image/webp",
            ImageKind::Gif => "image/gif",
            ImageKind::Bmp => "image/bmp",
        }
    }
}

/// Reconnaît une image sur ses premiers octets, ou `None`.
///
/// Les signatures sont volontairement strictes : on préfère refuser une image
/// rare plutôt qu'écrire une page web dans un `.cbz`. Chaque format est
///identified par plusieursoctets quand c'est possible, pour ne pas confondre
/// un format avec un autre.
pub fn detect(bytes: &[u8]) -> Option<ImageKind> {
    // JPEG : SOI (FFD8) puis un marqueur (FF Ex / FF C0..FF CF).
    if bytes.len() >= 3 && bytes[0] == 0xFF && bytes[1] == 0xD8 && bytes[2] == 0xFF {
        return Some(ImageKind::Jpeg);
    }
    // PNG : signature complète de 8 octets.
    if bytes.len() >= 8 && bytes[..8] == [0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A] {
        return Some(ImageKind::Png);
    }
    // WebP : "RIFF" .... "WEBP" (l'offset 8 est le type de format).
    if bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        return Some(ImageKind::Webp);
    }
    // GIF : "GIF87a" ou "GIF89a".
    if bytes.len() >= 6 && (&bytes[..6] == b"GIF87a" || &bytes[..6] == b"GIF89a") {
        return Some(ImageKind::Gif);
    }
    // BMP : "BM".
    if bytes.len() >= 2 && &bytes[..2] == b"BM" {
        return Some(ImageKind::Bmp);
    }
    None
}

/// Message d'erreur destiné à l'utilisateur quand les octets ne sont pas une
/// image. Volontairement parlant : la cause la plus fréquente mesurée est un
/// interstitiel de vérification d'âge ou anti-robot, pas une panne réseau.
pub fn not_an_image_message(what: &str, content_type: Option<&str>) -> String {
    let cause = match content_type {
        Some(ct) if !ct.starts_with("image/") => format!("le serveur a annoncé « {ct} »"),
        _ => "le serveur a annoncé un type image".to_string(),
    };
    format!(
        "{what} : ce n'est pas une image ({cause}). \
         La source demande probablement une vérification d'âge ou anti-robot ; \
        refaites la vérification puis relancez le téléchargement."
    )
}

/// Taille en pixels d'une image, lue dans son **en-tête**.
///
/// Pourquoi c'est nécessaire : le mode webtoon (tâche 1.9) doit connaître la
/// hauteur de chaque page **avant** de l'afficher. Mesuré le 2026-10-04 sur
/// Killer Peter (303 pages dans un seul `.cbz`) : sans cela, le navigateur
/// charge les 303 images à l'ouverture et place la mise en page 303 fois. Une
/// estimation CSS (`content-visibility` + `contain-intrinsic-size`) a été
/// essayée et **retirée** : la hauteur de réserve fausse faisait reculer la
/// position de défilement de 3 964 à 1 788 px, donc l'utilisateur perdait sa
/// place. La seule hauteur qui ne trompe pas est celle que l'en-tête annonce.
///
/// On ne décompresse rien : seuls les octets d'en-tête sont lus.
pub fn dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    match detect(bytes)? {
        ImageKind::Jpeg => jpeg_dimensions(bytes),
        ImageKind::Png => png_dimensions(bytes),
        ImageKind::Webp => webp_dimensions(bytes),
        // GIF, BMP : la lecture de l'en-tête est triviale mais ces formats
        // n'apparaissent pas dans un chapitre de manga. `None` = « dimensions
        // inconnues », ce que l'interface sait traiter.
        _ => None,
    }
}

/// JPEG : le cadre est décrit par un marqueur SOF, qui peut être n'importe où
/// après l'en-tête. On parcourt donc les segments plutôt que de deviner un
/// décalage fixe.
fn jpeg_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    if bytes.len() < 4 {
        return None;
    }
    let mut i = 2usize; // après FFD8
    while i + 4 <= bytes.len() {
        if bytes[i] != 0xFF {
            i += 1;
            continue;
        }
        let marqueur = bytes[i + 1];
        // SOF0..SOF15, sauf DHT (C4), JPG (C8) et DAC (CC) qui ne décrivent pas
        // un cadre.
        let est_sof = (0xC0..=0xCF).contains(&marqueur)
            && marqueur != 0xC4
            && marqueur != 0xC8
            && marqueur != 0xCC;
        if est_sof {
            if i + 9 > bytes.len() {
                return None;
            }
            let hauteur = u16::from_be_bytes([bytes[i + 5], bytes[i + 6]]) as u32;
            let largeur = u16::from_be_bytes([bytes[i + 7], bytes[i + 8]]) as u32;
            return (largeur > 0 && hauteur > 0).then_some((largeur, hauteur));
        }
        // Les segments sans charge utile (RSTn, TEM) n'ont pas de longueur.
        if (0xD0..=0xD9).contains(&marqueur) {
            i += 2;
            continue;
        }
        if i + 4 > bytes.len() {
            return None;
        }
        let longueur = u16::from_be_bytes([bytes[i + 2], bytes[i + 3]]) as usize;
        if longueur < 2 {
            return None;
        }
        i += 2 + longueur;
    }
    None
}

fn png_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    // 8 (signature) + 4 (longueur IHDR) + 4 (type) puis largeur et hauteur.
    if bytes.len() < 24 || &bytes[12..16] != b"IHDR" {
        return None;
    }
    let largeur = u32::from_be_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]);
    let hauteur = u32::from_be_bytes([bytes[20], bytes[21], bytes[22], bytes[23]]);
    (largeur > 0 && hauteur > 0).then_some((largeur, hauteur))
}

fn webp_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    let variante = bytes.get(12..16)?;
    match variante {
        // VP8 (avec perte) : tag d'image 3 octets, code de démarrage 3 octets
        // (9d 01 2a), puis largeur et hauteur sur 14 bits chacun.
        b"VP8 " => {
            if bytes.len() < 30 {
                return None;
            }
            if bytes[23..26] != [0x9d, 0x01, 0x2a] {
                return None;
            }
            let largeur = u16::from_le_bytes([bytes[26], bytes[27]]) & 0x3FFF;
            let hauteur = u16::from_le_bytes([bytes[28], bytes[29]]) & 0x3FFF;
            (largeur > 0 && hauteur > 0).then_some((largeur as u32, hauteur as u32))
        }
        // VP8L (sans perte) : signature 2f, puis 14 bits de largeur et 14 bits
        // de hauteur dans un mot de 32 bits, tous deux « moins un ».
        b"VP8L" => {
            if bytes.len() < 25 || bytes[20] != 0x2f {
                return None;
            }
            let v = u32::from_le_bytes([bytes[21], bytes[22], bytes[23], bytes[24]]);
            let largeur = (v & 0x3FFF) + 1;
            let hauteur = ((v >> 14) & 0x3FFF) + 1;
            (largeur > 0 && hauteur > 0).then_some((largeur, hauteur))
        }
        // VP8X (étendu) : drapeaux sur 4 octets, puis la toile en 24 bits.
        b"VP8X" => {
            if bytes.len() < 30 {
                return None;
            }
            let lire24 =
                |a: u8, b: u8, c: u8| u32::from(a) | (u32::from(b) << 8) | (u32::from(c) << 16);
            let largeur = lire24(bytes[24], bytes[25], bytes[26]) + 1;
            let hauteur = lire24(bytes[27], bytes[28], bytes[29]) + 1;
            (largeur > 0 && hauteur > 0).then_some((largeur, hauteur))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn les_formats_reconnus_sont_detectes() {
        assert_eq!(
            detect(&[0xFF, 0xD8, 0xFF, 0xE0, 0x00]),
            Some(ImageKind::Jpeg)
        );
        assert_eq!(
            detect(&[0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0x00]),
            Some(ImageKind::Png)
        );
        let mut webp = b"RIFF".to_vec();
        webp.extend([0, 0, 0, 0]);
        webp.extend(b"WEBPVP8 ");
        assert_eq!(detect(&webp), Some(ImageKind::Webp));
        assert_eq!(detect(b"GIF89a..."), Some(ImageKind::Gif));
        assert_eq!(detect(b"BM......"), Some(ImageKind::Bmp));
    }

    #[test]
    fn une_page_web_nest_pas_une_image() {
        // Le cas mesuré : l'interstitiel de vérification d'âge.
        let html = b"    \n<!DOCTYPE html>\n<html lang=\"fr-FR\">";
        assert_eq!(detect(html), None);
        // Un JSON d'erreur, lui aussi.
        assert_eq!(detect(br#"{"code":"INTERNAL"}"#), None);
        // Des octets vides ou trop courts.
        assert_eq!(detect(&[]), None);
        assert_eq!(detect(&[0xFF]), None);
        // Un JPEG tronqué en tête reste refusé : on n'écrit pas à l'aveugle.
        assert_eq!(detect(&[0xFF, 0xD8]), None);
    }

    #[test]
    fn lextension_vient_du_contenu_reel() {
        assert_eq!(ImageKind::Webp.extension(), "webp");
        assert_eq!(ImageKind::Jpeg.content_type(), "image/jpeg");
        // Un `text/html` annoncé ne doit jamais produire une extension image :
        // c'est exactement le défaut qui a produit 37 `.cbz` illisibles.
        assert_ne!(ImageKind::Jpeg.extension(), "html");
    }

    /// JPEG minimal : SOI, puis un segment APP0 de 16 octets, puis SOF0.
    fn jpeg(largeur: u16, hauteur: u16) -> Vec<u8> {
        let mut v = vec![0xFF, 0xD8, 0xFF];
        v.extend_from_slice(&[0xE0, 0x00, 0x04, 0x00, 0x00]); // APP0, longueur 4
        v.extend_from_slice(&[0xFF, 0xC0, 0x00, 0x11, 0x08]);
        v.extend_from_slice(&hauteur.to_be_bytes());
        v.extend_from_slice(&largeur.to_be_bytes());
        v.extend_from_slice(&[0x03, 0x01, 0x11, 0x00, 0x02, 0x11, 0x01, 0x03, 0x11, 0x01]);
        v
    }

    #[test]
    fn lit_les_dimensions_d_un_jpeg() {
        // 800 x 2171 : le format d'une page de webtoon.
        assert_eq!(dimensions(&jpeg(800, 2171)), Some((800, 2171)));
    }

    #[test]
    fn lit_les_dimensions_d_un_png() {
        let mut v = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        v.extend_from_slice(&13u32.to_be_bytes());
        v.extend_from_slice(b"IHDR");
        v.extend_from_slice(&1234u32.to_be_bytes());
        v.extend_from_slice(&5678u32.to_be_bytes());
        v.extend_from_slice(&[8, 6, 0, 0, 0]);
        assert_eq!(dimensions(&v), Some((1234, 5678)));
    }

    #[test]
    fn lit_les_dimensions_d_un_webp_avec_perte() {
        // Structure réelle mesurée sur une page WebP : tag + 9d 01 2a.
        let mut v = b"RIFF\0\0\0\0WEBPVP8 ".to_vec();
        v.extend_from_slice(&[0x1a, 0, 0, 0]); // taille du chunk
        v.extend_from_slice(&[0x30, 0x01, 0x00]); // tag
        v.extend_from_slice(&[0x9d, 0x01, 0x2a]); // code de démarrage
        v.extend_from_slice(&(800u16).to_le_bytes());
        v.extend_from_slice(&(2171u16).to_le_bytes());
        assert_eq!(dimensions(&v), Some((800, 2171)));
    }

    #[test]
    fn refuse_une_page_web_qui_n_a_pas_de_cadre() {
        assert_eq!(dimensions(b"    \n<!DOCTYPE html>"), None);
        assert_eq!(dimensions(&[]), None);
        // WebP sans code de démarrage valide : on ne devine pas.
        let mut v = b"RIFF\0\0\0\0WEBPVP8 ".to_vec();
        v.extend_from_slice(&[0x1a, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        assert_eq!(dimensions(&v), None);
    }

    #[test]
    fn la_page_reelle_du_probleme_est_deja_lue() {
        // Les 31 premiers octets de `032.webp`, la page trop petite du chapitre
        // HENTAIORIGINES, relevés dans le `.cbz` le 2026-10-04.
        //
        // Attention : ce fichier avait été décodé à la main en « 413 x 42 »,
        // ce qui était **faux**. Les octets disent 800 x 50. C'est exactement
        // pourquoi les dimensions sont lues par code et pas estimées à l'œil.
        let octets: Vec<u8> = vec![
            0x52, 0x49, 0x46, 0x46, 0x2e, 0x01, 0x00, 0x00, 0x57, 0x45, 0x42, 0x50, 0x56, 0x50,
            0x38, 0x20, 0x22, 0x01, 0x00, 0x00, 0x70, 0x13, 0x00, 0x9d, 0x01, 0x2a, 0x20, 0x03,
            0x32, 0x00, 0x3e,
        ];
        assert_eq!(
            dimensions(&octets),
            Some((800, 50)),
            "la taille du placeholder doit etre lue, pas devinee"
        );
    }
}
