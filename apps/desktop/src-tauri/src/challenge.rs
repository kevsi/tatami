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

use std::sync::Arc;
use std::time::Duration;

use serde_json::json;
use tauri::{
    AppHandle, Emitter, Manager, State, Url, WebviewUrl, WebviewWindowBuilder, WindowEvent,
};

use super::supervisor::Supervisor;

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
    let mut builder = WebviewWindowBuilder::new(&app, &label, WebviewUrl::External(parsed))
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
    let window = app
        .get_webview_window(&label)
        .ok_or("la fenêtre de vérification est fermée")?;
    let parsed: Url = url
        .parse()
        .map_err(|e| format!("URL de challenge invalide : {e}"))?;
    let host = parsed
        .host_str()
        .ok_or("hôte introuvable dans l'URL de challenge")?
        .to_string();

    tracing::info!("challenge: lecture des cookies source={source} label={label}");
    let cookies = window
        .cookies_for_url(parsed)
        .map_err(|e| format!("lecture des cookies : {e}"))?;

    let payload: Vec<serde_json::Value> = cookies
        .iter()
        .filter(|c| !c.name().is_empty() && !c.value().is_empty())
        .map(|c| {
            json!({
                "name": c.name(),
                "value": c.value(),
                "domain": host,
                "path": c.path().unwrap_or("/"),
            })
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
            "userAgent": engine_user_agent(&sup).await,
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
    use super::challenge_label;

    #[test]
    fn libelle_de_challenge_est_sanitise() {
        assert_eq!(challenge_label("MANGADEX"), "challenge-MANGADEX");
        // Un seul nom de dossier : pas de « .. » qui traverse quoi que ce soit.
        let label = challenge_label("site/../evil");
        assert!(label.starts_with("challenge-site_"));
        assert!(!label.contains('/'));
        assert_eq!(challenge_label(".."), "challenge-sans-titre");
    }
}
