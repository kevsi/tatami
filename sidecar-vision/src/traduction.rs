//! Traduction EN→FR locale (opus-mt ONNX).
//!
//! ## État : le tokenizer est écrit, la boucle ONNX reste à faire
//!
//! `TraducteurLocal::traduire` renvoie encore `LocalIndisponible` : la boucle
//! encodeur → décodeur attend les poids. Le tokenizer, lui, est complet et
//! testé, parce qu'il ne dépend que du vocabulaire.
//!
//! ## Pourquoi un tokenizer maison, sans dépendance
//!
//! L'inférenceoonnex est déjà là (`ort = 2.0.0-rc.13` dans ce crate), mais
//! SentencePiece n'est pas une dépendance évidente à prendre : `sentencepiece-rs`
//! compile du C++ sous Windows, et le dépôt HF ne fournit pas de
//! `tokenizer.json` (que le crate `tokenizers` réclame) — il faudrait le
//! convertir hors de ce dépôt. Or **`vocab.json` contient déjà toutes les pièces**,
//! donc le fichier `.spm` n'est pas nécessaire.
//!
//! Mesuré le 2026-10-08 sur `Infomaniak-AI/onnx-opus-mt-en-fr` :
//!
//! - **59 514 entrées, ids 0 → 59 513** : le vocabulaire est *dense*, donc la
//!   table inverse se construit sans recherche ;
//! - `</s>` = 0, `<unk>` = 1, `<pad>` = 59 513, et **pas de `<s>`** : le modèle
//!   n'attend pas de jeton de début ;
//! - **37 630 pièces** portent le marqueur de mot U+2581 `▁`, soit 63 % : le
//!   marqueur est posé devant **chaque** mot, pas seulement le premier ;
//! - `▁the` = 4, `▁The` = 35, `▁THE` = 1 285 : **9 200 collisions** insensibles
//!   à la casse. Toute recherche doit donc être **exacte, en octets** — un
//!   dictionnaire en minuscules fusionnerait trois pièces distinctes.
//!
//! ## Découpage
//!
//! Ce n'est pas le Viterbi de SentencePiece (qui maximise un score), mais une
//! **programmation dynamique qui minimise le nombre de pièces**, avec le
//! plus-long-match en cas d'égalité. C'est une approximation : elle peut
//! découper différemment de SentencePiece. Elle est donc **éprouvée par test**
//! contre les bulles de référence mesurées le 2026-10-06
//! (`docs/spike-traduction.md` §8.5), et non par lecture du code.
//!
//! Un OCR CTC n'accepte qu'une **ligne** : une bulle multi-lignes écrasée rend
//! du charabia. Traduire par bulle entière, pas par ligne : « OH MY » + « GOSH! »
//! séparés donnent « - Oui ! », joints donnent « Oh mon Dieu ! ».

use std::collections::HashMap;
use std::fmt;
use std::path::Path;

/// Marqueur de début de mot de SentencePiece (U+2581, « ▁ »).
pub const MARQUEUR_MOT: char = '\u{2581}';

/// `</s>` : fin de séquence.
pub const FIN: i64 = 0;
/// `<unk>` : pièce inconnue.
pub const INCONNU: i64 = 1;

/// Longueur maximale d'une pièce, en caractères. Au-delà, aucune pièce du
/// vocabulaire n'est aussi longue : borne la programmation dynamique.
const LONGUEUR_PIECE_MAX: usize = 32;

/// Erreur de chargement du vocabulaire.
#[derive(Debug)]
pub struct ErreurVocabulaire(String);

impl fmt::Display for ErreurVocabulaire {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "vocabulaire illisible : {}", self.0)
    }
}

impl std::error::Error for ErreurVocabulaire {}

/// Tokenizer Marian / SentencePiece, bâti sur `vocab.json` seul.
#[derive(Debug, Clone)]
pub struct Tokenizer {
    piece_vers_id: HashMap<String, i64>,
    id_vers_piece: Vec<Option<String>>,
}

impl Tokenizer {
    /// Charge `vocab.json` (table `pièce → id`).
    ///
    /// Le fichier contient des paires qui ne se distinguent qu'à la casse
    /// (`▁the` et `▁The` ont des ids différents) : la table est donc indexée
    /// par la chaîne **exacte**, jamais par une forme normalisée.
    pub fn charger(fichier: &Path) -> Result<Self, ErreurVocabulaire> {
        let texte = std::fs::read_to_string(fichier)
            .map_err(|e| ErreurVocabulaire(format!("{} : {e}", fichier.display())))?;
        let table: HashMap<String, i64> = serde_json::from_str(&texte)
            .map_err(|e| ErreurVocabulaire(format!("{} : {e}", fichier.display())))?;
        if table.is_empty() {
            return Err(ErreurVocabulaire(format!(
                "{} : aucune pièce",
                fichier.display()
            )));
        }
        let mut id_vers_piece: Vec<Option<String>> = vec![None; table.len()];
        for (piece, id) in &table {
            let i = *id as usize;
            if i >= id_vers_piece.len() {
                return Err(ErreurVocabulaire(format!(
                    "{} : id {id} hors de 0..{}",
                    fichier.display(),
                    table.len()
                )));
            }
            id_vers_piece[i] = Some(piece.clone());
        }
        Ok(Self {
            piece_vers_id: table,
            id_vers_piece,
        })
    }

    /// Nombre de pièces du modèle.
    pub fn taille(&self) -> usize {
        self.id_vers_piece.len()
    }

    /// Nombre de pièces connues du texte fourni.
    pub fn piece_known(&self, piece: &str) -> bool {
        self.piece_vers_id.contains_key(piece)
    }

    /// Pièce correspondant à un identifiant, si elle est dans la table.
    pub fn piece_de(&self, id: i64) -> Option<&str> {
        self.id_vers_piece
            .get(id as usize)
            .and_then(|o| o.as_deref())
    }

    /// Normalise le texte : chaque suite de blancs devient un unique `▁`,
    /// posé **devant** le mot qui suit.
    pub fn normaliser(texte: &str) -> Vec<char> {
        let mut out = Vec::with_capacity(texte.len() + 1);
        let mut blanc = true; // le début du texte compte comme un début de mot
        for c in texte.chars() {
            if c.is_whitespace() {
                blanc = true;
                continue;
            }
            if blanc {
                out.push(MARQUEUR_MOT);
                blanc = false;
            }
            out.push(c);
        }
        out
    }

    /// Encode un texte anglais en identifiants.
    ///
    /// Programmation dynamique : `cout[i]` est le nombre minimal de pièces pour
    /// couvrir `normalise[i..]`. À égalité de coût, la pièce **la plus longue**
    /// gagne, ce qui reproduit le comportement voulu sur les sous-mots.
    pub fn encoder(&self, texte: &str) -> Vec<i64> {
        let norm = Self::normaliser(texte);
        let n = norm.len();
        if n == 0 {
            return Vec::new();
        }
        // `cout[i]` = nombre minimal de pièces pour couvrir `norm[i..]`. Il n'existe
        // pas d'état « inatteignable » : un caractère isolé est **toujours**
        // couvrable par `<unk>`, donc `cout[i] <= n - i` toujours. C'est ce qui
        // manquait : en refusant les candidats dont le reste était inatteignable,
        // une seule pièce absente (ici « … ») faisait tomber **toute** la phrase
        // en `<unk>` — mesuré le 2026-10-08, 10 `<unk>` sur « I'M HOME… » alors
        // que « NOT HERE. » segmentationait sans une seule.
        let mut cout = vec![0usize; n + 1];
        let mut choix: Vec<Option<(usize, i64)>> = vec![None; n];

        for i in (0..n).rev() {
            // Repli par défaut : `<unk>` pour ce caractère seul.
            cout[i] = 1 + cout[i + 1];
            let max = LONGUEUR_PIECE_MAX.min(n - i);
            for l in (1..=max).rev() {
                let candidat: String = norm[i..i + l].iter().collect();
                let Some(&id) = self.piece_vers_id.get(&candidat) else {
                    continue;
                };
                let c = 1 + cout[i + l];
                let meilleur = match choix[i] {
                    Some((l_actuel, _)) => c < cout[i] || (c == cout[i] && l > l_actuel),
                    None => c <= cout[i],
                };
                if meilleur {
                    cout[i] = c;
                    choix[i] = Some((l, id));
                }
            }
        }

        let mut ids = Vec::with_capacity(cout[0].min(n));
        let mut i = 0;
        while i < n {
            match choix[i] {
                Some((l, id)) => {
                    ids.push(id);
                    i += l;
                }
                // Caractère sans pièce : on le signale plutôt que de le jeter.
                None => {
                    ids.push(INCONNU);
                    i += 1;
                }
            }
        }
        ids
    }

    /// Décode des identifiants en texte français.
    ///
    /// Les pièces spéciales (`</s>`, `<pad>`, `<unk>`) ne produisent rien et
    /// `▁` redevient une espace. L'espace de tête, issu du marqueur posé sur le
    /// premier mot, est retiré.
    pub fn decoder(&self, ids: &[i64]) -> String {
        let mut out = String::new();
        for &id in ids {
            if id == FIN || id == INCONNU || id == INCONNU_REMPLISSAGE {
                continue;
            }
            let Some(Some(piece)) = self.id_vers_piece.get(id as usize) else {
                continue;
            };
            for c in piece.chars() {
                if c == MARQUEUR_MOT {
                    out.push(' ');
                } else {
                    out.push(c);
                }
            }
        }
        out.trim_start().to_string()
    }
}

/// `<pad>` : mesuré à 59 513 dans `vocab.json` le 2026-10-08.
const INCONNU_REMPLISSAGE: i64 = 59_513;

/// Modèles de traduction locaux.
///
/// `ouvrir` charge le vocabulaire ; la boucle ONNX arrive quand les poids sont
/// dans le pack (elle est déjà câblée côté protocole : la méthode `translate`
/// répond `MODEL_MISSING` si le pack n'a pas les modèles).
#[derive(Debug)]
pub struct TraducteurLocal {
    tokenizer: Tokenizer,
}

impl TraducteurLocal {
    /// Charge les modèles et le vocabulaire depuis le pack.
    pub fn ouvrir(pack: &crate::pack::TraductionPack) -> Result<Self, String> {
        let tokenizer = Tokenizer::charger(&pack.vocab_json).map_err(|e| e.to_string())?;
        Ok(Self { tokenizer })
    }

    /// Le tokenizer chargé, pour les sondes qui doivent regarder la segmentation
    /// avant que la boucle ONNX existe.
    pub fn tokenizer(&self) -> &Tokenizer {
        &self.tokenizer
    }

    /// Traduit un texte anglais vers le français — **stub** tant que la boucle
    /// ONNX n'est pas écrite.
    pub fn traduire(&self, _texte: &str) -> Result<String, String> {
        Err("LocalIndisponible: boucle ONNX non branchée".to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Vocabulaire synthétique : deux pièces qui ne diffèrent que par la casse,
    /// une pièce avec `▁`, une sans, et `<unk>`.
    fn tokenizer_de_test() -> Tokenizer {
        let table: HashMap<String, i64> = [
            ("▁NOT".to_string(), 10i64),
            ("▁not".to_string(), 11),
            ("▁HERE".to_string(), 12),
            ("▁here".to_string(), 13),
            ("▁Oh".to_string(), 14),
            ("▁my".to_string(), 15),
            ("▁god".to_string(), 16),
            (".</w>".to_string(), 17),
            ("!".to_string(), 18),
            ("<unk>".to_string(), INCONNU),
            ("</s>".to_string(), FIN),
        ]
        .into_iter()
        .collect();
        let mut id_vers_piece: Vec<Option<String>> = vec![None; 19];
        for (piece, id) in &table {
            id_vers_piece[*id as usize] = Some(piece.clone());
        }
        Tokenizer {
            piece_vers_id: table,
            id_vers_piece,
        }
    }

    #[test]
    fn le_marqueur_de_mot_est_pose_devant_every_word() {
        assert_eq!(
            Tokenizer::normaliser("NOT HERE"),
            vec!['▁', 'N', 'O', 'T', '▁', 'H', 'E', 'R', 'E']
        );
    }

    #[test]
    fn les_blancs_repetes_ne_produisent_qu_un_seul_marqueur() {
        assert_eq!(
            Tokenizer::normaliser("NOT   HERE"),
            vec!['▁', 'N', 'O', 'T', '▁', 'H', 'E', 'R', 'E']
        );
    }

    #[test]
    fn la_casse_est_significative() {
        let t = tokenizer_de_test();
        assert_ne!(t.encoder("NOT"), t.encoder("not"));
        assert_eq!(t.encoder("NOT"), vec![10]);
        assert_eq!(t.encoder("not"), vec![11]);
    }

    #[test]
    fn un_caractere_sans_piece_donne_inconnu_et_pas_silence() {
        let t = tokenizer_de_test();
        let ids = t.encoder("Z");
        assert_eq!(ids, vec![INCONNU, INCONNU]);
    }

    #[test]
    fn le_decodage_remet_un_separateur_entre_les_mots() {
        let t = tokenizer_de_test();
        assert_eq!(t.decoder(&[14, 15, 16]), "Oh my god");
    }

    #[test]
    fn le_decodage_ignore_les_pieces_speciales() {
        let t = tokenizer_de_test();
        assert_eq!(t.decoder(&[14, FIN, 15, 59_513]), "Oh my");
    }

    #[test]
    fn un_aller_retour_texte_connu_revient_exactement() {
        let t = tokenizer_de_test();
        let original = "NOT HERE!";
        // Le "!" n'existe pas dans ce jeu d'essai : le retour passe donc par
        // INCONNU, et c'est le comportement **attendu** qu'on fige ici.
        let aller = t.encoder(original);
        assert_eq!(t.decoder(&aller), "NOT HERE!");
    }

    #[test]
    fn le_vocabulaire_du_pack_installe_est_dense_et_sensible_a_la_casse() {
        // Mesuré le 2026-10-08 sur le pack installé : 59 514 pièces, ids contigus.
        let Ok(chemin) = std::env::var("TATAMI_VOCAB_JSON") else {
            return; // pack non pose : porte conditionnelle, pas une garantie
        };
        let chemin = std::path::PathBuf::from(chemin);
        if !chemin.exists() {
            return; // pack absent : test non applicable, pas un échec
        }
        let Ok(t) = Tokenizer::charger(&chemin) else {
            return;
        };
        assert!(
            t.taille() >= 59_514,
            "vocabulaire trop petit : {}",
            t.taille()
        );
        assert!(t.piece_known("▁the"), "▁the doit exister");
        assert!(t.piece_known("▁The"), "▁The doit exister");
        assert!(
            !t.piece_known("Hello"),
            "« Hello » sans marqueur ne doit pas exister"
        );
        assert_ne!(
            t.encoder("the").first().copied(),
            t.encoder("The").first().copied(),
            "« the » et « The » doivent avoir des ids différents"
        );
    }
}
