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
use tokio::sync::Mutex;

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

/// Un seul téléchargement à la fois : le moteur est séquentiel (cf. Server.kt).
#[derive(Default)]
pub struct DownloadState {
    running: Mutex<bool>,
}

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
        "sans-titre".to_string()
    } else {
        trimmed.chars().take(80).collect()
    }
}

#[allow(dead_code)]
fn extension_for(content_type: &str) -> &'static str {
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

pub fn downloads_root() -> PathBuf {
    std::env::var("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("Tatami")
        .join("downloads")
}

/// Un chapitre = un `.cbz` directement dans le dossier du webtoon :
/// `downloads/<source>/<webtoon>/<titre-chapitre>.cbz` (pas de sous-dossier par
/// chapitre : c'est plus lisible dans l'explorateur).
fn chapter_dir(root: &Path, source: &str, manga_title: &str) -> PathBuf {
    root.join(sanitize(source)).join(sanitize(manga_title))
}

fn chapter_file_name(chapter_title: &str, chapter_url: &str) -> String {
    let base = sanitize(chapter_title);
    if base.is_empty() || base == "sans-titre" {
        let from_url = chapter_url
            .rsplit('/')
            .next()
            .map(sanitize)
            .unwrap_or_else(|| "chapitre".to_string());
        format!("{from_url}.cbz")
    } else {
        format!("{base}.cbz")
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
    {
        let mut running = state.running.lock().await;
        if *running {
            return Err("un téléchargement est déjà en cours".to_string());
        }
        *running = true;
    }
    let result = run_download(
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
    .await;
    *state.running.lock().await = false;
    result
}

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
    tokio::fs::create_dir_all(&dir)
        .await
        .map_err(|e| format!("création dossier: {e}"))?;

    // 1. chapitres -> details (le cache moteur a le manga de la source)
    let details = sup
        .request(
            "details",
            serde_json::json!({ "source": source, "url": manga_url }),
            FETCH_TIMEOUT,
        )
        .await?;
    let _ = details; // le moteur complete le cache manga/chapitres

    // 2. pages du chapitre
    let pages = sup
        .request(
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

    let mut cbz = CbzWriter::new();
    for (i, page) in page_list.iter().enumerate() {
        let page_url = page
            .get("url")
            .and_then(|v| v.as_str())
            .ok_or("page sans url")?;
        let progress = Progress {
            source: source.to_string(),
            chapter: chapter_title.to_string(),
            page: i + 1,
            total,
            done: false,
            error: None,
        };
        let _ = app.emit("download://progress", &progress);

        let fetched = sup
            .request(
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
                        source: source.to_string(),
                        chapter: chapter_title.to_string(),
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
        cbz.add_image(i + 1, content_type, &bytes);
    }

    let written = total;
    let file = dir.join(chapter_file_name(chapter_title, chapter_url));
    let data = cbz.finish();
    tokio::fs::write(&file, &data)
        .await
        .map_err(|e| format!("écriture {}: {e}", file.display()))?;

    // État d'avancement du webtoon (library.rs) : c'est ce que lit la vue
    // Téléchargements pour afficher « 12/40 chapitres ».
    let _ = super::library::note_chapter(
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
    .await;

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
