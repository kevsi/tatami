#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod supervisor;

use std::sync::Arc;

use supervisor::{dev_sidecar_paths, Supervisor};
use tauri::Manager;

fn main() {
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .init();

    let (java, classpath) = dev_sidecar_paths();
    tracing::info!("sidecar java={} (dev)", java.display());
    let sup: Arc<Supervisor> = Supervisor::new(java, classpath);

    tauri::Builder::default()
        .manage(sup)
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
