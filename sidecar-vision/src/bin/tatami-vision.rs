//! Sidecar vision : binaire NDJSON sur stdin/stdout.
//!
//!(stdout est réservé au protocole, ADR 0001. Les journaux partent sur stderr,
//! ce qui permet de lire le journal de l'application sans mixture.)
//!
//! ## Méthodes
//!
//! - `hello` : version du protocole et du moteur, comme le moteur de sources.
//! - `capabilities` : ce que le pack chargé permet réellement de faire.
//! - `health` : vivant, point.
//! - `loadPack` : ouvre un dossier de pack et charge les modèles.
//! - `detect` : bulles et texte libre dans une tuile.
//! - `ocr` : texte d'une culture, ligne par ligne, avec les boîtes.
//! - `inpaint` : **indisponible tant que T2.5 n'est pas fait** — répond
//!   `MODEL_MISSING`, ce qui est une réponse honnête et non un silence.
//! - `translate` : traduction EN→FR (opus-mt), indisponible sans pack traduction.
//! - `shutdown` : sort proprement.

use std::io::{BufRead, Write};

use tatami_vision::detect::DetecteurBulles;
use tatami_vision::efface::Effaceur;
use tatami_vision::lignes::DetecteurLignes;
use tatami_vision::lignes::Rectangle;
use tatami_vision::ocr::Lecteur;
use tatami_vision::pack::Pack;
use tatami_vision::proto::{
    Code, ImageParams, Reponse, Requete, VERSION_MOTEUR, VERSION_PROTOCOLE,
};
use tatami_vision::traduction::TraducteurLocal;

/// Côté maximal d'une image acceptée par `detect` : au-delà, le détecteur
/// déforme et rend des boîtes absurdes. On refuse donc en nommant la cause,
/// plutôt que de rendre n'importe quoi.
const COTE_MAXIMAL_TUILE: u32 = 1024;

/// Modèles chargés, tous ouverts en même temps pour éviter un coût par page.
struct Moteurs {
    pack: Pack,
    bulles: DetecteurBulles,
    lignes: DetecteurLignes,
    lecteur: Lecteur,
    /// Effacement : chargé seulement si le pack contient LaMa. Son absence est
    /// un état propre (`inpaint` indisponible), pas une panne.
    effaceur: Option<Effaceur>,
    /// Traduction : chargée seulement si le pack contient opus-mt.
    traducteur: Option<TraducteurLocal>,
}

fn main() {
    let stdin = std::io::stdin();
    let mut stdout = std::io::stdout();
    let mut moteurs: Option<Moteurs> = None;

    for ligne in stdin.lock().lines() {
        let Ok(ligne) = ligne else {
            break; // stdin fermé : fin normale
        };
        if ligne.trim().is_empty() {
            continue;
        }
        let reponse = match serde_json::from_str::<Requete>(&ligne) {
            Ok(requete) => repondre(&requete, &mut moteurs),
            // Une ligne illisible n'a pas d'identifiant : on répond tout de même,
            // sinon Rust attend une réponse qui ne viendra jamais.
            Err(e) => Reponse::echec("?", Code::ParseError, format!("requete illisible : {e}")),
        };
        let texte = match serde_json::to_string(&reponse) {
            Ok(t) => t,
            Err(e) => {
                tracing::error!("reponse non serialisable : {e}");
                continue;
            }
        };
        if writeln!(stdout, "{texte}").is_err() || stdout.flush().is_err() {
            break; // l'appelant a coupe : on sort sans bruit
        }
        if reponse_finale(&ligne) {
            break;
        }
    }
}

/// Une réponse qui annonce la fin du protocole.
fn reponse_finale(ligne: &str) -> bool {
    serde_json::from_str::<Requete>(ligne)
        .map(|r| r.method == "shutdown")
        .unwrap_or(false)
}

fn repondre(requete: &Requete, moteurs: &mut Option<Moteurs>) -> Reponse {
    let id = &requete.id;
    match requete.method.as_str() {
        "hello" => Reponse::succes(
            id,
            serde_json::json!({
                "protocolVersion": VERSION_PROTOCOLE,
                "engineVersion": VERSION_MOTEUR,
                "moteur": "vision",
            }),
        ),
        "capabilities" => {
            let Some(m) = moteurs.as_ref() else {
                return Reponse::echec(
                    id,
                    Code::ModelMissing,
                    "aucun pack chargé : `loadPack` d'abord",
                );
            };
            Reponse::succes(
                id,
                serde_json::json!({
                    "packId": m.pack.id,
                    "version": m.pack.version,
                    "methods": m.pack.capacites(),
                    "inpaintDisponible": m.pack.effacement.is_some(),
                }),
            )
        }
        "health" => Reponse::succes(id, serde_json::json!({ "ok": true })),
        "loadPack" => {
            let Some(chemin) = requete.params.get("path").and_then(|v| v.as_str()) else {
                return Reponse::echec(
                    id,
                    Code::ParseError,
                    "`loadPack` attend un paramètre `path`",
                );
            };
            match Pack::ouvrir(std::path::Path::new(chemin)).and_then(|pack| charger(&pack)) {
                Ok(charge) => {
                    let resultat = serde_json::json!({
                        "packId": charge.pack.id,
                        "version": charge.pack.version,
                    });
                    *moteurs = Some(charge);
                    Reponse::succes(id, resultat)
                }
                Err((code, message)) => Reponse::echec(id, code, message),
            }
        }
        "detect" => {
            let Some(m) = moteurs.as_mut() else {
                return Reponse::echec(
                    id,
                    Code::ModelMissing,
                    "aucun pack chargé : `loadPack` d'abord",
                );
            };
            let Some(image) = lire_image(requete) else {
                return Reponse::echec(
                    id,
                    Code::ParseError,
                    "`detect` attend un paramètre `image` en base64",
                );
            };
            // Le tuilage appartient à Rust (T2.1, `tile.rs`), pas au sidecar.
            // Mesuré : une page entière de 800×1000 déformée en 640×640 rend
            // des boîtes de 769×645, plus grandes que la page. Le sidecar ne
            // se plaint pas de la déformation — il rend ce qu'il a reçu — mais
            // il refuse une image démesurée, parce que c'est le garde-fou qui
            // reste et qu'un `TILE_TOO_LARGE` nomme la cause.
            if image.width() > COTE_MAXIMAL_TUILE || image.height() > COTE_MAXIMAL_TUILE {
                return Reponse::echec(
                    id,
                    Code::TileTooLarge,
                    format!(
                        "image {}x{} au-delà de la tuile maximale {COTE_MAXIMAL_TUILE} : \
                         l'appelant doit découper la page en tuiles (T2.1)",
                        image.width(),
                        image.height()
                    ),
                );
            }
            let seuil = requete
                .params
                .get("scoreMinimal")
                .and_then(|v| v.as_f64())
                .map(|v| v as f32)
                .unwrap_or(tatami_vision::detect::SCORE_MINIMAL);
            match m.bulles.detecter(&image, seuil) {
                Ok(detections) => {
                    // Les classes voyagent **avec** les boîtes. Les perdre ici
                    // alors que le modèle les calcule (T2.3 : `bubble`,
                    // `text_bubble`, `text_free`) ferait_deviner le type de bloc
                    // côté application, alors qu'il est mesuré. Et
                    // `type_bloc` est obligatoire au schéma de traduction.
                    let detections_json: Vec<serde_json::Value> = detections
                        .iter()
                        .map(|d| {
                            serde_json::json!({
                                "bbox": {
                                    "x": d.boite.x.round() as i64,
                                    "y": d.boite.y.round() as i64,
                                    "w": d.boite.largeur.round() as i64,
                                    "h": d.boite.hauteur.round() as i64,
                                },
                                "classe": d.classe.id(),
                                "score": (d.score * 1000.0).round() / 1000.0,
                            })
                        })
                        .collect();
                    Reponse::succes(id, serde_json::json!({ "detections": detections_json }))
                }
                Err(e) => Reponse::echec(id, Code::Internal, e.to_string()),
            }
        }
        "ocr" => {
            let Some(m) = moteurs.as_mut() else {
                return Reponse::echec(
                    id,
                    Code::ModelMissing,
                    "aucun pack chargé : `loadPack` d'abord",
                );
            };
            let Some(mut image) = lire_image(requete) else {
                return Reponse::echec(
                    id,
                    Code::ParseError,
                    "`ocr` attend un paramètre `image` en base64",
                );
            };
            // Une boîte restreint la lecture à une zone de l'image. C'est ainsi
            // que l'application lit une bulle précise sans retailler l'image.
            //
            // Les boîtes rendues sont **dans le repère de l'image reçue**, pas
            // dans celui de la culture. Mesuré le 2026-10-07 : en renvoyant des
            // coordonnées de culture, l'effacement masquait au mauvais endroit —
            // le texte restait intact et des artefacts apparaissaient ailleurs,
            // sans qu'aucune erreur ne soit levée. Un appelant ne doit pas avoir
            // à savoir de quel crop vient quelle boîte pour regrouper des blocs
            // d'une même page.
            let mut decalage = (0i64, 0i64);
            if let Some(boite) = requete.params.get("bbox") {
                decalage = (
                    boite.get("x").and_then(|v| v.as_i64()).unwrap_or(0),
                    boite.get("y").and_then(|v| v.as_i64()).unwrap_or(0),
                );
                match cropper(&image, boite) {
                    Ok(c) => image = c,
                    Err((code, message)) => return Reponse::echec(id, code, message),
                }
            }
            let lignes = match m.lignes.detecter_par_defaut(&image) {
                Ok(l) => l,
                // Une image sans texte n'est pas une erreur : c'est une page
                // d'illustration, très fréquente dans les webtoons.
                Err(tatami_vision::lignes::ErreurLignes::AucuneLigne) => {
                    return Reponse::succes(id, serde_json::json!({ "lines": [] }));
                }
                Err(e) => return Reponse::echec(id, Code::Internal, e.to_string()),
            };
            let mut resultat = Vec::new();
            for b in &lignes {
                let crop = borner(&image, b.x, b.y, b.largeur, b.hauteur, 2.0);
                match m.lecteur.lire(&crop) {
                    // Une ligne sans texte ne perturbe pas la lecture de la page.
                    Ok(texte) if !texte.trim().is_empty() => resultat.push(serde_json::json!({
                        "text": texte,
                        "bbox": {
                            "x": b.x.round() as i64 + decalage.0,
                            "y": b.y.round() as i64 + decalage.1,
                            "w": b.largeur.round() as i64,
                            "h": b.hauteur.round() as i64,
                        },
                    })),
                    Ok(_) => {}
                    Err(e) => tracing::warn!("ligne illisible : {e}"),
                }
            }
            Reponse::succes(id, serde_json::json!({ "lines": resultat }))
        }
        "inpaint" => {
            let Some(m) = moteurs.as_mut() else {
                return Reponse::echec(
                    id,
                    Code::ModelMissing,
                    "aucun pack chargé : `loadPack` d'abord",
                );
            };
            let Some(effaceur) = m.effaceur.as_mut() else {
                return Reponse::echec(
                    id,
                    Code::ModelMissing,
                    "effacement indisponible : le modèle LaMa n'est pas dans le pack \
                     (inpaint/lama.onnx). `capabilities` le dit aussi.",
                );
            };
            let Some(image) = lire_image(requete) else {
                return Reponse::echec(
                    id,
                    Code::ParseError,
                    "`inpaint` attend un paramètre `image` en base64",
                );
            };
            // Les boîtes sont facultatives : sans elles, rien à effacer et
            // l'image revient telle quelle, sans lancer le modèle.
            let boites = lire_boites(requete);
            let debut = std::time::Instant::now();
            match effaceur.effacer(&image, &boites) {
                Ok(sortie) => {
                    let mut buffer = std::io::Cursor::new(Vec::new());
                    // Toujours du PNG : un JPEG repasserait par une compression
                    // qui bavouille précisément les bords de texte qu'on vient
                    // d'effacer.
                    if sortie
                        .write_to(&mut buffer, image::ImageFormat::Png)
                        .is_err()
                    {
                        return Reponse::echec(id, Code::Internal, "image de sortie non encodable");
                    }
                    let octets = buffer.into_inner();
                    Reponse::succes(
                        id,
                        serde_json::json!({
                            "data": base64_encode(&octets),
                            "contentType": "image/png",
                            "tiles": tatami_vision::efface::tuiles_couvrantes(
                                &boites,
                                sortie.width(),
                                sortie.height(),
                                tatami_vision::efface::COTE_TUILE,
                            ).len(),
                            "ms": debut.elapsed().as_millis(),
                        }),
                    )
                }
                Err(e) => Reponse::echec(id, Code::Internal, e.to_string()),
            }
        }
        "translate" => {
            let Some(m) = moteurs.as_mut() else {
                return Reponse::echec(
                    id,
                    Code::ModelMissing,
                    "aucun pack chargé : `loadPack` d'abord",
                );
            };
            let Some(traducteur) = m.traducteur.as_ref() else {
                return Reponse::echec(
                    id,
                    Code::ModelMissing,
                    "traduction indisponible : le modèle opus-mt n'est pas dans le pack \
                     (traduction/). `capabilities` le dit aussi.",
                );
            };
            let Some(texte) = requete.params.get("text").and_then(|v| v.as_str()) else {
                return Reponse::echec(
                    id,
                    Code::ParseError,
                    "`translate` attend un paramètre `text` (chaîne)",
                );
            };
            let debut = std::time::Instant::now();
            match traducteur.traduire(texte) {
                Ok(traduit) => Reponse::succes(
                    id,
                    serde_json::json!({
                        "translatedText": traduit,
                        "ms": debut.elapsed().as_millis(),
                    }),
                ),
                Err(e) => Reponse::echec(id, Code::Internal, e.to_string()),
            }
        }
        "shutdown" => Reponse::succes(id, serde_json::json!({ "ok": true })),
        autre => Reponse::echec(id, Code::ParseError, format!("méthode inconnue : {autre}")),
    }
}

fn charger(pack: &Pack) -> Result<Moteurs, (Code, String)> {
    let bulles = DetecteurBulles::ouvrir(&pack.detecteur)
        .map_err(|e| (Code::ModelMissing, e.to_string()))?;
    let lignes = DetecteurLignes::ouvrir(&pack.detecteur_lignes)
        .map_err(|e| (Code::ModelMissing, e.to_string()))?;
    let dico = std::fs::read_to_string(&pack.dictionnaire).map_err(|e| {
        (
            Code::ModelMissing,
            format!(
                "dictionnaire illisible {} : {e}",
                pack.dictionnaire.display()
            ),
        )
    })?;
    let lecteur = Lecteur::ouvrir(&pack.reconnaissance, &dico)
        .map_err(|e| (Code::ModelMissing, e.to_string()))?;
    // L'effacement est le poste dominant (8 à 14 s par tuile de 512, mesuré) :
    // on ne le charge que si le pack le contient, et son absence laisse
    // `inpaint` indisponible plutôt que de faire échouer le pack entier.
    let effaceur = match &pack.effacement {
        Some(chemin) => match Effaceur::ouvrir(chemin) {
            Ok(e) => Some(e),
            Err(e) => {
                tracing::warn!("effacement présent mais inutilisable : {e}");
                None
            }
        },
        None => None,
    };
    // Traduction : optionnelle, comme l'effacement.
    let traducteur = match &pack.traduction {
        Some(traduction_pack) => match TraducteurLocal::ouvrir(traduction_pack) {
            Ok(t) => Some(t),
            Err(e) => {
                tracing::warn!("traduction présente mais inutilisable : {e}");
                None
            }
        },
        None => None,
    };
    Ok(Moteurs {
        pack: pack.clone(),
        bulles,
        lignes,
        lecteur,
        effaceur,
        traducteur,
    })
}

/// Les boîtes de texte à effacer, lues de `params.boxes`.
fn lire_boites(requete: &Requete) -> Vec<Rectangle> {
    let Some(liste) = requete.params.get("boxes").and_then(|v| v.as_array()) else {
        return Vec::new();
    };
    liste
        .iter()
        .filter_map(|b| {
            let g = |k: &str| b.get(k).and_then(|v| v.as_i64()).unwrap_or(0) as f32;
            if g("w") <= 0.0 || g("h") <= 0.0 {
                return None;
            }
            Some(Rectangle {
                x: g("x"),
                y: g("y"),
                largeur: g("w"),
                hauteur: g("h"),
            })
        })
        .collect()
}

/// Base64 standard, pour renvoyer l'image nettoyée.
fn base64_encode(octets: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(octets.len().div_ceil(3) * 4);
    for groupe in octets.chunks(3) {
        let b = [
            groupe[0],
            *groupe.get(1).unwrap_or(&0),
            *groupe.get(2).unwrap_or(&0),
        ];
        let n = ((b[0] as u32) << 16) | ((b[1] as u32) << 8) | b[2] as u32;
        out.push(ALPHABET[(n >> 18) as usize & 63] as char);
        out.push(ALPHABET[(n >> 12) as usize & 63] as char);
        out.push(if groupe.len() > 1 {
            ALPHABET[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if groupe.len() > 2 {
            ALPHABET[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

/// Décode l'image d'une requête. Le type vient des **octets** : le contrat
/// l'impose déjà pour les couvertures, et un en-tête annoncé ne prouve rien.
fn lire_image(requete: &Requete) -> Option<image::RgbImage> {
    let params: ImageParams = serde_json::from_value(requete.params.get("image")?.clone()).ok()?;
    let octets = base64_decode(&params.data).ok()?;
    image::load_from_memory(&octets).ok().map(|i| i.to_rgb8())
}

/// Recadre la boîte `bbox` de la requête.
fn cropper(
    image: &image::RgbImage,
    boite: &serde_json::Value,
) -> Result<image::RgbImage, (Code, String)> {
    let x = boite
        .get("x")
        .and_then(|v| v.as_i64())
        .ok_or_else(|| (Code::ParseError, "`bbox` sans `x`".to_string()))?;
    let y = boite
        .get("y")
        .and_then(|v| v.as_i64())
        .ok_or_else(|| (Code::ParseError, "`bbox` sans `y`".to_string()))?;
    let w = boite
        .get("w")
        .and_then(|v| v.as_i64())
        .ok_or_else(|| (Code::ParseError, "`bbox` sans `w`".to_string()))?;
    let h = boite
        .get("h")
        .and_then(|v| v.as_i64())
        .ok_or_else(|| (Code::ParseError, "`bbox` sans `h`".to_string()))?;
    if w <= 0 || h <= 0 {
        return Err((Code::TileTooLarge, format!("boîte vide : {w}x{h}")));
    }
    let (iw, ih) = (image.width() as i64, image.height() as i64);
    if x + w > iw || y + h > ih {
        return Err((
            Code::TileTooLarge,
            format!("boîte {x},{y} {w}x{h} hors de l'image {iw}x{ih}"),
        ));
    }
    Ok(image::imageops::crop_imm(image, x as u32, y as u32, w as u32, h as u32).to_image())
}

/// Culture bornée à l'image, avec une marge. `crop_imm` attend une largeur et
/// une hauteur, **pas** un coin : la borne se calcule donc explicitement.
fn borner(image: &image::RgbImage, x: f32, y: f32, l: f32, h: f32, marge: f32) -> image::RgbImage {
    let (iw, ih) = (image.width() as i64, image.height() as i64);
    let x1 = (x - marge).round().max(0.0).min(iw as f32) as i64;
    let y1 = (y - marge).round().max(0.0).min(ih as f32) as i64;
    let x2 = ((x + l + marge).round() as i64).clamp(x1, iw);
    let y2 = ((y + h + marge).round() as i64).clamp(y1, ih);
    image::imageops::crop_imm(
        image,
        x1 as u32,
        y1 as u32,
        (x2 - x1) as u32,
        (y2 - y1) as u32,
    )
    .to_image()
}

/// Base64 standard, sans dépendance externe : le payload d'une page fait
/// quelques centaines de kilo-octets, pas assez pour justifier une crate.
fn base64_decode(s: &str) -> Result<Vec<u8>, String> {
    let mut out = Vec::with_capacity(s.len() * 3 / 4);
    let mut tampon = 0u32;
    let mut bits = 0u32;
    for c in s.bytes() {
        let v = match c {
            b'A'..=b'Z' => c - b'A',
            b'a'..=b'z' => c - b'a' + 26,
            b'0'..=b'9' => c - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            b'=' => break,
            b'\n' | b'\r' => continue,
            _ => return Err(format!("caractère base64 invalide : {}", c as char)),
        } as u32;
        tampon = (tampon << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((tampon >> bits) as u8);
        }
    }
    Ok(out)
}
