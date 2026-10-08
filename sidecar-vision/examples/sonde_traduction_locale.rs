//! Sonde du tokenizer local sur le **vocabulaire réellement installé**.
//!
//! Elle existe parce qu'un test ne peut pas dépendre du pack de la machine :
//! ici la sonde **échoue bruyamment** si le pack est absent, ce qu'un test
//! conditionnel ne peut pas faire.
//!
//! Ce qu'elle mesure, avant même que la boucle ONNX existe :
//! - la forme du vocabulaire (taille, pièces spéciales, marqueur de mot) ;
//! - le nombre de `<unk>` produits sur cinq bulles de référence. Un encoder
//!   faux en produit beaucoup : c'est le signal qui tranche, bien avant de
//!   disposer d'une traduction.
//!
//! Les cinq bulles et leur français attendu viennent de `docs/spike-traduction.md`
//! §8.5, mesure du 2026-10-06 : c'est la référence à reproduire, pas une
//! invention.
//!
//! Usage : `cargo run --example sonde_traduction_locale`

use std::path::PathBuf;

use tatami_vision::pack::Pack;
use tatami_vision::traduction::{Tokenizer, TraducteurLocal};

/// Bulles de référence et traduction attendue, mesurées le 2026-10-06.
const BULLES: &[(&str, &str)] = &[
    ("OH MY GOSH!", "Oh mon Dieu !"),
    ("I'M HOME…", "Je suis chez moi…"),
    ("NOT HERE.", "Pas ici."),
    (
        "…BUT HE'S NOT THAT BAD OF A KID IN MY EYES.",
        "Mais il n'est pas si mal qu'un gamin dans mes yeux.",
    ),
    (
        "HOW COULD YOU TEACH YOUR KID LIKE THIS…",
        "Comment pourrais-tu apprendre à tes enfants quand tu es un prof ?",
    ),
];

fn racine_du_pack() -> PathBuf {
    if let Ok(p) = std::env::var("TATAMI_PACK") {
        return PathBuf::from(p);
    }
    let appdata = std::env::var("APPDATA").unwrap_or_default();
    PathBuf::from(appdata)
        .join("Tatami")
        .join("packs")
        .join("vision-fr-0.1.0")
}

fn main() {
    let racine = racine_du_pack();
    println!("pack : {}", racine.display());
    let pack = match Pack::ouvrir(&racine) {
        Ok(p) => p,
        Err((code, message)) => {
            eprintln!("ECHEC : pack illisible ({code:?}) : {message}");
            std::process::exit(1);
        }
    };
    println!("capacites : {:?}", pack.capacites());

    let Some(traduction) = pack.traduction.as_ref() else {
        eprintln!(
            "ECHEC : le pack n'a pas de dossier traduction/ (vocabulaire absent).\n\
             Attendu : encoder.onnx, decoder.onnx, decoder_with_past.onnx,\n\
             vocab.json, tokenizer.model."
        );
        std::process::exit(1);
    };

    // On passe par `TraducteurLocal::ouvrir` : c'est le chemin qu'emprunttera
    // l'application, donc la sonde mesure ce qui sera réellement utilisé.
    let traducteur = match TraducteurLocal::ouvrir(traduction) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("ECHEC : {e}");
            std::process::exit(1);
        }
    };
    let tokenizer = traducteur.tokenizer();
    println!("vocabulaire : {} pieces", tokenizer.taille());
    for piece in [
        "</s>", "<unk>", "<pad>", "▁", "▁the", "▁The", "▁THE", "▁Hello", "Hello",
    ] {
        let ids = tokenizer.encoder(piece);
        let etat = if tokenizer.piece_known(piece) {
            "connue"
        } else {
            "ABSENTE"
        };
        println!("  {piece:>9} : {etat}  ids={ids:?}");
    }

    println!();
    let entete = format!(
        "{:>3}  {:>5}  {:>5}  {:>8}  {}",
        "#", "ids", "unk", "unk_minus", "aller-retour"
    );
    println!("{entete}");
    let mut total_unk = 0usize;
    for (i, (anglais, _attendu)) in BULLES.iter().enumerate() {
        let ids = tokenizer.encoder(anglais);
        let unk = ids
            .iter()
            .filter(|&&id| id == tatami_vision::traduction::INCONNU)
            .count();
        total_unk += unk;
        let retour = tokenizer.decoder(&ids);
        // Mesure de l'hypothèse : le vocabulaire est dominé par les minuscules,
        // donc segmenter le texte en minuscules doit faire tomber les <unk>.
        let ids_minus = tokenizer.encoder(&anglais.to_lowercase());
        let unk_minus = ids_minus
            .iter()
            .filter(|&&id| id == tatami_vision::traduction::INCONNU)
            .count();
        // Détail des pièces choisies : c'est ce qui tranche entre « le mot
        // manque dans le vocabulaire » et « mon encodeur le cherche mal ».
        if unk > 0 && std::env::var("TATAMI_DEBUG_UNK").is_ok() {
            println!("      detail :");
            let norm = Tokenizer::normaliser(anglais);
            let vus: Vec<String> = norm
                .iter()
                .map(|c| format!("{c:?}U+{:04X}", *c as u32))
                .collect();
            println!(
                "        normalise ({} car.) : {}",
                norm.len(),
                vus.join(" ")
            );
            for &id in &ids {
                let nom = tokenizer.piece_de(id).unwrap_or("<hors-table>");
                let detail = format!("        {:>6}  {}", id, nom);
                println!("{detail}");
            }
        }
        let ligne = format!(
            "{:>3}  {:>5}  {:>5}  {:>8}  {}",
            i,
            ids.len(),
            unk,
            unk_minus,
            retour
        );
        println!("{ligne}");
    }
    println!();
    println!("total <unk> sur {} bulles : {}", BULLES.len(), total_unk);
    if total_unk > 0 {
        println!(
            "ATTENTION : des caractères n'ont pas été segmentés. Soit l'encoder\n\
             est faux, soit le texte contient autre chose que de l'anglais."
        );
    }
    for (anglais, attendu) in BULLES {
        println!("  attendu : {anglais:?} -> {attendu:?}");
    }
}
