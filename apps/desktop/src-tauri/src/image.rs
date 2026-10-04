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
}
