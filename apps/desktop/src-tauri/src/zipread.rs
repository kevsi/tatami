//! Lecture d'un `.cbz` (ZIP) — le pendant de `cbz.rs`, qui ne sait qu'écrire.
//!
//! **Pourquoi l'écrire à la main.** `Cargo.toml` ne contient aucun crate ZIP, et
//! en ajouter un est une décision d'architecture à trancher avec l'humain
//! (AGENTS.md §9). Or nos propres `.cbz` sont écrits en `STORED` (méthode 0,
//! sans compression : des images JPEG/WebP/PNG sont déjà compressées, la
//! recompression ne fait que perdre du temps). Lire ce qu'on écrit soi-même
//! n'a donc besoin que du répertoire central.
//!
//! **Ce qui n'est pas supporté, et pourquoi c'est dit.** Une entrée compressée
//! en `DEFLATE` (méthode 8) exigerait un inflate. On ne devine pas et on ne
//! renvoie pas de bytes faux : on rend une erreur explicite. C'est le cas de
//! beaucoup de `.cbz` tiers ; les importer est la tâche 2.4, qui devra décider
//! de la dépendance (ADR).
//!
//! **Intégrité.** Le CRC32 de chaque entrée est vérifié à la lecture. Un fichier
//! tronqué ou modifié est donc refusé, pas affiché en bordel — exactement le
//! défaut qui a produit 37 `.cbz` illisibles (cf. journal du 2026-10-04).

use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

const SIG_EOCD: u32 = 0x0605_4b50;
const SIG_CENTRAL: u32 = 0x0201_4b50;
const SIG_LOCAL: u32 = 0x0403_4b50;
const TAILLE_EOCD: usize = 22;
// Un commentaire de fin de fichier peut faire 65 535 octets.
const TAILLE_MAX_EOCD: usize = TAILLE_EOCD + 0xFFFF;
const TAILLE_CENTRAL_FIXE: usize = 46;
const TAILLE_LOCAL_FIXE: usize = 30;
/// Octets lus pour connaître la taille d'une image sans la charger.
///
/// 512 suffisaient pour un JPEG « minimal », pas pour un JPEG de site. Le
/// marqueur SOF qui décrit le cadre peut être **très loin** de `FFD8` : un
/// JPEG servi par MangaDex porte un profil ICC de 472 octets en APP2, et son
/// SOF0 tombe à l'offset **632**. Au-delà de 512, la lecture s'arrêtait avant
/// lui, `jpeg_dimensions` rendait `None`, et toutes les dimensions tombaient à 0.
///
/// Conséquence mesurée : la bande webtoon recevait une hauteur totale de 0, et
/// le conteneur `height: 0` empilait les 77 pages les unes sur les autres
/// (`position: absolute`, `top: 0`) — lecture cassée, sans erreur console.
/// Un chapitre téléchargé n'était donc lisible que si sa source ne publiait pas
/// de profil ICC : d'où « ça marche sauf chez MangaDEX ».
///
/// 64 Kio couvrent tous les en-têtes réalistes ; au-delà, on abandonne plutôt
/// que de lire une image entière pour rien (cf. `dimensions_entree`).
const EN_TETE_MAX: usize = 64 * 1024;

/// Une entrée d'image du `.cbz`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entree {
    /// Rang de l'entrée dans le répertoire central, dans l'ordre du fichier.
    pub index: usize,
    pub nom: String,
    pub methode: u16,
    pub crc: u32,
    pub taille_compressee: u64,
    pub taille_reelle: u64,
    pub offset_local: u64,
}

impl Entree {
    /// Vrai si cette entrée est une image que le lecteur sait afficher.
    /// Le nom est étendu : c'est l'extension qui décide.
    pub fn est_image(&self) -> bool {
        let bas = self
            .nom
            .rsplit('/')
            .next()
            .unwrap_or(&self.nom)
            .to_ascii_lowercase();
        [".jpg", ".jpeg", ".png", ".webp", ".gif", ".bmp", ".avif"]
            .iter()
            .any(|e| bas.ends_with(e))
    }
}

/// Erreurs explicites : le message doit dire ce qui manque, jamais « échec ».
#[derive(Debug)]
pub enum ErreurZip {
    Io(String),
    /// Le marqueur de fin de répertoire central est absent : ce n'est pas un ZIP.
    PasUnZip,
    /// ZIP64 : hors de portée de ce lecteur, et il le dit.
    Zip64NonSupporte,
    /// Entrée compressée : il faut un inflate (tâche 2.4, décision de dépendance).
    MethodeNonSupporte(u16),
    /// Le CRC ne correspond pas : fichier corrompu ou modifié.
    CrcInattendu {
        attendu: u32,
        obtenu: u32,
    },
}

impl std::fmt::Display for ErreurZip {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ErreurZip::Io(m) => write!(f, "lecture impossible : {m}"),
            ErreurZip::PasUnZip => write!(
                f,
                "ce fichier n'est pas un ZIP valide (aucun répertoire central)"
            ),
            ErreurZip::Zip64NonSupporte => write!(
                f,
                "ce ZIP utilise ZIP64 (plus de 4 Go ou plus de 65 535 entrées) : non supporté"
            ),
            ErreurZip::MethodeNonSupporte(m) => write!(
                f,
                "entrée compressée en méthode {m} : ce lecteur ne décompresse pas encore. \
                 Les chapitres téléchargés par Tatami ne sont pas compressés, donc un \
                 .cbz exporté par l'application fonctionne ; un .cbz tiers compressé, non."
            ),
            ErreurZip::CrcInattendu { attendu, obtenu } => write!(
                f,
                "entrée corrompue : CRC attendu {attendu:#010x}, obtenu {obtenu:#010x}"
            ),
        }
    }
}

fn io<E: std::fmt::Display>(e: E) -> ErreurZip {
    ErreurZip::Io(e.to_string())
}

/// Un nom d'entrée ne doit jamais décider d'un chemin hors du `.cbz`.
fn nom_remonte(nom: &str) -> bool {
    nom.contains("..") || nom.starts_with('/') || nom.contains('\\') || nom.contains(':')
}

/// Lit le répertoire central et renvoie les entrées, **dans l'ordre du fichier**.
///
/// L'ordre du répertoire central est l'ordre d'écriture de notre `cbz.rs`, donc
/// l'ordre des pages. Pour un `.cbz` tiers, c'est aussi l'ordre usuel — mais on
/// ne s'appuie pas sur le nom pour le deviner, on trie par le nom du fichier
/// quand il est numéroté, ce qui est plus sûr pour les archives externes.
pub fn lire_entrees(chemin: &Path) -> Result<Vec<Entree>, ErreurZip> {
    let mut f = File::open(chemin).map_err(io)?;
    let taille = f.metadata().map_err(io)?.len();
    if taille < (TAILLE_EOCD as u64) {
        return Err(ErreurZip::PasUnZip);
    }
    let debut_recherche = taille.saturating_sub(TAILLE_MAX_EOCD as u64);
    let mut fente = vec![0u8; (taille - debut_recherche) as usize];
    f.seek(SeekFrom::Start(debut_recherche)).map_err(io)?;
    f.read_exact(&mut fente).map_err(io)?;

    // Le répertoire central est référencé par un offset absolu : on cherche donc le
    // marqueur EOCD en partant de la fin.
    let pos_eocd = (0..=fente.len() - TAILLE_EOCD)
        .rev()
        .find(|&i| lire_u32(&fente, i) == SIG_EOCD)
        .ok_or(ErreurZip::PasUnZip)?;

    let n = lire_u16(&fente, pos_eocd + 10) as u64;
    let taille_repertoire = lire_u32(&fente, pos_eocd + 12) as u64;
    let offset_repertoire = lire_u32(&fente, pos_eocd + 16) as u64;
    if n == 0xFFFF || offset_repertoire == 0xFFFF_FFFF || taille_repertoire == 0xFFFF_FFFF {
        return Err(ErreurZip::Zip64NonSupporte);
    }

    let mut repertoire = vec![0u8; taille_repertoire as usize];
    f.seek(SeekFrom::Start(offset_repertoire)).map_err(io)?;
    f.read_exact(&mut repertoire).map_err(io)?;

    let mut entrees = Vec::with_capacity(n as usize);
    let mut pos = 0usize;
    while pos + TAILLE_CENTRAL_FIXE <= repertoire.len() {
        if lire_u32(&repertoire, pos) != SIG_CENTRAL {
            break;
        }
        let methode = lire_u16(&repertoire, pos + 10);
        let crc = lire_u32(&repertoire, pos + 16);
        let taille_compressee = lire_u32(&repertoire, pos + 20) as u64;
        let taille_reelle = lire_u32(&repertoire, pos + 24) as u64;
        let len_nom = lire_u16(&repertoire, pos + 28) as usize;
        let len_extra = lire_u16(&repertoire, pos + 30) as usize;
        let len_commentaire = lire_u16(&repertoire, pos + 32) as usize;
        let debut_nom = pos + TAILLE_CENTRAL_FIXE;
        if debut_nom + len_nom > repertoire.len() {
            break;
        }
        let nom = String::from_utf8_lossy(&repertoire[debut_nom..debut_nom + len_nom]).to_string();
        // Un nom avec « .. » ou un chemin absolu est refusé : on n'écrit rien,
        // mais on ne doit pas non plus laisser une archive malveillante
        // décider d'un chemin (AGENTS.md §6).
        let nom_sur = if nom.contains("..") || nom.starts_with('/') || nom.contains('\\') {
            continue;
        } else {
            nom.clone()
        };
        let offset_local = lire_u32(&repertoire, pos + 42) as u64;
        if offset_local == 0xFFFF_FFFF
            || taille_compressee == 0xFFFF_FFFF
            || taille_reelle == 0xFFFF_FFFF
        {
            return Err(ErreurZip::Zip64NonSupporte);
        }
        entrees.push(Entree {
            index: entrees.len(),
            nom: nom_sur,
            methode,
            crc,
            taille_compressee,
            taille_reelle,
            offset_local,
        });
        pos = debut_nom + len_nom + len_extra + len_commentaire;
    }
    if entrees.is_empty() {
        return Err(ErreurZip::PasUnZip);
    }
    Ok(entrees)
}

/// Les entrées qui sont des images, dans l'ordre de lecture.
///
/// **Double garde sur le nom.** `lire_entrees` écarte déjà les entrées dont le
/// nom remonte (`..`) ou est absolu. Le filtre est **répété ici** à dessein :
/// cette fonction est publique, donc appelable avec des entrées venues
/// d'ailleurs, et une seule barrière est une barrière fragile (AGENTS.md §6).
///
/// On trie par nom **seulement** si tous les noms portent un préfixe numérique :
/// c'est le cas de `001.jpg` (notre écriture) comme de la plupart des archives
/// externes. Un nom `page_10.webp` doit venir après `page_9.webp`, alors qu'un
/// tri alphabétique l'inverserait — un bug classique, et invisible sur une
/// série de 9 pages.
pub fn entrees_images(entrees: &[Entree]) -> Vec<Entree> {
    let mut images: Vec<Entree> = entrees
        .iter()
        .filter(|e| e.est_image() && !nom_remonte(&e.nom))
        .cloned()
        .collect();
    let tous_numerotes = images.iter().all(|e| {
        e.nom
            .rsplit('/')
            .next()
            .unwrap_or(&e.nom)
            .split('.')
            .next()
            .map(|s| !s.is_empty() && s.chars().all(|c| c.is_ascii_digit()))
            .unwrap_or(false)
    });
    if tous_numerotes {
        images.sort_by_key(|e| {
            let base = e.nom.rsplit('/').next().unwrap_or(&e.nom);
            let numero = base.split('.').next().unwrap_or("0");
            // Un nom de 5 chiffres tient dans un u32 ; au-delà, on ne triche pas.
            numero.parse::<u32>().unwrap_or(u32::MAX)
        });
    }
    images
}

/// Décompresse une entrée en mémoire. `STORED` uniquement, CRC vérifié.
pub fn lire_entree(f: &mut File, e: &Entree) -> Result<Vec<u8>, ErreurZip> {
    if e.methode != 0 {
        return Err(ErreurZip::MethodeNonSupporte(e.methode));
    }
    f.seek(SeekFrom::Start(e.offset_local)).map_err(io)?;
    let mut entete = [0u8; TAILLE_LOCAL_FIXE];
    f.read_exact(&mut entete).map_err(io)?;
    if lire_u32(&entete, 0) != SIG_LOCAL {
        return Err(ErreurZip::PasUnZip);
    }
    let len_nom = lire_u16(&entete, 26) as u64;
    let len_extra = lire_u16(&entete, 28) as u64;
    let debut = e.offset_local + TAILLE_LOCAL_FIXE as u64 + len_nom + len_extra;
    f.seek(SeekFrom::Start(debut)).map_err(io)?;

    let mut donnees = vec![0u8; e.taille_compressee as usize];
    f.read_exact(&mut donnees).map_err(io)?;
    let obtenu = super::cbz::crc32_public(&donnees);
    if obtenu != e.crc {
        return Err(ErreurZip::CrcInattendu {
            attendu: e.crc,
            obtenu,
        });
    }
    Ok(donnees)
}

/// Lit **le début** d'une entrée, sans la décompresser ni la CRC-vérifier.
///
/// Sert à lire l'en-tête d'une image (dimensions) sans payer le prix de l'image
/// entière : mesuré, 303 pages × ~300 ko = ~90 Mo relus pour rien. 512 octets
/// suffisent largement pour un en-tête JPEG (le segment APP0 fait 16 octets, le
/// SOF arrive donc vers l'offset 20-40) comme pour PNG et WebP.
pub fn lire_entree_partielle(f: &mut File, e: &Entree, max: usize) -> Result<Vec<u8>, ErreurZip> {
    if e.methode != 0 {
        return Err(ErreurZip::MethodeNonSupporte(e.methode));
    }
    f.seek(SeekFrom::Start(e.offset_local)).map_err(io)?;
    let mut entete = [0u8; TAILLE_LOCAL_FIXE];
    f.read_exact(&mut entete).map_err(io)?;
    if lire_u32(&entete, 0) != SIG_LOCAL {
        return Err(ErreurZip::PasUnZip);
    }
    let len_nom = lire_u16(&entete, 26) as u64;
    let len_extra = lire_u16(&entete, 28) as u64;
    let debut = e.offset_local + TAILLE_LOCAL_FIXE as u64 + len_nom + len_extra;
    f.seek(SeekFrom::Start(debut)).map_err(io)?;

    let a_lire = (e.taille_compressee as usize).min(max);
    let mut donnees = vec![0u8; a_lire];
    f.read_exact(&mut donnees).map_err(io)?;
    Ok(donnees)
}

/// Dimensions d'une entrée, lues dans son en-tête. `None` si l'image est
/// illisible ou d'un format dont on ne lit pas l'en-tête.
pub fn dimensions_entree(f: &mut File, e: &Entree) -> Option<(u32, u32)> {
    let debut = lire_entree_partielle(f, e, EN_TETE_MAX).ok()?;
    super::image::dimensions(&debut)
}

/// Liste les pages d'un `.cbz` : `(index d'entrée, nom)`.
pub fn pages_de(chemin: &Path) -> Result<Vec<(usize, String)>, ErreurZip> {
    let entrees = lire_entrees(chemin)?;
    Ok(entrees_images(&entrees)
        .into_iter()
        .map(|e| (e.index, e.nom))
        .collect())
}

fn lire_u16(d: &[u8], i: usize) -> u16 {
    u16::from_le_bytes([d[i], d[i + 1]])
}

fn lire_u32(d: &[u8], i: usize) -> u32 {
    u32::from_le_bytes([d[i], d[i + 1], d[i + 2], d[i + 3]])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cbz::CbzWriter;
    use crate::image::ImageKind;

    /// Dossier unique par appel.
    ///
    /// Le pid seul ne suffit pas : deux tests qui fabriquent le même nombre
    /// d'entrées partageaient le dossier **et** le nom de fichier, et
    /// s'écrasaient en parallèle — les tests Rust tournent en parallèle. Le
    /// compteur atomique est exactement la correction notée dans AGENTS.md §14.
    fn dossier_unique(nom: &str) -> std::path::PathBuf {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static COMPTEUR: AtomicUsize = AtomicUsize::new(0);
        let n = COMPTEUR.fetch_add(1, Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("tatami-zipread-{}-{nom}-{n}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn cbz_avec(batons: &[(&str, &[u8])]) -> std::path::PathBuf {
        let dir = dossier_unique("ecriture");
        let p = dir.join("chapitre.cbz");
        {
            let mut w = CbzWriter::create(&p).unwrap();
            for (i, (_nom, octets)) in batons.iter().enumerate() {
                w.add_image_kind(i + 1, ImageKind::Jpeg, octets).unwrap();
            }
            w.finish_vec().unwrap();
        }
        p
    }

    #[test]
    fn lit_les_entrees_ecrites_par_notre_ecrivain() {
        let p = cbz_avec(&[("a", &[1u8; 100]), ("b", &[2u8; 200])]);
        let entrees = lire_entrees(&p).unwrap();
        assert_eq!(entrees.len(), 2);
        assert_eq!(entrees[0].nom, "001.jpg");
        assert_eq!(entrees[1].nom, "002.jpg");
        assert_eq!(entrees[0].methode, 0, "nos ecritures sont en STORED");
    }

    #[test]
    fn relit_les_octets_exacts() {
        let p = cbz_avec(&[("a", &[7u8; 4096])]);
        let entrees = lire_entrees(&p).unwrap();
        let mut f = File::open(&p).unwrap();
        let octets = lire_entree(&mut f, &entrees[0]).unwrap();
        assert_eq!(octets, vec![7u8; 4096]);
    }

    #[test]
    fn refuse_un_fichier_qui_n_est_pas_un_zip() {
        let dir = dossier_unique("pas-un-zip");
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("faux.cbz");
        std::fs::write(&p, b"ceci n'est pas un zip, juste du texte").unwrap();
        assert!(matches!(lire_entrees(&p), Err(ErreurZip::PasUnZip)));
    }

    #[test]
    fn detecte_un_contenu_corrompu_par_son_crc() {
        let p = cbz_avec(&[("a", &[3u8; 512])]);
        let mut octets = std::fs::read(&p).unwrap();
        let n = octets.len();
        // On modifie un octet au milieu des donnees de l'entrée.
        octets[TAILLE_LOCAL_FIXE + 10] ^= 0xFF;
        let corrompu = p.with_extension("corrompu.cbz");
        std::fs::write(&corrompu, &octets).unwrap();
        let entrees = lire_entrees(&corrompu).unwrap();
        let mut f = File::open(&corrompu).unwrap();
        assert!(
            matches!(
                lire_entree(&mut f, &entrees[0]),
                Err(ErreurZip::CrcInattendu { .. })
            ),
            "un fichier modifie doit etre refuse, pas affiche"
        );
        let _ = n;
    }

    #[test]
    fn trie_par_numero_et_pas_alphabetiquement() {
        // Le piege : "10" < "9" en tri alphabétique, donc l'ordre des pages
        // serait inverse a partir de la page 10.
        let entrees: Vec<Entree> = ["001.jpg", "009.jpg", "010.jpg", "011.jpg"]
            .iter()
            .enumerate()
            .map(|(i, n)| Entree {
                index: i,
                nom: (*n).to_string(),
                methode: 0,
                crc: 0,
                taille_compressee: 1,
                taille_reelle: 1,
                offset_local: 0,
            })
            .collect();
        let ordre: Vec<String> = entrees_images(&entrees)
            .into_iter()
            .map(|e| e.nom)
            .collect();
        assert_eq!(ordre, vec!["001.jpg", "009.jpg", "010.jpg", "011.jpg"]);
    }

    #[test]
    fn ne_trie_pas_si_les_noms_ne_sont_pas_numerotes() {
        let entrees: Vec<Entree> = ["cover.jpg", "page-a.jpg", "page-b.jpg"]
            .iter()
            .enumerate()
            .map(|(i, n)| Entree {
                index: i,
                nom: (*n).to_string(),
                methode: 0,
                crc: 0,
                taille_compressee: 1,
                taille_reelle: 1,
                offset_local: 0,
            })
            .collect();
        let ordre: Vec<String> = entrees_images(&entrees)
            .into_iter()
            .map(|e| e.nom)
            .collect();
        assert_eq!(
            ordre,
            vec!["cover.jpg", "page-a.jpg", "page-b.jpg"],
            "l'ordre du fichier doit etre conserve"
        );
    }

    #[test]
    fn ecarte_les_entrees_dont_le_nom_remonte() {
        let entrees: Vec<Entree> = ["001.jpg", "../../evil.jpg", "002.jpg"]
            .iter()
            .enumerate()
            .map(|(i, n)| Entree {
                index: i,
                nom: (*n).to_string(),
                methode: 0,
                crc: 0,
                taille_compressee: 1,
                taille_reelle: 1,
                offset_local: 0,
            })
            .collect();
        let images = entrees_images(&entrees);
        assert!(
            images.iter().all(|e| !e.nom.contains("..")),
            "une entree qui remonte de chemin doit etre ecartee"
        );
        assert_eq!(images.len(), 2);
    }

    #[test]
    fn le_message_d_explication_est_actionnable() {
        let m = ErreurZip::MethodeNonSupporte(8).to_string();
        assert!(m.contains("méthode 8"), "le message nomme la méthode : {m}");
        assert!(
            m.to_lowercase().contains("téléchargés par tatami"),
            "il dit ce qui marche : {m}"
        );
    }
}
