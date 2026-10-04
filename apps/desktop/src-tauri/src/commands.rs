//! Commandes Tauri fines (phase 0.9) : validation, appel au superviseur,
//! retour du `result` tel quel. La logique vit dans le superviseur / le moteur.

use std::sync::Arc;
use std::time::Duration;

use tauri::State;

use super::pagecache::PageCache;
use super::supervisor::Supervisor;

/// Listes, filtres et pagination. 60 s par défaut : certaines sources
/// scannent réellement le catalogue (EXHENTAI mesuré à 35,7 s pour une page),
/// mais au-delà on ne gagne plus rien — le délai de lecture OkHttp du moteur
/// est lui-même de 60 s.
const LIST_TIMEOUT: Duration = Duration::from_secs(60);
/// Plafond dur, même si l'utilisateur règle un délai absurde.
const LIST_TIMEOUT_MAX: Duration = Duration::from_secs(180);
/// Détails et chapitres : une source qui pose une page de plus est normale.
const DEEP_TIMEOUT: Duration = Duration::from_secs(60);
/// `listSources` est un calcul local (index du JAR), pas un appel réseau.
const LOCAL_TIMEOUT: Duration = Duration::from_secs(10);

fn list_timeout(seconds: Option<u32>) -> Duration {
    let asked = Duration::from_secs(seconds.unwrap_or(60).clamp(10, 180) as u64);
    asked.min(LIST_TIMEOUT_MAX)
}

/// Ajoute un jeton de couverture à chaque manga d'une réponse du moteur.
///
/// La webview ne doit jamais appeler un site de manga : elle perdrait le
/// user-agent, les cookies et le déblocage Cloudflare que le moteur met en
/// place, et l'échec serait silencieux (vignette vide, console propre). Le jeton
/// est donc la seule chose que l'interface connaît ; `coverUrl` reste dans la
/// charge utile parce que Rust s'en sert pour la résolution après redémarrage.
fn ajouter_jetons_couverture(cache: &PageCache, source: &str, result: &mut serde_json::Value) {
    let Some(listes) = result.get_mut("list").and_then(|v| v.as_array_mut()) else {
        return;
    };
    for manga in listes.iter_mut() {
        porter_jeton(cache, source, manga);
    }
}

/// Même chose pour un manga unique (`details` renvoie `{ manga: {...} }`).
fn ajouter_jeton_couverture_unique(
    cache: &PageCache,
    source: &str,
    result: &mut serde_json::Value,
) {
    if let Some(manga) = result.get_mut("manga") {
        porter_jeton(cache, source, manga);
    }
}

fn porter_jeton(cache: &PageCache, source: &str, manga: &mut serde_json::Value) {
    let Some(objet) = manga.as_object_mut() else {
        return;
    };
    let url = objet
        .get("url")
        .and_then(|v| v.as_str())
        .unwrap_or_default()
        .to_string();
    let cover = objet
        .get("coverUrl")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    if url.is_empty() || cover.is_none() {
        return;
    }
    let token = cache.register_cover(source, &url, cover.as_deref());
    objet.insert("coverToken".to_string(), serde_json::Value::String(token));
}

#[tauri::command]
pub async fn filter_options(
    sup: State<'_, Arc<Supervisor>>,
    source: String,
) -> Result<serde_json::Value, String> {
    into_result(
        sup.request_with_retry(
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

/// Classification demandée pour une liste de titres.
///
/// `None` = **aucun filtre** (option « Toutes » des réglages). Une chaîne vide
/// ou blanche ne doit surtout pas être confondue avec `SAFE` : mesuré, le
/// réglage « Toutes » envoyait `SAFE` et les sources adultes renvoyaient zéro
/// résultat alors que l'utilisateur avait explicitement demandé l'inverse.
fn requested_rating(content_rating: Option<&str>) -> Option<String> {
    content_rating
        .map(str::trim)
        .filter(|r| !r.is_empty())
        .map(str::to_ascii_uppercase)
}

#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn latest(
    sup: State<'_, Arc<Supervisor>>,
    cache: State<'_, Arc<PageCache>>,
    source: String,
    offset: Option<u32>,
    limit: Option<u32>,
    query: Option<String>,
    tags: Option<Vec<String>>,
    content_rating: Option<String>,
    timeout_secs: Option<u32>,
) -> Result<serde_json::Value, String> {
    let timeout = list_timeout(timeout_secs);
    // Une seule classification à la fois : plusieurs lèvent « Multiple Content
    // ratings are not supported by this source » côté parser (mesuré sur MangaDex).
    let rating = requested_rating(content_rating.as_deref());
    tracing::info!(
        "latest source={source} offset={:?} query={:?} tags={} rating={}",
        offset,
        query.as_deref().unwrap_or(""),
        tags.as_ref().map_or(0, Vec::len),
        rating.as_deref().unwrap_or("AUCUN"),
    );
    let mut params = serde_json::json!({
        "source": source.trim(),
        "offset": offset.unwrap_or(0),
        "limit": limit.unwrap_or(24).min(100),
        "query": query.unwrap_or_default(),
        "tags": tags.unwrap_or_default(),
        // Le moteur retombe sur SAFE quand `contentRating` est vide ET
        // `safeOnly` est vrai. Le choix est déjà fait en amont (réglages), donc
        // on désactive ce repli : sinon l'option « Toutes » renvoyait SAFE et
        // les sources adultes ne donnaient rien (mesuré sur WAMANGA).
        "safeOnly": false,
    });
    if let Some(r) = &rating {
        params["contentRating"] = serde_json::json!(r);
    }
    let mut r = into_result(sup.request_with_retry("latest", params, timeout).await?)?;
    ajouter_jetons_couverture(&cache, source.trim(), &mut r);
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
    cache: State<'_, Arc<PageCache>>,
    source: String,
    query: String,
    limit: Option<u32>,
    content_rating: Option<String>,
) -> Result<serde_json::Value, String> {
    let query = query.trim();
    if query.is_empty() {
        return Err("PARSE_ERROR: query vide".to_string());
    }
    let rating = match content_rating
        .as_deref()
        .map(str::trim)
        .filter(|r| !r.is_empty())
    {
        Some(r) => r.to_ascii_uppercase(),
        None => "SAFE".to_string(),
    };
    let params = serde_json::json!({
        "source": source.trim(),
        "query": query,
        "limit": limit.unwrap_or(20).min(50),
        "contentRating": rating,
    });
    let mut r = into_result(
        sup.request_with_retry("search", params, LIST_TIMEOUT)
            .await?,
    )?;
    ajouter_jetons_couverture(&cache, source.trim(), &mut r);
    Ok(r)
}

/// Tâche 0.17 : fiche détaillée (résumé, état, genres). Distinct de
/// `chapters` : celle-ci récupère la liste des chapitres, celle-là la fiche.
#[tauri::command]
pub async fn details(
    sup: State<'_, Arc<Supervisor>>,
    cache: State<'_, Arc<PageCache>>,
    source: String,
    manga_url: String,
    manga_title: Option<String>,
) -> Result<serde_json::Value, String> {
    if manga_url.trim().is_empty() {
        return Err("PARSE_ERROR: manga_url vide".to_string());
    }
    tracing::info!("details source={source} manga_url={manga_url}");
    let params = serde_json::json!({
        "source": source.trim(),
        "url": manga_url,
        "mangaTitle": manga_title.unwrap_or_default(),
    });
    let mut r = into_result(
        sup.request_with_retry("details", params, DEEP_TIMEOUT)
            .await?,
    )?;
    ajouter_jeton_couverture_unique(&cache, source.trim(), &mut r);
    Ok(r)
}

#[tauri::command]
pub async fn chapters(
    sup: State<'_, Arc<Supervisor>>,
    source: String,
    manga_url: String,
    manga_title: Option<String>,
) -> Result<serde_json::Value, String> {
    if manga_url.trim().is_empty() {
        return Err("PARSE_ERROR: manga_url vide".to_string());
    }
    tracing::info!("chapters source={source} manga_url={manga_url}");
    // `mangaTitle` permet au moteur de reconstruire le webtoon après un
    // redémarrage (cache de session vide) : sans lui, toute reprise échoue.
    let params = serde_json::json!({
        "source": source.trim(),
        "mangaUrl": manga_url,
        "mangaTitle": manga_title.unwrap_or_default(),
    });
    into_result(
        sup.request_with_retry("chapters", params, DEEP_TIMEOUT)
            .await?,
    )
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
    let mut result = into_result(
        sup.request_with_retry("pages", params, DEEP_TIMEOUT)
            .await?,
    )?;
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

#[cfg(test)]
mod tests {
    use super::requested_rating;

    #[test]
    fn une_chaine_vide_ne_signifie_pas_safe() {
        // Régression mesurée : l'option « Toutes » des réglages envoyait SAFE,
        // les sources adultes renvoyaient alors zéro manga.
        assert_eq!(requested_rating(Some("")), None);
        assert_eq!(requested_rating(Some("   ")), None);
        assert_eq!(requested_rating(None), None);
        assert_eq!(requested_rating(Some("adult")), Some(String::from("ADULT")));
        assert_eq!(requested_rating(Some(" safe ")), Some(String::from("SAFE")));
    }
}
