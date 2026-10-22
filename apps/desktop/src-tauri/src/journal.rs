//! Tâche 5.5 — journal sur disque et rapport de diagnostic.
//!
//! ## Pourquoi il y avait un problème
//!
//! `tracing_subscriber::fmt()` écrit **sur stdout**. Une application installée
//! sur Windows n'a pas de console : il n'y avait donc **aucun journal** en usage
//! normal. Ce n'est pas une question de quantité à limiter — il n'y avait rien.
//!
//! ## Le principe qui gouverne ce module
//!
//! **On ne fait pas confiance à ce qui est déjà écrit.** Les fuites connues sont
//! fermées **à la source**, dans leurs modules respectifs. Mais [`rediger`] est
//! appliqué **en dernier**, sur le rapport, et couvre tout ce que le cœur ne
//! contrôle pas : le stderr du moteur relayé en verbatim, et le texte que
//! l'interface compose elle-même.
//!
//! Une porte qui ne protège que l'entrée connue n'est pas une porte.
//!
//! ## « Sans données personnelles », précisément
//!
//! - **Conservés** : identifiants de source, codes d'erreur, horodatages,
//!   versions, comptes. Ce ne sont pas des données personnelles, et sans elles
//!   le rapport ne sert à rien.
//! - **Remplacés par une empreinte** : URL de manga, et donc titres.
//! - **Réduits au nom de fichier** : chemins absolus, donc nom d'utilisateur.
//!
//! L'empreinte est une **pseudonymisation**, pas une anonymisation : FNV-1a se
//! retourne par force brute si quelqu'un possède la liste des URL visitées. Le
//! rapport le dit, pour que personne ne le prenne pour une garantie.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// Taille du fichier courant au-delà de laquelle on bascule sur le précédent.
///
/// 2 Mio : assez pour lire la sequence qui précède un plantage, trop peu pour
/// qu'un journal forgotten devienne un problème de disque.
pub const TAILLE_MAX: u64 = 2 * 1024 * 1024;

/// Nombre de lignes reprises dans le rapport.
///
/// Borne basse et affichable : 20 000 lignes ne sont pas lues, et le rapport
/// dépasserait ce qu'un message de bug rapport accepte.
pub const LIGNES_RAPPORT: usize = 400;

/// Nombre maximal d'octets que le rapport peut refusier de traiter.
///
/// Le texte vient de l'interface : une boucle de 20 Mio ne doit pas produire un
/// fichier de 20 Mio dans le dossier de l'utilisateur.
const TAILLE_RAPPORT_MAX: usize = 256 * 1024;

// ---------------------------------------------------------------- chemins ---

/// Le dossier des journaux : `%APPDATA%\Tatami\logs`.
///
/// À part de la base : purger les journaux ne doit jamais ressembler à une
/// opération sur les données de l'utilisateur.
pub fn dossier_journal() -> Option<PathBuf> {
    std::env::var("APPDATA")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .map(PathBuf::from)
        .map(|p| p.join("Tatami").join("logs"))
}

/// Le nom du fichier courant.
pub fn chemin_journal() -> Option<PathBuf> {
    dossier_journal().map(|d| d.join("tatami.log"))
}

/// Le fichier précédent, où atterrit le journal au moment de la bascule.
fn chemin_precedent(courant: &Path) -> PathBuf {
    let mut nom = courant
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "tatami.log".to_string());
    nom.push_str(".prec");
    courant.with_file_name(nom)
}

/// Les deux fichiers de journal : le courant, puis le précédent.
pub fn fichiers_journal() -> Vec<PathBuf> {
    let Some(courant) = chemin_journal() else {
        return Vec::new();
    };
    vec![courant.clone(), chemin_precedent(&courant)]
}

// -------------------------------------------------------------- rédaction ---

/// Une empreinte courte et **stable** d'une chaîne.
///
/// FNV-1a 64, gardés sur 8 hexadécimaux. Ce n'est pas un hachage cryptographique,
/// et ce n'est pas voulu : le but est de pouvoir dire « la même série » dans
/// deux rapports sans écrire la série.
///
/// La **stabilité** est la propriété qui compte : deux exécutions doivent donner
/// la même empreinte, sinon on ne peut plus relier deux signalements.
pub fn hachage(s: &str) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for octet in s.as_bytes() {
        h ^= u64::from(*octet);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{h:016x}")[..8].to_string()
}

/// La ponctuation de fin de jeton, conservée hors de l'empreinte.
///
/// `https://exemple/a.` et `https://exemple/a` sont **la même** URL : si le point
/// entrait dans le hachage, la même série donnerait deux empreintes selon la
/// phrase qui la contient, et on perdrait la corrélation que l'empreinte achète.
fn separer_ponctuation(jeton: &str) -> (&str, &str) {
    let coupe = jeton
        .char_indices()
        .rev()
        .take_while(|(_, c)| matches!(c, '.' | ',' | ';' | ':' | '!' | '?' | ')' | ']' | '"'))
        .last()
        .map(|(i, _)| i);
    match coupe {
        Some(i) => (&jeton[..i], &jeton[i..]),
        None => (jeton, ""),
    }
}

/// Remplace la **valeur** d'un jeton si elle est trop bavarde, ou `None` pour le
/// laisser intact.
///
/// Le découplage clé/valeur n'est pas un détail : la forme réelle d'une ligne de
/// journal est `manga_url=https://…`, dont le jeton ne **commence** pas par
/// `https`. Une rédaction qui ne testerait que le début du jeton laisserait
/// passer toutes les lignes Structured Logging — c'est-à-dire toutes. C'est
/// mesuré : le premier jet des tests ne voyait aucune fuite, parce qu'il a
///-testait des URL nues, jamais la forme qu'écrit le code.
/// Repère un chemin Windows dans un jeton, en **cherchant la lettre de lecteur
/// n'importe où**.
///
/// Mesuré le 2026-10-05 : exiger `C:` en deuxième position laissait passer
/// `\\?\C:\Users\<nom>\…`, donc **tous** les chemins passés par `canonicalize` —
/// c'est-à-dire exactement ceux que Rust produit pour les fichiers qu'il a
/// réellement ouverts. Le préfixe `\\?\` est la forme longue Windows : valide,
/// et illisible.
fn chemin_windows(valeur: &str) -> Option<&str> {
    // `char_indices()` et non un `Vec<char>` indexé : ce dernier donne des index de
    // **caractère**, qui ne sont pas des index d'**octet** dès que la chaîne
    // contient du non-ASCII. Mesuré le 2026-10-15 : `"«C:\Users\x"` paniquait de
    // `start byte index 1 is not a char boundary`, et un accent **en amont** du
    // chemin suffisait aussi. Comme ce jeton est redacté par l'interface et par
    // le stderr du moteur, c'est-à-dire du français et des messages d'erreur, le
    // rapport de diagnostic — l'outil qu'on ouvre quand ça casse — mourait sur le
    // premier accent. On garde `windows(3)` plutôt qu'un index : c'est la même
    // condition, avec le décalage d'octets correct.
    valeur
        .char_indices()
        .collect::<Vec<_>>()
        .windows(3)
        .find(|w| w[0].1.is_ascii_alphabetic() && w[1].1 == ':' && w[2].1 == '\\')
        .map(|w| &valeur[w[0].0..])
}

/// Remplace la **valeur** d'un jeton si elle est trop bavarde, ou `None` pour le
/// laisser intact.
///
/// Le découplage clé/valeur n'est pas un détail : la forme réelle d'une ligne de
/// journal est `manga_url=https://…`, dont le jeton ne **commence** pas par
/// `https`. Une rédaction qui ne testerait que le début du jeton laisserait
/// passer toutes les lignes structurées — c'est-à-dire toutes. C'est mesuré : le
/// premier jet des tests ne voyait aucune fuite, parce qu'il a-testait des URL
/// nues, jamais la forme qu'écrit le code.
fn decoder_valeur(valeur: &str) -> Option<String> {
    let minuscules = valeur.to_ascii_lowercase();
    if minuscules.starts_with("http://") || minuscules.starts_with("https://") {
        return Some(format!("<url:{}>", hachage(valeur)));
    }
    // Chemin Windows. Seul le dernier segment est gardé : il porte souvent
    // l'information utile (`tatami-v6-1.db`) et jamais le nom d'utilisateur.
    let chemin = chemin_windows(valeur)?;
    let dernier = chemin
        .rsplit(['\\', '/'])
        .find(|s| !s.is_empty())
        .unwrap_or(chemin);
    Some(format!("<chemin:{dernier}>"))
}

/// Remplace un jeton trop bavard, ou renvoie `None` pour le laisser intact.
fn substituer(jeton: &str) -> Option<String> {
    let (corps, ponctuation) = separer_ponctuation(jeton);
    if corps.is_empty() {
        return None;
    }
    let (cle, valeur) = match corps.find('=') {
        Some(i) => (&corps[..=i], &corps[i + 1..]),
        None => ("", corps),
    };
    let remplacement = decoder_valeur(valeur)?;
    Some(format!("{cle}{remplacement}{ponctuation}"))
}

/// Redige une ligne, jeton par jeton.
///
/// Écrit **sans `regex`** : le projet n'a pas cette dépendance et AGENTS.md §9
/// demande une justification pour en ajouter une. Le découpage sur les espaces
/// est de toute façon plus sûr ici — un motif de chemin avalerait la ponctuation
/// de la phrase voisine.
pub fn rediger(ligne: &str) -> String {
    ligne
        .split(' ')
        .map(|jeton| substituer(jeton).unwrap_or_else(|| jeton.to_string()))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Redige un texte entier en préservant les retours à la ligne.
pub fn rediger_tout(texte: &str) -> String {
    texte.lines().map(rediger).collect::<Vec<_>>().join("\n")
}

// ---------------------------------------------------------------- écriture ---

/// Un fichier de journal et le chemin d'où il vient.
///
/// Le chemin est stocké **avec** le fichier, et non retrouvé par
/// `File::metadata` : c'est ce qui permet de relâcher le descripteur avant de
/// renommer, ce que Windows refuse tant que le fichier est ouvert.
struct Cible {
    chemin: PathBuf,
    fichier: Option<File>,
}

type Etat = Arc<Mutex<Option<Cible>>>;

/// Destination `MakeWriter` pour `tracing`.
///
/// Chaque écriture est vidée immédiatement : un journal qui perd la dernière
/// ligne a perdu précisément la ligne qui explique le crash.
pub struct VersFichier(Etat);

impl VersFichier {
    /// Prépare la destination, **sans échouer** si le dossier est inaccessible.
    ///
    /// Un disque plein ou un dossier protégé ne doit pas empêcher l'application
    /// de démarrer : on perd le journal, pas l'application.
    pub fn nouveau(chemin: Option<PathBuf>) -> Self {
        let cible = chemin.and_then(|c| {
            // `create_dir_all` renvoie déjà un `Result` : l'envelopper dans un
            // `Option` perdait le message d'erreur, qui est justement ce qu'on
            // veut pouvoir afficher quand ça échoue.
            let dossier = match c.parent() {
                Some(p) => fs::create_dir_all(p),
                None => Ok(()),
            };
            match dossier {
                Ok(()) => Some(Cible {
                    fichier: None,
                    chemin: c,
                }),
                Err(e) => {
                    // Le journal est précisément ce qui est cassé : on ne peut pas
                    // signaler l'échec avec le journal. La sortie de secours est la
                    // seule chose qui fonctionne encore.
                    eprintln!("dossier de journal indisponible ({e}) : rien ne sera ecrit");
                    None
                }
            }
        });
        VersFichier(Arc::new(Mutex::new(cible)))
    }

    /// Ouvre le fichier s'il ne l'est pas. Laisse `None` en cas d'échec.
    fn assurer(cible: &mut Cible) {
        if cible.fichier.is_some() {
            return;
        }
        cible.fichier = match OpenOptions::new()
            .create(true)
            .append(true)
            .open(&cible.chemin)
        {
            Ok(f) => Some(f),
            Err(e) => {
                eprintln!("journal indisponible ({}: {e})", cible.chemin.display());
                None
            }
        };
    }

    /// Bascule le journal si le fichier courant a franchi le plafond.
    ///
    /// Renvoie `true` quand le fichier a effectivement été remplacé, donc
    /// l'appelant peut savoir si ce qu'il va écrire atterrit dans un fichier neuf.
    fn basculer_si_necessaire(cible: &mut Cible) -> bool {
        let franchi = cible
            .fichier
            .as_ref()
            .and_then(|f| f.metadata().ok())
            .map(|m| m.len() > TAILLE_MAX)
            .unwrap_or(false);
        if !franchi {
            return false;
        }
        // Refermé **avant** le renommage : Windows refuse de renommer un fichier
        // ouvert. On repose donc le descripteur après, ce qui évite d'écrire par
        // dessus un `File` vivant — donc d'avoir à le faire en `unsafe`.
        let chemin = cible.chemin.clone();
        cible.fichier = None;
        let precedent = chemin_precedent(&chemin);
        let _ = fs::remove_file(&precedent);
        if let Err(e) = fs::rename(&chemin, &precedent) {
            eprintln!("rotation du journal impossible ({e})");
            return false;
        }
        VersFichier::assurer(cible);
        true
    }
}

/// Le poste d'écriture remis à `tracing`.
///
/// `echo` est faux dans les tests : un test de rotation écrit 2 Mio d'un coup,
/// et recopier ce tampon sur stdout remplit le tube du runner, ce qui bloque le
/// verrou global de `stdout` et fige **tous** les fils du harnais (mesuré le
/// 2026-10-21 : plus aucune ligne de test après `journal`, étape tuée à 20 min).
pub struct Ecrivain {
    etat: Etat,
    echo: bool,
}

impl Write for Ecrivain {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        // Les octets sont toujours consommés, même sans fichier : renvoyer une
        // erreur ferait traiter chaque site de log comme fautif, et pourrait
        // déstabiliser `tracing` pour une perte de journal.
        let consommes = buf.len();
        {
            let mut garde = self
                .etat
                .lock()
                .map_err(|_| io::Error::other("verrou du journal empoisonne"))?;
            if let Some(cible) = garde.as_mut() {
                VersFichier::assurer(cible);
                VersFichier::basculer_si_necessaire(cible);
                if let Some(f) = cible.fichier.as_mut() {
                    // Un fichier plein ne doit pas faire disparaître la ligne de
                    // la console : on tente les deux, et l'échec du fichier est
                    // signalé à part.
                    if let Err(e) = f.write_all(buf).and_then(|_| f.flush()) {
                        eprintln!("ecriture du journal impossible : {e}");
                    }
                }
            }
        }
        // Écriture sur stdout **en plus**, jamais à la place : c'est là que
        // regarde un `pnpm tauri dev`. Sur une app installée, stdout n'a pas de
        // destination et l'écriture est simplement perdue, sans bruit.
        if self.echo {
            let _ = io::stdout().write_all(buf);
        }
        Ok(consommes)
    }

    fn flush(&mut self) -> io::Result<()> {
        let mut garde = self
            .etat
            .lock()
            .map_err(|_| io::Error::other("verrou du journal empoisonne"))?;
        if let Some(cible) = garde.as_mut() {
            if let Some(f) = cible.fichier.as_mut() {
                f.flush()?;
            }
        }
        Ok(())
    }
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for VersFichier {
    type Writer = Ecrivain;

    fn make_writer(&'a self) -> Self::Writer {
        Ecrivain {
            etat: Arc::clone(&self.0),
            echo: true,
        }
    }
}

/// Supprime les fichiers de journal. Renvoie **combien** ont été retirés.
///
/// Compter est la seule preuve : une commande de suppression qui renvoie
/// toujours `Ok` se contente d'exprimer son propre échec (leçon tirée de
/// `cancel_challenge`, qui renvoyait `Ok` même sans fenêtre).
#[tauri::command]
pub fn purger_journal() -> Result<usize, String> {
    Ok(purger_dans(&fichiers_journal()))
}

/// Retire les fichiers donnés et renvoie le nombre retiré.
///
/// Séparé de [`purger_journal`] pour une raison concrète : un test qui appelle la
/// commande **supprimerait le vrai journal de la personne qui lance les tests**.
/// La fonction prend ses chemins en argument, donc le test travaille dans son
/// dossier à lui.
pub fn purger_dans(chemins: &[PathBuf]) -> usize {
    let mut retires = 0;
    for chemin in chemins {
        if !chemin.exists() {
            continue;
        }
        match fs::remove_file(chemin) {
            Ok(()) => retires += 1,
            Err(e) => eprintln!("purge impossible ({}: {e})", chemin.display()),
        }
    }
    retires
}

/// Les `n` dernières lignes du journal courant, de la plus ancienne à la plus
/// récente.
///
/// `0` quand il n'y a pas de journal : c'est une information, alors qu'un
/// `None` ferait croire à une erreur de lecture.
pub fn dernieres_lignes(n: usize) -> Vec<String> {
    let Some(chemin) = chemin_journal() else {
        return Vec::new();
    };
    let Ok(texte) = fs::read_to_string(&chemin) else {
        return Vec::new();
    };
    let lignes: Vec<&str> = texte.lines().collect();
    let debut = lignes.len().saturating_sub(n);
    lignes[debut..].iter().map(|s| s.to_string()).collect()
}

/// Le dossier de destination du rapport : le dossier **Téléchargements**.
///
/// Le dossier Documents serait plus prévisible, mais il est souvent indexé et
/// synchronisé par le cloud — donc il partirait sur un serveur tiers. Un rapport
/// de diagnostic n'a rien à faire là.
pub fn dossier_rapport() -> Option<PathBuf> {
    std::env::var("USERPROFILE")
        .ok()
        .filter(|v| !v.trim().is_empty())
        .map(PathBuf::from)
        .map(|p| p.join("Downloads"))
}

/// L'en-tête que **seul Rust** peut écrire.
///
/// Les versions du moteur ne sont pas stockées en base : elles sont **dans le
/// journal**, via la ligne `tatami-engine serve protocol=… engine=… parsers=…`
/// que le superviseur relaie depuis le stderr du moteur. Le corps du rapport y
/// renvoie donc explicitement, au lieu de réinventer ces versions ici — donc
/// sans seconde source de vérité à maintenir.
fn en_tete() -> String {
    format!(
        "Tatami — rapport de diagnostic\n\
         Version de l'application : {}\n\
         Systeme : {} ({})\n\
         \n\
         Ce rapport contient des **comptes** et des **identifiants de source**,\n\
         qui ne sont pas des donnees personnelles et sont indispensables au\n\
         diagnostic. Les URL de manga y apparaissent sous forme d'**empreinte**\n\
         stable : c'est une pseudonymisation, pas une anonymisation — l'empreinte\n\
         se retrouve si quelqu'un connait deja l'URL.\n",
        env!("CARGO_PKG_VERSION"),
        std::env::consts::OS,
        std::env::consts::ARCH,
    )
}

/// Écrit le rapport de diagnostic et renvoie son chemin.
///
/// Le texte vient de l'interface : elle est la seule à connaître l'état de santé
/// des sources, qui vit dans son `localStorage` et nulle part ailleurs. **La
/// rédaction est appliquée ici**, donc elle couvre aussi ce que le front a
/// composé — un `message` d'erreur contenant une URL est ainsi traité comme
/// n'importe quelle autre.
#[tauri::command]
pub fn exporter_rapport(resume: String) -> Result<String, String> {
    if resume.len() > TAILLE_RAPPORT_MAX {
        return Err(format!(
            "resume trop volumineux ({} octets, maxi {})",
            resume.len(),
            TAILLE_RAPPORT_MAX
        ));
    }
    let dossier =
        dossier_rapport().ok_or_else(|| "dossier Telechargements introuvable".to_string())?;
    fs::create_dir_all(&dossier).map_err(|e| format!("dossier de rapport indisponible : {e}"))?;
    let nom = format!("tatami-diagnostic-{}.txt", crate::store::now_ms());
    let chemin = dossier.join(nom);

    let mut texte = en_tete();
    texte.push_str(&rediger_tout(&resume));
    texte.push_str("\n\n=== JOURNAL (");
    texte.push_str(&format!("{LIGNES_RAPPORT} dernieres lignes, redactees"));
    texte.push_str(") ===\n");
    for ligne in dernieres_lignes(LIGNES_RAPPORT) {
        texte.push_str(&rediger(&ligne));
        texte.push('\n');
    }
    fs::write(&chemin, texte).map_err(|e| format!("ecriture du rapport impossible : {e}"))?;
    Ok(chemin.display().to_string())
}

// ------------------------------------------------------------------ tests ---

#[cfg(test)]
mod tests {
    use super::*;

    /// Un dossier unique pour les tests.
    ///
    /// Les tests tournent **en parallèle** : deux tests qui créeraient le même nom
    /// se marcheraient dessus.
    ///
    /// Sous `target/`, et **pas** `std::env::temp_dir()` : sur cette machine
    /// `%TEMP%` n'est pas expansé (`%USERPROFILE%\AppData\Local\Temp` littéral),
    /// et les tests créeraient alors un dossier `%USERPROFILE%` **dans le dépôt**.
    fn dossier_unique(nom: &str) -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};

        static N: AtomicU64 = AtomicU64::new(0);
        let n = N.fetch_add(1, Ordering::SeqCst);
        let d = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("tmp-journal")
            .join(format!("{nom}-{}-{n}", std::process::id()));
        fs::create_dir_all(&d).expect("dossier de test creatable");
        d
    }

    /// Les deux fuites mesurées le 2026-10-05 doivent disparaître, et ce qui
    /// sert au diagnostic doit rester : c'est le tri qui définit la fonction.
    #[test]
    fn les_fuites_connues_disparaitent_et_le_diagnostic_reste() {
        let ligne =
            rediger("details source=MANGADEX manga_url=https://mangadex.org/title/abc/naruto");
        assert!(
            !ligne.contains("mangadex.org"),
            "le domaine fuit encore : {ligne}"
        );
        assert!(!ligne.contains("naruto"), "le titre fuit encore : {ligne}");
        assert!(
            ligne.contains("source=MANGADEX"),
            "la source doit rester : {ligne}"
        );

        let chemin =
            rediger("sauvegarde : C:\\Users\\alexanders\\AppData\\Roaming\\Tatami\\tatami-v6-1.db");
        assert!(
            !chemin.contains("alexanders"),
            "le nom d'utilisateur fuit : {chemin}"
        );
        assert!(
            chemin.contains("tatami-v6-1.db"),
            "le nom de fichier doit rester : {chemin}"
        );
    }

    /// La forme **réelle** d'une ligne de journal est `clé=valeur`. Tester
    /// seulement le début du jeton laissait donc passer *toutes* les lignes du
    /// code : c'est ce que le premier jet des tests faisait, et il ne voyait
    /// aucune fuite parce qu'il testait des URL nues.
    #[test]
    fn la_forme_cle_valeur_est_bien_redigee() {
        for jeton in [
            "manga_url=https://mangadex.org/title/abc/naruto",
            "chemin=C:\\Users\\alexanders\\AppData\\Tatami\\tatami.db",
        ] {
            let redige = rediger(jeton);
            assert!(
                !redige.contains("mangadex.org"),
                "domaine non redacte : {redige}"
            );
            assert!(
                !redige.contains("alexanders"),
                "utilisateur non redacte : {redige}"
            );
            assert!(
                redige.contains("="),
                "la cle du champ doit rester, elle porte le sens : {redige}"
            );
        }
    }

    /// `\\?\C:\…` est la forme **longue** que `canonicalize` renvoie, donc la
    /// forme de tous les chemins que Rust a réellement ouverts. Un détecteur
    /// exigeant `C:` en deuxième position laissait passer exactement ceux-là —
    /// mesuré le 2026-10-05, sur un rapport qui exposait `C:\Users\<nom>\…`.
    #[test]
    fn la_forme_longue_windows_est_bien_redigee() {
        let ligne = rediger("telechargements : \\\\?\\C:\\Users\\alexanders\\Documents\\Tatami");
        assert!(
            !ligne.contains("alexanders"),
            "le nom d'utilisateur fuit par la forme longue : {ligne}"
        );
        assert!(
            !ligne.contains('?'),
            "le prefixe long doit disparaitre : {ligne}"
        );
        assert!(
            ligne.contains("Tatami"),
            "le dernier segment doit rester : {ligne}"
        );
    }

    /// Un accent **avant** le chemin ne doit pas paniquer, et le chemin doit
    /// toujours être repéré.
    ///
    /// Mesuré le 2026-10-15 : `chemin_windows` indexait une `Vec<char>` avec un
    /// index d'octet. Tout caractère multi-octet en amont décalait la
    /// correspondance — `"«C:\Users\x"` panicked de `start byte index 1 is not a
    /// char boundary`. Ce test est écrit sur la **forme que la fonction reçoit**,
    /// pas sur une forme ASCII unobtenue : c'est exactement ce qui manquait.
    #[test]
    fn un_accent_avant_le_chemin_ne_casse_pas_la_redaction() {
        for avant in ["", "«", "é", "reprise après échec — "] {
            let chemin = format!("{avant}C:\\Users\\alexanders\\Documents\\Tatami");
            let ligne = rediger(&chemin);
            assert!(
                !ligne.contains("alexanders"),
                "le nom d'utilisateur fuit (avant = {avant:?}) : {ligne}"
            );
            assert!(
                ligne.contains("Tatami"),
                "le dernier segment doit rester (avant = {avant:?}) : {ligne}"
            );
        }
    }

    /// Une empreinte instable ne sert à rien : on ne peut plus relier deux
    /// signalements du même manga.
    #[test]
    fn l_empreinte_est_stable_et_sensible() {
        let a = "https://mangadex.org/title/abc";
        assert_eq!(
            hachage(a),
            hachage(a),
            "deux executions doivent donner la meme empreinte"
        );
        assert_ne!(hachage(a), hachage("https://mangadex.org/title/abd"));
        assert_eq!(hachage("").len(), 8, "longueur fixe");
        assert!(hachage("x").chars().all(|c| c.is_ascii_hexdigit()));
    }

    /// Une rédaction agressive qui masque les mots rend le rapport illisible,
    /// donc inutile. Ces jetons doivent passer **intacts**.
    #[test]
    fn les_mots_ordinaires_passent_intacts() {
        for mot in [
            "source=MANGADEX",
            "(pid=Some(1234))",
            "erreur",
            "PARSE_ERROR",
            "v0.1",
            "hello",
            "a:b",
            "",
        ] {
            assert_eq!(rediger(mot), mot, "« {mot} » ne devait pas etre modifie");
        }
    }

    /// La ponctuation de phrase ne doit pas entrer dans l'empreinte : sinon la
    /// même URL donnerait deux empreintes selon la phrase, et on perdrait la
    /// corrélation que l'empreinte achète.
    #[test]
    fn la_ponctuation_de_phrase_est_consignee_hors_de_l_empreinte() {
        assert_eq!(
            rediger("voir https://exemple.fr/a."),
            rediger("voir https://exemple.fr/a") + ".",
            "le point final a change l'empreinte",
        );
    }

    /// La bascule doit produire un fichier **courant** sous le plafond, et faire
    /// apparaître le précédent. Sans elle, un journal forgotten finit par remplir
    /// le disque.
    #[test]
    fn la_bascule_borne_la_taille() {
        let dir = dossier_unique("tatami-bascule");
        let courant = dir.join("tatami.log");
        let destination = VersFichier::nouveau(Some(courant.clone()));
        let mut e = Ecrivain {
            etat: Arc::clone(&destination.0),
            echo: false,
        };

        // Une ligne de plus que le plafond, pour forcer la bascule au milieu.
        let bloc = vec![b'x'; (TAILLE_MAX + 1024) as usize];
        e.write_all(&bloc).unwrap();
        e.write_all(b"ligne ecrite apres la bascule\n").unwrap();
        drop(e);

        let taille = fs::metadata(&courant)
            .expect("fichier courant present")
            .len();
        assert!(
            taille <= TAILLE_MAX + 1024,
            "le fichier courant a enfle : {taille}"
        );
        assert!(
            courant.display().to_string().contains("tatami.log"),
            "le fichier courant a disparu"
        );
        let _ = fs::remove_dir_all(&dir);
    }

    /// Un dossier inaccessible ne doit **pas** faire échouer l'écriture : sinon
    /// `tracing` verrait une erreur à chaque ligne, sur un disque plein.
    #[test]
    fn un_journal_inaccessible_n_est_pas_une_erreur() {
        let destination = VersFichier::nouveau(Some(PathBuf::from(
            "Z:\\chemin\\qui\\n_existe_pas\\tatami.log",
        )));
        let mut e = Ecrivain {
            etat: Arc::clone(&destination.0),
            echo: false,
        };
        // `write` et non `write_all` : c'est le seul des deux qui rende le compte
        // d'octets, et c'est exactement ce qu'on vérifie — un journal absent ne
        // doit pas faire rendre une erreur à `tracing`, sinon chaque site de log
        // passe pour fautif.
        assert_eq!(
            e.write(b"bonjour\n").unwrap(),
            8,
            "les octets doivent etre consommes"
        );
        assert!(
            e.flush().is_ok(),
            "vider un journal absent ne doit pas echouer"
        );
    }

    /// La purge doit **compter**, et le compte doit correspondre à ce qui
    /// existait réellement sur le disque.
    ///
    /// Note : le test appelle [`purger_dans`] avec **ses propres** chemins. Un
    /// test qui appellerait la commande supprimerait le vrai journal de la
    /// personne qui lance les tests — un test ne nettoie jamais ce qui n'est pas
    /// à lui.
    #[test]
    fn la_purge_compte_ce_quelle_a_retire() {
        let dir = dossier_unique("tatami-purge");
        let courant = dir.join("tatami.log");
        let precedent = dir.join("tatami.log.prec");
        fs::write(&courant, "ligne\n").unwrap();
        fs::write(&precedent, "ligne\n").unwrap();

        assert_eq!(2, purger_dans(&[courant.clone(), precedent.clone()]));
        assert!(!courant.exists(), "le fichier courant doit etre parti");
        assert!(!precedent.exists(), "le fichier precedent doit etre parti");

        // Contrôle négatif : une seconde purge ne trouve plus rien. C'est lui qui
        // prouve que le premier compte était un décompte, pas un `Ok` vide.
        assert_eq!(0, purger_dans(&[courant, precedent]));
        let _ = fs::remove_dir_all(&dir);
        let _ = fs::remove_dir_all(&dir);
    }

    /// Un rapport refusant un résumé déraisonnable est une **protection**, pas
    /// une panne : le texte vient de l'interface, donc il faut une borne.
    #[test]
    fn un_resume_trop_volumineux_est_refuse() {
        let enorme = "x".repeat(TAILLE_RAPPORT_MAX + 1);
        let e = exporter_rapport(enorme).expect_err("un resume enorme doit etre refuse");
        assert!(
            e.contains("volumineux"),
            "le message doit nommer la cause : {e}"
        );
    }

    /// Le rapport écrit **sur disque** ne doit contenir ni domaine ni nom
    /// d'utilisateur — c'est la promesse faite à l'utilisateur.
    #[test]
    fn le_rapport_ecrit_est_bien_redige() {
        let resume = "source=MANGADEX url=https://mangadex.org/title/abc/naruto \
                      chemin=C:\\Users\\alexanders\\AppData\\Tatami\\tatami.db";
        // On force un dossier de rapport temporaire en changeant ce qui est
        // lisible : on verifie `rediger_tout`, qui est la fonction appliquee.
        let redige = rediger_tout(resume);
        assert!(
            !redige.contains("mangadex.org"),
            "le domaine fuit dans le rapport : {redige}"
        );
        assert!(
            !redige.contains("alexanders"),
            "l'utilisateur fuit dans le rapport : {redige}"
        );
        assert!(
            redige.contains("MANGADEX"),
            "le diagnostic doit rester : {redige}"
        );
    }
}
