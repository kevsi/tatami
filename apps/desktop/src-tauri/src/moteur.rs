//! Tâche 5.1 — mise à jour du moteur : signature, installation, activation,
//! retour arrière.
//!
//! ## La question que 5.1 tranche
//!
//! AGENTS.md §10 : « les mises à jour du moteur sont téléchargées depuis un
//! manifeste **signé** ; l'app vérifie la signature et le hash, remplace de façon
//! atomique, et revient en arrière si le démarrage échoue. Ne jamais exécuter un
//! JAR non vérifié. »
//!
//! Deux de ces quatre exigences sont des décisions, pas du code :
//!
//! - **La racine de confiance.** Elle est **auto-gérée** : une clé Ed25519 dont la
//!   partie publique est **embarquée dans le binaire** ([`CLE_PUBLIQUE`]) et dont
//!   la partie privée vit **hors du dépôt**. C'est ce qui distingue 5.1 de 5.6 :
//!   5.6 signe l'**installeur** et exige une autorité de certification payante ;
//!   5.1 signe le **manifeste du moteur**, ce qu’unmono-développeur peut faire
//!   seul. Les deux ne se remplacent pas, et l'un ne débloque pas l'autre.
//! - **La source du manifeste.** Aucune par défaut. Tant que l'utilisateur n'a
//!   rien configuré, ce module est **inerte** : rien ne se télécharge tout seul.
//!
//! ## Ce que la signature couvre, et pourquoi pas le JSON
//!
//! La signature porte sur un **texte canonique** ([`charge_signee`]), pas sur le
//! JSON : le JSON a des espaces libres, un ordre de clés libre et des
//! échappements, donc signer « le JSON » demanderait deux implémentations d'une
//! canonicalisation — et elles divergeraient. Le texte est produit par deux
//! côtés indépendants : `scripts/moteur-sign.mjs` côté signature, [`charge_signee`]
//! côté vérification. Une divergence ne peut donc passer que par un test de
//! contrat, et il y en a un.
//!
//! ## Pourquoi aucune version n'est écrasée
//!
//! Une installation remplace **tout** le moteur : runtime, JAR, lanceur. Le faire
//! par-dessus l'existant n'est pas atomique — un plantage au milieu laisse un
//! moteur dans un état qu'aucune version ne décrit. Donc : chaque version est
//! extraite dans `versions/<version>/`, et un **pointeur** [`Actif`] désigne la
//! version courante. Retour arrière = réécrire le pointeur. C'est la raison pour
//! laquelle le moteur a besoin d'un dossier **écrivable** : en app installée, la
//! ressource embarquée est dans `Program Files`, donc non modifiable.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use ed25519_dalek::{Signature, VerifyingKey};
use sha2::{Digest, Sha256};

/// La clé publique du dépôt, embarquée à la compilation.
///
/// `include_bytes!` produit `&[u8; 32]` **et refuse de compiler** si le fichier ne
/// fait pas exactement 32 octets. C'est une porte gratuite : on ne peut pas embarquer
/// une clé tronquée sans que la compilation échoue.
const CLE_PUBLIQUE: &[u8; 32] = include_bytes!("../keys/cle-moteur.pub");

/// Première ligne de la charge signée.
///
/// Sans elle, un changement de format passerait inaperçu : les deux côtés
/// signeraient deux choses différentes et le refus serait inexplicable.
pub const DOMAINE: &str = "tatami:moteur:manifeste:1";

/// Les champs couverts par la signature, dans un ordre **fixe**.
///
/// La liste est écrite deux fois : ici et dans `scripts/moteur-sign.mjs`. Le test
/// [`les_champs_signes_concordent_avec_le_script`] compare les deux, donc un champ
/// ajouté d'un seul côté est rouge au lieu d'être silencieusement non protégé.
pub const CHAMPS_SIGNES: [&str; 5] = ["version", "engine", "parsers", "archive", "sha256"];

/// Ce qu'un manifeste déclare.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct Manifeste {
    pub version: String,
    pub engine: String,
    pub parsers: String,
    pub archive: String,
    pub sha256: String,
    #[serde(default)]
    pub signature: String,
}

/// Le contenu de `actif.json` : la version en service, et celle d'avant.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
pub struct Actif {
    pub version: String,
    /// La version **à laquelle revenir** si la nouvelle refuse de démarrer.
    #[serde(default)]
    pub precedente: Option<String>,
}

/// Les raisons pour lesquelles une mise à jour est refusée.
///
/// Chaque variante porte le **quoi**, jamais « échec » : un refus qui ne dit pas
/// pourquoi oblige à deviner, et deviner sur une chaîne de confiance, c'est
/// accepter.
#[derive(Debug)]
pub enum Erreur {
    Json(String),
    ChampManquant(String),
    SignatureInvalide,
    SignatureAbsente,
    EmpreinteInattendue {
        attendue: String,
        obtenue: String,
    },
    Io(String),
    /// L'archive contient un chemin qui remonte : AGENTS.md §6 impose de
    /// n'écrire **jamais** hors du dossier de destination.
    CheminHostile(String),
    /// La version annoncée est déjà celle en service : rien à faire.
    DejaActive(String),
    /// Aucune version précédente : impossible de revenir en arrière.
    AucunePrecedente,
}

impl std::fmt::Display for Erreur {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Erreur::Json(m) => write!(f, "manifeste illisible : {m}"),
            Erreur::ChampManquant(c) => {
                write!(f, "champ absent ou vide dans le manifeste : {c}")
            }
            Erreur::SignatureAbsente => write!(
                f,
                "manifeste non signé : le champ `signature` est vide. Un moteur \
                 sans signature n'est jamais exécuté (AGENTS.md §10)."
            ),
            Erreur::SignatureInvalide => write!(
                f,
                "signature invalide : le manifeste n'a pas été signé par la clé de \
                 ce dépôt, ou il a été modifié après signature."
            ),
            Erreur::EmpreinteInattendue { attendue, obtenue } => write!(
                f,
                "empreinte de l'archive inattendue : announced {attendue}, obtenu {obtenue}"
            ),
            Erreur::Io(m) => write!(f, "opération sur disque impossible : {m}"),
            Erreur::CheminHostile(n) => write!(
                f,
                "l'archive contient une entrée qui remonte de chemin : {n}. Refus d'écrire."
            ),
            Erreur::DejaActive(v) => write!(f, "la version {v} est déjà celle en service"),
            Erreur::AucunePrecedente => write!(
                f,
                "aucune version précédente : il n'y a rien vers quoi revenir"
            ),
        }
    }
}

// ------------------------------------------------------- charge signée ---

/// Le texte canonique signé. **Doit** être identique à celui de
/// `scripts/moteur-sign.mjs`.
pub fn charge_signee(m: &Manifeste) -> Result<String, Erreur> {
    let mut lignes = vec![DOMAINE.to_string()];
    for champ in CHAMPS_SIGNES {
        let valeur = match champ {
            "version" => &m.version,
            "engine" => &m.engine,
            "parsers" => &m.parsers,
            "archive" => &m.archive,
            "sha256" => &m.sha256,
            autre => return Err(Erreur::ChampManquant(autre.to_string())),
        };
        if valeur.trim().is_empty() {
            return Err(Erreur::ChampManquant(champ.to_string()));
        }
        lignes.push(format!("{champ}={}", valeur.trim()));
    }
    // Chaque ligne est terminée par \n, la dernière comprise : sans cela, deux
    // charges qui ne diffèrent que par la ligne finale seraient identiques.
    Ok(lignes.join("\n") + "\n")
}

/// Vérifie une signature contre une clé donnée.
///
/// La clé est un **paramètre** et non une constante : les tests doivent pouvoir
/// fabriquer un manifeste valide, or la clé privée est **hors du dépôt** — c'est
/// tout l'intérêt. Une fonction qui lit [`CLE_PUBLIQUE`] en dur serait correcte et
/// non testable, donc bien testable et non vérifiée.
pub fn verifier_avec_cle(cle: &[u8; 32], charge: &str, signature_b64: &str) -> Result<(), Erreur> {
    let octets = base64_decode(signature_b64).ok_or(Erreur::SignatureInvalide)?;
    if octets.len() != 64 {
        return Err(Erreur::SignatureInvalide);
    }
    let mut brut = [0u8; 64];
    brut.copy_from_slice(&octets);
    let signature = Signature::from_bytes(&brut);

    let publique = VerifyingKey::from_bytes(cle).map_err(|_| Erreur::SignatureInvalide)?;
    // `verify_strict` et non `verify` : la variante stricte refuse aussi les
    // signatures à clé publique de petit ordre, donc elle ne se laisse pas
    // contourner par une clé dégénérée.
    publique
        .verify_strict(charge.as_bytes(), &signature)
        .map_err(|_| Erreur::SignatureInvalide)
}

/// Vérifie le manifeste contre la clé **embarquée**.
pub fn verifier(m: &Manifeste) -> Result<(), Erreur> {
    if m.signature.trim().is_empty() {
        return Err(Erreur::SignatureAbsente);
    }
    verifier_avec_cle(CLE_PUBLIQUE, &charge_signee(m)?, m.signature.trim())
}

/// Parse un manifeste **et** le vérifie dans la foulée.
///
/// Les deux dans une seule fonction, volontairement : rendre ces deux étapes
/// séparées inviterait un appelant qui oublie la vérification — et une signature
/// vérifiée « quand on y pense » n'est pas une vérification.
pub fn charger_signe(json: &str) -> Result<Manifeste, Erreur> {
    let m: Manifeste = serde_json::from_str(json).map_err(|e| Erreur::Json(e.to_string()))?;
    verifier(&m)?;
    Ok(m)
}

// ------------------------------------------------------------ empreintes ---

/// L'empreinte SHA-256 d'un fichier, en hexadécimal minuscule.
pub fn empreinte_fichier(chemin: &Path) -> Result<String, Erreur> {
    let mut fichier = fs::File::open(chemin).map_err(|e| Erreur::Io(e.to_string()))?;
    let mut h = Sha256::new();
    std::io::copy(&mut fichier, &mut h).map_err(|e| Erreur::Io(e.to_string()))?;
    Ok(hex(&h.finalize()))
}

fn hex(octets: &[u8]) -> String {
    octets.iter().map(|o| format!("{o:02x}")).collect()
}

/// Décodage base64 **strict**.
///
/// On n'utilise pas le base64 de `base64` déjà présent : ici il suffit de 64
/// octets, et une erreur de décodage doit être un refus net, pas un `unwrap`.
fn base64_decode(texte: &str) -> Option<Vec<u8>> {
    let table = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let nettoye: Vec<u8> = texte
        .bytes()
        .filter(|b| !b.is_ascii_whitespace() && *b != b'=')
        .collect();
    if nettoye.len() % 4 == 1 {
        return None;
    }
    let mut out = Vec::with_capacity(nettoye.len() / 4 * 3);
    let mut tampon = 0u32;
    let mut bits = 0u32;
    for b in nettoye {
        let v = table.iter().position(|t| *t == b)? as u32;
        tampon = (tampon << 6) | v;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((tampon >> bits) as u8);
        }
    }
    Some(out)
}

// ------------------------------------------------------------ arborescence ---

/// `%APPDATA%\Tatami\moteur` — le dossier **écrivable** des versions installées.
pub fn dossier_moteur() -> Option<PathBuf> {
    std::env::var("APPDATA")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .map(PathBuf::from)
        .map(|p| p.join("Tatami").join("moteur"))
}

/// Le pointeur : quel moteur est en service.
pub fn chemin_actif() -> Option<PathBuf> {
    dossier_moteur().map(|d| d.join("actif.json"))
}

/// Les versions installées, côte à côte.
pub fn dossier_versions() -> Option<PathBuf> {
    dossier_moteur().map(|d| d.join("versions"))
}

/// Relit le pointeur. `None` quand aucun moteur n'a été installé — c'est l'état
/// normal d'une application qui n'a jamais été mise à jour.
pub fn lire_actif() -> Option<Actif> {
    let chemin = chemin_actif()?;
    let texte = fs::read_to_string(chemin).ok()?;
    serde_json::from_str(&texte).ok()
}

/// Le chemin du moteur en service, s'il y en a un d'installé.
fn dossier_version(version: &str) -> Option<PathBuf> {
    Some(dossier_versions()?.join(version))
}

/// `(java, classpath)` du moteur installé, ou `None`.
///
/// `None` signifie « aucun moteur installé » et l'appelant passe au repli
/// (ressource embarquée). C'est le chemin ordinaire, pas une erreur.
pub fn installe() -> Option<(PathBuf, String)> {
    let actif = lire_actif()?;
    let dossier = dossier_version(&actif.version)?;
    let java = dossier.join("runtime").join("bin").join("java.exe");
    java.is_file().then(|| {
        let cp = format!(r"{}\lib\*", dossier.display());
        (java, cp)
    })
}

/// Bascule le pointeur sur `version`.
///
/// **Atomique** : on écrit `actif.json.tmp` puis on renome. Sur Windows un
/// renommage échoue si la cible existe, donc l'ancien est retiré juste avant —
/// fenêtre de l'ordre de la milliseconde, pendant laquelle le moteur précédent
/// reste **intact sur le disque**. Une coupure de courant là ne perd rien : il
/// suffit de régénérer le pointeur, alors que l'autre méthode (écrire par-dessus)
/// laisserait un fichier tronqué.
pub fn activer(version: &str) -> Result<Actif, Erreur> {
    if version.trim().is_empty() || version.contains(['/', '\\', '.']) {
        return Err(Erreur::ChampManquant("version".to_string()));
    }
    let dossier = dossier_version(version).ok_or_else(|| {
        Erreur::Io("dossier des versions introuvable (APPDATA absent ?)".to_string())
    })?;
    let java = dossier.join("runtime").join("bin").join("java.exe");
    if !java.is_file() {
        return Err(Erreur::Io(format!(
            "version {version} non installée : {} est absent",
            java.display()
        )));
    }

    let precedent = lire_actif()
        .filter(|a| a.version != version)
        .map(|a| a.version);
    let nouvel = Actif {
        version: version.to_string(),
        precedente: precedent,
    };

    let chemin = chemin_actif().ok_or_else(|| Erreur::Io("APPDATA absent".to_string()))?;
    fs::create_dir_all(chemin.parent().unwrap_or(&chemin))
        .map_err(|e| Erreur::Io(e.to_string()))?;
    let temporaire = chemin.with_extension("json.tmp");
    let mut f = fs::File::create(&temporaire).map_err(|e| Erreur::Io(e.to_string()))?;
    f.write_all(serde_json::to_string_pretty(&nouvel).unwrap().as_bytes())
        .map_err(|e| Erreur::Io(e.to_string()))?;
    f.sync_all().map_err(|e| Erreur::Io(e.to_string()))?;
    drop(f);

    let _ = fs::remove_file(&chemin);
    fs::rename(&temporaire, &chemin).map_err(|e| Erreur::Io(e.to_string()))?;
    Ok(nouvel)
}

/// Revient à la version précédente. Renvoie `false` s'il n'y en avait pas.
///
/// Le geste est volontairement le même que [`activer`] : écrire le pointeur. Il
/// n'y a donc qu'**un seul** chemin qui peut rendre le moteur en service, et donc
/// une seule chose à vérifier.
pub fn revenir_en_arriere() -> Result<bool, Erreur> {
    let Some(actif) = lire_actif() else {
        return Ok(false);
    };
    let Some(precedente) = actif.precedente.clone() else {
        return Err(Erreur::AucunePrecedente);
    };
    activer(&precedente)?;
    Ok(true)
}

// ----------------------------------------------------------- installation ---

/// Extrait l'archive du moteur dans `versions/<version>/`.
///
/// L'empreinte est vérifiée **avant** toute extraction : une archive dont le
/// contenu ne correspond pas au manifeste n'a rien à décompresser, et la
/// décompresser serait du travail offert à quelqu'un qui veut juste saturer le
/// disque.
pub fn installer(archive: &Path, m: &Manifeste) -> Result<PathBuf, Erreur> {
    verifier(m)?;

    let obtenue = empreinte_fichier(archive)?;
    if obtenue != m.sha256.trim().to_ascii_lowercase() {
        return Err(Erreur::EmpreinteInattendue {
            attendue: m.sha256.clone(),
            obtenue,
        });
    }

    let destination = dossier_version(&m.version)
        .ok_or_else(|| Erreur::Io("dossier des versions introuvable".to_string()))?;
    if destination
        .join("runtime")
        .join("bin")
        .join("java.exe")
        .is_file()
    {
        return Err(Erreur::DejaActive(m.version.clone()));
    }
    // Extraction dans un dossier **stagiaire**, puis renommage : si l'extraction
    // s'arrête au milieu, `versions/<version>` reste absent plutôt que
    // présent-et-incomplet. C'est la même raison que pour le pointeur.
    let temporaire = destination.with_extension(".en-cours");
    let _ = fs::remove_dir_all(&temporaire);
    fs::create_dir_all(&temporaire).map_err(|e| Erreur::Io(e.to_string()))?;

    let entrees = crate::zipread::lire_entrees(archive).map_err(|e| Erreur::Io(e.to_string()))?;
    let mut fichier = fs::File::open(archive).map_err(|e| Erreur::Io(e.to_string()))?;
    let mut ecrits = 0usize;
    for entree in &entrees {
        // **Refus**, pas « on saute » : `zipread` écarte discrètement les noms
        // hostiles, ce qui convient à la lecture d'un `.cbz`. Ici, une entrée
        // écartée signifie une installation incomplète, donc silencieuse.
        if entree.nom.contains("..") || entree.nom.starts_with('/') || entree.nom.contains('\\') {
            let _ = fs::remove_dir_all(&temporaire);
            return Err(Erreur::CheminHostile(entree.nom.clone()));
        }
        let contenu = crate::zipread::lire_entree(&mut fichier, entree)
            .map_err(|e| Erreur::Io(e.to_string()))?;
        let cible = temporaire.join(&entree.nom);
        if entree.nom.ends_with('/') {
            fs::create_dir_all(&cible).map_err(|e| Erreur::Io(e.to_string()))?;
            continue;
        }
        if let Some(parent) = cible.parent() {
            fs::create_dir_all(parent).map_err(|e| Erreur::Io(e.to_string()))?;
        }
        fs::write(&cible, &contenu).map_err(|e| Erreur::Io(e.to_string()))?;
        ecrits += 1;
    }
    if ecrits == 0 {
        let _ = fs::remove_dir_all(&temporaire);
        return Err(Erreur::Io(
            "l'archive ne contient aucun fichier".to_string(),
        ));
    }

    let _ = fs::remove_dir_all(&destination);
    fs::rename(&temporaire, &destination).map_err(|e| Erreur::Io(e.to_string()))?;
    Ok(destination)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    /// Une clé **de test**, écrite en clair dans le test.
    ///
    /// Ce n'est pas un secret : c'est une graine fixe, et la clé privée réelle
    /// est hors du dépôt. Le but est que le chemin « manifeste valide » soit
    /// exerçable en test sans dépendre d'un fichier hors du dépôt.
    const GRAINE_TEST: [u8; 32] = [42u8; 32];

    fn signer(m: &mut Manifeste, cle: &SigningKey) {
        let charge = charge_signee(m).expect("charge canonique");
        m.signature = base64_encode(&cle.sign(charge.as_bytes()).to_bytes());
    }

    fn base64_encode(octets: &[u8]) -> String {
        const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = String::new();
        for tranche in octets.chunks(3) {
            let b = [
                tranche[0],
                *tranche.get(1).unwrap_or(&0),
                *tranche.get(2).unwrap_or(&0),
            ];
            let v = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
            out.push(T[(v >> 18) as usize & 63] as char);
            out.push(T[(v >> 12) as usize & 63] as char);
            out.push(if tranche.len() > 1 {
                T[(v >> 6) as usize & 63] as char
            } else {
                '='
            });
            out.push(if tranche.len() > 2 {
                T[v as usize & 63] as char
            } else {
                '='
            });
        }
        out
    }

    fn manifeste_de_test() -> Manifeste {
        Manifeste {
            version: "0.2.0".into(),
            engine: "0.2.0-SNAPSHOT".into(),
            parsers: "b3c0000".into(),
            archive: "tatami-engine-0.2.0.zip".into(),
            sha256: "a".repeat(64),
            signature: String::new(),
        }
    }

    /// Le chemin nominal : un manifeste signé par la bonne clé passe.
    #[test]
    fn un_manifeste_signe_passe() {
        let cle = SigningKey::from_bytes(&GRAINE_TEST);
        let publique = cle.verifying_key().to_bytes();
        let mut m = manifeste_de_test();
        signer(&mut m, &cle);
        verifier_avec_cle(&publique, &charge_signee(&m).unwrap(), &m.signature).unwrap();
    }

    /// Un champ modifié **après** signature doit être refusé, et le refus doit
    /// nommer le champ en cause — sinon on ne sait pas quoi reprocher à l'auteur.
    ///
    /// Chaque champ est testé séparément : un test qui boucle dirait seulement
    /// « ça échoue ».
    #[test]
    fn modifier_un_champ_apres_signature_est_refuse() {
        let cle = SigningKey::from_bytes(&GRAINE_TEST);
        let publique = cle.verifying_key().to_bytes();
        let base = {
            let mut m = manifeste_de_test();
            signer(&mut m, &cle);
            m
        };

        let variantes = [
            ("version", "9.9.9".to_string()),
            ("engine", "9.9.9".to_string()),
            ("parsers", "deadbee".to_string()),
            ("archive", "autre.zip".to_string()),
            ("sha256", "b".repeat(64)),
        ];
        for (champ, valeur) in variantes {
            let mut m = base.clone();
            match champ {
                "version" => m.version = valeur,
                "engine" => m.engine = valeur,
                "parsers" => m.parsers = valeur,
                "archive" => m.archive = valeur,
                _ => m.sha256 = valeur,
            }
            // La charge est **recalculée** : c'est le pire cas, puisque le
            // signataire aurait pu produire exactement ce texte.
            let e = verifier_avec_cle(&publique, &charge_signee(&m).unwrap(), &m.signature)
                .expect_err("un champ modifie doit etre refuse");
            assert!(matches!(e, Erreur::SignatureInvalide), "{champ} : {e:?}");
        }
    }

    /// Une signature valide sous **une autre** clé ne vaut rien : c'est le test
    /// qui distingue « signature présente » de « signature de *ce* dépôt ».
    #[test]
    fn une_signature_d_une_autre_cle_est_refusee() {
        let signante = SigningKey::from_bytes(&GRAINE_TEST);
        let autre = SigningKey::from_bytes(&[7u8; 32]);
        let mut m = manifeste_de_test();
        signer(&mut m, &signante);
        let erreur = verifier_avec_cle(
            &autre.verifying_key().to_bytes(),
            &charge_signee(&m).unwrap(),
            &m.signature,
        )
        .expect_err("une signature etrangere doit etre refusee");
        assert!(matches!(erreur, Erreur::SignatureInvalide));
    }

    /// Un manifeste sans signature doit dire qu'il est **non signé**, pas
    /// « signature invalide » : les deux sont des refus, mais l'un est
    /// réparable et l'autre non, et l'utilisateur doit pouvoir les distinguer.
    #[test]
    fn un_manifeste_non_signe_le_dit() {
        let m = manifeste_de_test();
        let e = verifier(&m).expect_err("un manifeste nu doit etre refuse");
        assert!(matches!(e, Erreur::SignatureAbsente), "{e:?}");
        assert!(e.to_string().contains("jamais exécuté"), "{e}");
    }

    /// Un champ vide doit être refusé **avant** toute.crypto, et nommé : sinon le
    /// signataire découvrirait autre chose.
    #[test]
    fn un_champ_vide_est_nomme() {
        let mut m = manifeste_de_test();
        m.parsers = "   ".into();
        let e = charge_signee(&m).expect_err("un champ vide doit etre refuse");
        assert!(
            matches!(e, Erreur::ChampManquant(ref c) if c == "parsers"),
            "{e:?}"
        );
    }

    /// Le base64 doit être décodé **strictement** : une signature tronquée ou
    /// contenant des caractères étrangers est un refus, pas  une panique。
    #[test]
    fn une_signature_tronquee_est_refusee() {
        let cle = SigningKey::from_bytes(&GRAINE_TEST);
        let publique = cle.verifying_key().to_bytes();
        let mut m = manifeste_de_test();
        signer(&mut m, &cle);
        let tronquee = &m.signature[..m.signature.len() - 4];
        assert!(verifier_avec_cle(&publique, &charge_signee(&m).unwrap(), tronquee).is_err());
    }

    /// **Contrat avec le script de signature.** Le fichier `.mjs` vit hors de
    /// Rust, donc rien ne garantit que les deux listes de champs restent les
    /// mêmes : le test lit le script et compare. C'est le seul endroit où cette
    /// garantie peut être vérifiée automatiquement.
    #[test]
    fn les_champs_signes_concordent_avec_le_script() {
        let script = fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("..")
                .join("..")
                .join("..")
                .join("scripts")
                .join("moteur-sign.mjs"),
        )
        .expect("scripts/moteur-sign.mjs");
        for champ in CHAMPS_SIGNES {
            assert!(
                script.contains(&format!("'{champ}'")),
                "le champ {champ} est signé en Rust mais pas dans le script"
            );
        }
        // Le domaine aussi : un changement de l'un des deux doit se voir.
        assert!(
            script.contains(DOMAINE),
            "le domaine signé differe entre Rust et le script"
        );
    }

    /// Le nom de version est un **chemin** : il ne doit pas pouvoir contenir de
    /// séparateur, sinon `versions/<version>/..` sortirait du dossier.
    #[test]
    fn un_nom_de_version_hostile_est_refuse() {
        for nom in ["../ailleurs", "a/b", "a\\b", ".", "", "   "] {
            let e = activer(nom).expect_err("un nom hostile doit etre refuse");
            assert!(
                matches!(e, Erreur::ChampManquant(_) | Erreur::Io(_)),
                "« {nom} » : {e:?}"
            );
        }
    }
}
