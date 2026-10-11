//! Tâche 0.11 — sources protégées : fenêtre de résolution anti-bot.
//!
//! Le moteur répond `CHALLENGE_REQUIRED` avec l'URL à passer. On ouvre une
//! fenêtre WebView2 sur cette URL, l'utilisateur résout le challenge, puis on
//! récupère les cookies **HttpOnly** (`cf_clearance`) via
//! `WebviewWindow::cookies_for_url`, on les pousse dans le moteur avec
//! `setCookies` et on rejoue la requête.
//!
//! Limites connues et assumées (cf. PLAN.md 0.11) :
//! - la clearance est liée à l'IP et expire : il faut refaire la procédure ;
//! - la fenêtre utilise le profil WebView2 partagé, donc les cookies sont bien
//!   ceux du navigateur de l'app ;
//! - le user-agent du moteur (Firefox) diffère de celui de WebView2 (Chromium) :
//!   selon le site, la clearance peut être refusée. C'est mesurable en
//!   rejouant la requête après résolution.
//!
//! Détection automatique de la fin du challenge (mesuré le 2026-10-10 sur
//! TOONFR) : `setCookies` n'était appelé que si l'utilisateur revenait dans
//! l'app cliquer « C'est fait, réessayer ». Mesure : 2 challenges, 0 cookie
//! transmis (PLAN.md:1356-1359). Une tâche de surveillance interroge donc
//! les cookies toutes les 1,5 s et applique le même chemin dès que
//! `cf_clearance` apparaît. Le bouton manuel reste le secours.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Duration;

use serde_json::json;
use tauri::{
    AppHandle, Emitter, Manager, State, Url, WebviewUrl, WebviewWindowBuilder, WindowEvent,
};

use super::supervisor::Supervisor;

/// Cookie dont la présence marque la résolution du challenge.
const CF_CLEARANCE: &str = "cf_clearance";
/// Cadence de la surveillance automatique.
const SURVEILLE_PAS: Duration = Duration::from_millis(1500);
/// Au-delà, on arrête de surveiller : l'utilisateur a soit résolu (les
/// cookies sont partis), soit renoncé. Une surveillance sans fin garderait
/// une tâche éveillée par source ouverte.
const SURVEILLE_MAX: Duration = Duration::from_secs(180);
/// Delai d'attente de la reponse JS `navigator.userAgent` dans la fenetre.
const UA_DELAI: Duration = Duration::from_secs(5);
/// Delai avant d'ajouter une **seconde** lecture : un `cf_clearance`
/// qu'on retrouve identique est stable, donc reellement pose.
const SURVEILLE_CONFIRMATION: Duration = Duration::from_millis(1500);

/// Ce que la surveillance doit faire d'une lecture.
#[derive(Debug, PartialEq, Eq)]
enum Action {
    /// On continue d'attendre.
    Relire,
    /// Confirme : on pousse les cookies au moteur et on ferme la fenetre.
    Appliquer,
    /// La fenetre a disparu, ou le drapeau est deja leve : on s'arrete.
    Terminer,
}

/// Un cookie Cloudflare de ce genre signifie « le challenge TOURNE encore ».
///
/// **Mesure du 2026-10-11 sur HENTAIZONE, deux cycles identiques.** 3,3 s
/// apres l'ouverture de la fenetre, `cf_clearance` etait deja la (597
/// octets, expiration 2027, secure, httpOnly) **et** `cf_chl_rc_ni` (1
/// octet) l'accompagnait. La requete rejouee fut refusee dans les deux cas.
///
/// Donc `cf_clearance` seul ne prouve **rien** : il est pose des le
/// chargement de la page d'interstitial, avant que l'utilisateur ait
/// agi. Ce qui distingue « resolu » de « en cours » est la presence d'un
/// cookie `cf_chl_*` / `__cf_chl*`.
///
/// Fonction pure sur les **noms** seulement, testee sans navigateur.
fn challenge_en_cours<'a>(mut noms: impl Iterator<Item = &'a str>) -> bool {
    noms.any(|n| {
        let n = n.to_ascii_lowercase();
        n.starts_with("cf_chl_") || n.starts_with("__cf_chl")
    })
}

/// Ce que la surveillance voit dans la fenetre a un instant donne.
#[derive(Debug, PartialEq, Eq, Default)]
struct LectureCookies {
    /// Valeur du `cf_clearance`, si present. Comparee a la lecture
    /// precedente pour distinguer un nouveau cookie d'un cookie de passage.
    clearance: Option<String>,
    /// Cloudflare poursuit-il sa verification ?
    en_cours: bool,
}

/// Decide d'une lecture de surveillance, **sans effet de bord**.
///
/// Les conditions sont cumulatives :
/// - `clearance` : sans `cf_clearance`, rien a appliquer ;
/// - `!en_cours` : **le correctif du 2026-10-11**. Cloudflare pose
///   `cf_clearance` des le chargement de la page d'interstitial ; la seule
///   presence du cookie declenchait l'application, la requete rejouee etait
///   refusee, et la borne anti-boucle finissait par dire « la source
///   bloque l'application » alors que personne n'avait pu rien faire ;
/// - `nouveau` puis `confirme` : le cookie a change, et il a survecu a une
///   relecture ;
/// - `!drapeau_deja_leve` : le premier arrive applique.
fn action_surveillance(
    lecture: &LectureCookies,
    nouveau: bool,
    confirme: bool,
    drapeau_deja_leve: bool,
) -> Action {
    if lecture.clearance.is_none() {
        return Action::Relire;
    }
    if lecture.en_cours {
        return Action::Relire;
    }
    if !nouveau {
        return Action::Relire;
    }
    if !confirme {
        return Action::Relire;
    }
    if drapeau_deja_leve {
        return Action::Terminer;
    }
    Action::Appliquer
}

/// Surveillances actives : label → drapeau « cookies déjà appliqués ».
///
/// Le drapeau garantit **un seul déclenchement par fenêtre** : appliqués par
/// la surveillance OU par le bouton manuel, jamais les deux — un double
/// `setCookies` émettrait deux `challenge://resolu`, donc le front
/// rejouerait la requête deux fois. L'entrée est retirée à la fin de la
/// tâche, ce qui réarme la surveillance à la prochaine ouverture.
static SURVEILLANCES: OnceLock<Mutex<HashMap<String, Arc<AtomicBool>>>> = OnceLock::new();

/// Nombre de fenêtres ouvertes par source.
///
/// Instrumentation `TATAMI_CHALLENGE_DEBUG` : la mesure de boucle demande
/// « combien de fois `open_challenge` est déclenché pour une même action ».
static OUVERTURES: OnceLock<Mutex<HashMap<String, u64>>> = OnceLock::new();

fn compter_ouverture(source: &str) -> u64 {
    let carte = OUVERTURES.get_or_init(|| Mutex::new(HashMap::new()));
    let mut garde = match carte.lock() {
        Ok(g) => g,
        Err(_) => return 0,
    };
    let n = garde.entry(source.to_string()).or_insert(0);
    *n += 1;
    *n
}

/// Vrai quand l'instrumentation du flux challenge est demandée.
///
/// `TATAMI_CHALLENGE_DEBUG=1` — variable d'environnement lue à chaque appel
/// (une session de mesure la pose avant de lancer l'app). **Aucune valeur de
/// cookie n'est jamais journalisée** : seulement nom, domaine, chemin,
/// expiration, drapeaux et longueur de valeur.
fn debug_challenge() -> bool {
    std::env::var("TATAMI_CHALLENGE_DEBUG").is_ok()
}

fn surveillances() -> &'static Mutex<HashMap<String, Arc<AtomicBool>>> {
    SURVEILLANCES.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Enregistre la surveillance d'un label. Rend `None` (et n'insère rien) si
/// une surveillance vit déjà : deux tâches sur le même label pourraient
/// appliquer chacune de leur côté. `Some(drapeau)` = nouvelle surveillance à
/// démarrer, avec son drapeau neuf.
fn enregistrer_surveillance(label: &str) -> Option<Arc<AtomicBool>> {
    let mut carte = surveillances().lock().ok()?;
    if carte.contains_key(label) {
        return None;
    }
    let drapeau = Arc::new(AtomicBool::new(false));
    carte.insert(label.to_string(), Arc::clone(&drapeau));
    Some(drapeau)
}

/// Retire la surveillance, mais seulement si c'est encore la nôtre : une
/// fenêtre recréée dans la seconde suivant sa fermeture a pu installer une
/// nouvelle tâche entre-temps, et l'ancienne ne doit pas l'effacer.
fn retirer_surveillance(label: &str, drapeau: &Arc<AtomicBool>) {
    if let Ok(mut carte) = surveillances().lock() {
        if carte.get(label).is_some_and(|d| Arc::ptr_eq(d, drapeau)) {
            carte.remove(label);
        }
    }
}

/// Domaine à transmettre pour un cookie lu dans la webview.
///
/// Par défaut, l'hôte de l'URL de challenge. Mais si le cookie porte un
/// domaine **plus large** (`exemple.com` alors que l'URL visait
/// `www.exemple.com`), on garde le sien : il couvre les sous-domaines
/// (API, CDN) que l'hôte seul ne nomme pas. Un domaine sans rapport avec
/// l'hôte est écarté — l'hôte a au moins servi au challenge, l'autre n'a
/// rien prouvé.
///
/// Fonction pure, testée : la décision se prend ici, une fois, et pas
/// dispersée côté moteur.
fn domaine_effectif(cookie_domain: Option<&str>, host: &str) -> String {
    let h = host.trim_end_matches('.').to_ascii_lowercase();
    match cookie_domain {
        Some(d) => {
            // WebView2 rend les cookies de domaine avec un point initial
            // (« .exemple.com ») : on l'enlève, okhttp attend la forme
            // canonique sans point (cf. `Server.kt`).
            let d = d.trim_matches('.').to_ascii_lowercase();
            if !d.is_empty() && h.ends_with(&format!(".{d}")) {
                d
            } else {
                h
            }
        }
        None => h,
    }
}

/// Lit l'etat des cookies de la fenetre pour la surveillance.
///
/// Ne rend **que** ce que la decision a besoin : la valeur du
/// `cf_clearance` (jamais journalisee) et le fait que Cloudflare
/// pursue sa verification. Les valeurs restent en memoire.
async fn lire_cookies(app: &AppHandle, label: &str, url: &Url) -> Option<LectureCookies> {
    let fenetre = app.get_webview_window(label)?;
    let cookies = fenetre.cookies_for_url(url.clone()).ok()?;
    Some(LectureCookies {
        clearance: cookies
            .iter()
            .find(|c| c.name() == CF_CLEARANCE)
            .map(|c| c.value().to_string()),
        en_cours: challenge_en_cours(cookies.iter().map(|c| c.name())),
    })
}

/// Surveille la fenêtre jusqu'à `SURVEILLE_MAX`, par pas de `SURVEILLE_PAS`.
///
/// Dès que `cf_clearance` **apparaît** (ou change de valeur), les cookies
/// sont poussés au moteur, la fenêtre se ferme et le front est prévenu par
/// `challenge://resolu` — le même chemin que le bouton « C'est fait,
/// réessayer », sans le clic.
///
/// La comparaison à une valeur de départ est indispensable : le profil
/// WebView2 est partagé, donc un `cf_clearance` **périmé** peut déjà être
/// présent à l'ouverture. Le prendre pour une résolution bouclerait —
/// refuser, rouvrir, redéclencher sur le même vieux cookie.
fn surveiller_fenetre(
    app: AppHandle,
    sup: Arc<Supervisor>,
    source: String,
    url: Url,
    label: String,
    drapeau: Arc<AtomicBool>,
) {
    tauri::async_runtime::spawn(async move {
        // **Reference de depart.** Volontairement relue a chaque
        // confirmation : c'est ce qui distingue « cookie pose definitivement »
        // de « cookie de passage » (cf. `SURVEILLE_CONFIRMATION`).
        let mut deja_vu = lire_cookies(&app, &label, &url)
            .await
            .and_then(|l| l.clearance);
        // Instrumentation : l'UA **réel** de la webview, lu par JS, à
        // comparer à celui d'OkHttp. C'est la question de fond du flux — la
        // fenêtre est un Chromium (WebView2) à qui l'on a posé l'UA Firefox
        // du moteur. Si `navigator.userAgent` ne rend PAS la chaîne posée,
        // le réglage n'a pas pris et la clearance sera liée à une autre UA.
        //
        // `Sec-CH-UA` / `Sec-CH-UA-Platform` qu'un navigateur envoie ne sont
        // pas lisibles depuis l'app (il faudrait un proxy ou CDP) ; leur
        // source déclarée, `navigator.userAgentData`, l'est.
        if debug_challenge() {
            let ua_moteur = engine_user_agent(&sup).await;
            if let Some(fenetre) = app.get_webview_window(&label) {
                let source_js = source.clone();
                let _ = fenetre.eval_with_callback(
                    "JSON.stringify({ua: navigator.userAgent, \
                     platform: navigator.platform, \
                     uaDataMobile: (navigator.userAgentData || {}).mobile, \
                     uaDataBrands: ((navigator.userAgentData || {}).brands || [])})",
                    move |rendu: String| {
                        tracing::info!(
                            "challenge-debug: UA webview={rendu} | UA OkHttp={ua_moteur} \
                             source={source_js}"
                        );
                    },
                );
            }
        }
        let limite = tokio::time::Instant::now() + SURVEILLE_MAX;
        loop {
            tokio::time::sleep(SURVEILLE_PAS).await;
            if app.get_webview_window(&label).is_none() {
                break; // fermée (croix rouge ou application) : terminé
            }
            let lecture = match lire_cookies(&app, &label, &url).await {
                Some(l) => l,
                None => {
                    if tokio::time::Instant::now() >= limite {
                        break;
                    }
                    continue;
                }
            };
            // Rien de nouveau : on attend. C'est aussi le cas le plus
            // frequent (l'utilisateur reflechit devant la page).
            let nouveau = lecture.clearance.is_some() && lecture.clearance != deja_vu;

            // **Confirmation de stabilite.** Un cookie nouveau n'applique
            // rien tout seul : on le relit apres `SURVEILLE_CONFIRMATION`.
            let confirme = if nouveau {
                let attendu = lecture.clearance.clone();
                tokio::time::sleep(SURVEILLE_CONFIRMATION).await;
                if app.get_webview_window(&label).is_none() {
                    break;
                }
                lire_cookies(&app, &label, &url)
                    .await
                    .and_then(|l| l.clearance)
                    == attendu
            } else {
                false
            };
            if nouveau && confirme {
                // Reference mise a jour avant d'appliquer : un eventuel
                // relancement de la surveillance ne doit pas rejouer le meme
                // cookie comme une nouvelle apparition.
                deja_vu = lecture.clearance.clone();
            }

            match action_surveillance(&lecture, nouveau, confirme, drapeau.load(Ordering::SeqCst)) {
                Action::Relire => {
                    if tokio::time::Instant::now() >= limite {
                        break;
                    }
                    continue;
                }
                Action::Terminer => break,
                Action::Appliquer => {
                    // Le drapeau est leve **avant** l'application : deux ticks
                    // ne peuvent pas appliquer deux fois, et le bouton manuel
                    // qui gagnerait la course verrait le drapeau leve.
                    if drapeau.swap(true, Ordering::SeqCst) {
                        break;
                    }
                    match appliquer_cookies(&app, &sup, &source, &url).await {
                        Ok(n) => {
                            tracing::info!(
                                "challenge: {n} cookie(s) applique(s) automatiquement source={source}"
                            );
                            let _ = app.emit(
                                "challenge://resolu",
                                json!({ "source": source, "cookies": n }),
                            );
                        }
                        Err(e) => {
                            tracing::warn!("challenge: application automatique echouee : {e}");
                        }
                    }
                    break;
                }
            }
        }
        retirer_surveillance(&label, &drapeau);
    });
}

fn challenge_label(source: &str) -> String {
    format!("challenge-{}", super::download::sanitize(source))
}

/// User-agent du moteur, appris au `hello`. Si le champ est vide (démarrage
/// tout juste, ou `hello` pas encore passé), on redemande : la fenêtre de
/// vérification ne doit pas dépendre du timing du superviseur.
async fn engine_user_agent(sup: &Arc<Supervisor>) -> String {
    let cached = sup.user_agent().await;
    if !cached.is_empty() {
        return cached;
    }
    sup.request("hello", serde_json::json!({}), Duration::from_secs(8))
        .await
        .ok()
        .and_then(|v| {
            v.get("result")
                .and_then(|r| r.get("defaultUserAgent"))
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
        })
        .unwrap_or_default()
}

/// Ouvre la fenêtre de résolution sur l'URL qui a renvoyé le challenge.
#[tauri::command]
pub async fn open_challenge(
    app: AppHandle,
    sup: State<'_, Arc<Supervisor>>,
    source: String,
    url: String,
) -> Result<(), String> {
    let parsed: Url = url
        .parse()
        .map_err(|e| format!("URL de challenge invalide ({url}) : {e}"))?;
    if parsed.scheme() != "http" && parsed.scheme() != "https" {
        return Err(format!("schéma non autorisé : {}", parsed.scheme()));
    }
    let label = challenge_label(&source);
    if debug_challenge() {
        tracing::info!(
            "challenge-debug: open_challenge source={source} label={label} appel n°{}",
            compter_ouverture(&source)
        );
    }
    // La fenêtre de vérification n'était journalisée **nulle part**. Mesuré le
    // 2026-10-09 : `setCookies` a été appelé 0 fois sur 6 tentatives, et rien ne
    // distinguait « la fenêtre ne s'est pas ouverte » de « elle s'est ouverte et
    // l'utilisateur n'a pas cliqué » de « elle s'est ouverte, il a cliqué, et les
    // cookies étaient vides ». Trois causes, trois remèdes : il fallait une trace.
    tracing::info!("challenge: ouverture demandée source={source} label={label}");
    if let Some(existing) = app.get_webview_window(&label) {
        // Déclenchée deux fois : on réutilise la fenêtre. Refermer puis recréer
        // échoue, car `close()` est asynchrone et le label existe encore.
        // Mesuré le 2026-10-09 : ce chemin rendait `Ok(())` sans rien laisser au
        // journal. Symptôme vu de l'extérieur : « la fenêtre revient » alors
        // qu'elle ne se recrée pas — la réutilisation est silencieuse, donc la
        // fenêtre semble réapparaître sans jamais être refermée.
        tracing::info!(
            "challenge: fenêtre DÉJÀ ouverte, réutilisation label={label} (rien n'est recréé)"
        );
        // **Re-naviguer, ne pas se contenter de focaliser.** La page peut
        // être restée sur une vérification en boucle (mesuré sur TOONFR) :
        // refocaliser la même fenêtre ne relance rien, l'utilisateur croit
        // « réessayer » alors que la page n'a pas bougé. `navigate` recharge
        // l'URL du challenge dans la fenêtre existante — plus simple que
        // fermer/recréer, dont `close()` est asynchrone et laisse le label
        // en place (cf. le commentaire d'origine).
        existing
            .navigate(parsed.clone())
            .map_err(|e| format!("re-navigation vers l'URL de vérification : {e}"))?;
        let _ = existing.set_focus();
        return Ok(());
    }
    // **Aucune usurpation d'UA.** La fenetre garde celui du vrai moteur de
    // rendu (Chromium/WebView2). C'etait une erreur : on lui imposait
    // l'UA `Firefox/116 / Linux` du moteur, alors que `platform` valait Win32
    // et que les client hints designaient Edge WebView2 — trois signaux
    // contradictoires dans une meme page.
    //
    // A/B mesure le 2026-10-11 sur HENTAIZONE, deux configurations :
    // - UA usurpe : le widget Turnstile **boucle**, ne produit jamais de
    //   reponse, la fenetre elle-meme obtient 403 « Just a moment... » ;
    // - UA reel : le challenge **aboutit**, la fenetre affiche le site.
    //
    // Le controle qui rend la comparaison valide : le meme site, dans le
    // navigateur de l'utilisateur, fonctionne — la protection n'a pas
    // change, c'est nous qui nous presenteions mal.
    WebviewWindowBuilder::new(&app, &label, WebviewUrl::External(parsed.clone()))
        .title(format!("Vérification — {source}"))
        .inner_size(980.0, 860.0)
        .build()
        .map_err(|e| format!("ouverture de la fenêtre : {e}"))?;
    let fenetre = app
        .get_webview_window(&label)
        .ok_or("la fenêtre de vérification vient de se fermer")?;
    // Croix rouge : l'utilisateur renonce. Rien ne suivait cette fermeture —
    // l'état restait « vérification en cours » (boutons désactivés, fenêtre
    // fantôme dans le bandeau). On le dit au front, qui remet son
    // indicateur à zéro ; le bandeau, lui, reste : la source est toujours
    // bloquée et « Résoudre la vérification » ouvre une fenêtre neuve.
    let app_pour_fermeture = app.clone();
    let source_pour_fermeture = source.clone();
    fenetre.on_window_event(move |evenement| {
        if let WindowEvent::CloseRequested { .. } = evenement {
            let _ = app_pour_fermeture.emit(
                "challenge://fermee",
                serde_json::json!({ "source": source_pour_fermeture }),
            );
        }
    });
    // Surveillance automatique de la fin du challenge. `None` = une
    // surveillance vit déjà sur ce label (fenêtre recréée aussitôt après sa
    // fermeture) : on n'en démarre pas une seconde.
    if let Some(drapeau) = enregistrer_surveillance(&label) {
        surveiller_fenetre(
            app.clone(),
            Arc::clone(&*sup),
            source.clone(),
            parsed.clone(),
            label.clone(),
            drapeau,
        );
    }
    Ok(())
}

/// Après résolution : récupère les cookies de la fenêtre, les envoie au
/// moteur, ferme la fenêtre. Renvoie le nombre de cookies transmis.
#[tauri::command]
pub async fn solve_challenge(
    app: AppHandle,
    sup: State<'_, Arc<Supervisor>>,
    source: String,
    url: String,
) -> Result<usize, String> {
    let label = challenge_label(&source);
    if app.get_webview_window(&label).is_none() {
        return Err("la fenêtre de vérification est fermée".to_string());
    }
    let parsed: Url = url
        .parse()
        .map_err(|e| format!("URL de challenge invalide : {e}"))?;
    // Un seul déclenchement par fenêtre : si la surveillance automatique a
    // déjà appliqué les cookies, rejouer ici enverrait un second
    // `challenge://resolu` au front, donc une seconde relecture.
    if let Some(drapeau) = surveillances()
        .lock()
        .ok()
        .and_then(|carte| carte.get(&label).cloned())
    {
        if drapeau.swap(true, Ordering::SeqCst) {
            return Err("la vérification a déjà été appliquée automatiquement".to_string());
        }
    }
    appliquer_cookies(&app, &sup, &source, &parsed).await
}

/// Lit les cookies de la fenêtre, les pousse au moteur, ferme la fenêtre.
///
/// Partagée par le bouton manuel (`solve_challenge`) et par la surveillance
/// automatique (`surveiller_fenetre`) — un seul des deux passe, gardé par le
/// drapeau de `SURVEILLANCES`.
/// User-Agent **réel** de la fenêtre de résolution, lu par JavaScript.
///
/// **Pourquoi il faut le lire plutôt que le supposer.** La clearance
/// Cloudflare est délivrée pour un agent donné, et c'est **la fenêtre** qui
/// l'obtient. Mesure du 2026-10-11 sur HENTAIZONE : le challenge passait
/// dans la fenêtre (UA Chromium/WebView2) mais le rejeu était refusé — parce
/// que `appliquer_cookies` envoyait l'UA du moteur (`FIREFOX_DESKTOP`), donc
/// une autre identité que celle qui avait résolu.
///
/// `None` si la fenêtre a disparu ou ne répond pas : l'appelant retombe
/// alors sur l'UA du moteur, ce qui est le comportement d'avant, pas une
/// panne.
async fn ua_de_la_fenetre(app: &AppHandle, label: &str) -> Option<String> {
    let fenetre = app.get_webview_window(label)?;
    // Le callback est une closure `Fn` — il peut donc etre appele plusieurs
    // fois, alors que `Sender::send` consomme. D'ou la prise unique sous
    // verrou : le premier rendu gagne, les suivants ne trouvent plus rien.
    let (tx, rx) = tokio::sync::oneshot::channel();
    let envoi = std::sync::Arc::new(std::sync::Mutex::new(Some(tx)));
    fenetre
        .eval_with_callback("navigator.userAgent", move |rendu: String| {
            if let Ok(mut slot) = envoi.lock() {
                if let Some(emmetteur) = slot.take() {
                    let _ = emmetteur.send(rendu);
                }
            }
        })
        .ok()?;
    // `timeout` -> Result<Result<String, RecvError>, Elapsed> ; on ecarte les deux
    // couches d'echec avec deux `.ok()` — un seul `?` ne peut pas les
    // porter tous les deux.
    let ua = tokio::time::timeout(UA_DELAI, rx).await.ok()?.ok()?;
    let ua = ua.trim().to_string();
    (!ua.is_empty()).then_some(ua)
}

async fn appliquer_cookies(
    app: &AppHandle,
    sup: &Arc<Supervisor>,
    source: &str,
    parsed: &Url,
) -> Result<usize, String> {
    let label = challenge_label(source);
    let window = app
        .get_webview_window(&label)
        .ok_or("la fenêtre de vérification est fermée")?;
    let host = parsed
        .host_str()
        .ok_or("hôte introuvable dans l'URL de challenge")?
        .to_string();

    tracing::info!("challenge: lecture des cookies source={source} label={label}");
    let cookies = window
        .cookies_for_url(parsed.clone())
        .map_err(|e| format!("lecture des cookies : {e}"))?;

    let payload: Vec<serde_json::Value> = cookies
        .iter()
        .filter(|c| !c.name().is_empty() && !c.value().is_empty())
        // **On ne renvoie pas les marqueurs d'un challenge en cours.**
        // Mesure du 2026-10-11 sur HENTAIZONE, apres resolution reussie :
        // le rejeu partait avec `cf_clearance` ET `cf_chl_rc_ni`, et
        // Cloudflare repondait 403 `cf-mitigated: challenge`. Ce second
        // cookie dit « je teste encore ce client » — le renvoyer apres
        // reussite invite a un nouveau challenge. C'est un cookie a nous :
        // la fenetre l'a pose pendant le test, il ne sert plus a rien.
        .filter(|c| !challenge_en_cours(std::iter::once(c.name())))
        .map(|c| {
            let mut cookie = json!({
                "name": c.name(),
                "value": c.value(),
                "domain": domaine_effectif(c.domain(), &host),
                "path": c.path().unwrap_or("/"),
            });
            if let Some(secure) = c.secure() {
                cookie["secure"] = json!(secure);
            }
            if let Some(http_only) = c.http_only() {
                cookie["httpOnly"] = json!(http_only);
            }
            // Expiration réelle, lue dans la webview. Absente = cookie de
            // session : le moteur choisit alors son défaut. On ne force plus
            // `maintenant + 365 j`, qui gardait en vie un cookie périmé.
            if let Some(fin) = c.expires_datetime() {
                cookie["expiresAt"] = json!(fin.unix_timestamp() * 1000);
            }
            cookie
        })
        .collect();

    tracing::info!(
        "challenge: {n} cookie(s) lu(s) sur {host}, {k} retenu(s)",
        n = cookies.len(),
        host = host,
        k = payload.len()
    );
    if debug_challenge() {
        // Inventaire cookie par cookie — **nom, domaine, chemin, expiration,
        // drapeaux, longueur** : jamais la valeur (règle projet).
        for c in &cookies {
            tracing::info!(
                "challenge-debug: cookie nom={} domaine={:?} chemin={:?} \
                 expiration={:?} secure={:?} httpOnly={:?} longueurValeur={}",
                c.name(),
                c.domain(),
                c.path(),
                c.expires_datetime().map(|d| d.unix_timestamp()),
                c.secure(),
                c.http_only(),
                c.value().len(),
            );
        }
        tracing::info!(
            "challenge-debug: cf_clearance présent={} host={host}",
            cookies.iter().any(|c| c.name() == CF_CLEARANCE)
        );
    }
    if payload.is_empty() {
        return Err(
            "aucun cookie pour ce site : la vérification n'est probablement pas terminée"
                .to_string(),
        );
    }

    // **L'agent qui a résolu, pas celui du moteur.** La clearance est liée à
    // l'identité de la fenêtre ; envoyer celle d'OkHttp fait refuser le
    // rejeu. Repli sur l'UA du moteur si la fenêtre ne répond plus.
    let ua = match ua_de_la_fenetre(app, &label).await {
        Some(u) => u,
        None => engine_user_agent(sup).await,
    };
    tracing::info!(
        "challenge-debug: rejeu avec userAgent={}",
        if debug_challenge() {
            ua.as_str()
        } else {
            "<instrumente>"
        }
    );
    sup.request(
        "setCookies",
        json!({
            "source": source,
            "cookies": payload,
            "userAgent": ua,
        }),
        Duration::from_secs(20),
    )
    .await
    .map_err(|e| format!("setCookies : {e}"))?;

    let _ = window.close();
    Ok(payload.len())
}

/// Ferme la fenêtre de vérification sans appliquer les cookies.
#[tauri::command]
pub async fn cancel_challenge(app: AppHandle, source: String) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(&challenge_label(&source)) {
        window.close().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        action_surveillance, challenge_en_cours, challenge_label, domaine_effectif,
        enregistrer_surveillance, retirer_surveillance, surveillances, Action, LectureCookies,
    };

    /// Le cookie de clearance, tel que mesure le 2026-10-11 sur HENTAIZONE.
    fn lecture_resolue() -> LectureCookies {
        LectureCookies {
            clearance: Some("clearance-597-o".to_string()),
            en_cours: false,
        }
    }

    /// **Le defaut reel du 2026-10-11, mesure dans l'app.** A 3,3 s apres
    /// l'ouverture de la fenetre, `cf_clearance` etait deja la (597 octets,
    /// expiration 2027) et `cf_chl_rc_ni` l'accompagnait : le challenge
    /// **tourne encore**. La requete rejouee fut refusee dans les deux
    /// cycles mesures. Appliquer sur la seule presence du cookie renvoyait
    /// l'utilisateur sur la page de la source et faisait monter le compteur
    /// jusqu'a « la source bloque l'application ».
    #[test]
    fn un_cookie_pose_pendant_le_challenge_ne_declenche_pas() {
        let lecture = LectureCookies {
            clearance: Some("clearance-597-o".to_string()),
            en_cours: true,
        };
        assert_eq!(
            action_surveillance(&lecture, true, true, false),
            Action::Relire,
            "le challenge tourne encore : appliquer renverrait l'utilisateur sur la page source"
        );
    }

    /// La contre-epreuve : le declencheur d'hier (cookie present, aucune
    /// question posee) appliait. C'est ce qui prouve que le test
    /// ci-dessus mord.
    #[test]
    fn contre_epreuve_le_declencheur_dhier_appliquait_malgre_le_challenge() {
        let lecture = LectureCookies {
            clearance: Some("clearance-597-o".to_string()),
            en_cours: true,
        };
        let ancien_decide = if lecture.clearance.is_some() {
            Action::Appliquer
        } else {
            Action::Relire
        };
        assert_eq!(
            ancien_decide,
            Action::Appliquer,
            "l'ancien code appliait sur le seul fait qu'un cookie soit present"
        );
        assert_ne!(
            action_surveillance(&lecture, true, true, false),
            ancien_decide,
            "la fonction d'aujourd'hui doit refuser ce cas"
        );
    }

    /// `cf_chl_rc_ni` est le marqueur mesure ; les autres noms appartiennent
    /// a la meme famille (nonce, jeton de re-verification).
    #[test]
    fn les_marqueurs_de_challenge_en_cours_sont_reconnus() {
        assert!(challenge_en_cours(
            ["cf_clearance", "cf_chl_rc_ni"].into_iter()
        ));
        assert!(challenge_en_cours(["__cf_chl_tk"].into_iter()));
        assert!(
            challenge_en_cours(["CF_CHL_RC_I"].into_iter()),
            "la casse ne doit pas masquer le marqueur"
        );
        // Un site normal, ou Cloudflare a fini : rien ne doit bloquer.
        assert!(!challenge_en_cours(
            ["cf_clearance", "sessionid"].into_iter()
        ));
        assert!(!challenge_en_cours([].into_iter()));
    }

    #[test]
    fn un_challenge_resolu_declenche_une_fois() {
        assert_eq!(
            action_surveillance(&lecture_resolue(), true, true, false),
            Action::Appliquer,
            "clearance + pas de challenge en cours + nouveau + stable + drapeau libre"
        );
        assert_eq!(
            action_surveillance(&lecture_resolue(), true, true, true),
            Action::Terminer,
            "drapeau deja leve (bouton manuel passe avant) : on s'arrete"
        );
    }

    #[test]
    fn rien_ne_declenche_sans_clearance_ou_si_inchange() {
        let vide = LectureCookies::default();
        assert_eq!(
            action_surveillance(&vide, true, true, false),
            Action::Relire,
            "pas de cf_clearance : rien a appliquer"
        );
        assert_eq!(
            action_surveillance(&lecture_resolue(), false, false, false),
            Action::Relire,
            "cookie inchange : on attend"
        );
        assert_eq!(
            action_surveillance(&lecture_resolue(), true, false, false),
            Action::Relire,
            "cookie nouveau mais non confirme : on relit"
        );
    }

    #[test]
    fn libelle_de_challenge_est_sanitise() {
        assert_eq!(challenge_label("MANGADEX"), "challenge-MANGADEX");
        // Un seul nom de dossier : pas de « .. » qui traverse quoi que ce soit.
        let label = challenge_label("site/../evil");
        assert!(label.starts_with("challenge-site_"));
        assert!(!label.contains('/'));
        assert_eq!(challenge_label(".."), "challenge-sans-titre");
    }

    /// **Un seul déclenchement par fenêtre.** La surveillance automatique et
    /// le bouton manuel partagent le même drapeau : si les deux passaient,
    /// `setCookies` partirait deux fois et le front rejouerait la requête
    /// deux fois. Premier venu gagne ; le second voit le drapeau déjà levé.
    #[test]
    fn une_seule_surveillance_par_label_et_un_seul_declenchement() {
        // Label unique : la carte est un static partagé par tous les tests.
        let label = "challenge-test-registre";
        let drapeau = enregistrer_surveillance(label).expect("première surveillance");
        let second = enregistrer_surveillance(label);
        assert!(
            second.is_none(),
            "une seconde surveillance ne doit pas démarrer"
        );
        assert!(
            !drapeau.swap(true, std::sync::atomic::Ordering::SeqCst),
            "le premier passage lève le drapeau"
        );
        assert!(
            drapeau.swap(true, std::sync::atomic::Ordering::SeqCst),
            "le second trouve le drapeau déjà levé : il s'arrête"
        );
        retirer_surveillance(label, &drapeau);
        let rearmee = enregistrer_surveillance(label)
            .expect("après arrêt de la tâche, la prochaine fenêtre réarme la surveillance");
        // Nettoyage : les tests sont parallèles et la carte est partagée.
        // C'est bien la **nouvelle** entrée qu'il faut retirer : `ptr_eq`
        // protège justement l'ancienne de cet effacement.
        retirer_surveillance(label, &rearmee);
        assert!(
            surveillances().lock().unwrap().get(label).is_none(),
            "le label ne doit rien laisser derrière lui"
        );
    }

    /// Le cookie lu porte parfois un domaine plus large que l'hôte du
    /// challenge. Le garder, c'est couvrir les sous-domaines (API, CDN) ;
    /// forcer l'hôte les perdrait en silence.
    #[test]
    fn domaine_effectif_garde_le_sur_domaine_du_cookie() {
        // Challenge résolu sur www, cookie posé pour le domaine racine.
        assert_eq!(
            domaine_effectif(Some(".exemple.com"), "www.exemple.com"),
            "exemple.com"
        );
        // Même hôte : rien à élargir.
        assert_eq!(
            domaine_effectif(Some("exemple.com"), "exemple.com"),
            "exemple.com"
        );
        // Domaine sans rapport : on retombe sur l'hôte du challenge.
        assert_eq!(
            domaine_effectif(Some("autre.test"), "exemple.com"),
            "exemple.com"
        );
        // Aucun domaine fourni : l'hôte.
        assert_eq!(domaine_effectif(None, "exemple.com"), "exemple.com");
        // Casse et points de bord, tels que WebView2 les rend.
        assert_eq!(
            domaine_effectif(Some(".Exemple.COM."), "WWW.exemple.com"),
            "exemple.com"
        );
    }
}
