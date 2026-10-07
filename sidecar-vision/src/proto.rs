//! Protocole du sidecar vision : NDJSON sur stdin/stdout.
//!
//! **stdout est réservé au protocole**, tous les journaux partent sur stderr
//! (ADR 0001). Une ligne JSON par requête, une ligne JSON par réponse, dans
//! l'ordre d'arrivée. Les identifiants sont recopiés tels quels : c'est Rust qui
//! apparie, donc le sidecar n'a pas besoin d'être séquentiel.

use serde::{Deserialize, Serialize};

/// Version du protocole, tenue dans le même fichier que le reste du contrat.
pub const VERSION_PROTOCOLE: &str = "0.1";

/// Version de l'inférence vision elle-même, distincte du protocole.
pub const VERSION_MOTEUR: &str = env!("CARGO_PKG_VERSION");

/// Codes d'erreur du contrat, dans son ordre.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Code {
    ParseError,
    Timeout,
    Cancelled,
    Internal,
    ModelMissing,
    OutOfMemory,
    TileTooLarge,
}

/// Une requête reçue sur stdin.
#[derive(Debug, Clone, Deserialize)]
pub struct Requete {
    pub id: String,
    pub method: String,
    #[serde(default)]
    pub params: serde_json::Value,
}

/// Le corps d'une réponse réussie.
#[derive(Debug, Clone, Serialize)]
pub struct ReponseOk {
    pub id: String,
    pub ok: bool,
    pub result: serde_json::Value,
}

/// Le corps d'une réponse en erreur.
#[derive(Debug, Clone, Serialize)]
pub struct ReponseErr {
    pub id: String,
    pub ok: bool,
    pub error: DetailErreur,
}

/// Un code et un message lisibles, comme l'exige le contrat.
#[derive(Debug, Clone, Serialize)]
pub struct DetailErreur {
    pub code: Code,
    pub message: String,
}

impl ReponseOk {
    pub fn nouvelle(id: &str, result: serde_json::Value) -> ReponseOk {
        ReponseOk {
            id: id.to_string(),
            ok: true,
            result,
        }
    }
}

impl ReponseErr {
    pub fn nouvelle(id: &str, code: Code, message: impl Into<String>) -> ReponseErr {
        ReponseErr {
            id: id.to_string(),
            ok: false,
            error: DetailErreur {
                code,
                message: message.into(),
            },
        }
    }
}

/// Une réponse, telle qu'elle part sur stdout.
#[derive(Debug, Clone, Serialize)]
#[serde(untagged)]
pub enum Reponse {
    Ok(ReponseOk),
    Err(ReponseErr),
}

impl Reponse {
    pub fn succes(id: &str, result: serde_json::Value) -> Reponse {
        Reponse::Ok(ReponseOk::nouvelle(id, result))
    }

    pub fn echec(id: &str, code: Code, message: impl Into<String>) -> Reponse {
        Reponse::Err(ReponseErr::nouvelle(id, code, message))
    }

    /// L'identifiant que la requête portait.
    pub fn id(&self) -> &str {
        match self {
            Reponse::Ok(r) => &r.id,
            Reponse::Err(r) => &r.id,
        }
    }
}

impl std::fmt::Display for Code {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let nom = match self {
            Code::ParseError => "PARSE_ERROR",
            Code::Timeout => "TIMEOUT",
            Code::Cancelled => "CANCELLED",
            Code::Internal => "INTERNAL",
            Code::ModelMissing => "MODEL_MISSING",
            Code::OutOfMemory => "OUT_OF_MEMORY",
            Code::TileTooLarge => "TILE_TOO_LARGE",
        };
        write!(f, "{nom}")
    }
}

/// Image transportée en base64, comme le contrat le fait pour `fetchImage`.
#[derive(Debug, Clone, Deserialize)]
pub struct ImageParams {
    pub data: String,
    /// Ignoré à dessein : le type se déduit des **octets**. Le contrat le dit
    /// pour les couvertures, et c'est la même règle ici : un en-tête annoncé ne
    /// prouve rien.
    #[serde(default)]
    #[allow(dead_code)]
    pub content_type: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn une_requete_se_parse_avec_ses_params() {
        let r: Requete = serde_json::from_str(
            r#"{"id":"7","method":"detect","params":{"image":{"data":"AAA"}}}"#,
        )
        .expect("requete valide");
        assert_eq!(r.id, "7");
        assert_eq!(r.method, "detect");
        assert!(r.params.get("image").is_some());
    }

    #[test]
    fn des_params_absents_ne_cassent_pas_la_requete() {
        let r: Requete =
            serde_json::from_str(r#"{"id":"1","method":"health"}"#).expect("params optionnels");
        assert!(r.params.is_null() || r.params.as_object().is_some_and(|o| o.is_empty()));
    }

    #[test]
    fn une_reponse_reussie_ne_contient_pas_de_champ_erreur() {
        let json = serde_json::to_value(Reponse::succes("3", serde_json::json!({"boxes": []})))
            .expect("serialisable");
        assert_eq!(json["id"], "3");
        assert_eq!(json["ok"], true);
        assert!(json.get("error").is_none(), "pas de champ error quand ok");
        assert!(json["result"].is_object());
    }

    #[test]
    fn une_reponse_en_erreur_porte_le_code_du_contrat() {
        let json = serde_json::to_value(Reponse::echec("4", Code::ModelMissing, "poids absent"))
            .expect("serialisable");
        assert_eq!(json["ok"], false);
        assert_eq!(json["error"]["code"], "MODEL_MISSING");
        assert_eq!(json["error"]["message"], "poids absent");
        assert!(
            json.get("result").is_none(),
            "pas de champ result quand on échoue"
        );
    }

    #[test]
    fn tous_les_codes_sont_ceux_du_contrat() {
        // Les huit codes que le sidecar peut produire. Un code hors contrat
        // passerait inaperçu jusqu'au client Rust, qui le refuserait.
        let attendus = [
            "PARSE_ERROR",
            "TIMEOUT",
            "CANCELLED",
            "INTERNAL",
            "MODEL_MISSING",
            "OUT_OF_MEMORY",
            "TILE_TOO_LARGE",
        ];
        for nom in attendus {
            let json = serde_json::to_value(Reponse::echec("x", Code::Internal, "m")).unwrap();
            assert!(json["error"]["code"].is_string());
            let _ = nom;
        }
        // Le contrôle utile : chaque code se sérialise exactement comme le
        // contrat l'écrit.
        let paires = [
            (Code::ParseError, "PARSE_ERROR"),
            (Code::Timeout, "TIMEOUT"),
            (Code::Cancelled, "CANCELLED"),
            (Code::Internal, "INTERNAL"),
            (Code::ModelMissing, "MODEL_MISSING"),
            (Code::OutOfMemory, "OUT_OF_MEMORY"),
            (Code::TileTooLarge, "TILE_TOO_LARGE"),
        ];
        for (code, nom) in paires {
            assert_eq!(
                serde_json::to_value(Reponse::echec("x", code, "m")).unwrap()["error"]["code"],
                nom
            );
            assert_eq!(code.to_string(), nom);
        }
    }

    #[test]
    fn l_identifiant_est_recopie_tel_quel() {
        let reponses = vec![
            Reponse::succes("abc", serde_json::json!({})),
            Reponse::echec("abc", Code::Internal, "m"),
        ];
        for r in &reponses {
            assert_eq!(r.id(), "abc");
        }
    }
}
