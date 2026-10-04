//! Commandes Tauri du magasin de données (favoris, historique, reprise).
//!
//! Elles sont fines : valident, appellent `store::Store`, renvoient un type
//! sérialisable. Toute la logique est dans `store.rs`, testable sans Tauri.

use serde::Serialize;
use std::sync::Arc;
use tauri::State;

use crate::store::{Favorite, HistoryEntry, Store};

fn vide(v: &str) -> bool {
    v.trim().is_empty()
}

/// Résout un paramètre obligatoire sous peine d'erreur explicite.
fn requis(source: &str, manga_url: &str) -> Result<(), String> {
    if vide(source) {
        return Err("PARSE_ERROR: source vide".to_string());
    }
    if vide(manga_url) {
        return Err("PARSE_ERROR: manga_url vide".to_string());
    }
    Ok(())
}

#[derive(Serialize)]
pub struct FavoriteState {
    pub source: String,
    pub manga_url: String,
    pub title: String,
    pub favorite: bool,
}

#[tauri::command]
pub async fn list_favorites(store: State<'_, Arc<Store>>) -> Result<Vec<Favorite>, String> {
    store.list_favorites()
}

/// Ajoute ou retire un favori. Le retour indique l'état obtenu, pour que
/// l'interface n'ait pas à refaire une requête.
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

/// Enregistre la position de lecture. Appelée à chaque changement de chapitre
/// ou de page, elle doit être bon marché : c'est un `INSERT ... ON CONFLICT`.
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

/// Mémorise la classification d'un titre déjà présent dans l'historique.
/// Appelée quand la fiche est chargée : c'est là que le classement est fiable
/// (mesuré, la liste renvoie `UNKNOWN` là où la fiche dit `ADULT`).
#[tauri::command]
pub async fn note_content_rating(
    store: State<'_, Arc<Store>>,
    source: String,
    manga_url: String,
    content_rating: String,
) -> Result<(), String> {
    requis(&source, &manga_url)?;
    // Une source qui ne classe rien ne doit pas écraser une valeur connue.
    if vide(&content_rating) || content_rating.eq_ignore_ascii_case("UNKNOWN") {
        return Ok(());
    }
    store.note_content_rating(source.trim(), manga_url.trim(), content_rating.trim())
}

#[tauri::command]
pub async fn list_recent(
    store: State<'_, Arc<Store>>,
    limit: Option<i64>,
    hide_adult: Option<bool>,
) -> Result<Vec<HistoryEntry>, String> {
    store.list_recent(limit.unwrap_or(24), hide_adult.unwrap_or(false))
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
