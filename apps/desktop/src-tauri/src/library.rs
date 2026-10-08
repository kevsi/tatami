//! Bibliothèque locale des webtoons téléchargés.
//!
//! Un dossier = un webtoon : `%APPDATA%\Tatami\downloads\<source>\<webtoon>\`
//! contenant les `.cbz` et un `meta.json` (titre, source, url, couverture,
//! nombre de chapitres, chapitres présents). Le `meta.json` est la seule source
//! de vérité de l'état d'avancement ; s'il est absent (téléchargement antérieur
//! à cette version), l'état est reconstruit à partir des fichiers présents.
//!
//! SQLite arrivera plus tard (bibliothèque complète, favoris, signets) : ce
//! fichier est le format intermédiaire, volontairement lisible et remplaçable.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tauri::State;

const META: &str = "meta.json";
const META_VERSION: u32 = 1;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct ChapterEntry {
    pub url: String,
    pub title: String,
    pub file: String,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct MangaEntry {
    /// Métadonnées du `meta.json` ; `dir` est calculé à la lecture.
    #[serde(default)]
    pub version: u32,
    pub title: String,
    pub source: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub cover_url: Option<String>,
    #[serde(default)]
    pub total_chapters: usize,
    #[serde(default)]
    pub chapters: Vec<ChapterEntry>,
    #[serde(default)]
    pub updated_at: i64,

    #[serde(skip)]
    pub dir: String,
    #[serde(skip)]
    pub size_bytes: u64,
    #[serde(skip)]
    pub loose_files: Vec<String>,
}

impl MangaEntry {
    pub fn downloaded(&self) -> usize {
        self.chapters.len().max(self.loose_files.len())
    }

    /// Terminé = le total est connu et tous les chapitres sont là.
    pub fn complete(&self) -> bool {
        self.total_chapters > 0 && self.downloaded() >= self.total_chapters
    }
}

/// Vue renvoyée à l'interface : le `meta.json` plus ce qu'on sait calculer
/// (chapitres téléchargés, terminé ou non, poids du dossier).
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LibraryItem {
    pub title: String,
    pub source: String,
    pub url: String,
    pub cover_url: Option<String>,
    /// Jeton `tatami://cover/...` à utiliser par l'interface (cf. `cover_url`).
    pub cover_token: Option<String>,
    pub dir: String,
    pub downloaded: usize,
    pub total_chapters: usize,
    pub complete: bool,
    pub size_bytes: u64,
    pub updated_at: i64,
    pub chapters: Vec<ChapterEntry>,
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

pub fn manga_dir(root: &Path, source: &str, title: &str) -> PathBuf {
    root.join(crate::download::sanitize(source))
        .join(crate::download::sanitize(title))
}

/// Poids total du dossier, récursif : les téléchargements antérieurs à la
/// version CBZ rangeaient les chapitres dans un sous-dossier.
fn dir_size(dir: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    entries
        .flatten()
        .map(|entry| match entry.metadata() {
            Ok(meta) if meta.is_dir() => dir_size(&entry.path()),
            Ok(meta) => meta.len(),
            Err(_) => 0,
        })
        .sum()
}

/// Racine de la bibliothèque, **confinée** au dossier de téléchargements.
///
/// Le chemin vient de l'interface : sans contrôle, `list_library` ferait
/// lister n'importe quel dossier du disque et en exposerait le contenu à la
/// webview. Même mécanisme que `download::local_file_pages` : canonicalisation
/// du dossier de téléchargements, canonicalisation du chemin reçu, puis
/// vérification que le second reste sous le premier. La chaîne vide reste
/// valide : elle signifie « la racine des téléchargements ».
fn racine_confinement(root: &str) -> Result<PathBuf, String> {
    let racine = super::download::downloads_root()
        .canonicalize()
        .unwrap_or_else(|_| super::download::downloads_root());
    if root.is_empty() {
        return Ok(racine);
    }
    let candidat = PathBuf::from(root)
        .canonicalize()
        .map_err(|_| "dossier de bibliothèque introuvable".to_string())?;
    if !candidat.starts_with(&racine) {
        return Err("refus : hors du dossier de téléchargements".to_string());
    }
    Ok(candidat)
}

/// Liste les webtoons téléchargés (un par dossier), du plus récent au plus ancien.
///
/// Commande fine : elle valide la racine et délègue. Toute la logique est dans
/// `lister`, testable sans Tauri — `State` n'est pas constructible dans un test.
#[tauri::command]
pub async fn list_library(
    root: String,
    cache: State<'_, Arc<crate::pagecache::PageCache>>,
) -> Result<Vec<LibraryItem>, String> {
    lister(&root, &cache).await
}

/// Le travail réel de `list_library`, testable sans l'injection Tauri.
pub async fn lister(
    root: &str,
    cache: &crate::pagecache::PageCache,
) -> Result<Vec<LibraryItem>, String> {
    let root = racine_confinement(root)?;
    let mut out: Vec<MangaEntry> = Vec::new();
    let mut sources = tokio::fs::read_dir(&root)
        .await
        .map_err(|e| format!("lecture {}: {e}", root.display()))?;

    while let Ok(Some(source_entry)) = sources.next_entry().await {
        if !source_entry.path().is_dir() {
            continue;
        }
        let source_name = source_entry.file_name().to_string_lossy().to_string();
        let Ok(mut mangas) = tokio::fs::read_dir(source_entry.path()).await else {
            continue;
        };
        while let Ok(Some(manga_entry)) = mangas.next_entry().await {
            let dir = manga_entry.path();
            if !dir.is_dir() {
                continue;
            }
            let mut entry: MangaEntry = match tokio::fs::read_to_string(dir.join(META)).await {
                Ok(raw) => serde_json::from_str(&raw).unwrap_or_default(),
                Err(_) => MangaEntry::default(),
            };
            let Ok(mut files) = tokio::fs::read_dir(&dir).await else {
                continue;
            };
            while let Ok(Some(file)) = files.next_entry().await {
                let name = file.file_name().to_string_lossy().to_string();
                if name != META && !name.ends_with(".part") {
                    entry.loose_files.push(name);
                }
            }
            // Reconstruit l'état si le meta est absent ou incomplet.
            if entry.title.is_empty() {
                entry.title = manga_entry.file_name().to_string_lossy().to_string();
            }
            if entry.source.is_empty() {
                entry.source = source_name.clone();
            }
            if entry.total_chapters == 0 {
                entry.total_chapters = entry.downloaded();
            }
            entry.dir = dir.to_string_lossy().to_string();
            entry.size_bytes = dir_size(&dir);
            out.push(entry);
        }
    }
    out.sort_by(|a, b| b.updated_at.cmp(&a.updated_at).then(a.title.cmp(&b.title)));
    Ok(out
        .into_iter()
        .map(|entry| {
            // Le jeton est calculé AVANT le déplacement des champs : il lit
            // `source`, `url` et `cover_url`, que la structuelle consomme.
            let cover_token = crate::pagecache::jeton_cover(
                cache,
                &entry.source,
                &entry.url,
                entry.cover_url.as_deref(),
            );
            LibraryItem {
                downloaded: entry.downloaded(),
                complete: entry.complete(),
                title: entry.title,
                source: entry.source,
                url: entry.url,
                cover_url: entry.cover_url,
                cover_token,
                dir: entry.dir,
                total_chapters: entry.total_chapters,
                size_bytes: entry.size_bytes,
                updated_at: entry.updated_at,
                chapters: entry.chapters,
            }
        })
        .collect())
}

// =========================================================================
// Import d'un dossier de webtoon (tâche 2.4)
// =========================================================================

/// Source attribuée à un import.
///
/// Un dossier importé n'a pas de source : il n'a jamais été listé par une
/// source. On lui donne donc une source **sintétique**, qui n'existe dans aucun
/// `listSources` — ce qui est le comportement voulu : un import ne doit jamais
/// prétendre venir d'un site, sinon « retélécharger les chapitres manquants »
/// irait les chercher chez un site qui ne les a jamais eus.
pub const SOURCE_IMPORT: &str = "IMPORT";

/// Pourquoi un fichier n'a pas été importé.
///
/// Un **code stable**, pas une phrase : les motifs sont affichés par
/// l'interface, qui a deux langues. Renvoyer du français depuis Rust affichait
/// « pas un fichier .cbz » dans un écran anglais — mesuré le 2026-10-05. C'est la
/// même règle que le contrat du moteur (`code` stable + `message` lisible).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum RefusCode {
    /// Le fichier n'est pas un `.cbz` (ou c'est un dossier portant cette
    /// extension).
    PasUnCbz,
    /// L'archive n'est pas un ZIP lisible.
    ArchiveIllisible,
    /// L'archive est lisible mais ne contient aucune image.
    AucuneImage,
    /// Le nom de fichier ne peut pas servir de nom de chapitre adressable.
    NomChapitre,
    /// Un autre fichier du dossier porte déjà ce nom de chapitre.
    Collision,
    /// La copie a échoué (disque, droits, destination…).
    CopieImpossible,
}

/// Un refus, avec son code et le détail technique éventuellement utile.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Refus {
    pub code: RefusCode,
    /// Détail libre : message de la couche ZIP, chemin en cause. **Jamais**
    /// traduit — c'est une donnée technique, affichée telle quelle.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

impl Refus {
    fn nouveau(code: RefusCode) -> Self {
        Self { code, detail: None }
    }
    fn avec(code: RefusCode, detail: String) -> Self {
        Self {
            code,
            detail: Some(detail),
        }
    }
}

/// Ce qu'un import a produit, pour que l'interface dise la vérité.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportRapport {
    /// Dossier de destination, sous le dossier de téléchargements.
    pub dir: String,
    /// Chapitres effectivement copiés.
    pub importes: usize,
    /// Fichiers **refusés**, avec le motif. Non vide = l'import est partiel,
    /// et l'interface doit le dire plutôt que de montrer un compte flatteur.
    pub refuses: Vec<(String, Refus)>,
    /// Le webtoon tel qu'il apparaît dans la bibliothèque.
    pub titre: String,
}

/// Importe un dossier de `.cbz` déjà téléchargés, lisible hors ligne.
///
/// ## Pourquoi **copier** et non pointer
///
/// `local_file_pages` refuse tout chemin hors du dossier de téléchargements :
/// l'interface ne peut pas faire lire un fichier du disque par le protocole
/// d'URI. Un import qui se contenterait d'enregistrer le chemin d'origine serait
/// donc illisible. On **copie** dans `downloads_root/<IMPORT>/<titre>/`, et le
/// dossier importé devient un webtoon comme un autre.
///
/// ## Le `meta.json` est écrit même en cas de refus partiel
///
/// Sinon le dossier vide aurait disparu à la prochaine liste, et l'utilisateur
/// aurait perdu la trace de ce qu'il avait tenté d'importer.
///
/// ## Un chapitre qui existe déjà n'est pas réécrit
///
/// La copie est **idempotente par nom de fichier** : réimporter le même dossier
/// ne duplique rien et ne réécrit pas les 43 Mo déjà présents. Un `.cbz`
/// déjà téléchargé sous un autre nom, lui, est recopié — sans cela un
/// import « dupliqué » serait indétectable alors que l'utilisateur verrait deux
/// fois le même chapitre.
///
/// ## Le chemin d'origine est **lu**, jamais écrit
///
/// C'est la seule commande du projet qui accepte un chemin hors du dossier de
/// téléchargements, parce que la fonction *est* « importer un dossier
/// choisi ». Le confinement n'est donc pas possible ici — mais il n'est pas
/// nécessaire non plus : on ne lit que des `.cbz` du dossier indiqué et on
/// n'écrit **que** sous `downloads_root`, via `manga_dir` + `sanitize`. Un
/// chemin malveillant ne peut donc pas faire écrire hors de la bibliothèque.
#[tauri::command]
pub async fn import_webtoon(dir: String, titre: Option<String>) -> Result<ImportRapport, String> {
    // Le dossier doit exister et être un dossier : sans ce contrôle, un chemin
    // de fichier donnerait un message « aucun fichier .cbz » qui détourne vers la
    // mauvaise cause.
    let meta_dossier = std::fs::metadata(&dir).map_err(|e| format!("dossier illisible : {e}"))?;
    if !meta_dossier.is_dir() {
        return Err("le chemin indiqué n'est pas un dossier".to_string());
    }

    let racine = super::download::downloads_root();
    // Le nom du dossier d'origine est repris quand l'interface n'en fournit pas :
    // l'utilisateur a sélectionné un dossier, ce nom lui est donc familier.
    let nom = {
        let p = PathBuf::from(&dir);
        p.file_name()
            .map(|n| n.to_string_lossy().to_string())
            .filter(|n| !n.trim().is_empty())
            .unwrap_or_else(|| "import".to_string())
    };
    let titre = titre
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .unwrap_or(nom);

    let mut entree = MangaEntry {
        version: META_VERSION,
        title: titre.clone(),
        source: SOURCE_IMPORT.to_string(),
        // Une URL vide **signifie** « pas de source », et c'est ce que
        // `complete()` et l'écran des favoris doivent voir. On ne fabrique donc
        // pas une URL qui ne mènerait nulle part.
        url: String::new(),
        cover_url: None,
        total_chapters: 0,
        chapters: Vec::new(),
        updated_at: now_secs(),
        ..MangaEntry::default()
    };

    let mut rapport = ImportRapport {
        titre: titre.clone(),
        ..ImportRapport::default()
    };

    let mut fichiers: Vec<(String, std::path::PathBuf)> = Vec::new();
    let mut cibles: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut entrees = tokio::fs::read_dir(&dir)
        .await
        .map_err(|e| format!("dossier illisible : {e}"))?;
    while let Some(e) = entrees
        .next_entry()
        .await
        .map_err(|e| format!("lecture du dossier : {e}"))?
    {
        let pathi = e.path();
        let is_file = pathi.is_file();
        let Some(nom_fichier) = pathi.file_name().map(|n| n.to_string_lossy().to_string()) else {
            continue;
        };
        if !is_file || !nom_fichier.to_ascii_lowercase().ends_with(".cbz") {
            rapport
                .refuses
                .push((nom_fichier, Refus::nouveau(RefusCode::PasUnCbz)));
            continue;
        }
        fichiers.push((nom_fichier, pathi));
    }
    if fichiers.is_empty() {
        return Err("aucun fichier .cbz dans ce dossier".to_string());
    }
    // Tri par nom : l'ordre des chapitres suit l'ordre du nom, donc
    // `Chapitre 2.cbz` avant `Chapitre 10.cbz` — ce qui n'est pas garanti par le
    // système de fichiers, qui ne trie pas. Le tri sert ici à rendre
    // **l'ordre des refus** déterministe ; l'ordre de lecture, lui, est trié au
    // moment de l'écriture du `meta.json`, sur les noms écrits.
    fichiers.sort_by_key(|f| nom_chapitres(&f.0));

    let destination = manga_dir(&racine, SOURCE_IMPORT, &titre);
    // Le dossier **pouvait déjà exister** : c'est un réimport. La destination
    // est un nom assaini, donc deux titres différents y retombent (`Foo: Bar`
    // et `Foo? Bar` donnent le même dossier, et `sanitize` plafonne à 80
    // caractères). On note donc l'état *avant* la création, parce qu'il décide
    // de ce qu'on a le droit d'effacer plus bas.
    let destination_existait = destination.is_dir();
    tokio::fs::create_dir_all(&destination)
        .await
        .map_err(|e| format!("création du dossier : {e}"))?;

    for (nom_fichier, source_chemin) in fichiers {
        // Un `.cbz` illisible est refusé **avant** toute écriture : l'utilisateur
        // ne doit pas trouver dans sa bibliothèque un chapitre qu'il ne peut pas
        // ouvrir. `pages_de` échoue sur un ZIP invalide, et renvoie vide si le
        // ZIP ne contient aucune image.
        //
        // Le refus est aussi **avant** le dossier de destination : un dossier
        // d'import ne doit pas être créé puis laissé vide si tout est refusé.
        match super::zipread::pages_de(&source_chemin) {
            Ok(pages) if !pages.is_empty() => {}
            Ok(_) => {
                rapport
                    .refuses
                    .push((nom_fichier, Refus::nouveau(RefusCode::AucuneImage)));
                continue;
            }
            Err(e) => {
                rapport.refuses.push((
                    nom_fichier,
                    Refus::avec(RefusCode::ArchiveIllisible, e.to_string()),
                ));
                continue;
            }
        }

        let Some(nom_cible) = nom_chapitre_lecteur(&nom_fichier) else {
            rapport
                .refuses
                .push((nom_fichier, Refus::nouveau(RefusCode::NomChapitre)));
            continue;
        };
        // Deux fichiers ne peuvent pas porter le nom du même chapitre : le
        // second écraserait le premier, et la bibliothèque n'en announce
        // qu'un. C'est atteignable (`sans-titre.cbz` et `.cbz` retombent tous
        // deux sur `Chapitre.cbz`, et `sanitize` plafonne à 80 caractères).
        if !cibles.insert(nom_cible.clone()) {
            rapport.refuses.push((
                nom_fichier,
                Refus::avec(RefusCode::Collision, nom_cible.clone()),
            ));
            continue;
        }
        let cible = destination.join(&nom_cible);
        // Idempotence : ne pas recopier un chapitre déjà là. `tokio::fs::copy`
        // écrase, donc sans cette garde un réimport réécrirait 43 Mo.
        if !cible.is_file() {
            if let Err(e) = tokio::fs::copy(&source_chemin, &cible).await {
                rapport.refuses.push((
                    nom_fichier.clone(),
                    Refus::avec(RefusCode::CopieImpossible, e.to_string()),
                ));
                continue;
            }
        }
        rapport.importes += 1;
    }

    if rapport.importes == 0 {
        // Rien d'exploitable : on ne laisse pas un dossier vide derrière, et
        // surtout on n'écrit pas de `meta.json` — la bibliothèque n'afficherait
        // alors qu'un webtoon vide, ce qui est pire qu'une erreur franche.
        //
        // **Seulement si c'est nous qui l'avons créé.** Effacer un dossier qui
        // précédait reviendrait à supprimer les chapitres d'un import antérieur
        // alors que l'utilisateur n'a rien demandé d'autre qu'un nouvel import
        // : mesuré le 2026-10-08, un second import sans archive lisible
        // effaçait les 20 chapitres du premier (`remove_dir_all` était
        // inconditionnel). Le test qui couvre ce cas compte les fichiers
        // présents sur le disque, pas le code de retour.
        if !destination_existait {
            let _ = tokio::fs::remove_dir_all(&destination).await;
        } else {
            tracing::warn!(
                "import sans chapitre importable, dossier préexistant conservé : {}",
                destination.display()
            );
        }
        return Err(format!(
            "aucun chapitre importable dans ce dossier ({} fichier(s) refusé(s))",
            rapport.refuses.len()
        ));
    }

    // La liste des chapitres est relue **sur le disque**, pas accumulée pendant
    // la copie. Deux raisons, toutes deux mesurables :
    //  - réimporter un dossier **plus petit** ne doit pas faire disparaître de
    //    `meta.json` les chapitres d'un import précédent, qui sont pourtant
    //    toujours là ;
    //  - un `.cbz` assaini (`:` → `_`) doit être enregistré sous le nom **qui
    //    existe**, pas sous celui du fichier d'origine, sinon le chapitre est
    //    listé mais introuvable.
    entree.chapters = chapters_presents(&destination).await?;
    entree.total_chapters = entree.chapters.len();
    let json = serde_json::to_string_pretty(&entree).map_err(|e| e.to_string())?;
    tokio::fs::write(destination.join(META), json)
        .await
        .map_err(|e| format!("meta.json : {e}"))?;

    rapport.dir = destination.to_string_lossy().to_string();
    tracing::info!(
        "import : {} chapitres, {} refusés, -> {}",
        rapport.importes,
        rapport.refuses.len(),
        rapport.dir
    );
    Ok(rapport)
}

/// Nom sous lequel un fichier d'import sera connu du **lecteur**, ou `None` si
/// ce nom ne peut pas fonctionner.
///
/// Le lecteur retrouve un chapitre local en appelant
/// `download::chapter_file_name(titre, url)` — donc l'import écrit le fichier
/// sous le nom que **cette** fonction renvoie, jamais sous le nom d'origine
/// assaini.
///
/// `None` quand le nom ne se laisse pas addrounder : c'est le cas d'un fichier
/// dont le nom s'assaine en `sans-titre` ou en vide (`.cbz`), parce que le
/// lecteur remplacerait alors le titre par son repli `Chapitre.cbz` et ne
/// retrouverait pas le fichier. Un chapitre listé mais introuvable est pire
/// qu'un refus : l'utilisateur voit le chapitre et clique sur une page vide.
fn nom_chapitre_lecteur(nom_fichier: &str) -> Option<String> {
    let titre = crate::download::sanitize(sans_extension(nom_fichier));
    let cible = crate::download::chapter_file_name(&titre, "");
    // `chapter_file_name` ajoute l'extension, donc la seule adresse possible
    // est `<titre assaini>.cbz`. Toute autre réponse signifie qu'un
    // remplacement a eu lieu.
    if cible == format!("{titre}.cbz") {
        Some(cible)
    } else {
        None
    }
}

/// Liste les chapitres **présents sur le disque** d'un dossier de destination.
///
/// Le `meta.json` est ainsi toujours derivé du disque, jamais l'inverse : c'est
/// la seule façon qu'un import réexécuté, ou un fichier supprimé à la main, ne
/// laisse pas la bibliothèque annoncer un chapitre que le lecteur ne peut pas
/// ouvrir.
async fn chapters_presents(destination: &Path) -> Result<Vec<ChapterEntry>, String> {
    let mut noms: Vec<String> = Vec::new();
    let mut entrees = tokio::fs::read_dir(destination)
        .await
        .map_err(|e| format!("lecture du dossier de destination : {e}"))?;
    while let Some(e) = entrees
        .next_entry()
        .await
        .map_err(|e| format!("lecture du dossier de destination : {e}"))?
    {
        let pathi = e.path();
        let nom = pathi
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let est_cbz = pathi.is_file() && nom.to_ascii_lowercase().ends_with(".cbz");
        if !est_cbz || nom.ends_with(".part") {
            continue;
        }
        noms.push(nom);
    }
    // Un dossier peut contenir `Chapitre 2.cbz` et `chapitre 2.CBZ` : sur
    // Windows ils sont **le même fichier**, donc `read_dir` n'en rend qu'un.
    // Rien à dédoublonner — mais le tri, lui, doit être fait ici et pas dans le
    // dossier d'origine, parce que c'est le nom **écrit** qui compte.
    noms.sort_by_key(|n| nom_chapitres(n));
    Ok(noms
        .into_iter()
        .map(|file| ChapterEntry {
            url: String::new(),
            title: sans_extension(&file).to_string(),
            file,
        })
        .collect())
}

/// Clé de tri d'un nom de chapitre : les chiffres sont lus comme des nombres.
///
/// Sans ça `Chapitre 10.cbz` se range **avant** `Chapitre 2.cbz`, et un import
/// d'un chapitre par fichier arrive dans le désordre.
fn nom_chapitres(nom: &str) -> (String, u64, String) {
    let sans = sans_extension(nom).to_lowercase();
    let chiffres: String = sans.chars().filter(|c| c.is_ascii_digit()).collect();
    let reste = sans
        .chars()
        .filter(|c| !c.is_ascii_digit())
        .collect::<String>();
    (reste, chiffres.parse().unwrap_or(u64::MAX), sans)
}

fn sans_extension(nom: &str) -> &str {
    if nom.len() > 4 && nom[nom.len() - 4..].eq_ignore_ascii_case(".cbz") {
        &nom[..nom.len() - 4]
    } else {
        nom
    }
}

/// Supprime le dossier d'un webtoon téléchargé (chapitres + meta.json).
/// Le nom est assaini comme au téléchargement, donc jamais de `..` ni de
/// séparateur : la suppression reste confinée à `%APPDATA%\Tatami\downloads`.
#[tauri::command]
pub async fn remove_download(source: String, title: String) -> Result<usize, String> {
    let root = super::download::downloads_root();
    let dir = manga_dir(&root, &source, &title);
    let canonical_root = root
        .canonicalize()
        .map_err(|e| format!("racine introuvable ({}): {e}", root.display()))?;
    let canonical_dir = dir
        .canonicalize()
        .map_err(|_| format!("{title} n'est pas téléchargé"))?;
    if !canonical_dir.starts_with(&canonical_root) {
        return Err("refus : hors du dossier de téléchargements".to_string());
    }
    let removed = count_chapters(&canonical_dir).await;
    tokio::fs::remove_dir_all(&canonical_dir)
        .await
        .map_err(|e| format!("suppression: {e}"))?;
    // Un dossier de source vide ne sert plus à rien.
    if let Some(parent) = canonical_dir.parent() {
        let _ = tokio::fs::remove_dir(parent).await;
    }
    Ok(removed)
}

async fn count_chapters(dir: &Path) -> usize {
    let Ok(mut files) = tokio::fs::read_dir(dir).await else {
        return 0;
    };
    let mut n = 0;
    while let Ok(Some(file)) = files.next_entry().await {
        let name = file.file_name().to_string_lossy().to_string();
        // Un .part = téléchargement interrompu : pas un chapitre acquis.
        if name != META && !name.ends_with(".part") {
            n += 1;
        }
    }
    n
}

/// Ajoute un chapitre au `meta.json` du webtoon (idempotent par url).
#[allow(clippy::too_many_arguments)]
pub async fn note_chapter(
    dir: &Path,
    source: &str,
    title: &str,
    url: &str,
    cover_url: Option<&str>,
    total_chapters: usize,
    chapter_url: &str,
    chapter_title: &str,
    file: &str,
) -> Result<(), String> {
    let path = dir.join(META);
    let mut entry: MangaEntry = match tokio::fs::read_to_string(&path).await {
        Ok(raw) => serde_json::from_str(&raw).unwrap_or_default(),
        Err(_) => MangaEntry::default(),
    };
    if entry.version == 0 {
        entry.version = META_VERSION;
    }
    if entry.title.is_empty() {
        entry.title = title.to_string();
    }
    if entry.source.is_empty() {
        entry.source = source.to_string();
    }
    if entry.url.is_empty() {
        entry.url = url.to_string();
    }
    if cover_url.is_some() && entry.cover_url.is_none() {
        entry.cover_url = cover_url.map(str::to_string);
    }
    if total_chapters > entry.total_chapters {
        entry.total_chapters = total_chapters;
    }
    let known = entry
        .chapters
        .iter()
        .any(|c| c.url == chapter_url || c.file == file);
    if !known {
        entry.chapters.push(ChapterEntry {
            url: chapter_url.to_string(),
            title: chapter_title.to_string(),
            file: file.to_string(),
        });
    }
    entry.updated_at = now_secs();
    let json = serde_json::to_string_pretty(&entry).map_err(|e| e.to_string())?;
    tokio::fs::write(&path, json)
        .await
        .map_err(|e| format!("meta.json: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Fixture **dans la vraie racine des téléchargements** : c'est la seule
    /// racine que `list_library` accepte, donc un test qui verifiableait le
    /// confinement avec une racine bidon ne prouverait rien.
    fn temp_dir(tag: &str) -> crate::download::DossierDeTest {
        crate::download::DossierDeTest::new(tag)
    }

    /// Cache jetable pour les tests : seule la couverture l'utilise, et un
    /// cache réel ici laisserait des fichiers derrière lui.
    ///
    /// Le dossier est **unique et effacé**, comme toute fixture. Le nom fixe
    /// `tatami-cache-test` posait deux problèmes, et un seul était visible :
    /// il ne disparaissait jamais (mesuré le 2026-10-06, il traînait dans la
    /// bibliothèque), et surtout **deux tests parallèles se partageaient le
    /// même dossier** — donc le jeton écrit par l'un était lisible par l'autre.
    /// Ce couplage invisible peut masquer un défaut de `list_library`, qui ne
    /// doit lire que les jetons de ses propres chapitres.
    ///
    /// La garde est retournée pour rester vivante : liée au cache, pas à une
    /// variable du test.
    fn cache_de_test() -> (
        std::sync::Arc<crate::pagecache::PageCache>,
        crate::download::DossierDeTest,
    ) {
        let garde = crate::download::DossierDeTest::new("cache");
        (crate::pagecache::PageCache::new(garde.chemin()), garde)
    }

    /// Un chemin **réellement existant** hors des téléchargements : sans
    /// cela, un test « le chemin est refusé » passerait aussi quand le chemin
    /// n'existe pas, et ne prouverait donc rien du confinement.
    #[tokio::test]
    async fn list_library_refuse_un_chemin_hors_des_telechargements() {
        let hors = std::env::temp_dir()
            .canonicalize()
            .expect("dossier temporaire");
        assert!(
            hors.is_dir(),
            "le dossier temporaire doit exister pour que le test soit probant"
        );
        let (cache, _garde_cache) = cache_de_test();
        let err = lister(&hors.to_string_lossy(), &cache)
            .await
            .expect_err("un chemin hors des téléchargements doit être refusé");
        assert!(
            err.contains("hors du dossier de téléchargements"),
            "message inattendu : {err}"
        );
        // La racine par défaut reste utilisable : chaîne vide = téléchargements.
        assert!(
            lister("", &cache).await.is_ok(),
            "la racine par défaut doit rester fonctionnelle"
        );
    }

    #[tokio::test]
    async fn note_chapter_cree_le_meta_et_ignore_les_doublons() {
        let garde = temp_dir("note");
        let root = garde.chemin();
        // Organisation réelle : <downloads>/<source>/<webtoon>/
        let dir = root.join("MANGADEX").join("Test");
        std::fs::create_dir_all(&dir).unwrap();
        note_chapter(
            &dir,
            "MANGADEX",
            "Test",
            "manga/1",
            Some("http://c.jpg"),
            12,
            "ch/1",
            "Chapitre 1",
            "Chapitre 1.cbz",
        )
        .await
        .unwrap();
        // Rappel : ne doit pas dupliquer.
        note_chapter(
            &dir,
            "MANGADEX",
            "Test",
            "manga/1",
            Some("http://c.jpg"),
            12,
            "ch/1",
            "Chapitre 1",
            "Chapitre 1.cbz",
        )
        .await
        .unwrap();

        let (cache, _garde_cache) = cache_de_test();
        let items = lister(&root.to_string_lossy(), &cache).await.unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "Test");
        assert_eq!(items[0].source, "MANGADEX");
        assert_eq!(items[0].downloaded, 1);
        assert_eq!(items[0].total_chapters, 12);
        assert!(!items[0].complete);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[tokio::test]
    async fn meta_absent_reconstruit_letat_depuis_les_fichiers() {
        let garde = temp_dir("rebuild");
        let dir = garde.chemin();
        let manga = dir.join("MANGABUFF").join("Sans Meta");
        std::fs::create_dir_all(&manga).unwrap();
        std::fs::write(manga.join("Chapitre 1.cbz"), b"x").unwrap();
        std::fs::write(manga.join("Chapitre 2.cbz"), b"x").unwrap();

        let (cache, _garde_cache) = cache_de_test();
        let items = lister(&dir.to_string_lossy(), &cache).await.unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "Sans Meta");
        assert_eq!(items[0].source, "MANGABUFF");
        assert_eq!(items[0].downloaded, 2);
        assert_eq!(items[0].total_chapters, 2);
        assert!(items[0].complete);
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ---------------------------------------------------------------------
    // Import d'un dossier de webtoon (2.4)
    // ---------------------------------------------------------------------

    /// Un `.cbz` **réellement lisible**, avec le nombre d'images demandé.
    ///
    /// Écrire `b"x"` dans un fichier nommé `.cbz` ne prouverait rien :
    /// `pages_de` le refuserait, donc l'import se comporterait comme face à un
    /// dossier de fichiers corrompus et le cas nominal ne serait jamais
    /// atteint. Un vrai `.cbz` écrit par notre propre écrivain, relu par le
    /// lecteur, c'est le seul fixture honnête ici.
    fn cbz_lisible(path: &Path, images: usize) {
        let mut w = crate::cbz::CbzWriter::create(path).expect("écrivain cbz");
        for i in 0..images {
            // Octets distincts par page : une suite identique passerait aussi
            // bien un test « 11 entrées, 11 tailles distinctes », donc on ne
            // prend pas le risque d'un fichier dont toutes les pages sont
            // la même image.
            w.add_image_kind(
                i + 1,
                crate::image::ImageKind::Jpeg,
                &vec![(i % 251) as u8 + 1; 256 + i * 8],
            )
            .expect("ajout d'image");
        }
        w.sync_and_finish().expect("finalisation");
    }

    /// Dossier source d'un import, avec `noms` fichiers `.cbz` lisibles.
    /// La garde est **retournée** avec le chemin, et c'est pas un détail.
    ///
    /// Mesuré le 2026-10-06 : la première version renvoyait seulement le
    /// `PathBuf`, donc la garde tombait **à la fin de cette fonction** — le
    /// dossier était effacé avant même le premier `assert`. Symptôme : 14 tests
    /// en échec sur `écrivain cbz: NotFound`, alors que le code de production
    /// n'avait pas bougé. Corollaire : un garde-fou qui nettoie doit être
    /// **transporté** jusqu'à la fin du test, sinon il nettoie trop tôt ; d'où
    /// le type `(PathBuf, DossierDeTest)` plutôt que `PathBuf`.
    fn dossier_source(noms: &[&str]) -> (PathBuf, crate::download::DossierDeTest) {
        let garde = temp_dir("import-src");
        let dir = garde.chemin();
        for n in noms {
            cbz_lisible(&dir.join(n), 2);
        }
        (dir, garde)
    }

    /// Titre d'import **unique par test et effacé à la fin**, comme `DossierDeTest`.
    ///
    /// `titre_unique` seul ne suffisait pas : un `nettoyer(&titre)` écrit en
    /// fin de fonction est sauté par le `panic!` qui précède, et il est
    /// **facile de l'oublier** — mesuré le 2026-10-06, un seul des treize tests
    /// d'import appelait `nettoyer`, et `IMPORT/` contenait 48 sous-dossiers
    /// `tatami-import-*` dans la bibliothèque de la personne. Rendre l'effacement
    /// automatique rend l'oubli impossible.
    ///
    /// Le dossier est **retourné** pour que la garde reste vivante : écrire
    /// `DossierDeTest::pour_import(…).titre()` seul ferait tomber la garde à la
    /// fin de l'instruction, et l'import recréerait le dossier juste après.
    fn import_de_test(tag: &str) -> (String, crate::download::DossierDeTest) {
        let garde = crate::download::DossierDeTest::pour_import(tag);
        let titre = garde
            .chemin()
            .file_name()
            .expect("nom de dossier")
            .to_string_lossy()
            .to_string();
        (titre, garde)
    }

    fn nettoyer(titre: &str) {
        let _ = std::fs::remove_dir_all(
            crate::download::downloads_root()
                .join(SOURCE_IMPORT)
                .join(crate::download::sanitize(titre)),
        );
    }

    /// Le cas nominal, avec un contrôle négatif sur ce qui **n'a pas** dû
    /// être importé : `.txt`, sous-dossier, et `meta.json` d'origine.
    #[tokio::test]
    async fn un_import_copie_les_cbz_et_nimporte_que_ca() {
        let (titre, _garde_import) = import_de_test("nominal");
        let (src, _garde_src) = dossier_source(&["Chapitre 1.cbz", "Chapitre 2.cbz"]);
        // Du bruit dans le dossier : un seul import « naïf » le remonterait
        // comme un chapitre.
        std::fs::write(src.join("notes.txt"), b"rien a voir").unwrap();
        std::fs::write(src.join("meta.json"), b"{\"title\":\"faux\"}").unwrap();
        std::fs::create_dir_all(src.join("Chapitre 3.cbz")).unwrap();

        let rapport = import_webtoon(src.to_string_lossy().to_string(), Some(titre.clone()))
            .await
            .expect("import");

        assert_eq!(rapport.importes, 2, "seuls les .cbz sont importés");
        assert_eq!(
            rapport.refuses.len(),
            3,
            "le .txt, le meta.json et le sous-dossier doivent être refusés : {:?}",
            rapport.refuses
        );
        let refuses: Vec<&str> = rapport.refuses.iter().map(|(n, _)| n.as_str()).collect();
        assert!(refuses.contains(&"notes.txt"));
        assert!(refuses.contains(&"meta.json"));
        assert!(
            refuses.contains(&"Chapitre 3.cbz"),
            "un sous-dossier nommé .cbz doit être refusé : {refuses:?}"
        );

        // L'index est la **seule** preuve que le résultat est lisible : on
        // relit par la vraie commande de bibliothèque, pas par le rapport que
        // l'import vient de remplir.
        let (cache, _garde_cache) = cache_de_test();
        let items = lister(&crate::download::downloads_root().to_string_lossy(), &cache)
            .await
            .unwrap();
        let importe = items
            .iter()
            .find(|i| i.title == titre)
            .expect("webtoon listé");
        assert_eq!(importe.source, SOURCE_IMPORT, "source synthétique");
        assert_eq!(importe.downloaded, 2);
        assert_eq!(importe.total_chapters, 2);
        assert!(importe.complete, "2 chapitres sur 2 : complet");
        assert!(
            importe.url.is_empty(),
            "une URL fabriquerait « retélécharger » vers un site inexistant"
        );
        assert_eq!(
            importe
                .chapters
                .iter()
                .map(|c| c.title.as_str())
                .collect::<Vec<_>>(),
            vec!["Chapitre 1", "Chapitre 2"],
            "ordre de lecture"
        );
        assert!(importe.chapters.iter().all(|c| c.url.is_empty()));

        nettoyer(&titre);
        let _ = std::fs::remove_dir_all(&src);
    }

    /// Un chapitre illisible est refusé **et** absent du disque de
    /// destination. Un test « le rapport contient un refus » ne prouverait rien
    /// si le fichier avait été copié malgré tout : c'est ce second contrôle qui
    /// compte, parce qu'un chapitre illisible s'affiche bien dans la liste.
    #[tokio::test]
    async fn un_cbz_illisible_est_refuse_et_non_copie() {
        let (titre, _garde_import) = import_de_test("corrompu");
        let (src, _garde_src) = dossier_source(&["Chapitre 1.cbz"]);
        // Un fichier nommé `.cbz` qui n'est pas un ZIP : c'est exactement ce
        // qu'un téléchargement interrompu ou une page HTML renommée laisse.
        std::fs::write(src.join("Chapitre 9.cbz"), b"<html>challenge</html>").unwrap();

        let rapport = import_webtoon(src.to_string_lossy().to_string(), Some(titre.clone()))
            .await
            .expect("import partiel");
        assert_eq!(rapport.importes, 1, "le chapitre valide passe");
        assert_eq!(rapport.refuses.len(), 1);
        assert_eq!(rapport.refuses[0].0, "Chapitre 9.cbz");
        assert_eq!(
            rapport.refuses[0].1.code,
            RefusCode::ArchiveIllisible,
            "le code doit être stable : l'interface le traduit, donc une \
             phrase ici produirait un écran à moitié français"
        );
        // Le détail technique est conservé : c'est lui qui dit *pourquoi* la
        // couche ZIP a refusé, et il reste dans la langue du moteur.
        let detail = rapport.refuses[0].1.detail.as_deref().unwrap_or("");
        assert!(
            detail.contains("ZIP"),
            "le message de la couche ZIP doit être conservé, obtenu : {detail:?}"
        );

        let dest = crate::download::downloads_root()
            .join(SOURCE_IMPORT)
            .join(crate::download::sanitize(&titre));
        assert!(
            !dest.join("Chapitre 9.cbz").exists(),
            "le fichier illisible ne doit pas se retrouver dans la bibliothèque"
        );
        assert!(dest.join("Chapitre 1.cbz").is_file());

        nettoyer(&titre);
        let _ = std::fs::remove_dir_all(&src);
    }

    /// Un ZIP **lisible mais sans aucune image** est le cas que `pages_de`
    /// renvoie vide : il ne lève pas d'erreur, donc un import qui ne testerait
    /// que le cas d'erreur le laisserait dans la bibliothèque.
    ///
    /// Le fixture ne peut pas être un ZIP vide : `lire_entrees` rejette
    /// explicitement une archive **sans entrée** (`PasUnZip`, mesuré). Il faut
    /// donc une archive dont l'unique entrée n'est pas une image — c'est
    /// exactement ce qu'est une archive faite par un compresseur qui a rangé un
    /// `readme.txt` à côté des pages.
    fn zip_sans_image(path: &Path) {
        let nom = b"lisezmoi.txt";
        let donnees: &[u8] = b"ceci n'est pas une image";
        let crc = crc32(donnees);
        let taille = donnees.len() as u32;
        let mut z: Vec<u8> = Vec::new();
        // En-tête local (30 + nom), données, répertoire central (46 + nom), EOCD.
        z.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        z.extend_from_slice(&20u16.to_le_bytes()); // version requise
        z.extend_from_slice(&0u16.to_le_bytes()); // drapeaux
        z.extend_from_slice(&0u16.to_le_bytes()); // méthode : Stored
        z.extend_from_slice(&0u16.to_le_bytes()); // heure
        z.extend_from_slice(&0u16.to_le_bytes()); // date
        z.extend_from_slice(&crc.to_le_bytes());
        z.extend_from_slice(&taille.to_le_bytes());
        z.extend_from_slice(&taille.to_le_bytes());
        z.extend_from_slice(&(nom.len() as u16).to_le_bytes());
        z.extend_from_slice(&0u16.to_le_bytes()); // extra
        z.extend_from_slice(nom);
        z.extend_from_slice(donnees);

        let debut_local = 0u32;
        let debut_central = z.len() as u32;
        z.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
        z.extend_from_slice(&20u16.to_le_bytes()); // version auteur
        z.extend_from_slice(&20u16.to_le_bytes()); // version requise
        z.extend_from_slice(&0u16.to_le_bytes()); // drapeaux
        z.extend_from_slice(&0u16.to_le_bytes()); // méthode
        z.extend_from_slice(&0u16.to_le_bytes()); // heure
        z.extend_from_slice(&0u16.to_le_bytes()); // date
        z.extend_from_slice(&crc.to_le_bytes());
        z.extend_from_slice(&taille.to_le_bytes());
        z.extend_from_slice(&taille.to_le_bytes());
        z.extend_from_slice(&(nom.len() as u16).to_le_bytes());
        z.extend_from_slice(&0u16.to_le_bytes()); // extra
        z.extend_from_slice(&0u16.to_le_bytes()); // commentaire
        z.extend_from_slice(&0u16.to_le_bytes()); // disque
        z.extend_from_slice(&0u16.to_le_bytes()); // attributs internes
        z.extend_from_slice(&0u32.to_le_bytes()); // attributs externes
        z.extend_from_slice(&debut_local.to_le_bytes());
        z.extend_from_slice(nom);

        z.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
        z.extend_from_slice(&0u16.to_le_bytes()); // disque
        z.extend_from_slice(&0u16.to_le_bytes()); // disque du répertoire
        z.extend_from_slice(&1u16.to_le_bytes()); // entrées sur ce disque
        z.extend_from_slice(&1u16.to_le_bytes()); // entrées au total
        z.extend_from_slice(&((z.len() - debut_central as usize) as u32).to_le_bytes());
        z.extend_from_slice(&debut_central.to_le_bytes());
        z.extend_from_slice(&0u16.to_le_bytes()); // commentaire
        std::fs::write(path, z).expect("écriture du zip");
    }

    fn crc32(donnees: &[u8]) -> u32 {
        let mut table = [0u32; 256];
        for (i, e) in table.iter_mut().enumerate() {
            let mut c = i as u32;
            for _ in 0..8 {
                c = if c & 1 != 0 {
                    0xEDB8_8320 ^ (c >> 1)
                } else {
                    c >> 1
                };
            }
            *e = c;
        }
        let mut c = 0xFFFF_FFFFu32;
        for b in donnees {
            c = table[((c ^ *b as u32) & 0xFF) as usize] ^ (c >> 8);
        }
        c ^ 0xFFFF_FFFF
    }

    #[tokio::test]
    async fn un_cbz_sans_image_est_refuse() {
        let (titre, _garde_import) = import_de_test("sans-image");
        let (src, _garde_src) = dossier_source(&["Chapitre 1.cbz"]);
        let vide = src.join("Chapitre 7.cbz");
        zip_sans_image(&vide);
        // Le contrôle qui prouve que le fixture teste bien le cas voulu : si
        // l'archive n'était pas lisible, ce serait le test du fichier
        // corrompu qui passerait, et le cas « archive vide d'images » resterait
        // jamais couvert.
        assert_eq!(
            crate::zipread::lire_entrees(&vide)
                .map(|e| e.len())
                .expect("archive illisible"),
            1,
            "fixture : l'archive doit être lisible et contenir une entrée"
        );
        assert!(
            crate::zipread::pages_de(&vide)
                .expect("zip lisible")
                .is_empty(),
            "fixture invalide : le ZIP doit être lisible et vide d'images"
        );

        let rapport = import_webtoon(src.to_string_lossy().to_string(), Some(titre.clone()))
            .await
            .expect("import partiel");
        assert_eq!(rapport.importes, 1);
        assert_eq!(rapport.refuses.len(), 1);
        assert_eq!(rapport.refuses[0].0, "Chapitre 7.cbz");
        assert_eq!(rapport.refuses[0].1.code, RefusCode::AucuneImage);
        assert!(
            rapport.refuses[0].1.detail.is_none(),
            "ce refus n'a pas de détail technique"
        );

        nettoyer(&titre);
        let _ = std::fs::remove_dir_all(&src);
    }

    /// Réimporter un dossier **plus petit** ne doit pas faire disparaître les
    /// chapitres d'un import précédent : ils sont toujours sur le disque, donc
    /// les annoncer absents serait un mensonge de la bibliothèque.
    #[tokio::test]
    async fn reimporter_un_dossier_plus_petit_ne_perd_pas_les_chapitres() {
        let (titre, _garde_import) = import_de_test("reimport");
        let (grand, _garde_src) =
            dossier_source(&["Chapitre 1.cbz", "Chapitre 2.cbz", "Chapitre 3.cbz"]);
        import_webtoon(grand.to_string_lossy().to_string(), Some(titre.clone()))
            .await
            .expect("premier import");

        let (petit, _garde_src) = dossier_source(&["Chapitre 1.cbz"]);
        let rapport = import_webtoon(petit.to_string_lossy().to_string(), Some(titre.clone()))
            .await
            .expect("second import");
        assert_eq!(rapport.importes, 1, "un seul fichier fourni");

        let (cache, _garde_cache) = cache_de_test();
        let items = lister(&crate::download::downloads_root().to_string_lossy(), &cache)
            .await
            .unwrap();
        let importe = items
            .iter()
            .find(|i| i.title == titre)
            .expect("webtoon listé");
        assert_eq!(
            importe.downloaded, 3,
            "les trois chapitres sont toujours sur le disque"
        );
        assert_eq!(importe.chapters.len(), 3);
        assert!(
            importe.chapters.iter().any(|c| c.file == "Chapitre 3.cbz"),
            "Chapitre 3 doit être encore listé"
        );

        nettoyer(&titre);
        let _ = std::fs::remove_dir_all(&grand);
        let _ = std::fs::remove_dir_all(&petit);
    }

    /// Un second import **sans rien d'importable** ne doit pas effacer ce qu'un
    /// premier import avait écrit.
    ///
    /// Le défaut mesuré le 2026-10-08 : `import_webtoon` faisait
    /// `remove_dir_all(destination)` dès que `rapport.importes == 0`, sans savoir
    /// si le dossier existait avant. Or la destination est un nom **assaini** :
    /// `sanitize` transforme `:` et `?` en `_` et plafonne à 80 caractères, donc
    /// `Foo: Bar` et `Foo? Bar` retombent sur le même dossier. Un import réussi,
    /// puis un second import du même dossier dont les archives sont illisibles
    /// (copie encore en cours) : les chapitres déjà importés étaient effacés, et
    /// la commande renvoyait une erreur — donc l'utilisateur perdait sa
    /// bibliothèque sans invite.
    ///
    /// Le test échoue sur l'ancien code, et échoue **sur le disque** : il relit
    /// les `.cbz` présents plutôt que de se fier au code de retour, qui est un
    /// `Ok`/`Err` et ne dit rien de l'effacement.
    #[tokio::test]
    async fn un_import_sans_rien_importable_n_efface_pas_le_precedent() {
        let (titre, _garde_import) = import_de_test("precedent");

        // 1. Un import réussi, de trois chapitres.
        let (premier, _garde_src) =
            dossier_source(&["Chapitre 1.cbz", "Chapitre 2.cbz", "Chapitre 3.cbz"]);
        let rapport = import_webtoon(premier.to_string_lossy().to_string(), Some(titre.clone()))
            .await
            .expect("premier import");
        assert_eq!(rapport.importes, 3);

        let destination = crate::download::downloads_root()
            .join(SOURCE_IMPORT)
            .join(crate::download::sanitize(&titre));
        assert_eq!(
            std::fs::read_dir(&destination).unwrap().count(),
            4,
            "3 chapitres + meta.json"
        );

        // 2. Un second import du MÊME titre, dont le seul `.cbz` est illisible.
        //    Il passe le filtre d'extension puis échoue à la lecture : c'est ce
        //    chemin qui rend `importes == 0` **après** la création du dossier.
        let (second, _garde_src2) = dossier_source(&["Chapitre 4.cbz"]);
        std::fs::write(
            second.join("Chapitre 4.cbz"),
            b"ceci n'est pas une archive zip".as_slice(),
        )
        .expect("archive factice");
        let echec = import_webtoon(second.to_string_lossy().to_string(), Some(titre.clone())).await;
        assert!(
            echec.is_err(),
            "aucun chapitre importable : la commande doit échouer"
        );

        // 3. Le contrôle décisif : les trois chapitres sont **encore là**.
        let presents: Vec<String> = std::fs::read_dir(&destination)
            .unwrap()
            .filter_map(|e| e.ok())
            .map(|e| e.file_name().to_string_lossy().to_string())
            .collect();
        assert_eq!(
            presents.len(),
            4,
            "le dossier d'import doit être intact : trouvé {presents:?}"
        );
        for attendu in ["Chapitre 1.cbz", "Chapitre 2.cbz", "Chapitre 3.cbz"] {
            assert!(
                presents.iter().any(|p| p == attendu),
                "{attendu} a disparu de {destination:?} : trouvé {presents:?}"
            );
        }
    }

    /// Un réimport ne doit pas **réécrire** les fichiers déjà là : c'est la
    /// garantie « ne jamais retélécharger ce qui est là », appliquée à
    /// l'import. Contrôlé par la **date de modification**, seule preuve que le
    /// fichier n'a pas été touché — la taille serait identique à l'octet près.
    #[tokio::test]
    async fn reimporter_ne_reecrit_pas_un_chapitre_deja_present() {
        let (titre, _garde_import) = import_de_test("idempotent");
        let (src, _garde_src) = dossier_source(&["Chapitre 1.cbz"]);
        import_webtoon(src.to_string_lossy().to_string(), Some(titre.clone()))
            .await
            .expect("premier import");

        let dest = crate::download::downloads_root()
            .join(SOURCE_IMPORT)
            .join(crate::download::sanitize(&titre));
        let cible = dest.join("Chapitre 1.cbz");
        let avant = std::fs::metadata(&cible).unwrap().modified().unwrap();

        std::thread::sleep(std::time::Duration::from_millis(1100));
        let rapport = import_webtoon(src.to_string_lossy().to_string(), Some(titre.clone()))
            .await
            .expect("second import");
        assert_eq!(rapport.importes, 1, "le chapitre est compté comme présent");

        let apres = std::fs::metadata(&cible).unwrap().modified().unwrap();
        assert_eq!(
            avant, apres,
            "le fichier a été réécrit : le réimport coûte 43 Mo pour rien"
        );

        let (cache, _garde_cache) = cache_de_test();
        let items = lister(&crate::download::downloads_root().to_string_lossy(), &cache)
            .await
            .unwrap();
        let importe = items
            .iter()
            .find(|i| i.title == titre)
            .expect("webtoon listé");
        assert_eq!(importe.downloaded, 1, "pas de doublon après réimport");

        nettoyer(&titre);
        let _ = std::fs::remove_dir_all(&src);
    }

    /// Un dossier d'import vide doit **échouer**, et surtout ne pas laisser de
    /// dossier dans la bibliothèque : un webtoon à zéro chapitre s'afficherait
    /// dans la liste et proposerait un lecteur vide.
    #[tokio::test]
    async fn un_dossier_sans_cbz_echoue_sans_laisser_de_dossier() {
        let (titre, _garde_import) = import_de_test("vide");
        let garde = temp_dir("import-vide");
        let src = garde.chemin();
        std::fs::write(src.join("lisezmoi.txt"), b"rien").unwrap();

        let err = import_webtoon(src.to_string_lossy().to_string(), Some(titre.clone()))
            .await
            .expect_err("un dossier sans .cbz doit échouer");
        assert!(
            err.contains("aucun fichier .cbz"),
            "message inattendu : {err}"
        );

        let dest = crate::download::downloads_root()
            .join(SOURCE_IMPORT)
            .join(crate::download::sanitize(&titre));
        assert!(
            !dest.exists(),
            "un dossier vide ne doit pas rester dans la bibliothèque"
        );
        let _ = std::fs::remove_dir_all(&src);
    }

    /// Un dossier dont **tous** les `.cbz` sont illisibles échoue aussi, et
    /// là encore sans laisser de dossier — sinon la bibliothèque afficherait un
    /// import « réussi » et vide.
    #[tokio::test]
    async fn un_dossier_entierement_illisible_echoue_et_nettoie() {
        let (titre, _garde_import) = import_de_test("tout-illisible");
        let garde = temp_dir("import-pDK");
        let src = garde.chemin();
        std::fs::write(src.join("a.cbz"), b"pas un zip").unwrap();
        std::fs::write(src.join("b.cbz"), b"pas un zip non plus").unwrap();

        let err = import_webtoon(src.to_string_lossy().to_string(), Some(titre.clone()))
            .await
            .expect_err("rien d'importable");
        assert!(
            err.contains("aucun chapitre importable"),
            "message inattendu : {err}"
        );
        assert!(
            err.contains("2"),
            "le compte de refus doit être annoncé, sinon l'utilisateur suppose \
             que le dossier était vide : {err}"
        );
        let dest = crate::download::downloads_root()
            .join(SOURCE_IMPORT)
            .join(crate::download::sanitize(&titre));
        assert!(!dest.exists(), "dossier résiduel : {}", dest.display());
        let _ = std::fs::remove_dir_all(&src);
    }

    /// Un chemin qui n'est pas un dossier : le message doit le dire, sinon
    /// l'utilisateur cherche pourquoi « aucun .cbz » alors qu'il a bien
    /// sélectionné un fichier.
    #[tokio::test]
    async fn un_chemin_qui_nest_pas_un_dossier_est_refuse_clairement() {
        let garde = temp_dir("import-fichier");
        let src = garde.chemin();
        let fichier = src.join("un.cbz");
        cbz_lisible(&fichier, 1);

        let err = import_webtoon(fichier.to_string_lossy().to_string(), None)
            .await
            .expect_err("un fichier n'est pas un dossier");
        assert!(
            err.contains("n'est pas un dossier"),
            "message inattendu : {err}"
        );
        let _ = std::fs::remove_dir_all(&src);
    }

    /// Un titre hostile ne doit **jamais** produire un chemin hors du dossier de
    /// téléchargements : c'est le seul vecteur d'écriture可控 ici, donc il
    /// mérite un test dédié et non une assertion greffée plus haut.
    #[tokio::test]
    async fn un_titre_hostile_ne_sort_pas_du_dossier_de_telechargements() {
        let (titre, _garde_import) = import_de_test("hostile");
        let (src, _garde_src) = dossier_source(&["Chapitre 1.cbz"]);
        let toxique = format!("{titre}/../../../evil");

        let rapport = import_webtoon(src.to_string_lossy().to_string(), Some(toxique.clone()))
            .await
            .expect("import");
        assert_eq!(rapport.importes, 1);

        let dest = PathBuf::from(&rapport.dir);
        let racine = crate::download::downloads_root();
        assert!(
            dest.starts_with(&racine),
            "écriture hors du dossier de téléchargements : {}",
            dest.display()
        );
        assert!(
            !racine.parent().unwrap().join("evil").exists(),
            "le traversée de répertoire a créé un dossier hors de la bibliothèque"
        );
        assert!(
            !racine
                .parent()
                .unwrap()
                .parent()
                .unwrap()
                .join("evil")
                .exists(),
            "idempotent, mais vérifié aussi au niveau parent"
        );

        nettoyer(&titre);
        // Le titre **sanitisé** est `tatami-import-hostile-…_.._.._.._evil` :
        // `sanitize` remplace `/` par `_`, donc le dossier réellement écrit n'a
        // pas le nom que couvre le garde (qui connaît `{titre}`, pas le titre
        // transformé). Sans cet effacement, chaque `cargo test` laissait un
        // dossier derrière lui — mesuré le 2026-10-06.
        nettoyer(&crate::download::sanitize(&toxique));
        let _ = std::fs::remove_dir_all(&src);
    }

    /// Un nom de fichier `.cbz` que Windows **accepte** mais que `sanitize`
    /// doit corriger doit être écrit sous sa forme assainie, et le `meta.json`
    /// doit enregistrer le nom **écrit** — sinon le chapitre est listé sous un
    /// nom introuvable et le lecteur ne peut pas l'ouvrir.
    ///
    /// Les noms choisis sont hostiles **créables**. Deux catégories sont
    /// hors d'atteinte ici, ce que la fixture elle-même documente :
    /// `< > : " / \ | ? *` et `CON.cbz` sont **refusés par Windows** (mesuré :
    /// création impossible, handle invalide), donc un dossier importé depuis
    /// une clé USB ou un partage réseau est le seul endroit où on les
    /// rencontre. Restent les pièges accessibles : point final, espaces de bord,
    /// et point en tête.
    #[tokio::test]
    async fn un_nom_de_fichier_hostile_est_assaini_et_recorde_sous_son_nom_reel() {
        let (titre, _garde_import) = import_de_test("assaini");
        let (src, _garde_src) = dossier_source(&[]);
        cbz_lisible(&src.join("Chapitre 4..cbz"), 3);
        cbz_lisible(&src.join("  Espaces  .cbz"), 1);
        cbz_lisible(&src.join("...et des points.cbz"), 1);

        let rapport = import_webtoon(src.to_string_lossy().to_string(), Some(titre.clone()))
            .await
            .expect("import");
        assert_eq!(rapport.importes, 3, "les trois noms sont importables");

        let (cache, _garde_cache) = cache_de_test();
        let items = lister(&crate::download::downloads_root().to_string_lossy(), &cache)
            .await
            .unwrap();
        let importe = items
            .iter()
            .find(|i| i.title == titre)
            .expect("webtoon listé");
        let dest = PathBuf::from(&importe.dir);
        let annonces: Vec<&str> = importe.chapters.iter().map(|c| c.file.as_str()).collect();
        assert_eq!(
            importe.chapters.len(),
            3,
            "trois chapitres listés : {annonces:?}"
        );

        // **Chaque** chapitre listé doit être un fichier qui existe, sous le nom
        // exact annoncé, **et** être celui que le lecteur recalcule. C'est
        // l'invariant de bout en bout de l'import.
        for c in &importe.chapters {
            let chemin = dest.join(&c.file);
            assert!(chemin.is_file(), "chapitre listé mais absent : {}", c.file);
            let par_lecteur =
                crate::download::chapter_file_for(SOURCE_IMPORT, &titre, &c.title, "");
            // **Canonisation obligatoire** : `importe.dir` vient d'un
            // `canonicalize` et porte donc le préfixe `\\?\`, alors que le
            // lecteur recalcule un chemin ordinaire. Les deux chaînes sont
            // différentes pour le même fichier — comparer les chaînes brutes
            // échouerait sur un chemin correct.
            let a = chemin.canonicalize().expect("chemin canonique");
            let b = par_lecteur.canonicalize().expect("chemin canonique");
            assert_eq!(
                a,
                b,
                "le lecteur ne retrouve pas « {} » : il cherche {} et le meta \
                 annonce {}",
                c.title,
                par_lecteur.display(),
                chemin.display()
            );
        }
        assert!(
            annonces.contains(&"Chapitre 4.cbz"),
            "point final retiré : {annonces:?}"
        );
        assert!(
            annonces.contains(&"Espaces.cbz"),
            "espaces de bord retirés : {annonces:?}"
        );
        assert!(
            annonces.contains(&"et des points.cbz"),
            "points de tête retirés : {annonces:?}"
        );

        nettoyer(&titre);
        let _ = std::fs::remove_dir_all(&src);
    }

    /// Le tri des chapitres : `Chapitre 10` doit venir **après**
    /// `Chapitre 2`, pas avant. Un import « d'un chapitre par fichier » arrive
    /// dans l'ordre du système de fichiers, qui est arbitraire — sans ce tri,
    /// la lecture avance de 2 à 10 et le lecteur croit à un saut de huit.
    #[tokio::test]
    async fn les_chapitres_sont_tries_par_numero_et_non_par_nom() {
        let (titre, _garde_import) = import_de_test("tri");
        let (src, _garde_src) = dossier_source(&[
            "Chapitre 10.cbz",
            "Chapitre 2.cbz",
            "Chapitre 1.cbz",
            "Chapitre 21.cbz",
        ]);

        let rapport = import_webtoon(src.to_string_lossy().to_string(), Some(titre.clone()))
            .await
            .expect("import");
        assert_eq!(rapport.importes, 4);

        let (cache, _garde_cache) = cache_de_test();
        let items = lister(&crate::download::downloads_root().to_string_lossy(), &cache)
            .await
            .unwrap();
        let importe = items
            .iter()
            .find(|i| i.title == titre)
            .expect("webtoon listé");
        assert_eq!(
            importe
                .chapters
                .iter()
                .map(|c| c.title.as_str())
                .collect::<Vec<_>>(),
            vec!["Chapitre 1", "Chapitre 2", "Chapitre 10", "Chapitre 21"],
            "ordre de lecture : les chiffres doivent être lus comme des nombres"
        );

        nettoyer(&titre);
        let _ = std::fs::remove_dir_all(&src);
    }

    /// Un import doit produire un webtoon **lisible de bout en bout** : la
    /// commande qui sert les pages d'un chapitre téléchargé doit renvoyer les
    /// pages de l'import. C'est le seul test qui ferme la chaîne — les autres
    /// prouvent la copie et le `meta.json`, pas l'accès par le lecteur.
    #[tokio::test]
    async fn les_pages_d_un_chapitre_importe_sont_lisibles_par_le_lecteur() {
        let (titre, _garde_import) = import_de_test("lecteur");
        let (src, _garde_src) = dossier_source(&["Chapitre 1.cbz", "Chapitre 2.cbz"]);
        import_webtoon(src.to_string_lossy().to_string(), Some(titre.clone()))
            .await
            .expect("import");

        // Le lecteur ne devine pas le chemin : il **recalcule** celui du
        // chapitre avec `chapter_file_for`, exactement comme le fait
        // `local_chapter_pages`. C'est donc le calcul qu'il faut éprouver — un
        // import qui écrivait ailleurs donnerait un chapitre listé et
        // introuvable.
        //
        // `local_chapter_pages` elle-même prend un `tauri::State` : elle n'est
        // pas appelable sans application. Le reste de sa chaîne
        // (`pages_de` → `entries_to_pages`) est déjà couvert par les tests de
        // la tâche 2.3.
        for chapitre in ["Chapitre 1", "Chapitre 2"] {
            let fichier = crate::download::chapter_file_for(SOURCE_IMPORT, &titre, chapitre, "");
            assert!(
                fichier.is_file(),
                "le lecteur ne retrouve pas « {chapitre} » : {}",
                fichier.display()
            );
            let pages = crate::zipread::pages_de(&fichier)
                .unwrap_or_else(|e| panic!("{chapitre} illisible : {e}"));
            assert_eq!(pages.len(), 2, "le chapitre importé doit être lisible");
        }

        nettoyer(&titre);
        let _ = std::fs::remove_dir_all(&src);
    }

    /// Deux fichiers dont le nom s'assaine pareil ne peuvent pas coexister : le
    /// second écraserait le premier et un seul chapitre serait annoncé.
    ///
    /// Deux exemples réels du même piège : `sans-titre.cbz` et `   .cbz`
    /// s'assainissent tous deux en `sans-titre`, et `sanitize` plafonne à
    /// 80 caractères, au-delà duquel deux noms distincts se confondent.
    #[tokio::test]
    async fn deux_fichiers_de_meme_nom_assaini_ne_produisent_qu_un_chapitre() {
        let (titre, _garde_import) = import_de_test("collision");
        let (src, _garde_src) = dossier_source(&[]);
        cbz_lisible(&src.join("sans-titre.cbz"), 1);
        cbz_lisible(&src.join("   .cbz"), 2);
        // Et le plafond de longueur : au-delà de 80 caractères, deux noms
        // différents retombent sur le même.
        let prefixe = "A".repeat(80);
        cbz_lisible(&src.join(format!("{prefixe}{}.cbz", "B".repeat(20))), 3);
        cbz_lisible(&src.join(format!("{prefixe}{}.cbz", "C".repeat(20))), 4);

        let rapport = import_webtoon(src.to_string_lossy().to_string(), Some(titre.clone()))
            .await
            .expect("import partiel");
        assert_eq!(rapport.importes, 2, "deux paires, donc deux chapitres");
        assert_eq!(rapport.refuses.len(), 2);
        for (nom, refus) in &rapport.refuses {
            assert_eq!(refus.code, RefusCode::Collision, "refus de {nom}");
            // Le détail porte le nom en conflit : c'est lui qui permet à
            // l'utilisateur de savoir lequel des deux fichiers est gardé.
            assert!(
                refus.detail.as_deref().is_some_and(|d| d.ends_with(".cbz")),
                "le détail doit nommer le fichier en conflit, obtenu {:?}",
                refus.detail
            );
        }

        let (cache, _garde_cache) = cache_de_test();
        let items = lister(&crate::download::downloads_root().to_string_lossy(), &cache)
            .await
            .unwrap();
        let importe = items
            .iter()
            .find(|i| i.title == titre)
            .expect("webtoon listé");
        assert_eq!(importe.chapters.len(), 2, "aucun doublon annoncé");

        nettoyer(&titre);
        let _ = std::fs::remove_dir_all(&src);
    }

    /// L'invariant que l'import tient vraiment : **tout chapitre annoncé est
    /// adressable par le lecteur**, quels que soient les noms de fichiers.
    ///
    /// Testé sur `nom_chapitre_lecteur` directement, parce que c'est là que la
    /// garantie se joue — et parce qu'elle n'est **pas** évidente :
    /// `chapter_file_name` a un chemin de repli (titre vide ou `sans-titre` →
    /// segment d'URL) qui, avec une URL vide, retombe sur `sanitize("")` —
    /// soit `sans-titre.cbz`. Autrement dit un fichier `.cbz` ou `"   .cbz"`
    /// devient `cbz.cbz` / `sans-titre.cbz` **et reste trouvable**.
    ///
    /// La première version de ce test supposait le contraire et attendait un
    /// refus : elle a échoué en montrant que le repli est Totalement cohérent.
    /// Le test vérifie donc ce qui est vrai — l'aller-retour — plutôt que
    /// d'imposer un refus qui n'a pas lieu d'être.
    #[test]
    fn tout_nom_de_chapitre_importe_est_adressable_par_le_lecteur() {
        let noms = [
            "Chapitre 1",
            "Ch. 1",
            "1",
            "sans-titre",
            ".cbz",
            "   ",
            "  Espaces  ",
            "CON",
            "Chapitre 4.",
            "",
            "éèàçùô",
            "日本語",
            "Chapitre/12",
            "NUL",
        ];
        for nom in noms {
            let fichier = format!("{nom}.cbz");
            let Some(cible) = nom_chapitre_lecteur(&fichier) else {
                // Le refus est une issue légitime : le fichier ne sera pas écrit,
                // donc le problème est inexistant. Il faut juste le noter.
                continue;
            };
            assert_eq!(
                cible,
                format!(
                    "{}.cbz",
                    crate::download::sanitize(sans_extension(&fichier))
                ),
                "le nom écrit doit être exactement l'assaini, sans quoi le \
                 lecteur ne le retrouve pas (cas « {nom} »)"
            );
            // Aller-retour par la **vraie** fonction du lecteur.
            let par_lecteur = crate::download::chapter_file_name(sans_extension(&cible), "");
            assert_eq!(
                par_lecteur, cible,
                "aller-retour cassé pour « {nom} » : le lecteur calcule \
                 {par_lecteur}, l'import a écrit {cible}"
            );
        }
    }
}
