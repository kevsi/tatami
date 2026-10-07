//! Superviseur du **sidecar vision** (contrat NDJSON sur stdio, ADR 0001).
//!
//! Distinct du superviseur du moteur de sources, volontairement : deux
//! processus, deux durées de vie, deux rythmes. Le moteur de sources répond en
//! millisecondes ; le sidecar vision met **2,2 s** rien qu'à détecter une tuile,
//! et 6 s à charger ses modèles. Partager le superviseur ferait hériter la
//! traduction d'une latence et un chemin de relance conçue pour l'autre.
//!
//! ## Ce qui est propre à ce superviseur
//!
//! - **Démarrage à la demande** : le sidecar n'est pas lancé au démarrage de
//!   l'application. Personne ne traduit par défaut, et 361 Mo de poids ne
//!   doivent pas être chargés pour rien.
//! - **ORT_DYLIB_PATH** : le runtime ONNX est trouvé par cette variable
//!   (ADR 0007, `load-dynamic`). Sans elle, `ort` ne trouve pas la DLL et le
//!   chargement échoue — donc on la pose **avant** de lancer, jamais après.
//! - **Pas de chien de garde** : contrairement au moteur, une mort du sidecar
//!   n'a rien à relancer. Le prochain appel le redémarre. Relancer en boucle un
//!   processus qui vient d'échouer à charger 361 Mo de poids serait du bruit.

use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, Command};
use tokio::sync::{oneshot, Mutex};

/// Version majeure du protocole acceptée, comme pour le moteur de sources.
pub const PROTOCOL_MAJOR: &str = "0";

/// Délai de la poignée de main. Le sidecar ne charge **aucun** modèle au
/// démarrage (`loadPack` est explicite), donc 30 s est déjà large ; c'est la
/// même valeur que le moteur pour que les deux se lisent pareil.
const HELLO_TIMEOUT: Duration = Duration::from_secs(30);

/// Délai d'une détection. Mesuré le 2026-10-07 : 2,1 à 2,5 s par tuile de
/// 640×640 sur la machine cible, en int8. On laisse le double pour une machine
/// plus lente ou une tuile pleine.
pub const DELAI_DETECT: Duration = Duration::from_secs(10);

/// Délai d'une lecture de texte. Mesuré : 6 lignes en 342 ms, une tuile entière
/// bien plus. 30 s pour une page, c'est large sans être illimité.
pub const DELAI_OCR: Duration = Duration::from_secs(30);

/// Délai de chargement d'un pack. Mesuré : trois sessions ouvertes en quelques
/// secondes ; 120 s couvre une machine lente et un disque externe.
pub const DELAI_CHARGEMENT: Duration = Duration::from_secs(120);

struct Poignees {
    stdin: ChildStdin,
    lecteur: tokio::task::JoinHandle<()>,
}

/// Le superviseur du sidecar vision. Partagé par `Arc` comme celui du moteur.
pub struct SuperviseurVision {
    binaire: PathBuf,
    runtime_onnx: Option<PathBuf>,
    pack: Option<PathBuf>,
    requetes_en_cours: Arc<Mutex<HashMap<String, oneshot::Sender<Value>>>>,
    enfant: Arc<Mutex<Option<Poignees>>>,
    processus: Arc<Mutex<Option<Child>>>,
    demarre: Arc<Mutex<bool>>,
}

impl SuperviseurVision {
    /// `runtime_onnx` est le chemin de `onnxruntime.dll`. `pack` le dossier de
    /// poids ; sans lui, le sidecar démarre mais refuse toute méthode de vision,
    /// ce qui est un état propre et pas une panne.
    pub fn new(
        binaire: PathBuf,
        runtime_onnx: Option<PathBuf>,
        pack: Option<PathBuf>,
    ) -> Arc<Self> {
        Arc::new(Self {
            binaire,
            runtime_onnx,
            pack,
            requetes_en_cours: Arc::new(Mutex::new(HashMap::new())),
            enfant: Arc::new(Mutex::new(None)),
            processus: Arc::new(Mutex::new(None)),
            demarre: Arc::new(Mutex::new(false)),
        })
    }

    /// Démarre le sidecar s'il ne tourne pas, exige un `hello`, puis charge le
    /// pack s'il y en a un. Idempotent : le planter deux fois ne fait rien de
    /// plus.
    pub async fn assurer_demarrage(self: &Arc<Self>) -> Result<(), String> {
        if *self.demarre.lock().await && self.enfant.lock().await.is_some() {
            return Ok(());
        }
        if !self.binaire.is_file() {
            return Err(format!(
                "sidecar vision introuvable : {}",
                self.binaire.display()
            ));
        }

        let mut cmd = Command::new(&self.binaire);
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            // stderr est lu : sinon le tube se remplit et le sidecar se bloque
            // sur une écriture. Les journaux partent dans la journalisation.
            .stderr(Stdio::piped());
        if let Some(dll) = &self.runtime_onnx {
            // AVANT le spawn : `ort` lit cette variable au chargement de la
            // DLL, donc la poser après ne servirait à rien.
            cmd.env("ORT_DYLIB_PATH", dll);
        }
        let mut enfant = cmd
            .spawn()
            .map_err(|e| format!("spawn sidecar vision : {e}"))?;

        let stdout = enfant.stdout.take().ok_or("stdout indisponible")?;
        let stdin = enfant.stdin.take().ok_or("stdin indisponible")?;
        let stderr = enfant.stderr.take();
        if let Some(e) = stderr {
            tokio::spawn(async move {
                let mut lignes = BufReader::new(e).lines();
                while let Ok(Some(l)) = lignes.next_line().await {
                    tracing::info!("[vision] {l}");
                }
            });
        }

        let en_cours = self.requetes_en_cours.clone();
        let lecteur = tokio::spawn(async move {
            let mut lignes = BufReader::new(stdout).lines();
            loop {
                match lignes.next_line().await {
                    Ok(Some(ligne)) => {
                        let v: Value = match serde_json::from_str(ligne.trim()) {
                            Ok(v) => v,
                            Err(e) => {
                                tracing::warn!("[vision] ligne non-JSON : {e}");
                                continue;
                            }
                        };
                        let id = v.get("id").and_then(Value::as_str).unwrap_or("");
                        match en_cours.lock().await.remove(id) {
                            Some(tx) => {
                                let _ = tx.send(v);
                            }
                            None => tracing::warn!("[vision] id inconnu {id}"),
                        }
                    }
                    Ok(None) => break,
                    Err(e) => {
                        tracing::warn!("[vision] lecture stdout : {e}");
                        break;
                    }
                }
            }
        });

        *self.enfant.lock().await = Some(Poignees { stdin, lecteur });
        *self.processus.lock().await = Some(enfant);
        *self.demarre.lock().await = true;

        let hello = self
            .demander("hello", serde_json::json!({}), HELLO_TIMEOUT)
            .await
            .map_err(|e| format!("hello sidecar vision : {e}"))?;
        let version = hello
            .get("result")
            .and_then(|r| r.get("protocolVersion"))
            .and_then(Value::as_str)
            .unwrap_or("");
        if version.split('.').next().unwrap_or("") != PROTOCOL_MAJOR {
            return Err(format!("protocolVersion incompatible : {version}"));
        }

        if let Some(pack) = self.pack.clone() {
            let chemin = pack.to_string_lossy().to_string();
            self.demander(
                "loadPack",
                serde_json::json!({ "path": chemin }),
                DELAI_CHARGEMENT,
            )
            .await
            .map_err(|e| format!("loadPack : {e}"))?;
        }
        tracing::info!("sidecar vision prêt (protocol={version})");
        Ok(())
    }

    /// Envoie une requête et attend sa réponse, avec le même ordre de verrous que
    /// le superviseur du moteur : `requetes_en_cours` **avant** `enfant`, et
    /// `enfant` jamais tenu en revenant vers `requetes_en_cours`.
    pub async fn demander(
        &self,
        methode: &str,
        params: Value,
        delai: Duration,
    ) -> Result<Value, String> {
        let id = format!("vision-{methode}-{}", uuid_lite());
        let ligne = serde_json::json!({
            "id": id,
            "method": methode,
            "params": params,
            "timeoutMs": delai.as_millis() as u64,
        })
        .to_string();
        let (tx, rx) = oneshot::channel();
        self.requetes_en_cours.lock().await.insert(id.clone(), tx);

        let ecriture = {
            let mut garde = self.enfant.lock().await;
            match garde.as_mut() {
                Some(p) => p
                    .stdin
                    .write_all(format!("{ligne}\n").as_bytes())
                    .await
                    .map_err(|e| format!("stdin vision : {e}")),
                None => Err("sidecar vision non démarré".to_string()),
            }
        };
        if let Err(e) = ecriture {
            self.requetes_en_cours.lock().await.remove(&id);
            *self.demarre.lock().await = false;
            return Err(e);
        }

        match tokio::time::timeout(delai, rx).await {
            Ok(Ok(v)) => Ok(v),
            Ok(Err(_)) => {
                self.requetes_en_cours.lock().await.remove(&id);
                Err("sidecar vision a fermé la réponse".to_string())
            }
            Err(_) => {
                // Le délai est le nôtre : le sidecar peut travailler encore.
                // On désenregistre pour ne pas laisser un `oneshot` en fuite.
                self.requetes_en_cours.lock().await.remove(&id);
                Err(format!("{methode} vision : délai dépassé ({delai:?})"))
            }
        }
    }

    /// Un `true` si le sidecar répond, sans rien charger. C'est la sonde de
    /// santé que l'interface peut appeler pour dire « traduction disponible ».
    pub async fn vivant(&self) -> bool {
        self.demander("health", serde_json::json!({}), Duration::from_secs(5))
            .await
            .map(|v| v.get("ok") == Some(&Value::Bool(true)))
            .unwrap_or(false)
    }

    /// Ce que le pack permet de faire. `inpaintDisponible` est `false` tant que
    /// T2.5 n'est pas fait : l'interface l'affiche au lieu d'échouer plus tard.
    pub async fn capacites(&self) -> Result<Value, String> {
        self.demander("capabilities", serde_json::json!({}), HELLO_TIMEOUT)
            .await
    }

    /// Arrête le sidecar proprement : `shutdown`, puis on rend les poignées.
    pub async fn arreter(&self) {
        let _ = self
            .demander("shutdown", serde_json::json!({}), Duration::from_secs(5))
            .await;
        if let Some(p) = self.enfant.lock().await.take() {
            p.lecteur.abort();
        }
        if let Some(mut c) = self.processus.lock().await.take() {
            let _ = c.kill().await;
        }
        *self.demarre.lock().await = false;
    }
}

/// Identifiant de requête. Le moteur a son `unique_id` ; celui-ci est local au
/// sidecar vision et n'a pas à être globally unique.
/// Le chemin de la DLL ONNX, ou `None` pour que `ort` la cherche seul.
///
/// `ort` est compilé en `load-dynamic` (ADR 0007) : sans `ORT_DYLIB_PATH`, il
/// cherche `onnxruntime.dll` **dans le dossier de l'exécutable** — ce qui est
/// exactement là que l'installeur la pose. Donc :
///
/// - une variable qui pointe vers un fichier **inexistant** est ignorée, avec un
///   avertissement : elle ferait échouer le chargement alors que la DLL est à
///   côté ;
/// - sinon `None`, et le sidecar se débrouille.
///
/// Une seule décision, un seul endroit : le sidecar ne résout rien lui-même,
/// sinon les deux chercheraient à des moments différents.
pub fn runtime_onnx() -> Option<PathBuf> {
    if let Ok(v) = std::env::var("ORT_DYLIB_PATH") {
        let p = PathBuf::from(&v);
        if p.is_file() {
            return Some(p);
        }
        tracing::warn!("ORT_DYLIB_PATH pointe vers {v}, qui n'existe pas : ignoré");
    }
    None
}

fn uuid_lite() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn les_identifiants_de_requete_ne_se_repetent_pas() {
        let mut vus = std::collections::HashSet::new();
        for _ in 0..1000 {
            assert!(
                vus.insert(uuid_lite()),
                "deux requetes ont le meme identifiant"
            );
        }
    }

    #[test]
    fn un_binaire_absent_est_nomme() {
        let s = SuperviseurVision::new(
            PathBuf::from("C:/tatami-vision-inexistant-xyz.exe"),
            None,
            None,
        );
        // Le chemin est vérifié avant tout spawn : pas de processus inutile.
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("runtime de test");
        let e = rt
            .block_on(s.assurer_demarrage())
            .expect_err("binaire absent");
        assert!(e.contains("introuvable"), "{e}");
    }
}
