//! Lecture d'un `.cbz` (ZIP) — le pendant de `cbz.rs`, qui ne sait qu'écrire.
//!
//! **Pourquoi l'écrire à la main.** `Cargo.toml` ne contenait aucun crate ZIP, et
//! en ajouter un est une décision d'architecture à trancher avec l'humain
//! (AGENTS.md §9). Or nos propres `.cbz` sont écrits en `STORED` (méthode 0,
//! sans compression : des images JPEG/WebP/PNG sont déjà compressées, la
//! recompression ne fait que perdre du temps). Lire ce qu'on écrit soi-même
//! n'a donc besoin que du répertoire central.
//!
//! **`DEFLATE` (méthode 8), ajouté le 2026-10-05 pour la tâche 5.1.** La mise à
//! jour du moteur télécharge une archive, et une archive de JRE produite par
//! `Compress-Archive` est en `DEFLATE` : sans inflate, 5.1 n'aurait rien à
//! installer. La dépendance est donc **`flate2` seule** (miniz_oxide, Rust pur,
//! sans chaîne C), et non le crate `zip` — le répertoire central est déjà lu et
//! testé ici, donc l'ajouter n'aurait apporté que le inflate, à une constante
//! près. C'est une **décision**, donc ADR 0006.
//!
//! **Ce qui n'est toujours pas supporté, et pourquoi c'est dit.** Les méthodes
//! 12 (bzip2), 14 (LZMA) et 93 (Zstandard) exigeraient d'autres décodeurs. On ne
//! devine pas et on ne renvoie pas de bytes faux : on rend une erreur explicite.
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
    /// Méthode de compression qu'on ne sait pas décompresser (12, 14, 93, …).
    MethodeNonSupporte(u16),
    /// Entrée décompressée plus grande que le plafond : c'est un *zip bomb*, et
    /// on refuse **avant** d'allouer, pas après.
    EntreeTropGrosse {
        annonce: u64,
        plafond: u64,
    },
    /// Le répertoire central annonce plus d'octets que le plafond : c'est lui
    /// qu'on alloue (`vec![0u8; taille_repertoire]`), donc la borne doit le
    /// couvrir aussi. Un `u32` lu dans l'EOCD allows 4 Gio sur une machine
    /// qui en a 12.
    RepertoireTropGros {
        annonce: u64,
        plafond: u64,
    },
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
                "entrée compressée en méthode {m} : seuls STORED (0) et DEFLATE (8) \
                 sont décompressés par ce lecteur"
            ),
            ErreurZip::EntreeTropGrosse { annonce, plafond } => write!(
                f,
                "entrée annonçant {annonce} octets, au-delà du plafond de {plafond} : \
                 refusé avant allocation"
            ),
            ErreurZip::RepertoireTropGros { annonce, plafond } => write!(
                f,
                "répertoire central annonçant {annonce} octets, au-delà du plafond de \
                 {plafond} : refusé avant allocation"
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
/// Normalise un nom d'entrée d'archive en chemin relatif sûr, ou renvoie `None`.
///
/// Deux règles, dans cet ordre :
///
/// 1. **`\\` devient `/`.** La norme ZIP impose des slashs, mais les outils
///    Windows écrivent des antislashs — mesuré le 2026-10-05 sur
///    `Compress-Archive`, qui a produit 180 entrées sur 181 avec un backslash.
///    Refuser ces noms ne protégeait rien et rendait l'archive inutilisable.
/// 2. **Refus** si le chemin est absolu, remontant, ou contient une lettre de
///    lecteur : une archive ne doit jamais décider où l'on écrit
///    (AGENTS.md §6).
///
/// La normalisation précède **volontairement** le refus, donc
/// `..\\..\\evil.dll` devient `../../evil.dll` et est refusé comme remontant —
/// si l'on refusait les backslashs avant de normaliser, ce nom serait refusé
/// pour la **mauvaise** raison, et une future correction du premier filtre
/// rouvrirait la traversée sans que personne ne s'en aperçoive.
pub fn assainir_nom(nom: &str) -> Option<String> {
    let sur: String = nom.replace('\\', "/");
    if sur.starts_with('/') || sur.contains("..") || lettre_de_lecteur(&sur) {
        return None;
    }
    Some(sur)
}

/// Vrai si le chemin contient une lettre de lecteur Windows (`C:`).
///
/// La lettre est cherchée **n'importe où**, pas en deuxième position :
/// `canonicalize` renvoie la forme longue `\\\\?\\C:\\…`, donc exiger `C:` en
/// deuxième position laissait passer tous les chemins réellement ouverts.
fn lettre_de_lecteur(sur: &str) -> bool {
    let c: Vec<char> = sur.chars().collect();
    (0..c.len().saturating_sub(1)).any(|i| {
        c[i].is_ascii_alphabetic()
            && c[i + 1] == ':'
            && (i + 2 >= c.len() || c[i + 2] == '\\' || c[i + 2] == '/')
    })
}
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

    // Le répertoire central est alloué **avant** d'être lu, donc sa taille
    // annoncée doit être bornée elle aussi : c'est un `u32` de l'EOCD, donc
    // 4 Gio sont déclarables sur une machine de 12 Go.
    if taille_repertoire > TAILLE_ENTREE_MAX {
        return Err(ErreurZip::RepertoireTropGros {
            annonce: taille_repertoire,
            plafond: TAILLE_ENTREE_MAX,
        });
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
        // La position suivante est calculée **avant** le refus de nom, parce que le
        // `continue` doit aussi avancer : il ne le faisait pas, donc le même
        // en-tête était relu, refusé, relu… et la boucle ne rendait jamais la main.
        // Mesuré le 2026-10-15 : un seul `.cbz` dont le répertoire central porte un
        // nom remontant suffisait à figer la lecture (et donc l'import).
        let pos_suivant = debut_nom + len_nom + len_extra + len_commentaire;
        // Refus ou normalisation : voir `assainir_nom`, qui porte la règle et
        // sa raison. En bref : la norme ZIP impose des slashs, mais
        // `Compress-Archive` écrit des antislashs — mesuré le 2026-10-05, avec
        // **toutes** les entrées de l'archive du moteur en backslash. Les écarter
        // ne protégeait rien : cela faisait qu'une archive produite par l'outil du
        // projet lui-même n'installait rien, silencieusement.
        let Some(nom_sur) = assainir_nom(&nom) else {
            pos = pos_suivant;
            continue;
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
        pos = pos_suivant;
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

/// Plafond de taille décompressée pour **une** entrée.
///
/// Une archive qui annonce 10 Go pour 1 Ko compressés est un *zip bomb* : sans
/// plafond, l'allocation a lieu avant qu'on puisse s'apercevoir du problème. On
/// vérifie donc la taille **déclarée** avant de lire quoi que ce soit. 512 Mio
/// couvre largement le plus gros fichier d'un runtime `jlink`.
pub const TAILLE_ENTREE_MAX: u64 = 512 * 1024 * 1024;

/// Décompresse une entrée en mémoire. CRC vérifié dans les deux cas.
pub fn lire_entree(f: &mut File, e: &Entree) -> Result<Vec<u8>, ErreurZip> {
    // Le plafond porte sur la taille **déclarée**, donc le refus arrive avant
    // l'allocation. Une entrée dont l'auteur n'a pas renseigné la taille
    // décompressée porte 0 : on retombe alors sur la taille compressée, qui
    // borne aussi — un zip bomb déclare toujours *quelque chose*.
    //
    // Il porte sur **les deux**, parce qu'on alloue la compressée : ne borner
    // que la décompressée laissait passer une entrée « 1 octet réel, 256 Mio
    // compressés », qui allouait 256 Mio avant d'échouer à la lecture
    // (mesuré le 2026-10-08 ; le test
    // `la_taille_compressee_annoncee_est_bornee_avant_allocation` le prouve en
    // distinguant `EntreeTropGrosse` de `Io`).
    if e.taille_reelle > TAILLE_ENTREE_MAX || e.taille_compressee > TAILLE_ENTREE_MAX {
        return Err(ErreurZip::EntreeTropGrosse {
            annonce: e.taille_reelle.max(e.taille_compressee),
            plafond: TAILLE_ENTREE_MAX,
        });
    }
    if e.methode != 0 && e.methode != 8 {
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

    // Pour `STORED`, compressé = décompressé. Pour `DEFLATE`, il faut gonfler —
    // et c'est la taille **décompressée** qui porte le CRC, pas celle qu'on
    // vient de lire. Confondre les deux ferait échouer toute entrée compressée.
    let donnees = if e.methode == 0 {
        donnees
    } else {
        let mut sortie = Vec::with_capacity(e.taille_reelle as usize);
        flate2::read::DeflateDecoder::new(&donnees[..])
            .read_to_end(&mut sortie)
            .map_err(|e| ErreurZip::Io(format!("décompression DEFLATE impossible : {e}")))?;
        if e.taille_reelle != 0 && sortie.len() as u64 != e.taille_reelle {
            return Err(ErreurZip::Io(format!(
                "taille décompressée {} alors que l'archive annonce {}",
                sortie.len(),
                e.taille_reelle
            )));
        }
        sortie
    };

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
    if e.methode == 8 {
        // DEFLATE : les octets de l'image ne sont pas là où on les lit. Refuser
        // rendait les dimensions inconnues pour tout `.cbz` compressé, donc la
        // virtualisation se désactivait (le lecteur rendait toutes les pages).
        // On retombe donc sur la lecture complète, qui décompresse déjà.
        //
        // On ne triche pas avec `take(max)` avant le décodage : un flux DEFLATE
        // tronqué coupe au milieu d'un bloc et le remplissage de sortie n'est
        // alors pas encore dans un état lisible. `lire_entree` borne déjà les
        // deux tailles **avant** d'allouer, donc borner ici n'ajouterait rien.
        return lire_entree(f, e);
    }
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
///
/// Mesuré le 2026-10-09 : une entrée DEFLATE renvoyait `None`, parce que
/// `lire_entree_partielle` refusait toute méthode non nulle. Or
/// `download.rs:478-483` reporte ce `None` en `(0, 0)`, et l'interface exige
/// **toutes** les hauteurs positives pour virtualiser la bande : un seul `.cbz`
/// compressé suffisait à désactiver toute la virtualisation.
pub fn dimensions_entree(f: &mut File, e: &Entree) -> Option<(u32, u32)> {
    let octets = if e.methode == 8 {
        lire_entree(f, e).ok()?
    } else {
        lire_entree_partielle(f, e, EN_TETE_MAX).ok()?
    };
    super::image::dimensions(&octets)
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

    /// Position du répertoire central, lue dans l'EOCD.
    ///
    /// **Pas** un scan de la signature : dans une archive `DEFLATE`, les octets
    /// compressés peuvent contenir `0x0201_4b50` par hasard, et le scan trouve
    /// alors un décalage **dans les données** — mesuré le 2026-10-05, le test
    /// « CRC falsifié » corrompait donc le mauvais endroit et la lecture passait
    /// quand même. L'EOCD est toujours en fin de fichier, donc il est non ambigu.
    fn position_repertoire_central(archive: &[u8]) -> usize {
        let eocd = archive.len() - 22;
        assert_eq!(
            &archive[eocd..eocd + 4],
            &0x0605_4b50u32.to_le_bytes(),
            "le dernier EOCD n est pas la ou il devrait etre",
        );
        u32::from_le_bytes(archive[eocd + 16..eocd + 20].try_into().unwrap()) as usize
    }
    /// Les antislashs sont **normalisés**, et le remontement reste **refusé**.
    ///
    /// Mesuré le 2026-10-05 : `Compress-Archive` a écrit 180 entrées sur 181 avec
    /// un backslash. Écarter ces noms ne protégeait rien et rendait l'archive
    /// inutilisable.
    ///
    /// Le cas `..\\..\\evil.dll` est le plus important : il doit être refusé, mais
    /// **après** normalisation — sinon il le serait pour la mauvaise raison (le
    /// backslash), et une correction future du filtre des backslashs rouvrirait la
    /// traversée sans que personne ne s'en aperçoive.
    #[test]
    fn les_antislashs_sont_normalises_et_le_remontement_refuse() {
        // --- cas nominaux : le contenu réel d'une archive Windows ---
        assert_eq!(
            assainir_nom(r"lib\asm-7.3.1.jar"),
            Some("lib/asm-7.3.1.jar".to_string())
        );
        assert_eq!(
            assainir_nom(r"runtime\bin\java.exe"),
            Some("runtime/bin/java.exe".to_string())
        );
        assert_eq!(
            assainir_nom("tatami-engine.bat"),
            Some("tatami-engine.bat".to_string()),
            "un nom simple ne doit pas bouger"
        );

        // --- refus : chacun pour une raison distincte ---
        for nom in [
            r"..\..\evil.dll",      // remontant, en antislashs
            "../../evil.dll",       // remontant, en slashs
            "lib/../../evil.dll",   // remontant au milieu
            "/etc/passwd",          // absolu
            r"C:\Windows\evil.dll", // lettre de lecteur
            r"\\?\C:\evil.dll",     // forme longue
        ] {
            assert_eq!(assainir_nom(nom), None, "« {nom} » aurait du etre refuse");
        }
    }
    /// Assemble une archive ZIP à la main, en `STORED` ou en `DEFLATE`.
    ///
    /// **Pourquoi à la main.** Les tests existants ne couvrent que `STORED`,
    /// parce que `CbzWriter` n’écrit que ça. Sans archive `DEFLATE`, tout le
    /// chemin ajouté pour 5.1 resterait non vérifié — et une porte qui n’a jamais
    /// rien trouvé n’est pas une porte. On écrit donc les ~100 octets
    /// d’en-têtes ici plutôt que d’élargir `CbzWriter` à la compression, qui
    /// serait une fonctionnalité hors périmètre.
    fn archive_zip(nom: &str, contenu: &[u8], methode: u16) -> Vec<u8> {
        use std::io::Write as _;

        let (charge, taille_comp, taille_reelle) = if methode == 0 {
            (contenu.to_vec(), contenu.len() as u32, contenu.len() as u32)
        } else {
            let mut e =
                flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
            e.write_all(contenu).unwrap();
            let charge = e.finish().unwrap();
            let c = charge.len() as u32;
            (charge, c, contenu.len() as u32)
        };
        let crc = crate::cbz::crc32_public(contenu);
        let octets_nom = nom.as_bytes();

        let mut out: Vec<u8> = Vec::new();
        let offset_local = 0u32;
        // --- en-tête local ---
        out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        out.extend_from_slice(&20u16.to_le_bytes()); // version nécessaire
        out.extend_from_slice(&0u16.to_le_bytes()); // drapeaux
        out.extend_from_slice(&methode.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes()); // heure
        out.extend_from_slice(&0u16.to_le_bytes()); // date
        out.extend_from_slice(&crc.to_le_bytes());
        out.extend_from_slice(&taille_comp.to_le_bytes());
        out.extend_from_slice(&taille_reelle.to_le_bytes());
        out.extend_from_slice(&(octets_nom.len() as u16).to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes()); // champ extra
        out.extend_from_slice(octets_nom);
        out.extend_from_slice(&charge);

        let offset_repertoire = out.len() as u32;
        // --- répertoire central ---
        out.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
        out.extend_from_slice(&20u16.to_le_bytes()); // version émetteur
        out.extend_from_slice(&20u16.to_le_bytes()); // version nécessaire
        out.extend_from_slice(&0u16.to_le_bytes()); // drapeaux
        out.extend_from_slice(&methode.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&crc.to_le_bytes());
        out.extend_from_slice(&taille_comp.to_le_bytes());
        out.extend_from_slice(&taille_reelle.to_le_bytes());
        out.extend_from_slice(&(octets_nom.len() as u16).to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes()); // extra
        out.extend_from_slice(&0u16.to_le_bytes()); // commentaire
        out.extend_from_slice(&0u16.to_le_bytes()); // disque
        out.extend_from_slice(&0u16.to_le_bytes()); // attributs internes
        out.extend_from_slice(&0u32.to_le_bytes()); // attributs externes
        out.extend_from_slice(&offset_local.to_le_bytes());
        out.extend_from_slice(octets_nom);

        let taille_repertoire = out.len() as u32 - offset_repertoire;
        // --- fin de répertoire central ---
        out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&1u16.to_le_bytes()); // entrées sur ce disque
        out.extend_from_slice(&1u16.to_le_bytes()); // entrées au total
        out.extend_from_slice(&taille_repertoire.to_le_bytes());
        out.extend_from_slice(&offset_repertoire.to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes()); // commentaire
        out
    }

    /// Assemble une archive `STORED` à **deux** entrées.
    ///
    /// `archive_zip` ne sait écrire qu'**une** entrée, donc le cas qui porte le
    /// défaut n'était pas représentable : une entrée **refusée** suivie d'une
    /// entrée valide. C'est exactement la forme qui figeait [`lire_entrees`] —
    /// le `continue` du refus n'écrivait pas `pos`, donc le même en-tête était
    /// relu sans fin. Mesuré le 2026-10-15.
    fn archive_deux_entrees(nom_refuse: &str, nom_sain: &str) -> Vec<u8> {
        let mut corps: Vec<u8> = Vec::new();
        let mut central: Vec<u8> = Vec::new();
        // Les deux entrées sont vides : ce qui est testé est la lecture du
        // répertoire central, pas le contenu.
        for nom in [nom_refuse, nom_sain] {
            let n = nom.as_bytes();
            let offset_local = corps.len() as u32;
            // --- en-tête local ---
            corps.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
            corps.extend_from_slice(&20u16.to_le_bytes()); // version
            corps.extend_from_slice(&0u16.to_le_bytes()); // drapeaux
            corps.extend_from_slice(&0u16.to_le_bytes()); // STORED
            corps.extend_from_slice(&0u16.to_le_bytes()); // heure
            corps.extend_from_slice(&0u16.to_le_bytes()); // date
            corps.extend_from_slice(&0u32.to_le_bytes()); // crc
            corps.extend_from_slice(&0u32.to_le_bytes()); // taille compressée
            corps.extend_from_slice(&0u32.to_le_bytes()); // taille réelle
            corps.extend_from_slice(&(n.len() as u16).to_le_bytes());
            corps.extend_from_slice(&0u16.to_le_bytes()); // extra
            corps.extend_from_slice(n);
            // --- entrée du répertoire central ---
            central.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
            central.extend_from_slice(&20u16.to_le_bytes()); // version émetteur
            central.extend_from_slice(&20u16.to_le_bytes()); // version nécessaire
            central.extend_from_slice(&0u16.to_le_bytes()); // drapeaux
            central.extend_from_slice(&0u16.to_le_bytes()); // STORED
            central.extend_from_slice(&0u16.to_le_bytes()); // heure
            central.extend_from_slice(&0u16.to_le_bytes()); // date
            central.extend_from_slice(&0u32.to_le_bytes()); // crc
            central.extend_from_slice(&0u32.to_le_bytes()); // taille compressée
            central.extend_from_slice(&0u32.to_le_bytes()); // taille réelle
            central.extend_from_slice(&(n.len() as u16).to_le_bytes());
            central.extend_from_slice(&0u16.to_le_bytes()); // extra
            central.extend_from_slice(&0u16.to_le_bytes()); // commentaire
            central.extend_from_slice(&0u16.to_le_bytes()); // disque
            central.extend_from_slice(&0u16.to_le_bytes()); // attributs internes
            central.extend_from_slice(&0u32.to_le_bytes()); // attributs externes
            central.extend_from_slice(&offset_local.to_le_bytes());
            central.extend_from_slice(n);
        }
        let taille_repertoire = central.len() as u32;
        let offset_repertoire = corps.len() as u32;
        corps.extend_from_slice(&central);
        // --- fin de répertoire central ---
        corps.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
        corps.extend_from_slice(&0u16.to_le_bytes());
        corps.extend_from_slice(&0u16.to_le_bytes());
        corps.extend_from_slice(&2u16.to_le_bytes()); // entrées sur ce disque
        corps.extend_from_slice(&2u16.to_le_bytes()); // entrées au total
        corps.extend_from_slice(&taille_repertoire.to_le_bytes());
        corps.extend_from_slice(&offset_repertoire.to_le_bytes());
        corps.extend_from_slice(&0u16.to_le_bytes()); // commentaire
        corps
    }

    /// Une entrée refusée ne doit pas empêcher de lire la suivante.
    ///
    /// **Le délai n'est pas décoratif.** Avant la correction, cette assertion
    /// n'était pas fausse : elle était **infinie**, donc le job de tests entier
    /// expirait sans nommer la cause. Le fil porteur porte donc un garde-fou,
    /// pour qu'un retour de la boucle infinie soit un test rouge qui dit *quoi*.
    #[test]
    fn une_entree_refusee_ne_bloque_pas_la_suivante() {
        use std::sync::mpsc;

        let p = dossier_unique("refus").join("hostile.cbz");
        std::fs::write(&p, archive_deux_entrees("../evil.jpg", "001.jpg")).unwrap();

        let (tx, rx) = mpsc::channel();
        let chemin = p.clone();
        std::thread::spawn(move || {
            let lu = lire_entrees(&chemin)
                .map(|entrees| entrees.iter().map(|e| e.nom.clone()).collect::<Vec<_>>())
                .map_err(|e| e.to_string());
            let _ = tx.send(lu);
        });

        let lu = rx.recv_timeout(std::time::Duration::from_secs(10)).expect(
            "lire_entrees ne rend pas la main quand une entrée est refusée : \
             la position du répertoire central n'avance pas",
        );

        assert_eq!(
            lu.expect("l'archive doit rester lisible"),
            vec!["001.jpg".to_string()]
        );
    }

    /// Le chemin `DEFLATE` de [`lire_entree`] doit rendre les octets **d'origine**.
    ///
    /// Deux choses cassent facilement et silencieusement : le CRC porte sur les
    /// octets **décompressés** (pas ceux lus), et la taille décompressée doit
    /// correspondre à celle annoncée. Une archive réellement compressée — donc
    /// `taille_compressee < taille_reelle` — est ce qui rend la distinction
    /// visible : sans elle, un inflate qui neGonfle rien passerait quand même.
    #[test]
    fn une_archive_deflate_est_decompressee() {
        use std::io::{Seek, Write};

        // Contenu chosen pour compresser : du texte repetitif gonfle bien, donc
        // l archive est reellement plus petite que son contenu.
        let contenu: Vec<u8> = b"tatami moteur runtime "
            .iter()
            .cycle()
            .take(400)
            .copied()
            .collect();
        let archive = archive_zip("runtime/bin/java.exe", &contenu, 8);

        let chemin =
            std::env::temp_dir().join(format!("tatami-deflate-{}.zip", std::process::id()));
        let mut f = std::fs::File::create(&chemin).unwrap();
        f.write_all(&archive).unwrap();
        drop(f);

        let mut f = std::fs::File::open(&chemin).unwrap();
        let entrees = lire_entrees(&chemin).unwrap();
        assert_eq!(1, entrees.len());
        assert_eq!(8, entrees[0].methode, "la methode doit etre DEFLATE");
        assert!(
            entrees[0].taille_compressee < entrees[0].taille_reelle,
            "l archive de test ne compresse rien : elle ne prouverait pas l inflate",
        );

        f.rewind().unwrap();
        let lu = lire_entree(&mut f, &entrees[0]).unwrap();
        assert_eq!(contenu.len(), lu.len(), "taille restituee inattendue");
        assert_eq!(
            contenu, lu,
            "les octets decompresses doivent etre ceux d origine"
        );
        let _ = std::fs::remove_file(&chemin);
    }

    /// Une entrée `STORED` doit continuer de fonctionner : le chemin ajouté ne
    /// doit pas avoir cassé le cas déjà couvert.
    #[test]
    fn une_archive_stored_reste_intacte() {
        use std::io::Write;

        let contenu = b"octets non compresses".to_vec();
        let archive = archive_zip("lib/tatami-engine.jar", &contenu, 0);
        let chemin = std::env::temp_dir().join(format!("tatami-stored-{}.zip", std::process::id()));
        let mut f = std::fs::File::create(&chemin).unwrap();
        f.write_all(&archive).unwrap();
        drop(f);

        let mut f = std::fs::File::open(&chemin).unwrap();
        let entrees = lire_entrees(&chemin).unwrap();
        assert_eq!(0, entrees[0].methode);
        assert_eq!(contenu, lire_entree(&mut f, &entrees[0]).unwrap());
        let _ = std::fs::remove_file(&chemin);
    }

    /// Un octet corrompu dans une entrée `DEFLATE` doit être **refusé**.
    ///
    /// Ce test existe parce qu'une porte a failli passer : vérifier le CRC sur
    /// les octets *compressés* au lieu des octets *décompressés* rendait la
    /// vérification simplement **absente** pour une entrée DEFLATE — et le test
    /// « je relis les bons octets » passait quand même, parce qu'il ne regardait
    /// que le contenu, jamais le contrôle. Corollaire : un test qui vérifie une
    /// donnée ne vérifie pas forcément le contrôle qui la protège.
    #[test]
    fn une_archive_deflate_corrompue_est_refusee() {
        use std::io::{Seek, Write};

        let contenu: Vec<u8> = b"tatami moteur runtime "
            .iter()
            .cycle()
            .take(400)
            .copied()
            .collect();
        let mut archive = archive_zip("runtime/bin/java.exe", &contenu, 8);

        // On corrompt **un seul octet** au milieu de la charge compressée : entre
        // l'en-tête local (30 + nom) et le répertoire central.
        let debut_charge = 30 + "runtime/bin/java.exe".len();
        let fin_charge = archive.len() - 22 - 46 - "runtime/bin/java.exe".len();
        assert!(fin_charge > debut_charge, "la charge compressee est vide");
        let cible = debut_charge + (fin_charge - debut_charge) / 2;
        archive[cible] ^= 0xff;

        let chemin =
            std::env::temp_dir().join(format!("tatami-corrompu-{}.zip", std::process::id()));
        let mut f = std::fs::File::create(&chemin).unwrap();
        f.write_all(&archive).unwrap();
        drop(f);

        let mut f = std::fs::File::open(&chemin).unwrap();
        let entrees = lire_entrees(&chemin).unwrap();
        f.rewind().unwrap();
        match lire_entree(&mut f, &entrees[0]) {
            Ok(obtenu) => panic!(
                "une archive corrompue a ete acceptee ({} octets rendus)",
                obtenu.len()
            ),
            Err(ErreurZip::CrcInattendu { .. }) => {} // cas attendu : CRC
            Err(e @ ErreurZip::Io(_)) => {
                // Également acceptable : un octet renversé peut casser le flux
                // deflate, et l'inflate échoue alors avant même que le CRC ne
                // soit atteint. Les deux sont des refus, et c'est tout ce qui
                // compte ici.
                assert!(
                    e.to_string().contains("DEFLATE"),
                    "un refus doit nommer la cause : {e}"
                );
            }
            Err(autre) => panic!("refus inattendu : {autre:?}"),
        }
        let _ = std::fs::remove_file(&chemin);
    }
    /// Le CRC **attendu** est bien comparé aux octets **décompressés**.
    ///
    /// Ce test est le deuxième essai, et il existe parce que le premier ne
    /// prouveait rien. Corrompre un octet de la charge compressée fait
    /// généralement échouer l'inflate **avant** le CRC — donc le test passait
    /// même avec la vérification du CRC supprimée pour DEFLATE. On corrompt donc
    /// la valeur **attendue**, pas les données : le inflate réussit, les octets
    /// sont les bons, et seul un CRC réellement comparé peut refuser.
    ///
    /// Corollaire : « la lecture échoue quand j'abîme les données » ne prouve pas
    /// que le contrôle est branché — il prouve seulement qu'*un* contrôle
    /// l'est, et n'identifie pas lequel.
    #[test]
    fn le_crc_attendu_est_verifie_pour_deflate() {
        use std::io::{Seek, Write};

        let contenu: Vec<u8> = b"tatami moteur runtime "
            .iter()
            .cycle()
            .take(400)
            .copied()
            .collect();
        let mut archive = archive_zip("runtime/bin/java.exe", &contenu, 8);

        // Le répertoire central porte le CRC aux octets 16..20. On le remplace par
        // un autre CRC valide : le inflate va réussir et rendre les bons octets,
        // donc seul un CRC réellement comparé peut refuser.
        let pos = position_repertoire_central(&archive);
        let vrai_crc = u32::from_le_bytes([
            archive[pos + 16],
            archive[pos + 17],
            archive[pos + 18],
            archive[pos + 19],
        ]);
        let faux_crc = vrai_crc ^ 0xffff_ffff;
        archive[pos + 16..pos + 20].copy_from_slice(&faux_crc.to_le_bytes());

        let chemin = std::env::temp_dir().join(format!("tatami-crc-{}.zip", std::process::id()));
        let mut f = std::fs::File::create(&chemin).unwrap();
        f.write_all(&archive).unwrap();
        drop(f);

        let mut f = std::fs::File::open(&chemin).unwrap();
        let entrees = lire_entrees(&chemin).unwrap();
        f.rewind().unwrap();
        match lire_entree(&mut f, &entrees[0]) {
            Ok(_) => panic!(
                "le CRC attendu etait {faux_crc:08x} au lieu de {vrai_crc:08x} : \
                 la lecture aurait du refuser",
            ),
            Err(ErreurZip::CrcInattendu { attendu, obtenu }) => {
                assert_eq!(
                    attendu, faux_crc,
                    "le CRC annonce doit etre celui du repertoire"
                );
                assert_eq!(
                    obtenu, vrai_crc,
                    "le CRC calcule doit etre celui des octets rendus"
                );
                assert_ne!(attendu, obtenu);
            }
            Err(autre) => panic!("refus inattendu : {autre:?}"),
        }
        let _ = std::fs::remove_file(&chemin);
    }
    /// Le refus d'une taille décompressée absurde doit arriver **avant**
    /// l'allocation, sinon un *zip bomb* tue la machine avant qu'on puisse
    /// répondre « non ».
    /// Le plafond doit couvrir **les deux** tailles annoncées, pas seulement la
    /// taille décompressée.
    ///
    /// Défaut mesuré le 2026-10-08 : le plafond portait sur
    /// `taille_reelle` quand elle était renseignée, et l'allocation portait sur
    /// `taille_compressee` (`vec![0u8; e.taille_compressee]`). Une entrée
    /// déclarant « 1 octet décompressé, 1 Gio compressés » passait donc le
    /// contrôle et allouait 1 Gio avant même d'essayer de lire — le refus
    /// arrivait ensuite, en erreur de lecture, donc **après** l'allocation.
    ///
    /// Le test ne vérifie pas qu'une erreur sort (les deux codes en sortent une,
    /// donc le test passerait avec le défaut) : il vérifie **laquelle**, parce
    /// que `EntreeTropGrosse` signifie « refusé avant allocation » et `Io`
    /// signifie « on a déjà alloué et essayé de lire ». C'est la seule
    /// différence observable entre les deux comportements.
    #[test]
    fn la_taille_compressee_annoncee_est_bornee_avant_allocation() {
        use std::io::Write;

        let mut archive = archive_zip("petit.bin", b"x", 0);
        // 1 Gio compressés pour 1 octet réel : au-delà du plafond de 512 Mio,
        // donc le refus doit tomber avant le `vec![0u8; …]`.
        let menteur = 1024 * 1024 * 1024u32;
        archive[18..22].copy_from_slice(&menteur.to_le_bytes()); // local : compressée
        let pos_repertoire = position_repertoire_central(&archive);
        archive[pos_repertoire + 20..pos_repertoire + 24].copy_from_slice(&menteur.to_le_bytes());

        let chemin = dossier_unique("bomb-compresse").join("bomb.zip");
        std::fs::File::create(&chemin)
            .unwrap()
            .write_all(&archive)
            .unwrap();

        let mut f = std::fs::File::open(&chemin).unwrap();
        let entrees = lire_entrees(&chemin).unwrap();
        let e = lire_entree(&mut f, &entrees[0]).unwrap_err();
        assert!(
            matches!(e, ErreurZip::EntreeTropGrosse { .. }),
            "le plafond doit refuser la taille **compressée**, donc avant \
             l'allocation ; obtenu : {e:?}"
        );
        let _ = std::fs::remove_file(&chemin);
    }

    /// Même règle pour le répertoire central : c'est la deuxième allocation non
    /// bornée du fichier, et elle est lue d'un simple `u32` de l'EOCD.
    #[test]
    fn le_repertoire_central_annonce_est_borne_avant_allocation() {
        use std::io::Write;

        let archive = archive_zip("petit.bin", b"x", 0);
        // L'EOCD porte la taille du répertoire 12 octets après sa signature.
        let pos_eocd = archive
            .windows(4)
            .rposition(|w| w == [0x50, 0x4b, 0x05, 0x06])
            .expect("EOCD");
        let geant = 2 * 1024 * 1024 * 1024u32; // 2 Gio, dans un u32
        let mut archive = archive;
        archive[pos_eocd + 12..pos_eocd + 16].copy_from_slice(&geant.to_le_bytes());

        let chemin = dossier_unique("bomb-repertoire").join("bomb.zip");
        std::fs::File::create(&chemin)
            .unwrap()
            .write_all(&archive)
            .unwrap();

        let e = lire_entrees(&chemin).unwrap_err();
        assert!(
            matches!(e, ErreurZip::RepertoireTropGros { .. }),
            "un répertoire central de 2 Gio doit être refusé avant allocation ; \
             obtenu : {e:?}"
        );
        let _ = std::fs::remove_file(&chemin);
    }

    #[test]
    fn une_entree_geantesque_est_refusee_avant_allocation() {
        use std::io::{Seek, Write};

        let contenu = b"x";
        let archive = archive_zip("grand.bin", contenu, 0);
        // On réécrit la taille décompressée annoncée dans l'en-tête local **et**
        // dans le répertoire central, sinon le CRC bloquerait avant notre refus.
        let mut archive = archive;
        let geant = (TAILLE_ENTREE_MAX + 1) as u32;
        archive[22..26].copy_from_slice(&geant.to_le_bytes()); // local : taille reelle
                                                               // Le repertoire central est localise par son **signature**, pas par un
                                                               // offset calcule : la longueur du nom change des que le nom change, et
                                                               // un offset faux donne « failed to fill whole buffer », c est-a-dire une
                                                               // panne de lecture plutot qu un echec de test.
        let pos_repertoire = position_repertoire_central(&archive);
        archive[pos_repertoire + 24..pos_repertoire + 28].copy_from_slice(&geant.to_le_bytes());

        let chemin = std::env::temp_dir().join(format!("tatami-bomb-{}.zip", std::process::id()));
        let mut f = std::fs::File::create(&chemin).unwrap();
        f.write_all(&archive).unwrap();
        drop(f);

        let mut f = std::fs::File::open(&chemin).unwrap();
        let entrees = lire_entrees(&chemin).unwrap();
        f.rewind().unwrap();
        let e = lire_entree(&mut f, &entrees[0]).unwrap_err();
        assert!(
            matches!(e, ErreurZip::EntreeTropGrosse { .. }),
            "un refus du plafond est attendu, obtenu : {e:?}",
        );
        let _ = std::fs::remove_file(&chemin);
    }
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

    /// Le message d'erreur doit dire **ce qui marche**, pour qu'on ne passe pas
    /// une heure à chercher pourquoi un `.cbz` tiers ne s'ouvre pas.
    ///
    /// Révisé le 2026-10-05 : `DEFLATE` (8) est désormais décompressé, donc le
    /// message parle des méthodes qui **restent** hors de portée (bzip2, LZMA,
    /// zstd) au lieu de parler de la 8 comme si elle était refusée. Un message
    /// d'erreur qui décrit un état dépassé est un piège : il drove le lecteur
    /// vers une cause qui n'existe plus.
    #[test]
    fn le_message_d_explication_est_actionnable() {
        for methode in [12u16, 14, 93] {
            let m = ErreurZip::MethodeNonSupporte(methode).to_string();
            assert!(
                m.contains(&format!("méthode {methode}")),
                "le message nomme la méthode : {m}"
            );
            assert!(
                m.contains("STORED") && m.contains("DEFLATE"),
                "il dit ce qui marche : {m}"
            );
        }

        // Et la méthode 8, elle, ne doit **plus** être refusée : c'est le
        // régression que ce message masquerait si on ne le vérifiait pas.
        let octets = b"tatami moteur runtime "
            .iter()
            .cycle()
            .take(400)
            .copied()
            .collect::<Vec<u8>>();
        let archive = archive_zip("runtime/bin/java.exe", &octets, 8);
        let chemin =
            std::env::temp_dir().join(format!("tatami-deflate-msg-{}.zip", std::process::id()));
        std::fs::write(&chemin, &archive).unwrap();
        let mut f = std::fs::File::open(&chemin).unwrap();
        let entrees = lire_entrees(&chemin).unwrap();
        let lu = lire_entree(&mut f, &entrees[0]);
        assert!(
            lu.is_ok(),
            "la méthode 8 doit se lire, pas être refusée : {:?}",
            lu.err()
        );
        let _ = std::fs::remove_file(&chemin);
    }

    /// Le défaut moyen mesuré le 2026-10-09 : une entrée **compressée** n'a
    /// jamais de dimensions, donc un `.cbz` DEFLATE perdait la virtualisation.
    ///
    /// `lire_entree_partielle` (`:412-415`) refuse `methode != 0` — logique
    /// pour lire des octets *bruts*, fausse pour lire un en-tête : en DEFLATE,
    /// les octets de l'image ne sont pas là où on les lit. `dimensions_entree`
    /// s'appuyait dessus et renvoyait donc `None`, `download.rs:478-483` reportait
    /// `(0, 0)`, et `bandeVirtualisable` (exige **toutes** les hauteurs > 0)
    /// passait à `false` : le lecteur rendait toutes les pages d'un coup.
    ///
    /// Ce n'est pas rare : `Compress-Archive`, l'outil de maintenance du projet,
    /// produit du DEFLATE, et `lire_entree` décompresse déjà correctement pour
    /// *lire* la page (`:379-385`). Seule la lecture d'en-tête était bloquée.
    #[test]
    fn les_dimensions_dune_entree_deflate_sont_lues() {
        // JPEG minimal 800x2171, le format d'une page de webtoon.
        let jpeg: Vec<u8> = {
            let mut v = vec![0xFF, 0xD8, 0xFF];
            v.extend_from_slice(&[0xE0, 0x00, 0x04, 0x00, 0x00]);
            v.extend_from_slice(&[0xFF, 0xC0, 0x00, 0x11, 0x08]);
            v.extend_from_slice(&2171u16.to_be_bytes());
            v.extend_from_slice(&800u16.to_be_bytes());
            v.extend_from_slice(&[0x03, 0x01, 0x11, 0x00, 0x02, 0x11, 0x01, 0x03, 0x11, 0x01]);
            v
        };
        let archive = archive_zip("001.jpg", &jpeg, 8);
        let chemin = dossier_unique("deflate-dims").join("p.zip");
        std::fs::write(&chemin, &archive).unwrap();
        let mut f = File::open(&chemin).unwrap();
        let entrees = lire_entrees(&chemin).unwrap();
        assert_eq!(entrees.len(), 1);
        assert_eq!(
            dimensions_entree(&mut f, &entrees[0]),
            Some((800, 2171)),
            "une entrée DEFLATE doit rendre ses dimensions : sans elles la \
             virtualisation se désactive et le lecteur rend tout d'un coup"
        );
        let _ = std::fs::remove_file(&chemin);
    }

    /// Le cas nominal à côté du DEFLATE : une entrée STORED doit continuer de
    /// donner ses dimensions. Un test « un X est refusé » ne prouve jamais que
    /// le cas légitime fonctionne encore (leçon AGENTS.md §14).
    #[test]
    fn les_dimensions_dune_entree_stored_sont_lues() {
        let jpeg: Vec<u8> = {
            let mut v = vec![0xFF, 0xD8, 0xFF];
            v.extend_from_slice(&[0xE0, 0x00, 0x04, 0x00, 0x00]);
            v.extend_from_slice(&[0xFF, 0xC0, 0x00, 0x11, 0x08]);
            v.extend_from_slice(&2171u16.to_be_bytes());
            v.extend_from_slice(&800u16.to_be_bytes());
            v.extend_from_slice(&[0x03, 0x01, 0x11, 0x00, 0x02, 0x11, 0x01, 0x03, 0x11, 0x01]);
            v
        };
        let archive = archive_zip("001.jpg", &jpeg, 0);
        let chemin = dossier_unique("stored-dims").join("p.zip");
        std::fs::write(&chemin, &archive).unwrap();
        let mut f = File::open(&chemin).unwrap();
        let entrees = lire_entrees(&chemin).unwrap();
        assert_eq!(dimensions_entree(&mut f, &entrees[0]), Some((800, 2171)));
        let _ = std::fs::remove_file(&chemin);
    }
}
