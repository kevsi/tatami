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
    #[error("consentement refusé par l'utilisateur")]
    ConsentementRefuse,
}

// ---------------------------------------------------------------------------
// Constantes
// ---------------------------------------------------------------------------

/// Modèle Gemini utilisé pour la traduction.
// Le 2026-10-07, ce modèle a changé deux fois dans la même journée. Mesuré :
// `gemini-2.5-flash` listé mais 404 « no longer available to new users » (le
// message nomme son remplaçant), et `gemini-3.5-flash` a 404 silencieusement.
// **Liste ≠ utilisable** : un modèle listé par `/v1beta/models` peut refuser de
// répondre. `gemini-3.8-flash` est le remplaçant nommé par l'API elle-même.
const MODELE_GEMINI: &str = "gemini-3.8-flash";

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
///
/// Sérialisé en minuscules, comme le schéma `contract/translation/schema.json`
/// l'exige (`"ok"`, `"review"`, `"fallback"`). Mesuré le 2026-10-07 : sans ce
/// `rename_all`, le type Rust sortait `"Ok"` et la porte
/// `contrat_traduction_types_rust_accord` échouait — le schéma et le code
/// divergeaient depuis T1.1, et rien d'autre ne les rapprochait.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
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
///
/// Les champs optionnels sont sérialisés **absents** quand ils sont `None`, et
/// non `null`. Le schéma les déclare sans `null` dans leur `type` : mesuré le
/// 2026-10-07, un `null` rendait `PageTranslation` invalide
/// (`null is not of type "string"`), et le type est echangé tel quel avec
/// l'interface TypeScript, qui distingue `undefined` (champ absent) de `null`.
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
    #[serde(rename = "originalText", skip_serializing_if = "Option::is_none")]
    pub original_text: Option<String>,
    /// Texte traduit (français). Vide si `status` n'est pas `ok`.
    #[serde(rename = "translatedText", skip_serializing_if = "Option::is_none")]
    pub translated_text: Option<String>,
    /// Ordre de lecture sur la page (0 = premier bloc lu).
    #[serde(rename = "readingOrder")]
    pub reading_order: i64,
    /// Résultat du pipeline.
    #[serde(rename = "status")]
    pub status: StatutBloc,
    /// Score de confiance OCR/traduction, 0 = inconnu, 1 = certain.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub confidence: Option<f64>,
    /// Taille de police choisie pour le rendu, en px.
    #[serde(rename = "fontSize", skip_serializing_if = "Option::is_none")]
    pub font_size: Option<f64>,
    /// Nom de la police utilisée.
    #[serde(rename = "fontFamily", skip_serializing_if = "Option::is_none")]
    pub font_family: Option<String>,
    /// Indices de coupure dans `translated_text`, triés croissants.
    #[serde(rename = "lineBreaks", skip_serializing_if = "Option::is_none")]
    pub line_breaks: Option<Vec<usize>>,
}

/// Page de webtoon prête à être traduite / affichée.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PageTranslation {
    /// Identifiant stable de la page.
    #[serde(rename = "pageId")]
    pub page_id: String,
    /// URL stable du manga.
    #[serde(rename = "mangaUrl", skip_serializing_if = "Option::is_none")]
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

        // Une bulle par requête. `503` est documenté comme **transitoire** —
        // mesuré le 2026-10-07, l'API renvoie 503 « high demand » sur trois
        // modèles différents en même temps, ce qui n'est ni le quota (429) ni
        // une faute du client. Donc on retente deux fois, pas une.
        let mut dernier_code = None;
        let mut json = None;
        for essai in 0..3 {
            if essai > 0 {
                tokio::time::sleep(std::time::Duration::from_secs(2 * essai)).await;
            }
            let reponse = self
                .http
                .post(&url)
                .header("Content-Type", "application/json")
                .json(&corps)
                .send()
                .await
                .map_err(|e| ErreurTraduction::Reseau(e.to_string()))?;

            let status = reponse.status();
            if status.is_success() {
                json = Some(
                    reponse
                        .json()
                        .await
                        .map_err(|e| ErreurTraduction::Reponse(e.to_string()))?,
                );
                dernier_code = None;
                break;
            }
            let body = reponse.text().await.unwrap_or_default();
            if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
                return Err(ErreurTraduction::Quota);
            }
            // 503 Service Unavailable : laisser retenter, pas abandonner.
            if status != reqwest::StatusCode::SERVICE_UNAVAILABLE {
                return Err(ErreurTraduction::Reseau(format!(
                    "HTTP {}: {}",
                    status,
                    body.chars().take(200).collect::<String>()
                )));
            }
            dernier_code = Some(format!(
                "HTTP 503: {}",
                body.chars().take(200).collect::<String>()
            ));
        }
        let json: serde_json::Value = match json {
            Some(j) => j,
            None => {
                return Err(ErreurTraduction::Reseau(
                    dernier_code.unwrap_or_else(|| "aucun corps".into()),
                ))
            }
        };

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

    /// Clé API OpenRouter, dans l'ordre : trousseau, variable d'environnement.
    ///
    /// La variable `OPENROUTER_API_KEY` existe pour le développement en ligne de
    /// commande : le service OpenCode ne voit pas les `setx` (leçon AGENTS.md §8),
    /// mais un `cargo run` lancé dans le même shell la voit. La production passe
    /// par le trousseau, comme Gemini.
    fn cle_openrouter() -> Result<Option<String>, String> {
        let entree = Entry::new("tatami", "openrouter-api-key").map_err(|e| e.to_string())?;
        match entree.get_password() {
            Ok(cle) if !cle.is_empty() => return Ok(Some(cle)),
            _ => {}
        }
        match std::env::var("OPENROUTER_API_KEY") {
            Ok(cle) if !cle.is_empty() => Ok(Some(cle)),
            _ => Ok(None),
        }
    }

    /// Traduit via l'API OpenRouter (mêmes URL et corps que Gemini : OpenAI
    /// chat/completions). Mesuré le 2026-10-07 : sur les modèles gratuits,
    /// `cohere/north-mini-code:free` est le seul, parmi six testés, à avoir
    /// répondu une traduction française correcte en gardant les retours à la
    /// ligne du bulle — `google/gemma-4-*:free` et `nemotron-*` étaient saturés,
    /// `dots-3-note-preview` aplatissait les sauts de ligne.
    async fn traduire_openrouter(
        &self,
        texte: &str,
        cle: &str,
    ) -> Result<String, ErreurTraduction> {
        const MODELE: &str = "cohere/north-mini-code:free";
        let systeme = "You are a manga/webtoon translator. \
            Translate English speech bubbles to French. \
            Preserve the speaker's tone. Keep it short. \
            Output ONLY the French translation, nothing else.";

        let url = "https://openrouter.ai/api/v1/chat/completions";
        let corps = serde_json::json!({
            "model": MODELE,
            "messages": [
                { "role": "system", "content": systeme },
                { "role": "user", "content": texte }
            ],
            "temperature": 0.3,
            "max_tokens": 200,
            // Mesuré le 2026-10-07 : `cohere/north-mini-code:free` est un modèle
            // **à raisonnement** — sans ce drapeau, les 200 tokens partent dans
            // `reasoning` et `content` revient `null` (`finish_reason: "length"`).
            "reasoning": { "enabled": false }
        });

        let mut dernier_code = None;
        let mut corps_reponse = None;
        for essai in 0..3 {
            if essai > 0 {
                tokio::time::sleep(std::time::Duration::from_secs(2 * essai)).await;
            }
            let reponse = self
                .http
                .post(url)
                .header("Authorization", format!("Bearer {cle}"))
                .header("Content-Type", "application/json")
                .json(&corps)
                .send()
                .await
                .map_err(|e| ErreurTraduction::Reseau(e.to_string()))?;

            let status = reponse.status();
            if status.is_success() {
                corps_reponse = Some(
                    reponse
                        .json()
                        .await
                        .map_err(|e| ErreurTraduction::Reponse(e.to_string()))?,
                );
                dernier_code = None;
                break;
            }
            let body = reponse.text().await.unwrap_or_default();
            if status == reqwest::StatusCode::TOO_MANY_REQUESTS {
                return Err(ErreurTraduction::Quota);
            }
            // `cohere/*` et `nemotron-*` renvoient tous deux « Upstream error »
            // 503 quand le serveur est saturé — même famille que Gemini.
            if status != reqwest::StatusCode::SERVICE_UNAVAILABLE
                && !body.contains("Upstream error")
                && status != reqwest::StatusCode::BAD_GATEWAY
            {
                return Err(ErreurTraduction::Reseau(format!(
                    "HTTP {}: {}",
                    status,
                    body.chars().take(200).collect::<String>()
                )));
            }
            dernier_code = Some(format!(
                "HTTP {}: {}",
                status,
                body.chars().take(200).collect::<String>()
            ));
        }
        let json: serde_json::Value = match corps_reponse {
            Some(j) => j,
            None => {
                return Err(ErreurTraduction::Reseau(
                    dernier_code.unwrap_or_else(|| "aucun corps".into()),
                ))
            }
        };

        let Some(texte_reponse) = json
            .get("choices")
            .and_then(|c| c.get(0))
            .and_then(|c| c.get("message"))
            .and_then(|m| m.get("content"))
            .and_then(|t| t.as_str())
        else {
            return Err(ErreurTraduction::Reponse(
                "pas de texte dans la réponse OpenRouter".into(),
            ));
        };

        Ok(texte_reponse.trim().to_string())
    }

    /// Traduit un texte anglais vers le français.
    ///
    /// Ordre des fournisseurs :
    /// 1. OpenRouter, modèle gratuit (si une clé est configurée).
    /// 2. Gemini (si une clé est configurée).
    /// 3. opus-mt local (quand le modèle sera chargé).
    pub async fn traduire(&self, texte: &str) -> Result<String, ErreurTraduction> {
        if texte.trim().is_empty() {
            return Ok(texte.to_string());
        }

        if let Ok(Some(cle)) = Self::cle_openrouter() {
            if !cle.is_empty() {
                match self.traduire_openrouter(texte, &cle).await {
                    Ok(trad) => {
                        tracing::debug!(
                            "traduction openrouter: {} chars -> {} chars",
                            texte.len(),
                            trad.len()
                        );
                        return Ok(trad);
                    }
                    Err(e) => {
                        tracing::warn!(
                            "OpenRouter a échoué pour « {}… » : {e}, repli sur Gemini/local",
                            &texte.chars().take(40).collect::<String>()
                        );
                    }
                }
            }
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

/// Clé du réglage de consentement à la traduction en ligne.
const REGLAGE_CONSENTEMENT_TRADUCTION: &str = "consentement_traduction_en_ligne";

/// Vérifie si l'utilisateur a consenti à la traduction en ligne.
///
/// Par défaut (réglage absent), le consentement est considéré comme donné
/// pour préserver la rétrocompatibilité. L'utilisateur peut le retirer
/// explicitement via `definir_consentement_traduction`.
fn consentement_traduction_en_ligne(store: &Store) -> Result<bool, String> {
    match store.get_setting(REGLAGE_CONSENTEMENT_TRADUCTION) {
        Ok(Some(valeur)) => match valeur.as_str() {
            "false" => Ok(false),
            "true" => Ok(true),
            _ => Ok(true),
        },
        Ok(None) => Ok(true),
        Err(e) => Err(e),
    }
}

/// Traduit une page complète en utilisant le glossaire de la série.
#[tauri::command]
pub async fn traduire_page(
    store: State<'_, Arc<Store>>,
    page: PageTranslation,
    series_key: String,
) -> Result<PageTranslation, String> {
    use crate::translator::{traduire_page, GeminiTranslator, LocalTranslator};

    // Vérifier le consentement avant tout appel en ligne.
    if !consentement_traduction_en_ligne(store.inner())? {
        return Err(ErreurTraduction::ConsentementRefuse.to_string());
    }

    // 1. Vérifier le cache d'abord.
    if let Ok(Some((cached_page, cached_blocks))) = store.get_page_translation(&page.page_id) {
        // Convertir les données du cache vers PageTranslation.
        let blocks: Vec<BlocTraduction> = cached_blocks
            .into_iter()
            .map(|b| BlocTraduction {
                id: b.id,
                type_bloc: match b.type_bloc.as_str() {
                    "dialogue" => TypeBloc::Dialogue,
                    "narration" => TypeBloc::Narration,
                    "sfx" => TypeBloc::Sfx,
                    "free_text" => TypeBloc::FreeText,
                    _ => TypeBloc::Ignore,
                },
                bbox: Bbox {
                    x: b.bbox_x,
                    y: b.bbox_y,
                    w: b.bbox_w,
                    h: b.bbox_h,
                },
                original_text: b.original_text,
                translated_text: b.translated_text,
                reading_order: b.reading_order,
                status: match b.status.as_str() {
                    "review" => StatutBloc::Review,
                    "fallback" => StatutBloc::Fallback,
                    _ => StatutBloc::Ok,
                },
                confidence: b.confidence,
                font_size: None,
                font_family: None,
                line_breaks: None,
            })
            .collect();

        return Ok(PageTranslation {
            page_id: cached_page.page_id,
            manga_url: cached_page.manga_url,
            chapter_url: cached_page.chapter_url,
            source: cached_page.source,
            width: cached_page.width,
            height: cached_page.height,
            blocks,
        });
    }

    // Récupérer le glossaire de la série.
    use crate::prompt::GlossaryEntry;
    let entries = store
        .list_glossary(&series_key)
        .map_err(|e| e.to_string())?;
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
    let page_traduite = traduire_page(&translator, &fallback, &page, &glossaire)
        .await
        .map_err(|e| e.to_string())?;

    // 2. Sauvegarder en cache si la traduction a réussi (au moins un bloc OK).
    if page_traduite.blocks.iter().any(|b| b.status == StatutBloc::Ok) {
        // Convertir PageTranslation vers PageTranslationRow + BlockRow pour le cache.
        use crate::store::{PageTranslationRow, BlockRow};
        let now = crate::store::now_ms();

        let page_row = PageTranslationRow {
            page_id: page_traduite.page_id.clone(),
            manga_url: page_traduite.manga_url.clone(),
            chapter_url: page_traduite.chapter_url.clone(),
            source: page_traduite.source.clone(),
            width: page_traduite.width,
            height: page_traduite.height,
            pipeline_version: "0.1".into(), // TODO: version réelle du pipeline
            models: "gemini-3.8-flash".into(), // TODO: modèle réel utilisé
            source_lang: "en".into(),
            target_lang: "fr".into(),
            clean_image_ref: None,
            created_at: now,
            updated_at: now,
        };

        let block_rows: Vec<BlockRow> = page_traduite
            .blocks
            .iter()
            .map(|b| BlockRow {
                id: b.id.clone(),
                page_id: page_traduite.page_id.clone(),
                type_bloc: format!("{:?}", b.type_bloc).to_lowercase(),
                bbox_x: b.bbox.x,
                bbox_y: b.bbox.y,
                bbox_w: b.bbox.w,
                bbox_h: b.bbox.h,
                original_text: b.original_text.clone(),
                translated_text: b.translated_text.clone(),
                reading_order: b.reading_order,
                status: format!("{:?}", b.status).to_lowercase(),
                confidence: b.confidence,
                overrides: None,
                created_at: now,
                updated_at: now,
            })
            .collect();

        // Sauvegarder sans bloquer la réponse si erreur.
        if let Err(e) = store.save_page_translation(&page_row, &block_rows) {
            tracing::warn!("échec sauvegarde traduction en cache : {e}");
        }
    }

    Ok(page_traduite)
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

/// Enregistre la clé API OpenRouter fournie par l'utilisateur dans le trousseau.
///
/// Une chaîne vide supprime la clé (retour au mode hors ligne uniquement).
#[tauri::command]
pub async fn definir_cle_openrouter(cle: String) -> Result<(), String> {
    if cle.is_empty() {
        let entree = Entry::new("tatami", "openrouter-api-key").map_err(|e| e.to_string())?;
        entree.delete_credential().ok();
        return Ok(());
    }
    let entree = Entry::new("tatami", "openrouter-api-key").map_err(|e| e.to_string())?;
    entree.set_password(&cle).map_err(|e| e.to_string())?;
    Ok(())
}

/// Indique si une clé OpenRouter est configurée (trousseau ou variable d'env).
#[tauri::command]
pub async fn cle_openrouter_configuree() -> Result<bool, String> {
    match Traducteur::cle_openrouter()? {
        Some(cle) => Ok(!cle.is_empty()),
        None => Ok(false),
    }
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

/// Lit le consentement à la traduction en ligne.
///
/// Retourne `true` si l'utilisateur a accepté l'envoi de texte vers le
/// fournisseur en ligne. En l'absence de réglage explicite, la valeur par
/// défaut est `true` (rétrocompatibilité).
#[tauri::command]
pub async fn obtenir_consentement_traduction(store: State<'_, Arc<Store>>) -> Result<bool, String> {
    consentement_traduction_en_ligne(store.inner())
}

/// Écrit le consentement à la traduction en ligne.
///
/// # Paramètre
/// - `valeur` : `true` pour accepter, `false` pour refuser.
#[tauri::command]
pub async fn definir_consentement_traduction(
    store: State<'_, Arc<Store>>,
    valeur: bool,
) -> Result<(), String> {
    store.set_setting(
        REGLAGE_CONSENTEMENT_TRADUCTION,
        if valeur { "true" } else { "false" },
    )?;
    Ok(())
}

/// Résumé de ce qui serait envoyé au fournisseur pour une page donnée.
///
/// Ce résumé est affiché à l'utilisateur pour qu'il puisse donner ou
/// retirer son consentement en connaissance de cause.
#[tauri::command]
pub async fn resume_traduction_page(
    _store: State<'_, Arc<Store>>,
    page: PageTranslation,
    series_key: String,
) -> Result<ResumeTraduction, String> {
    use crate::prompt::{construire_prompt, GlossaryEntry};

    let entries = _store
        .list_glossary(&series_key)
        .map_err(|e| e.to_string())?;
    let glossaire: Vec<GlossaryEntry<'_>> = entries
        .iter()
        .map(|e| GlossaryEntry {
            term: e.term.as_str(),
            translation: e.translation.as_str(),
            register: e.register.as_deref(),
        })
        .collect();

    let prompt = construire_prompt(&page, &glossaire);
    let nb_blocs = page.blocks.len();
    let nb_caracteres = prompt.chars().count();

    // Compter les blocs contenant du texte original.
    let nb_blocs_texte = page
        .blocks
        .iter()
        .filter(|b| {
            b.original_text
                .as_ref()
                .is_some_and(|t| !t.trim().is_empty())
        })
        .count();

    Ok(ResumeTraduction {
        nb_blocs,
        nb_blocs_texte,
        nb_caracteres_prompt: nb_caracteres,
        envoi_images: false,
        fournisseur: "Gemini".into(),
        mode_hors_ligne_possible: true,
    })
}

/// Résumé de ce qui serait envoyé au fournisseur.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResumeTraduction {
    /// Nombre total de blocs sur la page.
    pub nb_blocs: usize,
    /// Nombre de blocs contenant du texte à traduire.
    pub nb_blocs_texte: usize,
    /// Taille du prompt en caractères.
    pub nb_caracteres_prompt: usize,
    /// Indique si des images seraient envoyées.
    pub envoi_images: bool,
    /// Nom du fournisseur en ligne.
    pub fournisseur: String,
    /// Indique si un mode hors ligne est disponible.
    pub mode_hors_ligne_possible: bool,
}
