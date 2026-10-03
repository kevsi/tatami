//! Commandes Tauri fines (phase 0.9) : validation, appel au superviseur,
//! retour du `result` tel quel. La logique vit dans le superviseur / le moteur.

use std::sync::Arc;
use std::time::Duration;

use tauri::State;

use super::pagecache::PageCache;
use super::supervisor::Supervisor;

/// Listes, filtres et pagination : 30 s suffisent pour une source qui répond ;
/// au-delà c'est une source morte (mesuré : 404, hôte inconnu, 403 anti-bot).
const LIST_TIMEOUT: Duration = Duration::from_secs(30);
/// Détails et chapitres : une source qui pose une page de plus est normale.
const DEEP_TIMEOUT: Duration = Duration::from_secs(60);
/// `listSources` est un calcul local (index du JAR), pas un appel réseau.
const LOCAL_TIMEOUT: Duration = Duration::from_secs(10);

#[tauri::command]
pub async fn filter_options(
    sup: State<'_, Arc<Supervisor>>,
    source: String,
) -> Result<serde_json::Value, String> {
    into_result(
        sup.request(
            "filterOptions",
            serde_json::json!({ "source": source.trim() }),
            LIST_TIMEOUT,
        )
        .await?,
    )
}

#[tauri::command]
pub async fn list_sources(sup: State<'_, Arc<Supervisor>>) -> Result<serde_json::Value, String> {
    tracing::info!("list_sources: appel");
    let r = into_result(
        sup.request("listSources", serde_json::json!({}), LOCAL_TIMEOUT)
            .await?,
    )?;
    let n = r
        .get("sources")
        .and_then(|v| v.as_array())
        .map(|a| a.len())
        .unwrap_or(0);
    tracing::info!("list_sources: {n} sources");
    Ok(r)
}

#[tauri::command]
pub async fn latest(
    sup: State<'_, Arc<Supervisor>>,
    source: String,
    offset: Option<u32>,
    limit: Option<u32>,
    query: Option<String>,
    tags: Option<Vec<String>>,
    content_rating: Option<String>,
) -> Result<serde_json::Value, String> {
    // Une seule classification à la fois : plusieurs lèvent « Multiple Content
    // ratings are not supported by this source » côté parser (mesuré sur MangaDex).
    let rating = match content_rating.as_deref().map(str::trim).filter(|r| !r.is_empty()) {
        Some(r) => r.to_ascii_uppercase(),
        None => "SAFE".to_string(),
    };
    tracing::info!(
        "latest source={source} offset={:?} query={:?} tags={} rating={rating}",
        offset,
        query.as_deref().unwrap_or(""),
        tags.as_ref().map_or(0, Vec::len),
    );
    let params = serde_json::json!({
        "source": source.trim(),
        "offset": offset.unwrap_or(0),
        "limit": limit.unwrap_or(24).min(100),
        "query": query.unwrap_or_default(),
        "tags": tags.unwrap_or_default(),
        "contentRating": rating,
    });
    let r = into_result(sup.request("latest", params, LIST_TIMEOUT).await?)?;
    tracing::info!(
        "latest ok: {} items",
        r.get("list")
            .and_then(|v| v.as_array())
            .map(|a| a.len())
            .unwrap_or(0)
    );
    Ok(r)
}


fn into_result(envelope: serde_json::Value) -> Result<serde_json::Value, String> {
    if envelope.get("ok").and_then(serde_json::Value::as_bool) == Some(true) {
        Ok(envelope
            .get("result")
            .cloned()
            .unwrap_or(serde_json::Value::Null))
    } else {
        let e = envelope
            .get("error")
            .cloned()
            .unwrap_or(serde_json::Value::Null);
        let code = e
            .get("code")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("INTERNAL");
        let msg = e
            .get("message")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        Err(format!("{code}: {msg}"))
    }
}

#[tauri::command]
pub async fn search(
    sup: State<'_, Arc<Supervisor>>,
    source: String,
    query: String,
    limit: Option<u32>,
    content_rating: Option<String>,
) -> Result<serde_json::Value, String> {
    let query = query.trim();
    if query.is_empty() {
        return Err("PARSE_ERROR: query vide".to_string());
    }
    let rating = match content_rating.as_deref().map(str::trim).filter(|r| !r.is_empty()) {
        Some(r) => r.to_ascii_uppercase(),
        None => "SAFE".to_string(),
    };
    let params = serde_json::json!({
        "source": source.trim(),
        "query": query,
        "limit": limit.unwrap_or(20).min(50),
        "contentRating": rating,
    });
    into_result(sup.request("search", params, LIST_TIMEOUT).await?)
}

#[tauri::command]
pub async fn chapters(
    sup: State<'_, Arc<Supervisor>>,
    source: String,
    manga_url: String,
) -> Result<serde_json::Value, String> {
    if manga_url.trim().is_empty() {
        return Err("PARSE_ERROR: manga_url vide".to_string());
    }
    tracing::info!("chapters source={source} manga_url={manga_url}");
    let params = serde_json::json!({ "source": source.trim(), "mangaUrl": manga_url });
    into_result(sup.request("chapters", params, DEEP_TIMEOUT).await?)
}

#[tauri::command]
pub async fn pages(
    sup: State<'_, Arc<Supervisor>>,
    pages_idx: State<'_, Arc<PageCache>>,
    source: String,
    chapter_url: String,
) -> Result<serde_json::Value, String> {
    if chapter_url.trim().is_empty() {
        return Err("PARSE_ERROR: chapter_url vide".to_string());
    }
    let params = serde_json::json!({ "source": source.trim(), "chapterUrl": chapter_url });
    let mut result = into_result(sup.request("pages", params, DEEP_TIMEOUT).await?)?;
    if let Some(arr) = result.get_mut("pages").and_then(|v| v.as_array_mut()) {
        for p in arr.iter_mut() {
            if let Some(url) = p.get("url").and_then(|v| v.as_str()) {
                let token = pages_idx.register(source.trim(), url);
                p.as_object_mut()
                    .map(|o| o.insert("token".to_string(), serde_json::Value::String(token)));
            }
        }
    }
    Ok(result)
}
