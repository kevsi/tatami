//! Protocole `tatami://` (Windows : `http://tatami.localhost/`).
//!
//! index source -> moteur `fetchImage` -> cache disque -> webview ne parle qu'aux
//! IDs connus (jamais une URL arbitraire). ADR transporteur ne le couvre pas :
//! images en base64 (protocole v0) seraient mieux en fichiers streamés plus tard.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use serde_json::Value;

#[derive(Clone, Debug)]
pub struct PageRef {
    pub source: String,
    pub page_url: String,
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
            if p.page_url == page_url && p.source == source {
                return tok.clone();
            }
        }
        let token = format!(
            "{:016x}{:016x}",
            seed_a(source, page_url),
            seed_b(source, page_url)
        );
        map.insert(
            token.clone(),
            PageRef {
                source: source.to_string(),
                page_url: page_url.to_string(),
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

/// Récupère (cache disque sinon moteur) et renvoie les octets + content-type.
pub async fn serve_page(
    sup: &Arc<super::supervisor::Supervisor>,
    cache: &PageCache,
    pages: &PageRef,
    token: &str,
) -> Result<(Vec<u8>, String), String> {
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
    let params = serde_json::json!({ "source": pages.source, "pageUrl": pages.page_url });
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

fn base64_decode(s: &str) -> Result<Vec<u8>, base64::DecodeError> {
    base64::Engine::decode(&base64::engine::general_purpose::STANDARD, s)
}
