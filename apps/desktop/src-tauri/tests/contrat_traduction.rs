//! Tests de contrat pour le schéma de traduction (`contract/translation/schema.json`).
//!
//! Deux vérifications :
//! 1. La fixture valide contre le schéma JSON (via `jsonschema`).
//! 2. La sérialisation/désérialisation Rust des types du contrat respecte
//!    la même forme que la fixture.

use std::path::PathBuf;

use jsonschema::JSONSchema;
use tatami_desktop::traduction::{Bbox, BlocTraduction, PageTranslation, StatutBloc, TypeBloc};

fn contrat_traduction_chemin(nom: &str) -> PathBuf {
    let racine = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../");
    racine.join("contract/translation").join(nom)
}

#[test]
fn contrat_traduction_fixture_valide() {
    let schema_chemin = contrat_traduction_chemin("schema.json");
    let schema_json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&schema_chemin).expect("schema absent"))
            .expect("schema JSON invalide");
    let schema = JSONSchema::compile(&schema_json).expect("schema invalide");

    let fixture_chemin = contrat_traduction_chemin("fixtures/page-basic.json");
    let fixture: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&fixture_chemin).expect("fixture absente"))
            .expect("fixture JSON invalide");

    let result = schema.validate(&fixture);
    if let Err(errors) = result {
        let messages: Vec<String> = errors.map(|e| format!("{}", e)).collect();
        panic!("fixture invalide : {}", messages.join(" | "));
    }
}

#[test]
fn contrat_traduction_types_rust_accord() {
    let fixture = PageTranslation {
        page_id: "page-1".into(),
        manga_url: Some("manga-1".into()),
        chapter_url: "chapter-1".into(),
        source: "WEBTOONS_EN".into(),
        width: 800,
        height: 1200,
        blocks: vec![BlocTraduction {
            id: "b1".into(),
            type_bloc: TypeBloc::Dialogue,
            bbox: Bbox {
                x: 10,
                y: 20,
                w: 200,
                h: 40,
            },
            original_text: Some("HELLO WORLD".into()),
            translated_text: Some("Bonjour le monde".into()),
            reading_order: 0,
            status: StatutBloc::Ok,
            confidence: Some(0.92),
            font_size: None,
            font_family: None,
            line_breaks: None,
        }],
    };

    let value = serde_json::to_value(&fixture).expect("sérialisation Rust invalide");
    let schema_chemin = contrat_traduction_chemin("schema.json");
    let schema_json: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&schema_chemin).expect("schema absent"))
            .expect("schema JSON invalide");
    let schema = JSONSchema::compile(&schema_json).expect("schema invalide");

    let result = schema.validate(&value);
    if let Err(errors) = result {
        let messages: Vec<String> = errors.map(|e| format!("{}", e)).collect();
        panic!("types Rust invalides : {}", messages.join(" | "));
    }
}
