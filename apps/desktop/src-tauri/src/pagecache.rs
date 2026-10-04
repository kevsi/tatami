//! Protocole `tatami://` (Windows : `http://tatami.localhost/`).
//!
//! index source -> moteur `fetchImage` -> cache disque -> webview ne parle qu'aux
//! IDs connus (jamais une URL arbitraire). ADR transporteur ne le couvre pas :
//! images en base64 (protocole v0) seraient mieux en fichiers streamés plus tard.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde_json::Value;

/// Ce que une page du lecteur désigne.
///
/// Deux natures, et c'est tout le changement : jusqu'ici le lecteur passait
/// **toujours** par le moteur, y compris pour un chapitre téléchargé. Un `.cbz`
/// écrit par Tatami était donc illisible par Tatami — le seul moyen de le lire
/// était de l'ouvrir dans un autre logiciel. Une entrée locale se sert
/// directement du fichier, sans moteur ni réseau : c'est ce que rend la lecture
/// hors ligne (tâche 2.3).
#[derive(Clone, Debug)]
pub enum PageRef {
    /// Page d'une source distante : le moteur va la chercher.
    Moteur { source: String, page_url: String },
    /// Page d'un `.cbz` déjà téléchargé : lue dans le fichier.
    Local {
        cbz: PathBuf,
        /// Rang de l'entrée dans le répertoire central, et non son nom.
        entry: usize,
    },
}

pub struct PageCache {
    map: Mutex<HashMap<String, PageRef>>,
    dir: PathBuf,
}

impl PageCache {
    pub fn new(dir: PathBuf) -> Arc<Self> {
        let _ = std::fs::create_dir_all(&dir);
        Arc::new(Self {
            map: Mutex::new(HashMap::new()),
            dir,
        })
    }

    pub fn register(&self, source: &str, page_url: &str) -> String {
        let mut map = self.map.lock().unwrap_or_else(|e| e.into_inner());
        for (tok, p) in map.iter() {
            if let PageRef::Moteur {
                source: s,
                page_url: u,
            } = p
            {
                if u == page_url && s == source {
                    return tok.clone();
                }
            }
        }
        let token = format!(
            "{:016x}{:016x}",
            seed_a(source, page_url),
            seed_b(source, page_url)
        );
        map.insert(
            token.clone(),
            PageRef::Moteur {
                source: source.to_string(),
                page_url: page_url.to_string(),
            },
        );
        token
    }

    /// Enregistre une page d'un `.cbz` local et renvoie son jeton.
    ///
    /// Le jeton est dérivé du chemin et du rang : rejouer la même liste rend les
    /// mêmes jetons, donc un chapitre rouvert ne crée pas de nouvelles entrées.
    pub fn register_local(&self, cbz: &Path, entry: usize) -> String {
        let chemin = cbz.to_string_lossy().to_string();
        let token = format!(
            "l{:016x}{:016x}",
            seed_a("local", &chemin),
            seed_b(&format!("{entry}"), &chemin)
        );
        let mut map = self.map.lock().unwrap_or_else(|e| e.into_inner());
        map.insert(
            token.clone(),
            PageRef::Local {
                cbz: cbz.to_path_buf(),
                entry,
            },
        );
        token
    }

    pub fn resolve(&self, token: &str) -> Option<PageRef> {
        self.map
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(token)
            .cloned()
    }

    fn cache_path(&self, token: &str) -> PathBuf {
        self.dir.join(format!("page-{token}.bin"))
    }
}

fn seed_a(s: &str, u: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    "tatami-a".hash(&mut h);
    s.hash(&mut h);
    u.hash(&mut h);
    h.finish()
}

fn seed_b(s: &str, u: &str) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut h = std::collections::hash_map::DefaultHasher::new();
    "tatami-b".hash(&mut h);
    s.hash(&mut h);
    u.hash(&mut h);
    h.finish()
}

/// Récupère (cache disque, fichier local, sinon moteur) les octets + content-type.
pub async fn serve_page(
    sup: &Arc<super::supervisor::Supervisor>,
    cache: &PageCache,
    pages: &PageRef,
    token: &str,
) -> Result<(Vec<u8>, String), String> {
    // Un chapitre téléchargé se lit dans son `.cbz`, sans moteur ni réseau.
    // C'est ce chemin qui rend la lecture hors ligne possible.
    if let PageRef::Local { cbz, entry } = pages {
        return servir_local(cbz, *entry);
    }
    let (source, page_url) = match pages {
        PageRef::Moteur { source, page_url } => (source, page_url),
        PageRef::Local { .. } => unreachable!("traité ci-dessus"),
    };
    let path = cache.cache_path(token);
    if let Ok(meta) = std::fs::metadata(&path) {
        if meta.len() > 0 {
            let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
            // Une entrée de cache écrite avant la vérification peut contenir la
            // page HTML d'un interstitiel : on la relit telle quelle pour ne pas
            // casser ce qui est déjà bon, mais on ne la sert plus comme image.
            let ct = match super::image::detect(&bytes) {
                Some(kind) => kind.content_type().to_string(),
                None => "application/octet-stream".to_string(),
            };
            return Ok((bytes, ct));
        }
    }
    let params = serde_json::json!({ "source": source, "pageUrl": page_url });
    let value: Value = sup
        .request("fetchImage", params, std::time::Duration::from_secs(120))
        .await
        .map_err(|e| format!("engine: {e}"))?;
    if value.get("ok").and_then(Value::as_bool) != Some(true) {
        let e = value.get("error").cloned().unwrap_or(Value::Null);
        return Err(format!(
            "{}: {}",
            e.get("code").and_then(Value::as_str).unwrap_or("ERR"),
            e.get("message").and_then(Value::as_str).unwrap_or("")
        ));
    }
    let result = value.get("result").cloned().unwrap_or(Value::Null);
    let b64 = result
        .get("data")
        .and_then(Value::as_str)
        .ok_or("result.data absent")?;
    let ct = result
        .get("contentType")
        .and_then(Value::as_str)
        .unwrap_or("application/octet-stream");
    let bytes = base64_decode(b64).map_err(|e| e.to_string())?;
    // Même garantie que le téléchargement : on ne met **rien** en cache tant
    // que les octets ne sont pas une image. Sinon le cache conserve la page
    // HTML d'un interstitiel et la réessaie ensuite depuis le disque, à jamais.
    let kind = crate::image::detect(&bytes)
        .ok_or_else(|| crate::image::not_an_image_message("cette page", Some(ct)))?;
    let _ = std::fs::write(&path, &bytes);
    Ok((bytes, kind.content_type().to_string()))
}

/// Lit une entrée d'un `.cbz` local.
///
/// Le type est déduit des **octets**, comme partout ailleurs dans le projet :
/// l'extension du nom de fichier est une convention du site d'origine, pas une
/// garantie. Le CRC est vérifié par `zipread`, donc une page corrompue est
/// refusée au lieu d'être affichée en bordel.
fn servir_local(cbz: &Path, entry: usize) -> Result<(Vec<u8>, String), String> {
    let entrees = crate::zipread::lire_entrees(cbz).map_err(|e| format!("{e}"))?;
    let cible = entrees
        .iter()
        .find(|e| e.index == entry)
        .ok_or_else(|| format!("entrée {entry} absente de {}", cbz.display()))?;
    let mut fichier = std::fs::File::open(cbz).map_err(|e| format!("{} : {e}", cbz.display()))?;
    let octets = crate::zipread::lire_entree(&mut fichier, cible).map_err(|e| format!("{e}"))?;
    let kind = crate::image::detect(&octets).ok_or_else(|| {
        format!(
            "l'entrée {} de {} n'est pas une image ({} octets)",
            cible.nom,
            cbz.display(),
            octets.len()
        )
    })?;
    Ok((octets, kind.content_type().to_string()))
}

fn base64_decode(s: &str) -> Result<Vec<u8>, base64::DecodeError> {
    base64::Engine::decode(&base64::engine::general_purpose::STANDARD, s)
}
