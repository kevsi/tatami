//! Trait de traduction et implémentations (T3.1).
//!
//! Ce module définit l'abstraction `Translator` utilisée par le reste
//! de l'application, puis fournit l'implémentation Gemini en ligne
//! (`Traducteur`). Le modèle local (ADR 0008) sera ajouté en T3.1
//! quand le runtime ONNX sera branché.

use super::traduction::{ErreurTraduction, Traducteur};

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
// Erreurs
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

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
}
