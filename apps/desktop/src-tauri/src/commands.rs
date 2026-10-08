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
///
/// `pub(crate)` : le planificateur (3.1) pose exactement la même question que
/// l'interface et doit donc partager le MÊME délai. Deux constantes pour le même
/// délai divergent dès que l'une des deux est retouchée.
pub(crate) const DEEP_TIMEOUT: Duration = Duration::from_secs(60);
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
///
/// `pub` pour le test de contrat : une régression ici **ne casse rien** à la
/// compilation et ne se voit qu'à l'écran, sous la forme d'une vignette vide et
/// d'une console propre. Cf. `lib.rs`.
pub fn ajouter_jetons_couverture(cache: &PageCache, source: &str, result: &mut serde_json::Value) {
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

/// Paramètres invariants de `latest`, isolés pour être testables.
///
/// `safeOnly: false` désarme le repli `SAFE` du moteur : sans cette clé, une
/// option « Toutes » demandait du SAFE et les sources adultes renvoyaient zéro
/// résultat (mesuré sur WAMANGA). Et `contentRating` est **absent** tant que
/// l'utilisateur n'a pas choisi : la classification se décide une fois, dans les
/// réglages, jamais par défaut.
fn latest_params() -> serde_json::Value {
    serde_json::json!({
        "source": "",
        "offset": 0,
        "limit": 24,
        "query": "",
        "tags": [],
        "safeOnly": false,
    })
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
    let mut params = latest_params();
    params["source"] = serde_json::json!(source.trim());
    params["offset"] = serde_json::json!(offset.unwrap_or(0));
    params["limit"] = serde_json::json!(limit.unwrap_or(24).min(100));
    params["query"] = serde_json::json!(query.unwrap_or_default());
    params["tags"] = serde_json::json!(tags.unwrap_or_default());
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

/// Déballe l'enveloppe `{ok, result|error}` du moteur en `result`, ou en
/// `"CODE: message"`.
///
/// `pub` : le planificateur (3.1) appellera le moteur lui aussi, et il doit
/// partager cet unique déballage. Le dupliquer créerait un second endroit où
/// décider qu'une erreur est transitoire ou pas — exactement la divergence que
/// la régression de `search` vs `latest` avait déjà coûtée une fois.
///
/// `pub` aussi pour le test de contrat : c'est la fonction que
/// `tests/contrat_moteur.rs` rejoue sur **chaque** réponse enregistrée. Une
/// régression ici ne casse rien à la compilation : elle vide un écran, ou
/// transforme un challenge Cloudflare en « source morte ». Cf. `lib.rs`.
pub fn into_result(envelope: serde_json::Value) -> Result<serde_json::Value, String> {
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
    // Tâche 5.5 : l'URL **en clair** est retirée du journal. Elle contient le
    // titre, donc elle dit ce que la personne lit — et le journal part dans les
    // rapports de diagnostic. L'empreinte est stable d'une exécution à l'autre :
    // on peut donc relier deux signalements du même manga sans jamais écrire le
    // manga.
    tracing::info!(
        "details source={source} manga={}",
        crate::journal::hachage(manga_url.trim())
    );
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
    // Même raison que dans `details` : l'URL devient une empreinte stable.
    tracing::info!(
        "chapters source={source} manga={}",
        crate::journal::hachage(manga_url.trim())
    );
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

/// Injecte le jeton de cache et les dimensions (si disponibles) dans chaque
/// page d'un tableau JSON.
///
/// Extraite en fonction pure pour être testable sans `tauri::State`. La commande
/// `pages` délègue ici toute la logique d'injection ; `commands.rs::tests` la
/// teste directement.
pub fn injecter_dims_et_token(arr: &mut [serde_json::Value], cache: &PageCache, source: &str) {
    for p in arr.iter_mut() {
        let url = match p.get("url").and_then(|v| v.as_str()) {
            Some(u) => u.to_string(),
            None => continue,
        };
        let token = cache.register(source, &url);
        if let Some(dims) = cache.lire_dims(&token) {
            if let Some(o) = p.as_object_mut() {
                o.insert("largeur".to_string(), serde_json::json!(dims.0));
                o.insert("hauteur".to_string(), serde_json::json!(dims.1));
            }
        }
        p.as_object_mut()
            .map(|o| o.insert("token".to_string(), serde_json::Value::String(token)));
    }
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
        injecter_dims_et_token(arr, &pages_idx, source.trim());
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::{injecter_dims_et_token, latest_params, requested_rating};
    use crate::pagecache::PageCache;

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

    /// Le repli `SAFE` du moteur ne doit jamais être armé par accident.
    ///
    /// `requested_rating` renvoie `None` pour « Toutes », et `latest` construit
    /// alors ses params **sans** `contentRating` : c'est le moteur qui décide,
    /// et il ne demandera que du SAFE que si `safeOnly` est vrai. La commande
    /// `search`, qui faisait l'inverse (SAFE par défaut), a été retirée : deux
    /// sémantiques opposées pour le même réglage étaient le vrai défaut.
    #[test]
    fn le_repli_safe_du_moteur_nest_jamais_arme_par_l_accident() {
        let params = latest_params();
        assert_eq!(
            params["safeOnly"],
            serde_json::Value::Bool(false),
            "`safeOnly: false` doit être explicite : c'est lui qui désarme le repli SAFE"
        );
        assert!(
            !params
                .as_object()
                .expect("params objet")
                .contains_key("contentRating"),
            "aucune classification ne doit être envoyée quand l'option est « Toutes »"
        );
    }

    /// Construit un cache de test isolé (pid + compteur, effacé en Drop).
    fn cache_test() -> (std::sync::Arc<PageCache>, crate::download::DossierDeTest) {
        let garde = crate::download::DossierDeTest::new("cmd");
        (PageCache::new(garde.chemin()), garde)
    }

    // ---- Tests ADR-0012 : injecter_dims_et_token ----

    /// Cache froid : pas de dims → champ `largeur`/`hauteur` absent.
    #[test]
    fn pages_sans_cache_ne_renvoie_pas_largeur() {
        let (cache, _garde) = cache_test();
        let mut arr = vec![serde_json::json!({"index": 0, "url": "https://x/1.jpg"})];
        injecter_dims_et_token(&mut arr, &cache, "MANGADEX");
        let page = &arr[0];
        assert!(
            page.get("token").is_some(),
            "le token doit toujours être présent"
        );
        assert!(
            page.get("largeur").is_none(),
            "largeur absente si cache froid"
        );
        assert!(
            page.get("hauteur").is_none(),
            "hauteur absente si cache froid"
        );
    }

    /// Cache chaud : dims stockées → champs `largeur`/`hauteur` présents et corrects.
    #[test]
    fn pages_avec_cache_renvoie_largeur_et_hauteur() {
        let (cache, _garde) = cache_test();
        let token = cache.register("MANGADEX", "https://x/1.jpg");
        cache.stocker_dims(&token, (800, 2171));

        let mut arr = vec![serde_json::json!({"index": 0, "url": "https://x/1.jpg"})];
        injecter_dims_et_token(&mut arr, &cache, "MANGADEX");
        let page = &arr[0];
        assert_eq!(page["token"].as_str(), Some(token.as_str()));
        assert_eq!(page["largeur"].as_u64(), Some(800));
        assert_eq!(page["hauteur"].as_u64(), Some(2171));
    }

    /// Une page sans champ `url` est ignorée sans paniquer.
    #[test]
    fn pages_sans_url_sont_ignorees() {
        let (cache, _garde) = cache_test();
        let mut arr = vec![serde_json::json!({"index": 0})];
        injecter_dims_et_token(&mut arr, &cache, "MANGADEX");
        let page = &arr[0];
        assert!(page.get("token").is_none(), "pas de token si pas d'url");
        assert!(page.get("largeur").is_none());
    }
}

/// Retourne `true` si la base de données est en mémoire (repli de secours).
/// L'interface peut alors afficher un bandeau d'avertissement.
#[tauri::command]
pub async fn est_base_en_memoire(
    store: State<'_, Arc<crate::store::Store>>,
) -> Result<bool, String> {
    Ok(store.est_en_memoire())
}
