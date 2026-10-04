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

use serde::{Deserialize, Serialize};

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
    root.join(super::download::sanitize(source))
        .join(super::download::sanitize(title))
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

/// Liste les webtoons téléchargés (un par dossier), du plus récent au plus ancien.
#[tauri::command]
pub async fn list_library(root: String) -> Result<Vec<LibraryItem>, String> {
    let root = if root.is_empty() {
        super::download::downloads_root()
    } else {
        PathBuf::from(root)
    };
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
        .map(|entry| LibraryItem {
            downloaded: entry.downloaded(),
            complete: entry.complete(),
            title: entry.title,
            source: entry.source,
            url: entry.url,
            cover_url: entry.cover_url,
            dir: entry.dir,
            total_chapters: entry.total_chapters,
            size_bytes: entry.size_bytes,
            updated_at: entry.updated_at,
            chapters: entry.chapters,
        })
        .collect())
}

/// Noms de fichiers déjà présents pour un webtoon ( permet d'ignorer les
/// chapitres déjà téléchargés et de relancer une file interrompue).
#[tauri::command]
pub async fn existing_chapters(source: String, title: String) -> Result<Vec<String>, String> {
    let dir = manga_dir(&super::download::downloads_root(), &source, &title);
    let mut out = Vec::new();
    let Ok(mut files) = tokio::fs::read_dir(&dir).await else {
        return Ok(out);
    };
    while let Ok(Some(file)) = files.next_entry().await {
        let name = file.file_name().to_string_lossy().to_string();
        if name != META {
            out.push(name);
        }
    }
    Ok(out)
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

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("tatami-lib-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("dossier temporaire");
        dir
    }

    #[tokio::test]
    async fn note_chapter_cree_le_meta_et_ignore_les_doublons() {
        let root = temp_dir("note");
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

        let items = list_library(root.to_string_lossy().to_string())
            .await
            .unwrap();
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
        let dir = temp_dir("rebuild");
        let manga = dir.join("MANGABUFF").join("Sans Meta");
        std::fs::create_dir_all(&manga).unwrap();
        std::fs::write(manga.join("Chapitre 1.cbz"), b"x").unwrap();
        std::fs::write(manga.join("Chapitre 2.cbz"), b"x").unwrap();

        let items = list_library(dir.to_string_lossy().to_string())
            .await
            .unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title, "Sans Meta");
        assert_eq!(items[0].source, "MANGABUFF");
        assert_eq!(items[0].downloaded, 2);
        assert_eq!(items[0].total_chapters, 2);
        assert!(items[0].complete);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
