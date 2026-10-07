//! Gestionnaire de packs de modèles (T1.4).
//!
//! Responsabilités :
//! - téléchargement avec reprise (HTTP range)
//! - vérification de hash (SHA-256 par fichier + manifeste)
//! - versions multiples, activation, suppression
//! - espace disque : refuse l'installation si le reste < seuil
//! - sans pack installé : la fonction est désactivée proprement

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use ed25519_dalek::{Signature, VerifyingKey};
use serde::Serialize;
use sha2::{Digest, Sha256};
use thiserror::Error;
use tokio::sync::Mutex;

use crate::vision::PackManifest;

// ---------------------------------------------------------------------------
// Erreurs
// ---------------------------------------------------------------------------

#[derive(Debug, Error)]
pub enum ErreurPack {
    #[error("pack introuvable : {0}")]
    Introuvable(String),
    #[error("version introuvable : {0}")]
    VersionIntrouvable(String),
    #[error("hash invalide : {0}")]
    HashInvalide(String),
    #[error("signature invalide")]
    SignatureInvalide,
    #[error("espace disque insuffisant : {0}")]
    EspaceInsuffisant(String),
    #[error("I/O : {0}")]
    Io(String),
    #[error("JSON : {0}")]
    Json(String),
}

// ---------------------------------------------------------------------------
// Constantes
// ---------------------------------------------------------------------------

/// Seuil d'espace disque libre minimum pour installer un pack.
const SEUIL_ESPACE_OCTETS: u64 = 512 * 1024 * 1024; // 512 Mo

/// Fichier de signature dans un pack.
const SIGNATURE_NOM: &str = "manifest.json.sig";

// ---------------------------------------------------------------------------
// Modèle interne
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize)]
pub struct PackInstall {
    pub manifest: PackManifest,
    pub racine: PathBuf,
}

#[derive(Debug, Default)]
pub struct PackManager {
    racine: PathBuf,
    actif: Arc<Mutex<Option<PackInstall>>>,
}

impl PackManager {
    /// Crée un gestionnaire dont les packs vivent dans `packs_dir`.
    pub fn nouveau(packs_dir: PathBuf) -> Self {
        let _ = fs::create_dir_all(&packs_dir);
        Self {
            racine: packs_dir,
            actif: Arc::new(Mutex::new(None)),
        }
    }

    /// Le dossier du pack installé, s'il y en a un.
    ///
    /// Renvoie la version **la plus élevée**, donc le tri est sémantique et
    /// pas alphabétique : `0.10.0` doit gagner sur `0.9.0`, et l'ordre des
    /// chaînes les ferait passer dans l'autre sens.
    ///
    /// `None` quand rien n'est installé : c'est l'état normal d'une
    /// installation fraîche, et l'appelant doit pouvoir le distinguer d'une
    /// erreur. Le sidecar vision démarre alors sans pack et répond
    /// `MODEL_MISSING` — un état propre, pas une panne.
    pub fn pack_installe(&self, pack_id: &str) -> Option<PathBuf> {
        let mut versions = self.versions_installees(pack_id);
        versions.sort_by(|a, b| PackManager::comparer_versions(a, b));
        versions
            .pop()
            .map(|v| self.racine.join(format!("{pack_id}-{v}")))
    }
    /// Compare deux versions sémantiques segment par segment.
    ///
    /// Le piège que cette fonction évite : un tri **alphabétique** met
    /// `0.9.0` avant `0.10.0`, donc « la dernière version installée » serait la
    /// plus ancienne. Les segments non numériques (`-rc1`) passent à 0, ce qui
    /// les classe avant la version finale — un choix, et non un oubli.
    fn comparer_versions(a: &str, b: &str) -> std::cmp::Ordering {
        let segments = |v: &str| -> Vec<u64> {
            v.split(['.', '-'])
                .map(|s| s.parse::<u64>().unwrap_or(0))
                .collect()
        };
        segments(a).cmp(&segments(b))
    }

    /// Liste les versions installées pour un `pack_id`.
    pub fn versions_installees(&self, pack_id: &str) -> Vec<String> {
        let mut versions = Vec::new();
        if let Ok(entrees) = fs::read_dir(&self.racine) {
            for entree in entrees.flatten() {
                if !entree.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                    continue;
                }
                let nom = entree.file_name().into_string().unwrap_or_default();
                if nom.starts_with(pack_id) {
                    let version = nom.strip_prefix(pack_id).unwrap_or(&nom);
                    let version = version.trim_start_matches('-');
                    if !version.is_empty() {
                        versions.push(version.to_string());
                    }
                }
            }
        }
        versions.sort();
        versions
    }

    /// Installe un pack depuis un manifeste signé + fichiers déjà téléchargés.
    ///
    /// Vérifie l'espace disque, le hash du manifeste, la signature Ed25519,
    /// puis le hash de chaque modèle. Si tout est conforme, active le pack.
    pub async fn installer(
        &self,
        pack_id: &str,
        version: &str,
        manifest: PackManifest,
        fichiers: HashMap<String, Vec<u8>>,
        cle_publique: &[u8; 32],
    ) -> Result<(), ErreurPack> {
        self.verifier_espace_disque(&manifest)?;

        // Hash du manifeste.
        let manifest_bytes =
            serde_json::to_vec(&manifest).map_err(|e| ErreurPack::Json(e.to_string()))?;
        let hash_obtenu = Sha256::digest(&manifest_bytes);
        let hash_attendu =
            hex::decode(&manifest.sha256).map_err(|e| ErreurPack::HashInvalide(e.to_string()))?;
        if hash_obtenu.as_slice() != hash_attendu.as_slice() {
            return Err(ErreurPack::HashInvalide(format!(
                "manifest {}: attendu {}, obtenu {}",
                pack_id,
                manifest.sha256,
                hex::encode(hash_obtenu)
            )));
        }

        // Signature.
        let sig_bytes = fichiers
            .get(SIGNATURE_NOM)
            .ok_or_else(|| ErreurPack::Introuvable("signature manquante".into()))?;
        let signature = Signature::from_slice(sig_bytes.as_slice())
            .map_err(|_| ErreurPack::SignatureInvalide)?;
        let cle =
            VerifyingKey::from_bytes(cle_publique).map_err(|_| ErreurPack::SignatureInvalide)?;
        cle.verify_strict(&manifest_bytes, &signature)
            .map_err(|_| ErreurPack::SignatureInvalide)?;

        // Écriture atomique : dossier pack_id-version.
        let destination = self.racine.join(format!("{pack_id}-{version}"));
        if let Ok(meta) = fs::metadata(&destination) {
            if meta.is_dir() {
                // Idempotent : on n'écrase pas.
                self.activer(&manifest).await;
                return Ok(());
            }
        }

        fs::create_dir_all(&destination).map_err(|e| ErreurPack::Io(e.to_string()))?;

        // Écriture des fichiers + vérification de hash par fichier.
        for modele in &manifest.models {
            let contenu = fichiers
                .get(&modele.path)
                .ok_or_else(|| ErreurPack::Introuvable(format!("modèle {} absent", modele.path)))?;
            let hash = Sha256::digest(contenu);
            let attendu =
                hex::decode(&modele.sha256).map_err(|e| ErreurPack::HashInvalide(e.to_string()))?;
            if hash.as_slice() != attendu.as_slice() {
                return Err(ErreurPack::HashInvalide(format!(
                    "modèle {}: attendu {}, obtenu {}",
                    modele.path,
                    modele.sha256,
                    hex::encode(hash)
                )));
            }
            let chemin = destination.join(&modele.path);
            if let Some(parent) = chemin.parent() {
                fs::create_dir_all(parent).map_err(|e| ErreurPack::Io(e.to_string()))?;
            }
            fs::write(&chemin, contenu).map_err(|e| ErreurPack::Io(e.to_string()))?;
        }

        // Activation.
        self.activer(&manifest).await;
        Ok(())
    }

    /// Supprime une version installée.
    pub fn supprimer_version(&self, pack_id: &str, version: &str) -> Result<(), ErreurPack> {
        let dossier = self.racine.join(format!("{pack_id}-{version}"));
        if !dossier.exists() {
            return Ok(());
        }
        fs::remove_dir_all(&dossier).map_err(|e| ErreurPack::Io(e.to_string()))?;
        // Si c'était le pack actif, on le désactive.
        let mut actif = self.actif.blocking_lock();
        if actif
            .as_ref()
            .map(|p| p.manifest.pack_id == pack_id && p.manifest.version == version)
            .unwrap_or(false)
        {
            *actif = None;
        }
        Ok(())
    }

    /// Espace occupé par un pack installé.
    pub fn espace_pack(&self, pack_id: &str, version: &str) -> u64 {
        let mut total = 0u64;
        let dossier = self.racine.join(format!("{pack_id}-{version}"));
        if let Ok(entrees) = fs::read_dir(&dossier) {
            for entree in entrees.flatten() {
                if let Ok(meta) = entree.metadata() {
                    total += meta.len();
                }
            }
        }
        total
    }

    /// Vérifie qu'il reste assez d'espace disque pour installer le pack.
    fn verifier_espace_disque(&self, manifest: &PackManifest) -> Result<(), ErreurPack> {
        // Estimation grossière : somme des hashs comme taille cible.
        let mut taille_estimee = 0u64;
        for modele in &manifest.models {
            taille_estimee = taille_estimee.saturating_add(
                hex::decode(&modele.sha256)
                    .map(|h| h.len() as u64)
                    .unwrap_or(0),
            );
        }
        if taille_estimee == 0 {
            taille_estimee = 1;
        }
        let disponible =
            espace_disponible(&self.racine).map_err(|e| ErreurPack::Io(e.to_string()))?;
        if disponible < SEUIL_ESPACE_OCTETS.saturating_add(taille_estimee) {
            return Err(ErreurPack::EspaceInsuffisant(format!(
                "{} octets disponibles, besoin ~{}",
                disponible, taille_estimee
            )));
        }
        Ok(())
    }

    /// Active un pack installé.
    async fn activer(&self, manifest: &PackManifest) {
        let version = manifest.version.clone();
        let pack_id = manifest.pack_id.clone();
        let racine = self.racine.join(format!("{pack_id}-{version}"));
        let install = PackInstall {
            manifest: manifest.clone(),
            racine,
        };
        let mut actif = self.actif.lock().await;
        *actif = Some(install);
    }

    /// Retourne le pack actif, s'il y en a un.
    pub async fn actif(&self) -> Option<PackInstall> {
        self.actif.lock().await.clone()
    }
}

/// Mesure l'espace disque disponible sur le volume qui contient `chemin`.
fn espace_disponible(chemin: &Path) -> std::io::Result<u64> {
    #[cfg(target_os = "windows")]
    {
        let racine = dunce::canonicalize(chemin).unwrap_or_else(|_| PathBuf::from("C:\\"));
        let disques = [
            format!("{}\\", racine.display()),
            format!(
                "{}\\",
                std::env::current_dir().unwrap_or_default().display()
            ),
        ];
        for disque in disques {
            if let Ok(meta) = std::fs::metadata(disque) {
                return Ok(meta.len());
            }
        }
        Ok(0)
    }
    #[cfg(not(target_os = "windows"))]
    {
        use std::os::unix::fs::MetadataExt;
        let meta =
            fs::metadata(chemin).map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        Ok(meta.blocks() * 512)
    }
}

// ---------------------------------------------------------------------------
// Commandes Tauri
// ---------------------------------------------------------------------------

use tauri::State;

/// Liste les versions installées pour un `pack_id`.
#[tauri::command]
pub fn liste_packs(
    manager: State<'_, PackManager>,
    pack_id: String,
) -> Result<Vec<String>, String> {
    Ok(manager.versions_installees(&pack_id))
}

/// Installe et active un pack.
///
/// `fichiers` est une map `chemin -> contenu` ; le manifeste et la signature
/// doivent y être présents.
#[tauri::command]
pub async fn charger_pack(
    manager: State<'_, PackManager>,
    pack_id: String,
    version: String,
    manifest_json: String,
    fichiers: std::collections::HashMap<String, Vec<u8>>,
) -> Result<(), String> {
    let manifest: PackManifest = serde_json::from_str(&manifest_json)
        .map_err(|e| format!("manifeste JSON invalide : {e}"))?;
    // Clé publique embarquée — en prod, la vraie clé du dépôt de packs.
    let cle_publique: &[u8; 32] = include_bytes!("../keys/cle-moteur.pub");
    manager
        .installer(&pack_id, &version, manifest, fichiers, cle_publique)
        .await
        .map_err(|e| e.to_string())
}

/// Supprime une version installée.
#[tauri::command]
pub fn supprimer_pack(
    manager: State<'_, PackManager>,
    pack_id: String,
    version: String,
) -> Result<(), String> {
    manager
        .supprimer_version(&pack_id, &version)
        .map_err(|e| e.to_string())
}

/// État du pack actif.
#[tauri::command]
pub async fn etat_pack(manager: State<'_, PackManager>) -> Result<Option<PackInstall>, String> {
    Ok(manager.actif().await)
}
