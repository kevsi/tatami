//! Commandes Tauri fines (phase 0.9) : validation, appel au superviseur,
//! retour du `result` tel quel. La logique vit dans le superviseur / le moteur.

use std::sync::Arc;
use std::time::Duration;

use tauri::{AppHandle, State};

use super::pagecache::PageCache;
use super::supervisor::Supervisor;
use crate::metacache::{Genre, MetaCache};
use crate::store::now_ms;

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
/// `listSources` : le délai doit couvrir l'**instanciation des 1370 parsers**.
///
/// Mesuré le 2026-10-12 : 7,0–8,5 s moteur au repos, mais **55–57 s** sur le poste
/// réel (le moteur, `cargo`, `next dev` et la WebView se partagent le CPU). Le
/// commentaire ancien disait « calcul local (index du JAR), pas un appel réseau »
/// — c'est faux, et c'est ce qui a justifié les 10 s : `Server.kt:554` instancie
/// chaque parser (`newParserInstance`) pour en lire le domaine. Un budget de 10 s
/// y était donc franchi en permanence, la commande échouait, et le catalogue
/// s'affichait vide.
///
/// `pub(crate)` : la recherche globale (`recherche.rs`) interroge le même
/// `listSources` et doit porter le MÊME budget. Deux délais pour la même requête
/// divergeraaient au premier réglage.
pub(crate) const LOCAL_TIMEOUT: Duration = Duration::from_secs(60);

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
    // `request_with_retry`, pas `request` : c'était la **seule** commande du
    // fichier sans rejeu (lignes 90, 193, 268, 298, 339 et le planificateur
    // utilisent toutes le retry). Un timeout de plus sous charge — et
    // l'instanciation des 1370 parsers en fait un événement courant — vidait
    // donc le catalogue sans qu'aucune nouvelle tentative ne le rattrape.
    // `chargerSources` ne fait que `setError` sur échec : aucun écran vide
    // n'était explicable à l'utilisateur.
    let r = into_result(
        sup.request_with_retry("listSources", serde_json::json!({}), LOCAL_TIMEOUT)
            .await?,
    )?;
    sup.enregistrer_domaines_sources(&r).await;
    let n = r
        .get("sources")
        .and_then(|v| v.as_array())
        .map(|a| a.len())
        .unwrap_or(0);
    tracing::info!("list_sources: {n} sources");
    Ok(r)
}

/// Scripts que le moteur a **refusés** de charger, avec la raison qu'il a
/// donnée sur stderr.
///
/// Le moteur écrit ses refus sur stderr, jamais sur stdout (cf. contrat), donc
/// `list_sources` ne peut pas les porter : l'interface ne pouvait que montrer
/// le fichier, comme s'il était chargé. Mesuré le 2026-10-10 — `mangadexFr.js`
/// refusé à chaque démarrage (`Missing close quote`), invisible pour
/// l'utilisateur alors que le fichier était là depuis trois jours.
#[tauri::command]
pub async fn scripts_refuses(sup: State<'_, Arc<Supervisor>>) -> Result<Vec<String>, String> {
    Ok(sup.avertissements().await)
}

/// Classification demandée pour une liste de titres.
///
/// `None` = **aucun filtre** (option « Toutes » des réglages). Une chaîne vide
/// ou blanche ne doit surtout pas être confondue avec `SAFE` : mesuré, le
/// réglage « Toutes » envoyait `SAFE` et les sources adultes renvoyaient zéro
/// résultat alors que l'utilisateur avait explicitement demandé l'inverse.
/// `pub(crate)` : la recherche globale porte exactement la même règle
/// (`recherche.rs`). La divergence `search` / `latest` sur ce réglage avait
/// déjà coûté une régression — deux implémentations de la même décision
/// finissent toujours par diverger.
pub(crate) fn requested_rating(content_rating: Option<&str>) -> Option<String> {
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
    app: AppHandle,
    sup: State<'_, Arc<Supervisor>>,
    cache: State<'_, Arc<PageCache>>,
    meta: State<'_, Arc<MetaCache>>,
    source: String,
    offset: Option<u32>,
    limit: Option<u32>,
    query: Option<String>,
    tags: Option<Vec<String>>,
    content_rating: Option<String>,
    timeout_secs: Option<u32>,
    refresh: Option<bool>,
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
    // La cle du cache est **les paramètres eux-mêmes** (sans le délai, qui ne
    // change pas la réponse) : deux appels ne partagent une entrée que s'ils
    // demandent exactement la même page — source, offset, tri, filtres.
    let cle = params.to_string();
    if refresh != Some(true) {
        if let Some((at, donnees)) = meta.lire(Genre::Latest, &cle) {
            let mut r = donnees;
            ajouter_jetons_couverture(&cache, source.trim(), &mut r);
            tracing::info!(
                "latest cache=hit items={} age_s={}",
                nb_items(&r),
                (now_ms() - at) / 1000
            );
            return Ok(marquer_cache(r, at));
        }
    }
    // Passage par le transport webview : une source derrière Cloudflare se voit
    // refuser ici, et le navigateur passe. `request_avec_transport` ne le
    // tente **qu'une fois par URL** et jamais pour un refus déjà résolu.
    let mut r = into_result(
        sup.request_avec_transport(&app, source.trim(), "latest", params, timeout)
            .await?,
    )?;
    // Écrit **avant** l'injection des jetons : le cache garde la réponse du
    // moteur telle quelle, et l'injection (déterministe dans `pagecache`) est
    // refaite à chaque lecture — un jeton d'une session morte ne fuit donc pas.
    meta.ecrire(Genre::Latest, &cle, &r);
    ajouter_jetons_couverture(&cache, source.trim(), &mut r);
    tracing::info!("latest ok: {} items", nb_items(&r));
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
pub fn message_erreur(enveloppe: &serde_json::Value) -> Option<String> {
    if enveloppe.get("ok").and_then(serde_json::Value::as_bool) == Some(true) {
        return None;
    }
    let e = enveloppe
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
    Some(format!("{code}: {msg}"))
}

pub fn into_result(enveloppe: serde_json::Value) -> Result<serde_json::Value, String> {
    if let Some(message) = message_erreur(&enveloppe) {
        return Err(message);
    }
    Ok(enveloppe
        .get("result")
        .cloned()
        .unwrap_or(serde_json::Value::Null))
}

/// Marque une réponse **servie par le cache disque** avec l'instant de son
/// écriture (`cacheAt`, epoch ms). L'interface s'en sert pour rafraîchir en
/// arrière-plan : une réponse fraîche du réseau n'a pas ce champ, donc il n'y a
/// rien à revalider. Un champ **absent** ne ment pas — contrairement à un
/// `cacheAt: 0`, qu'il faudrait distinguer d'un vrai minuit de 1970.
fn marquer_cache(mut valeur: serde_json::Value, at: i64) -> serde_json::Value {
    if let Some(o) = valeur.as_object_mut() {
        o.insert("cacheAt".to_string(), serde_json::json!(at));
    }
    valeur
}

/// Nombre d'entrées d'une réponse `latest` — pour les lignes de journal, seule
/// trace d'un coup de cache réussi (le superviseur, lui, n'écrit que sur un
/// aller réseau).
fn nb_items(v: &serde_json::Value) -> usize {
    v.get("list")
        .and_then(|v| v.as_array())
        .map(|a| a.len())
        .unwrap_or(0)
}

/// Tâche 0.17 : fiche détaillée (résumé, état, genres). Distinct de
/// `chapters` : celle-ci récupère la liste des chapitres, celle-là la fiche.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn details(
    app: AppHandle,
    sup: State<'_, Arc<Supervisor>>,
    cache: State<'_, Arc<PageCache>>,
    meta: State<'_, Arc<MetaCache>>,
    source: String,
    manga_url: String,
    manga_title: Option<String>,
    refresh: Option<bool>,
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
    // Cle : source + URL, jamais le titre — celui-ci varie d'un chemin
    // d'ouverture à l'autre (« Art » contre « Art (1) ») sans changer la réponse.
    let cle = format!("{}|{}", source.trim(), manga_url.trim());
    if refresh != Some(true) {
        if let Some((at, donnees)) = meta.lire(Genre::Details, &cle) {
            let mut r = donnees;
            ajouter_jeton_couverture_unique(&cache, source.trim(), &mut r);
            tracing::info!("details cache=hit age_s={}", (now_ms() - at) / 1000);
            return Ok(marquer_cache(r, at));
        }
    }
    let params = serde_json::json!({
        "source": source.trim(),
        "url": manga_url,
        "mangaTitle": manga_title.unwrap_or_default(),
    });
    let mut r = into_result(
        sup.request_avec_transport(&app, source.trim(), "details", params, DEEP_TIMEOUT)
            .await?,
    )?;
    meta.ecrire(Genre::Details, &cle, &r);
    ajouter_jeton_couverture_unique(&cache, source.trim(), &mut r);
    Ok(r)
}

#[tauri::command]
pub async fn chapters(
    app: AppHandle,
    sup: State<'_, Arc<Supervisor>>,
    meta: State<'_, Arc<MetaCache>>,
    source: String,
    manga_url: String,
    manga_title: Option<String>,
    refresh: Option<bool>,
) -> Result<serde_json::Value, String> {
    if manga_url.trim().is_empty() {
        return Err("PARSE_ERROR: manga_url vide".to_string());
    }
    // Même raison que dans `details` : l'URL devient une empreinte stable.
    tracing::info!(
        "chapters source={source} manga={}",
        crate::journal::hachage(manga_url.trim())
    );
    let cle = format!("{}|{}", source.trim(), manga_url.trim());
    let titre = manga_title.unwrap_or_default();
    if refresh != Some(true) {
        if let Some((at, donnees)) = meta.lire(Genre::Chapters, &cle) {
            tracing::info!("chapters cache=hit age_s={}", (now_ms() - at) / 1000);
            // L'index se remplit **aussi** sur un coup de cache : c'est le seul
            // moyen pour `pages` de savoir quel manga réamorcer quand le moteur
            // n'a jamais vu la fiche (cf. `pages_avec_amorcage`).
            meta.indexer_chapitres(source.trim(), manga_url.trim(), &titre, &donnees);
            return Ok(marquer_cache(donnees, at));
        }
    }
    // `mangaTitle` permet au moteur de reconstruire le webtoon après un
    // redémarrage (cache de session vide) : sans lui, toute reprise échoue.
    let params = serde_json::json!({
        "source": source.trim(),
        "mangaUrl": manga_url.trim(),
        "mangaTitle": titre.as_str(),
    });
    let r = into_result(
        sup.request_avec_transport(&app, source.trim(), "chapters", params, DEEP_TIMEOUT)
            .await?,
    )?;
    meta.ecrire(Genre::Chapters, &cle, &r);
    meta.indexer_chapitres(source.trim(), manga_url.trim(), &titre, &r);
    Ok(r)
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

/// Marqueur du refus du moteur : sa **session** ne connaît pas encore le
/// chapitre (elle ne l'apprend que par `chapters`). Le libellé vient de
/// `engine/…/Server.kt` (« unknown chapter url, call chapters first: … ») — un
/// test de ce module lit le fichier Kotlin et vérifie que les deux concordent,
/// parce qu'aucun compilateur ne relie un message Kotlin à une comparaison Rust
/// (leçon AGENTS.md : une constante partagée entre deux langages a besoin d'un
/// test qui lit l'autre fichier).
const REFUS_CHAPTERS_MOTEUR: &str = "unknown chapter url";

/// `pages`, avec l'amorçage que le moteur exige.
///
/// **Le défaut mesuré (2026-10-19, session 2 du chantier A).** Le moteur ne
/// reconnaît un chapitre que si la fiche a été listée **pendant sa session**
/// (cache en mémoire, `Server.kt`). Quand le cache disque sert `chapters` sans
/// toucher le moteur, `pages` répond « unknown chapter url » : mesuré, chaque
/// revalidation d'arrière-plan échouait, et un chapitre non encore mis en cache
/// aurait été illisible. Quatre parades locales existaient déjà (téléchargeur,
/// reprise, signet, commentaire de `openReaderAt`) — celle de ce chemin
/// manquait, et la centraliser ici évite que le prochain flux l'oublie à son
/// tour.
///
/// **La parade.** Sur ce refus précis, et si l'index de session connaît le manga
/// du chapitre : refaire `chapters` au moteur (la réponse fraîche réécrit
/// l'entrée disque, donc les prochains `pages` n'ont plus à amorcer), puis
/// **une seule** nouvelle tentative de `pages`. Toute autre erreur — et le
/// refus sans manga connu — ressort inchangée.
///
/// **Course assumée.** Si le rafraîchissement d'arrière-plan de l'interface
/// relaie la même fiche en même temps, le moteur sérialise par source et la
/// fiche est demandée deux fois : une requête de trop sous course, préférée à
/// un partage de futur en vol pour un chemin rare.
async fn pages_avec_amorcage(
    app: &AppHandle,
    sup: &Supervisor,
    meta: &MetaCache,
    source: &str,
    chapter_url: &str,
) -> Result<serde_json::Value, String> {
    let params = serde_json::json!({ "source": source, "chapterUrl": chapter_url });
    match into_result(
        sup.request_avec_transport(app, source, "pages", params.clone(), DEEP_TIMEOUT)
            .await?,
    ) {
        Ok(r) => Ok(r),
        Err(erreur) if erreur.contains(REFUS_CHAPTERS_MOTEUR) => {
            let Some((manga, titre)) = meta.manga_du_chapitre(source, chapter_url) else {
                // Jamais vue cette session : rien à amorcer, et inventer un
                // manga à partir d'une URL serait pire que le refus.
                return Err(erreur);
            };
            tracing::info!(
                "pages amorcage chapters puis nouvel essai chapitre={}",
                crate::journal::hachage(chapter_url)
            );
            let chapitres = into_result(
                sup.request_avec_transport(
                    app,
                    source,
                    "chapters",
                    serde_json::json!({
                        "source": source,
                        "mangaUrl": manga.as_str(),
                        "mangaTitle": titre.as_str(),
                    }),
                    DEEP_TIMEOUT,
                )
                .await?,
            )?;
            meta.ecrire(Genre::Chapters, &format!("{source}|{manga}"), &chapitres);
            meta.indexer_chapitres(source, &manga, &titre, &chapitres);
            into_result(
                sup.request_avec_transport(app, source, "pages", params, DEEP_TIMEOUT)
                    .await?,
            )
        }
        Err(erreur) => Err(erreur),
    }
}

#[tauri::command]
pub async fn pages(
    app: AppHandle,
    sup: State<'_, Arc<Supervisor>>,
    pages_idx: State<'_, Arc<PageCache>>,
    meta: State<'_, Arc<MetaCache>>,
    source: String,
    chapter_url: String,
    refresh: Option<bool>,
) -> Result<serde_json::Value, String> {
    if chapter_url.trim().is_empty() {
        return Err("PARSE_ERROR: chapter_url vide".to_string());
    }
    // Aucune ligne n'existait pour `pages` jusqu'ici : on voyait la demande
    // arriver au moteur, jamais partir du cœur. Même empreinte que `details`,
    // l'URL du chapitre contient le titre.
    tracing::info!(
        "pages source={source} chapitre={}",
        crate::journal::hachage(chapter_url.trim())
    );
    let cle = format!("{}|{}", source.trim(), chapter_url.trim());
    if refresh != Some(true) {
        if let Some((at, donnees)) = meta.lire(Genre::Pages, &cle) {
            let mut result = donnees;
            if let Some(arr) = result.get_mut("pages").and_then(|v| v.as_array_mut()) {
                injecter_dims_et_token(arr, &pages_idx, source.trim());
            }
            tracing::info!("pages cache=hit age_s={}", (now_ms() - at) / 1000);
            return Ok(marquer_cache(result, at));
        }
    }
    let mut result =
        pages_avec_amorcage(&app, &sup, &meta, source.trim(), chapter_url.trim()).await?;
    // Cache la réponse du moteur **avant** l'injection : les jetons et les
    // dimensions vivent dans le registre de session (`pagecache`) et sont
    // refaits à chaque lecture — les figer ici les lierait à cette session.
    meta.ecrire(Genre::Pages, &cle, &result);
    if let Some(arr) = result.get_mut("pages").and_then(|v| v.as_array_mut()) {
        injecter_dims_et_token(arr, &pages_idx, source.trim());
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::{
        injecter_dims_et_token, into_result, latest_params, marquer_cache, nb_items,
        requested_rating, REFUS_CHAPTERS_MOTEUR,
    };
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

    /// Le marqueur de cache est la seule chose qu'une réponse servie par le
    /// disque ajoute : si son nom ou sa forme changeait sans que l'interface
    /// suive, le rafraîchissement d'arrière-plan ne partirait plus jamais —
    /// les données, elles, sembleraient bonnes.
    #[test]
    fn marquer_cache_ajoute_l_instant_sans_toucher_aux_donnees() {
        let v = serde_json::json!({ "chapters": [{ "url": "u" }], "manga": { "titre": "T" } });
        let marque = marquer_cache(v, 1_700_000_000_000);
        assert_eq!(marque["cacheAt"], serde_json::json!(1_700_000_000_000i64));
        assert!(marque["chapters"].is_array(), "les données restent");
        assert_eq!(marque["manga"]["titre"], "T");
        assert_eq!(
            marquer_cache(serde_json::json!([1, 2]), 5),
            serde_json::json!([1, 2]),
            "une réponse qui n'est pas un objet n'est pas transformée de force"
        );
    }

    #[test]
    fn nb_items_compte_la_liste_du_catalogue() {
        assert_eq!(nb_items(&serde_json::json!({ "list": [1, 2, 3] })), 3);
        assert_eq!(nb_items(&serde_json::json!({ "list": [] })), 0);
        assert_eq!(nb_items(&serde_json::json!({ "pages": [1] })), 0);
    }

    /// Une constante partagée entre deux langages a besoin d'un test qui lit
    /// l'autre fichier : le jour où `Server.kt` formule le refus autrement,
    /// l'amorçage de `pages` se désactiverait **en silence** (aucune erreur,
    /// aucun échec — juste le comportement d'avant le correctif A4).
    #[test]
    fn le_marqueur_de_refus_concorde_avec_le_moteur() {
        let chemin = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../../engine/src/main/kotlin/org/tatami/engine/Server.kt"
        );
        let source =
            std::fs::read_to_string(chemin).unwrap_or_else(|e| panic!("lire {chemin}: {e}"));
        assert!(
            source.contains(REFUS_CHAPTERS_MOTEUR),
            "le marqueur {REFUS_CHAPTERS_MOTEUR:?} n'apparaît plus dans Server.kt"
        );
        assert!(
            source.contains("unknown chapter url, call chapters first:"),
            "le message complet du refus a changé : relire `pages_avec_amorcage`"
        );
    }

    /// Le refus doit être reconnu dans la forme réelle de `into_result`
    /// (« {code}: {message} »), et un échec quelconque ne doit **pas** armer
    /// l'amorçage — sinon chaque `pages` en panne déclencherait une requête
    /// `chapters` de plus.
    #[test]
    fn le_refus_est_reconnu_dans_la_forme_reelle_du_message() {
        let refus = serde_json::json!({
            "ok": false,
            "error": { "code": "PARSE_ERROR", "message": "unknown chapter url, call chapters first: https://x/ch-1" }
        });
        let err = into_result(refus).expect_err("le refus est une erreur");
        assert!(err.contains(REFUS_CHAPTERS_MOTEUR), "{err}");
        let reseau = serde_json::json!({
            "ok": false,
            "error": { "code": "TIMEOUT", "message": "delai depasse" }
        });
        assert!(!into_result(reseau)
            .expect_err("timeout")
            .contains(REFUS_CHAPTERS_MOTEUR));
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
