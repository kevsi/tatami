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
/// Deux nouvelles tentatives : 1,5 s puis 3 s. Suffisant pour un DNS qui
/// titille, sans faire attendre une source vraiment morte.
const RETRY_ATTEMPTS: u32 = 2;
const RETRY_BASE: Duration = Duration::from_millis(1500);

/// Un échec réseau qui se résorbera tout seul. Le message vient de
/// `commands.rs::into_result` (« CODE: message ») ou du superviseur.
fn is_transient(result: &Result<Value, String>) -> bool {
    let Err(message) = result else {
        return false;
    };
    if message.contains("PARSE_ERROR") || message.contains("CHALLENGE_REQUIRED") {
        return false;
    }
    if message.starts_with("RATE_LIMITED") {
        return true;
    }
    if message.starts_with("TIMEOUT") || message.contains("timeout") {
        return true;
    }
    // Côté moteur, ces exceptions JDK viennent toutes du réseau ou du site.
    [
        "UnknownHostException",
        "SocketTimeoutException",
        "ConnectException",
        "NoRouteToHostException",
        "SSLException",
        "503 Service Unavailable",
        "502 Bad Gateway",
        "500 Internal Server Error",
    ]
    .iter()
    .any(|marker| message.contains(marker))
}

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
                // Les sites répondent souvent en IPv6 alors que le réseau local ne
                // l'a pas : mesuré 6,7 s en IPv4 contre 8,1 s sur EXHENTAI, et
                // plus aucune résolution DNS en échec sur ce portable.
                "-Djava.net.preferIPv4Stack=true",
                // stdout est du NDJSON UTF-8 : on verrouille l'encodage JVM.
                "-Dfile.encoding=UTF-8",
                "-Dsun.stdout.encoding=UTF-8",
                "-Dsun.stderr.encoding=UTF-8",
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
            let mut lines = BufReader::new(stdout);
            let mut buf: Vec<u8> = Vec::new();
            loop {
                buf.clear();
                match lines.read_until(b'\n', &mut buf).await {
                    Ok(0) => {
                        tracing::warn!("stdout: EOF (moteur ferme)");
                        break;
                    }
                    Ok(_) => {}
                    Err(e) => {
                        tracing::warn!("lecture stdout terminee: {e}");
                        break;
                    }
                }
                // Octets -> texte tolerant : un titre exotique ne doit pas tuer
                // la boucle de lecture (incident UTF-8 / cp1252 sur Windows).
                let line = String::from_utf8_lossy(&buf).trim().to_string();
                let value: Value = match serde_json::from_str(&line) {
                    Ok(v) => v,
                    Err(e) => {
                        tracing::warn!("ligne stdout non-JSON ignoree: {e}");
                        continue;
                    }
                };
                let id = value.get("id").and_then(Value::as_str).unwrap_or("");
                tracing::info!("stdout recu id={id} len={}", line.len());
                match pending.lock().await.remove(id) {
                    Some(tx) => {
                        let _ = tx.send(value);
                    }
                    None => tracing::warn!("stdout: id inconnu {id}"),
                }
            }
            tracing::warn!("stdout: boucle de lecture terminee");
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
            // Le moteur abandonne au même instant que nous : sinon il reste
            // coincé sur un parser bloqué (thread, sockets).
            "timeoutMs": timeout.as_millis() as u64,
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

    /// Comme `request`, mais réessaie les échecs réseau transitoires.
    ///
    /// Un `UnknownHostException` ou un timeout arrive sur des sources qui
    /// répondent très bien une seconde plus tard (DNS qui titille, box qui
    /// bascule). Sans nouvelle tentative, un chapitre était marqué en échec
    /// définitivement alors que le simple fait de réessayer suffisait.
    /// Les erreurs définitives (paramètre invalide, challenge anti-bot) ne sont
    /// jamais réessayées.
    pub async fn request_with_retry(
        &self,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<Value, String> {
        let mut attempt = 0;
        loop {
            let result = self.request(method, params.clone(), timeout).await;
            if attempt >= RETRY_ATTEMPTS || !is_transient(&result) {
                return result;
            }
            attempt += 1;
            let wait = RETRY_BASE * attempt;
            tracing::warn!("{method}: échec transitoire, nouvel essai dans {wait:?}");
            tokio::time::sleep(wait).await;
        }
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
