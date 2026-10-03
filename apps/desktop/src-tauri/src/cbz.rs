//! Ecriture CBZ (ZIP non compresse, mode "stored") sans dependance externe.
//!
//! Un CBZ = un ZIP dont les entrees sont des images, dans l'ordre des pages.
//! Les images sont deja compressees (webp/jpg) : la compression ZIP n'ajouterait
//! rien, on ecrit donc en mode stored (le lecteur de CBZ l'accepte tel quel).

pub struct CbzWriter {
    buf: Vec<u8>,
    entries: Vec<(String, u32, u32, u32)>, // (nom, offset local, crc32, taille)
    offset: u32,
}

impl Default for CbzWriter {
    fn default() -> Self {
        Self::new()
    }
}

impl CbzWriter {
    pub fn new() -> Self {
        Self {
            buf: Vec::new(),
            entries: Vec::new(),
            offset: 0,
        }
    }

    pub fn add_image(&mut self, index: usize, content_type: &str, bytes: &[u8]) {
        let name = format!("{:03}.{}", index, extension(content_type));
        let name_bytes = name.as_bytes();
        let crc = crc32(bytes);
        let size = bytes.len() as u32;

        // En-tete local
        self.buf.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        self.buf.extend_from_slice(&20u16.to_le_bytes()); // version needed
        self.buf.extend_from_slice(&0u16.to_le_bytes()); // flags
        self.buf.extend_from_slice(&0u16.to_le_bytes()); // method = stored
        self.buf.extend_from_slice(&0u16.to_le_bytes()); // mod time
        self.buf.extend_from_slice(&0x21u16.to_le_bytes()); // date 1980-01-01
        self.buf.extend_from_slice(&crc.to_le_bytes());
        self.buf.extend_from_slice(&size.to_le_bytes());
        self.buf.extend_from_slice(&size.to_le_bytes());
        self.buf
            .extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
        self.buf.extend_from_slice(&0u16.to_le_bytes()); // extra field len
        self.buf.extend_from_slice(name_bytes);
        self.buf.extend_from_slice(bytes);

        self.entries
            .push((name.to_string(), self.offset, crc, size));
        self.offset += 30 + name_bytes.len() as u32 + size;
    }

    /// Repertoire central + EOCD, puis retour du fichier CBZ complet.
    pub fn finish(mut self) -> Vec<u8> {
        let cd_offset = self.offset;
        let mut cd = Vec::new();
        let owned: Vec<(Vec<u8>, u32, u32, u32)> = self
            .entries
            .iter()
            .map(|(name, off, crc, size)| (name.as_bytes().to_vec(), *off, *crc, *size))
            .collect();
        for (name_bytes, local_offset, crc, size) in &owned {
            cd.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
            cd.extend_from_slice(&20u16.to_le_bytes()); // version made by
            cd.extend_from_slice(&20u16.to_le_bytes()); // version needed
            cd.extend_from_slice(&0u16.to_le_bytes()); // flags
            cd.extend_from_slice(&0u16.to_le_bytes()); // method stored
            cd.extend_from_slice(&0u16.to_le_bytes()); // mod time
            cd.extend_from_slice(&0x21u16.to_le_bytes()); // date
            cd.extend_from_slice(&crc.to_le_bytes());
            cd.extend_from_slice(&size.to_le_bytes());
            cd.extend_from_slice(&size.to_le_bytes());
            cd.extend_from_slice(&(name_bytes.len() as u16).to_le_bytes());
            cd.extend_from_slice(&0u16.to_le_bytes()); // extra len
            cd.extend_from_slice(&0u16.to_le_bytes()); // comment len
            cd.extend_from_slice(&0u16.to_le_bytes()); // disk number
            cd.extend_from_slice(&0u16.to_le_bytes()); // internal attrs
            cd.extend_from_slice(&0u32.to_le_bytes()); // external attrs
            cd.extend_from_slice(&local_offset.to_le_bytes());
            cd.extend_from_slice(name_bytes);
        }
        let cd_size = cd.len() as u32;
        self.buf.extend_from_slice(&cd);
        self.buf.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
        self.buf.extend_from_slice(&0u16.to_le_bytes()); // disk
        self.buf.extend_from_slice(&0u16.to_le_bytes()); // cd start disk
        self.buf
            .extend_from_slice(&(self.entries.len() as u16).to_le_bytes());
        self.buf
            .extend_from_slice(&(self.entries.len() as u16).to_le_bytes());
        self.buf.extend_from_slice(&cd_size.to_le_bytes());
        self.buf.extend_from_slice(&cd_offset.to_le_bytes());
        self.buf.extend_from_slice(&0u16.to_le_bytes()); // comment len
        self.buf
    }
}

fn extension(content_type: &str) -> &'static str {
    if content_type.contains("png") {
        "png"
    } else if content_type.contains("webp") {
        "webp"
    } else if content_type.contains("gif") {
        "gif"
    } else {
        "jpg"
    }
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

    #[test]
    fn cbz_has_local_header_and_central_directory() {
        let mut w = CbzWriter::new();
        w.add_image(1, "image/webp", b"page-one");
        w.add_image(2, "image/jpeg", b"page-two");
        let bytes = w.finish();
        // signatures locales + central + EOCD
        assert_eq!(&bytes[0..4], &[0x50, 0x4b, 0x03, 0x04]);
        assert!(bytes.windows(2).any(|w| w == [0x50, 0x4b]));
        assert_eq!(
            &bytes[bytes.len() - 22..bytes.len() - 18],
            &[0x50, 0x4b, 0x05, 0x06]
        );
        assert!(bytes.len() > 30 + 8 + 30 + 8);
    }
}
