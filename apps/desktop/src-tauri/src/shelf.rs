//! Commandes Tauri du magasin de donnÃ©es (favoris, historique, reprise).
//!
//! Elles sont fines : valident, appellent `store::Store`, renvoient un type
//! sÃ©rialisable. Toute la logique est dans `store.rs`, testable sans Tauri.

use serde::Serialize;
use std::sync::Arc;
use tauri::State;

use crate::pagecache::jeton_cover as jeton;
use crate::pagecache::PageCache;
use crate::store::{Bookmark, Category, Favorite, HistoryEntry, Store};

/// Nombre d'entrÃ©es renvoyÃ©es quand l'interface ne le prÃ©cise pas.
const RECENT_LIMIT: u32 = 24;

fn vide(v: &str) -> bool {
    v.trim().is_empty()
}

/// RÃ©sout un paramÃ¨tre obligatoire sous peine d'erreur explicite.
fn requis(source: &str, manga_url: &str) -> Result<(), String> {
    if vide(source) {
        return Err("PARSE_ERROR: source vide".to_string());
    }
    if vide(manga_url) {
        return Err("PARSE_ERROR: manga_url vide".to_string());
    }
    Ok(())
}

/// Sources que l'utilisateur a Ã©cartÃ©es. Liste vide = toutes actives.
#[tauri::command]
pub async fn list_disabled_sources(store: State<'_, Arc<Store>>) -> Result<Vec<String>, String> {
    store.disabled_sources()
}

/// Active ou dÃ©sactive une source. Une source jamais touchÃ©e est active.
#[tauri::command]
pub async fn set_source_enabled(
    store: State<'_, Arc<Store>>,
    source: String,
    enabled: bool,
) -> Result<(), String> {
    if vide(&source) {
        return Err("PARSE_ERROR: source vide".to_string());
    }
    store.set_source_enabled(source.trim(), enabled)
}

/// RÃ©active toutes les sources d'un coup.
#[tauri::command]
pub async fn enable_all_sources(store: State<'_, Arc<Store>>) -> Result<(), String> {
    store.enable_all_sources()
}

/// CatÃ©gories de favoris, dans l'ordre choisi par l'utilisateur.
#[tauri::command]
pub async fn list_categories(store: State<'_, Arc<Store>>) -> Result<Vec<Category>, String> {
    store.list_categories()
}

/// CrÃ©e une catÃ©gorie. La couleur est un index de palette (1..=8), pas une
/// valeur CSS : elle est bornÃ©e ici pour que l'interface ait toujours une
/// variable Ã  traduire.
#[tauri::command]
pub async fn add_category(
    store: State<'_, Arc<Store>>,
    name: String,
    color: u8,
) -> Result<Category, String> {
    let nom = nom_categorie(&name)?;
    store.add_category(nom, color.clamp(1, 8))
}

/// Renomme et/ou recolore une catÃ©gorie.
#[tauri::command]
pub async fn rename_category(
    store: State<'_, Arc<Store>>,
    id: i64,
    name: String,
    color: u8,
) -> Result<(), String> {
    let nom = nom_categorie(&name)?;
    store.rename_category(id, nom, color.clamp(1, 8))
}

/// Supprime une catÃ©gorie ; ses favoris reviennent en Â« sans catÃ©gorie Â».
#[tauri::command]
pub async fn delete_category(store: State<'_, Arc<Store>>, id: i64) -> Result<(), String> {
    store.delete_category(id)
}

/// RÃ©Ã©crit l'ordre des catÃ©gories Ã  partir de la liste d'identifiants.
#[tauri::command]
pub async fn reorder_categories(store: State<'_, Arc<Store>>, ids: Vec<i64>) -> Result<(), String> {
    store.reorder_categories(&ids)
}

/// Affecte (ou retire) la catÃ©gorie d'un favori.
#[tauri::command]
pub async fn set_favorite_category(
    store: State<'_, Arc<Store>>,
    source: String,
    manga_url: String,
    category_id: Option<i64>,
) -> Result<(), String> {
    requis(&source, &manga_url)?;
    store.set_favorite_category(source.trim(), manga_url.trim(), category_id)
}

/// Nom de catÃ©gorie valide : non vide et bornÃ©. La longueur maximale Ã©vite une
/// Ã©tiquette qui dÃ©borde partout ailleurs.
fn nom_categorie(name: &str) -> Result<&str, String> {
    let n = name.trim();
    if n.is_empty() {
        return Err("PARSE_ERROR: nom de catÃ©gorie vide".to_string());
    }
    if n.chars().count() > 30 {
        return Err("PARSE_ERROR: nom de catÃ©gorie trop long (30 caractÃ¨res max)".to_string());
    }
    Ok(n)
}

/// Ajoute un signet sur la page courante.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn add_bookmark(
    store: State<'_, Arc<Store>>,
    source: String,
    manga_url: String,
    chapter_url: String,
    title: String,
    cover_url: Option<String>,
    chapter_title: Option<String>,
    page_index: u16,
    page_count: u16,
) -> Result<(), String> {
    requis(&source, &manga_url)?;
    if vide(&chapter_url) {
        return Err("PARSE_ERROR: chapter_url vide".to_string());
    }
    store.add_bookmark(
        source.trim(),
        manga_url.trim(),
        chapter_url.trim(),
        title.trim(),
        cover_url.as_deref(),
        chapter_title.as_deref(),
        page_index,
        page_count,
    )
}

/// Retire le signet d'une page prÃ©cise.
#[tauri::command]
pub async fn remove_bookmark(
    store: State<'_, Arc<Store>>,
    source: String,
    manga_url: String,
    chapter_url: String,
    page_index: u16,
) -> Result<(), String> {
    requis(&source, &manga_url)?;
    store.remove_bookmark(
        source.trim(),
        manga_url.trim(),
        chapter_url.trim(),
        page_index,
    )
}

/// Signets, du plus rÃ©cent au plus ancien, Ã©ventuellement limitÃ©s Ã  un webtoon.
#[tauri::command]
pub async fn list_bookmarks(
    store: State<'_, Arc<Store>>,
    cache: State<'_, Arc<PageCache>>,
    limit: Option<u32>,
    manga_url: Option<String>,
) -> Result<Vec<Bookmark>, String> {
    let n = limit.unwrap_or(RECENT_LIMIT).min(200);
    let filtre = manga_url
        .as_deref()
        .map(str::trim)
        .filter(|u| !u.is_empty());
    let mut lignes = store.list_bookmarks(n, filtre)?;
    for l in lignes.iter_mut() {
        l.cover_token = jeton(&cache, &l.source, &l.manga_url, l.cover_url.as_deref());
    }
    Ok(lignes)
}

#[derive(Serialize)]
pub struct FavoriteState {
    pub source: String,
    pub manga_url: String,
    pub title: String,
    pub favorite: bool,
}

#[tauri::command]
pub async fn list_favorites(
    store: State<'_, Arc<Store>>,
    cache: State<'_, Arc<PageCache>>,
) -> Result<Vec<Favorite>, String> {
    let mut lignes = store.list_favorites()?;
    for l in lignes.iter_mut() {
        l.cover_token = jeton(&cache, &l.source, &l.manga_url, l.cover_url.as_deref());
    }
    Ok(lignes)
}

/// Ajoute ou retire un favori. Le retour indique l'Ã©tat obtenu, pour que
/// l'interface n'ait pas Ã  refaire une requÃªte.
#[tauri::command]
pub async fn set_favorite(
    store: State<'_, Arc<Store>>,
    source: String,
    manga_url: String,
    title: String,
    cover_url: Option<String>,
    content_rating: Option<String>,
    favorite: bool,
) -> Result<FavoriteState, String> {
    requis(&source, &manga_url)?;
    if vide(&title) {
        return Err("PARSE_ERROR: title vide".to_string());
    }
    // Une classification inconnue ne doit pas remplacer une valeur connue.
    let rating = content_rating.filter(|c| !c.eq_ignore_ascii_case("UNKNOWN"));
    store.set_favorite(
        source.trim(),
        manga_url.trim(),
        title.trim(),
        cover_url.as_deref(),
        rating.as_deref(),
        favorite,
    )?;
    Ok(FavoriteState {
        source: source.trim().to_string(),
        manga_url: manga_url.trim().to_string(),
        title: title.trim().to_string(),
        favorite,
    })
}

/// Enregistre la position de lecture. AppelÃ©e Ã  chaque changement de chapitre
/// ou de page, elle doit Ãªtre bon marchÃ© : c'est un `INSERT ... ON CONFLICT`.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn save_progress(
    store: State<'_, Arc<Store>>,
    source: String,
    manga_url: String,
    title: String,
    cover_url: Option<String>,
    content_rating: Option<String>,
    chapter_url: Option<String>,
    chapter_title: Option<String>,
    chapter_index: Option<i64>,
    page_index: Option<i64>,
    page_count: Option<i64>,
) -> Result<(), String> {
    requis(&source, &manga_url)?;
    if vide(&title) {
        return Err("PARSE_ERROR: title vide".to_string());
    }
    store.save_progress(
        source.trim(),
        manga_url.trim(),
        title.trim(),
        cover_url.as_deref(),
        content_rating.as_deref(),
        chapter_url.as_deref(),
        chapter_title.as_deref(),
        chapter_index.unwrap_or(0),
        page_index.unwrap_or(0),
        page_count.unwrap_or(0),
    )
}

/// MÃ©morise la classification d'un titre dÃ©jÃ  prÃ©sent dans l'historique.
/// AppelÃ©e quand la fiche est chargÃ©e : c'est lÃ  que le classement est fiable
/// (mesurÃ©, la liste renvoie `UNKNOWN` lÃ  oÃ¹ la fiche dit `ADULT`).
#[tauri::command]
pub async fn note_content_rating(
    store: State<'_, Arc<Store>>,
    source: String,
    manga_url: String,
    content_rating: String,
) -> Result<(), String> {
    requis(&source, &manga_url)?;
    // Une source qui ne classe rien ne doit pas Ã©craser une valeur connue.
    if vide(&content_rating) || content_rating.eq_ignore_ascii_case("UNKNOWN") {
        return Ok(());
    }
    store.note_content_rating(source.trim(), manga_url.trim(), content_rating.trim())
}

#[tauri::command]
pub async fn list_recent(
    store: State<'_, Arc<Store>>,
    cache: State<'_, Arc<PageCache>>,
    limit: Option<i64>,
    hide_adult: Option<bool>,
) -> Result<Vec<HistoryEntry>, String> {
    let mut lignes = store.list_recent(limit.unwrap_or(24), hide_adult.unwrap_or(false))?;
    for l in lignes.iter_mut() {
        l.cover_token = jeton(&cache, &l.source, &l.manga_url, l.cover_url.as_deref());
    }
    Ok(lignes)
}

#[tauri::command]
pub async fn forget_history(
    store: State<'_, Arc<Store>>,
    source: String,
    manga_url: String,
) -> Result<(), String> {
    requis(&source, &manga_url)?;
    store.forget(source.trim(), manga_url.trim())
}

#[tauri::command]
pub async fn clear_history(store: State<'_, Arc<Store>>) -> Result<(), String> {
    store.forget_all()
}
