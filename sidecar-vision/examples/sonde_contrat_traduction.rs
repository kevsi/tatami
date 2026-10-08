//! Chaîne NDJSON complète du sidecar : `hello` → `loadPack` → `translate`.
//!
//! Ce que cette sonde prouve, et qu'aucun test ne peut prouver : le contrat
//! **réellement** échangé par le cœur Rust. Un test unitaire du sidecar peut
//! passer avec un nom de méthode ou un nom de champ faux ; ici on écrit les
//! octets que `SuperviseurVision::demander` écrit, et on lit ce qui revient.
//!
//! Le chemin mesuré (`tatami-vision/src/bin/tatami-vision.rs`, ligne `translate`)
//! rend `{"translatedText": …, "ms": …}` ; c'est ce nom de champ que le cœur
//! cherche. Un écart ici se verrait en `LocalIndisponible`, pas en crash.
//!
//! Usage : `cargo run --release --example sonde_contrat_traduction`

use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};

/// Racine du pack, comme le Superviseur la résout.
fn racine_du_pack() -> PathBuf {
    if let Ok(p) = std::env::var("TATAMI_PACK") {
        return PathBuf::from(p);
    }
    PathBuf::from(std::env::var("APPDATA").unwrap_or_default())
        .join("Tatami")
        .join("packs")
        .join("vision-fr-0.1.0")
}

/// Le binaire que l'application supervise.
fn binaire() -> PathBuf {
    if let Ok(p) = std::env::var("TATAMI_VISION_BIN") {
        return PathBuf::from(p);
    }
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("release")
        .join(if cfg!(windows) {
            "tatami-vision.exe"
        } else {
            "tatami-vision"
        })
}

fn main() {
    let racine = racine_du_pack();
    let exe = binaire();
    println!("binaire : {}", exe.display());
    println!("pack    : {}", racine.display());

    let mut enfant = match Command::new(&exe)
        .env(
            "ORT_DYLIB_PATH",
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("target")
                .join("release")
                .join("onnxruntime.dll"),
        )
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(e) => e,
        Err(e) => {
            eprintln!("ECHEC : spawn : {e}");
            std::process::exit(1);
        }
    };

    let mut entrée = enfant.stdin.take().expect("stdin piped");
    let mut lecture = BufReader::new(enfant.stdout.take().expect("stdout piped"));

    let mut envoyer = |ligne: &str, lecture: &mut BufReader<std::process::ChildStdout>| -> String {
        if writeln!(entrée, "{ligne}").is_err() {
            return "{\"ECHEC\":\"stdin ferme\"}".to_string();
        }
        let _ = entrée.flush();
        let mut reponse = String::new();
        match lecture.read_line(&mut reponse) {
            Ok(0) => "{\"ECHEC\":\"stdout ferme\"}".to_string(),
            Ok(_) => reponse.trim_end().to_string(),
            Err(e) => format!("{{\"ECHEC\":\"{e}\"}}"),
        }
    };

    // 1. hello
    let r = envoyer(
        r#"{"id":"1","method":"hello","params":{},"timeoutMs":30000}"#,
        &mut lecture,
    );
    println!("\nhello  : {r}");

    // 2. loadPack
    let params = serde_json::json!({ "path": racine.to_string_lossy() });
    let req = serde_json::json!({"id":"2","method":"loadPack","params":params,"timeoutMs":120000});
    let r = envoyer(&req.to_string(), &mut lecture);
    println!("loadPack : {r}");
    if r.contains("MODEL_MISSING") || r.contains("\"error\"") {
        eprintln!("ECHEC : le pack n'a pas été chargé, `translate` ne le sera pas plus.");
        std::process::exit(1);
    }

    // 3. capabilities : la méthode doit y figurer, sinon l'interface ne le
    //    montrera jamais (cf. `Pack::capacites`).
    let r = envoyer(
        r#"{"id":"3","method":"capabilities","params":{},"timeoutMs":30000}"#,
        &mut lecture,
    );
    println!("capabilities : {r}");

    // 4. translate sur les deux bulles de référence §8.5, en Unicode échappé
    //    pour éviter toute Sleep d'encodage dans PowerShell.
    for (i, (en, attendu)) in [("OH MY GOSH!", "Oh mon Dieu !"), ("NOT HERE.", "Pas ici.")]
        .iter()
        .enumerate()
    {
        let params = serde_json::json!({ "text": en });
        let req = serde_json::json!({
            "id": format!("t{}", i + 4),
            "method": "translate",
            "params": params,
            "timeoutMs": 60000u64,
        });
        let r = envoyer(&req.to_string(), &mut lecture);
        let verdict = if r.contains(attendu) {
            "identique"
        } else {
            "DIFFERENT"
        };
        println!("translate [{verdict}] {en:?} -> {r}");
    }

    // 5. shutdown
    let r = envoyer(
        r#"{"id":"9","method":"shutdown","params":{},"timeoutMs":10000}"#,
        &mut lecture,
    );
    println!("shutdown : {r}");
    let _ = enfant.wait();
}
