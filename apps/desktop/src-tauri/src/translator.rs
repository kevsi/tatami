//! Trait de traduction et implémentations (T3.1).
//!
//! Ce module définit l'abstraction `Translator` utilisée par le reste
//! de l'application, puis fournit l'implémentation Gemini en ligne
//! (`Traducteur`). Le modèle local (ADR 0008) sera ajouté en T3.1
//! quand le runtime ONNX sera branché.

use super::prompt::{construire_prompt, schema_traduction};
use super::traduction::{ErreurTraduction, PageTranslation, StatutBloc, Traducteur};
use std::collections::HashMap;

/// Trait de traduction EN → FR.
///
/// Les implémentations peuvent provenir :
/// - d'un service en ligne (Gemini, etc.)
/// - d'un modèle local (opus-mt ONNX, etc.)
///
/// La traduction doit préserver le ton et rester courte pour des bulles
/// de webtoon. En cas d'échec, l'erreur est propagée : c'est l'appelant
/// qui décide du repli.
#[async_trait::async_trait]
pub trait Translator: Send + Sync {
    /// Traduit un texte anglais vers le français.
    async fn translate(&self, text: &str) -> Result<String, ErreurTraduction>;
}

// ---------------------------------------------------------------------------
// Implémentation Gemini (en ligne)
// ---------------------------------------------------------------------------

/// Fournisseur Gemini en ligne, utilisant la clé stockée dans le trousseau OS.
#[derive(Debug, Default)]
pub struct GeminiTranslator {
    inner: Traducteur,
}

impl GeminiTranslator {
    pub fn new() -> Self {
        Self {
            inner: Traducteur::new(),
        }
    }
}

#[async_trait::async_trait]
impl Translator for GeminiTranslator {
    async fn translate(&self, text: &str) -> Result<String, ErreurTraduction> {
        self.inner.traduire(text).await
    }
}

// ---------------------------------------------------------------------------
// Implémentation locale (stub — modèle ONNX à brancher en T3/T4)
// ---------------------------------------------------------------------------

/// Fournisseur local (stub).
///
/// Ce fournisseur n'est fonctionnel que quand un modèle ONNX est chargé
/// et exposé via les méthodes de `Traducteur`. Pour l'instant, il renvoie
/// systématiquement `LocalIndisponible`.
#[derive(Debug, Default)]
pub struct LocalTranslator {
    inner: Traducteur,
}

impl LocalTranslator {
    pub fn new() -> Self {
        Self {
            inner: Traducteur::new(),
        }
    }
}

#[async_trait::async_trait]
impl Translator for LocalTranslator {
    async fn translate(&self, text: &str) -> Result<String, ErreurTraduction> {
        self.inner.traduire_local(text).await
    }
}

// ---------------------------------------------------------------------------
// Traduction par page avec validation et réessais (T3.2)
// ---------------------------------------------------------------------------

/// Nombre maximum de tentatives de traduction par page.
const TENTATIVES_MAX: usize = 3;

/// Traduit une page complète.
///
/// # Algorithme
/// 1. Construire le prompt avec tous les blocs, leur budget et le glossaire.
/// 2. Appeler le fournisseur de traduction.
/// 3. Valider la réponse JSON contre le schéma.
/// 4. Vérifier que le nombre de blocs en sortie == nombre en entrée.
/// 5. Si échec : réessayer (jusqu'à `TENTATIVES_MAX` fois).
/// 6. Si toujours échec : marquer tous les blocs en `Fallback`.
///
/// # Retour
/// La page mise à jour avec les traductions, ou les blocs en `Fallback`.
pub async fn traduire_page<T>(
    translator: &T,
    page: &PageTranslation,
    glossaire: &HashMap<String, String>,
) -> Result<PageTranslation, ErreurTraduction>
where
    T: Translator,
{
    let prompt = construire_prompt(page, glossaire);
    let schema = schema_traduction();

    for tentative in 0..TENTATIVES_MAX {
        match translator.translate(&prompt).await {
            Ok(reponse) => match valider_reponse(&reponse, page, &schema) {
                Ok(page_traduite) => return Ok(page_traduite),
                Err(e) => {
                    tracing::warn!("tentative {} : réponse invalide : {e}", tentative + 1);
                }
            },
            Err(e) => {
                tracing::warn!("tentative {} : traduction échouée : {e}", tentative + 1);
            }
        }
    }

    // Échec après toutes les tentatives : repli.
    let mut page_fallback = page.clone();
    for bloc in &mut page_fallback.blocks {
        bloc.translated_text = None;
        bloc.status = StatutBloc::Fallback;
    }
    Ok(page_fallback)
}

/// Valide la réponse JSON du fournisseur.
///
/// Vérifie :
/// 1. C'est un tableau JSON.
/// 2. Même nombre d'éléments que les blocs en entrée.
/// 3. Chaque élément a les champs requis (`id`, `translatedText`, `status`).
/// 4. Les `id` correspondent aux blocs en entrée (dans l'ordre).
///
/// Retourne la page mise à jour si valide, ou une erreur.
fn valider_reponse(
    reponse: &str,
    page: &PageTranslation,
    _schema: &serde_json::Value,
) -> Result<PageTranslation, ErreurTraduction> {
    use std::collections::HashSet;

    // Parser le JSON.
    let tableau: serde_json::Value = serde_json::from_str(reponse)
        .map_err(|e| ErreurTraduction::Reponse(format!("JSON invalide : {e}")))?;

    // Vérifier que c'est un tableau.
    if !tableau.is_array() {
        return Err(ErreurTraduction::Reponse(
            "la réponse n'est pas un tableau JSON".into(),
        ));
    }

    let elements = tableau.as_array().unwrap();

    // Vérifier le nombre de blocs.
    if elements.len() != page.blocks.len() {
        return Err(ErreurTraduction::Reponse(format!(
            "nombre de blocs invalide : attendu {}, obtenu {}",
            page.blocks.len(),
            elements.len()
        )));
    }

    // Vérifier les champs de chaque bloc.
    let mut ids_attendus: HashSet<&str> = page.blocks.iter().map(|b| b.id.as_str()).collect();
    let mut page_traduite = page.clone();

    for (i, element) in elements.iter().enumerate() {
        let id = element
            .get("id")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ErreurTraduction::Reponse(format!("bloc {} : id manquant", i)))?;

        // Vérifier que l'id correspond.
        if !ids_attendus.remove(id) {
            return Err(ErreurTraduction::Reponse(format!(
                "bloc {} : id inattendu '{}'",
                i, id
            )));
        }

        let texte = element
            .get("translatedText")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                ErreurTraduction::Reponse(format!("bloc {} : translatedText manquant", i))
            })?;

        let statut = element
            .get("status")
            .and_then(|v| v.as_str())
            .ok_or_else(|| ErreurTraduction::Reponse(format!("bloc {} : status manquant", i)))?;

        // Mettre à jour le bloc.
        if let Some(bloc) = page_traduite.blocks.get_mut(i) {
            bloc.translated_text = Some(texte.into());
            bloc.status = match statut {
                "review" => StatutBloc::Review,
                "fallback" => StatutBloc::Fallback,
                _ => StatutBloc::Ok,
            };
        }
    }

    Ok(page_traduite)
}

// ---------------------------------------------------------------------------
// Erreurs
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traduction::{Bbox, BlocTraduction, StatutBloc, TypeBloc};

    #[tokio::test]
    async fn gemini_sans_cle_retourne_local_indisponible() {
        let translator = GeminiTranslator::new();
        let result = translator.translate("Hello").await;
        // Sans clé configurée, Gemini n'est pas appelé et on retombe sur
        // le local qui renvoie LocalIndisponible.
        assert!(matches!(result, Err(ErreurTraduction::LocalIndisponible)));
    }

    #[tokio::test]
    async fn local_retourne_toujours_indisponible() {
        let translator = LocalTranslator::new();
        let result = translator.translate("Hello").await;
        assert!(matches!(result, Err(ErreurTraduction::LocalIndisponible)));
    }

    #[tokio::test]
    async fn texte_vide_retourne_texte_vide() {
        let translator = GeminiTranslator::new();
        let result = translator.translate("").await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "");
    }

    #[test]
    fn valider_reponse_correcte() {
        let page = PageTranslation {
            page_id: "p1".into(),
            manga_url: None,
            chapter_url: "c".into(),
            source: "S".into(),
            width: 100,
            height: 100,
            blocks: vec![BlocTraduction {
                id: "b1".into(),
                type_bloc: TypeBloc::Dialogue,
                bbox: Bbox {
                    x: 0,
                    y: 0,
                    w: 10,
                    h: 10,
                },
                original_text: Some("Hello".into()),
                translated_text: None,
                reading_order: 0,
                status: StatutBloc::Ok,
                confidence: Some(1.0),
            }],
        };

        let reponse = r#"[{"id":"b1","translatedText":"Bonjour","status":"ok"}]"#;
        let _schema = schema_traduction();
        let result = valider_reponse(reponse, &page, &_schema);
        assert!(result.is_ok());
        let page_traduite = result.unwrap();
        assert_eq!(
            page_traduite.blocks[0].translated_text,
            Some("Bonjour".into())
        );
    }

    #[test]
    fn valider_reponse_mauvais_nombre_de_blocs() {
        let page = PageTranslation {
            page_id: "p1".into(),
            manga_url: None,
            chapter_url: "c".into(),
            source: "S".into(),
            width: 100,
            height: 100,
            blocks: vec![BlocTraduction {
                id: "b1".into(),
                type_bloc: TypeBloc::Dialogue,
                bbox: Bbox {
                    x: 0,
                    y: 0,
                    w: 10,
                    h: 10,
                },
                original_text: Some("Hello".into()),
                translated_text: None,
                reading_order: 0,
                status: StatutBloc::Ok,
                confidence: Some(1.0),
            }],
        };

        let reponse = r#"[]"#; // 0 bloc au lieu de 1.
        let schema = schema_traduction();
        let result = valider_reponse(reponse, &page, &schema);
        assert!(result.is_err());
    }
}
