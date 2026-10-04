#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod cbz;
mod challenge;
mod commands;
mod download;
mod image;
mod library;
mod pagecache;
mod shelf;
mod store;
mod supervisor;
mod zipread;

use std::path::PathBuf;
use std::sync::Arc;

use supervisor::{dev_sidecar_paths, Supervisor};
use tauri::Manager;

fn not_found() -> tauri::http::Response<Vec<u8>> {
    tauri::http::Response::builder()
        .status(404)
        .body(Vec::new())
        .unwrap_or_else(|_| tauri::http::Response::new(Vec::new()))
}

/// Candidats `(java, classpath)` du moteur, testés dans l'ordre : le
/// **premier dont le `java` existe** gagne.
///
/// `CARGO_MANIFEST_DIR` (dans `supervisor::dev_sidecar_paths`) n'existe
/// qu'à la compilation : dans une app installée, ce chemin ne mène nulle part
/// et l'app ne démarre aucune source. D'où le second emplacement :
///
/// - **dev** — `engine/dist/tatami-engine` à la racine du dépôt, via
///   `dev_sidecar_paths()`. C'est ce que `cargo tauri dev` utilise : inchangé,
///   c'est le premier candidat et il existe en local.
/// - **production** — la ressource empaquetée déclarée dans
///   `src-tauri/tauri.conf.json` (`bundle.resources`) : le dossier
///   `engine/tatami-engine` à côté de l'exécutable, soit
///   `resource_dir()/engine/tatami-engine`. C'est là que vit le moteur dans
///   une app installée (Windows : `C:\Program Files\Tatami\engine\tatami-engine`).
fn sidecar_candidates(resource_dir: Option<&std::path::Path>) -> Vec<(PathBuf, String)> {
    let mut candidats = vec![dev_sidecar_paths()];
    if let Some(dir) = resource_dir {
        let dist = dir.join("engine").join("tatami-engine");
        candidats.push((
            dist.join("runtime").join("bin").join("java.exe"),
            format!(r"{}\lib\*", dist.display()),
        ));
    }
    candidats
}

/// (java, classpath) retenus. Si aucun candidat n'existe, on rend le chemin de
/// dev : le superviseur échouera en nommant l'emplacement attendu.
fn sidecar_paths(resource_dir: Option<&std::path::Path>) -> (PathBuf, String) {
    let candidats = sidecar_candidates(resource_dir);
    for (java, cp) in &candidats {
        if java.is_file() {
            return (java.clone(), cp.clone());
        }
    }
    candidats.into_iter().next().unwrap_or_else(|| {
        let java = PathBuf::from("java.exe");
        (java.clone(), java.display().to_string())
    })
}

fn main() {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    let context = tauri::generate_context!();
    let resource_dir =
        tauri::utils::platform::resource_dir(context.package_info(), &tauri::utils::Env::default())
            .ok();
    let (java, classpath) = sidecar_paths(resource_dir.as_deref());
    tracing::info!("sidecar java={}", java.display());
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
                // Deux préfixes, deux natures : `page/` (lecteur) et `cover/`
                // (vignettes). Les deux passent par le moteur — la webview ne
                // contacte jamais un site de manga elle-même.
                let path = request.uri().path().trim_start_matches('/');
                let item = path
                    .strip_prefix("page/")
                    .map(|t| (t, "page"))
                    .or_else(|| path.strip_prefix("cover/").map(|t| (t, "cover")));
                let Some((token, _genre)) = item else {
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
            download::local_chapter_pages,
            download::local_file_pages,
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
            shelf::add_bookmark,
            shelf::remove_bookmark,
            shelf::list_bookmarks,
            shelf::list_categories,
            shelf::add_category,
            shelf::rename_category,
            shelf::delete_category,
            shelf::reorder_categories,
            shelf::set_favorite_category,
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
        .build(context)
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
