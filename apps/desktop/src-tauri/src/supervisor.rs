//! Superviseur du sidecar JVM (contrat NDJSON sur stdio, cf. ADR 0001).
//!
//! - Démarrage : `java … org.tatami.engine.MainKt serve` (runtime jlink + jars,
//!   résolus depuis `engine/dist` en dev ; packaging via `resources` en 0.9+,
//!   pas `externalBin` : ce n'est pas un binaire natif).
//! - Poignée de main `hello` (refus si MAJEUR différent).
//! - Chien de garde : toute sortie (crash ou EOF) => backoff puis relance.
//! - stdout = protocole uniquement ; stderr du moteur redirigé en logs.

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::{oneshot, Mutex};

const PROTOCOL_MAJOR: &str = "0";
const HELLO_TIMEOUT: Duration = Duration::from_secs(30);

type PendingMap = Arc<Mutex<HashMap<String, oneshot::Sender<Value>>>>;

enum WatchEnd {
    Exited,
    StartFailed(String),
}

struct ChildHandles {
    stdin: ChildStdin,
    reader_task: tokio::task::JoinHandle<()>,
}

pub struct Supervisor {
    java: PathBuf,
    classpath: String,
    pending: PendingMap,
    child: Arc<Mutex<Option<ChildHandles>>>,
    engine_proc: Arc<Mutex<Option<Child>>>,
}

impl Supervisor {
    pub fn new(java: PathBuf, classpath: String) -> Arc<Self> {
        Arc::new(Self {
            java,
            classpath,
            pending: Arc::new(Mutex::new(HashMap::new())),
            child: Arc::new(Mutex::new(None)),
            engine_proc: Arc::new(Mutex::new(None)),
        })
    }

    /// Boucle de supervision : ne rend la main qu'à l'arrêt de l'app.
    /// `spawn_and_watch` ne revient qu'à la sortie du fils (ou à l'échec de démarrage).
    pub async fn run(self: Arc<Self>) {
        let mut backoff = Duration::from_secs(1);
        loop {
            match self.spawn_and_watch().await {
                WatchEnd::Exited => {
                    tracing::warn!("sidecar sorti, relance");
                    backoff = Duration::from_secs(1);
                }
                WatchEnd::StartFailed(e) => {
                    tracing::warn!("sidecar indisponible ({e}), nouvel essai");
                    backoff = (backoff * 2).min(Duration::from_secs(30));
                }
            }
            // On purge les requêtes en vol puis on relance après backoff.
            self.fail_pending("sidecar restarted").await;
            self.kill_child().await;
            tracing::info!("relance du sidecar dans {backoff:?}");
            tokio::time::sleep(backoff).await;
        }
    }

    async fn spawn_and_watch(&self) -> WatchEnd {
        match self.spawn_once().await {
            Err(e) => WatchEnd::StartFailed(e),
            Ok(()) => WatchEnd::Exited,
        }
    }

    async fn spawn_once(&self) -> Result<(), String> {
        if !self.java.is_file() {
            return Err(format!("java introuvable : {}", self.java.display()));
        }
        let mut child = Command::new(&self.java)
            .args([
                "-Xmx256m",
                "-XX:+UseSerialGC",
                "-cp",
                &self.classpath,
                "org.tatami.engine.MainKt",
                "serve",
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("spawn: {e}"))?;
        let pid = child.id();
        tracing::info!("sidecar démarré (pid={pid:?})");

        let stdin = child.stdin.take().ok_or("stdin indisponible")?;
        let stdout = child.stdout.take().ok_or("stdout indisponible")?;
        let stderr = child.stderr.take().ok_or("stderr indisponible")?;

        // stderr du moteur => logs (jamais sur stdout, cf. contrat).
        tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                tracing::warn!(target: "sidecar", "{line}");
            }
        });

        // stdout => routage des réponses par id.
        let pending = self.pending.clone();
        let reader_task = tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let value: Value = match serde_json::from_str(&line) {
                    Ok(v) => v,
                    Err(e) => {
                        tracing::warn!("ligne stdout non-JSON ignorée : {e}");
                        continue;
                    }
                };
                let id = value.get("id").and_then(Value::as_str).unwrap_or("");
                if let Some(tx) = pending.lock().await.remove(id) {
                    let _ = tx.send(value);
                }
            }
        });

        *self.child.lock().await = Some(ChildHandles { stdin, reader_task });
        *self.engine_proc.lock().await = Some(child);

        // Poignée de main : le moteur répond protocolVersion/engineVersion/parsersVersion.
        let hello = self
            .request("hello", serde_json::json!({}), HELLO_TIMEOUT)
            .await
            .map_err(|e| format!("hello: {e}"))?;
        let version = hello
            .get("result")
            .and_then(|r| r.get("protocolVersion"))
            .and_then(Value::as_str)
            .unwrap_or("");
        let major = version.split('.').next().unwrap_or("");
        if major != PROTOCOL_MAJOR {
            return Err(format!("protocolVersion incompatible : {version}"));
        }
        tracing::info!("hello OK (protocol={version})");

        // Surveillance bloquante : on ne revient qu'à la sortie du fils
        // (crash, EOF, ou arrêt). C'est ce qui déclenche la relance.
        let mut child = self.engine_proc.lock().await.take();
        if let Some(c) = child.as_mut() {
            match c.wait().await {
                Ok(status) => tracing::warn!("sidecar terminé ({status})"),
                Err(e) => tracing::warn!("attente sidecar : {e}"),
            }
        }
        Ok(())
    }

    /// Envoie une requête et attend la réponse (utilisé par les commandes Tauri en 0.9).
    pub async fn request(
        &self,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<Value, String> {
        let id = format!("{method}-{}", unique_id());
        let line = serde_json::json!({
            "id": id,
            "method": method,
            "params": params,
        })
        .to_string();
        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(id.clone(), tx);
        {
            let mut guard = self.child.lock().await;
            match guard.as_mut() {
                Some(handles) => {
                    if let Err(e) = handles
                        .stdin
                        .write_all(format!("{line}\n").as_bytes())
                        .await
                    {
                        self.pending.lock().await.remove(&id);
                        return Err(format!("stdin: {e}"));
                    }
                }
                None => {
                    self.pending.lock().await.remove(&id);
                    return Err("sidecar non démarré".to_string());
                }
            }
        }
        tokio::time::timeout(timeout, rx)
            .await
            .map_err(|_| format!("timeout {timeout:?} sur {method}"))?
            .map_err(|_| format!("sidecar relancé pendant {method}"))
    }

    async fn fail_pending(&self, reason: &str) {
        let mut map = self.pending.lock().await;
        for (_, tx) in map.drain() {
            let _ = tx.send(serde_json::json!({"id":"","ok":false,
                "error":{"code":"CANCELLED","message":reason}}));
        }
        if let Some(h) = self.child.lock().await.take() {
            h.reader_task.abort();
        }
    }

    async fn kill_child(&self) {
        if let Some(mut child) = self.engine_proc.lock().await.take() {
            let _ = child.kill().await;
            let _ = child.wait().await;
        }
    }

    pub async fn shutdown(&self) {
        self.kill_child().await;
    }
}

fn unique_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static CTR: AtomicU64 = AtomicU64::new(0);
    let n = CTR.fetch_add(1, Ordering::Relaxed);
    format!(
        "{}-{n}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis())
            .unwrap_or(0)
    )
}

/// Résolution du runtime en dev : `engine/dist/tatami-engine` (cf. scripts/build-engine.ps1).
/// Depuis src-tauri : 3 crans (src-tauri -> desktop -> apps -> racine).
pub fn dev_sidecar_paths() -> (PathBuf, String) {
    let dist = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("..")
        .join("engine")
        .join("dist")
        .join("tatami-engine");
    let java = dist.join("runtime").join("bin").join("java.exe");
    let cp = format!(r"{}\lib\*", dist.display());
    (java, cp)
}
