//! Porte de publication : la signature publiée correspond-elle à l'installeur ?
//!
//! Le plugin de mise à jour ne vérifie la signature qu'**au moment d'installer**
//! (`verify_signature`, `tauri-plugin-updater` 2.11.0), donc jamais avant qu'un
//! utilisateur ait téléchargé et lancé l'installeur. Une release dont le `.sig`
//! ne correspond pas part donc sans que rien ne crie, et le premier à le savoir
//! est l'utilisateur, avec un refus d'installation.
//!
//! Ces tests rejouent **exactement** la séquence du plugin, avec la même crate
//! (`minisign-verify` 0.3) : `base64_to_string` sur la clé et la signature,
//! `PublicKey::decode`, `Signature::decode`, puis `verify(data, sig, true)`.
//! Le troisième argument à `true` est `allow_legacy` : il autorise les
//! signatures `Ed` historiques, comme le plugin.
//!
//! Ils sont `#[ignore]` parce qu'ils exigent des fichiers produits par un build
//! release. Le workflow `.github/workflows/release.yml` les lance avec
//! `--ignored` après `tauri build`, en renseignant les quatre variables
//! ci-dessous. Les variables sont lues, jamais devinées : un test qui
//! silencieusement ne teste rien est le défaut que ces portes corrigent.

use base64::Engine;
use minisign_verify::{PublicKey, Signature};

fn variable(nom: &str) -> String {
    std::env::var(nom).unwrap_or_else(|_| panic!("variable d'environnement {nom} absente"))
}

/// Reproduit `base64_to_string` du plugin : la valeur de configuration et le
/// contenu d'un `.sig` sont tous deux de la base64 qui décode vers du texte.
fn base64_vers_texte(valeur: &str) -> String {
    let octets = base64::engine::general_purpose::STANDARD
        .decode(valeur.trim())
        .unwrap_or_else(|e| panic!("base64 invalide : {e}"));
    String::from_utf8(octets).expect("la base64 ne décode pas vers de l'UTF-8")
}

#[test]
#[ignore = "exige un installeur, sa signature et la clé publique (tauri build)"]
fn la_signature_correspond_a_l_installeur() {
    let installeur = std::fs::read(variable("TATAMI_INSTALLEUR"))
        .unwrap_or_else(|e| panic!("lecture de TATAMI_INSTALLEUR : {e}"));
    let signature_fichier = std::fs::read_to_string(variable("TATAMI_INSTALLEUR_SIG"))
        .unwrap_or_else(|e| panic!("lecture de TATAMI_INSTALLEUR_SIG : {e}"));
    let cle_publique = base64_vers_texte(&variable("TATAMI_CLE_PUBLIQUE"));

    let cle = PublicKey::decode(&cle_publique).expect("clé publique minisign illisible");
    let signature = Signature::decode(&base64_vers_texte(&signature_fichier))
        .expect("signature minisign illisible");

    cle.verify(&installeur, &signature, true)
        .unwrap_or_else(|e| {
            panic!(
                "la signature ne correspond pas à l'installeur : {e}. \
             Un binaire publié avec cette signature serait refusé par le plugin."
            )
        });
}

#[test]
#[ignore = "exige latest.json et l'installeur (tauri build)"]
fn le_manifeste_cite_l_installeur_et_sa_signature() {
    let manifeste: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(variable("TATAMI_MANIFESTE"))
            .unwrap_or_else(|e| panic!("lecture de TATAMI_MANIFESTE : {e}")),
    )
    .expect("latest.json n'est pas un JSON valide");

    // Les champs que le plugin exige (`RemoteRelease` + `ReleaseManifestPlatform`).
    let version = manifeste["version"]
        .as_str()
        .expect("latest.json sans « version »");
    let pub_date = manifeste["pub_date"]
        .as_str()
        .expect("latest.json sans « pub_date »");
    let plateforme = &manifeste["platforms"]["windows-x86_64"];
    let url = plateforme["url"]
        .as_str()
        .expect("latest.json sans platforms.windows-x86_64.url");
    let signature = plateforme["signature"]
        .as_str()
        .expect("latest.json sans platforms.windows-x86_64.signature");

    // `pub_date` est un `OffsetDateTime` chez le plugin : RFC 3339 obligatoire.
    // La forme est vérifiée ici sans ajouter la crate `time` en dépendance
    // directe ; le plugin, lui, refusera un manifeste mal formé à l'installation.
    let forme_rfc3339 = pub_date.len() == 20
        && pub_date.ends_with('Z')
        && pub_date.as_bytes()[4] == b'-'
        && pub_date.as_bytes()[7] == b'-'
        && pub_date.as_bytes()[10] == b'T'
        && pub_date.as_bytes()[13] == b':'
        && pub_date.as_bytes()[16] == b':'
        && pub_date[..4]
            .parse::<u16>()
            .is_ok_and(|annee| annee >= 2026);
    assert!(
        forme_rfc3339,
        "pub_date « {pub_date} » n'a pas la forme RFC 3339 attendue (AAAA-MM-JJTHH:MM:SSZ, année >= 2026)"
    );

    let signature_fichier = std::fs::read_to_string(variable("TATAMI_INSTALLEUR_SIG"))
        .expect("lecture de TATAMI_INSTALLEUR_SIG");
    assert_eq!(
        signature,
        signature_fichier.trim(),
        "la signature du manifeste n'est pas le contenu du .sig publié"
    );

    let nom = std::path::Path::new(&variable("TATAMI_INSTALLEUR"))
        .file_name()
        .expect("TATAMI_INSTALLEUR sans nom de fichier")
        .to_string_lossy()
        .into_owned();
    assert!(
        url.ends_with(&format!("/{nom}")),
        "l'URL « {url} » ne pointe pas l'installeur « {nom} »"
    );
    assert!(
        url.contains("/releases/download/"),
        "l'URL « {url} » n'est pas une URL d'asset de release GitHub"
    );
    assert!(!version.is_empty(), "latest.json porte une version vide");
}
