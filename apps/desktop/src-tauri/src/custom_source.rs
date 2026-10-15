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
    // NOTE SÉCURITÉ (À VÉRIFIER, tracé dans PLAN.md) : `importer_source`
    // prend un **chemin** en texte. Rien dans la chaîne ne prouve que ce
    // chemin vienne d'un vrai dialogue natif `tauri-plugin-dialog` : l'appel
    // `invoke('importer_source', { path })` est possible depuis la webview.
    // CA N'EST PAS ENCORE EXPOSÉ — la recherche dans `apps/desktop/app/` du
    // 2026-10-15 ne trouve aucune invocation — mais la surface existe et la
    // `Capability` fait confiance au webview. Au moment de brancher S2, exiger
    // un *handle* de dialogue (ou restreindre le chemin aux dossiers
    // utilisateur documentés par l'utilisateur lui-même) plutôt qu'une chaîne.
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
/// enregistre en base. Renvoie le `CustomSource` créé.
#[tauri::command]
pub async fn importer_source(
    store: State<'_, Arc<Store>>,
    path: String,
) -> Result<CustomSource, String> {
    let dest_dir = sources_dir();
    importer_source_impl(&store, &path, &dest_dir)
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
    store.remove_custom_source(id)
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
}
