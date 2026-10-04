#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod cbz;
mod challenge;
mod commands;
mod download;
mod library;
mod pagecache;
mod shelf;
mod store;
mod supervisor;

use std::sync::Arc;

use supervisor::{dev_sidecar_paths, Supervisor};
use tauri::Manager;

fn not_found() -> tauri::http::Response<Vec<u8>> {
    tauri::http::Response::builder()
        .status(404)
        .body(Vec::new())
        .unwrap_or_else(|_| tauri::http::Response::new(Vec::new()))
}

fn main() {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    let (java, classpath) = dev_sidecar_paths();
    tracing::info!("sidecar java={} (dev)", java.display());
    let sup: Arc<Supervisor> = Supervisor::new(java, classpath);
    let cache_dir = std::env::var("APPDATA")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("."))
        .join("Tatami")
        .join("cache");
    let cache = pagecache::PageCache::new(cache_dir);

    // Base de données de l'utilisateur (favoris, historique, reprise). Si le
    // fichier est inaccessible (dossier verrouillé, disque plein, droits),
    // on bascule sur une base en mémoire : l'application démarre quand même,
    // seuls les favoris et l'historique ne survivront pas à la fermeture.
    let data_dir = std::env::var("APPDATA")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("."))
        .join("Tatami");
    let db_path = data_dir.join("tatami.db");
    let store = match store::Store::open(db_path.clone()) {
        Ok(s) => {
            tracing::info!("base ouverte : {}", db_path.display());
            s
        }
        Err(e) => {
            tracing::error!("base de donnees indisponible ({e}), repli en memoire");
            store::Store::open_in_memory().expect("la base en memoire ne peut pas echouer")
        }
    };

    tauri::Builder::default()
        .manage(sup.clone())
        .manage(Arc::new(store))
        .manage(cache.clone())
        .register_asynchronous_uri_scheme_protocol("tatami", move |_ctx, request, responder| {
            let sup = sup.clone();
            let cache = cache.clone();
            tauri::async_runtime::spawn(async move {
                let path = request.uri().path().trim_start_matches('/');
                let Some(token) = path.strip_prefix("page/") else {
                    responder.respond(not_found());
                    return;
                };
                let Some(pages) = cache.resolve(token) else {
                    responder.respond(not_found());
                    return;
                };
                match pagecache::serve_page(&sup, &cache, &pages, token).await {
                    Ok((bytes, ct)) => responder.respond(
                        tauri::http::Response::builder()
                            .header("Content-Type", ct)
                            .body(bytes)
                            .unwrap_or_else(|_| not_found()),
                    ),
                    Err(e) => {
                        tracing::warn!("tatami serve error: {e}");
                        responder.respond(not_found());
                    }
                }
            });
        })
        .invoke_handler(tauri::generate_handler![
            commands::search,
            commands::details,
            commands::chapters,
            commands::pages,
            commands::list_sources,
            commands::latest,
            commands::filter_options,
            download::download_chapter,
            download::missing_chapters,
            download::clean_partials,
            library::list_library,
            library::existing_chapters,
            library::remove_download,
            shelf::list_favorites,
            shelf::set_favorite,
            shelf::save_progress,
            shelf::note_content_rating,
            shelf::list_recent,
            shelf::list_disabled_sources,
            shelf::set_source_enabled,
            shelf::enable_all_sources,
            shelf::forget_history,
            shelf::clear_history,
            challenge::open_challenge,
            challenge::solve_challenge,
            challenge::cancel_challenge
        ])
        .manage(download::DownloadState::default())
        .setup(|app| {
            let handle = app.state::<Arc<Supervisor>>().inner().clone();
            tauri::async_runtime::spawn(async move { handle.run().await });
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("tauri build")
        .run(|handle, event| {
            // Arrêt propre : tuer le sidecar avant de quitter, sinon le java reste orphelin.
            if let tauri::RunEvent::ExitRequested { api, .. } = event {
                api.prevent_exit();
                let h = handle.clone();
                tauri::async_runtime::spawn(async move {
                    h.state::<Arc<Supervisor>>().shutdown().await;
                    h.exit(0);
                });
            }
        });
}
