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
    AppHandle, Emitter, Manager, State, Url, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
    WindowEvent,
};

use super::supervisor::Supervisor;

/// Cookie dont la présence marque la résolution du challenge.
const CF_CLEARANCE: &str = "cf_clearance";
const ERREUR_DOMAINE_CHALLENGE: &str = "CHALLENGE_DOMAIN_UNAVAILABLE";
/// Cadence de la surveillance automatique.
const SURVEILLE_PAS: Duration = Duration::from_millis(1500);
/// Au-delà, on arrête de surveiller : l'utilisateur a soit résolu (les
/// cookies sont partis), soit renoncé. Une surveillance sans fin garderait
/// une tâche éveillée par source ouverte.
const SURVEILLE_MAX: Duration = Duration::from_secs(180);
/// Delai d'attente de la reponse JS `navigator.userAgent` dans la fenetre.
/// Delai d'attente de la reponse JS `navigator.userAgent` dans la fenetre.
const UA_DELAI: Duration = Duration::from_secs(5);
/// Delai d'attente de la reponse JS d'etat de page.
const PAGE_DELAI: Duration = Duration::from_secs(5);

/// Ce que la fenetre montre, tel que lu par JavaScript.
///
/// **Pourquoi c'est la decision qui manquait.** Le 2026-10-11, la surveillance
/// a declenche **3,4 s** apres l'ouverture de la fenetre, sans qu'aucun humain
/// n'ait clique : Cloudflare avait pose un `cf_clearance` sur sa page
/// d'interstitielle, et l'application a annonce « 7 cookies passes ». Le rejeu
/// a ete ensuite refuse, et l'utilisateur croyait avoir ete resolu.
///
/// Aucun marqueur `cf_chl_*` n'etait present sur cette reponse : la garde
/// `challenge_en_cours` ne pouvait pas voir le piege. Ce qui le voit, c'est la
/// page : tant que c'est l'interstitielle, rien n'a ete resolu.
#[derive(Debug, PartialEq, Eq, Default)]
struct PageFenetre {
    /// `true` tant que Cloudflare affiche sa page de verification.
    interstitiel: bool,
}

/// Fonction **pure** : la fenetre montre-t-elle encore la page de verification ?
///
/// Trois signaux, parce qu'un seul se trompe :
/// - le titre, traduit ou non (« Just a moment... » / « Un instant... ») ;
/// - le formulaire `#challenge-form` de l'interstitielle ;
/// - le script `/cdn-cgi/challenge-platform/`, charge par cette page.
///
/// Un corps HTML qui est la page de verification de Cloudflare, et non le contenu.
///
/// **Pourquoi ce critere, et pas le script seul.** Mesure le 2026-10-14 :
/// `/cdn-cgi/challenge-platform/` est present aussi sur **sushiscan.net**, qui
/// n'est pourtant pas protege. Ce marqueur seul refuserait donc un catalogue
/// sain. Le critere retenu exige les **deux** : le titre « Just a moment… » et
/// la configuration `_cf_chl_opt`, qui n'apparaissent ensemble que sur la page
/// de verification (mesure : Astral 403 les porte, Sushiscan 200 ne les porte
/// pas).
///
/// Ce garde ne voit que le HTML : il ne remplace pas `reste_interstitiel`, qui
/// lit la fenetre. Il empeche qu'un corps de verification soit injecte comme s'il
/// etait le contenu.
fn est_page_de_verification(html: &str) -> bool {
    let bas = html.to_ascii_lowercase();
    let titre_verification = bas
        .find("<title>")
        .map(|i| {
            bas[i + "<title>".len()..]
                .trim_start()
                .starts_with("just a moment")
        })
        .unwrap_or(false);
    titre_verification && bas.contains("_cf_chl_opt")
}

/// Testee sans navigateur, sur des formes reelles.
fn reste_interstitiel(titre: &str, formulaire: bool, script: bool) -> bool {
    if formulaire || script {
        return true;
    }
    let t = titre.to_ascii_lowercase();
    t.contains("just a moment")
        || t.contains("un instant")
        || t.contains("attention requise")
        || t.contains("checking your browser")
}

/// Lit l'etat de la page affichee par la fenetre de resolution.
async fn lire_page(app: &AppHandle, label: &str) -> Option<PageFenetre> {
    let fenetre = app.get_webview_window(label)?;
    let (tx, rx) = tokio::sync::oneshot::channel();
    let envoi = std::sync::Arc::new(std::sync::Mutex::new(Some(tx)));
    let js = "JSON.stringify({t: document.title, \
f: !!document.querySelector('#challenge-form'), \
s: !!document.querySelector('script[src*=\"/cdn-cgi/challenge-platform/\"]')})";
    fenetre
        .eval_with_callback(js, move |rendu: String| {
            if let Ok(mut slot) = envoi.lock() {
                if let Some(prise) = slot.take() {
                    let _ = prise.send(rendu);
                }
            }
        })
        .ok()?;
    let brut = tokio::time::timeout(PAGE_DELAI, rx).await.ok()?.ok()?;
    let lu: serde_json::Value = serde_json::from_str(&brut).ok()?;
    Some(PageFenetre {
        interstitiel: reste_interstitiel(
            lu.get("t").and_then(|v| v.as_str()).unwrap_or(""),
            lu.get("f").and_then(|v| v.as_bool()).unwrap_or(false),
            lu.get("s").and_then(|v| v.as_bool()).unwrap_or(false),
        ),
    })
}

/// Lit l'etat des cookies de la fenetre pour la surveillance.
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
            // **La page prime sur le cookie.** Tant que la fenetre affiche
            // encore l'interstitielle de Cloudflare, aucun cookie n'est gagne :
            // le `cf_clearance` pose a ce moment-la est celui du piege, et
            // l'appliquer annonçait « 7 cookies passes » alors que personne
            // n'avait rien resolu (mesure du 2026-10-11 : declenchement a
            // t+3,4 s, sans aucun clic de l'utilisateur).
            //
            // Une page illisible n'est pas une page resolue : on continue
            // d'attendre plutot que d'appliquer sur une presomption.
            match lire_page(&app, &label).await {
                Some(page) if page.interstitiel => {
                    if tokio::time::Instant::now() >= limite {
                        break;
                    }
                    continue;
                }
                None => {
                    if tokio::time::Instant::now() >= limite {
                        break;
                    }
                    continue;
                }
                _ => {}
            }

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

fn normaliser_domaine_challenge(domaine: &str) -> Option<String> {
    if domaine.is_empty()
        || domaine.trim() != domaine
        || domaine
            .chars()
            .any(|c| matches!(c, '/' | '\\' | '@' | ':' | '?' | '#'))
    {
        return None;
    }
    let url = Url::parse(&format!("https://{domaine}/")).ok()?;
    let hote = url.host_str()?.trim_end_matches('.').to_ascii_lowercase();
    (!hote.is_empty()).then_some(hote)
}

/// Un hôte ne vaut pas « autorisation de naviguer » s'il est privé ou local.
///
/// Nécessaire depuis le 2026-10-13 : la fenêtre de vérification pouvait être
/// demandée sur un hôte **d'image**, c'est-à-dire un hôte choisi par le site
/// et non déclaré par le parseur. Or une source hostile — ou simplement
/// compromise — peut mettre `127.0.0.1`, `169.254.169.254` (métadonnées d'instance)
/// ou un nom interne dans ses URL de pages. Le navigateur de la fenêtre
/// joignait alors des cibles du réseau local avec les droits de l'utilisateur.
///
/// Le refus porte sur la forme littérale de l'hôte. Un nom public qui
/// *pointerait* vers une adresse privée (DNS rebinding) passe : c'est le même
/// angle mort que le contrôle déjà posé par le moteur sur les URL d'images,
/// et le corriger ici sans corriger là donnerait une fausse assurance.
fn hote_public(hote: &str) -> bool {
    let h = hote.trim().trim_end_matches('.').to_ascii_lowercase();
    if h.is_empty() {
        return false;
    }
    if h == "localhost"
        || h.ends_with(".localhost")
        || h.ends_with(".local")
        || h.ends_with(".internal")
        || h == "metadata.google.internal"
    {
        return false;
    }
    // IPv4 littérale : on rejette les plages non publiques.
    let octets: Vec<u8> = h
        .split('.')
        .filter_map(|part| part.parse::<u8>().ok())
        .collect();
    if octets.len() == 4 && h.split('.').count() == 4 {
        let [a, b, c, _] = [octets[0], octets[1], octets[2], octets[3]];
        let prive = a == 0
            || a == 10
            || a == 127
            || (a == 169 && b == 254)
            || (a == 172 && (16..=31).contains(&b))
            || (a == 192 && b == 168)
            || (a == 100 && (64..=127).contains(&b)) // CGNAT 100.64/10
            || (a >= 224); // multicast et au-delà
        let _ = c;
        if prive {
            return false;
        }
    }
    true
}

/// Autorise le hôte d'une **URL d'image** déjà demandée par le moteur.
///
/// Le contrat est volontairement étroit, et c'est la décision humaine du
/// 2026-10-13 : on n'ouvre la fenêtre que sur un hôte que **nous avons
/// réellement demandé** pour cette source, jamais sur un hôte arbitraire
/// annoncé par le site. Concrètement trois conditions, toutes vérifiées ici :
///
///   1. l'hôte est public (`hote_public`) — sinon la fenêtre viserait un
///      service du réseau local ;
///   2. l'URL de vérification est sur **exactement** cet hôte — sans ce
///      point, un site pourrait demander l'autorisation pour `cdn.fr` et
///      faire ouvrir la fenêtre sur `interbanque.example` ;
///   3. le reste des règles habituelles s'applique toujours (https ou http,
///      pas d'identifiants dans l'URL).
fn valider_url_challenge(
    url: &str,
    domaines: &[String],
    hote_image: Option<&str>,
) -> Result<Url, String> {
    if domaines.is_empty() && hote_image.is_none() {
        return Err(ERREUR_DOMAINE_CHALLENGE.to_string());
    }
    let parsed: Url = url
        .parse()
        .map_err(|_| "URL de challenge invalide".to_string())?;
    if parsed.scheme() != "http" && parsed.scheme() != "https" {
        return Err(format!("schéma non autorisé : {}", parsed.scheme()));
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err("identifiants interdits dans l'URL de challenge".to_string());
    }
    let hote = parsed
        .host_str()
        .ok_or_else(|| "URL de challenge sans hôte".to_string())?
        .trim_end_matches('.')
        .to_ascii_lowercase();
    let autorise = domaines
        .iter()
        .filter_map(|d| normaliser_domaine_challenge(d))
        .any(|d| {
            hote == d
                || hote
                    .strip_suffix(&d)
                    .is_some_and(|prefixe| prefixe.ends_with('.'))
        });
    if autorise {
        return Ok(parsed);
    }
    // Second chemin : le hôte d'image. Les trois conditions ci-dessus.
    if let Some(autorise) = hote_image.and_then(normaliser_domaine_challenge) {
        if hote == autorise && hote_public(&hote) {
            return Ok(parsed);
        }
        tracing::warn!(
            "challenge: hôte d'image refusé source-domaines={} url_host={hote} image_host={autorise}",
            diagnostic_refus_challenge("", url)
        );
    }
    Err(ERREUR_DOMAINE_CHALLENGE.to_string())
}

/// Fenêtre pendant laquelle on ne réémet pas pour une même source.
const SIGNELEMENT_LECTURE_DELAI: Duration = Duration::from_secs(45);

/// Dernière émission par source, pour ne pas émettre une fois par page.
fn dernier_signalement() -> &'static Mutex<HashMap<String, std::time::Instant>> {
    static DERNIER: OnceLock<Mutex<HashMap<String, std::time::Instant>>> = OnceLock::new();
    DERNIER.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Demande à l'interface d'ouvrir la fenêtre de vérification pour une **image**.
///
/// ## Pourquoi il manquait
///
/// Le flux de vérification n'existait que sur deux chemins : le catalogue
/// (`loadSource` côté interface) et les téléchargements (`download://challenge`).
/// La **lecture** n'avait aucun des deux. Mesuré le 2026-10-13 :
///
/// - `setChallenge(...)` n'est appelé qu'à un seul endroit de l'interface
///   (le `catch` de `loadSource`) ;
/// - `pagecache::serve_page` transformait l'échec `CHALLENGE_REQUIRED` du
///   moteur en simple page introuvable, sans rien remonter ;
/// - et après résolution, l'interface ne rechargeait que le catalogue.
///
/// Résultat visible : « la fenêtre s'ouvre, les cookies sont envoyés, mais les
/// webtoons ne s'affichent pas du tout ». Le toast disait vrai — le cookie
/// était envoyé — et la lecture restait morte.
///
/// ## Ce que l'interface ne peut pas faire à notre place
///
/// L'hôte n'est **pas** transmis par l'interface : il est déduit ici de
/// `page_url`, l'URL que le cœur a lui-même demandée au moteur. L'interface
/// ne nomme donc jamais un hôte, et ne peut pas élargir la surface.
pub fn signaler_challenge_lecture(app: &AppHandle, source: &str, page_url: &str, erreur: &str) {
    if !erreur.contains("CHALLENGE_REQUIRED") {
        return;
    }
    let Ok(cible) = Url::parse(page_url) else {
        return;
    };
    let Some(hote) = cible.host_str().map(str::to_ascii_lowercase) else {
        return;
    };
    if !hote_public(&hote) {
        tracing::warn!(
            "challenge: hôte d'image non public, aucune fenêtre source={source} host={hote}"
        );
        return;
    }

    // Un chapitre de 23 pages toutes bloquées émettrait 23 fois le même
    // signal. On garde une seule émission récente par source.
    {
        let mut garde = match dernier_signalement().lock() {
            Ok(g) => g,
            Err(_) => return,
        };
        let maintenant = std::time::Instant::now();
        if let Some(dernier) = garde.get(source) {
            if maintenant.duration_since(*dernier) < SIGNELEMENT_LECTURE_DELAI {
                return;
            }
        }
        garde.insert(source.to_string(), maintenant);
    }

    tracing::warn!("challenge: lecture bloquée source={source} host={hote} -> fenêtre demandée");
    let _ = app.emit(
        "challenge://page",
        json!({
            "source": source,
            "url": page_url,
            "host": hote,
            "reason": erreur.chars().take(160).collect::<String>(),
        }),
    );
}

fn diagnostic_refus_challenge(source: &str, url: &str) -> String {
    let host = Url::parse(url)
        .ok()
        .and_then(|parsed| {
            parsed
                .host_str()
                .map(|host| host.trim_end_matches('.').to_ascii_lowercase())
        })
        .filter(|host| !host.is_empty())
        .unwrap_or_else(|| "indisponible".to_string());
    format!(
        "challenge refusé source={} host={host}",
        super::download::sanitize(source)
    )
}

/// Ouvre la fenêtre de résolution sur l'URL qui a renvoyé le challenge.
///
/// `hote_image` est le second chemin d'autorisation, ajouté le 2026-10-13 :
/// une page de chapitre est souvent servie par un CDN qui n'appartient pas au
/// domaine déclaré par le parseur. Sans lui, la vérification ne pouvait pas
/// s'ouvrir sur ce CDN — et le lecteur n'affichait **aucune** image, sans
/// jamais demander de vérification. Voir `valider_url_challenge` pour les
/// trois conditions que ce chemin impose.
#[tauri::command]
pub async fn open_challenge(
    app: AppHandle,
    sup: State<'_, Arc<Supervisor>>,
    source: String,
    url: String,
    hote_image: Option<String>,
) -> Result<(), String> {
    let domaines = sup.domaines_source(&source).await;
    let parsed = match valider_url_challenge(&url, &domaines, hote_image.as_deref()) {
        Ok(parsed) => parsed,
        Err(error) => {
            tracing::warn!("{}", diagnostic_refus_challenge(&source, &url));
            return Err(error);
        }
    };
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

// ─────────────────────────────────────────────────────────────────────────────
// Transport webview (option B)
//
// Mesuré le 2026-10-13 (spike consigné dans `PLAN.md`) : le WebView2 résout
// seul le challenge Cloudflare de TOONFR en ~10 s et obtient **200** là où
// OkHttp, avec la même clearance, obtient **403**. Le seul client accepté est
// celui qui a résolu. D'où ce module : au lieu de contrefaire l'empreinte
// d'OkHttp, on **demande au navigateur**.
//
// Deux moitiés, et elles ne se parlent jamais dans le mauvais sens :
//   - ici (Rust) : webview **cachée**, liste blanche d'hôtes, attente de la fin
//     du challenge, puis lecture du HTML par le canal **tiré** ;
//   - côté moteur (`WebViewMemoire`) : mémoire URL → HTML servie **avant**
//     `CloudFlareInterceptor`, pour que la requête suivante ne soit plus
//     challengée.
//
// Le canal est tiré par Rust, jamais poussé par la page : `eval_with_callback`
// est lu ici, et la page n'a aucune commande à appeler (cf. la mesure de
// surface du spike — `invoke` existe sur l'origine distante, mais l'ACL Tauri
// le refuse hors fenêtre `main`).
// ─────────────────────────────────────────────────────────────────────────────

/// Délai d'attente de la fin du challenge dans la webview de transport.
///
/// Mesuré le 2026-10-13 sur TOONFR : le Turnstile non-interactif se résout
/// **seul** en ~10 s, sans aucun clic. On laisse large pour un site plus lent,
/// et surtout parce qu'un challenge qui demande un geste mérite qu'on attende
/// l'utilisateur plutôt qu'on renonce au bout de 10 s.
const TRANSPORT_DELAI: Duration = Duration::from_secs(60);

/// Cadence de l'attente de la fin du challenge.
const TRANSPORT_PAS: Duration = Duration::from_millis(500);

/// Étiquette de la webview cachée. **Distincte** de `challenge-<source>` : la
/// fenêtre de vérification est visible et se ferme après résolution, celle-ci
/// est cachée, réutilisée, et ne se ferme qu'avec l'application.
fn transport_label(source: &str) -> String {
    format!("webview-{}", super::download::sanitize(source))
}

/// Obtient le HTML d'une page protégée en la faisant rendre par une webview
/// cachée, puis le pousse dans la mémoire du moteur.
///
/// Renvoie le nombre d'octets de HTML injectés.
///
/// La commande Tauri est une **enveloppe** : toute la logique vit dans
/// [`lire_par_transport`], appelable depuis le cœur.
///
/// Pourquoi cette séparation : le transport doit pouvoir être tenté depuis
/// `Supervisor::request_avec_transport`, pour **toute** URL refusée et non
/// seulement pour la commande que l'interface a appelée. Or un
/// `#[tauri::command]` ne s'appelle qu'avec un `State<'_, _>`, impossible à
/// fabriquer hors d'une application. Séparer les deux rend le cœur
/// capable de se réessayer lui-même.
#[tauri::command]
pub async fn webview_lire(
    app: AppHandle,
    sup: State<'_, Arc<Supervisor>>,
    source: String,
    url: String,
) -> Result<usize, String> {
    lire_par_transport(&app, &sup, &source, &url).await
}

/// Le transport lui-même : lit l'URL par un navigateur caché, injecte le corps
/// dans la mémoire du moteur. Renvoie le nombre d'octets injectés.
pub(crate) async fn lire_par_transport(
    app: &AppHandle,
    sup: &Supervisor,
    source: &str,
    url: &str,
) -> Result<usize, String> {
    // **La liste blanche, avant tout.** Même validation que la fenêtre de
    // vérification, donc même garantie : l'hôte doit être l'un des domaines
    // connus du moteur pour cette source. On ne navigue jamais sur un hôte
    // annoncé par le site.
    let domaines = sup.domaines_source(source).await;
    let parsed = match valider_url_challenge(url, &domaines, None) {
        Ok(parsed) => parsed,
        Err(error) => {
            tracing::warn!("{}", diagnostic_refus_challenge(source, url));
            return Err(error);
        }
    };
    let label = transport_label(source);
    let fenetre = match app.get_webview_window(&label) {
        Some(existante) => {
            // Réutilisée : ouvrir une webview coûte un processus et de la
            // mémoire (12 Go de machine, un JVM, cf. AGENTS.md §3). On navigue
            // la même.
            existante
        }
        None => {
            // **Cachée.** Aucune usurpation d'UA (leçon du 2026-10-11 : trois
            // signaux contradictoires faisaient boucler le Turnstile), et
            // `.visible(false)` : l'utilisateur n'a rien à décider ici.
            let construite =
                WebviewWindowBuilder::new(app, &label, WebviewUrl::External(parsed.clone()))
                    .title(format!("Transport — {source}"))
                    .inner_size(1024.0, 768.0)
                    .visible(false)
                    .build()
                    .map_err(|e| format!("ouverture de la webview de transport : {e}"))?;
            construite
        }
    };

    // Navigation. Sur une fenêtre réutilisée, la page précédente est encore
    // affichée : sans navigate, on lirait le contenu de la page d'avant.
    fenetre
        .navigate(parsed.clone())
        .map_err(|e| format!("navigation de la webview de transport : {e}"))?;

    // Attendre la fin du challenge. Tant que la page affiche l'interstitielle,
    // le HTML n'est pas le contenu — c'est la leçon de la mesure du 2026-10-11
    // (« 7 cookies passés » alors que personne n'avait résolu).
    let limite = std::time::Instant::now() + TRANSPORT_DELAI;
    let mut interstitiel = true;
    while interstitiel && std::time::Instant::now() < limite {
        tokio::time::sleep(TRANSPORT_PAS).await;
        interstitiel = lire_page(app, &label)
            .await
            .map(|page| page.interstitiel)
            // Une page illisible n'est pas une page résolue : on continue
            // d'attendre, on ne conclut pas sur un échec de lecture.
            .unwrap_or(true);
    }
    if interstitiel {
        return Err(format!(
            "challenge non résolu en {} s sur {}",
            TRANSPORT_DELAI.as_secs(),
            parsed.host_str().unwrap_or("hôte inconnu")
        ));
    }

    // Laisser la page finir de se construire après la disparation de
    // l'interstitielle : le titre change avant que le contenu ne soit peint.
    tokio::time::sleep(Duration::from_millis(300)).await;

    // **Rejouer l'URL demandée, par `fetch`, et renvoyer son CORPS.**
    //
    // C'est la correction du défaut mesuré le 2026-10-13 : TOONFR est un
    // parser *Madara*, qui interroge `wp-admin/admin-ajax.php` — du **JSON**,
    // pas du HTML. Lire `document.documentElement.outerHTML` renvoyait le DOM
    // de la page d'accueil (`/webtoon/`), 40 octets de rendu JSON, et le
    // moteur en tirait `latest ok: 0 items`. Le catalogue semblait se remplir
    // parce que cela venait de `filterOptions`, une autre requête.
    //
    // Un `fetch` depuis la page même origine hérite de la clearance et du
    // contexte du navigateur. On renvoie donc exactement ce que le moteur
    // aurait reçu sur le réseau — du HTML ou du JSON, selon ce que l'URL sert.
    //
    // **Deux temps, parce que `eval_with_callback` est synchrone.** WebView2
    // ne rend pas une promesse : une expression `await fetch(...)` rend `{}`
    // ou rien. Mesuré le 2026-10-13 : `relecture de la page par le navigateur
    // impossible`, la promesse n'ayant pas eu le temps de se résoudre dans le
    // délai de 5 s du canal. On démarre donc le `fetch` sans l'attendre, on
    // stocke son aboutissement dans une globale, puis on relit cette globale
    // jusqu'à ce qu'elle porte un résultat — même schéma que
    // `identite_de_la_fenetre` avec `getHighEntropyValues`, pour la même
    // raison.
    let _ = fenetre.eval(format!(
        r#"(function() {{
            window.__tatamiCorps = null;
            fetch({url}, {{ credentials: 'include' }})
                .then(function(r) {{ return r.text(); }})
                .then(function(t) {{ window.__tatamiCorps = {{ ok: true, corps: t }}; }})
                .catch(function(e) {{ window.__tatamiCorps = {{ ok: false, erreur: String(e) }}; }});
        }})()"#,
        url = serde_json::to_string(parsed.as_str()).unwrap_or_default()
    ));

    let mut corps: Option<String> = None;
    // Un corps vide est un **résultat**, pas une absence de résultat : la
    // relecture a réussi, le site a renvoyé zéro octet. Attendre 20 s de plus ne
    // changera rien, et dire « relecture impossible » serait un mensonge — il
    // ferait chercher un défaut de transport chez une source dont l'endpoint
    // est simplement vide (mesuré le 2026-10-14 sur l'AJAX de TOONFR).
    let mut corps_vide = false;
    let echeance_corps = std::time::Instant::now() + Duration::from_secs(20);
    while corps.is_none() && std::time::Instant::now() < echeance_corps {
        tokio::time::sleep(Duration::from_millis(250)).await;
        // **On consume la `Value` telle quelle, pas sa forme « chaîne ».**
        // `evaluer_javascript` rend une `Value` **objet** ici, parce que
        // `resoudre_rendu` décode les deux niveaux. Demander `.as_str()` sur un
        // objet donne toujours `None` : la boucle n'aboutissait donc jamais et
        // chaque `webview_lire` attendait 20 s pour rien.
        //
        // Mesuré le 2026-10-14 : sur TOONFR comme sur HENTAIZONE, 22 s puis
        // « relecture ... impossible » — la lecture n'avait jamais rien reçu.
        let Some(v) =
            evaluer_javascript(&fenetre, "JSON.stringify(window.__tatamiCorps || null)").await
        else {
            continue;
        };
        if !v.is_object() {
            continue;
        }
        if v.get("ok").and_then(|o| o.as_bool()) == Some(true) {
            if let Some(c) = v.get("corps").and_then(|c| c.as_str()) {
                if c.is_empty() {
                    corps_vide = true;
                    break;
                }
                corps = Some(c.to_string());
            }
        } else if let Some(e) = v.get("erreur").and_then(|e| e.as_str()) {
            return Err(format!("relecture par le navigateur refusée : {e}"));
        }
    }
    let corps = match corps {
        Some(c) => c,
        None if corps_vide => {
            return Err(format!(
                "le site a renvoyé un corps vide pour {}",
                parsed.as_str()
            ))
        }
        None => {
            return Err("relecture de la page par le navigateur impossible".to_string());
        }
    };

    // **Une page de verification n'est jamais un contenu.** Mesure le
    // 2026-10-14 : le transport relisait la page « Un instant… » (6 071 o),
    // l'injectait, et le moteur la resservait en 200 OK pendant 5 minutes. Le
    // rejeu recevait alors du HTML sans catalogue. On refuse donc ici, avant
    // l'injection et avant de journaliser un succès.
    if est_page_de_verification(&corps) {
        return Err(format!(
            "la vérification n'est pas résolue sur {} : la page reçue est encore la page de contrôle",
            parsed.host_str().unwrap_or("hôte inconnu")
        ));
    }

    tracing::info!(
        "challenge: transport webview source={source} octets={} host={}",
        corps.len(),
        parsed.host_str().unwrap_or("?")
    );

    // `html` est le nom attendu par le protocole `injecter` ; son contenu, lui,
    // est le **corps de la réponse** de l'URL demandée.
    sup.request(
        "injecter",
        json!({ "url": parsed.as_str(), "html": corps }),
        Duration::from_secs(20),
    )
    .await
    .map_err(|e| format!("injecter : {e}"))?;

    Ok(corps.len())
}

/// Lit les cookies de la fenêtre, les pousse au moteur, ferme la fenêtre.
///
/// Partagée par le bouton manuel (`solve_challenge`) et par la surveillance
/// automatique (`surveiller_fenetre`) — un seul des deux passe, gardé par le
/// drapeau de `SURVEILLANCES`.
/// Identité navigateur capturée dans la fenêtre qui a reçu la clearance.
///
/// WebView2 `ExecuteScript` est synchrone ; `getHighEntropyValues` ne l'est
/// pas. On démarre donc sa promesse dans la page, puis on lit son résultat par
/// callback jusqu'à sa résolution ou jusqu'au délai borné.
async fn identite_de_la_fenetre(
    app: &AppHandle,
    label: &str,
) -> Option<(String, serde_json::Value)> {
    let fenetre = app.get_webview_window(label)?;
    fenetre
        .eval(
            r#"(function() {
                var data = navigator.userAgentData;
                window.__tatamiChallengeHighEntropy = { ready: false, values: {} };
                if (!data || typeof data.getHighEntropyValues !== "function") {
                    window.__tatamiChallengeHighEntropy.ready = true;
                    return;
                }
                data.getHighEntropyValues([
                    "fullVersionList", "platformVersion", "architecture", "bitness", "model"
                ]).then(function(values) {
                    window.__tatamiChallengeHighEntropy = { ready: true, values: values };
                }, function() {
                    window.__tatamiChallengeHighEntropy = { ready: true, values: {} };
                });
            })()"#,
        )
        .ok()?;

    let echeance = tokio::time::Instant::now() + Duration::from_secs(2);
    loop {
        let rendu = evaluer_javascript(
            &fenetre,
            r#"(function() {
            var data = navigator.userAgentData || {};
            var high = window.__tatamiChallengeHighEntropy || { ready: false, values: {} };
            var values = high.values || {};
            return {
                userAgent: navigator.userAgent || "",
                highEntropyReady: high.ready === true,
                profile: {
                    brands: data.brands || [],
                    mobile: typeof data.mobile === "boolean" ? data.mobile : null,
                    platform: typeof data.platform === "string" ? data.platform : null,
                    fullVersionList: values.fullVersionList || [],
                    platformVersion: values.platformVersion || null,
                    architecture: values.architecture || null,
                    bitness: values.bitness || null,
                    model: values.model || null,
                    language: navigator.language || null,
                    languages: Array.isArray(navigator.languages) ? navigator.languages : []
                }
            };
        })()"#,
        )
        .await?;

        let user_agent = rendu
            .get("userAgent")
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|ua| !ua.is_empty())?
            .to_string();
        let high_entropy_ready = rendu
            .get("highEntropyReady")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        let profile = rendu.get("profile")?.clone();

        if high_entropy_ready {
            let _ = fenetre.eval("delete window.__tatamiChallengeHighEntropy");
            return Some((user_agent, profile));
        }
        if tokio::time::Instant::now() >= echeance {
            tracing::warn!(
                "challenge: délai dépassé pour les Client Hints haute entropie source={label}"
            );
            let _ = fenetre.eval("delete window.__tatamiChallengeHighEntropy");
            return Some((user_agent, profile));
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

/// Interprète la chaîne rendue par `eval_with_callback`.
///
/// **Pourquoi cette fonction existe, et pourquoi elle est séparée.** Mesuré le
/// 2026-10-13 dans l'app réelle : `webview_lire` renvoyait
/// `lecture du HTML rendu impossible` alors que la page affichait bien son
/// contenu. La cause est ici, pas dans le réseau.
///
/// `eval_with_callback` rend une chaîne JSON. Quand l'expression évaluée rend
/// **un objet** (`JSON.stringify({...})`, ce que font tous les appels
/// existants), la chaîne est `{"cle":"valeur"}` et `from_str` la résout
/// directement. Quand elle rend une **chaîne brute** — `outerHTML` seul, par
/// exemple — la chaîne est `"<!DOCTYPE html>…"`, c'est-à-dire du JSON valide
/// dont la valeur est une `Value::String`. Cette fonction est le seul endroit
/// qui décide si ce second cas est résolu ou jeté.
///
/// Le bug d'origine : sans cette séparation, une chaîne brute partait dans un
/// `from_str` destiné à un objet et rendait `None`. En la sortant de
/// `evaluer_javascript`, elle devient testable — et le test ci-dessous est
/// exactement le cas qui échouait.
fn resoudre_rendu(brut: &str) -> Option<serde_json::Value> {
    let valeur = serde_json::from_str::<serde_json::Value>(brut).ok()?;
    // Une `Value::String` porte un second niveau de JSON : la chaîne que
    // l'expression a rendue. On la décode une fois de plus. C'est ce que
    // `JSON.stringify(document.documentElement.outerHTML)` produit.
    if let serde_json::Value::String(interne) = valeur {
        return serde_json::from_str::<serde_json::Value>(&interne).ok();
    }
    Some(valeur)
}

async fn evaluer_javascript(
    fenetre: &WebviewWindow,
    javascript: &str,
) -> Option<serde_json::Value> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let envoi = std::sync::Arc::new(std::sync::Mutex::new(Some(tx)));
    fenetre
        .eval_with_callback(javascript, move |rendu: String| {
            if let Ok(mut slot) = envoi.lock() {
                if let Some(emmetteur) = slot.take() {
                    let _ = emmetteur.send(rendu);
                }
            }
        })
        .ok()?;
    let rendu = tokio::time::timeout(UA_DELAI, rx).await.ok()?.ok()?;
    resoudre_rendu(&rendu)
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
    let (ua, browser_profile) = match identite_de_la_fenetre(app, &label).await {
        Some((user_agent, profile)) => (user_agent, Some(profile)),
        None => (engine_user_agent(sup).await, None),
    };
    tracing::info!(
        "challenge-debug: rejeu avec userAgent={}",
        if debug_challenge() {
            ua.as_str()
        } else {
            "<instrumente>"
        }
    );
    let mut params = json!({
        "source": source,
        "cookies": payload,
        "userAgent": ua,
    });
    if let Some(profile) = browser_profile {
        params["browserProfile"] = profile;
    }
    sup.request("setCookies", params, Duration::from_secs(20))
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
        action_surveillance, challenge_en_cours, challenge_label, diagnostic_refus_challenge,
        domaine_effectif, enregistrer_surveillance, est_page_de_verification, hote_public,
        resoudre_rendu, reste_interstitiel, retirer_surveillance, surveillances, transport_label,
        valider_url_challenge, Action, LectureCookies, ERREUR_DOMAINE_CHALLENGE,
    };

    /// **Le defaut du 2026-10-11, en une assertion.** La surveillance a
    /// applique a t+3,4 s sans qu'aucun humain n'ait clique : Cloudflare avait
    /// pose un `cf_clearance` sur sa page d'interstitielle. Aucun marqueur
    /// `cf_chl_*` n'etait present — la garde `challenge_en_cours` ne pouvait pas
    /// voir le piege. Seul l'etat de la page le voit.
    #[test]
    fn une_page_de_verification_bloque_la_resolution() {
        // Ce que la fenetre affichait reellement, mesure dans l'app.
        assert!(
            reste_interstitiel("Un instant\u{2026}", true, true),
            "l'interstitielle de Cloudflare doit bloquer l'application des cookies"
        );
        // La meme page, translatee.
        assert!(reste_interstitiel("Just a moment...", false, false));
        // Un titre qui ne dit rien, mais avec le formulaire de verification.
        assert!(reste_interstitiel("hentaizone.xyz", true, false));
    }

    /// La page de verification d'Astral, telle que mesuree le 2026-10-14 : elle
    /// doit etre refusee avant injection.
    #[test]
    fn une_page_de_verification_n_est_jamais_injectee() {
        let verification = "<!DOCTYPE html><html><head><title>Just a moment...</title>\
            <script>window._cf_chl_opt={cvId:'3'};</script></head><body></body></html>";
        assert!(
            est_page_de_verification(verification),
            "la page de verification doit etre reconnue, sinon elle est injectee"
        );
    }

    /// Le faux positif a eviter : Sushiscan porte le script de Cloudflare sans
    /// etre protege. Son catalogue doit passer, sinon le garde casse une source saine.
    #[test]
    fn un_catalogue_sain_n_est_jamais_refuse() {
        let catalogue = "<!DOCTYPE html><html><head><title>Sushiscan - Scan FR</title>\
            <script src=\"/cdn-cgi/challenge-platform/h/g/scripts.js\"></script></head>\
            <body><a href=\"/catalogue/sawako/\">Sawako</a></body></html>";
        assert!(
            !est_page_de_verification(catalogue),
            "un catalogue sain porte le script Cloudflare sans etre bloque : il ne doit pas etre refuse"
        );
    }

    /// Le cas limite : un titre « Just a moment » sans la configuration de
    /// verification n'est pas une verification (un article qui parle de Cloudflare).
    #[test]
    fn le_titre_seul_ne_suffit_pas() {
        let article = "<html><head><title>Just a moment of silence</title></head>\
            <body>texte</body></html>";
        assert!(!est_page_de_verification(article));
    }

    /// La contre-epreuve : sans ce test, le garde-derouillerait des que la
    /// fenetre affiche la vraie page — et l'application n'appliquerait plus
    /// jamais rien.
    #[test]
    fn la_vraie_page_ne_bloque_plus() {
        assert!(
            !reste_interstitiel("Hentai Zone - Lecture de manga", false, false),
            "une page reelle doit laisser appliquer la clearance"
        );
        // Un site sans Cloudflare : rien ne doit bloquer.
        assert!(!reste_interstitiel("Mangadex", false, false));
    }

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

    fn domaines_exemple() -> Vec<String> {
        vec!["exemple.com".to_string()]
    }

    #[test]
    fn challenge_accepte_l_hote_declare_et_ses_sous_domaines() {
        let domaines = domaines_exemple();
        assert!(valider_url_challenge("https://exemple.com/check", &domaines, None).is_ok());
        assert!(valider_url_challenge("http://cdn.exemple.com/check", &domaines, None).is_ok());
    }

    #[test]
    fn challenge_refuse_un_hote_externe_et_un_faux_suffixe() {
        let domaines = domaines_exemple();
        assert_eq!(
            valider_url_challenge("https://autre.test/check", &domaines, None).unwrap_err(),
            ERREUR_DOMAINE_CHALLENGE
        );
        assert_eq!(
            valider_url_challenge("https://attaquant-exemple.com/check", &domaines, None)
                .unwrap_err(),
            ERREUR_DOMAINE_CHALLENGE
        );
    }

    #[test]
    fn challenge_toomics_refuse_les_hotes_qui_contiennent_son_nom() {
        let domaines = vec!["toomics.com".to_string()];
        assert_eq!(
            valider_url_challenge("https://toomics.com.attaquant.com/check", &domaines, None)
                .unwrap_err(),
            ERREUR_DOMAINE_CHALLENGE
        );
        assert_eq!(
            valider_url_challenge("https://attaquant-toomics.com/check", &domaines, None)
                .unwrap_err(),
            ERREUR_DOMAINE_CHALLENGE
        );
    }

    #[test]
    fn challenge_refuse_un_schema_non_http_et_des_identifiants() {
        let domaines = domaines_exemple();
        assert!(valider_url_challenge("file://exemple.com/check", &domaines, None).is_err());
        assert!(
            valider_url_challenge("https://utilisateur@exemple.com/check", &domaines, None)
                .is_err()
        );
    }

    #[test]
    fn challenge_refuse_une_source_sans_domaine_declare() {
        assert_eq!(
            valider_url_challenge("https://exemple.com/check", &[], None).unwrap_err(),
            ERREUR_DOMAINE_CHALLENGE
        );
    }

    #[test]
    fn challenge_accepte_un_domaine_alternatif_declare() {
        let domaines = vec!["mangalib.org".to_string(), "mangalib.me".to_string()];
        assert!(valider_url_challenge("https://mangalib.me/check", &domaines, None).is_ok());
        assert!(valider_url_challenge("https://www.mangalib.me/check", &domaines, None).is_ok());
    }

    #[test]
    fn journal_refus_challenge_ne_contient_que_source_et_hote() {
        let diagnostic = diagnostic_refus_challenge(
            "MANGADEX",
            "https://utilisateur:secret@example.net/private/path?cf_clearance=cookie#fragment",
        );

        assert_eq!(
            diagnostic,
            "challenge refusé source=MANGADEX host=example.net"
        );
    }

    // ------------------------------------------------------------------
    // Hôte d'image (2026-10-13) — la lecture n'avait aucun chemin de
    // vérification : `pagecache` transformait le `CHALLENGE_REQUIRED` du moteur
    // en page introuvable. Ces cas verrouillent la politique choisie.
    // ------------------------------------------------------------------

    #[test]
    fn hote_dimage_autorise_sur_son_propre_hote() {
        // Le cas qui Unlock tout : la page est servie par un CDN qui n'est pas
        // le domaine du parseur, et la fenêtre doit pouvoir s'y ouvrir.
        assert!(valider_url_challenge(
            "https://cdn.tiers.fr/img/001.jpg",
            &domaines_exemple(),
            Some("cdn.tiers.fr")
        )
        .is_ok());
    }

    #[test]
    fn hote_dimage_n_autorise_pas_un_autre_hote() {
        // **La condition 2 de `valider_url_challenge`.** Sans elle, un site
        // demanderait l'autorisation pour `cdn.fr` et ferait ouvrir la fenêtre
        // sur une cible de son choix — la fenêtre est un navigateur complet.
        assert_eq!(
            valider_url_challenge(
                "https://interbanque.example/prive",
                &domaines_exemple(),
                Some("cdn.tiers.fr")
            )
            .unwrap_err(),
            ERREUR_DOMAINE_CHALLENGE
        );
    }

    #[test]
    fn hote_dimage_prive_ou_local_est_refuse() {
        // **La condition 1.** Une source peut mettre 127.0.0.1 ou l'adresse des
        // métadonnées d'instance dans ses URL de pages : la fenêtre ne doit
        // jamais y naviguer.
        for hote in [
            "127.0.0.1",
            "169.254.169.254",
            "10.1.2.3",
            "192.168.1.10",
            "172.16.0.1",
            "localhost",
            "metadata.google.internal",
            "bd.cdn.local",
        ] {
            assert_eq!(
                valider_url_challenge(
                    &format!("https://{hote}/img.jpg"),
                    &domaines_exemple(),
                    Some(hote)
                )
                .unwrap_err(),
                ERREUR_DOMAINE_CHALLENGE,
                "{hote} ne doit jamais autoriser une fenêtre"
            );
        }
    }

    #[test]
    fn hote_public_accepte_un_nom_public_norminal() {
        // Contrôle **positif** : sans lui, une liste de refus ne prouverait rien,
        // et une régression qui refuserait tout passerait le test.
        for hote in [
            "cdn.tiers.fr",
            "c.sushiscan.net",
            "img.reimg.org",
            "8.8.8.8",
        ] {
            assert!(hote_public(hote), "{hote} devrait être accepté");
        }
    }

    #[test]
    fn aucun_hote_image_ne_ouvre_la_fenetre_tel_quel() {
        // Le second chemin ne contourne pas les autres règles : pas de schéma
        // douteux, pas d'identifiants dans l'URL.
        assert!(valider_url_challenge(
            "file://cdn.tiers.fr/img.jpg",
            &domaines_exemple(),
            Some("cdn.tiers.fr")
        )
        .is_err());
        assert!(valider_url_challenge(
            "https://moi:secret@cdn.tiers.fr/img.jpg",
            &domaines_exemple(),
            Some("cdn.tiers.fr")
        )
        .is_err());
    }

    #[test]
    fn source_sans_domaine_sans_hote_image_reste_refusee() {
        assert_eq!(
            valider_url_challenge("https://exemple.com/check", &[], None).unwrap_err(),
            ERREUR_DOMAINE_CHALLENGE
        );
    }

    // ─── Le parsing du résultat d'`eval_with_callback` ───

    /// **Le bug réel du 2026-10-13, en une assertion.** L'app renvoyait
    /// `lecture du HTML rendu impossible` alors que la page avait résolu son
    /// challenge et affichait son contenu. Cause : l'expression évaluée rendait
    /// une **chaîne brute**, et `resoudre_rendu` la traitait comme un objet.
    ///
    /// Ce test est écrit depuis la forme que rend vraiment
    /// `eval_with_callback` — une chaîne JSON, pas un objet — et non depuis ce
    /// que le code espérait recevoir.
    #[test]
    fn une_chaine_brute_rendue_est_resolue_en_html() {
        // Ce que `eval_with_callback` rend pour
        // `JSON.stringify(document.documentElement.outerHTML)`.
        let brut = serde_json::to_string(
            &serde_json::to_string("<!DOCTYPE html><html><body>chapitres</body></html>").unwrap(),
        )
        .unwrap();
        let rendu = resoudre_rendu(&brut);
        assert!(
            rendu.is_some(),
            "le HTML rendu par la webview doit être résolu, pas jeté"
        );
        assert_eq!(
            rendu.unwrap().as_str(),
            Some("<!DOCTYPE html><html><body>chapitres</body></html>"),
        );
    }

    /// La contre-épreuve : **sans** ce que le correctif ajoute, une chaîne
    /// brute partait en `from_str` sur `"<!DOCTYPE...` et rendait `None`. Ce
    /// test décrit précisément la forme qui échouait.
    #[test]
    fn un_objet_rendu_reste_resolu() {
        // Tous les appels existants passent par `JSON.stringify({...})` : la
        // chaîne rendue est alors un objet, qui doit continuer de passer.
        let brut = serde_json::to_string(&serde_json::json!({"t": "instant", "f": false}))
            .unwrap()
            .to_string();
        let brut = serde_json::to_string(&brut).unwrap();
        let rendu = resoudre_rendu(&brut);
        assert!(rendu.is_some());
        assert_eq!(
            rendu.unwrap().get("t").and_then(|v| v.as_str()),
            Some("instant")
        );
    }

    /// Un rendu qui n'est ni chaîne ni objet valide reste un échec : on ne
    /// devine pas. Sans ce test, une régression qui accepterait n'importe quoi
    /// rendrait un HTML vide comme un succès.
    #[test]
    fn un_rendu_invalide_reste_un_echec() {
        assert!(resoudre_rendu("pas du json").is_none());
        assert!(resoudre_rendu("").is_none());
    }

    // ─── Transport webview (option B) ───

    /// **Pourquoi deux fenêtres, et pas une.** La fenêtre de vérification est
    /// visible et se **ferme** après résolution (`appliquer_cookies` finit par
    /// `window.close()`). La webview de transport, elle, est cachée et
    /// **réutilisée** : elle survit à une lecture pour servir la suivante, et
    /// ne coûte donc un processus qu'une fois.
    ///
    /// Les confondre ferait que le transport rouvre une fenêtre visible à
    /// chaque page — ou, pire, chercherait une fenêtre déjà fermée et
    /// échouerait pour toujours.
    #[test]
    fn la_webview_de_transport_n_est_pas_la_fenetre_de_verification() {
        assert_eq!(transport_label("TOONFR"), "webview-TOONFR");
        assert_eq!(challenge_label("TOONFR"), "challenge-TOONFR");
        assert_ne!(
            transport_label("TOONFR"),
            challenge_label("TOONFR"),
            "deux fenêtres de durées de vie différentes ne peuvent pas partager une étiquette"
        );
    }

    /// Le nom de source entre dans une étiquette de fenêtre, donc dans le
    /// système de fenêtres de Tauri : `sanitize` s'applique comme ailleurs.
    /// Valeurs relevées dans `download::sanitize` (`/` et `\` → `_`), pas
    /// devinées.
    #[test]
    fn l_etiquette_de_transport_est_assainie() {
        assert_eq!(transport_label("a/b\\c"), "webview-a_b_c");
    }

    /// Le transport réutilise la **même** liste blanche que la fenêtre de
    /// vérification. Ce test le prouve sur le cas qui compte : un hôte qui
    /// n'appartient pas à la source est refusé, donc la webview cachée ne
    /// navigue jamais sur un domaine annoncé par le site.
    #[test]
    fn le_transport_refuse_un_hote_hors_domaines_de_la_source() {
        assert!(
            valider_url_challenge("https://exemple.com/check", &domaines_exemple(), None).is_ok(),
            "l'hôte de la source passe"
        );
        assert!(
            valider_url_challenge("https://interbanque.example/", &domaines_exemple(), None)
                .is_err(),
            "un hôte étranger est refusé par la même validation"
        );
    }
}
