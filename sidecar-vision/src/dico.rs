//! Dictionnaire de caractères du reconnaissance PP-OCR, et décodage CTC.
//!
//! ## Pourquoi ce module existe
//!
//! Le modèle `PP-OCRv6_small_rec` a **18 710 classes**, mais le fichier
//! `PP-OCRv6_small_rec.txt` qui accompagne les poids ne fournit que **18 708
//! caractères**. L'indice 0 étant réservé au blanc CTC, il reste un indice
//! **18709 que rien ne mappe** — et c'est **l'espace**.
//!
//! Mesuré le 2026-10-07 sur du texte rendu dont la réponse est connue :
//!
//! ```text
//! attendu : 'YOU GOT TO GET CLOTHES'
//! ids     : [67, 57, 63, 18709, 49, 57, 62, 18709, 62, 57, 18709, ...]
//! ```
//!
//! Le décodage de l'évaluation (`if i != prev and i > 0 and i < len(DICT)`)
//! **supprimait chaque 18709**, et le texte arrivait `YOUGOTTOGETCLOSHESFORDOGS`.
//! Son contrôle ne pouvait pas le voir : la comparaison se faisait après avoir
//! retiré les espaces des deux côtés. Un test qui nettoie ce qu'il veut mesurer
//! ne peut plus crier.
//!
//! ## Règle de remplissage
//!
//! On ne complète **que** le cas mesuré : le dictionnaire compte exactement une
//! classe de moins que le modèle, et cette classe est l'espace. Tout autre
//! écart est une erreur signalée, pas un écart comblé : un dictionnaire faux
//! produirait des caractères faux en silence, ce qui est pire que l'échec.

use std::fmt;

/// Indice réservé au blanc CTC dans tous les dictionnaires PP-OCR.
pub const BLANC: i64 = 0;

/// Le caractère que les dictionnaires PP-OCR laissent non écrit en fin de
/// fichier, et que le modèle émet pourtant entre les mots.
pub const ESPACE_MANQUANT: &str = " ";

/// Erreur de chargement ou de concordance du dictionnaire.
#[derive(Debug, PartialEq, Eq)]
pub enum ErreurDictionnaire {
    /// Le fichier ne contient aucun caractère.
    Vide,
    /// Le dictionnaire et le modèle ne concordent pas, et l'écart n'est pas
    /// celui que l'on sait expliquer (une classe manquante, l'espace).
    Discordance { caracteres: usize, classes: usize },
}

impl fmt::Display for ErreurDictionnaire {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ErreurDictionnaire::Vide => {
                write!(f, "le dictionnaire ne contient aucun caractère")
            }
            ErreurDictionnaire::Discordance {
                caracteres,
                classes,
            } => write!(
                f,
                "le dictionnaire fournit {caracteres} caractères pour {classes} classes : \
                 écart de {} ; seul l'écart de 1 (l'espace) est connu",
                *classes as i64 - *caracteres as i64
            ),
        }
    }
}

impl std::error::Error for ErreurDictionnaire {}

/// Dictionnaire de caractères indexé par la classe du modèle.
///
/// `classes[0]` est le blanc CTC ; `classes[i]` est le caractère de la classe
/// `i`. La longueur est **toujours** égale au nombre de classes du modèle.
#[derive(Debug, Clone)]
pub struct Dictionnaire {
    classes: Vec<String>,
}

impl Dictionnaire {
    /// Construit le dictionnaire à partir du texte du `.txt` et du nombre de
    /// classes annoncé par le modèle.
    ///
    /// Le `.txt` de PP-OCR liste un caractère par ligne et se termine par un
    /// saut de ligne ; sa dernière ligne utile est un emoji. L'espace que le
    /// modèle attend entre les mots n'y est pas.
    pub fn depuis_txt(txt: &str, classes: usize) -> Result<Dictionnaire, ErreurDictionnaire> {
        let caracteres: Vec<String> = txt
            .lines()
            .filter(|l| !l.is_empty())
            .map(str::to_string)
            .collect();
        if caracteres.is_empty() {
            return Err(ErreurDictionnaire::Vide);
        }
        // +1 : l'indice 0 est le blanc CTC, absent du fichier.
        let ecart = classes as i64 - (caracteres.len() as i64 + 1);
        match ecart {
            0 => {
                let mut v = Vec::with_capacity(classes);
                v.push(String::new());
                v.extend(caracteres);
                Ok(Dictionnaire { classes: v })
            }
            1 => {
                // Le cas mesuré : le modèle attend une classe de plus, et c'est
                // l'espace. On le signale, pour qu'un fichier mis à jour ne
                // fasse pas croire qu'on a deviné.
                tracing::warn!(
                    classes,
                    caracteres = caracteres.len(),
                    "dictionnaire complété de l'espace manquant"
                );
                let mut v = Vec::with_capacity(classes);
                v.push(String::new());
                v.extend(caracteres);
                v.push(ESPACE_MANQUANT.to_string());
                Ok(Dictionnaire { classes: v })
            }
            _ => Err(ErreurDictionnaire::Discordance {
                caracteres: caracteres.len(),
                classes,
            }),
        }
    }

    /// Le caractère d'une classe, ou `None` si la classe est hors dictionnaire.
    pub fn caractere(&self, classe: i64) -> Option<&str> {
        if classe < 0 {
            return None;
        }
        self.classes.get(classe as usize).map(String::as_str)
    }

    /// Nombre de classes couvertes.
    pub fn nb_classes(&self) -> usize {
        self.classes.len()
    }

    /// Décode une suite d'indices en texte : CTC, c'est-à-dire sans répétition
    /// consécutive et sans blanc.
    ///
    /// Les indices hors dictionnaire sont **ignorés** comme le blanc : une
    /// classe que l'on ne sait pas nommer ne doit pas produire un caractère
    /// arbitraire au milieu d'une bulle.
    pub fn decoder(&self, ids: &[i64]) -> String {
        let mut texte = String::new();
        let mut precedent = -1i64;
        for &id in ids {
            if id != BLANC && id != precedent {
                if let Some(c) = self.caractere(id) {
                    texte.push_str(c);
                }
            }
            precedent = id;
        }
        // Les dictionnaires PP-OCR écrivent l'espace comme U+3000 (espace
        // idéographique) à l'indice 1748 : sans normalisation, le texte arrive
        // au traducteur avec des espaces CJK. Mesuré le 2026-10-07.
        texte.replace('\u{3000}', ESPACE_MANQUANT)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Les identifiants relevés le 2026-10-07 sur du texte rendu, dont la
    /// réponse est connue. Test écrit depuis la **mesure**, pas depuis
    /// l'explication du code.
    const IDS_MESURES: &[i64] = &[
        67, 57, 63, 18709, 49, 57, 62, 18709, 62, 57, 18709, 49, 47, 62, 18709, 45, 54, 57, 62, 50,
        47, 61,
    ];

    /// Reproduit la forme du fichier réel : **un caractère par ligne**,
    /// 18 708 caractères, l'espace de la classe 1748 écrit en U+3000, et les
    /// lettres des classes relevées le 2026-10-07.
    ///
    /// La classe `i` correspond au caractère `i - 1` du fichier, l'indice 0
    /// étant le blanc CTC.
    fn txt_reel() -> String {
        // (classe, caractère) relevés sur 'YOU GOT TO GET CLOTHES'
        let mesure: &[(i64, char)] = &[
            (67, 'Y'),
            (57, 'O'),
            (63, 'U'),
            (49, 'G'),
            (62, 'T'),
            (47, 'E'),
            (45, 'C'),
            (54, 'L'),
            (50, 'H'),
            (61, 'S'),
        ];
        let mut txt = String::new();
        for i in 0..18708usize {
            let classe = i as i64 + 1;
            let c = if classe == 1748 {
                // L'entrée 1748 du fichier réel est une espace idéographique.
                '\u{3000}'
            } else if let Some((_, c)) = mesure.iter().find(|(k, _)| *k == classe) {
                *c
            } else {
                'x'
            };
            txt.push(c);
            txt.push('\n');
        }
        txt
    }

    fn dictionnaire_de_test() -> Dictionnaire {
        Dictionnaire::depuis_txt(&txt_reel(), 18710).expect("concordance attendue")
    }

    #[test]
    fn le_dictionnaire_comble_exactement_la_classe_manquante() {
        let d = dictionnaire_de_test();
        assert_eq!(d.nb_classes(), 18710, "une classe par classe du modèle");
        assert_eq!(d.caractere(BLANC), Some(""), "l'indice 0 est le blanc");
        assert_eq!(
            d.caractere(18709),
            Some(ESPACE_MANQUANT),
            "la classe 18709 est l'espace, celle que le fichier ne nomme pas"
        );
    }

    #[test]
    fn les_identifiants_releves_donnent_le_texte_attendu() {
        assert_eq!(
            dictionnaire_de_test().decoder(IDS_MESURES),
            "YOU GOT TO GET CLOTHES"
        );
    }

    #[test]
    fn un_dictionnaire_trop_grand_pour_le_modele_est_refuse() {
        // 3 caractères + le blanc = 4 classes, pour un modèle qui n'en a que 3.
        let err = Dictionnaire::depuis_txt("a\nb\nc\n", 3).expect_err("concordance absente");
        assert_eq!(
            err,
            ErreurDictionnaire::Discordance {
                caracteres: 3,
                classes: 3
            }
        );
    }

    #[test]
    fn un_dictionnaire_vide_est_refuse() {
        assert_eq!(
            Dictionnaire::depuis_txt("\n\n", 18710).expect_err("vide"),
            ErreurDictionnaire::Vide
        );
    }

    #[test]
    fn les_repetitions_et_le_blanc_sont_ignores() {
        let d = dictionnaire_de_test();
        // CTC : deux fois la même classe ne donne qu'un caractère, le blanc
        // disparaît, et un identifiant hors dictionnaire ne casse pas la bulle.
        // 67 = 'Y', 57 = 'O' (classes relevées).
        assert_eq!(d.decoder(&[67, 67, BLANC, 67, 99999, 57]), "YYO");
    }

    #[test]
    fn l_espace_ideographique_est_normalise() {
        let d = dictionnaire_de_test();
        // L'indice 1748 du fichier réel vaut U+3000 : il doit ressortir en
        // espace ASCII, sinon le traducteur reçoit une espace idéographique.
        assert_eq!(d.decoder(&[1, 1748, 2]), "x x");
    }
}
