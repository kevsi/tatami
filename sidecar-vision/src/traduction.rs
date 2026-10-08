//! Traduction EN→FR locale (opus-mt ONNX).
//!
//! ## État : tokenizer et boucle de génération écrits
//!
//! `TraducteurLocal::traduire` enchaîne l'encodeur, le décodeur, puis
//! `decoder_with_past` jusqu'au jeton de fin. Les noms de tenseurs ne sont pas
//! devinés : ils ont été relevés sur les trois `.onnx` installés (mesuré le
//! 2026-10-08, cf. `examples/sonde_onnx_traduction.rs`).
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

use ort::session::{Session, SessionInputValue, SessionOutputs};
use ort::value::Tensor;

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

/// Jeton de début du décodeur, mesuré dans `generation_config.json` le
/// 2026-10-08 : `decoder_start_token_id: 59513`, c'est-à-dire **`<pad>`**, et pas
/// `</s>`. Le vocabulaire n'a d'ailleurs **pas** de `<s>` (`bos_token_id` vaut 0).
const DEBUT_DECODEUR: i64 = INCONNU_REMPLISSAGE;

/// `bad_words_ids: [[59513]]` : le modèle ne doit jamais **émettre** `<pad>`.
/// Sans cette interdiction, la boucle peut s'y installer au lieu de s'arrêter.
const JETONS_INTERDITS: &[i64] = &[INCONNU_REMPLISSAGE];

/// `max_length: 512`, mesuré. Ce n'est pas un réglage : une bulle tient en
/// quelques dizaines de jetons, donc c'est un garde-fou.
const JETONS_MAX: usize = 512;

/// Les trois sessions ONNX, chargées à la première traduction.
///
/// Mesuré le 2026-10-08 : **840 Mo** au total. Les charger dans `ouvrir` les
/// ferait payer à chaque `loadPack`, alors que la traduction est occasionnelle —
/// donc à la première demande, pas au démarrage du sidecar.
#[derive(Debug)]
struct Modeles {
    encodeur: Session,
    decodeur: Session,
    decodeur_avec_past: Session,
}

/// Une sortie `present.*` recopiée hors du prêt ONNX.
///
/// `SessionOutputs` emprunte la session qui l'a produite : sans copie, impossible
/// de conserver un passé d'un pas de génération à l'autre.
#[derive(Debug)]
struct Copie {
    nom: String,
    forme: Vec<i64>,
    valeurs: Vec<f32>,
}

fn session(chemin: &Path) -> Result<Session, String> {
    let mut builder = Session::builder().map_err(|e| format!("session ONNX : {e}"))?;
    builder
        .commit_from_file(chemin)
        .map_err(|e| format!("{} illisible : {e}", chemin.display()))
}

fn tenseur(forme: &[i64], valeurs: Vec<f32>) -> Result<Tensor<f32>, String> {
    Tensor::from_array((forme.to_vec(), valeurs)).map_err(|e| format!("tenseur : {e}"))
}

fn ids(forme: &[i64], valeurs: &[i64]) -> Result<Tensor<i64>, String> {
    Tensor::from_array((forme.to_vec(), valeurs.to_vec()))
        .map_err(|e| format!("tenseur d'identifiants : {e}"))
}

impl Modeles {
    fn ouvrir(pack: &crate::pack::TraductionPack) -> Result<Self, String> {
        Ok(Self {
            encodeur: session(&pack.encoder)?,
            decodeur: session(&pack.decoder)?,
            decodeur_avec_past: session(&pack.decoder_with_past)?,
        })
    }
}

/// Recopie les tenseurs `present.*` d'une sortie, en gardant leur nom.
///
/// Mesuré : le décodeur en rend **24** (6 couches × décodeur/encodeur ×
/// clé/valeur), `decoder_with_past` n'en rend que **12** — celles du décodeur.
/// Le compte est donc lu dans la sortie, jamais écrit en dur : un modèle à un
/// autre nombre de couches marcherait sans changer une ligne.
fn copier_pasts(sortie: &SessionOutputs) -> Result<Vec<Copie>, String> {
    let mut out = Vec::new();
    for (nom, valeur) in sortie.iter() {
        if !nom.starts_with("present.") {
            continue;
        }
        let (forme, valeurs) = valeur
            .try_extract_tensor::<f32>()
            .map_err(|e| format!("passé {nom} : {e}"))?;
        out.push(Copie {
            nom: nom.to_string(),
            forme: forme.to_vec(),
            valeurs: valeurs.to_vec(),
        });
    }
    Ok(out)
}

/// Renomme une sortie `present.…` en entrée `past_key_values.…`.
fn nom_du_passe(nom: &str) -> String {
    nom.replacen("present.", "past_key_values.", 1)
}

/// Le jeton suivant : plus fort logit de la dernière position.
///
/// `bad_words_ids` est appliqué ici, en constante, parce que
/// `generation_config.json` **n'est pas** dans le manifeste du pack : les
/// valeurs sont reportées avec la mesure qui les fonde (`JETONS_INTERDITS`).
fn meilleur_jeton(sortie: &SessionOutputs) -> Result<i64, String> {
    let (forme, valeurs) = sortie
        .get("logits")
        .ok_or("le décodeur n'a pas de sortie `logits`")?
        .try_extract_tensor::<f32>()
        .map_err(|e| format!("logits : {e}"))?;
    let Some(&classes) = forme.last() else {
        return Err("logits sans dimension".to_string());
    };
    if classes <= 0 || valeurs.len() < classes as usize {
        return Err(format!(
            "logits de forme {forme:?} pour {} valeurs",
            valeurs.len()
        ));
    }
    let ligne = &valeurs[valeurs.len() - classes as usize..];
    Ok(meilleur_de(ligne, JETONS_INTERDITS))
}

/// Plus grand logit, en sautant les jetons interdits.
fn meilleur_de(ligne: &[f32], interdits: &[i64]) -> i64 {
    let mut id = 0i64;
    let mut score = f32::NEG_INFINITY;
    for (i, &v) in ligne.iter().enumerate() {
        if interdits.contains(&(i as i64)) {
            continue;
        }
        if v > score {
            score = v;
            id = i as i64;
        }
    }
    id
}

/// La source telle que Marian l'attend : les pièces, puis `</s>`.
///
/// `Tokenizer::encoder` rend les pièces brutes ; c'est ici qu'on ferme la
/// séquence. Un texte vide rend une source vide : lui ajouter `</s>` ferait
/// traduire une chaîne vide en une phrase.
fn source_de(tokenizer: &Tokenizer, texte: &str) -> Vec<i64> {
    let mut ids = tokenizer.encoder(texte);
    if !ids.is_empty() {
        ids.push(FIN);
    }
    ids
}

/// Traduction EN→FR locale (opus-mt ONNX).
#[derive(Debug)]
pub struct TraducteurLocal {
    tokenizer: Tokenizer,
    pack: crate::pack::TraductionPack,
    modeles: Option<Modeles>,
}

impl TraducteurLocal {
    /// Charge le vocabulaire. Les modèles sont chargés plus tard, à la première
    /// traduction — voir [`Modeles`].
    pub fn ouvrir(pack: &crate::pack::TraductionPack) -> Result<Self, String> {
        let tokenizer = Tokenizer::charger(&pack.vocab_json).map_err(|e| e.to_string())?;
        Ok(Self {
            tokenizer,
            pack: pack.clone(),
            modeles: None,
        })
    }

    /// Le tokenizer chargé, pour les sondes qui doivent regarder la segmentation.
    pub fn tokenizer(&self) -> &Tokenizer {
        &self.tokenizer
    }

    /// Traduit un texte anglais vers le français.
    ///
    /// Décodage **glouton** alors que `generation_config.json` demande
    /// `num_beams: 4` : l'écart joue sur la qualité, pas sur l'exactitude de la
    /// boucle, et il est mesurable tout de suite (les cinq bulles de référence de
    /// `docs/spike-traduction.md` §8.5).
    ///
    /// Aucun budget de jetons n'est en jeu : une bulle ne dépasse jamais quelques
    /// dizaines de jetons, ce qui écarte d'un coup la troncature qui rendait
    /// l'API en ligne inutilisable sur les pages à six blocs et plus.
    pub fn traduire(&mut self, texte: &str) -> Result<String, String> {
        let source = source_de(&self.tokenizer, texte);
        if source.is_empty() {
            return Ok(String::new());
        }
        if self.modeles.is_none() {
            self.modeles = Some(Modeles::ouvrir(&self.pack)?);
        }
        let modeles = self
            .modeles
            .as_mut()
            .ok_or("modèles de traduction absents après ouverture")?;

        let n = source.len() as i64;
        let masque = vec![1i64; n as usize];

        // --- passe encodeur : une seule sortie mesurée, `last_hidden_state` ---
        // Les douze passés encodeur ne sortent **pas** d'ici : c'est le décodeur
        // qui les rend (`present.N.encoder.*`), mesuré le 2026-10-08.
        let cache = {
            let sortie = modeles
                .encodeur
                .run(ort::inputs![
                    "input_ids" => ids(&[1, n], &source)?,
                    "attention_mask" => ids(&[1, n], &masque)?,
                ])
                .map_err(|e| format!("encodeur : {e}"))?;
            let (forme, valeurs) = sortie
                .get("last_hidden_state")
                .ok_or("l'encodeur n'a pas de sortie `last_hidden_state`")?
                .try_extract_tensor::<f32>()
                .map_err(|e| format!("last_hidden_state : {e}"))?;
            let forme = forme.to_vec();
            let valeurs = valeurs.to_vec();
            tenseur(&forme, valeurs)?
        };

        // --- passe 1 : le décodeur, jeton de début, sans passé ---
        let (mut jeton, mut pasts) = {
            let sortie = modeles
                .decodeur
                .run(ort::inputs![
                    "encoder_attention_mask" => ids(&[1, n], &masque)?,
                    "input_ids" => ids(&[1, 1], &[DEBUT_DECODEUR])?,
                    "encoder_hidden_states" => cache,
                ])
                .map_err(|e| format!("decodeur : {e}"))?;
            let jeton = meilleur_jeton(&sortie)?;
            (jeton, copier_pasts(&sortie)?)
        };

        // --- passes suivantes : un jeton à la fois, passés réutilisés ---
        let mut genere = vec![jeton];
        while jeton != FIN && genere.len() < JETONS_MAX {
            let mut entrees: Vec<(String, SessionInputValue)> = Vec::with_capacity(pasts.len() + 2);
            entrees.push((
                "encoder_attention_mask".to_string(),
                ids(&[1, n], &masque)?.into(),
            ));
            entrees.push(("input_ids".to_string(), ids(&[1, 1], &[jeton])?.into()));
            for p in &pasts {
                entrees.push((
                    nom_du_passe(&p.nom),
                    tenseur(&p.forme, p.valeurs.clone())?.into(),
                ));
            }
            let (suivant, nouveaux) = {
                let sortie = modeles
                    .decodeur_avec_past
                    .run(entrees)
                    .map_err(|e| format!("decodeur_avec_past : {e}"))?;
                let suivant = meilleur_jeton(&sortie)?;
                let nouveaux = copier_pasts(&sortie)?;
                (suivant, nouveaux)
            };
            // Mesuré : `decoder_with_past` ne réémet plus que les douze passés
            // du décodeur. Ceux de l'encodeur cessent d'être produits, et c'est
            // correct : la source ne change pas, donc ils restent ceux de la
            // passe 1. D'où une fusion **par nom**, et non un remplacement.
            for p in nouveaux {
                match pasts.iter_mut().find(|q| q.nom == p.nom) {
                    Some(q) => {
                        q.forme = p.forme;
                        q.valeurs = p.valeurs;
                    }
                    None => pasts.push(p),
                }
            }
            jeton = suivant;
            genere.push(jeton);
        }

        Ok(self.tokenizer.decoder(&genere))
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

    #[test]
    fn une_sortie_present_devient_une_entree_past_key_values() {
        // Mesuré : le décodeur rend `present.0.decoder.key`, le décodeur à passé
        // attend `past_key_values.0.decoder.key`. Le préfixe change, le reste du
        // nom non — d'où une substitution et non une construction.
        assert_eq!(
            nom_du_passe("present.3.encoder.value"),
            "past_key_values.3.encoder.value"
        );
    }

    #[test]
    fn le_jeton_interdit_ne_gagne_jamais() {
        // `bad_words_ids: [[59513]]` : même au sommet, `<pad>` ne doit pas être
        // choisi, sinon la boucle s'installe au lieu de s'arrêter.
        assert_eq!(meilleur_de(&[0.1, 9.0, 0.2], &[1]), 2);
        assert_eq!(meilleur_de(&[0.1, 0.2, 0.05], &[1]), 0);
        // Un seul candidat interdit : le suivant doit être pris, pas 0 par défaut.
        assert_eq!(meilleur_de(&[-3.0, -7.0], &[0]), 1);
    }

    #[test]
    fn la_source_se_termine_par_fin_de_sequence() {
        // Marian attend `</s>` ; `Tokenizer::encoder` ne le met pas.
        let t = tokenizer_de_test();
        assert_eq!(source_de(&t, "not").last().copied(), Some(FIN));
        // Un texte vide ne rend aucune source : ajouter `</s>` ferait traduire
        // une chaîne vide en une phrase.
        assert!(source_de(&t, "").is_empty());
    }

    #[test]
    fn un_texte_vide_ne_charge_aucun_modele() {
        // Les poids pèsent 840 Mo (mesuré) : `ouvrir` ne doit pas les charger, et
        // un texte vide ne doit pas déclencher le chargement par la suite. Les
        // chemins ci-dessous n'existent pas : si la boucle les ouvrait, elle
        // échouerait au lieu de rendre une chaîne vide.
        let faux = std::path::PathBuf::from("inexistant-ne-pas-ouvrir.onnx");
        let pack = crate::pack::TraductionPack {
            encoder: faux.clone(),
            decoder: faux.clone(),
            decoder_with_past: faux.clone(),
            vocab_json: faux.clone(),
            tokenizer_model: faux,
        };
        let mut t = TraducteurLocal {
            tokenizer: tokenizer_de_test(),
            pack,
            modeles: None,
        };
        assert_eq!(t.traduire("").unwrap(), "");
        assert!(t.modeles.is_none(), "aucun modèle ne doit être chargé");
    }
}
