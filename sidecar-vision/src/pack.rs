//! Pack de modèles : ce que le sidecar charge, et ce qui manque vraiment.
//!
//! Les poids ne sont **jamais** dans le dépôt. Ils arrivent par packs
//! versionnés, téléchargés et vérifiés par l'application (`pack.rs` côté
//! application, T1.4). Le sidecar ne fait que **constater** : il ouvre un
//! dossier, refuse de démarrer si un modèle annoncé manque, et le dit en
//! nommant le fichier.
//!
//! ## Ce qui est requis, et pourquoi
//!
//! - `manifest.json` : l'identité du pack (`packId`, `version`). Sans elle, deux
//!   packs ne sont pas distinguables et le cache de l'application ne peut pas
//!   savoir ce qu'il a exécuté.
//! - `detector.onnx` : sans détection, il n'y a pas de bulle.
//! - `ocr/lignes.onnx` : un CTC ne lit qu'une ligne, donc sans détecteur de
//!   lignes le texte revient en fragments.
//! - `ocr/rec.onnx` + `ocr/rec.txt` : la reconnaissance et son dictionnaire.
//!
//! `inpaint/lama.onnx` est **optionnel** : son absence rend `inpaint`
//! indisponible, et c'est l'état attendu tant que l'effacement n'est pas
//! intégré (décision du 2026-10-07).

use std::path::{Path, PathBuf};

use crate::proto::Code;

/// Un pack ouvert.
#[derive(Debug, Clone)]
pub struct Pack {
    pub racine: PathBuf,
    pub id: String,
    pub version: String,
    /// RT-DETR-V2, détection des bulles.
    pub detecteur: PathBuf,
    /// Détecteur de lignes à l'intérieur d'une bulle.
    pub detecteur_lignes: PathBuf,
    /// Reconnaissance d'une ligne.
    pub reconnaissance: PathBuf,
    /// Dictionnaire de la reconnaissance.
    pub dictionnaire: PathBuf,
    /// Effacement, absent tant que T2.5 n'est pas fait.
    pub effacement: Option<PathBuf>,
    /// Traduction EN→FR (opus-mt), optionnelle (T3).
    pub traduction: Option<TraductionPack>,
}

/// Modèles de traduction EN→FR.
#[derive(Debug, Clone)]
pub struct TraductionPack {
    pub encoder: PathBuf,
    pub decoder: PathBuf,
    pub decoder_with_past: PathBuf,
    pub vocab_json: PathBuf,
    pub tokenizer_model: PathBuf,
}

impl Pack {
    /// Ce que le sidecar sait faire avec ce pack. Le client Rust s'en sert pour
    /// afficher « effacement indisponible » au lieu d'échouer plus tard.
    pub fn capacites(&self) -> Vec<&'static str> {
        let mut c = vec!["detect", "ocr"];
        if self.effacement.is_some() {
            c.push("inpaint");
        }
        if self.traduction.is_some() {
            c.push("translate");
        }
        c
    }

    /// Ouvre un pack et vérifie que tout ce qui est requis est présent.
    pub fn ouvrir(racine: &Path) -> Result<Pack, (Code, String)> {
        let manifeste = racine.join("manifest.json");
        if !manifeste.is_file() {
            return Err((
                Code::ModelMissing,
                format!("manifeste absent : {}", manifeste.display()),
            ));
        }
        let texte = std::fs::read_to_string(&manifeste).map_err(|e| {
            (
                Code::Internal,
                format!("manifeste illisible {} : {e}", manifeste.display()),
            )
        })?;
        let v: serde_json::Value = serde_json::from_str(&texte).map_err(|e| {
            (
                Code::ParseError,
                format!("manifeste illisible {} : {e}", manifeste.display()),
            )
        })?;
        let id = v
            .get("packId")
            .and_then(|x| x.as_str())
            .ok_or_else(|| {
                (
                    Code::ParseError,
                    format!("{} : champ `packId` absent", manifeste.display()),
                )
            })?
            .to_string();
        let version = v
            .get("version")
            .and_then(|x| x.as_str())
            .ok_or_else(|| {
                (
                    Code::ParseError,
                    format!("{} : champ `version` absent", manifeste.display()),
                )
            })?
            .to_string();

        let requi = [
            ("detecteur.onnx", racine.join("detecteur.onnx")),
            ("ocr/lignes.onnx", racine.join("ocr").join("lignes.onnx")),
            ("ocr/rec.onnx", racine.join("ocr").join("rec.onnx")),
            ("ocr/rec.txt", racine.join("ocr").join("rec.txt")),
        ];
        for (nom, chemin) in requi {
            if !chemin.is_file() {
                return Err((
                    Code::ModelMissing,
                    format!("modèle absent du pack {id} : {nom} ({})", chemin.display()),
                ));
            }
        }

        let effacement = {
            let chemin = racine.join("inpaint").join("lama.onnx");
            if chemin.is_file() {
                Some(chemin)
            } else {
                None
            }
        };

        let traduction = {
            let trad_dir = racine.join("traduction");
            let encoder = trad_dir.join("encoder.onnx");
            let decoder = trad_dir.join("decoder.onnx");
            let decoder_with_past = trad_dir.join("decoder_with_past.onnx");
            let vocab_json = trad_dir.join("vocab.json");
            let tokenizer_model = trad_dir.join("tokenizer.model");

            if encoder.is_file()
                && decoder.is_file()
                && decoder_with_past.is_file()
                && vocab_json.is_file()
                && tokenizer_model.is_file()
            {
                Some(TraductionPack {
                    encoder,
                    decoder,
                    decoder_with_past,
                    vocab_json,
                    tokenizer_model,
                })
            } else {
                None
            }
        };

        Ok(Pack {
            racine: racine.to_path_buf(),
            id,
            version,
            detecteur: racine.join("detecteur.onnx"),
            detecteur_lignes: racine.join("ocr").join("lignes.onnx"),
            reconnaissance: racine.join("ocr").join("rec.onnx"),
            dictionnaire: racine.join("ocr").join("rec.txt"),
            effacement,
            traduction,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Dossier temporaire unique : les tests sont parallèles, deux d'entre eux
    /// ne doivent pas partager le même nom.
    fn dossier(nom: &str) -> PathBuf {
        let base = std::env::temp_dir().join(format!("tatami-pack-{}-{}", nom, std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(&base).expect("dossier temporaire");
        base
    }

    fn ecrire(racine: &Path, chemin: &str, contenu: &str) {
        let p = racine.join(chemin);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).expect("dossiers du pack");
        }
        std::fs::write(p, contenu).expect("écriture");
    }

    fn pack_complet(racine: &Path) {
        ecrire(
            racine,
            "manifest.json",
            r#"{"packId":"vision-fr","version":"0.1.0"}"#,
        );
        ecrire(racine, "detecteur.onnx", "x");
        ecrire(racine, "ocr/lignes.onnx", "x");
        ecrire(racine, "ocr/rec.onnx", "x");
        ecrire(racine, "ocr/rec.txt", "x");
    }

    #[test]
    fn un_pack_complet_s_ouvre_et_annonce_ses_capacites() {
        let d = dossier("complet");
        pack_complet(&d);
        let p = Pack::ouvrir(&d).expect("pack complet");
        assert_eq!(p.id, "vision-fr");
        assert_eq!(p.version, "0.1.0");
        assert_eq!(p.capacites(), vec!["detect", "ocr"], "pas d'effacement");
        assert!(p.effacement.is_none());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn l_effacement_apparait_des_que_le_modele_est_la() {
        let d = dossier("lama");
        pack_complet(&d);
        ecrire(&d, "inpaint/lama.onnx", "x");
        let p = Pack::ouvrir(&d).expect("pack complet");
        assert_eq!(p.capacites(), vec!["detect", "ocr", "inpaint"]);
        assert!(p.effacement.is_some());
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn un_modele_manquant_est_nomme() {
        let d = dossier("manquant");
        pack_complet(&d);
        std::fs::remove_file(d.join("ocr").join("rec.txt")).expect("suppression");
        let (code, message) = Pack::ouvrir(&d).expect_err("pack incomplet");
        assert_eq!(code, Code::ModelMissing);
        assert!(
            message.contains("ocr/rec.txt"),
            "le message nomme le fichier absent : {message}"
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn un_manifeste_absent_est_refuse() {
        let d = dossier("sans-manifeste");
        pack_complet(&d);
        std::fs::remove_file(d.join("manifest.json")).expect("suppression");
        let (code, _) = Pack::ouvrir(&d).expect_err("sans manifeste");
        assert_eq!(code, Code::ModelMissing);
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn un_manifeste_sans_pack_id_est_refuse() {
        let d = dossier("id-manquant");
        pack_complet(&d);
        ecrire(&d, "manifest.json", r#"{"version":"0.1.0"}"#);
        let (code, message) = Pack::ouvrir(&d).expect_err("packId absent");
        assert_eq!(code, Code::ParseError);
        assert!(message.contains("packId"), "{message}");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn un_dossier_inexistant_est_refuse_sans_paniquer() {
        let (code, _) =
            Pack::ouvrir(Path::new("C:/tatami-pack-inexistant-xyz")).expect_err("absent");
        assert_eq!(code, Code::ModelMissing);
    }
}
