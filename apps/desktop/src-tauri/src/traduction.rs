//! Module de traduction EN → FR pour les bulles de webtoon.
//!
//! Stratégie (ADR 0008) : hybride Gemini en ligne + opus-mt hors ligne en repli.
//! La clé Gemini est fournie par l'utilisateur dans les paramètres de l'application,
//! stockée dans le trousseau de l'OS (`keyring`), jamais en clair dans SQLite.
//!
//! Architecture :
//!   - `Traducteur` gère les deux fournisseurs et le fallback.
//!   - Les commandes Tauri exposent : `traduire_texte`, `definir_cle_gemini`,
//!     `cle_gemini_configuree`.
//!   - opus-mt local est un stub pour l'instant : le modèle ONNX sera branché
//!     quand la pipeline vision (T2) fournira le texte à traduire.

use std::sync::Arc;

use keyring::Entry;
use serde::{Deserialize, Serialize};
use tauri::State;
use thiserror::Error;

use crate::store::Store;

// ---------------------------------------------------------------------------
// Erreurs
// ---------------------------------------------------------------------------

#[derive(Error, Debug, Clone)]
pub enum ErreurTraduction {
    #[error("clé API Gemini non configurée")]
    CleManquante,
    #[error("erreur réseau Gemini : {0}")]
    Reseau(String),
    #[error("réponse Gemini inattendue : {0}")]
    Reponse(String),
    #[error("traduction locale indisponible (modèle non chargé)")]
    LocalIndisponible,
    #[error("trousseau indisponible : {0}")]
    Trousseau(String),
    #[error("quota Gemini atteint (429)")]
    Quota,
}

// ---------------------------------------------------------------------------
// Constantes
// ---------------------------------------------------------------------------

/// Modèle Gemini utilisé pour la traduction.
const MODELE_GEMINI: &str = "gemini-3.5-flash";

// ---------------------------------------------------------------------------
// Types du contrat de traduction
// ---------------------------------------------------------------------------

/// Type de bloc reconnu sur une page de webtoon.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TypeBloc {
    Dialogue,
    Narration,
    Sfx,
    FreeText,
    Ignore,
}

/// Résultat du pipeline sur un bloc.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum StatutBloc {
    Ok,
    Review,
    Fallback,
}

/// Boîte englobante d'un bloc, en pixels.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Bbox {
    pub x: i64,
    pub y: i64,
    pub w: i64,
    pub h: i64,
}

/// Bloc de texte détecté sur une page.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlocTraduction {
    /// Identifiant stable du bloc sur la page.
    pub id: String,
    /// Classification du bloc.
    #[serde(rename = "type")]
    pub type_bloc: TypeBloc,
    /// Position et taille dans la page.
    pub bbox: Bbox,
    /// Texte source lu par l'OCR (anglais attendu).
    #[serde(rename = "originalText")]
    pub original_text: Option<String>,
    /// Texte traduit (français). Vide si `status` n'est pas `ok`.
    #[serde(rename = "translatedText")]
    pub translated_text: Option<String>,
    /// Ordre de lecture sur la page (0 = premier bloc lu).
    #[serde(rename = "readingOrder")]
    pub reading_order: i64,
    /// Résultat du pipeline.
    #[serde(rename = "status")]
    pub status: StatutBloc,
    /// Score de confiance OCR/traduction, 0 = inconnu, 1 = certain.
    pub confidence: Option<f64>,
}

/// Page de webtoon prête à être traduite / affichée.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageTranslation {
    /// Identifiant stable de la page.
    #[serde(rename = "pageId")]
    pub page_id: String,
    /// URL stable du manga.
    #[serde(rename = "mangaUrl")]
    pub manga_url: Option<String>,
    /// URL stable du chapitre.
    #[serde(rename = "chapterUrl")]
    pub chapter_url: String,
    /// Nom d'entrée MangaParserSource.
    pub source: String,
    /// Largeur de la page en pixels.
    pub width: i64,
    /// Hauteur de la page en pixels.
    pub height: i64,
    /// Blocs lus sur la page, triés par `readingOrder` croissant.
    pub blocks: Vec<BlocTraduction>,
}

// ---------------------------------------------------------------------------
// Traducteur
// ---------------------------------------------------------------------------

#[derive(Debug, Default)]
pub struct Traducteur {
    http: reqwest::Client,
}

impl Traducteur {
    #[allow(clippy::new_without_default)]
    pub fn new() -> Self {
        Self {
            http: reqwest::Client::new(),
        }
    }

    /// Lit la clé Gemini depuis le trousseau de l'OS.
    fn cle_gemini() -> Result<Option<String>, String> {
        let entree = Entry::new("tatami", "gemini-api-key").map_err(|e| e.to_string())?;
        match entree.get_password() {
            Ok(cle) if !cle.is_empty() => Ok(Some(cle)),
            _ => Ok(None),
        }
    }

    /// Traduit via l'API Gemini.
    async fn traduire_gemini(&self, texte: &str, cle: &str) -> Result<String, ErreurTraduction> {
        let systeme = "You are a manga/webtoon translator. \
            Translate English speech bubbles to French. \
            Preserve the speaker's tone. Keep it short. \
            Output ONLY the French translation, nothing else.";

        let url = format!(
            "https://generativelanguage.googleapis.com/v1beta/models/{}:generateContent?key={}",
            MODELE_GEMINI, cle
        );

        let corps = serde_json::json!({
            "system_instruction": { "parts": [{ "text": systeme }] },
            "contents": [{ "parts": [{ "text": texte }] }],
            "generationConfig": {
                "temperature": 0.3,
                "maxOutputTokens": 200,
                "thinkingConfig": { "thinkingBudget": 0 },
            }
        });

        let reponse = self
            .http
            .post(&url)
            .header("Content-Type", "application/json")
            .json(&corps)
            .send()
            .await
            .map_err(|e| ErreurTraduction::Reseau(e.to_string()))?;

        if !reponse.status().is_success() {
            let status = reponse.status();
            let body = reponse.text().await.unwrap_or_default();
            if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
                return Err(ErreurTraduction::Quota);
            }
            return Err(ErreurTraduction::Reseau(format!(
                "HTTP {}: {}",
                status,
                body.chars().take(200).collect::<String>()
            )));
        }

        let json: serde_json::Value = reponse
            .json()
            .await
            .map_err(|e| ErreurTraduction::Reponse(e.to_string()))?;

        let Some(texte_reponse) = json
            .get("candidates")
            .and_then(|c| c.get(0))
            .and_then(|c| c.get("content"))
            .and_then(|c| c.get("parts"))
            .and_then(|p| p.get(0))
            .and_then(|p| p.get("text"))
            .and_then(|t| t.as_str())
        else {
            return Err(ErreurTraduction::Reponse(
                "pas de texte dans la réponse Gemini".into(),
            ));
        };

        Ok(texte_reponse.trim().to_string())
    }

    /// Traduit via le modèle local (stub — ONNX à brancher en T2).
    pub async fn traduire_local(&self, _texte: &str) -> Result<String, ErreurTraduction> {
        Err(ErreurTraduction::LocalIndisponible)
    }

    /// Traduit un texte anglais vers le français.
    ///
    /// Ordre des fournisseurs :
    /// 1. Gemini (si une clé est configurée).
    /// 2. opus-mt local (quand le modèle sera chargé).
    pub async fn traduire(&self, texte: &str) -> Result<String, ErreurTraduction> {
        if texte.trim().is_empty() {
            return Ok(texte.to_string());
        }

        if let Ok(Some(cle)) = Self::cle_gemini() {
            if !cle.is_empty() {
                match self.traduire_gemini(texte, &cle).await {
                    Ok(trad) => {
                        tracing::debug!(
                            "traduction gemini: {} chars -> {} chars",
                            texte.len(),
                            trad.len()
                        );
                        return Ok(trad);
                    }
                    Err(e) => {
                        tracing::warn!(
                            "Gemini a échoué pour « {}… » : {e}, repli sur le local",
                            &texte.chars().take(40).collect::<String>()
                        );
                    }
                }
            }
        }

        self.traduire_local(texte).await
    }
}

// ---------------------------------------------------------------------------
// Commandes Tauri
// ---------------------------------------------------------------------------

/// Traduit une page complète en utilisant le glossaire de la série.
#[tauri::command]
pub async fn traduire_page(
    store: State<'_, Arc<Store>>,
    page: PageTranslation,
    series_key: String,
) -> Result<PageTranslation, String> {
    use crate::prompt::GlossaryEntry;
    use crate::translator::{traduire_page, GeminiTranslator, LocalTranslator};

    // Récupérer le glossaire de la série.
    let entries = store.list_glossary(&series_key).map_err(|e| e.to_string())?;
    let glossaire: Vec<GlossaryEntry<'_>> = entries
        .iter()
        .map(|e| GlossaryEntry {
            term: e.term.as_str(),
            translation: e.translation.as_str(),
            register: e.register.as_deref(),
        })
        .collect();

    let translator = GeminiTranslator::new();
    let fallback = LocalTranslator::new();
    traduire_page(&translator, &fallback, &page, &glossaire)
        .await
        .map_err(|e| e.to_string())
}

/// Traduit un texte anglais vers le français.
#[tauri::command]
pub async fn traduire_texte(
    _store: State<'_, Arc<Store>>,
    texte: String,
) -> Result<String, String> {
    let traducteur = Traducteur::new();
    traducteur.traduire(&texte).await.map_err(|e| e.to_string())
}

/// Enregistre la clé API Gemini fournie par l'utilisateur dans le trousseau.
///
/// Une chaîne vide supprime la clé (retour au mode hors ligne uniquement).
#[tauri::command]
pub async fn definir_cle_gemini(cle: String) -> Result<(), String> {
    if cle.is_empty() {
        let entree = Entry::new("tatami", "gemini-api-key").map_err(|e| e.to_string())?;
        entree.delete_credential().ok();
        return Ok(());
    }
    let entree = Entry::new("tatami", "gemini-api-key").map_err(|e| e.to_string())?;
    entree.set_password(&cle).map_err(|e| e.to_string())?;
    Ok(())
}

/// Indique si une clé Gemini est configurée.
#[tauri::command]
pub async fn cle_gemini_configuree() -> Result<bool, String> {
    let entree = Entry::new("tatami", "gemini-api-key").map_err(|e| e.to_string())?;
    match entree.get_password() {
        Ok(cle) => Ok(!cle.is_empty()),
        Err(_) => Ok(false),
    }
}
