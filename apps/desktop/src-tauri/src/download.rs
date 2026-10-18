//! Téléchargement de chapitres (phase 2) : pages -> fetchImage -> fichiers.
//!
//! Un chapitre = un `.cbz` dans `AppData/Tatami/downloads/<source>/<webtoon>/`,
//! avec un `meta.json` à côté (cf. library.rs) qui porte l'état d'avancement.
//! Nommage assaini (anti path traversal). La progression est renvoyée par
//! événement Tauri (`download://progress`).

use std::path::{Path, PathBuf};
use std::sync::Arc;

use base64::Engine;
use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter, State};

use super::cbz::CbzWriter;
use super::supervisor::Supervisor;

const FETCH_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);

#[derive(Clone, Serialize)]
pub struct Progress {
    pub source: String,
    pub chapter: String,
    pub page: usize,
    pub total: usize,
    pub done: bool,
    pub error: Option<String>,
}

/// Deux téléchargements à la fois, **de sources différentes**.
///
/// Le moteur impose déjà un `Mutex` par source (l'instance de parser et son
/// paginateur ne sont pas partageables) : lancer deux chapitres du même site en
/// parallèle ne ferait que les mettre en file sur le `Mutex` du moteur — l'un
/// attendrait l'autre avec la même vitesse qu'un-après-l'autre, en doublant le
/// suivi. Deux sources différentes, elles, ont chacune leur instance de parser
/// et leur `Mutex` : rien ne s'y marche dessus, et le gain est réel.
///
/// `std::sync::Mutex` et non `tokio` : le drapeau n'est jamais tenu à travers un
/// `await`, et c'est ce qui permet à `VerrouTelechargement` de le relâcher dans
/// `Drop` (y compris sur une panique).
#[derive(Default)]
pub struct DownloadState {
    actifs: Arc<std::sync::Mutex<std::collections::HashMap<String, ()>>>,
}

/// Assainit un nom de fichier Windows.
///
/// Au-delà des caractères interdits et des points de fin (que `Path` refuse),
/// Windows **réserve** certains noms, avec ou sans extension : écrire un fichier
/// nommé `CON` ou `NUL.cbz` échoue, et `create_dir_all` sur un dossier `CON`
/// échoue aussi. Comme le titre du webtoon vient du site, un titre `CON`
/// rendait un téléchargement impossible à mener à son terme.
pub fn sanitize(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    let trimmed = cleaned.trim().trim_matches('.').trim().to_string();
    if trimmed.is_empty() {
        return "sans-titre".to_string();
    }
    let plafonne = plafonner(&trimmed);
    if est_nom_reserve(&plafonne) {
        // On préfixe plutôt que de suffixe : `CON (2)` serait encore réservé,
        // alors que `_CON` ne l'est pas.
        format!("_{plafonne}")
    } else {
        plafonne
    }
}

/// Noms réservés par Windows (CON, PRN, AUX, NUL, COM1-9, LPT1-9), insensibles
/// à la casse, **avec ou sans extension** : `NUL` et `NUL.cbz` sont tous deux
/// refusés par le système de fichiers.
fn est_nom_reserve(nom: &str) -> bool {
    let base = nom.split('.').next().unwrap_or("").trim_end();
    let base = base.to_ascii_uppercase();
    matches!(base.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || (base.len() == 4
            && (base.starts_with("COM") || base.starts_with("LPT"))
            && matches!(base.as_bytes()[3], b'1'..=b'9'))
}

/// Porte le nom à **80 caractères** au maximum, **sans** que deux noms distincts
/// ne se confondent au-delà du plafond : au-delà, un suffixe dérivé du nom
/// complet est ajouté. C'est ce qui maintient `sanitize` injectif — au sens de
/// « deux noms lisibles distincts ne donnent jamais le même dossier ».
///
/// La fonction de hachage (@tag fnv64) est volontairement portable : un
/// `DefaultHasher` rendrait le nom de dossier dépendant du compilateur.
fn plafonner(nom: &str) -> String {
    const LIMITE: usize = 80;
    const LARGEUR_SUFFIXE: usize = 8; // HEXADÉCIMAL SUR 4 OCTETS
    const PREFIXE: usize = LIMITE - LARGEUR_SUFFIXE - 1; // le tiret
    if nom.chars().count() <= LIMITE {
        return nom.to_string();
    }
    let prefixe: String = nom.chars().take(PREFIXE).collect();
    format!("{prefixe}-{:08x}", (fnv64(nom.as_bytes()) as u32))
}
fn fnv64(octets: &[u8]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for &o in octets {
        hash ^= o as u64;
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
}
/// Dossier par défaut, avant tout choix de l'utilisateur :
/// `%APPDATA%\Tatami\downloads`.
pub fn downloads_root_par_defaut() -> PathBuf {
    std::env::var("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("Tatami")
        .join("downloads")
}

/// Dossier de téléchargements **effectif**, choisi par l'utilisateur.
///
/// Il est stocké dans un global plutôt que relu de la base à chaque appel :
/// `downloads_root()` est appelé des dizaines de fois par téléchargement, et
/// une lecture SQLite dans un chemin critique (dont des tests) n'apporte rien.
/// La base reste la source de vérité au démarrage : `init_racine_telechargements`
/// lit le réglage et initialise ce global une fois pour toutes.
static RACINE: std::sync::OnceLock<std::sync::RwLock<PathBuf>> = std::sync::OnceLock::new();

fn racine() -> &'static std::sync::RwLock<PathBuf> {
    RACINE.get_or_init(|| std::sync::RwLock::new(downloads_root_par_defaut()))
}

/// Dossier de téléchargements en vigueur.
pub fn downloads_root() -> PathBuf {
    racine().read().unwrap_or_else(|e| e.into_inner()).clone()
}

/// Dossier de fixture **effacé à la fin du test**, quel qu'en soit le dénouement.
///
/// Pourquoi il vit dans la vraie racine des téléchargements : `list_library`
/// n'accepte que cette racine, donc une fixture ailleurs ne prouverait rien du
/// confinement. Pourquoi il faut donc le faire disparaître : mesuré le
/// 2026-10-06, `%APPDATA%\Tatami\downloads` contenait **34 dossiers
/// `tatami-test-*`** et un `IMPORT/`, tous laissés par des `cargo test`
/// précédents. Ils sont dans la bibliothèque de la personne qui teste, et
/// personne ne les nettoie à la main.
///
/// Le nettoyage passe par `Drop`, donc il a lieu **même si le test panique** —
/// c'est tout l'intérêt d'un garde-fou plutôt que d'un `remove_dir_all` à la
/// fin d'une fonction, qui serait sauté par le `panic!` juste avant.
///
/// Le dossier est unique par **pid + compteur** : les tests sont parallèles, et
/// deux fixtures de même nom se marcheraient dessus.
#[cfg(test)]
pub(crate) struct DossierDeTest {
    chemin: PathBuf,
}

#[cfg(test)]
impl DossierDeTest {
    /// Fixture de **source** : un dossier frère, directement sous la racine.
    pub(crate) fn new(tag: &str) -> Self {
        Self::creer(tag)
    }

    /// Destination d'un import : `import_webtoon` écrit dans
    /// `downloads_root/IMPORT/<titre>`, donc la fixture doit être ce **titre**
    /// sous `IMPORT/`, pas un dossier frère — sinon le garde effacerait une
    /// fixture pendant que le dossier réellement écrit reste sur le disque.
    ///
    /// Le nom garde le préfixe `tatami-import-` que les tests attendent déjà, donc
    /// un reliquat d'une version antérieure reste reconnaissable.
    pub(crate) fn pour_import(tag: &str) -> Self {
        Self::creer_import(tag)
    }

    /// Nom unique par **pid + compteur** : les tests sont parallèles, et deux
    /// fixtures de même nom se marcheraient dessus.
    fn nom_unique(prefixe: &str) -> String {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static COMPTEUR: AtomicUsize = AtomicUsize::new(0);
        let n = COMPTEUR.fetch_add(1, Ordering::SeqCst);
        format!("{prefixe}-{}-{n}", std::process::id())
    }

    /// Fixture de source : dossier frère, directement sous la racine.
    fn creer(tag: &str) -> Self {
        let chemin = downloads_root().join(Self::nom_unique(&format!("tatami-test-{tag}")));
        let _ = std::fs::remove_dir_all(&chemin);
        std::fs::create_dir_all(&chemin).expect("dossier de test");
        DossierDeTest { chemin }
    }

    /// Destination d'import : `IMPORT/<titre>`. `IMPORT/` est créé par
    /// `import_webtoon` lui-même, donc le garde ne le crée pas — il efface le
    /// dossier du titre à la fin.
    fn creer_import(tag: &str) -> Self {
        let chemin = downloads_root()
            .join(crate::library::SOURCE_IMPORT)
            .join(Self::nom_unique(&format!("tatami-import-{tag}")));
        let _ = std::fs::remove_dir_all(&chemin);
        DossierDeTest { chemin }
    }

    pub(crate) fn chemin(&self) -> PathBuf {
        self.chemin.clone()
    }
}

#[cfg(test)]
impl Drop for DossierDeTest {
    fn drop(&mut self) {
        // Un échec ici ne doit pas faire échouer le test : le dossier restant
        // est inoffensif, contrairement à un `expect` dans un `Drop`.
        let _ = std::fs::remove_dir_all(&self.chemin);
    }
}

/// Change le dossier de téléchargements pour cette session.
///
/// Le dossier doit déjà exister et être accessible en écriture : c'est
/// vérifié par `verifier_dossier_ecriture` avant d'appeler cette fonction, donc
/// aucun téléchargement ne peut échouer plus tard sur un chemin invalide.
pub fn definir_racine_telechargements(chemin: PathBuf) {
    *racine().write().unwrap_or_else(|e| e.into_inner()) = chemin;
}

/// Vérifie qu'un dossier peut servir de racine de téléchargements.
///
/// On **essaie vraiment** d'y écrire un fichier : un dossier existe sans pour
/// autant être accessible (droits, antivirus, disque plein). Vérifier l'existence
/// ne suffirait pas, et l'échec n'apparaîtrait qu'au premier téléchargement.
pub fn verifier_dossier_ecriture(chemin: &Path) -> Result<PathBuf, String> {
    if !chemin.is_dir() {
        return Err(format!("ce dossier n'existe pas : {}", chemin.display()));
    }
    let canonique = chemin
        .canonicalize()
        .map_err(|e| format!("dossier inaccessible : {e}"))?;
    let sonde = canonique.join(format!(".tatami-sonde-{}", std::process::id()));
    std::fs::write(&sonde, b"").map_err(|e| format!("dossier non accessible en écriture : {e}"))?;
    let _ = std::fs::remove_file(&sonde);
    Ok(canonique)
}

/// Un chapitre = un `.cbz` directement dans le dossier du webtoon :
/// `downloads/<source>/<webtoon>/<titre-chapitre>.cbz` (pas de sous-dossier par
/// chapitre : c'est plus lisible dans l'explorateur).
fn chapter_dir(root: &Path, source: &str, manga_title: &str) -> PathBuf {
    root.join(sanitize(source)).join(sanitize(manga_title))
}

/// Nom du fichier `.cbz` d'un chapitre, **publié pour l'import (2.4)**.
///
/// L'import doit écrire le fichier sous **exactement** ce nom : c'est cette
/// fonction que le lecteur rappelle pour retrouver un chapitre local. Un import
/// qui choisirait un nom de fichier de son côté produirait un chapitre listé
/// mais introuvable — le décalage silencieux que la fonction existe pour
/// empêcher.
pub(crate) fn chapter_file_name(chapter_title: &str, chapter_url: &str) -> String {
    let base = sanitize(chapter_title);
    if base.is_empty() || base == "sans-titre" {
        let dernier = chapter_url.rsplit('/').next().unwrap_or("").trim();
        // Un UUID ne dit rien de lisible : mesuré sur MANGADEX, le chapitre
        // s'appelait `22d20970-38d1-48aa-bbd3-9ca9afbbb3dc.cbz`. Valide, mais
        // inutilisable dans un lecteur externe — donc inutile, ce qui vide
        // l'exigence permanente « un .cbz par chapitre ».
        let est_uuid = segments_uuid(dernier);
        // Sinon, on cherche un **numéro** de chapitre dans l'URL : lisible et
        // stable. Le filtre exclut les groupes de plus de 4 chiffres, sinon un
        // identifiant long produit « Chapitre 4831 ».
        let numero = if est_uuid {
            None
        } else {
            chapter_url
                .split(|c: char| !c.is_ascii_digit())
                .filter(|s| !s.is_empty() && s.len() <= 4)
                .max_by_key(|s| s.len())
        };
        match numero {
            Some(n) => format!("Chapitre {n}.cbz"),
            None => {
                // Repli : le segment d'URL, sauf s'il est un UUID — auquel cas
                // un nom fixe vaut mieux qu'un identifiant illisible.
                let from_url = if est_uuid {
                    String::new()
                } else {
                    sanitize(dernier)
                };
                if from_url.is_empty() {
                    "Chapitre.cbz".to_string()
                } else {
                    format!("{from_url}.cbz")
                }
            }
        }
    } else {
        format!("{base}.cbz")
    }
}

/// `8-4-4-4-12` en hexadécimaux : la forme que prennent les URL de chapitre
/// quand la source n'a pas de titre lisible.
fn segments_uuid(s: &str) -> bool {
    let parts: Vec<&str> = s.split('-').collect();
    if parts.len() != 5 {
        return false;
    }
    let attendus = [8usize, 4, 4, 4, 12];
    parts
        .iter()
        .zip(attendus.iter())
        .all(|(p, n)| p.len() == *n && p.chars().all(|c| c.is_ascii_hexdigit()))
}

/// Chapitres réellement absents du dossier d'un webtoon — c'est ce qui permet
/// de reprendre un téléchargement interrompu sans tout retélécharger. Le nom de
/// fichier attendu est calculé ici, avec la même règle que `download_chapter` :
/// la règle de nommage reste à un seul endroit.
///
/// `urls` et `titles` sont deux tableaux parallèles (la macro de commande
/// Tauri n'accepte pas les structures Personalized en argument).
#[tauri::command]
pub async fn missing_chapters(
    source: String,
    title: String,
    urls: Vec<String>,
    titles: Vec<String>,
) -> Result<Vec<String>, String> {
    let dir = chapter_dir(&downloads_root(), &source, &title);
    Ok(missing_in_dir(&dir, &urls, &titles))
}

/// Logique de reprise : renvoie les chapitres dont le `.cbz` est absent.
/// Un `.part` compte comme absent (téléchargement interrompu).
fn missing_in_dir(dir: &Path, urls: &[String], titles: &[String]) -> Vec<String> {
    // Même résolution de noms, dans le même ordre, que `run_download` : c'est ce
    // qui garantit qu'un chapitre absent ici ET absent après téléchargement
    // sont le même chapitre. Résoudre différemment des deux côtés ferait
    // retélécharger un `.cbz` déjà présent, ce que l'exigence permanente
    // interdit.
    let presents = noms_presents(dir);
    let mut attribues: Vec<String> = Vec::new();
    let mut manquants = Vec::new();
    for (i, url) in urls.iter().enumerate() {
        let label = titles.get(i).map_or("", String::as_str);
        let nom = resoudre_nom(chapter_file_name(label, url), &mut attribues);
        if !presents.contains(&nom) {
            manquants.push(url.clone());
        }
    }
    manquants
}

/// Fichiers déjà écrits dans le dossier. Un `.part` en est exclu : il compte
/// comme absent, donc le chapitre sera retéléchargé.
///
/// `unwrap_or_default()` transforme une **erreur de lecture du disque** en
/// « aucun chapitre existant ». Mesuré le 2026-10-09 (audit) : c'est le seul
/// endroit où un `read_dir` qui échoue produit ce silence, et la conséquence est
/// réelle — `resoudre_nom` (`:605`) retombe alors sur le nom de base et le
/// `.cbz` existant peut être **écrasé**. Un disque plein, un dossier verrouillé
/// ou un antivirus donnent cette erreur, et ils arrivent au pire moment.
///
/// On journalise donc au lieu d'avaler : l'appelant garde son comportement (il
/// ne peut pas faire mieux sans nom à comparer), mais la cause est traçable.
fn noms_presents(dir: &Path) -> Vec<String> {
    match std::fs::read_dir(dir) {
        Ok(iter) => iter
            .flatten()
            .map(|e| e.file_name().to_string_lossy().to_string())
            .filter(|n| !n.ends_with(".part"))
            .collect(),
        Err(e) => {
            tracing::warn!(
                "lecture du dossier {} impossible ({e}) : les noms presents sont \
                 inconnus, un fichier existant pourrait etre reecrit",
                dir.display()
            );
            Vec::new()
        }
    }
}

/// Attribue un nom de fichier à un chapitre, sans jamais collisionner avec un
/// nom déjà attribué.
///
/// `chapter_file_name` dérive le nom du titre, donc deux chapitres de même
/// titre (« Chapitre 1 » en volume original et en édition corrigée), ou
/// partageant leurs 80 premiers caractères, tombaient sur le **même** `.cbz` :
/// le second écrasait le premier, qui devenissait invisible à la bibliothèque
/// et à la reprise. L'exigence « un `.cbz` par chapitre » n'était pas tenue.
///
/// La règle est volontairement symétrique, pour que les deux appelants
/// s'accordent. Appelé avec les noms présents sur le disque, comme le fait le
/// téléchargement, ou avec une liste vide, comme le fait la reprise, le
/// PREMIER chapitre homonyme prend toujours le nom de base et le suivant le
/// premier nom suffixé libre. Ce qui compte n'est pas le contexte fourni, mais
/// que les deux appels voisinent le même chapitre dans le même état du dossier :
/// c'est le cas, les chapitres étant téléchargés dans l'ordre de la liste.
fn resoudre_nom(base: String, attribues: &mut Vec<String>) -> String {
    if !attribues.contains(&base) {
        attribues.push(base.clone());
        return base;
    }
    // « Ch. 1.cbz » -> stem « Ch. 1 », extension « .cbz ».
    let (stem, ext) = match base.rsplit_once('.') {
        Some((s, e)) => (s.to_string(), format!(".{e}")),
        None => (base.clone(), String::new()),
    };
    for n in 2..1000 {
        let candidat = format!("{stem} ({n}){ext}");
        if !attribues.contains(&candidat) {
            attribues.push(candidat.clone());
            return candidat;
        }
    }
    // 999 chapitres homonymes : on sort de la boucle plutôt que de boucler.
    let candidat = format!("{stem} ({}){ext}", attribues.len() + 1);
    attribues.push(candidat.clone());
    candidat
}

/// Chemin du `.cbz` d'un chapitre, **tel que l'écriture l'a réellement nommé**.
///
/// ## Pourquoi le `meta.json` fait foi
///
/// L'écriture désambiguïse les noms homonymes : deux chapitres de même titre
/// deviennent `Ch. 1.cbz` puis `Ch. 1 (2).cbz` (`resoudre_nom`). Cette version
/// recalculait le nom avec `chapter_file_name` seul, donc **tombaient toujours
/// sur le premier** : le lecteur affichait les pages du chapitre 1 en croyant
/// ouvrir le chapitre 2. Silencieux, et invisible pour l'utilisateur.
///
/// La correspondance n'est donc pas recalculée, elle est **lue** :
/// `library::note_chapter` écrit dans `meta.json` le fichier réellement produit
/// pour chaque `chapter_url`. C'est la seule source qui dise la vérité, et elle
/// est déjà là.
///
/// Le repli sur le nom de base reste nécessaire — l'import écrit le fichier sans
/// passer par `note_chapter`, et un dossier sans `meta.json` est valide.
pub async fn chapter_file_for(
    source: &str,
    manga_title: &str,
    chapter_title: &str,
    chapter_url: &str,
) -> PathBuf {
    let root = downloads_root();
    let dir = chapter_dir(&root, source, manga_title);
    if !chapter_url.is_empty() {
        if let Some(nom) = fichier_note(&dir, chapter_url).await {
            return dir.join(nom);
        }
    }
    dir.join(chapter_file_name(chapter_title, chapter_url))
}

/// Le nom de fichier que le `meta.json` associe à cette URL de chapitre.
async fn fichier_note(dir: &Path, chapter_url: &str) -> Option<String> {
    let brut = tokio::fs::read_to_string(dir.join(super::library::META))
        .await
        .ok()?;
    let entree: super::library::MangaEntry = serde_json::from_str(&brut).ok()?;
    entree
        .chapters
        .into_iter()
        .find(|c| c.url == chapter_url)
        .map(|c| c.file)
        .filter(|f| !f.is_empty())
}

/// Le chemin du chapitre **déjà téléchargé**, s'il existe vraiment.
///
/// Le nom vient du `meta.json` — donc de l'écriture — et il n'est renvoyé que
/// si le fichier est **présent sur le disque**. Sans cette seconde condition, un
/// `meta.json` qui mentionne un fichier supprimé à la main ferait échouer le
/// téléchargement au lieu de le refaire : l'exigence « ne jamais retélécharger
/// ce qui est là » ne doit pas devenir « ne jamais retélécharger ce qui manque ».
async fn chapitre_deja_present(dir: &Path, chapter_url: &str) -> Option<String> {
    if chapter_url.is_empty() {
        return None;
    }
    let nom = fichier_note(dir, chapter_url).await?;
    let chemin = dir.join(&nom);
    if chemin.is_file() {
        Some(chemin.to_string_lossy().to_string())
    } else {
        None
    }
}

/// Liste les pages d'un chapitre **déjà téléchargé**, sans moteur ni réseau.
///
/// Tâche 2.3. Renvoie une liste vide si le fichier est absent : à l'appelant de
/// décider s'il faut passer par le moteur. Ne jamais faire erreur ici, sinon un
/// chapitre téléchargé mais mal nommé deviendrait illisible au lieu de retomber
/// sur la source.
#[tauri::command]
pub async fn local_chapter_pages(
    cache: tauri::State<'_, std::sync::Arc<super::pagecache::PageCache>>,
    source: String,
    manga_title: String,
    chapter_title: String,
    chapter_url: String,
) -> Result<Vec<serde_json::Value>, String> {
    let fichier = chapter_file_for(&source, &manga_title, &chapter_title, &chapter_url).await;
    if !fichier.is_file() {
        return Ok(Vec::new());
    }
    let pages =
        crate::zipread::pages_de(&fichier).map_err(|e| format!("{} : {e}", fichier.display()))?;
    Ok(entries_to_pages(&pages, &fichier, &cache))
}

/// Pages d'un `.cbz` désigné par son chemin, pour la vue Téléchargements.
///
/// **Contrôle de confinement obligatoire.** La bibliothèque fournit un chemin
/// venant de l'interface : sans vérification, l'interface pourrait faire lire
/// n'importe quel fichier du disque à travers le protocole d'URI. Le chemin doit
/// être canonisé **et** rester sous le dossier de téléchargement (AGENTS.md §7 :
/// le protocole n'accepte que des identifiants connus).
#[tauri::command]
pub async fn local_file_pages(
    cache: tauri::State<'_, std::sync::Arc<super::pagecache::PageCache>>,
    dir: String,
    name: String,
) -> Result<Vec<serde_json::Value>, String> {
    let racine = downloads_root()
        .canonicalize()
        .unwrap_or_else(|_| downloads_root());
    // Le chemin est assemblé ici, pas dans l'interface : c'est le chemin
    // **assemblé** qui est contrôlé, pas la pièce reçue.
    let chemin = PathBuf::from(&dir).join(&name);
    let canonique = chemin
        .canonicalize()
        .map_err(|_| "chapitre introuvable".to_string())?;
    if !canonique.starts_with(&racine) {
        return Err("chemin hors du dossier de téléchargement".to_string());
    }
    if !canonique.is_file() {
        return Ok(Vec::new());
    }
    let pages = crate::zipread::pages_de(&canonique).map_err(|e| format!("{e}"))?;
    Ok(entries_to_pages(&pages, &canonique, &cache))
}

/// Transcription commune aux deux commandes de lecture locale.
///
/// Chaque page emporte ses **dimensions réelles**, lues dans l'en-tête de
/// l'image. C'est ce qui permet au mode webtoon (tâche 1.9) de connaître la
/// hauteur de la bande dès le premier rendu, donc de ne monter qu'une fenêtre
/// de pages au lieu des 303 (mesuré : Killer Peter, 303 pages dans un `.cbz`).
/// Sans elles, l'interface ne peut qu'estimer — et une estimation fausse fait
/// perdre la position de lecture.
fn entries_to_pages(
    pages: &[(usize, String)],
    fichier: &Path,
    cache: &super::pagecache::PageCache,
) -> Vec<serde_json::Value> {
    let entrees = crate::zipread::lire_entrees(fichier).unwrap_or_default();
    let mut lecture = std::fs::File::open(fichier).ok();
    pages
        .iter()
        .map(|(rang, nom)| {
            let (largeur, hauteur) = lecture
                .as_mut()
                .zip(entrees.iter().find(|e| e.index == *rang))
                .and_then(|(f, e)| crate::zipread::dimensions_entree(f, e))
                .map(|(l, h)| (l as usize, h as usize))
                .unwrap_or((0, 0));
            serde_json::json!({
                "index": rang,
                "token": cache.register_local(fichier, *rang),
                "name": nom,
                "width": largeur,
                "height": hauteur,
            })
        })
        .collect()
}

/// Supprime les `.part` laissés par un téléchargement interrompu (le chapitre
/// sera retéléchargé en entier).
#[tauri::command]
pub async fn clean_partials(source: String, title: String) -> Result<usize, String> {
    let dir = chapter_dir(&downloads_root(), &source, &title);
    let mut removed = 0;
    let Ok(mut files) = tokio::fs::read_dir(&dir).await else {
        return Ok(0);
    };
    while let Ok(Some(file)) = files.next_entry().await {
        let name = file.file_name().to_string_lossy().to_string();
        if name.ends_with(".part") && tokio::fs::remove_file(file.path()).await.is_ok() {
            removed += 1;
        }
    }
    Ok(removed)
}

/// Verrou d'unicité, libéré même en cas de panique ou de sortie anticipée.
///
/// La version précédente posait `*running = true`, appelait `run_download`, puis
/// reposait `false` **après** l'appel : une panique entre les deux laissait
/// `running` à `true` pour le reste de la vie du processus, et tout
/// téléchargement futur était refusé avec « un téléchargement est déjà en
/// cours » alors qu'aucun n'était en cours. `Drop` rend la libération
/// inconditionnelle — c'est tout l'intérêt d'un garde-fou RAII.
struct VerrouTelechargement {
    etat: Arc<std::sync::Mutex<std::collections::HashMap<String, ()>>>,
    source: String,
}

impl VerrouTelechargement {
    fn prendre(
        etat: &Arc<std::sync::Mutex<std::collections::HashMap<String, ()>>>,
        source: &str,
    ) -> Result<Self, String> {
        let mut actifs = etat.lock().unwrap_or_else(|e| e.into_inner());
        if actifs.len() >= 2 {
            return Err("deux téléchargements sont déjà en cours".to_string());
        }
        if actifs.contains_key(source) {
            return Err(format!("un téléchargement est déjà en cours pour {source}"));
        }
        actifs.insert(source.to_string(), ());
        Ok(Self {
            etat: etat.clone(),
            source: source.to_string(),
        })
    }
}

impl Drop for VerrouTelechargement {
    fn drop(&mut self) {
        if let Ok(mut actifs) = self.etat.lock() {
            actifs.remove(&self.source);
        }
    }
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn download_chapter(
    app: AppHandle,
    sup: State<'_, Arc<Supervisor>>,
    state: State<'_, DownloadState>,
    source: String,
    manga_url: String,
    manga_title: String,
    chapter_url: String,
    chapter_title: String,
    cover_url: Option<String>,
    total_chapters: Option<usize>,
) -> Result<String, String> {
    let _verrou = VerrouTelechargement::prendre(&state.actifs, &source)?;

    // **Idempotence avant tout réseau.** Mesuré le 2026-10-15 : `resoudre_nom`
    // reçoit les noms **déjà présents** sur le disque, donc un chapitre déjà
    // téléchargé recevait `Ch. 1 (2).cbz`. Ensuite `note_chapter` voyait l'URL
    // déjà connue et n'ajoutait rien : le doublon restait sur le disque,
    // non noté, donc compté par `loose_files` — la bibliothèque annonçait « 2/1
    // chapitres ». Deux clics sur « Télécharger » suffisaient.
    //
    // Le garde est ici, **avant** le moteur, donc avant tout octet téléchargé :
    // il rend le téléchargement idempotent plutôt que de compter sur
    // l'interface pour ne pas redemander.
    let dir = chapter_dir(&downloads_root(), &source, &manga_title);
    if let Some(deja) = chapitre_deja_present(&dir, &chapter_url).await {
        return Ok(deja);
    }

    run_download(
        &app,
        &sup,
        &source,
        &manga_url,
        &manga_title,
        &chapter_url,
        &chapter_title,
        cover_url.as_deref(),
        total_chapters.unwrap_or(0),
    )
    .await
}

/// Une page téléchargée en parallèle : (index, octets, type d'image).
/// L'alias évite un `Vec<JoinHandle<…>>` au-delà du seuil de complexité de
/// clippy — le type n'est pas plus simple, il est seulement nommé.
type PageTelechargee = Result<(usize, Vec<u8>, crate::image::ImageKind), String>;

#[allow(clippy::too_many_arguments)]
async fn run_download(
    app: &AppHandle,
    sup: &Arc<Supervisor>,
    source: &str,
    manga_url: &str,
    manga_title: &str,
    chapter_url: &str,
    chapter_title: &str,
    cover_url: Option<&str>,
    total_chapters: usize,
) -> Result<String, String> {
    let root = downloads_root();
    let dir = chapter_dir(&root, source, manga_title);
    // Le nom est résolu APRÈS `create_dir_all` : la désambiguïsation teste
    // l'existence sur le disque, donc le dossier doit déjà exister pour que le
    // dossier vide ne soit pas pris pour un dossier plein.
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|e| format!("création dossier: {e}"))?;
    // Les noms déjà écrits sont « attribués » : un chapitre homonyme ne peut pas
    // les reprendre, sinon il écraserait un `.cbz` valide.
    let file = dir.join(resoudre_nom(
        chapter_file_name(chapter_title, chapter_url),
        &mut noms_presents(&dir),
    ));

    // 1. details + chapters en parallèle : details pour l'URL publique (challenge),
    //    chapters pour peupler le cache que `pages` exige (source native). Les deux
    //    sont indépendants, on les lance ensemble pour gagner un aller-retour.
    let (details_res, _chapters_res) = tokio::join!(
        sup.request_with_retry(
            "details",
            serde_json::json!({
                "source": source,
                "url": manga_url,
                "mangaTitle": manga_title,
            }),
            FETCH_TIMEOUT,
        ),
        sup.request_with_retry(
            "chapters",
            serde_json::json!({
                "source": source,
                "mangaUrl": manga_url,
                "mangaTitle": manga_title,
            }),
            FETCH_TIMEOUT,
        ),
    );
    let details = details_res?;
    // On ignore le résultat de chapters (seul le cache compte), mais on propage
    // une erreur réseau.
    _chapters_res?;
    let _ = &details;

    // URL à ouvrir si une vérification est nécessaire : la page du webtoon sur
    // le site, que l'utilisateur voit et où il confirme lui-même (âge, captcha).
    // Mesuré : HENTAIORIGINES répond 200 OK avec sa page de vérification d'âge,
    // donc le moteur ne signale AUCUN challenge et rien ne s'ouvrait.
    let page_publique = details
        .get("result")
        .and_then(|r| r.get("manga"))
        .and_then(|m| m.get("publicUrl"))
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .unwrap_or_else(|| manga_url.to_string());

    // Demande à l'utilisateur de passer la vérification, puis échoue
    // proprement : le chapitre reste absent, donc il sera retenté.
    let demander_verification = |raison: &str| -> String {
        let _ = app.emit(
            "download://challenge",
            serde_json::json!({ "source": source, "url": page_publique, "reason": raison }),
        );
        format!(
            "{source} demande une vérification ({raison}). \
             Une fenêtre va s'ouvrir : confirmez-y vous-même, puis relancez le téléchargement."
        )
    };

    // 2. pages du chapitre
    let pages = sup
        .request_with_retry(
            "pages",
            serde_json::json!({ "source": source, "chapterUrl": chapter_url }),
            FETCH_TIMEOUT,
        )
        .await?;
    let page_list = pages
        .get("result")
        .and_then(|r| r.get("pages"))
        .and_then(|v| v.as_array())
        .ok_or("pages: réponse invalide")?;
    let total = page_list.len();
    if total == 0 {
        return Err("chapitre sans page".to_string());
    }
    // Garde : un chapitre a une seule page est suspect. Mesure le 2026-10-17 :
    // la source native RIMUSCANS renvoyait 1 page (en realite 17 sur le site) et
    // le telechargement ecrivait un .cbz a une page sans rien signaler. On refuse
    // d'ecrire un fichier probablement incomplet et on explique la piste.
    if total <= 1 {
        return Err(format!("ce chapitre ne contient qu'une page ({}) - la source a probablement renvoye un resultat incomplet. Essayez la source personnalisee (custom:*) si elle existe.", total));
    }

    // Mesuré : sur une source à vérification d'âge, le moteur renvoie des pages
    // dont l'URL d'image est VIDE. Le téléchargement partait alors chercher
    // l'adresse "" et écrivait la page d'accueil du site 45 fois dans le
    // `.cbz`. Mieux vaut demander la vérification et échouer proprement.
    let urls_vides = page_list
        .iter()
        .filter(|p| {
            p.get("url")
                .and_then(|v| v.as_str())
                .map(str::trim)
                .unwrap_or("")
                .is_empty()
        })
        .count();
    if urls_vides > 0 {
        return Err(demander_verification(&format!(
            "{urls_vides} page(s) sur {total} sans URL d'image"
        )));
    }

    // Écriture au fil de l'eau dans un .part : les pages ne sont jamais gardées en
    // mémoire (un chapitre de 42 Mo tiendrait mal dans un tas de 256 Mo avec la
    // copie base64), et un arrêt en cours de route ne laisse pas un .cbz tronqué
    // compté comme téléchargé — le renommage n'a lieu qu'à la fin.
    let partial = file.with_extension("cbz.part");
    // Un .part d'une session précédente est écrasé : le chapitre sera
    // retéléchargé en entier plutôt que résumé (on ne sait pas où il s'était arrêté).
    let _ = tokio::fs::remove_file(&partial).await;
    let mut cbz = CbzWriter::create(&partial).map_err(|e| format!("création: {e}"))?;
    // Téléchargement parallèle des images, concurrence bornée à 4 (le moteur
    // n'a que MAX_WORKERS travailleurs ; au-delà, on sature la source et le
    // réseau). Les résultats sont collectés puis écrits dans l'ordre, pour que
    // le .cbz garde ses pages dans le bon ordre.
    const CONCURRENCE: usize = 4;
    let page_urls: Vec<String> = page_list
        .iter()
        .map(|p| {
            p.get("url")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string()
        })
        .collect();
    // Garde déjà faite plus haut : total >= 2 ici.

    let semaphore = Arc::new(tokio::sync::Semaphore::new(CONCURRENCE));
    let mut taches: Vec<tokio::task::JoinHandle<PageTelechargee>> = Vec::with_capacity(total);
    for (i, url) in page_urls.iter().enumerate() {
        let page_url = url.clone();
        let source = source.to_string();
        let sup = Arc::clone(sup);
        let sem = Arc::clone(&semaphore);
        let app = app.clone();
        let chapter_title = chapter_title.to_string();
        let page_publique = page_publique.clone();
        taches.push(tokio::spawn(async move {
            let _permit = sem.acquire().await.map_err(|e| format!("semaphore: {e}"))?;
            let _ = app.emit(
                "download://progress",
                Progress {
                    source: source.clone(),
                    chapter: chapter_title.clone(),
                    page: i + 1,
                    total,
                    done: false,
                    error: None,
                },
            );
            let fetched = sup
                .request_with_retry(
                    "fetchImage",
                    serde_json::json!({ "source": source, "pageUrl": page_url }),
                    FETCH_TIMEOUT,
                )
                .await;
            let value: Value = match fetched {
                Ok(v) => v,
                Err(e) => {
                    let message = format!("page {}: {e}", i + 1);
                    let _ = app.emit(
                        "download://progress",
                        Progress {
                            source: source.clone(),
                            chapter: chapter_title.clone(),
                            page: i + 1,
                            total,
                            done: false,
                            error: Some(message.clone()),
                        },
                    );
                    return Err(message);
                }
            };
            let result = value.get("result").ok_or("fetchImage: pas de result")?;
            let b64 = result
                .get("data")
                .and_then(|v| v.as_str())
                .ok_or("image: pas de data")?;
            let content_type = result
                .get("contentType")
                .and_then(|v| v.as_str())
                .unwrap_or("image/jpeg");
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(b64)
                .map_err(|e| format!("décodage base64: {e}"))?;
            let kind = match crate::image::detect(&bytes) {
                Some(k) => k,
                None => {
                    let _ = app.emit(
                        "download://progress",
                        Progress {
                            source: source.clone(),
                            chapter: chapter_title.clone(),
                            page: i + 1,
                            total,
                            done: false,
                            error: Some("réponse qui n'est pas une image".to_string()),
                        },
                    );
                    let _ = app.emit(
                        "download://challenge",
                        serde_json::json!({ "source": source, "url": page_publique, "reason": "le serveur a renvoyé une page web au lieu de l'image" }),
                    );
                    return Err("réponse qui n'est pas une image".to_string());
                }
            };
            let _ = content_type;
            Ok((i, bytes, kind))
        }));
    }

    // Collecte des résultats (propagation de la première erreur).
    let mut images: Vec<Option<(Vec<u8>, crate::image::ImageKind)>> = vec![None; total];
    for tache in taches {
        let res = tache.await.map_err(|e| format!("join: {e}"))?;
        match res {
            Ok((i, bytes, kind)) => {
                images[i] = Some((bytes, kind));
            }
            Err(e) => return Err(e),
        }
    }

    // Écriture séquentielle dans l'ordre des pages.
    for (i, img) in images.iter().enumerate() {
        let (bytes, kind) = img.as_ref().expect("image manquante");
        cbz.add_image_kind(i + 1, *kind, bytes)
            .map_err(|e| format!("écriture page {}: {e}", i + 1))?;
    }

    let written = total;
    // `finish` vide le tampon ET synchronise le fichier. Sans `sync_all`, une
    // coupure juste après le `rename` laissait un `.cbz` tronqué portant un nom
    // définitif : `missing_in_dir` teste la présence du nom, donc le chapitre
    // était compté téléchargé alors que le fichier ne s'ouvrait pas.
    cbz.sync_and_finish()
        .map_err(|e| format!("finalisation: {e}"))?;
    tokio::fs::rename(&partial, &file)
        .await
        .map_err(|e| format!("renommage {}: {e}", file.display()))?;

    // État d'avancement du webtoon (library.rs) : c'est ce que lit la vue
    // Téléchargements pour afficher « 12/40 chapitres ».
    //
    // L'erreur n'est plus avalée. Le `.cbz` est bon sur le disque, mais un
    // `meta.json` faux rend le chapitre invisible dans la bibliothèque et fausse
    // `total_chapters` : l'utilisateur voit « 11/40 » pour un chapitre
    // téléchargé, et la reprise le retéléchargera. C'est un défaut à part
    // entière, pas un détail à masquer — on renvoie l'erreur, le fichier reste.
    super::library::note_chapter(
        &dir,
        source,
        manga_title,
        manga_url,
        cover_url,
        total_chapters,
        chapter_url,
        chapter_title,
        &file.file_name().unwrap_or_default().to_string_lossy(),
    )
    .await
    .map_err(|e| {
        format!("chapitre écrit sur le disque mais invisible dans la bibliothèque : {e}")
    })?;

    let _ = app.emit(
        "download://progress",
        Progress {
            source: source.to_string(),
            chapter: chapter_title.to_string(),
            page: written,
            total,
            done: true,
            error: None,
        },
    );
    Ok(file.to_string_lossy().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn le_nom_de_fichier_reste_lisible_sans_titre_de_chapitre() {
        // Le cas mesure sur MANGADEX : `title` vaut null, le dernier segment de
        // l'URL est un UUID. Le fichier etait valide mais inutilisable dans un
        // lecteur externe.
        let nom = chapter_file_name("", "/chapter/22d20970-38d1-48aa-bbd3-9ca9afbbb3dc");
        assert_eq!(nom, "Chapitre.cbz");
        assert!(
            !nom.contains("22d20970"),
            "un UUID ne doit pas finir dans le nom : {nom}"
        );
        // Un UUID contient des groupes de chiffres : sans détection explicite,
        // le filename retenait « 48 » et produisait « Chapitre 48.cbz ».
        assert!(segments_uuid("22d20970-38d1-48aa-bbd3-9ca9afbbb3dc"));
        assert!(!segments_uuid("12"));
        assert!(!segments_uuid("chapitre-2"));

        // Un numéro de chapitre dans l'URL est utilisé : lisible et stable.
        assert_eq!(
            chapter_file_name("", "/manga/x/chapter/12/"),
            "Chapitre 12.cbz"
        );
        // Un titre normal prime toujours.
        assert_eq!(chapter_file_name("Chapitre 3", "/ch/9/"), "Chapitre 3.cbz");
    }

    #[test]
    fn les_noms_reserves_par_windows_sont_evites() {
        // Windows refuse `CON`, `NUL`, `COM1`… avec ou sans extension. Un titre
        // de webtoon `CON` produisait un dossier que `create_dir_all` refuse :
        // le téléchargement ne pouvait pas aboutir.
        for nom in ["CON", "con", "NUL", "AUX", "COM1", "LPT9", "PRN"] {
            let assaini = sanitize(nom);
            assert!(
                !est_nom_reserve(&assaini),
                "`{nom}` est resté un nom réservé après assainissement : `{assaini}`"
            );
            assert!(!assaini.is_empty(), "`{nom}` a produit un nom vide");
        }
        // Un nom normal ne doit surtout pas être prefixé.
        assert_eq!(sanitize("Conan"), "Conan");
        assert_eq!(sanitize("Nulitta"), "Nulitta");
    }

    /// Deux titres qui ne diffèrent qu'**après** 80 caractères ne doivent pas
    /// tomber dans le même dossier.
    ///
    /// Le plafond de longueur exists pour la limite de chemin de Windows, mais il
    /// rendait `sanitize` **non injectif** : `sanitize` ne gardait que les 80
    /// premiers caractères, donc deux séries au catalogue presque identiques
    /// partageaient un dossier. Les chapitres s'y mélangeaient, le compteur de la
    /// bibliothèque comptait une série au lieu de deux, et `meta.json` décrivait
    /// les deux. Aucun message : c'est une fusion silencieuse de deux séries.
    ///
    /// Le correctif est un suffixe dérivé du nom **complet**, donc la/folders
    /// dépend du nom entier et pas de son préfixe.
    #[test]
    fn deux_titres_partageant_leur_prefixe_ne_donnent_pas_le_meme_dossier() {
        let prefixe = "A".repeat(80);
        let a = chapter_dir(
            Path::new("d"),
            "SRC",
            &format!("{prefixe}{}", "B".repeat(20)),
        );
        let b = chapter_dir(
            Path::new("d"),
            "SRC",
            &format!("{prefixe}{}", "C".repeat(20)),
        );
        assert_ne!(
            a, b,
            "deux titres distincts ont obtenu le même dossier : {a:?}"
        );
        // Et l'inverse, qui compte autant : deux titres **identiques** doivent
        // toujours donner le même dossier, sinon le même webtoon est cherché
        // dans deux endroits à chaque téléchargement.
        assert_eq!(
            chapter_dir(
                Path::new("d"),
                "SRC",
                &format!("{prefixe}{}", "B".repeat(20))
            ),
            a
        );
        // Un titre court ne doit pas être changé : le suffixe ne s'ajoute que
        // lorsqu'il faut couper, sinon tous les noms lisibles de la bibliothèque
        // deviendraient illisibles.
        assert_eq!(sanitize("One Piece"), "One Piece");
        // Le plafond tient toujours : c'est la raison d'être du suffixe.
        let long = chapter_dir(Path::new("d"), "SRC", &"Z".repeat(200));
        let lf = long.file_name().and_then(|n| n.to_str()).unwrap_or("");
        assert!(
            lf.chars().count() <= 80,
            "le dossier a dépassé le plafond de 80 caractères : {lf:?} fait {}",
            lf.chars().count()
        );
    }

    #[test]
    fn deux_chapitres_de_meme_titre_obtiennent_deux_fichiers() {
        // Exigence permanente : « un .cbz par chapitre ». Deux chapitres
        // homonymes s'écrasaient mutuellement.
        let mut attribues: Vec<String> = Vec::new();
        let a = resoudre_nom("Chapitre 1.cbz".to_string(), &mut attribues);
        let b = resoudre_nom("Chapitre 1.cbz".to_string(), &mut attribues);
        assert_eq!(a, "Chapitre 1.cbz");
        assert_ne!(
            a, b,
            "deux chapitres homonymes ont reçu le même nom de fichier"
        );
        let c = resoudre_nom("Chapitre 1.cbz".to_string(), &mut attribues);
        assert_ne!(b, c);
        assert_eq!(attribues.len(), 3, "chaque chapitre doit être attribué");
    }

    #[test]
    fn la_reprise_voit_les_deux_chapitres_homonnes_comme_absents() {
        let garde = dossier_de_test("homonymes");
        let dir = garde.chemin();
        std::fs::create_dir_all(&dir).unwrap();
        let urls = vec!["c/1".to_string(), "c/1bis".to_string()];
        let titles = vec!["Chapitre 1".to_string(), "Chapitre 1".to_string()];
        // Les deux doivent être signalés absents, pas « l'un des deux ».
        assert_eq!(missing_in_dir(&dir, &urls, &titles), urls);
        // Une fois l'un écrit, seul l'autre reste à télécharger.
        std::fs::write(dir.join("Chapitre 1.cbz"), b"x").unwrap();
        assert_eq!(
            missing_in_dir(&dir, &urls, &titles),
            vec!["c/1bis".to_string()]
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn le_verrou_de_telechargement_se_libere_sur_panique() {
        let etat = Arc::new(std::sync::Mutex::new(std::collections::HashMap::new()));
        // Le cas réel : une panique entre l'acquisition et la libération
        // laissait tous les téléchargements futurs refusés jusqu'au redémarrage.
        let resultat = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let _verrou = VerrouTelechargement::prendre(&etat, "SOURCE_A").expect("premier verrou");
            panic!("panique simulée au milieu du téléchargement");
        }));
        assert!(resultat.is_err(), "la panique doit se propager");
        assert!(
            etat.lock().unwrap().is_empty(),
            "le verrou doit être libéré même après une panique, sinon \
             « un téléchargement est déjà en cours » pour toujours"
        );
        // Et un nouveau téléchargement doit pouvoir repartir, sur n'importe
        // quelle source.
        assert!(
            VerrouTelechargement::prendre(&etat, "SOURCE_A").is_ok(),
            "un nouveau téléchargement doit être accepté"
        );
    }

    #[test]
    fn deux_telechargements_de_sources_distinctes_passent() {
        let etat = Arc::new(std::sync::Mutex::new(std::collections::HashMap::new()));
        let _a = VerrouTelechargement::prendre(&etat, "SOURCE_A").expect("A actif");
        // Une deuxième source différente est acceptée : le moteur n'y mêle pas
        // les deux.
        let b = VerrouTelechargement::prendre(&etat, "SOURCE_B");
        assert!(b.is_ok(), "deux sources distinctes doivent passer");
        assert_eq!(etat.lock().unwrap().len(), 2);
        // Un troisième, n'importe quelle source, est refusé.
        assert!(
            VerrouTelechargement::prendre(&etat, "SOURCE_C").is_err(),
            "le slot maximum est deux"
        );
        // Deux téléchargements de la MÊME source restent refusés : ils
        // attendraient le même `Mutex` du moteur, donc le deuxième n'y gagne
        // rien — il échouerait juste en rendant le suivi muet.
        assert!(
            VerrouTelechargement::prendre(&etat, "SOURCE_A").is_err(),
            "une source déjà active ne recommence pas"
        );
    }

    #[test]
    fn le_verrou_relache_une_source_sans_toucher_l_autre() {
        let etat = Arc::new(std::sync::Mutex::new(std::collections::HashMap::new()));
        let _a = VerrouTelechargement::prendre(&etat, "SOURCE_A").expect("A");
        {
            let _b = VerrouTelechargement::prendre(&etat, "SOURCE_B").expect("B");
        } // B se termine
        let m = etat.lock().unwrap();
        assert!(
            m.contains_key("SOURCE_A") && m.len() == 1,
            "A doit rester actif"
        );
    }

    /// Dossier de fixture sous la vraie racine des téléchargements, comme le
    /// fait `library.rs` : `%TEMP%` n'est pas expansé sur cette machine.
    /// Le dossier est effacé à la sortie du test — voir `DossierDeTest`.
    fn dossier_de_test(tag: &str) -> super::DossierDeTest {
        super::DossierDeTest::new(tag)
    }

    /// Deux chapitres de **même titre** : le second ne doit pas s'ouvrir sur le
    /// fichier du premier.
    ///
    /// C'est le défaut mesuré le 2026-10-15. L'écriture désambiguïse les noms
    /// (`resoudre_nom` : `Ch. 1.cbz` puis `Ch. 1 (2).cbz`) et `note_chapter`
    /// écrit la correspondance dans `meta.json`. La lecture, elle, recalculait le
    /// nom avec `chapter_file_name` seul : elle retombait **toujours** sur le
    /// premier. Le lecteur affichait donc les pages du chapitre 1 en croyant
    /// ouvrir le chapitre 2 — sans le moindre message.
    ///
    /// Le test écrit les deux fichiers avec un contenu **distinct** (1 page contre
    /// 2) : sans cela, il ne vérifierait qu'un nom de fichier et raterait
    /// exactement le défaut — deux fichiers de même contenu ne se distinguent pas.
    #[tokio::test]
    async fn deux_chapitres_de_meme_titre_ouvrent_deux_fichiers_differents() {
        use crate::cbz::CbzWriter;
        use crate::image::ImageKind;

        let source = "SRC-HOMONYMES";
        let titre = "Serie a homonymes";
        let dir = super::chapter_dir(&downloads_root(), source, titre);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("dossier de chapitre");

        let ecrire = |nom: &str, pages: usize| {
            let mut w = CbzWriter::create(&dir.join(nom)).expect("archive");
            for i in 0..pages {
                w.add_image_kind(i + 1, ImageKind::Jpeg, &[7u8; 64])
                    .expect("page");
            }
            w.finish_vec().expect("archive complete");
        };
        ecrire("Ch. 1.cbz", 1);
        ecrire("Ch. 1 (2).cbz", 2);

        // Le `meta.json` que `note_chapter` aurait écrit.
        let meta = serde_json::json!({
            "version": 1,
            "title": titre,
            "source": source,
            "url": "/m/1",
            "total_chapters": 2,
            "chapters": [
                {"url": "c/original", "title": "Ch. 1", "file": "Ch. 1.cbz"},
                {"url": "c/corrige",  "title": "Ch. 1", "file": "Ch. 1 (2).cbz"}
            ]
        });
        std::fs::write(
            dir.join(crate::library::META),
            serde_json::to_string_pretty(&meta).unwrap(),
        )
        .expect("meta.json");

        // Les deux chapitres portent le MÊME titre : c'est la condition du défaut.
        let premier = chapter_file_for(source, titre, "Ch. 1", "c/original").await;
        let second = chapter_file_for(source, titre, "Ch. 1", "c/corrige").await;

        assert_ne!(
            premier, second,
            "les deux chapitres de même titre retombent sur le même fichier : \
             le lecteur affiche les pages du premier en croyant ouvrir le second"
        );
        assert_eq!(premier.file_name().unwrap(), "Ch. 1.cbz");
        assert_eq!(second.file_name().unwrap(), "Ch. 1 (2).cbz");

        // Et le contenu suit bien : 1 page contre 2. Un test qui ne compare que
        // des noms ne voit pas un fichier écrasé.
        let p1 = crate::zipread::pages_de(&premier)
            .expect("premier lisible")
            .len();
        let p2 = crate::zipread::pages_de(&second)
            .expect("second lisible")
            .len();
        assert_eq!(
            (p1, p2),
            (1, 2),
            "les pages doivent être celles du bon chapitre"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Le repli sans `meta.json` doit rester exact — l'import écrit le fichier
    /// sans passer par `note_chapter`, et un dossier sans meta est valide.
    ///
    /// Sans ce contrôle, « toujours lire le meta » donnerait un chapitre
    /// introuvable sur toute installation antérieure.
    #[tokio::test]
    async fn sans_meta_json_le_nom_de_base_reste_valide() {
        let source = "SRC-SANS-META";
        let titre = "Serie sans meta";
        let dir = super::chapter_dir(&downloads_root(), source, titre);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("dossier de chapitre");
        std::fs::write(dir.join("Ch. 9.cbz"), b"x").expect("fichier");

        let fichier = chapter_file_for(source, titre, "Ch. 9", "").await;
        assert!(
            fichier.is_file(),
            "le repli doit retrouver le fichier : {}",
            fichier.display()
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Un chapitre déjà téléchargé ne doit pas produire de doublon.
    ///
    /// Le défaut mesuré le 2026-10-15 : `resoudre_nom` partait des noms présents
    /// sur le disque, donc un second clic créait `Ch. 1 (2).cbz` ; ensuite
    /// `note_chapter`, qui déduplique par URL, n'ajoutait rien — le fichier
    /// restait non noté mais comptait dans `loose_files`, et la bibliothèque
    /// affichait « 2/1 chapitres ».
    ///
    /// On éprouve donc le garde lui-même, qui est la partie à risque : il doit
    /// nommer le fichier existant, et **refuser** de nommer quoi que ce soit
    /// quand le fichier a disparu.
    #[tokio::test]
    async fn un_chapitre_deja_telecharge_ne_produit_pas_de_doublon() {
        // Le garde est **transporté** jusqu'à la fin du test. Mesuré le
        // 2026-10-15 : `DossierDeTest::new(..).chemin()` liait le `PathBuf` et
        // lâchait le garde dans la même expression, donc `Drop` effaçait le
        // dossier avant le premier `write` — l'erreur venait du harnais, pas du
        // comportement étudié.
        let garde = super::DossierDeTest::new("doublon");
        let dir = garde.chemin();
        let meta = serde_json::json!({
            "version": 1, "title": "T", "source": "S", "url": "/m/1", "total_chapters": 1,
            "chapters": [{"url": "c/1", "title": "Ch. 1", "file": "Ch. 1.cbz"}]
        });
        std::fs::write(
            dir.join(crate::library::META),
            serde_json::to_string(&meta).unwrap(),
        )
        .unwrap();

        // Pas encore de fichier : le garde ne doit rien inventer, sinon un
        // `meta.json` became stale interdirait à jamais de retélécharger.
        assert_eq!(
            chapitre_deja_present(&dir, "c/1").await,
            None,
            "un fichier absent ne compte pas comme téléchargé"
        );

        std::fs::write(dir.join("Ch. 1.cbz"), b"x").unwrap();
        assert_eq!(
            chapitre_deja_present(&dir, "c/1").await.as_deref(),
            Some(dir.join("Ch. 1.cbz").to_string_lossy().as_ref()),
            "un chapitre présent doit être nommé tel quel, pas receives un suffixe"
        );

        // URL inconnue : le garde est muet, l'appelant télécharge.
        assert_eq!(chapitre_deja_present(&dir, "c/2").await, None);
        // Et le chapitre sans URL — l'import — n'est jamais « déjà présent ».
        assert_eq!(chapitre_deja_present(&dir, "").await, None);
    }

    #[test]
    fn reprise_ne_retelecharge_pas_les_chapitres_deja_la() {
        let dir = std::env::temp_dir().join(format!("tatami-dl-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let urls = vec!["c/1".to_string(), "c/2".to_string(), "c/3".to_string()];
        let titles = vec![
            "Chapitre 1".to_string(),
            "Chapitre 2".to_string(),
            "Chapitre 3".to_string(),
        ];
        // Le chapitre 2 est déjà là, le 3 est interrompu (.part).
        std::fs::write(dir.join("Chapitre 2.cbz"), b"x").unwrap();
        std::fs::write(dir.join("Chapitre 3.cbz.part"), b"x").unwrap();

        let missing = missing_in_dir(&dir, &urls, &titles);
        assert_eq!(missing, vec!["c/1".to_string(), "c/3".to_string()]);

        // Une fois téléchargé, il ne manque plus que le premier.
        std::fs::write(dir.join("Chapitre 1.cbz"), b"x").unwrap();
        std::fs::write(dir.join("Chapitre 3.cbz"), b"x").unwrap();
        std::fs::remove_file(dir.join("Chapitre 3.cbz.part")).unwrap();
        assert!(missing_in_dir(&dir, &urls, &titles).is_empty());

        let _ = std::fs::remove_dir_all(&dir);
    }

    // Test de régression du bug « lecture webtoon cassée ».
    //
    // On construit un `.cbz` dont les pages sont des JPEG **à profil ICC**, comme
    // ceux que sert MangaDEX. Avant le correctif, `EN_TETE_MAX` valait 512 : le
    // marqueur SOF0 de ces images est à l'offset 632, donc `dimensions_entree`
    // rendait `None`, toutes les dimensions arrivaient à 0, et l'interface
    // empilait les pages (`height: 0` + `position: absolute` + `top: 0`).
    //
    // La mesure qui a condosduit à ce test : sur le fichier réel
    // `downloads/MANGADEX/No. 6/Chapitre 1.cbz`, la commande `local_file_pages`
    // renvoyait `width: 0, height: 0` pour ses 77 pages, et le DOM mesuré dans
    // l'application donnait `.webtoon-inner` à `height: 0px` avec 77 images en
    // `top: 0px`.
    #[test]
    fn les_dimensions_sont_lues_meme_avec_un_profil_icc() {
        use crate::cbz::CbzWriter;
        use crate::image::ImageKind;

        /// JPEG dont le SOF0 est à ~632 octets (profil ICC de 472 octets).
        fn jpeg_bavard(largeur: u16, hauteur: u16) -> Vec<u8> {
            let mut v = vec![0xFF, 0xD8, 0xFF];
            v.extend_from_slice(&[0xE0, 0x00, 0x10]);
            v.extend_from_slice(b"JFIF\0");
            v.extend_from_slice(&[0x01, 0x02, 0x00, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00]);
            v.extend_from_slice(&[0xFF, 0xE2]);
            v.extend_from_slice(&(472u16).to_be_bytes());
            v.extend_from_slice(b"ICC_PROFILE");
            v.resize(v.len() + 472 - 12, 0x42);
            for _ in 0..2 {
                v.extend_from_slice(&[0xFF, 0xDB]);
                v.extend_from_slice(&(67u16).to_be_bytes());
                v.resize(v.len() + 65, 0x11);
            }
            v.extend_from_slice(&[0xFF, 0xC0, 0x00, 0x11, 0x08]);
            v.extend_from_slice(&hauteur.to_be_bytes());
            v.extend_from_slice(&largeur.to_be_bytes());
            v.extend_from_slice(&[0x03, 0x01, 0x11, 0x00, 0x02, 0x11, 0x01, 0x03, 0x11, 0x01]);
            v
        }

        let dir = crate::download::downloads_root()
            .join(format!("tatami-test-webtoon-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let chemin = dir.join("Chapitre 1.cbz");

        let taille = 12;
        {
            let mut w = CbzWriter::create(&chemin).unwrap();
            for i in 1..=taille {
                // 800 x 2171 : format d'une page de webtoon.
                w.add_image_kind(i, ImageKind::Jpeg, &jpeg_bavard(800, 2171))
                    .unwrap();
            }
            w.sync_and_finish().unwrap();
        }

        let pages = crate::zipread::pages_de(&chemin).unwrap();
        assert_eq!(pages.len(), taille);

        let mut fichier = std::fs::File::open(&chemin).unwrap();
        let entrees = crate::zipread::lire_entrees(&chemin).unwrap();
        for (rang, _) in &pages {
            assert_eq!(
                crate::zipread::dimensions_entree(&mut fichier, &entrees[*rang]),
                Some((800, 2171)),
                "page {rang} sans dimension correcte"
            );
        }

        // Et la transposition complète, celle que fait la commande : c'est ce
        // JSON que l'interface reçoit, et c'est lui qui portait le 0.
        let cache = crate::pagecache::PageCache::new(dir.join("cache"));
        let json = entries_to_pages(&pages, &chemin, &cache);
        assert!(
            json.iter().all(|p| p["width"].as_u64().unwrap_or(0) == 800
                && p["height"].as_u64().unwrap_or(0) == 2171),
            "la commande doit transmettre width/height : {}",
            json[0]
        );

        let _ = std::fs::remove_dir_all(&dir);
    }
}
