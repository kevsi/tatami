#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

// Le code vit dans la bibliothèque (`src/lib.rs`) et non ici : c'est ce qui
// rend les tests de contrat de `tests/` possibles (ils doivent rejouer les
// réponses enregistrées du moteur contre le code Rust, pas seulement contre le
// schéma). Ce binaire ne fait que l'assemblage Tauri. Cf. `lib.rs` pour pourquoi.
use std::path::PathBuf;
use std::sync::Arc;

use tatami_desktop::supervisor::{dev_sidecar_paths, Supervisor};
use tauri::Manager;

// `generate_handler!` résout le chemin `commands::details` jusqu'au **dernier**
// segment, puis invoque le macro `__cmd__details` **au même chemin**. Ce macro
// est `#[macro_export]`é *et* réexporté (`pub use`) par le module qui porte la
// commande : c'est le réexport qui rend la résolution possible depuis un autre
// crate. Vérifié dans tauri-macros 2.7.1 (`command/wrapper.rs`, ligne 345 :
// « allow the macro to be resolved with the same path as the command function »).
// L'import de `commands` ci-dessous n'est donc pas cosmétique : sans lui, le
// chemin de `generate_handler!` ne se résout pas.
use tatami_desktop::{challenge, commands, download, library, pagecache, scheduler, shelf, store};

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

    // Le dossier de téléchargements est un réglage en base : on le lit UNE fois
    // ici, avant toute commande, et `downloads_root()` s'en sert ensuite.
    // Sans cela, la bibliothèque et les téléchargements repartiraient du dossier
    // par défaut alors que l'utilisateur en a choisi un autre.
    let racine = match store.get_setting("dossier_telechargements") {
        Ok(Some(c)) if !c.trim().is_empty() => std::path::PathBuf::from(c),
        _ => download::downloads_root_par_defaut(),
    };
    let racine = download::verifier_dossier_ecriture(&racine).unwrap_or_else(|e| {
        tracing::warn!("dossier de telechargements inutilisable ({e}), retour au defaut");
        download::downloads_root_par_defaut()
    });
    tracing::info!("telechargements : {}", racine.display());
    download::definir_racine_telechargements(racine.clone());

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
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
            scheduler::reglages_planificateur,
            scheduler::set_reglages_planificateur,
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
            shelf::get_downloads_dir,
            shelf::set_downloads_dir,
            shelf::reset_downloads_dir,
            shelf::forget_history,
            shelf::add_bookmark,
            challenge::open_challenge,
            challenge::solve_challenge,
            challenge::cancel_challenge
        ])
        .manage(download::DownloadState::default())
        .setup(|app| {
            let handle = app.state::<Arc<Supervisor>>().inner().clone();
            // Clone AVANT le `move` ci-dessous : le planificateur a besoin du
            // même superviseur, et `async move` consomme la variable.
            let pour_planificateur = handle.clone();
            tauri::async_runtime::spawn(async move { handle.run().await });

            // Tâche 3.1 : le planificateur tourne en tâche de fond, à côté du
            // superviseur. Il interroge le moteur par la même voie que
            // l'interface (jamais directement) et n'annonce ses résultats que
            // par l'évènement `maj://nouveaux-chapitres` — l'affichage est
            // l'affaire de la tâche 3.2, pas d'ici.
            let arret = scheduler::nouvel_arret();
            let base = app.state::<Arc<store::Store>>().inner().clone();
            tauri::async_runtime::spawn(scheduler::demarrer(
                scheduler::moteur(pour_planificateur),
                base,
                app.handle().clone(),
                arret,
            ));
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

#[cfg(test)]
mod tests {
    /// Toute commande enregistrée doit être réellement appelée par l'interface.
    ///
    /// Mesuré le 2026-10-04 : `commands::search` et `library::existing_chapters`
    /// étaient enregistrées depuis la phase 0 et n'étaient plus appelées depuis
    /// la 0.9 — du code qui compile, se teste vert, et ne sert à rien. Pire,
    /// `search` portait une sémantique de classification opposée à `latest` pour
    /// le même réglage. Une commande morte n'échoue jamais ; il faut donc une
    /// porte qui la fasse échouer.
    ///
    /// La recherche est volontairement grossière : elle cherche le nom de la
    /// commande dans `page.tsx`, sans distinguer la forme de l'appel. Une
    /// commande citée dans un commentaire ne poserait pas de problème ici — le
    /// but est d'attraper l'oubli, pas de prouver l'absence.
    #[test]
    fn aucune_commande_tauri_nest_morte() {
        let main = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/main.rs"),
        )
        .expect("src/main.rs");
        let page = std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../app/page.tsx"),
        )
        .expect("app/page.tsx");

        let debut = main
            .find("generate_handler![")
            .expect("generate_handler! introuvable");
        let fin = main[debut..]
            .find("])")
            .map(|i| debut + i)
            .expect("fin du handler introuvable");
        let bloc = &main[debut..fin];

        let mut enregistrees: Vec<&str> = bloc
            .split("::")
            .skip(1)
            .filter_map(|mot| {
                mot.trim()
                    .trim_end_matches(',')
                    .split(|c: char| !(c.is_alphanumeric() || c == '_'))
                    .find(|s| !s.is_empty())
            })
            .collect();
        enregistrees.sort_unstable();
        enregistrees.dedup();
        assert!(
            !enregistrees.is_empty(),
            "aucune commande extraite : le parseur du handler a régressé"
        );

        let mortes: Vec<&&str> = enregistrees
            .iter()
            .filter(|c| !page.contains(**c))
            .collect();
        assert!(
            mortes.is_empty(),
            "commandes enregistrées mais jamais appelées par l'interface : {:?}. \
             Une commande morte ne casse rien : supprimez-la, ou appelez-la.",
            mortes
        );
    }
}
