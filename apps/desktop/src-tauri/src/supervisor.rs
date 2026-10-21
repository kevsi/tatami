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

/// Version majeure du protocole que ce cœur accepte. Comparée au
/// `protocolVersion` du `hello` (ADR 0001). `pub` pour que le test de contrat
/// puisse vérifier que la réponse `hello` **enregistrée** est bien acceptée :
/// une constante et une fixture qui divergent, ça ne se voit nulle part.
/// Cf. `lib.rs`.
pub const PROTOCOL_MAJOR: &str = "0";
const HELLO_TIMEOUT: Duration = Duration::from_secs(30);

/// Drapeaux JVM du lancement du moteur, **partagés** par `spawn_once` et `probe`.
///
/// Une seule source, volontairement. Si la sonde de la tâche 5.1 dupliquait cette
/// liste, une divergence ne se verrait qu'au moment où un moteur est réellement
/// installé — donc sur la machine du développeur seulement, et seulement après
/// qu'il a publié une mise à jour. C'est invisible jusqu'au jour où ça casse.
const SPAWN_ARGS: [&str; 6] = [
    "-Xmx256m",
    "-XX:+UseSerialGC",
    // Les sites répondent souvent en IPv6 alors que le réseau local ne l'a pas :
    // mesuré 6,7 s en IPv4 contre 8,1 s sur EXHENTAI, et plus aucune résolution
    // DNS en échec sur ce portable.
    "-Djava.net.preferIPv4Stack=true",
    // stdout est du NDJSON UTF-8 : on verrouille l'encodage JVM.
    "-Dfile.encoding=UTF-8",
    "-Dsun.stdout.encoding=UTF-8",
    "-Dsun.stderr.encoding=UTF-8",
];
/// Deux nouvelles tentatives : 1,5 s puis 3 s. Suffisant pour un DNS qui
/// titille, sans faire attendre une source vraiment morte.
const RETRY_ATTEMPTS: u32 = 2;
const RETRY_BASE: Duration = Duration::from_millis(1500);

/// Un échec réseau qui se résorbera tout seul. Le message vient de
/// `commands.rs::into_result` (« CODE: message ») ou du superviseur.
///
/// `pub` pour le test de contrat : c'est cette fonction qui décide si un échec
/// est rejoué, et une réponse d'erreur rejouée comme transitoire ferait boucler
/// une source morte. Cf. `lib.rs`.
/// L'URL à rejouer par le transport, extraite du message d'erreur.
///
/// Le moteur écrit `challenge at <url>` — c'est cette URL que l'interface
/// passait déjà à `webview_lire`, mais **seulement** pour la commande qu'elle
/// venait d'appeler. Elle est ici reprise au même endroit, pour tous les chemins.
///
/// ⚠️ Cette URL n'est pas forcément celle qui a échoué : mesuré le 2026-10-14
/// sur EPSILONSOFT, la requête partait sur `beta.epsilonsoft.to` tandis que le
/// message annonçait `epsilonsoft.to/manga/`. Rust refuse alors l'URL — c'est le
/// comportement voulu, ce n'est pas un défaut du transport, mais une divergence
/// du parseur amont, à corriger là-bas.
pub fn extraire_url_challenge(message: &str) -> Option<String> {
    let apres = message.split_once("challenge at ")?.1;
    let url = apres.split_whitespace().next()?.trim();
    if url.starts_with("http://") || url.starts_with("https://") {
        Some(url.to_string())
    } else {
        None
    }
}

/// Faut-il tenter le transport webview pour cet échec ?
///
/// Fonction **pure** : la décision est séparée de son exécution, donc elle se
/// teste sans fenêtre ni application. Chaque exclusion est mesurée, pas supposée.
///
/// - `CHALLENGE_REQUIRED` → **oui**, une fois par URL : le navigateur passe là
///   où OkHttp est refusé (mesuré le 2026-10-14, 158 285 octets sur HENTAIZONE).
/// - `CHALLENGE_UNREPLAYABLE` → **jamais** : le rejeu est déjà refusé, rouvrir
///   une fenêtre ne peut qu'échouer (mesuré le 2026-10-11).
/// - `SOURCE_BLOCKED` → **jamais** : Cloudflare refuse **sans proposer de
///   vérification**, il n'y a rien à résoudre.
/// - `PARSE_ERROR`, `TIMEOUT`, `AUTH_REQUIRED`, `RATE_LIMITED`, réseau → **jamais** :
///   aucun n'est un refus de signature, un navigateur n'y change rien.
/// - URL déjà tentée → **jamais** : une tentative par URL, sinon une webview par
///   commande.
///
/// Le message d'erreur d'une réponse, quel que soit le côté par où il arrive.
///
/// **Pourquoi cette fonction existe.** Mesuré le 2026-10-14 :
/// `Supervisor::request` ne met dans `Err` que trois choses — l'écriture sur
/// stdin, le timeout, la mort du sidecar. L'erreur du **moteur**, dont
/// `CHALLENGE_REQUIRED`, arrive dans l'enveloppe `{ok:false, error:{…}}`, donc
/// dans **`Ok`**. Le code faisait `let Err(erreur) = &premier else { return
/// premier }`, et inspectait donc toujours le mauvais côté : **le transport
/// n'était tenté sur aucune source, jamais**.
///
/// Les trois faits qui le prouvent, tous mesurés :
///  - aucune ligne `transport: …` dans le journal, jamais, dans les deux
///    branches (succès et échec) ;
///  - `latest` rendait `CHALLENGE_REQUIRED` en 0,5 s sur ASTRALMANGA, sans
///    fenêtre de vérification ;
///  - `transport_a_tenter`, pure et testée sur le message exact du journal,
///    n'était **jamais alimentée** : un test vert sur une fonction qu'aucun
///    appelant ne fournit d'échec.
///
/// Un `Err` reste traité comme une erreur : son message n'a pas d'URL de
/// challenge, donc le transport ne sera pas tenté — le comportement est
/// inchangé pour ces trois cas.
pub fn erreur_de(resultat: &Result<Value, String>) -> Option<String> {
    match resultat {
        Err(e) => Some(e.clone()),
        Ok(enveloppe) => crate::commands::message_erreur(enveloppe),
    }
}

/// Durée pendant laquelle une tentative de transport **memorise** son URL.
///
/// Borne la rafale, pas le second essai : voir le commentaire du champ
/// `transports_tentes` pour la mesure qui a fait passer la règle de « pour la
/// session » à « dix minutes ».
const TRANSPORT_MEMOISE_TTL: Duration = Duration::from_secs(600);

/// L'URL a-t-elle été tentée **récemment** (dans la borne) ?
///
/// Fonction **pure**, horloge injectée : la règle « une tentative par URL » est
/// testable sans démarrer de sidecar ni de webview, ce qui est impossible dès
/// qu'elle lit une horloge globale.
fn transport_tente_recemment(
    memo: &HashMap<String, std::time::Instant>,
    url: &str,
    maintenant: std::time::Instant,
) -> bool {
    memo.get(url).is_some_and(|instant| {
        maintenant
            .checked_duration_since(*instant)
            .is_some_and(|ecoule| ecoule < TRANSPORT_MEMOISE_TTL)
    })
}

pub fn transport_a_tenter(message: &str, url: Option<&str>, deja_tente: bool) -> bool {
    if deja_tente {
        return false;
    }
    let Some(url) = url else {
        return false;
    };
    if url.is_empty() || !message.contains("CHALLENGE_REQUIRED") {
        return false;
    }
    // Un message portant **les deux codes** est un refus : Cloudflare joint
    // parfois la demande de vérification au refus, et rouvrir une webview sur une
    // source inaccessible ne peut qu'échouer. Ce test passe donc devant.
    if message.contains("CHALLENGE_UNREPLAYABLE") {
        return false;
    }
    true
}

pub fn is_transient(result: &Result<Value, String>) -> bool {
    let Err(message) = result else {
        return false;
    };
    // `AUTH_REQUIRED` (tâche 0.13) n'est pas transitoire : c'est une décision
    // du site, pas une panne. Le rejouer retenterait indéfiniment un 401.
    //
    // `SOURCE_BLOCKED` (2026-10-13) non plus, et pour une raison plus forte
    // encore que `CHALLENGE_UNREPLAYABLE` : la source est **refusée sans
    // qu'aucune vérification ne soit proposée**. La rejouer renverrait le même
    // refus, et la fenêtre de vérification ne servirait à rien.
    //
    // `CHALLENGE_UNREPLAYABLE` (2026-10-11) non plus : la vérification a déjà
    // été faite et le site refuse quand même. Le rejouer coûterait le réseau et
    // reproduirait le 403 à l'identique.
    if message.contains("PARSE_ERROR")
        || message.contains("CHALLENGE_REQUIRED")
        || message.contains("CHALLENGE_UNREPLAYABLE")
        || message.contains("SOURCE_BLOCKED")
        || message.contains("AUTH_REQUIRED")
    {
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
    Exited { duree: Duration },
    StartFailed(String),
}

/// Une session qui a duré au moins ça n'est pas une session morte-née : le
/// moteur a réellement servi, donc repartir au bas du délai est justifié.
pub const SESSION_SAINTE: Duration = Duration::from_secs(60);

/// Plafond du délai entre deux relances.
pub const BACKOFF_MAX: Duration = Duration::from_secs(30);

/// Décide du délai avant la relance suivante.
///
/// `duree_session` est le temps écoulé entre le lancement et la mort du moteur.
///
/// Pourquoi la durée compte : le 2026-10-08, `run` faisait
/// `backoff = 1 s` **inconditionnellement** dès que le fils sortait, et
/// `spawn_once` rendait `Ok(())` sans regarder le statut de sortie. Un moteur
/// qui démarre, réussit son `hello`, puis meurt (OOM sous `-Xmx256m`,
/// `System.exit`, exception non rattrapée) était donc relancé **1 fois par
/// seconde, indéfiniment**, chaque cycle passant par `fail_pending` qui annule
/// les requêtes en vol. Le délai exponentiel ne couvrait que les échecs de
/// *lancement* (`StartFailed`), jamais les morts après démarrage.
pub fn delai_avant_relance(backoff: Duration, duree_session: Duration) -> Duration {
    if duree_session >= SESSION_SAINTE {
        Duration::from_secs(1)
    } else {
        (backoff * 2).min(BACKOFF_MAX)
    }
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
    /// User-agent du moteur, appris au `hello`. La fenêtre de résolution
    /// anti-bot s'en sert pour que la clearance soit émise pour le même agent.
    user_agent: Arc<Mutex<String>>,
    /// URLs déjà passées par le transport webview, avec **l'instant** de la
    /// tentative.
    ///
    /// ## Pourquoi une mémoire, et pas seulement une borne par appel
    ///
    /// Une borne par appel n'empêche pas la boucle : l'interface demande
    /// `details`, puis `chapters`, puis `pages`, et chacune reçoit le même refus
    /// sur une URL **différente**. Sans mémoire, chaque commande ouvrirait une
    /// webview — sur une machine de 12 Go avec un JVM déjà chargé. La règle est
    /// donc **une tentative par URL**, mais **bornée dans le temps**.
    ///
    /// Ce n'est pas une anticipation mais une mesure : le 2026-10-14, un
    /// parcours HENTAIZONE a fait sept `chapters` refusés d'affilée, chacun sur
    /// sa propre URL de série.
    ///
    /// ## Pourquoi une borne dans le temps, et non « pour la session »
    ///
    /// Mesuré le 2026-10-16 : une règle à vie de session devient « **un succès
    /// par URL et pour toujours** ». Le transport réussit pour une fiche à 11 h
    /// 12 (la webview rend 161 ko), puis l'utilisateur y revient à 11 h 19 : le
    /// moteur répond `CHALLENGE_REQUIRED`, l'URL est **déjà** dans la mémoire, le
    /// transport n'est pas tenté, et l'interface affiche le refus brut. Or ce
    /// qui avait fait réussir — le profil réchauffé, le cache mémoire — avait
    /// expiré entre-temps. La règle protégeait la rafale et interdisait le
    /// second essai, qui est précisément ce qu'il faut après une expiration.
    ///
    /// `TRANSPORT_MEMOISE_TTL` (10 min) garde la protection mesurée : une rafale
    /// de refus sur la même URL n'ouvre toujours **qu'une** webview.
    transports_tentes: Mutex<HashMap<String, std::time::Instant>>,
    /// Derniers avertissements du moteur sur les **scripts personalized**.
    ///
    /// Le moteur écrit sur stderr, jamais sur stdout (cf. contrat) : sans ce
    /// tampon, un script refusé — `Missing close quote` — ne laissait qu'une
    /// ligne de journal, invisible de l'utilisateur. Mesuré le 2026-10-10 :
    /// `mangadexFr.js` était refusé à chaque chargement et l'interface
    /// n'affichait que le fichier, comme s'il était chargé.
    ///
    /// Volontairement borné : le moteur réémet à chaque rechargement, donc une
    /// liste non bornée grossirait sans fin.
    avertissements: Arc<Mutex<Vec<String>>>,
    domaines_sources: Arc<Mutex<HashMap<String, Vec<String>>>>,
}

/// Nombre d'avertissements de script conservés. Assez pour voir tous les
/// scripts d'un dossier, assez peu pour que la liste reste lisible.
const MAX_AVERTISSEMENTS: usize = 20;

/// Retient une ligne de stderr si elle annonce un refus de script.
///
/// Le marqueur est le préfixe `AVERTISSEMENT:` que le moteur écrit pour un
/// script ignoré, et non le seul mot `WARN` : java.util émet des `WARN` à
/// profusion, et les retenir viderait le tampon en quelques secondes.
///
/// Renvoie `true` si la ligne a été retenue, pour que l'appelant puisse ne
/// journaliser qu'à cet endroit — la ligne doit apparaître dans le journal
/// **et** dans l'interface, jamais l'une sans l'autre.
fn retenir_avertissement(ligne: &str, liste: &mut Vec<String>) -> bool {
    if !ligne.contains("AVERTISSEMENT:") || !ligne.contains("Script ignoré") {
        return false;
    }
    // La ligne java.util est préfixée d'un horodatage et d'une pile d'appels ;
    // on ne garde que la partie utile, qui commence au marqueur.
    let utile = ligne
        .split_once("AVERTISSEMENT:")
        .map(|(_, suite)| format!("AVERTISSEMENT: {suite}"))
        .unwrap_or_else(|| ligne.to_string());
    if liste.len() >= MAX_AVERTISSEMENTS {
        liste.remove(0);
    }
    liste.push(utile);
    true
}

impl Supervisor {
    pub fn new(java: PathBuf, classpath: String) -> Arc<Self> {
        Arc::new(Self {
            java,
            classpath,
            pending: Arc::new(Mutex::new(HashMap::new())),
            child: Arc::new(Mutex::new(None)),
            engine_proc: Arc::new(Mutex::new(None)),
            user_agent: Arc::new(Mutex::new(String::new())),
            transports_tentes: Mutex::new(HashMap::new()),
            avertissements: Arc::new(Mutex::new(Vec::new())),
            domaines_sources: Arc::new(Mutex::new(HashMap::new())),
        })
    }

    /// Derniers avertissements du moteur sur les scripts personalized, du plus
    /// ancien au plus récent.
    pub async fn avertissements(&self) -> Vec<String> {
        self.avertissements.lock().await.clone()
    }

    pub async fn enregistrer_domaines_sources(&self, resultat: &Value) {
        let mut domaines = HashMap::new();
        if let Some(sources) = resultat.get("sources").and_then(Value::as_array) {
            for source in sources {
                let Some(nom) = source.get("name").and_then(Value::as_str) else {
                    continue;
                };
                let domaines_source = source
                    .get("domains")
                    .and_then(Value::as_array)
                    .map(|valeurs| {
                        valeurs
                            .iter()
                            .filter_map(Value::as_str)
                            .map(str::to_string)
                            .collect()
                    })
                    .unwrap_or_default();
                domaines.insert(nom.to_string(), domaines_source);
            }
        }
        *self.domaines_sources.lock().await = domaines;
    }

    pub async fn domaines_source(&self, source: &str) -> Vec<String> {
        self.domaines_sources
            .lock()
            .await
            .get(source)
            .cloned()
            .unwrap_or_default()
    }

    /// Démarre le moteur, exige un `hello`, puis **l'arrête**. Renvoie son verdict.
    ///
    /// C'est la sonde de la tâche 5.1 : on ne peut savoir si une version
    /// installée est utilisable qu'en la **démarrant réellement** — un dossier
    /// complet et une empreinte conforme ne prouvent pas que la JVM y démarre (JDK
    /// cassé, classpath faux, version de protocole incompatible).
    ///
    /// Elle réutilise **exactement** les mêmes drapeaux de spawn et la même
    /// poignée de main que `spawn_once`, donc elle ne peut pas valider un moteur
    /// que la production refuserait — le corollaire du principe « ne teste pas une
    /// réimplémentation ». Elle renvoie le `protocolVersion` annoncé en cas de
    /// succès.
    pub async fn probe(java: &std::path::Path, classpath: &str) -> Result<String, String> {
        if !java.is_file() {
            return Err(format!("java introuvable : {}", java.display()));
        }
        let mut cmd = Command::new(java);
        cmd.args(SPAWN_ARGS);
        cmd.args(["-cp", classpath, "org.tatami.engine.MainKt", "serve"]);
        cmd.stdin(Stdio::piped());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::null());
        #[cfg(windows)]
        {
            cmd.creation_flags(0x08000000);
        }
        let mut child = cmd.spawn().map_err(|e| format!("spawn: {e}"))?;

        // On lit stdout à la main : la sonde est ponctuelle, donc elle n'a pas
        // besoin du routage par identifiant du superviseur. `stderr` est jeté —
        // sinon le tube se remplit et le moteur se bloque sur une écriture.
        let stdout = child.stdout.take().ok_or("stdout indisponible")?;
        let mut reader = BufReader::new(stdout).lines();
        let mut stdin = child.stdin.take().ok_or("stdin indisponible")?;

        let verdict = tokio::time::timeout(HELLO_TIMEOUT, async {
            let requete = serde_json::json!({
                "id": "probe-hello",
                "method": "hello",
                "params": {},
                "timeoutMs": HELLO_TIMEOUT.as_millis() as u64,
            })
            .to_string();
            // Une écriture peut échouer si le moteur est déjà mort : ce n'est pas
            // une panne du moteur, c'est exactement le verdict « il ne démarre
            // pas ». Donc pas de `?` ici.
            let _ = stdin.write_all(format!("{requete}\n").as_bytes()).await;
            let _ = stdin.flush().await;
            loop {
                // `next_line` rend `Result<Option<String>>` : `None` = fin de
                // flux, donc le moteur a quitté. Les deux cas sont un échec de
                // démarrage, mais pas le même message.
                let ligne = match reader.next_line().await {
                    Ok(Some(l)) => l,
                    Ok(None) => {
                        return Err("le moteur a quitté avant de répondre au hello".to_string())
                    }
                    Err(e) => return Err(format!("lecture du hello impossible : {e}")),
                };
                let Ok(v) = serde_json::from_str::<Value>(&ligne) else {
                    continue; // ligne non JSON : on l'ignore, comme le superviseur
                };
                if v.get("id").and_then(Value::as_str) != Some("probe-hello") {
                    continue;
                }
                if let Some(err) = v.get("error") {
                    return Err(format!("hello refusé : {err}"));
                }
                let version = v
                    .get("result")
                    .and_then(|r| r.get("protocolVersion"))
                    .and_then(Value::as_str)
                    .unwrap_or("")
                    .to_string();
                if version.is_empty() {
                    return Err("hello sans protocolVersion".to_string());
                }
                return Ok(version);
            }
        })
        .await;

        // Arrêt propre dans **tous** les cas, y compris le timeout : un moteur
        // laissé vivant consommerait 256 Mo et un pid pour rien.
        let _ = stdin
            .write_all(b"{\"id\":\"probe-shutdown\",\"method\":\"shutdown\"}\n")
            .await;
        let _ = stdin.flush().await;
        drop(stdin);
        let _ = tokio::time::timeout(Duration::from_secs(5), child.wait()).await;
        let _ = child.kill().await;

        match verdict {
            Err(_elapsed) => Err(format!("pas de réponse au hello en {:?}", HELLO_TIMEOUT)),
            Ok(r) => r,
        }
    }

    /// User-agent annoncé par le moteur (vide avant le premier `hello`).
    pub async fn user_agent(&self) -> String {
        self.user_agent.lock().await.clone()
    }

    /// Boucle de supervision : ne rend la main qu'à l'arrêt de l'app.
    /// `spawn_and_watch` ne revient qu'à la sortie du fils (ou à l'échec de démarrage).
    pub async fn run(self: Arc<Self>) {
        let mut backoff = Duration::from_secs(1);
        loop {
            match self.spawn_and_watch().await {
                WatchEnd::Exited { duree } => {
                    tracing::warn!("sidecar sorti après {duree:?}, relance");
                    backoff = delai_avant_relance(backoff, duree);
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
            Ok(duree) => WatchEnd::Exited { duree },
        }
    }

    async fn spawn_once(&self) -> Result<Duration, String> {
        let demarrage = tokio::time::Instant::now();
        if !self.java.is_file() {
            return Err(format!("java introuvable : {}", self.java.display()));
        }
        let mut cmd = Command::new(&self.java);
        // Les drapeaux JVM sont partagés avec `probe` (voir `SPAWN_ARGS`) :
        // deux listes divergentes feraient valider par la sonde un moteur que
        // la production refuse.
        cmd.args(SPAWN_ARGS);
        cmd.args(["-cp", &self.classpath, "org.tatami.engine.MainKt", "serve"]);
        cmd.stdin(Stdio::piped());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());
        // **Nécessaire, pas prudent.** Mesuré le 2026-10-15 : `spawn_once`
        // fait `engine_proc.lock().await.take()` puis attend le fils dans une
        // variable locale (ligne 624), donc `engine_proc` vaut `None` pendant
        // toute la session et `kill_child` — qui ne voit que cette map —
        // ne tuait jamais rien. Sans ce drapeau, le `Child` de tokio est
        // simplement lâché à l'arrêt du runtime et le **JVM continue de
        // tourner, orphelin**, avec ses 256 Mo. Cinq fermetures dans la
        // journée, cinq JVM dormants sur une machine de 12 Go.
        cmd.kill_on_drop(true);
        #[cfg(windows)]
        {
            cmd.creation_flags(0x08000000);
        }
        let mut child = cmd.spawn().map_err(|e| format!("spawn: {e}"))?;
        let pid = child.id();
        tracing::info!("sidecar démarré (pid={pid:?})");

        let stdin = child.stdin.take().ok_or("stdin indisponible")?;
        let stdout = child.stdout.take().ok_or("stdout indisponible")?;
        let stderr = child.stderr.take().ok_or("stderr indisponible")?;

        // stderr du moteur => logs (jamais sur stdout, cf. contrat).
        // Le NIVEAU ne change pas : le journal reste la source de diagnostic
        // du moteur, et le faire passer en `debug` supprimerait des warnings
        // java.util utiles. Seule une copie bornée est retenue en plus, pour
        // que l'interface puisse dire « ce fichier n'est pas chargé ».
        let avertissements = self.avertissements.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(stderr).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                {
                    let mut garde = avertissements.lock().await;
                    retenir_avertissement(&line, &mut garde);
                }
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
                // Un challenge se compte, pas seulement sa longueur.
                //
                // Mesuré le 2026-10-09 : le journal ne notait que `len=169`, donc un
                // cycle de vérification Cloudflare y était **invisible** : impossible
                // de dire si la fenêtre s'ouvrait, si les cookies partaient, ou si le
                // site refusait. Le contenu de la réponse portait la seule information
                // utile, et il était jeté.
                //
                // On note le **code** et le **message**. Le code seul ne suffisent
                // plus : mesuré le 2026-10-13 sur TOONFR, un rejeu refusé
                // répondait `INTERNAL len=152` — et le journal ne disait rien du
                // tout sur la cause. C'est le même trou que celui du 2026-10-09
                // (« le journal ne le voyait pas »), refermé par la trace mais
                // rouvert par cette décision : on venait de jeter le contenu de
                // la réponse, **y compris le seul champ qui distinguait** « le
                // site refuse le rejeu » de « le parseur a planté ».
                //
                // Le message passe par la rédaction de `journal.rs` comme le
                // reste : une URL y devient une empreinte. Le type d'exception,
                // lui, reste lisible — c'est précisément ce qu'il faut.
                let code = value
                    .get("error")
                    .and_then(|e| e.get("code"))
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let message = value
                    .get("error")
                    .and_then(|e| e.get("message"))
                    .and_then(Value::as_str)
                    .unwrap_or("");
                let extrait: String = message.chars().take(220).collect();
                if code.is_empty() {
                    tracing::info!("stdout recu id={id} len={}", line.len());
                } else {
                    tracing::warn!(
                        "stdout erreur id={id} code={code} msg={extrait} len={}",
                        line.len()
                    );
                }
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
        if let Some(ua) = hello
            .get("result")
            .and_then(|r| r.get("defaultUserAgent"))
            .and_then(Value::as_str)
        {
            *self.user_agent.lock().await = ua.to_string();
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
        Ok(demarrage.elapsed())
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
        // ORDRE DE VERROU (invariant du module) : `pending` s'acquiert
        // TOUJOURS avant `child`, et `child` n'est jamais tenu pendant
        // l'acquisition de `pending`.
        //
        // Pourquoi : `write_all` est le seul await qui tient `child` (le pipe
        // peut être plein, l'écriture attend le moteur). Si l'écriture échoue
        // — donc si le moteur meurt pendant l'écriture — le chemin d'erreur
        // doit reprendre `pending` pour désenregistrer la requête. Ré-acquérir
        // `pending` en tenant `child`, alors que `fail_pending` (relance après
        // crash) tient `pending` et attend `child`, forme un cycle ABBA : les
        // deux futures restent en attente indéfiniment. Les deux taquets tokio
        // n'ont aucune détection de cycle, donc le superviseur ne relance plus
        // jamais le moteur et la commande Tauri appelante reste bloquée — avant
        // même son propre `timeout`, qui n'est atteint qu'après l'écriture.
        // D'où l'extraction ci-dessous : le résultat de l'écriture est produit
        // dans le bloc `child`, puis le verrou est lâché avant tout contact
        // avec `pending`.
        let write = {
            let mut guard = self.child.lock().await;
            match guard.as_mut() {
                Some(handles) => handles
                    .stdin
                    .write_all(format!("{line}\n").as_bytes())
                    .await
                    .map_err(|e| format!("stdin: {e}")),
                None => Err("sidecar non démarré".to_string()),
            }
        };
        if let Err(e) = write {
            self.pending.lock().await.remove(&id);
            return Err(e);
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

    /// Requête au moteur, avec **un** passage par le transport webview si — et
    /// seulement si — l'échec est un challenge rejouable jamais tenté.
    ///
    /// ## Pourquoi ici, et pas dans l'interface
    ///
    /// Tant que le transport n'était appelé que depuis `loadSource`, il ne
    /// couvrait que `latest`. Mesuré le 2026-10-14 sur HENTAIZONE : la liste
    /// passait, puis `details` et `chapters` échouaient — **chacune est une
    /// autre URL**, et rien ne les faisait passer par le transport. Un parcours en
    /// demande des dizaines : l'interface n'est donc pas l'endroit, c'est ici,
    /// une fois, que tous les chemins passent.
    ///
    /// ## Une tentative par URL, pas une par commande
    ///
    /// Voir `transports_tentes`.
    ///
    /// Si le transport **échoue**, l'erreur d'origine est renvoyée telle quelle :
    /// c'est elle que l'interface sait traduire, et un second message ici ferait
    /// perdre au front la distinction entre « à vérifier » et « inaccessible ».
    pub async fn request_avec_transport(
        &self,
        app: &tauri::AppHandle,
        source: &str,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> Result<Value, String> {
        let premier = self.request(method, params.clone(), timeout).await;
        let Some(erreur) = erreur_de(&premier) else {
            return premier;
        };
        let url = extraire_url_challenge(&erreur);
        let maintenant = std::time::Instant::now();
        let deja_tente = match &url {
            Some(u) => {
                let memo = self.transports_tentes.lock().await;
                transport_tente_recemment(&memo, u, maintenant)
            }
            None => true,
        };
        if !transport_a_tenter(&erreur, url.as_deref(), deja_tente) {
            return premier;
        }
        let url = url.expect("transport_a_tenter garantit l'URL");
        let mut memo = self.transports_tentes.lock().await;
        // On borne la mémoire en même temps qu'on écrit : une table qui ne fait
        // que grossir garde des URL d'il y a des heures, alors qu'une seule
        // information compte — « y a-t-il une tentative récente ? ».
        memo.retain(|_, instant| maintenant.duration_since(*instant) < TRANSPORT_MEMOISE_TTL);
        memo.insert(url.clone(), maintenant);
        drop(memo);
        match crate::challenge::lire_par_transport(app, self, source, &url).await {
            Ok(octets) => {
                tracing::info!(
                    "transport: {method} source={source} octets={octets} — nouvelle tentative"
                );
                self.request(method, params, timeout).await
            }
            Err(e) => {
                // L'URL est **déjà** dans la mémoire des tentatives : elle ne sera
                // pas réessayée au prochain appel. C'est ce qui borne la boucle
                // quand le navigateur échoue à son tour.
                tracing::warn!("transport: {method} source={source} échec : {e}");
                premier
            }
        }
    }

    /// Oublie les tentatives de transport **du même hôte** que cette URL.
    ///
    /// Appelé après une vérification réussie : c'est elle qui change ce qui
    /// manquait, donc interdire un second essai jusqu'à la borne ferait que
    /// l'utilisateur a résolu pour rien. Mesuré le 2026-10-16 : transport déjà
    /// tenté et échoué, puis `CHALLENGE_UNREPLAYABLE` sur le rejeu — le chemin qui
    /// fonctionne étant le navigateur, il doit pouvoir repartir.
    pub async fn oublier_transports_de(&self, url: &str) -> usize {
        let Some(hote) = url
            .split_once("://")
            .and_then(|(_, reste)| reste.split('/').next())
        else {
            return 0;
        };
        let hote = hote.split(':').next().unwrap_or(hote).to_string();
        let mut memo = self.transports_tentes.lock().await;
        let avant = memo.len();
        memo.retain(|u, _| {
            u.split_once("://")
                .and_then(|(_, reste)| reste.split('/').next())
                .map(|h| h.split(':').next().unwrap_or(h) != hote)
                .unwrap_or(true)
        });
        avant - memo.len()
    }

    async fn fail_pending(&self, reason: &str) {
        // `pending` puis `child` : conforme à l'invariant documenté dans
        // `request` (jamais `child` tenu en prenant `pending`).
        let mut map = self.pending.lock().await;
        for (_, tx) in map.drain() {
            let _ = tx.send(serde_json::json!({"id":"","ok":false,
                "error":{"code":"CANCELLED","message":reason}}));
        }
        if let Some(h) = self.child.lock().await.take() {
            h.reader_task.abort();
        }
    }

    /// Tuer le moteur si on tient encore son `Child`.
    ///
    /// **Ce n'est presque jamais le cas, et c'est mesuré.** `spawn_once` prend le
    /// `Child` dans `engine_proc` puis le garde dans une variable locale pendant
    /// tout le `wait()` (ligne 624) : la map est donc à `None` dès le premier
    /// démarrage. La fuite qui en découlait est désormais couverte par
    /// `kill_on_drop(true)` au spawn — c'est ce qui tue le fils quand le runtime
    /// s'arrête. Cette fonction reste pour le cas où `spawn_once` n'a pas encore
    /// pris le `Child`, et pour ne pas laisser croire qu'elle suffit.
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

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::process::Command as TestCommand;

    // ---- Politique du transport webview -------------------------------------
    //
    // Les messages ci-dessous sont **pris dans le journal du 2026-10-14**, pas
    // inventés : c'est exactement ce que le moteur a écrit.

    /// Le cas nominal : un challenge rejouable ouvre le transport.
    ///
    /// Journal : `stdout erreur id=latest-… code=CHALLENGE_REQUIRED msg=challenge
    /// at https://hentaizone.xyz/manga/`
    /// Le cas nominal : un challenge rejouable ouvre le transport.
    ///
    /// Journal : `stdout erreur id=latest-… code=CHALLENGE_REQUIRED msg=challenge
    /// at https://hentaizone.xyz/manga/`
    ///
    /// Le message est testé **sur l'enveloppe réellement échouée**, pas sur une
    /// chaîne fabriquée : c'est la forme que `Supervisor::request` rend
    /// (`Ok` + `{ok:false}`). Tester sur une chaîne laissait passer le défaut
    /// mesuré le 2026-10-14 — la fonction était verte, et jamais appelée.
    #[test]
    fn une_enveloppe_echouee_ouvre_le_transport() {
        let reponse: Result<Value, String> = Ok(serde_json::json!({
            "ok": false,
            "error": {
                "code": "CHALLENGE_REQUIRED",
                "message": "challenge at https://hentaizone.xyz/manga/"
            }
        }));
        let message = super::erreur_de(&reponse).expect("une enveloppe ok:false est une erreur");
        assert!(message.contains("CHALLENGE_REQUIRED"), "{message}");
        let url = extraire_url_challenge(&message);
        assert_eq!(
            url.as_deref(),
            Some("https://hentaizone.xyz/manga/"),
            "l'URL du challenge n'est pas lue dans l'enveloppe : le transport ne s'ouvrirait jamais"
        );
        assert!(
            transport_a_tenter(&message, url.as_deref(), false),
            "un challenge rejouable doit ouvrir le transport"
        );
    }

    /// Une réponse réussie n'expose **aucun** message d'erreur : sans ce test,
    /// une inversion de `message_erreur` ferait tenter le transport à chaque
    /// chargement de catalogue.
    #[test]
    fn une_enveloppe_reussie_n_expose_aucun_message() {
        let reponse: Result<Value, String> = Ok(serde_json::json!({
            "ok": true,
            "result": { "list": [] }
        }));
        assert_eq!(super::erreur_de(&reponse), None);
    }

    /// Un échec du cœur (timeout, sidecar mort) reste une erreur, et son
    /// message ne contient aucune URL de challenge : le comportement doit être
    /// inchangé, donc pas de transport.
    #[test]
    fn un_echec_du_coeur_ne_declenche_pas_de_transport() {
        let reponse: Result<Value, String> = Err("timeout 30s sur latest".to_string());
        let message = super::erreur_de(&reponse).expect("un Err est une erreur");
        let url = extraire_url_challenge(&message);
        assert!(url.is_none(), "{message}");
        assert!(!transport_a_tenter(&message, url.as_deref(), false));
    }

    #[test]
    fn un_challenge_requiert_une_tente_de_transport() {
        let message = "CHALLENGE_REQUIRED: challenge at https://hentaizone.xyz/manga/";
        assert_eq!(
            extraire_url_challenge(message).as_deref(),
            Some("https://hentaizone.xyz/manga/")
        );
        assert!(transport_a_tenter(
            message,
            Some("https://hentaizone.xyz/manga/"),
            false
        ));
    }

    /// Un rejeu déjà refusé ne rouvre **jamais** une webview : l'utilisateur a
    /// fait ce qu'il pouvait et le site refuse quand même.
    #[test]
    fn une_url_intramessurable_bloque_une_seconde_tente() {
        let mut memo = HashMap::new();
        let t0 = std::time::Instant::now();
        memo.insert("https://hentaizone.xyz/manga/".to_string(), t0);
        assert!(transport_tente_recemment(
            &memo,
            "https://hentaizone.xyz/manga/",
            t0 + Duration::from_secs(30)
        ));
    }

    /// **La correction du 2026-10-16.** Une règle valable « pour la session »
    /// interdisait aussi le second essai apres une expiration : mesure, la fiche
    /// s'affichait a 11 h 12 puis renvoyait `CHALLENGE_REQUIRED` a 11 h 19, sans
    /// transport tente. Passee la borne, la tentative redevient possible.
    #[test]
    fn une_url_older_que_la_borne_peut_reessayer() {
        let mut memo = HashMap::new();
        let t0 = std::time::Instant::now();
        memo.insert("https://hentaizone.xyz/manga/".to_string(), t0);
        assert!(!transport_tente_recemment(
            &memo,
            "https://hentaizone.xyz/manga/",
            t0 + TRANSPORT_MEMOISE_TTL + Duration::from_secs(1)
        ));
    }

    /// La regle complete : memoire fraiche -> refus, memoire expiree -> passage.
    #[test]
    fn la_borne_decide_de_la_seconde_tente() {
        let message = "CHALLENGE_REQUIRED: challenge at https://hentaizone.xyz/manga/";
        let url = "https://hentaizone.xyz/manga/";
        let mut memo = HashMap::new();
        let t0 = std::time::Instant::now();
        memo.insert(url.to_string(), t0);
        let t1 = t0 + Duration::from_secs(30);
        assert!(!transport_a_tenter(
            message,
            Some(url),
            transport_tente_recemment(&memo, url, t1)
        ));
        let t2 = t0 + TRANSPORT_MEMOISE_TTL + Duration::from_secs(1);
        assert!(transport_a_tenter(
            message,
            Some(url),
            transport_tente_recemment(&memo, url, t2)
        ));
    }

    /// Une URL jamais tentee reste tentable, quelle que soit l'heure.
    #[test]
    fn une_url_inconnue_est_toujours_tentable() {
        let memo = HashMap::new();
        assert!(!transport_tente_recemment(
            &memo,
            "https://hentaizone.xyz/manga/",
            std::time::Instant::now()
        ));
    }

    #[test]
    fn un_rejeu_refuse_ne_tente_jamais_le_transport() {
        let message =
            "CHALLENGE_UNREPLAYABLE: challenge at https://toonfr.com/webtoon/ (non rejouable)";
        assert!(!transport_a_tenter(
            message,
            Some("https://toonfr.com/webtoon/"),
            false
        ));
    }

    /// Le vrai piège : un message qui porte **les deux codes** est un refus, pas
    /// une demande. Cloudflare joint parfois les deux, et sans test d'ordre le
    /// transport se rouvrirait sur une source inaccessible.
    #[test]
    fn un_message_qui_porte_les_deux_codes_est_traite_comme_un_refus() {
        let message =
            "CHALLENGE_UNREPLAYABLE: challenge at https://x.example/ — CHALLENGE_REQUIRED au premier essai";
        assert!(
            message.contains("CHALLENGE_REQUIRED"),
            "le message porte bien les deux codes : c'est le cas à couvrir"
        );
        assert!(!transport_a_tenter(
            message,
            Some("https://x.example/"),
            false
        ));
    }

    /// Cloudflare refuse **sans proposer de vérification** : rien à résoudre.
    #[test]
    fn un_blocage_sans_verification_ne_tente_jamais() {
        let message = "SOURCE_BLOCKED: source blocked by Cloudflare at https://x.example/";
        assert!(!transport_a_tenter(
            message,
            Some("https://x.example/"),
            false
        ));
    }

    /// Une tentative par URL : c'est ce qui empêche une webview par commande.
    #[test]
    fn une_url_deja_tentee_ne_tente_plus() {
        let message = "CHALLENGE_REQUIRED: challenge at https://hentaizone.xyz/manga/";
        assert!(!transport_a_tenter(
            message,
            Some("https://hentaizone.xyz/manga/"),
            true
        ));
    }

    #[test]
    fn sans_url_le_transport_ne_tente_pas() {
        let message = "CHALLENGE_REQUIRED: challenge at ";
        assert_eq!(extraire_url_challenge(message), None);
        assert!(!transport_a_tenter(message, None, false));
    }

    /// Un 404, un délai ou une panne réseau ne sont pas des refus de signature :
    /// un navigateur n'y change rien, et en ouvrir un serait du gaspillage.
    #[test]
    fn une_panne_reseau_ne_tente_pas_le_transport() {
        for message in [
            "TIMEOUT: delai depasse",
            "UnknownHostException: hentaizone.xyz",
            "PARSE_ERROR: html inattendu",
            "AUTH_REQUIRED: this source asks for an account",
            "404 NotFoundException",
        ] {
            assert!(
                !transport_a_tenter(message, Some("https://hentaizone.xyz/"), false),
                "{message} ne doit pas ouvrir de transport"
            );
        }
    }

    /// L'extracteur ne rend jamais autre chose qu'une URL http(s) : la garde de
    /// `valider_url_challenge` existe pour ça, mais autant ne pas lui laisser du
    /// travail qui n'est pas le sien.
    #[test]
    fn un_non_http_nest_pas_une_url_de_challenge() {
        for url in [
            "ftp://hentaizone.xyz/",
            "javascript:alert(1)",
            "hentaizone.xyz",
        ] {
            assert_eq!(
                extraire_url_challenge(&format!("challenge at {url}")),
                None,
                "{url} ne doit pas être rendu"
            );
        }
    }

    /// Un avertissement du moteur sur un script doit être **retenu** : c'est la
    /// seule trace qui dit à l'utilisateur que son fichier n'est pas chargé.
    /// Mesuré le 2026-10-10 : `mangadexFr.js` était refusé à chaque
    /// chargement, le journal Rust le voyait, et l'interface n'en disait rien.
    #[test]
    fn un_avertissement_de_script_est_retenu() {
        let mut v = Vec::new();
        let retenu = retenir_avertissement(
            "AVERTISSEMENT: Script ignoré (mangadexFr.js) : <eval>:20:36 Missing close quote",
            &mut v,
        );
        assert!(retenu, "la ligne d'avertissement doit être retenue");
        assert_eq!(v.len(), 1);
        assert!(v[0].contains("mangadexFr.js"), "on garde le nom du fichier");
        assert!(v[0].contains("Missing close quote"), "on garde la raison");
    }

    /// Les lignes de log ordinaires ne doivent pas remplir la liste : elle est
    /// bornée, et un WARN de java.util fills la viderait en quelques secondes.
    #[test]
    fn les_lignes_hors_avertissement_ne_sont_pas_retenues() {
        let mut v = Vec::new();
        for ligne in [
            "Oct 10, 2026 10:13:13 AM org.tatami.engine.ScriptedSourceLoader charger",
            "SLF4J: Failed to load class org.slf4j.impl.StaticLoggerBinder",
            "WARN: un simple mot WARN ne veut pas dire script refusé",
        ] {
            assert!(
                !retenir_avertissement(ligne, &mut v),
                "ne doit pas être retenue : {ligne}"
            );
        }
        assert!(v.is_empty(), "la liste doit rester vide, elle vaut {v:?}");
    }

    /// La liste est **bornée** : le moteur peut refuser un script à chaque
    /// rechargement, et une liste non bornée grossirait sans fin.
    #[test]
    fn la_liste_des_avertissements_est_bornee() {
        let mut v = Vec::new();
        for i in 0..(MAX_AVERTISSEMENTS + 5) {
            retenir_avertissement(
                &format!("AVERTISSEMENT: Script ignoré (source{i}.js) : boom"),
                &mut v,
            );
        }
        assert_eq!(v.len(), MAX_AVERTISSEMENTS, "la liste doit être bornée");
        // Le plus récent est conservé, le plus ancien est lâché.
        assert!(
            v.last()
                .unwrap()
                .contains(&format!("source{}", MAX_AVERTISSEMENTS + 4)),
            "on garde le plus récent"
        );
    }

    /// Un processus quelconque qui ne lit jamais son stdin : il ne sert qu'à
    /// obtenir une vraie poignée `ChildStdin`, donc un vrai canal. On n'émule
    /// pas le moteur, seulement le canal.
    fn spawn_silent_process() -> tokio::process::Child {
        let mut cmd = if cfg!(windows) {
            // `cmd.exe` engendre `ping.exe` comme enfant : la poignée stdout est
            // héritée par cet enfant, qui survit à la mort de `cmd.exe` et garde
            // le pipe ouvert — le lecteur attend alors EOF jusqu'à la fin des
            // 30 pings, ce qui fige le harnais de tests. On spawne `ping`
            // directement pour que `kill()` le tue et ferme le pipe aussitôt.
            let mut c = TestCommand::new("ping");
            c.args(["-n", "30", "127.0.0.1"]);
            c
        } else {
            let mut c = TestCommand::new("sh");
            c.args(["-c", "sleep 30"]);
            c
        };
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("processus de substitution")
    }

    /// Attend qu'une requête soit enregistrée dans `pending` : elle est donc en
    /// vol, donc à cet instant ou sur le point de viser `child`.
    async fn wait_enregistree(m: &PendingMap, d: Duration) -> bool {
        let fin = tokio::time::Instant::now() + d;
        loop {
            let vide = m.lock().await.is_empty();
            if !vide {
                return true;
            }
            if tokio::time::Instant::now() >= fin {
                return false;
            }
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    }

    /// Attend que le verrou soit réellement détenu par une autre tâche.
    /// `try_lock` est la seule sonde possible : on ne peut pas observer la
    /// prise d'un verrou sans le prendre soi-même.
    async fn wait_held<T>(m: &Mutex<T>, d: Duration) -> bool {
        let fin = tokio::time::Instant::now() + d;
        loop {
            if m.try_lock().is_err() {
                return true;
            }
            if tokio::time::Instant::now() >= fin {
                return false;
            }
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    }

    /// Non-régression ABBA.
    ///
    /// Scénario réel : le JVM meurt pendant qu'une requête est en vol, et le
    /// superviseur purge les requêtes en vol au même moment. `request` tient
    /// alors `child` pendant l'écriture ; l'écriture échoue (lecteur du canal
    /// mort) et le chemin d'erreur doit reprendre `pending` pour désenregistrer
    /// la requête. `fail_pending` tient `pending` et attend `child`. Avant le
    /// correctif : cycle ABBA, les deux futures ne reviennent jamais — le
    /// superviseur ne relance plus le moteur et la commande appelante reste
    /// bloquée avant même son propre `timeout`.
    ///
    /// L'ordre est forcé par le test (verrou `child` tenu en amont, file FIFO
    /// de tokio) : sinon la purge peut gagner la course et le cycle ne se forme
    /// pas. Le délai est borné pour qu'un vrai interblocage fasse échouer le
    /// test au lieu de figer la suite.
    #[tokio::test]
    async fn purge_et_requete_en_vol_ne_se_bloquent_pas() {
        let sup = Supervisor::new(PathBuf::from("java"), String::new());
        assert!(sup.child.lock().await.is_none(), "état initial inattendu");

        let mut engine = spawn_silent_process();
        let mut stdin = engine.stdin.take().expect("stdin");
        let stdout = engine.stdout.take().expect("stdout");

        // Le moteur meurt. Sa poignée reste pourtant dans `child` : c'est
        // exactement la fenêtre réelle entre la mort du JVM et le `take()`.
        let _ = engine.kill().await;
        let _ = engine.wait().await;

        // Mesuré sur cette plateforme : une écriture vers un canal dont le
        // lecteur est mort renvoie `Ok` (octets tamponnés par tokio), puis
        // `Err(BrokenPipe)` au flush suivant. On amorce donc le tampon pour que
        // l'écriture de la requête échoue à coup sûr.
        stdin.write_all(b"#").await.expect("amorce");

        let reader_task = tokio::spawn(async move {
            let mut lines = BufReader::new(stdout).lines();
            while let Ok(Some(_ligne)) = lines.next_line().await {}
        });
        *sup.child.lock().await = Some(ChildHandles { stdin, reader_task });

        // On tient `child` pour ordonner l'entrelacement (file FIFO).
        let garde = sup.child.lock().await;

        // (1) Requête en vol : elle s'enregistre puis attend `child`.
        let requete = {
            let sup = sup.clone();
            tokio::spawn(async move {
                sup.request("search", serde_json::json!({}), Duration::from_secs(30))
                    .await
            })
        };
        assert!(
            wait_enregistree(&sup.pending, Duration::from_secs(5)).await,
            "la requête ne s'est jamais enregistrée dans `pending`"
        );

        // (2) Purge concurrente (relance après crash) : elle prend `pending`,
        //     puis attend `child`. On sonde `pending` : la requête en vol l'a
        //     déjà relâché (elle attend `child`), donc le détenteur observé ne
        //     peut être que la purge.
        let purge = {
            let sup = sup.clone();
            tokio::spawn(async move { sup.fail_pending("sidecar restarted").await })
        };
        assert!(
            wait_held(&sup.pending, Duration::from_secs(5)).await,
            "la purge n'a pas obtenu `pending` : le test ne reproduit pas l'interblocage"
        );

        // (3) On libère : la requête prend `child` (FIFO), son écriture échoue,
        //     et elle bifurque sur le chemin d'erreur qui repasse par `pending`.
        drop(garde);

        let (purge_ok, requete_ok) = tokio::time::timeout(Duration::from_secs(10), async {
            (purge.await, requete.await)
        })
        .await
        .unwrap_or_else(|_| {
            panic!(
                "INTERBLOCAGE ABBA : purge et requête en vol bloquées au-delà de 10 s \
                 (`child` détenu puis `pending` réclamé, pendant que `pending` est \
                 détenu puis `child` réclamé)"
            )
        });
        purge_ok.expect("la purge n'a pas terminé");
        // Une requête annulée par la purge n'a pas à réussir : ce qui compte
        // est qu'elle rende la main, pas qu'elle obtienne une réponse.
        let _ = requete_ok.expect("la requête n'a pas terminé");
    }

    /// Le défaut réel : `delai_avant_relance` ignorait `duree_session` et
    /// remettait toujours le backoff à 1 s. Un moteur qui meurt après 10 s
    /// était donc relancé chaque seconde, indéfiniment.
    ///
    /// Ce test échoue sur l'ancien code (remise à 1 s), et passe une fois la
    /// logique corrigée (backoff exponentiel pour les sessions courtes).
    #[test]
    fn backoff_exponentiel_pour_les_sessions_courtes() {
        let court = Duration::from_secs(10); // session de 10 s
        let long = Duration::from_secs(300); // session de 5 min

        // Session courte : le backoff doit grandir, pas être remis à 1 s
        assert_eq!(
            delai_avant_relance(Duration::from_secs(1), court),
            Duration::from_secs(2),
            "session courte → backoff double (1s → 2s)"
        );
        assert_eq!(
            delai_avant_relance(Duration::from_secs(2), court),
            Duration::from_secs(4),
            "session courte → backoff double (2s → 4s)"
        );
        assert_eq!(
            delai_avant_relance(Duration::from_secs(16), court),
            Duration::from_secs(30),
            "session courte → plafonné à BACKOFF_MAX (30s)"
        );

        // Session longue : on repart au plancher
        assert_eq!(
            delai_avant_relance(Duration::from_secs(30), long),
            Duration::from_secs(1),
            "session longue → backoff remis à 1s"
        );
    }

    #[tokio::test]
    async fn le_registre_de_domaines_suit_list_sources() {
        let sup = Supervisor::new(PathBuf::from("java"), String::new());
        sup.enregistrer_domaines_sources(&serde_json::json!({
            "sources": [
                {"name": "MANGALIB", "domains": ["mangalib.org", "mangalib.me"]},
                {"name": "CUSTOM:no-domain", "lang": "fr"}
            ]
        }))
        .await;

        assert_eq!(
            sup.domaines_source("MANGALIB").await,
            vec!["mangalib.org", "mangalib.me"]
        );
        assert!(sup.domaines_source("CUSTOM:no-domain").await.is_empty());
        assert!(sup.domaines_source("unknown").await.is_empty());

        sup.enregistrer_domaines_sources(&serde_json::json!({"sources": []}))
            .await;
        assert!(sup.domaines_source("MANGALIB").await.is_empty());
    }
}
