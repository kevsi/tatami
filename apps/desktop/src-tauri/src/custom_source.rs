//! Commandes Tauri pour les sources personnalisées (scripts JavaScript locaux).
//!
//! L'interface dépose un fichier `.js` fourni par l'utilisateur ; cette
//! couche valide, copie dans le dossier applicatif et enregistre en base.
//! Le moteur JVM charge les scripts depuis ce dossier au démarrage.
//!
//! Sécurité :
//! - Taille maximale 256 Ko (protection contre des scripts hors-contrôle).
//! - Le nom de fichier ne doit pas contenir `..` ni de séparateur de chemin.
//! - La destination est canonicalisée et comparée à `sources_dir()` (anti
//!   path-traversal).
//! - `dunce::canonicalize` est utilisé pour éviter le préfixe `\\?\` sur
//!   Windows, cohérent avec le reste du codebase.

use serde::Deserialize;
use std::{fs, path::Path, path::PathBuf, sync::Arc};

use tauri::State;

use crate::store::{CustomSource, Store};

/// Taille maximale d'un script importé (256 Ko).
const MAX_SCRIPT_BYTES: u64 = 256 * 1024;

/// Dossier de stockage des scripts. Sous `%APPDATA%\Tatami\sources\` sur
/// Windows, ou `$HOME/Tatami/sources` en fallback.
fn sources_dir() -> PathBuf {
    let base = std::env::var("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| dirs_next::home_dir().unwrap_or_else(|| PathBuf::from(".")));
    base.join("Tatami").join("sources")
}

/// Valide et importe un script `.js` depuis `path` vers `dest_dir`.
///
/// Extraite en fonction libre pour être testable sans Tauri State.
/// `dest_dir` est paramétré pour les tests (dossier temporaire).
pub fn importer_source_impl(
    store: &Store,
    path: &str,
    dest_dir: &Path,
) -> Result<CustomSource, String> {
    // --- 1. Vérification du fichier source ---
    let src = Path::new(path);

    // Taille
    let meta =
        fs::metadata(src).map_err(|e| format!("impossible de lire le fichier source : {e}"))?;
    if meta.len() > MAX_SCRIPT_BYTES {
        return Err(format!(
            "script trop volumineux (max {} Ko, reçu {} Ko)",
            MAX_SCRIPT_BYTES / 1024,
            meta.len() / 1024
        ));
    }

    // --- 2. Validation du nom de fichier ---
    // On extrait uniquement le dernier composant ; s'il contient un séparateur
    // ou `..`, c'est une tentative de traversal.
    let filename = src
        .file_name()
        .ok_or_else(|| "chemin source sans nom de fichier".to_string())?;
    let filename_str = filename
        .to_str()
        .ok_or_else(|| "nom de fichier non UTF-8".to_string())?;

    // Le nom seul (après file_name()) ne doit pas contenir de séparateur ni `..`
    if filename_str.contains('/') || filename_str.contains('\\') || filename_str == ".." {
        return Err("nom de fichier interdit (séparateur ou traversal)".to_string());
    }
    // Doit se terminer par `.js`
    if !filename_str.ends_with(".js") {
        return Err("le fichier doit être un script .js".to_string());
    }

    // id = nom sans l'extension `.js`
    let id = filename_str
        .strip_suffix(".js")
        .expect("on vient de vérifier .ends_with('.js')")
        .to_string();
    if id.is_empty() {
        return Err("nom de fichier vide avant l'extension".to_string());
    }

    // --- 3. Création du dossier de destination ---
    fs::create_dir_all(dest_dir)
        .map_err(|e| format!("impossible de créer le dossier de sources : {e}"))?;

    // --- 4. Vérification path-traversal sur la destination ---
    // On canonicalise `dest_dir` (préexistant après le create_dir_all) et on
    // reconstruit la destination finale. `dunce::canonicalize` évite le
    // préfixe `\\?\` sur Windows.
    let dest_dir_canon = dunce::canonicalize(dest_dir)
        .map_err(|e| format!("canonicalisation du dossier de destination impossible : {e}"))?;

    // On reconstruit la destination depuis le dossier canonicalisé + nom de
    // fichier validé (sans séparateur ni `..`) : c'est suffisant pour prévenir
    // la traversal sans devoir canonicaliser un fichier inexistant.
    let dest_canon = dest_dir_canon.join(filename_str);

    // Le parent de la destination doit être exactement dest_dir canonicalisé.
    if dest_canon.parent() != Some(dest_dir_canon.as_path()) {
        return Err("traversal de chemin détecté dans le nom de fichier".to_string());
    }

    // --- 5. Copie ---
    fs::copy(src, &dest_canon).map_err(|e| format!("copie du script impossible : {e}"))?;

    let dest_str = dest_canon
        .to_str()
        .ok_or_else(|| "chemin de destination non UTF-8".to_string())?
        .to_string();

    // --- 6. Enregistrement en base ---
    // `lang` est fixé à "fr" par défaut ; l'interface pourra l'exposer en S2.
    // TODO S2 : ajouter un paramètre `lang: Option<String>` à la commande
    // `importer_source` et le propager ici. Sans ce paramètre, toutes les
    // sources personnalisées importées avant S2 auront `lang = "fr"` en base.
    // NOTE SÉCURITÉ (résolue le 2026-10-26) : la commande `importer_source`
    // prenait un **chemin** en texte, donc `invoke('importer_source', { path })`
    // était possible depuis la webview. Elle ouvre désormais le sélecteur natif
    // elle-même et ne reçoit plus aucun chemin de la page : cf. sa
    // documentation, et `ScriptedParser` pour ce que le moteur protège (et ne
    // protège pas) une fois le script chargé.
    // Le filtrage sur l'extension `.js` et l'anti-traversal portent jusqu'ici.
    store.add_custom_source(&id, &id, "fr", &dest_str)?;

    // --- 7. Lecture du résultat depuis la base ---
    store.list_custom_sources().and_then(|mut liste| {
        liste
            .drain(..)
            .find(|s| s.id == id)
            .ok_or_else(|| "source introuvable après import".to_string())
    })
}

// ---------- Commandes Tauri ----------

/// Importe un script `.js` local comme source personnalisée.
///
/// Valide la taille, le nom, copie dans `%APPDATA%\Tatami\sources\` et
/// enregistre en base. Renvoie le `CustomSource` créé, ou `None` si
/// l'utilisateur annule le dialogue.
///
/// **Le dialogue est ouvert ici, pas par l'interface** : aucun chemin ne
/// traverse l'IPC. L'ancienne signature prenait `path: String`, donc
/// `invoke('importer_source', { path: "C:\\…\\x.js" })` suffisait depuis la
/// webview — une compromission du rendu pouvait faire exécuter au moteur
/// n'importe quel `.js` du disque. Le moteur évalue ces scripts dans Nashorn
/// avec `--no-java` (barrière principale) et une liste noire de globaux, mais
/// `HttpBridge` documente lui-même que « le sandbox des scripts reste
/// contournable » : le choix du fichier ne doit donc pas être une chaîne
/// fournie par la page. Ici, la seule action possible est celle que
/// l'utilisateur voit et confirme dans le sélecteur natif.
#[tauri::command]
pub async fn importer_source(
    app: tauri::AppHandle,
    store: State<'_, Arc<Store>>,
) -> Result<Option<CustomSource>, String> {
    use tauri_plugin_dialog::DialogExt;
    // `blocking_pick_file` n'est pas appelé sur le fil principal : la macro du
    // plugin poste la boîte sur ce fil et attend la réponse sur un canal.
    let choisi = app
        .dialog()
        .file()
        .set_title("Importer une source (.js)")
        .add_filter("JavaScript", &["js"])
        .blocking_pick_file();
    let Some(chemin) = choisi else {
        return Ok(None);
    };
    let chemin = chemin
        .into_path()
        .map_err(|e| format!("chemin choisi illisible : {e}"))?;
    let chemin = chemin
        .to_str()
        .ok_or_else(|| "chemin choisi non UTF-8".to_string())?;
    importer_source_impl(&store, chemin, &sources_dir()).map(Some)
}

/// Liste les sources personnalisées installées.
#[tauri::command]
pub async fn lister_sources_custom(
    store: State<'_, Arc<Store>>,
) -> Result<Vec<CustomSource>, String> {
    store.list_custom_sources()
}

/// Supprime une source personnalisée : retire le fichier `.js` et la ligne en
/// base.
///
/// L'absence du fichier sur le disque n'est pas une erreur : l'utilisateur
/// peut l'avoir déplacé manuellement.
#[tauri::command]
pub async fn supprimer_source_custom(
    store: State<'_, Arc<Store>>,
    id: String,
) -> Result<(), String> {
    supprimer_source_custom_impl(&store, &id, &sources_dir())
}

fn supprimer_source_custom_impl(
    store: &Store,
    id: &str,
    sources_root: &Path,
) -> Result<(), String> {
    if id.trim().is_empty() {
        return Err("PARSE_ERROR: id vide".to_string());
    }
    // Lire le chemin depuis la base pour trouver le fichier à supprimer.
    let liste = store.list_custom_sources()?;
    if let Some(src) = liste.iter().find(|s| s.id == id) {
        let p = Path::new(&src.path);
        if p.try_exists()
            .map_err(|e| format!("verification du fichier impossible : {e}"))?
        {
            let canonical_root = dunce::canonicalize(sources_root)
                .map_err(|e| format!("canonicalisation du dossier des sources impossible : {e}"))?;
            let canonical_path = dunce::canonicalize(p)
                .map_err(|e| format!("canonicalisation du script impossible : {e}"))?;
            if !canonical_path.starts_with(&canonical_root) {
                return Err("refus : hors du dossier des sources".to_string());
            }
            fs::remove_file(&canonical_path)
                .map_err(|e| format!("suppression du fichier impossible : {e}"))?;
        }
    }
    store.remove_custom_source(id)?;
    // Si la source venait du bundle, sa suppression doit être définitive : le
    // semis du prochain démarrage la verrait sinon comme « fichier manquant »
    // et la recopierait. `mark` ne touche qu'une ligne existante, donc une
    // source importée par l'utilisateur n'est pas concernée.
    if let Err(e) = store.mark_bundled_source_deleted(id) {
        tracing::warn!("source livrée {id} : pierre tombale impossible : {e}");
    }
    Ok(())
}

// ---------- Sources livrées avec l'application ----------

#[derive(Debug, Deserialize)]
struct ManifesteSources {
    sources: Vec<EntreeSourceEmbarquee>,
}

#[derive(Debug, Deserialize)]
struct EntreeSourceEmbarquee {
    fichier: String,
    #[serde(default)]
    lang: String,
    #[serde(default = "actif_par_defaut")]
    actif: bool,
}

fn actif_par_defaut() -> bool {
    true
}

/// Dossier des sources livrées : celui des ressources installées, avec un repli
/// sur l'arbre de développement (`cargo test`, `tauri dev` avant copie des
/// ressources).
fn dossier_sources_embarquees(resource_dir: Option<&Path>) -> Option<PathBuf> {
    let mut candidats = Vec::new();
    if let Some(rd) = resource_dir {
        candidats.push(rd.join("sources"));
    }
    candidats.push(
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("resources")
            .join("sources"),
    );
    candidats
        .into_iter()
        .find(|d| d.join("manifest.json").is_file())
}

/// Valide le nom d'un fichier livré. Rend l'identifiant (nom sans `.js`) si le
/// nom est sûr, `None` sinon : le manifeste est une donnée du paquet, mais une
/// entrée `../x.js` ne doit jamais pouvoir écrire hors du dossier des sources.
fn identifiant_embarque(fichier: &str) -> Option<String> {
    if !fichier.ends_with(".js") || fichier.len() <= 3 {
        return None;
    }
    if fichier.contains('/')
        || fichier.contains('\\')
        || fichier.contains(':')
        || fichier.contains("..")
    {
        return None;
    }
    fichier
        .strip_suffix(".js")
        .filter(|id| !id.is_empty())
        .map(str::to_string)
}

fn empreinte(chemin: &Path) -> Result<String, String> {
    crate::moteur::empreinte_fichier(chemin)
        .map_err(|e| format!("empreinte de {} : {e}", chemin.display()))
}

/// Écrit un fichier par `tmp` + `rename`, comme le reste du dépôt : un arrêt
/// au milieu ne laisse jamais un script tronqué que le moteur lirait.
fn ecrire_atomique(dest: &Path, octets: &[u8]) -> Result<(), String> {
    let tmp = dest.with_extension("js.tmp");
    fs::write(&tmp, octets).map_err(|e| format!("écriture de {} : {e}", tmp.display()))?;
    if dest.exists() {
        fs::remove_file(dest).map_err(|e| format!("remplacement de {} : {e}", dest.display()))?;
    }
    fs::rename(&tmp, dest).map_err(|e| format!("renommage de {} : {e}", tmp.display()))
}

/// Recopie les sources livrées dans le dossier que lit le moteur.
///
/// Règles, toutes dictées par un défaut mesuré :
/// - un fichier **modifié** par l'utilisateur n'est jamais écrasé (on compare
///   l'empreinte présente à celle que nous avions écrite) ;
/// - une source **supprimée** par l'utilisateur ne revient pas (pierre tombale
///   en base, posée par `supprimer_source_custom`) ;
/// - une version livrée **plus récente** remplace une copie restée intacte,
///   sans quoi un correctif de parseur ne partirait jamais dans une mise à jour ;
/// - `actif: false` désactive la source **à la première rencontre** seulement :
///   un utilisateur qui la réactive ensuite n'est pas contredit au démarrage
///   suivant.
///
/// Rend le nombre de fichiers écrits (copies et mises à jour).
pub fn semer_sources_embarquees(
    store: &Store,
    resource_dir: Option<&Path>,
) -> Result<usize, String> {
    semer_sources_dans(store, resource_dir, &sources_dir())
}

/// Même semis, avec le dossier de destination paramétré : les tests écrivent
/// dans un dossier temporaire, jamais dans les sources réelles de la machine
/// (leçon AGENTS.md §14 : `cargo test` écrivait dans la bibliothèque de
/// l'utilisateur).
fn semer_sources_dans(
    store: &Store,
    resource_dir: Option<&Path>,
    dest_dir: &Path,
) -> Result<usize, String> {
    let Some(src_dir) = dossier_sources_embarquees(resource_dir) else {
        tracing::info!("sources embarquées : aucun manifeste, rien à semer");
        return Ok(0);
    };
    let manifeste = fs::read_to_string(src_dir.join("manifest.json"))
        .map_err(|e| format!("lecture du manifeste des sources : {e}"))?;
    let manifeste: ManifesteSources = serde_json::from_str(&manifeste)
        .map_err(|e| format!("manifeste des sources invalide : {e}"))?;

    fs::create_dir_all(dest_dir).map_err(|e| format!("création du dossier des sources : {e}"))?;

    let mut ecrits = 0usize;
    for entree in manifeste.sources {
        let Some(id) = identifiant_embarque(&entree.fichier) else {
            tracing::warn!(
                "manifeste des sources : nom de fichier refusé ({:?})",
                entree.fichier
            );
            continue;
        };
        let src = src_dir.join(&entree.fichier);
        if !src.is_file() {
            tracing::warn!("source embarquée {id} : fichier absent du paquet");
            continue;
        }
        let empreinte_livree = match empreinte(&src) {
            Ok(h) => h,
            Err(e) => {
                tracing::warn!("{e}");
                continue;
            }
        };
        let etat = match store.bundled_source_state(&id) {
            Ok(e) => e,
            Err(e) => {
                tracing::warn!("source embarquée {id} : état illisible : {e}");
                continue;
            }
        };
        if etat.as_ref().is_some_and(|e| e.deleted) {
            tracing::info!("source embarquée {id} : supprimée par l'utilisateur, non recopiée");
            continue;
        }

        let dest = dest_dir.join(&entree.fichier);
        let existait = dest.is_file();
        let mut action = "inchangée";
        let mut ecrit = false;

        if existait {
            let empreinte_presente = match empreinte(&dest) {
                Ok(h) => h,
                Err(e) => {
                    tracing::warn!("{e}");
                    continue;
                }
            };
            // « Intacte » veut dire : identique à ce que NOUS avions écrit.
            // Une base migrée n'a pas encore d'état : on laisse alors le
            // fichier tel quel (il peut être une version utilisateur), et on
            // mémorise seulement l'empreinte livrée.
            let intacte = etat
                .as_ref()
                .is_some_and(|e| e.sha256 == empreinte_presente);
            if intacte && empreinte_presente != empreinte_livree {
                let octets = match fs::read(&src) {
                    Ok(o) => o,
                    Err(e) => {
                        tracing::warn!("lecture de {} : {e}", src.display());
                        continue;
                    }
                };
                if let Err(e) = ecrire_atomique(&dest, &octets) {
                    tracing::warn!("{e}");
                    continue;
                }
                action = "mise à jour";
                ecrit = true;
            } else if !intacte {
                action = "conservée (modifiée par l'utilisateur)";
            }
        } else {
            let octets = match fs::read(&src) {
                Ok(o) => o,
                Err(e) => {
                    tracing::warn!("lecture de {} : {e}", src.display());
                    continue;
                }
            };
            if let Err(e) = ecrire_atomique(&dest, &octets) {
                tracing::warn!("{e}");
                continue;
            }
            action = "copiée";
            ecrit = true;
        }

        let Some(dest_str) = dest.to_str() else {
            tracing::warn!("source embarquée {id} : chemin de destination non UTF-8");
            continue;
        };
        let lang = if entree.lang.trim().is_empty() {
            "fr"
        } else {
            entree.lang.as_str()
        };
        if let Err(e) = store.ensure_custom_source(&id, &id, lang, dest_str) {
            tracing::warn!("source embarquée {id} : enregistrement impossible : {e}");
            continue;
        }
        // Désactivation par défaut, une seule fois : à la première rencontre
        // de cette entrée (installation neuve OU mise à jour d'une base qui
        // n'a pas encore d'état). Réactivée par l'utilisateur, elle reste
        // active aux démarrages suivants puisque l'état existe.
        if etat.is_none() && !entree.actif {
            let cle = format!("custom:{id}");
            match store.set_source_enabled(&cle, false) {
                Ok(()) => tracing::info!("source embarquée {id} : désactivée par défaut"),
                Err(e) => tracing::warn!("source embarquée {id} : désactivation : {e}"),
            }
        }
        if let Err(e) = store.record_bundled_source(&id, &empreinte_livree, false) {
            tracing::warn!("source embarquée {id} : état non mémorisé : {e}");
        }
        if ecrit {
            ecrits += 1;
        }
        tracing::info!("source embarquée {id} : {action}");
    }
    Ok(ecrits)
}

// ---------- Tests ----------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::Store;
    use std::io::Write;
    use std::sync::atomic::{AtomicU64, Ordering};

    static COMPTEUR: AtomicU64 = AtomicU64::new(0);

    fn store_memoire() -> Store {
        Store::open_in_memory().expect("base en mémoire")
    }

    /// Garde RAII : efface le dossier temporaire à la fin du test, même en
    /// cas de panic.
    struct DossierGarde(PathBuf);
    impl Drop for DossierGarde {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    /// Crée un fichier `.js` de `taille` octets dans un dossier temporaire
    /// unique (pid + compteur atomique). Renvoie le chemin du fichier et un
    /// garde RAII qui nettoie le dossier.
    fn tmp_js(nom: &str, taille: usize) -> (PathBuf, DossierGarde) {
        let dir = std::env::temp_dir().join(format!(
            "tatami-cs-test-{}-{}-{}",
            std::process::id(),
            COMPTEUR.fetch_add(1, Ordering::Relaxed),
            nom.replace('.', "_")
        ));
        fs::create_dir_all(&dir).unwrap();
        let p = dir.join(nom);
        let mut f = fs::File::create(&p).unwrap();
        f.write_all(&vec![b'x'; taille]).unwrap();
        (p, DossierGarde(dir))
    }

    #[test]
    fn importer_refuse_trop_gros() {
        let store = store_memoire();
        let dest = std::env::temp_dir().join(format!(
            "tatami-cs-dest-{}-{}-trop-gros",
            std::process::id(),
            COMPTEUR.fetch_add(1, Ordering::Relaxed)
        ));
        let (p, _garde) = tmp_js("source.js", (MAX_SCRIPT_BYTES + 1) as usize);
        let err = importer_source_impl(&store, p.to_str().unwrap(), &dest).unwrap_err();
        assert!(err.contains("trop volumineux"), "message inattendu : {err}");
        let _ = fs::remove_dir_all(&dest);
    }

    #[test]
    fn importer_refuse_traversee() {
        let store = store_memoire();
        let dest = std::env::temp_dir().join(format!(
            "tatami-cs-dest-{}-{}-traversee",
            std::process::id(),
            COMPTEUR.fetch_add(1, Ordering::Relaxed)
        ));
        // Chemin inexistant dont le nom de fichier (vu par file_name()) est
        // `evil.js` — mais le chemin contient `..`. La validation tombe sur
        // l'ouverture de metadata (fichier inexistant), ce qui constitue bien
        // un refus.
        #[cfg(windows)]
        let err = importer_source_impl(&store, r"C:\legitimate\..\..\evil.js", &dest).unwrap_err();
        #[cfg(not(windows))]
        let err = importer_source_impl(&store, "/legitimate/../../evil.js", &dest).unwrap_err();
        assert!(
            !err.is_empty(),
            "un chemin hostile doit être rejeté : {err}"
        );
        let _ = fs::remove_dir_all(&dest);
    }

    /// Complémentaire à `importer_refuse_traversee` : exerce la logique
    /// anti-traversal avec un fichier `.js` **réellement existant** dont le
    /// chemin source remonte via `..`.
    ///
    /// Ce test couvre le vecteur noté dans la review (finding 2) : un
    /// attaquant fournirait un chemin qui remonte (`dir_a/../dir_a/real.js`)
    /// pour pointer vers un fichier existant, dans l'espoir que la copie
    /// atterrisse hors de `dest_dir`. Le code extrait le nom via
    /// `Path::file_name()` (section 2) puis reconstruit la destination à
    /// partir de `dest_dir_canon.join(filename)` (section 4) — donc la copie
    /// atterrit toujours dans `dest_dir`, jamais ailleurs.
    ///
    /// Ce test prouve que l'import réussit (le fichier est légitime) et que
    /// la copie est dans `dest_dir`, pas à l'endroit désigné par le `..`.
    #[test]
    fn importer_chemin_avec_remonter_copie_dans_dest_dir() {
        let store = store_memoire();
        // Créer un vrai fichier .js.
        let (src, _garde_src) = tmp_js("real.js", 64);
        let dest_dir = std::env::temp_dir().join(format!(
            "tatami-cs-dest-{}-{}-avec-remonter",
            std::process::id(),
            COMPTEUR.fetch_add(1, Ordering::Relaxed)
        ));

        // Construire un chemin avec `..` qui pointe vers le même fichier.
        // `src` est dans `tatami-cs-test-PID-N-real_js/real.js` ; on peut
        // passer `tatami-cs-test-PID-N-real_js/../tatami-cs-test-PID-N-real_js/real.js`.
        let src_parent = src.parent().unwrap();
        let src_avec_remonter = src_parent
            .join("..")
            .join(src_parent.file_name().unwrap())
            .join("real.js");

        let cs = importer_source_impl(&store, src_avec_remonter.to_str().unwrap(), &dest_dir)
            .expect("un fichier .js existant doit s'importer même si le chemin source contient ..");

        // La copie doit être dans dest_dir, pas ailleurs.
        assert_eq!(cs.id, "real");
        assert!(
            dest_dir.join("real.js").exists(),
            "le fichier doit être copié dans dest_dir"
        );
        let _ = fs::remove_dir_all(&dest_dir);
    }

    #[test]
    fn importer_valide_copie_et_enregistre() {
        let store = store_memoire();
        let dest_dir = std::env::temp_dir().join(format!(
            "tatami-cs-dest-{}-{}-valide",
            std::process::id(),
            COMPTEUR.fetch_add(1, Ordering::Relaxed)
        ));
        let (src, _garde_src) = tmp_js("ma-source.js", 512);

        let cs = importer_source_impl(&store, src.to_str().unwrap(), &dest_dir)
            .expect("import d'un script valide doit réussir");

        assert_eq!(cs.id, "ma-source");
        assert_eq!(cs.lang, "fr");
        // Le fichier doit avoir été copié dans dest_dir.
        assert!(
            dest_dir.join("ma-source.js").exists(),
            "le fichier copié est introuvable dans dest_dir"
        );
        // La base doit contenir exactement une entrée.
        let liste = store.list_custom_sources().unwrap();
        assert_eq!(liste.len(), 1);
        assert_eq!(liste[0].id, "ma-source");

        let _ = fs::remove_dir_all(&dest_dir);
    }

    #[test]
    fn suppression_refuse_un_fichier_hors_du_dossier_des_sources() {
        let store = store_memoire();
        let (outside, _garde_fichier) = tmp_js("hors-sources.js", 16);
        let root = std::env::temp_dir().join(format!(
            "tatami-cs-sources-{}-{}",
            std::process::id(),
            COMPTEUR.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).unwrap();
        let _garde_root = DossierGarde(root.clone());
        store
            .add_custom_source(
                "hors-sources",
                "hors-sources",
                "fr",
                outside.to_str().unwrap(),
            )
            .unwrap();

        let resultat = supprimer_source_custom_impl(&store, "hors-sources", &root);
        assert!(
            resultat.is_err() && outside.is_file(),
            "la suppression doit refuser le chemin externe sans effacer le fichier: \
             resultat={resultat:?}, fichier_present={}",
            outside.is_file()
        );
        assert_eq!(
            store.list_custom_sources().unwrap().len(),
            1,
            "un refus de confinement doit conserver la ligne en base"
        );
    }

    #[test]
    fn suppression_d_un_fichier_deja_absent_efface_la_source_de_la_base() {
        let store = store_memoire();
        let (fichier, _garde) = tmp_js("deja-absent.js", 16);
        fs::remove_file(&fichier).unwrap();
        let root = fichier.parent().unwrap();
        store
            .add_custom_source("absente", "absente", "fr", fichier.to_str().unwrap())
            .unwrap();

        supprimer_source_custom_impl(&store, "absente", root).unwrap();
        assert!(store.list_custom_sources().unwrap().is_empty());
    }

    // ---------- Semis des sources livrées ----------

    /// Fabrique un faux paquet de ressources : `<racine>/sources/<fichiers>` +
    /// `manifest.json`. Le dossier est effacé par le garde.
    fn paquet(fichiers: &[(&str, &str)], manifeste: &str) -> (PathBuf, DossierGarde) {
        let racine = std::env::temp_dir().join(format!(
            "tatami-emb-{}-{}",
            std::process::id(),
            COMPTEUR.fetch_add(1, Ordering::Relaxed)
        ));
        let sources = racine.join("sources");
        fs::create_dir_all(&sources).unwrap();
        for (nom, contenu) in fichiers {
            fs::write(sources.join(nom), contenu).unwrap();
        }
        fs::write(sources.join("manifest.json"), manifeste).unwrap();
        (racine.clone(), DossierGarde(racine))
    }

    fn destination() -> (PathBuf, DossierGarde) {
        let racine = std::env::temp_dir().join(format!(
            "tatami-emb-dest-{}-{}",
            std::process::id(),
            COMPTEUR.fetch_add(1, Ordering::Relaxed)
        ));
        (racine.clone(), DossierGarde(racine))
    }

    #[test]
    fn semis_copie_les_sources_et_desactive_scan_manga_une_seule_fois() {
        let store = store_memoire();
        let (pack, _gpack) = paquet(
            &[("bon.js", "// bon"), ("scan-manga.js", "// scan")],
            r#"{"sources":[
                 {"fichier":"bon.js","lang":"fr","actif":true},
                 {"fichier":"scan-manga.js","lang":"fr","actif":false}
               ]}"#,
        );
        let (dest_root, _gdest) = destination();
        let dest = dest_root.join("sources");

        let n = semer_sources_dans(&store, Some(pack.as_path()), &dest).unwrap();
        assert_eq!(n, 2, "les deux fichiers doivent être copiés");
        assert!(dest.join("bon.js").is_file());
        assert!(dest.join("scan-manga.js").is_file());
        assert_eq!(store.list_custom_sources().unwrap().len(), 2);
        assert_eq!(
            store.disabled_sources().unwrap(),
            vec!["custom:scan-manga".to_string()],
            "le manifeste doit la désactiver à la première rencontre"
        );

        // L'utilisateur la réactive : le démarrage suivant ne doit pas la
        // re-désactiver (l'état de semis existe désormais).
        store.set_source_enabled("custom:scan-manga", true).unwrap();
        let n2 = semer_sources_dans(&store, Some(pack.as_path()), &dest).unwrap();
        assert_eq!(n2, 0, "aucune écriture au second démarrage");
        assert!(
            store.disabled_sources().unwrap().is_empty(),
            "un choix explicite de l'utilisateur ne doit pas être contredit"
        );
    }

    #[test]
    fn semis_preserve_un_fichier_modifie_et_met_a_jour_un_fichier_intact() {
        let store = store_memoire();
        let (pack, _gpack) = paquet(
            &[("modifie.js", "// v1"), ("intact.js", "// v1")],
            r#"{"sources":[
                 {"fichier":"modifie.js","lang":"fr","actif":true},
                 {"fichier":"intact.js","lang":"fr","actif":true}
               ]}"#,
        );
        let (dest_root, _gdest) = destination();
        let dest = dest_root.join("sources");
        semer_sources_dans(&store, Some(pack.as_path()), &dest).unwrap();

        fs::write(dest.join("modifie.js"), "// version utilisateur").unwrap();
        fs::write(pack.join("sources").join("modifie.js"), "// v2").unwrap();
        fs::write(pack.join("sources").join("intact.js"), "// v2").unwrap();

        let n = semer_sources_dans(&store, Some(pack.as_path()), &dest).unwrap();
        assert_eq!(n, 1, "seul le fichier intact doit être mis à jour");
        assert_eq!(
            fs::read_to_string(dest.join("modifie.js")).unwrap(),
            "// version utilisateur",
            "un fichier modifié par l'utilisateur ne doit jamais être écrasé"
        );
        assert_eq!(
            fs::read_to_string(dest.join("intact.js")).unwrap(),
            "// v2",
            "une copie intacte doit recevoir la version livrée plus récente"
        );
    }

    #[test]
    fn semis_ne_ressuscite_pas_une_source_supprimee() {
        let store = store_memoire();
        let (pack, _gpack) = paquet(
            &[("bon.js", "// bon")],
            r#"{"sources":[{"fichier":"bon.js","lang":"fr","actif":true}]}"#,
        );
        let (dest_root, _gdest) = destination();
        let dest = dest_root.join("sources");
        semer_sources_dans(&store, Some(pack.as_path()), &dest).unwrap();

        supprimer_source_custom_impl(&store, "bon", &dest).unwrap();
        assert!(!dest.join("bon.js").exists());
        assert!(store.list_custom_sources().unwrap().is_empty());

        let n = semer_sources_dans(&store, Some(pack.as_path()), &dest).unwrap();
        assert_eq!(n, 0);
        assert!(
            !dest.join("bon.js").exists(),
            "une source supprimée par l'utilisateur ne doit pas revenir"
        );
        assert!(store.list_custom_sources().unwrap().is_empty());
    }

    #[test]
    fn semis_preserve_un_fichier_preexistant_sans_etat() {
        // Cas de la mise à jour d'une installation existante : le fichier est
        // là, la base n'a pas encore d'état de semis. On ne peut pas savoir
        // s'il vient de nous ou de l'utilisateur, donc on ne l'écrase pas.
        let store = store_memoire();
        let (pack, _gpack) = paquet(
            &[("bon.js", "// officiel")],
            r#"{"sources":[{"fichier":"bon.js","lang":"fr","actif":true}]}"#,
        );
        let (dest_root, _gdest) = destination();
        let dest = dest_root.join("sources");
        fs::create_dir_all(&dest).unwrap();
        fs::write(dest.join("bon.js"), "// deja la").unwrap();

        let n = semer_sources_dans(&store, Some(pack.as_path()), &dest).unwrap();
        assert_eq!(n, 0, "aucune copie : le fichier préexiste");
        assert_eq!(
            fs::read_to_string(dest.join("bon.js")).unwrap(),
            "// deja la"
        );
        assert_eq!(store.list_custom_sources().unwrap().len(), 1);
    }

    #[test]
    fn semis_refuse_un_nom_de_fichier_hors_du_dossier() {
        let store = store_memoire();
        let (pack, _gpack) = paquet(
            &[("bon.js", "// bon")],
            r#"{"sources":[
                 {"fichier":"../evil.js","lang":"fr","actif":true},
                 {"fichier":"bon.js","lang":"fr","actif":true}
               ]}"#,
        );
        let (dest_root, _gdest) = destination();
        let dest = dest_root.join("sources");
        let n = semer_sources_dans(&store, Some(pack.as_path()), &dest).unwrap();
        assert_eq!(n, 1, "seule l'entrée valide est copiée");
        assert!(!dest_root.join("evil.js").exists());
        assert!(!dest.join("evil.js").exists());
        assert_eq!(store.list_custom_sources().unwrap().len(), 1);
    }
}
