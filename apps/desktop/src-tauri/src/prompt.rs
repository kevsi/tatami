//! Construction du prompt de traduction par page (T3.2).
//!
//! Ce module construit le prompt envoyé au fournisseur de traduction
//! (Gemini, etc.) à partir des blocs détectés sur une page.
//!
//! Le prompt inclut :
//! - Les blocs dans l'ordre de lecture
//! - Un budget de caractères par bulle
//! - Un contexte minimal (source, langue)
//! - Une consigne de sortie JSON strict

use std::collections::HashMap;

use super::traduction::PageTranslation;

/// Construit le prompt de traduction pour une page.
///
/// # Format du prompt
///
/// Le prompt est structuré en sections :
/// 1. Rôle et consignes générales
/// 2. Contexte de la page
/// 3. Liste des blocs à traduire (ordre de lecture)
/// 4. Contrainte de budget par bulle
/// 5. Format de sortie JSON attendu
///
/// # Paramètres
/// - `page` : page de webtoon avec ses blocs
/// - `glossaire` : termes spécifiques à la série (optionnel)
///
/// # Retour
/// Le prompt complet en anglais (la consigne demande une sortie en français).
pub fn construire_prompt(page: &PageTranslation, glossaire: &HashMap<String, String>) -> String {
    let mut prompt = String::new();

    // Section 1 : Rôle et consignes générales.
    prompt.push_str(
        "You are a manga/webtoon translator. \
         Translate English speech bubbles to French. \
         Preserve the speaker's tone. Keep it short. \
         Output ONLY valid JSON matching the requested schema. \
         Never invent new blocks. Never drop a block.\n\n",
    );

    // Section 2 : Contexte.
    prompt.push_str("## Context\n");
    prompt.push_str(&format!("- Source: {}\n", page.source));
    if let Some(manga_url) = &page.manga_url {
        prompt.push_str(&format!("- Manga: {}\n", manga_url));
    }
    prompt.push_str(&format!("- Page: {}x{} px\n", page.width, page.height));
    prompt.push_str("- Language: English → French\n\n");

    // Section 3 : Glossaire.
    if !glossaire.is_empty() {
        prompt.push_str("## Glossary\n");
        for (terme, traduction) in glossaire {
            prompt.push_str(&format!("- {} → {}\n", terme, traduction));
        }
        prompt.push('\n');
    }

    // Section 4 : Blocs à traduire.
    prompt.push_str("## Blocks to translate\n");
    prompt.push_str(&format!("Total blocks: {}\n\n", page.blocks.len()));

    for bloc in &page.blocks {
        let budget = calculer_budget(&bloc.bbox, page.width, page.height);
        prompt.push_str(&format!(
            "- id={} type={:?} order={} budget={} chars\n",
            bloc.id, bloc.type_bloc, bloc.reading_order, budget
        ));
        if let Some(original) = &bloc.original_text {
            prompt.push_str(&format!("  original: \"{}\"\n", original));
        } else {
            prompt.push_str("  original: \"\"\n");
        }
    }

    // Section 5 : Format de sortie JSON.
    prompt.push_str("\n## Output format\n");
    prompt.push_str("Return a JSON array of objects, one per input block, in the same order:\n");
    prompt.push_str("[\n");
    prompt.push_str("  {\n");
    prompt.push_str("    \"id\": \"same id as input\",\n");
    prompt.push_str("    \"translatedText\": \"French translation\",\n");
    prompt.push_str("    \"status\": \"ok\"\n");
    prompt.push_str("  }\n");
    prompt.push_str("]\n");
    prompt.push_str("Rules:\n");
    prompt
        .push_str("- The array must contain exactly the same number of objects as input blocks.\n");
    prompt.push_str("- Each object must keep the exact `id` from the input.\n");
    prompt.push_str("- `translatedText` must be in French.\n");
    prompt.push_str("- `status` is always \"ok\".\n");
    prompt.push_str("- No extra text, no markdown, no explanation.\n");

    prompt
}

/// Calcule le budget de caractères pour un bloc.
///
/// Le budget est proportionnel à la surface du bloc par rapport à la page,
/// avec un minimum de 20 caractères et un maximum de 200.
///
/// # Formule
/// `budget = clamp(20, 200, (bbox_area / page_area) * 500)`
pub fn calculer_budget(bbox: &super::traduction::Bbox, page_width: i64, page_height: i64) -> usize {
    let page_area = (page_width * page_height).max(1) as f64;
    let bbox_area = (bbox.w * bbox.h).max(0) as f64;
    let ratio = bbox_area / page_area;
    let brut = (ratio * 500.0).round() as usize;
    brut.clamp(20, 200)
}

/// Schéma JSON de validation pour la réponse de traduction.
///
/// Ce schéma est utilisé pour valider que le fournisseur a bien retourné
/// un tableau de blocs avec les champs attendus.
pub fn schema_traduction() -> serde_json::Value {
    serde_json::json!({
        "type": "array",
        "items": {
            "type": "object",
            "required": ["id", "translatedText", "status"],
            "properties": {
                "id": {
                    "type": "string",
                    "minLength": 1
                },
                "translatedText": {
                    "type": "string"
                },
                "status": {
                    "type": "string",
                    "enum": ["ok", "review", "fallback"]
                }
            }
        },
        "minItems": 1
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traduction::{Bbox, BlocTraduction, PageTranslation, StatutBloc, TypeBloc};

    fn bloc_test(id: &str, texte: &str, w: i64, h: i64) -> BlocTraduction {
        BlocTraduction {
            id: id.into(),
            type_bloc: TypeBloc::Dialogue,
            bbox: Bbox { x: 0, y: 0, w, h },
            original_text: Some(texte.into()),
            translated_text: None,
            reading_order: 0,
            status: StatutBloc::Ok,
            confidence: Some(1.0),
        }
    }

    #[test]
    fn prompt_contient_tous_les_blocs() {
        let page = PageTranslation {
            page_id: "p1".into(),
            manga_url: Some("http://m".into()),
            chapter_url: "http://c".into(),
            source: "SRC".into(),
            width: 800,
            height: 1200,
            blocks: vec![
                bloc_test("b1", "Hello", 100, 50),
                bloc_test("b2", "World", 200, 100),
            ],
        };
        let glossaire = HashMap::new();
        let prompt = construire_prompt(&page, &glossaire);
        assert!(prompt.contains("Total blocks: 2"));
        assert!(prompt.contains("id=b1"));
        assert!(prompt.contains("id=b2"));
        assert!(prompt.contains("original: \"Hello\""));
        assert!(prompt.contains("original: \"World\""));
    }

    #[test]
    fn prompt_inclut_glossaire() {
        let page = PageTranslation {
            page_id: "p1".into(),
            manga_url: None,
            chapter_url: "http://c".into(),
            source: "SRC".into(),
            width: 800,
            height: 1200,
            blocks: vec![bloc_test("b1", "Hello", 100, 50)],
        };
        let mut glossaire = HashMap::new();
        glossaire.insert("Naruto".into(), "Naruto".into());
        let prompt = construire_prompt(&page, &glossaire);
        assert!(prompt.contains("Naruto → Naruto"));
    }

    #[test]
    fn budget_min_max_respectes() {
        // Petit bloc : au moins 20.
        let bbox_small = super::super::traduction::Bbox {
            x: 0,
            y: 0,
            w: 10,
            h: 10,
        };
        assert_eq!(calculer_budget(&bbox_small, 1000, 1000), 20);

        // Gros bloc : au plus 200.
        let bbox_big = super::super::traduction::Bbox {
            x: 0,
            y: 0,
            w: 1000,
            h: 1000,
        };
        assert_eq!(calculer_budget(&bbox_big, 1000, 1000), 200);
    }

    #[test]
    fn schema_est_validation_compatible() {
        let schema = schema_traduction();
        // Le schéma doit être un tableau.
        assert_eq!(schema.get("type").and_then(|t| t.as_str()), Some("array"));
    }
}
