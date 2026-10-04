//! Ecriture CBZ (ZIP non compresse, mode "stored") sans dependance externe.
//!
//! Un CBZ = un ZIP dont les entrees sont des images, dans l'ordre des pages.
//! Les images sont deja compressees (webp/jpg) : la compression ZIP n'ajouterait
//! rien et degrade le resultat, on ecrit donc en mode stored.
//!
//! Le fichier est ecrit au fil de l'eau : un chapitre de 300 pages ne tient
//! jamais en memoire. Seul le repertoire central (≈60 octets par page) est
//! conserve, ce qui est negligeable.

use std::io::{self, Write};
use std::path::Path;

pub struct CbzWriter<W: Write> {
    out: W,
    /// (nom, offset local, crc32, taille)
    ///
    /// `offset` est un `u64` et non un `u32` : le format ZIP32 impose des
    /// offsets 32 bits, mais un chapitre de plus de 4 Gio dépassait la capacité
    /// et **`as u32` tronquait en silence** — un `.cbz` produit, de la bonne
    /// taille, illisible. On suit donc la position réelle en `u64` et on refuse
    /// explicitement de dépasser la limite, au lieu d'écrire un fichier corrompu.
    entries: Vec<(String, u64, u32, u64)>,
    offset: u64,
}

/// Plus Offset32 bits : au-delà, ce n'est plus du ZIP lisible.
const LIMITE_ZIP32: u64 = u32::MAX as u64;

impl<W: Write> CbzWriter<W> {
    pub fn new(out: W) -> Self {
        Self {
            out,
            entries: Vec::new(),
            offset: 0,
        }
    }

    /// Ecrit une page dont le type a été **vérifié sur les octets**.
    ///
    /// C'est la seule voie d'écriture. Une version antérieure acceptait un
    /// `contentType` et en déduisait l'extension : une source qui répond
    /// `200 OK` avec la page HTML de son interstitiel de vérification d'âge
    /// produisait alors un `.cbz` valide plein de `001.jpg` illisibles
    /// (mesuré : 37 chapitres sur 44). L'extension dit maintenant la vérité.
    pub fn add_image_kind(
        &mut self,
        index: usize,
        kind: crate::image::ImageKind,
        bytes: &[u8],
    ) -> io::Result<()> {
        self.add_named(index, kind.extension(), bytes)
    }

    fn add_named(&mut self, index: usize, ext: &str, bytes: &[u8]) -> io::Result<()> {
        let name = format!("{:03}.{ext}", index);
        let name_bytes = name.as_bytes();
        let crc = crc32(bytes);
        let size = bytes.len() as u64;
        let local_offset = self.offset;
        // Refus explicite plutôt qu'un fichier silencieusement corrompu : un
        // chapitre de plus de 4 Gio n'est pas lisible dans un lecteur externe.
        let taille_entree = 30u64 + name_bytes.len() as u64 + size;
        if local_offset + taille_entree > LIMITE_ZIP32 {
            return Err(io::Error::other(format!(
                "chapitre trop volumineux pour un .cbz (ZIP32) : page {index}, \
                 {}.{} Go déjà écrits",
                local_offset / 1_000_000_000,
                (local_offset % 1_000_000_000) / 1_000_000
            )));
        }

        let mut header = [0u8; 30];
        header[0..4].copy_from_slice(&0x0403_4b50u32.to_le_bytes());
        header[4..6].copy_from_slice(&20u16.to_le_bytes()); // version needed
        header[6..8].copy_from_slice(&0u16.to_le_bytes()); // flags
        header[8..10].copy_from_slice(&0u16.to_le_bytes()); // method = stored
        header[10..12].copy_from_slice(&0u16.to_le_bytes()); // mod time
        header[12..14].copy_from_slice(&0x21u16.to_le_bytes()); // date 1980-01-01
        header[14..18].copy_from_slice(&crc.to_le_bytes());
        header[18..22].copy_from_slice(&(size as u32).to_le_bytes());
        header[22..26].copy_from_slice(&(size as u32).to_le_bytes());
        header[26..28].copy_from_slice(&(name_bytes.len() as u16).to_le_bytes());
        header[28..30].copy_from_slice(&0u16.to_le_bytes()); // extra field len

        self.out.write_all(&header)?;
        self.out.write_all(name_bytes)?;
        self.out.write_all(bytes)?;

        self.entries.push((name, local_offset, crc, size));
        self.offset += taille_entree;
        Ok(())
    }

    /// Repertoire central + EOCD, puis vide le tampon. Renvoie la taille écrite.
    /// Écrit le répertoire central + l'EOCD et rend la **taille totale du fichier**.
    ///
    /// La taille inclut le répertoire central et les 22 octets de l'EOCD : c'est
    /// elle que le téléchargeur compare au fichier réel. Omettre l'un des deux
    /// donnerait un compte faux — un test l'aattrapé avant qu'il n'aille en
    /// production.
    fn ecrire_repertoire_central(&mut self) -> io::Result<u64> {
        let cd_offset = self.offset;
        let mut cd = Vec::with_capacity(46 * self.entries.len());
        for (name, local_offset, crc, size) in &self.entries {
            let name_bytes = name.as_bytes();
            cd.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
            cd.extend_from_slice(&20u16.to_le_bytes()); // version made by
            cd.extend_from_slice(&20u16.to_le_bytes()); // version needed
            cd.extend_from_slice(&0u16.to_le_bytes()); // flags
            cd.extend_from_slice(&0u16.to_le_bytes()); // method stored
            cd.extend_from_slice(&0u16.to_le_bytes()); // mod time
            cd.extend_from_slice(&0x21u16.to_le_bytes()); // date
            cd.extend_from_slice(&crc.to_le_bytes());
            cd.extend_from_slice(&(*size as u32).to_le_bytes());
            cd.extend_from_slice(&(*size as u32).to_le_bytes());
            cd.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
            cd.extend_from_slice(&0u16.to_le_bytes()); // extra len
            cd.extend_from_slice(&0u16.to_le_bytes()); // comment len
            cd.extend_from_slice(&0u16.to_le_bytes()); // disk number
            cd.extend_from_slice(&0u16.to_le_bytes()); // internal attrs
            cd.extend_from_slice(&0u32.to_le_bytes()); // external attrs
            cd.extend_from_slice(&(*local_offset as u32).to_le_bytes());
            cd.extend_from_slice(name_bytes);
        }
        let cd_size = cd.len() as u32;
        self.out.write_all(&cd)?;
        self.out.write_all(&0x0605_4b50u32.to_le_bytes())?;
        self.out.write_all(&0u16.to_le_bytes())?; // disk
        self.out.write_all(&0u16.to_le_bytes())?; // cd start disk
        self.out
            .write_all(&(self.entries.len() as u16).to_le_bytes())?;
        self.out
            .write_all(&(self.entries.len() as u16).to_le_bytes())?;
        self.out.write_all(&cd_size.to_le_bytes())?;
        self.out.write_all(&(cd_offset as u32).to_le_bytes())?;
        self.out.write_all(&0u16.to_le_bytes())?; // comment len
        self.out.flush()?;
        Ok(cd_offset + u64::from(cd_size) + 22)
    }

    /// Rend la main à l'écrivain sous-jacent (utilisé par les tests).
    #[cfg(test)]
    pub fn into_inner(self) -> W {
        self.out
    }

    /// Fin sans synchronisation — **tests uniquement**.
    ///
    /// Le chemin de production est `sync_and_finish`. Ce wrapper existe parce
    /// que le nom `finish` seul ne doit plus être atteignable : un `.cbz`
    /// non synchronisé puis renommé est exactement le fichier tronqué que
    /// `sync_all` sert à empêcher, et le nom ne rappelait pas ce risque.
    ///
    /// Les tests de `cbz` écrivent dans un `Vec<u8>` : il n'y a pas de disque,
    /// donc rien à synchroniser.
    #[cfg(test)]
    pub fn finish_vec(&mut self) -> io::Result<u64> {
        self.ecrire_repertoire_central()
    }
}

impl CbzWriter<std::io::BufWriter<std::fs::File>> {
    /// Cree le fichier et renvoie un ecrivain tamponne dessus.
    pub fn create(path: &Path) -> io::Result<Self> {
        let file = std::fs::File::create(path)?;
        Ok(CbzWriter::new(std::io::BufWriter::with_capacity(
            256 * 1024,
            file,
        )))
    }

    /// Vide le tampon ET force l'écriture sur le support.
    ///
    /// `flush` seul ne protège de rien : c'est le `rename` suivant qui rend le
    /// nom définitif, et une coupure entre les deux laissait un `.cbz` tronqué
    /// portant le bon nom. Or `missing_in_dir` ne teste que la présence du nom :
    /// le chapitre comptait « téléchargé » alors que le fichier ne s'ouvrait
    /// pas — l'irréparable, et invisible jusqu'au jour où l'utilisateur
    /// l'ouvre.
    pub fn sync_and_finish(&mut self) -> io::Result<u64> {
        let total = self.ecrire_repertoire_central()?;
        self.out
            .get_ref()
            .sync_all()
            .map_err(|e| io::Error::other(format!("synchronisation du .cbz impossible : {e}")))?;
        Ok(total)
    }
}

/// CRC32 (IEEE) — la même valeur que celle écrite dans chaque en-tête local.
///
/// Public pour `zipread`, qui **vérifie** ce CRC en relisant une entrée : c'est
/// ce contrôle qui fait qu'un `.cbz` corrompu est refusé au lieu d'être
/// affiché en bordel.
pub fn crc32_public(data: &[u8]) -> u32 {
    crc32(data)
}

fn crc32(data: &[u8]) -> u32 {
    let mut table = [0u32; 256];
    for (i, slot) in table.iter_mut().enumerate() {
        let mut c = i as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 {
                0xEDB8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
        }
        *slot = c;
    }
    let mut crc = 0xFFFF_FFFFu32;
    for byte in data {
        crc = table[((crc ^ *byte as u32) & 0xFF) as usize] ^ (crc >> 8);
    }
    crc ^ 0xFFFF_FFFF
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build(pages: &[crate::image::ImageKind]) -> Vec<u8> {
        let mut w = CbzWriter::new(Vec::new());
        for (i, kind) in pages.iter().enumerate() {
            w.add_image_kind(i + 1, *kind, kind.content_type().as_bytes())
                .unwrap();
        }
        w.finish_vec().unwrap();
        w.into_inner()
    }

    #[test]
    fn cbz_has_local_header_and_central_directory() {
        let bytes = build(&[crate::image::ImageKind::Webp, crate::image::ImageKind::Jpeg]);
        assert_eq!(&bytes[0..4], &[0x50, 0x4b, 0x03, 0x04]);
        assert_eq!(
            &bytes[bytes.len() - 22..bytes.len() - 18],
            &[0x50, 0x4b, 0x05, 0x06]
        );
    }

    #[test]
    fn entrees_en_mode_stored_sans_recompression() {
        // Un lecteur ZIP doit retrouver les octets exacts et une taille
        // compressee egale a la taille reelle (aucune compression appliquee).
        let mut w = CbzWriter::new(Vec::new());
        let payload = vec![7u8; 4096];
        w.add_image_kind(1, crate::image::ImageKind::Jpeg, &payload)
            .unwrap();
        w.finish_vec().unwrap();
        let bytes = w.into_inner();
        assert_eq!(&bytes[0..4], &[0x50, 0x4b, 0x03, 0x04]);
        // methode = 0 (stored) dans l'en-tete local
        assert_eq!(&bytes[8..10], &0u16.to_le_bytes());
        assert!(bytes.windows(7).any(|f| f == b"001.jpg"));
        let expected = 30 + 7 + payload.len() + 46 + 7 + 22;
        assert_eq!(bytes.len(), expected, "taille inattendue");
    }

    #[test]
    fn ecriture_fichier_reel_via_create() {
        let dir = std::env::temp_dir().join(format!("tatami-cbz-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("test.cbz");
        let written = {
            let mut w = CbzWriter::create(&path).unwrap();
            w.add_image_kind(1, crate::image::ImageKind::Png, b"abc")
                .unwrap();
            w.add_image_kind(2, crate::image::ImageKind::Png, b"de")
                .unwrap();
            // Le chemin de production, celui qui synchronise.
            w.sync_and_finish().unwrap()
        };
        let on_disk = std::fs::read(&path).unwrap();
        assert_eq!(on_disk.len() as u64, written);
        assert_eq!(&on_disk[0..4], &[0x50, 0x4b, 0x03, 0x04]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
