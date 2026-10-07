//! Tests de contrat pour le sidecar vision (T1.3).
//!
//! Vérifie que :
//! 1. Les fixtures de `contract/protocol.fixtures.json` sont conformes au schéma
//!    de `contract/protocol.json`.
//! 2. Les types Rust de `vision.rs` sérialisent dans la forme attendue par le
//!    schéma.

use std::fs;

use jsonschema::JSONSchema;
use serde_json::json;
use tatami_desktop::vision::{
    CapabilitiesResult, DetectResult, Detection, HealthResult, InpaintResult, LoadPackResult,
    OcrResult, PackManifest, VisionBbox, VisionModel, VisionOcrLine,
};

fn charger_fixture(nom: &str) -> serde_json::Value {
    let chemin = "../../../contract/protocol.fixtures.json".to_string();
    let contenu = fs::read_to_string(&chemin).expect("fixture absente");
    let fixtures: serde_json::Value =
        serde_json::from_str(&contenu).expect("fixture JSON invalide");
    fixtures.get(nom).expect("fixture manquante").clone()
}

#[test]
fn contrat_vision_detect_valide() {
    let schema_json: serde_json::Value = serde_json::from_str(
        &fs::read_to_string("../../../contract/protocol.json").expect("schema absent"),
    )
    .expect("schema JSON invalide");
    let schema = JSONSchema::compile(&schema_json).expect("schema invalide");
    let fixture = charger_fixture("detect");
    let result = schema.validate(&fixture);
    if let Err(errors) = result {
        let messages: Vec<String> = errors.map(|e| format!("{}", e)).collect();
        panic!("detect invalide : {}", messages.join(" | "));
    }
}

#[test]
fn contrat_vision_ocr_valide() {
    let schema_json: serde_json::Value = serde_json::from_str(
        &fs::read_to_string("../../../contract/protocol.json").expect("schema absent"),
    )
    .expect("schema JSON invalide");
    let schema = JSONSchema::compile(&schema_json).expect("schema invalide");
    let fixture = charger_fixture("ocr");
    let result = schema.validate(&fixture);
    if let Err(errors) = result {
        let messages: Vec<String> = errors.map(|e| format!("{}", e)).collect();
        panic!("ocr invalide : {}", messages.join(" | "));
    }
}

#[test]
fn contrat_vision_inpaint_valide() {
    let schema_json: serde_json::Value = serde_json::from_str(
        &fs::read_to_string("../../../contract/protocol.json").expect("schema absent"),
    )
    .expect("schema JSON invalide");
    let schema = JSONSchema::compile(&schema_json).expect("schema invalide");
    let fixture = charger_fixture("inpaint");
    let result = schema.validate(&fixture);
    if let Err(errors) = result {
        let messages: Vec<String> = errors.map(|e| format!("{}", e)).collect();
        panic!("inpaint invalide : {}", messages.join(" | "));
    }
}

#[test]
fn contrat_vision_load_pack_valide() {
    let schema_json: serde_json::Value = serde_json::from_str(
        &fs::read_to_string("../../../contract/protocol.json").expect("schema absent"),
    )
    .expect("schema JSON invalide");
    let schema = JSONSchema::compile(&schema_json).expect("schema invalide");
    let fixture = charger_fixture("loadPack");
    let result = schema.validate(&fixture);
    if let Err(errors) = result {
        let messages: Vec<String> = errors.map(|e| format!("{}", e)).collect();
        panic!("loadPack invalide : {}", messages.join(" | "));
    }
}

#[test]
fn contrat_vision_health_valide() {
    let schema_json: serde_json::Value = serde_json::from_str(
        &fs::read_to_string("../../../contract/protocol.json").expect("schema absent"),
    )
    .expect("schema JSON invalide");
    let schema = JSONSchema::compile(&schema_json).expect("schema invalide");
    let fixture = charger_fixture("health");
    let result = schema.validate(&fixture);
    if let Err(errors) = result {
        let messages: Vec<String> = errors.map(|e| format!("{}", e)).collect();
        panic!("health invalide : {}", messages.join(" | "));
    }
}

#[test]
fn contrat_vision_types_rust_accord() {
    let schema_json: serde_json::Value = serde_json::from_str(
        &fs::read_to_string("../../../contract/protocol.json").expect("schema absent"),
    )
    .expect("schema JSON invalide");
    let schema = JSONSchema::compile(&schema_json).expect("schema invalide");

    let detect = DetectResult {
        detections: vec![Detection {
            bbox: VisionBbox {
                x: 10,
                y: 20,
                w: 200,
                h: 40,
            },
            classe: 0,
            score: 0.98,
        }],
    };
    let ocr = OcrResult {
        lines: vec![VisionOcrLine {
            text: "HELLO".into(),
            bbox: VisionBbox {
                x: 10,
                y: 20,
                w: 100,
                h: 20,
            },
        }],
    };
    let inpaint = InpaintResult {
        data: "base64".into(),
        content_type: "image/png".into(),
        tiles: 1,
        ms: 12000,
    };
    let load_pack = LoadPackResult {
        pack_id: "tatami-vision".into(),
        version: "0.1.0".into(),
    };
    let capabilities = CapabilitiesResult {
        pack_id: "tatami-vision".into(),
        version: "0.1.0".into(),
        methods: vec!["detect".into(), "ocr".into()],
        inpaint_disponible: false,
    };
    let health = HealthResult {
        ok: true,
        pack_id: Some("tatami-vision".into()),
        version: Some("0.1.0".into()),
        models: vec!["detector".into(), "ocr".into()],
    };
    let manifest = PackManifest {
        pack_id: "tatami-vision".into(),
        version: "0.1.0".into(),
        models: vec![VisionModel {
            id: "detector".into(),
            path: "models/detector.onnx".into(),
            sha256: "abcd".into(),
        }],
        sha256: "efgh".into(),
    };

    for (nom, valeur) in [
        ("detect", serde_json::to_value(&detect).unwrap()),
        ("ocr", serde_json::to_value(&ocr).unwrap()),
        ("inpaint", serde_json::to_value(&inpaint).unwrap()),
        ("loadPack", serde_json::to_value(&load_pack).unwrap()),
        ("health", serde_json::to_value(&health).unwrap()),
        ("capabilities", serde_json::to_value(&capabilities).unwrap()),
    ] {
        let enveloppe = json!({ "id": nom, "ok": true, "result": valeur });
        let result = schema.validate(&enveloppe);
        let messages: Vec<String> = if let Err(errors) = result {
            errors.map(|e| format!("{}", e)).collect()
        } else {
            vec![]
        };
        if !messages.is_empty() {
            panic!("types Rust invalides pour {nom} : {}", messages.join(" | "));
        }
    }

    let manifest_value = serde_json::to_value(&manifest).unwrap();
    let enveloppe = json!({ "id": "manifest", "ok": true, "result": manifest_value });
    let result = schema.validate(&enveloppe);
    let messages: Vec<String> = if let Err(errors) = result {
        errors.map(|e| format!("{}", e)).collect()
    } else {
        vec![]
    };
    if !messages.is_empty() {
        panic!(
            "types Rust invalides pour manifest : {}",
            messages.join(" | ")
        );
    }
}
