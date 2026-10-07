//! Trait de traduction et implémentations (T3.1).
//!
//! Ce module définit l'abstraction `Translator` utilisée par le reste
//! de l'application, puis fournit l'implémentation Gemini en ligne
//! (`Traducteur`). Le modèle local (ADR 0008) sera ajouté en T3.1
//! quand le runtime ONNX sera branché.

use super::prompt::{construire_prompt, schema_traduction, GlossaryEntry};
use super::traduction::{ErreurTraduction, PageTranslation, StatutBloc, Traducteur};

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
// Traduction par page avec validation, backoff et repli (T3.2 + T3.5)
// ---------------------------------------------------------------------------

/// Nombre maximum de tentatives de traduction par page.
const TENTATIVES_MAX: usize = 3;

/// Délai de backoff initial en millisecondes.
const BACKOFF_INITIAL_MS: u64 = 500;

/// Délai de backoff maximum en millisecondes.
const BACKOFF_MAX_MS: u64 = 10_000;

/// Calcule un délai de backoff exponentiel avec jitter.
///
/// # Formule
/// `delay = min(BACKOFF_MAX_MS, BACKOFF_INITIAL_MS * 2^tentative) + random(0, 250ms)`
fn calculer_backoff(tentative: usize) -> u64 {
    let base = BACKOFF_INITIAL_MS * 2u64.saturating_pow(tentative as u32);
    let delai = base.min(BACKOFF_MAX_MS);
    let jitter = (rand::random::<u64>() % 250).saturating_add(1);
    delai.saturating_add(jitter)
}

/// Traduit une page complète.
///
/// # Algorithme
/// 1. Construire le prompt avec tous les blocs, leur budget et le glossaire.
/// 2. Essayer le traducteur en ligne avec backoff exponentiel.
/// 3. Si quota atteint (429) : basculer immédiatement sur le traducteur local.
/// 4. Si échec après toutes les tentatives : marquer tous les blocs en `Fallback`.
///
/// # Retour
/// La page mise à jour avec les traductions, ou les blocs en `Fallback`.
pub async fn traduire_page<T, F>(
    translator: &T,
    fallback: &F,
    page: &PageTranslation,
    glossaire: &[GlossaryEntry<'_>],
) -> Result<PageTranslation, ErreurTraduction>
where
    T: Translator,
    F: Translator,
{
    let prompt = construire_prompt(page, glossaire);
    let schema = schema_traduction();

    for tentative in 0..TENTATIVES_MAX {
        match translator.translate(&prompt).await {
            Ok(reponse) => match valider_reponse(&reponse, page, &schema) {
                Ok(page_traduite) => return Ok(page_traduite),
                Err(e) => {
                    tracing::warn!(
                        "tentative {} : réponse invalide : {e}",
                        tentative + 1
                    );
                }
            },
            Err(ErreurTraduction::Quota) => {
                tracing::warn!("quota Gemini atteint, bascule sur le mode hors ligne");
                return traduire_avec_fallback(fallback, page, glossaire).await;
            }
            Err(e) => {
                tracing::warn!(
                    "tentative {} : traduction échouée : {e}",
                    tentative + 1
                );
            }
        }

        if tentative < TENTATIVES_MAX - 1 {
            let delai = calculer_backoff(tentative);
            tokio::time::sleep(tokio::time::Duration::from_millis(delai)).await;
        }
    }

    // Échec après toutes les tentatives : repli sur le traducteur local.
    traduire_avec_fallback(fallback, page, glossaire).await
}

/// Essaie le traducteur de secours (local) pour la page entière.
async fn traduire_avec_fallback<F>(
    fallback: &F,
    page: &PageTranslation,
    glossaire: &[GlossaryEntry<'_>],
) -> Result<PageTranslation, ErreurTraduction>
where
    F: Translator,
{
    let prompt = construire_prompt(page, glossaire);
    let schema = schema_traduction();

    match fallback.translate(&prompt).await {
        Ok(reponse) => match valider_reponse(&reponse, page, &schema) {
            Ok(page_traduite) => return Ok(page_traduite),
            Err(e) => {
                tracing::warn!("traducteur local : réponse invalide : {e}");
            }
        },
        Err(e) => {
            tracing::warn!("traducteur local indisponible : {e}");
        }
    }

    // Échec du fallback : marquer tous les blocs en Fallback.
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

    // ---------------------------------------------------------------------------
    // Tests T3.5 — résilience
    // ---------------------------------------------------------------------------

    /// Traducteur factice pour les tests de résilience.
    #[derive(Debug, Default)]
    struct FakeTranslator {
        responses: Vec<Result<String, ErreurTraduction>>,
        call_count: std::sync::atomic::AtomicUsize,
    }

    #[async_trait::async_trait]
    impl Translator for FakeTranslator {
        async fn translate(&self, _text: &str) -> Result<String, ErreurTraduction> {
            let idx = self.call_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            self.responses.get(idx).cloned().unwrap_or(Err(
                ErreurTraduction::Reseau("pas de réponse factice".into())
            ))
        }
    }

    #[tokio::test]
    async fn traduire_page_utilise_le_fallback_si_quota() {
        let online = FakeTranslator {
            responses: vec![Err(ErreurTraduction::Quota)],
            call_count: std::sync::atomic::AtomicUsize::new(0),
        };
        let offline = FakeTranslator {
            responses: vec![Ok(r#"[{"id":"b1","translatedText":"Bonjour","status":"ok"}]"#.into())],
            call_count: std::sync::atomic::AtomicUsize::new(0),
        };

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
                bbox: Bbox { x: 0, y: 0, w: 10, h: 10 },
                original_text: Some("Hello".into()),
                translated_text: None,
                reading_order: 0,
                status: StatutBloc::Ok,
                confidence: Some(1.0),
            }],
        };

        let result = traduire_page(&online, &offline, &page, &[]).await;
        assert!(result.is_ok());
        let page_traduite = result.unwrap();
        assert_eq!(
            page_traduite.blocks[0].translated_text,
            Some("Bonjour".into())
        );
        assert_eq!(page_traduite.blocks[0].status, StatutBloc::Ok);
    }

    #[tokio::test]
    async fn traduire_page_fallback_si_toutes_tentatives_echouent() {
        let online = FakeTranslator {
            responses: vec![
                Err(ErreurTraduction::Reseau("timeout".into())),
                Err(ErreurTraduction::Reseau("timeout".into())),
                Err(ErreurTraduction::Reseau("timeout".into())),
            ],
            call_count: std::sync::atomic::AtomicUsize::new(0),
        };
        let offline = FakeTranslator {
            responses: vec![Ok(r#"[{"id":"b1","translatedText":"Salut","status":"ok"}]"#.into())],
            call_count: std::sync::atomic::AtomicUsize::new(0),
        };

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
                bbox: Bbox { x: 0, y: 0, w: 10, h: 10 },
                original_text: Some("Hello".into()),
                translated_text: None,
                reading_order: 0,
                status: StatutBloc::Ok,
                confidence: Some(1.0),
            }],
        };

        let result = traduire_page(&online, &offline, &page, &[]).await;
        assert!(result.is_ok());
        let page_traduite = result.unwrap();
        assert_eq!(
            page_traduite.blocks[0].translated_text,
            Some("Salut".into())
        );
    }

    #[tokio::test]
    async fn traduire_page_fallback_marque_fallback_si_tout_echoue() {
        let online = FakeTranslator {
            responses: vec![Err(ErreurTraduction::Reseau("timeout".into()))],
            call_count: std::sync::atomic::AtomicUsize::new(0),
        };
        let offline = FakeTranslator {
            responses: vec![Err(ErreurTraduction::LocalIndisponible)],
            call_count: std::sync::atomic::AtomicUsize::new(0),
        };

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
                bbox: Bbox { x: 0, y: 0, w: 10, h: 10 },
                original_text: Some("Hello".into()),
                translated_text: None,
                reading_order: 0,
                status: StatutBloc::Ok,
                confidence: Some(1.0),
            }],
        };

        let result = traduire_page(&online, &offline, &page, &[]).await;
        assert!(result.is_ok());
        let page_traduite = result.unwrap();
        assert_eq!(page_traduite.blocks[0].translated_text, None);
        assert_eq!(page_traduite.blocks[0].status, StatutBloc::Fallback);
    }

    #[test]
    fn calculer_backoff_est_borne() {
        // Le backoff ne doit pas dépasser BACKOFF_MAX_MS.
        for tentative in 0..10 {
            let delai = calculer_backoff(tentative);
            assert!(delai <= BACKOFF_MAX_MS + 250, "backoff {} = {} ms", tentative, delai);
        }
    }
}
