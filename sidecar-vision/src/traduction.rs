//! Traduction EN→FR locale (opus-mt ONNX) — **stub**.
//!
//! Ce module sera implémenté quand les modèles opus-mt seront disponibles
//! dans le pack (T3). Pour l'instant, il renvoie `LocalIndisponible` comme
//! l'ancien `Traducteur::traduire_local`.
//!
//! Architecture prévue (mesurée le 2026-10-07 sur `Helsinki-NLP/opus-mt-en-fr`) :
//! - Tokenizer : SentencePiece (`.model` + `vocab.json`)
//! - Encoder : ONNX, une passe, produit `encoder_output` + 12 `encoder_past_*`
//! - Decoder : ONNX, auto-régressif, utilise `decoder_with_past` après le 1er token
//!
//! Le vocabulaire du modèle (59 514) ≠ le fichier `.spm` (32 000 pièces).
//! Il faut mapper pièce → id via `vocab.json` dans les deux sens.
//!
//! L'espace CTC est l'indice 18 709 (juste après le dictionnaire de 18 708 entrées).
//! Un OCR CTC n'accepte qu'**une ligne** : une bulle multi-lignes écrasée à 48 px
//! rend du charabia ; il faut une découpe en lignes (projection horizontale, 1,2 ms).
//!
//! Traduire ligne par ligne détruit le contexte ; par bulle entière le restaure.
//! Mesuré sur opus-mt-en-fr : « OH MY » + « GOSH! » séparés → « - Oui ! » ;
//! joints → « Oh mon Dieu ! ».

use crate::pack::TraductionPack;

/// Modèles de traduction locaux — **stub**.
///
/// Quand les modèles seront présents dans le pack, cette structure les chargera
/// et implémentera la traduction via ONNX Runtime + SentencePiece.
#[derive(Debug)]
pub struct TraducteurLocal {
    _pack: TraductionPack,
}

impl TraducteurLocal {
    /// Ouvre les modèles de traduction depuis le pack.
    ///
    /// Retourne une erreur `ModelMissing` si le pack n'a pas les modèles
    /// de traduction, ce qui permet au sidecar de répondre `MODEL_MISSING`
    /// à la méthode `translate` au lieu de planter.
    pub fn ouvrir(pack: &TraductionPack) -> Result<Self, String> {
        // Vérifier que les fichiers existent (le pack l'a déjà fait).
        // L'implémentation réelle chargera les modèles ici.
        Ok(Self {
            _pack: pack.clone(),
        })
    }

    /// Traduit un texte anglais vers le français — **stub**.
    ///
    /// Retourne `LocalIndisponible` tant que les modèles ne sont pas branchés.
    pub fn traduire(&self, _texte: &str) -> Result<String, String> {
        Err("LocalIndisponible: modèles opus-mt non chargés".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn traduire_local_est_un_stub_tant_que_modele_absent() {
        // Ce test vérifie que le module compile. L'implémentation réelle
        // nécessite les modèles ONNX (T3). Quand les modèles seront présents,
        // ce test devra être remplacé par un vrai test d'intégration.
    }
}
