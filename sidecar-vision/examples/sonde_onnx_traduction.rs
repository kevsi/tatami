//! Noms réels des tenseurs des trois modèles de traduction.
//!
//! Ces noms ne se devinent pas : le code de génération doit les utiliser
//! tels quels, et le spike du 2026-10-07 a déjà payé une erreur de ce genre
//! (`decoder_model` sort 12 pasts encodeur + 12 décodeur, alors que
//! `decoder_with_past_model` n'en ressort que 12).
//!
//! Usage : `cargo run --release --example sonde_onnx_traduction`

use std::path::PathBuf;

use ort::session::Session;
use tatami_vision::pack::Pack;

fn racine_du_pack() -> PathBuf {
    if let Ok(p) = std::env::var("TATAMI_PACK") {
        return PathBuf::from(p);
    }
    PathBuf::from(std::env::var("APPDATA").unwrap_or_default())
        .join("Tatami")
        .join("packs")
        .join("vision-fr-0.1.0")
}

fn decrire(nom_modele: &str, chemin: &std::path::Path) {
    println!("\n=== {nom_modele} ({})", chemin.display());
    let mut builder = match Session::builder() {
        Ok(b) => b,
        Err(e) => {
            eprintln!("ECHEC : builder : {e}");
            std::process::exit(1);
        }
    };
    let session = match builder.commit_from_file(chemin) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("ECHEC : chargement de {nom_modele} : {e}");
            std::process::exit(1);
        }
    };
    println!("  entrees ({}):", session.inputs().len());
    for i in session.inputs() {
        println!("    - {:<28} {}", i.name(), format!("{:?}", i.dtype()));
    }
    println!("  sorties ({}):", session.outputs().len());
    for o in session.outputs() {
        println!("    - {:<28} {}", o.name(), format!("{:?}", o.dtype()));
    }
}

fn main() {
    let racine = racine_du_pack();
    println!("pack : {}", racine.display());
    let pack = match Pack::ouvrir(&racine) {
        Ok(p) => p,
        Err((code, message)) => {
            eprintln!("ECHEC : pack illisible ({code:?}) : {message}");
            std::process::exit(1);
        }
    };
    let Some(t) = pack.traduction.as_ref() else {
        eprintln!("ECHEC : pas de dossier traduction/ dans le pack");
        std::process::exit(1);
    };
    decrire("encoder", &t.encoder);
    decrire("decoder", &t.decoder);
    decrire("decoder_with_past", &t.decoder_with_past);
}
