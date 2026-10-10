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

/// Surveillances actives : label → drapeau « cookies déjà appliqués ».
///
/// Le drapeau garantit **un seul déclenchement par fenêtre** : appliqués par
/// la surveillance OU par le bouton manuel, jamais les deux — un double
/// `setCookies` émettrait deux `challenge://resolu`, donc le front
/// rejouerait la requête deux fois. L'entrée est retirée à la fin de la
/// tâche, ce qui réarme la surveillance à la prochaine ouverture.
static SURVEILLANCES: OnceLock<Mutex<HashMap<String, Arc<AtomicBool>>>> = OnceLock::new();

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

/// Valeur du cookie `cf_clearance` visible pour `url`, s'il existe.
///
/// La valeur reste en mémoire : elle n'est **jamais** journalisée (règle
/// projet sur les cookies).
async fn cf_clearance_courant(app: &AppHandle, label: &str, url: &Url) -> Option<String> {
    let fenetre = app.get_webview_window(label)?;
    let cookies = fenetre.cookies_for_url(url.clone()).ok()?;
    cookies
        .into_iter()
        .find(|c| c.name() == CF_CLEARANCE)
        .map(|c| c.value().to_string())
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
        let deja_vu = cf_clearance_courant(&app, &label, &url).await;
        let limite = tokio::time::Instant::now() + SURVEILLE_MAX;
        loop {
            tokio::time::sleep(SURVEILLE_PAS).await;
            if app.get_webview_window(&label).is_none() {
                break; // fermée (croix rouge ou application) : terminé
            }
            let actuel = cf_clearance_courant(&app, &label, &url).await;
            if actuel.is_none() || actuel == deja_vu {
                if tokio::time::Instant::now() >= limite {
                    break;
                }
                continue;
            }
            // Un cookie **nouveau** (absent au départ, ou de valeur
            // différente) : le challenge vient d'être résolu.
            if drapeau.swap(true, Ordering::SeqCst) {
                // Le bouton manuel a appliqué entre deux ticks.
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
    let mut builder = WebviewWindowBuilder::new(&app, &label, WebviewUrl::External(parsed.clone()))
        .title(format!("Vérification — {source}"))
        .inner_size(980.0, 860.0);
    // Même user-agent que le moteur : la clearance est émise pour un agent
    // donné, si les deux diffèrent le site peut la refuser ensuite.
    let ua = engine_user_agent(&sup).await;
    if !ua.is_empty() {
        builder = builder.user_agent(&ua);
    }
    builder
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
    if payload.is_empty() {
        return Err(
            "aucun cookie pour ce site : la vérification n'est probablement pas terminée"
                .to_string(),
        );
    }

    sup.request(
        "setCookies",
        json!({
            "source": source,
            "cookies": payload,
            // Même agent que celui de la fenêtre de résolution.
            "userAgent": engine_user_agent(sup).await,
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
        challenge_label, domaine_effectif, enregistrer_surveillance, retirer_surveillance,
        surveillances,
    };

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
