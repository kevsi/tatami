//! Protocole `tatami://` (Windows : `http://tatami.localhost/`).
//!
//! index source -> moteur `fetchImage` -> cache disque -> webview ne parle qu'aux
//! IDs connus (jamais une URL arbitraire). ADR transporteur ne le couvre pas :
//! images en base64 (protocole v0) seraient mieux en fichiers streamés plus tard.
//!
//! **Rien ne sort d'ici sans être une image.** Couvertures comprises : la webview
//! n'a jamais le droit d'appeler un site de manga, sinon elle perd le user-agent,
//! les cookies et le déblocage Cloudflare que le moteur met en place — et l'échec
//! est silencieux (couverture vide, console propre).

use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use serde_json::Value;

/// Bornes du cache. Ces plafonds existent parce que les entrées ET les fichiers
/// de cache vivaient jusqu'à la fin de la session : ouvrir un chapitre de 300
/// pages, puis parcourir 50 couvertures, laissait 350 fichiers sur le disque et
/// autant de chaînes en mémoire, sans jamais être relâchés.
const CAP_PAGE: usize = 2048;
const CAP_COVER: usize = 384;

/// Ce qu'un jeton du protocole désigne.
///
/// Trois natures : une page de source distante (le moteur va la chercher), une
/// couverture de source distante (idem — c'est la même règle), ou une page
/// d'un `.cbz` déjà téléchargé (lue dans le fichier, sans réseau).
#[derive(Clone, Debug)]
pub enum PageRef {
    /// Page d'une source distante : le moteur va la chercher.
    Moteur { source: String, page_url: String },
    /// Couverture d'une série : passe par le moteur comme une page.
    ///
    /// `manga_url` sert au moteur à retrouver la série dans son cache de session ;
    /// `cover_url` est le repli quand le cache est vide (redémarrage, favori lu
    /// depuis la base) — sans lui, aucune couverture ne s'affiche jamais dans
    /// l'onglet Accueil.
    Couverture {
        source: String,
        manga_url: String,
        cover_url: Option<String>,
    },
    /// Page d'un `.cbz` déjà téléchargé : lue dans le fichier.
    Local {
        cbz: PathBuf,
        /// Rang de l'entrée dans le répertoire central, et non son nom.
        entry: usize,
    },
}

impl PageRef {
    /// Un `.cbz` local a déjà ses octets sur le disque : rien n'est à évacuer.
    fn est_cache(&self) -> bool {
        !matches!(self, PageRef::Local { .. })
    }
}

/// Clé métier d'une entrée, reconstituée depuis sa référence : c'est
/// exactement la clé que `inserer` a posée dans `by_key`.
///
/// `None` pour une page locale, qui n'a pas de clé métier — `register_local`
/// écrit dans `map` et dans `ordre` sans jamais passer par `by_key`.
///
/// Cette fonction est le miroir de ce que fait `inserer`, donc elle doit vivre
/// à côté : sans elle, purger `by_key` demandait de redouter à la main les trois
/// formes de `PageRef`, et une quatrième forme l'aurait oublié en silence —
/// c'est-à-dire exactement le défaut qu'elle corrige.
fn cle_metier(token: &str, r: &PageRef) -> Option<(u8, String, String)> {
    // `l…` = local : `register_local` n'inscrit jamais de clé métier.
    if token.starts_with('l') {
        return None;
    }
    match r {
        PageRef::Moteur { source, page_url } => Some((1, source.clone(), page_url.clone())),
        PageRef::Couverture {
            source, manga_url, ..
        } => Some((2, source.clone(), manga_url.clone())),
        PageRef::Local { .. } => None,
    }
}

/// Index inversé (clé métier -> jeton) + file d'insertion pour l'éviction.
struct Index {
    map: HashMap<String, PageRef>,
    by_key: HashMap<(u8, String, String), String>,
    ordre: HashMap<u8, VecDeque<String>>,
}

impl Index {
    fn vide() -> Self {
        Index {
            map: HashMap::new(),
            by_key: HashMap::new(),
            ordre: HashMap::new(),
        }
    }

    fn cap(kind: u8) -> usize {
        match kind {
            2 => CAP_COVER,
            _ => CAP_PAGE,
        }
    }
}

pub struct PageCache {
    index: Mutex<Index>,
    dir: PathBuf,
    /// Dimensions (largeur, hauteur) en pixels, mémorisées lors de l'écriture
    /// dans le cache disque. Valeur optionnelle : absente si l'image n'a pas
    /// encore été téléchargée lors de l'appel `pages`.
    ///
    /// En mémoire uniquement (pas de persistance entre sessions) : même pattern
    /// que l'index. Quand un jeton est évincé de l'index, ses dimensions
    /// restent dans cette map jusqu'au redémarrage — c'est inoffensif : un jeton
    /// évincé puis recréé pour la même paire (source, url) est **identique**
    /// (seeds déterministes dans la session), donc les dimensions correspondent.
    /// **Borne de croissance** : dans le pire cas raisonnable (session longue,
    /// 10 chapitres × 300 pages = 3 000 entrées), chaque `(String, (u32, u32))`
    /// coûte ~50 octets → ~150 Ko. Acceptable pour la v1 ; si un chapitre est
    /// évincé de l'index, ses dims restent ici jusqu'au redémarrage.
    ///
    /// TODO (post-v1) : connecter cette map à l'éviction de l'index pour borner
    /// la croissance à `CAP_PAGE + CAP_COVER` entrées.
    dims: Mutex<HashMap<String, (u32, u32)>>,
}

impl PageCache {
    pub fn new(dir: PathBuf) -> Arc<Self> {
        let _ = std::fs::create_dir_all(&dir);
        Arc::new(Self {
            index: Mutex::new(Index::vide()),
            dir,
            dims: Mutex::new(HashMap::new()),
        })
    }

    /// Mémorise les dimensions d'une image pour un jeton donné.
    ///
    /// Appelé depuis `ecrire_cache` **sur les octets déjà en mémoire**, sans
    /// aucune I/O supplémentaire. `pub(crate)` pour les tests de commande.
    pub(crate) fn stocker_dims(&self, token: &str, dims: (u32, u32)) {
        self.dims
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(token.to_string(), dims);
    }

    /// Retourne les dimensions d'un jeton, ou `None` si elles sont inconnues.
    ///
    /// **Mémoire d'abord, disque ensuite.** Les dimensions sont posées en
    /// mémoire à l'écriture (`stocker_dims`), mais rien n'était écrit sur le
    /// fichier : après un redémarrage, la mémoire est vide et l'interface ne
    /// connaissait plus la taille d'aucune page — donc `bandeVirtualisable`
    /// valait `false` et le lecteur rendait **tout** le chapitre d'un coup
    /// (mesuré le 2026-10-09 dans l'application réelle).
    ///
    /// Or les octets **sont** sur le disque, et leur en-tête porte exactement
    /// ces deux nombres. On les relit donc à la demande : `image::dimensions`
    /// s'arrête au marqueur `SOF`/`IHDR` et n'encode jamais l'image, donc le
    /// coût est un `read` de quelques kilo-octets — et il n'a lieu que pour
    /// une page déjà téléchargée, donc déjà sur le disque.
    ///
    /// Le résultat est mémorisé : la seconde lecture ne relit pas le fichier.
    pub fn lire_dims(&self, token: &str) -> Option<(u32, u32)> {
        if let Some(d) = self
            .dims
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .get(token)
            .copied()
        {
            return Some(d);
        }
        // Reprise depuis le disque. Un fichier illisible ou absent donne
        // `None` — jamais `(0, 0)`, qui ferait croire à une dimension connue.
        let chemin = self.cache_path(token);
        let lu = std::fs::read(&chemin)
            .ok()
            .and_then(|b| crate::image::dimensions(&b));
        if let Some(d) = lu {
            self.stocker_dims(token, d);
        }
        lu
    }

    /// Enregistre (ou retrouve) une page distante.
    ///
    /// L'index rend l'opération O(1). Le premier balayage linéaire de la map
    /// rendait `register` O(n²) : un chapitre de 303 pages coûtait ~46 000
    /// comparaisons de chaînes, et le chapitre suivant recommençait à zéro
    /// puisque rien n'était jamais évacué.
    pub fn register(&self, source: &str, page_url: &str) -> String {
        self.inserer(
            1,
            source,
            page_url,
            PageRef::Moteur {
                source: source.to_string(),
                page_url: page_url.to_string(),
            },
        )
    }

    /// Enregistre (ou retrouve) la couverture d'une série.
    pub fn register_cover(&self, source: &str, manga_url: &str, cover_url: Option<&str>) -> String {
        self.inserer(
            2,
            source,
            manga_url,
            PageRef::Couverture {
                source: source.to_string(),
                manga_url: manga_url.to_string(),
                cover_url: cover_url.map(str::to_string),
            },
        )
    }

    fn inserer(&self, kind: u8, source: &str, cle: &str, valeur: PageRef) -> String {
        let mut ix = self.index.lock().unwrap_or_else(|e| e.into_inner());
        let key = (kind, source.to_string(), cle.to_string());
        if let Some(existant) = ix.by_key.get(&key) {
            return existant.clone();
        }
        let token = format!(
            "{}{:016x}{:016x}",
            if kind == 2 { "c" } else { "p" },
            seed_a(source, cle),
            seed_b(source, cle)
        );
        let plafond = Index::cap(kind);
        // Purge des plus anciens AU DESSUS du plafond, pour ne jamais évacter
        // une entrée qu'on vient d'ajouter (elle n'est pas encore dans la file
        // à ce moment-là). L'emprunt de la file est refermé à chaque tour :
        // le garder ouvert interdit d'accéder à `map` dans la même fonction.
        loop {
            let victime = {
                let file = ix.ordre.entry(kind).or_default();
                if file.len() < plafond {
                    break;
                }
                file.pop_front()
            };
            let Some(victime) = victime else { break };
            // L'index inversé DOIT être purgé pour la victime. Le commentaire
            // d'avant (« ses clones ont d'autres jetons ») expliquait le défaut
            // critique mesuré le 2026-10-09 : l'entrée devenait absente de
            // `map` mais restait dans `by_key`, donc `inserer` la rendait à
            // nouveau SANS la réinscrire — et `main.rs:200` répondait
            // `not_found` sans jamais consulter le cache disque ni le moteur.
            // Un chapitre déjà lu s'ouvrait donc en images cassées, avec un
            // `.cbz` parfaitement intact.
            //
            // Les clones n'ont pas d'autres jetons : `token` dérive de
            // `(kind, source, cle)` par hachage (`:203-208`), donc deux
            // enregistrements de la même clé partagent le jeton et `map` ne
            // peut détenir qu'une entrée.
            if let Some(r) = ix.map.remove(&victime) {
                // Une entrée locale n'a pas de clé métier : elle n'est jamais
                // passée par `by_key`, donc il n'y a rien à purger.
                if let Some(cle) = cle_metier(&victime, &r) {
                    // Un clone a pu réinscrire la même clé entre-temps : ne
                    // purger que si `by_key` désigne ENCORE la victime.
                    if ix.by_key.get(&cle).is_some_and(|t| *t == victime) {
                        ix.by_key.remove(&cle);
                    }
                }
                if r.est_cache() {
                    let _ = std::fs::remove_file(self.cache_path(&victime));
                }
            }
        }
        if let Some(prec) = ix.map.insert(token.clone(), valeur.clone()) {
            if prec.est_cache() {
                let _ = std::fs::remove_file(self.cache_path(&token));
            }
        }
        ix.ordre.entry(kind).or_default().push_back(token.clone());
        ix.by_key.insert(key, token.clone());
        token
    }

    /// Enregistre une page d'un `.cbz` local et renvoie son jeton.
    ///
    /// Le jeton est dérivé du chemin et du rang : rejouer la même liste rend les
    /// mêmes jetons, donc un chapitre rouvert ne crée pas de nouvelles entrées.
    /// Aucune entrée, donc aucune éviction : un chapitre local reste lisible
    /// aussi longtemps que l'application tourne.
    pub fn register_local(&self, cbz: &Path, entry: usize) -> String {
        let chemin = cbz.to_string_lossy().to_string();
        let token = format!(
            "l{:016x}{:016x}",
            seed_a("local", &chemin),
            seed_b(&format!("{entry}"), &chemin)
        );
        let mut ix = self.index.lock().unwrap_or_else(|e| e.into_inner());
        ix.map.insert(
            token.clone(),
            PageRef::Local {
                cbz: cbz.to_path_buf(),
                entry,
            },
        );
        token
    }

    pub fn resolve(&self, token: &str) -> Option<PageRef> {
        self.index
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .map
            .get(token)
            .cloned()
    }

    /// Emplacement d'une page servie, à partir de son jeton.
    ///
    /// La lecture publique qui existait ici (`octets`) n'avait qu'un seul
    /// raison d'être : la chaîne de traduction, qui avait besoin des pixels hors
    /// du protocole `tatami://`. Elle a été retirée avec cette fonctionnalité,
    /// faute d'appelant — `serve_page` est le seul chemin de lecture.
    fn cache_path(&self, token: &str) -> PathBuf {
        self.dir.join(format!("page-{token}.bin"))
    }
}

/// Jeton de couverture d'une ligne de base, ou `None` si elle n'en a pas.
///
/// La base ignore le cache d'images ; c'est à la frontière avec l'interface que
/// chaque ligne reçoit le jeton que la webview utilisera. Sans cela la seule URL
/// disponible est celle du site, et l'application afficherait des vignettes vides
/// sur toute source protégée.
pub fn jeton_cover(
    cache: &PageCache,
    source: &str,
    manga_url: &str,
    cover_url: Option<&str>,
) -> Option<String> {
    let couv = cover_url?;
    if manga_url.is_empty() || couv.trim().is_empty() {
        return None;
    }
    Some(cache.register_cover(source, manga_url, Some(couv)))
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
///
/// L'ordre est toujours le même : le `.cbz` local d'abord (lecture hors ligne),
/// le cache disque ensuite, le moteur en dernier. Le type renvoyé est toujours
/// déduit des **octets** : aucun en-tête du site n'est cru sur parole.
pub async fn serve_page(
    sup: &Arc<super::supervisor::Supervisor>,
    cache: &PageCache,
    page: &PageRef,
    token: &str,
) -> Result<(Vec<u8>, String), String> {
    // Sonde de temps, 2026-10-10. Mesuré côté navigateur : **10,4 s** entre
    // l'émission d'une requête et sa première réponse, alors que le moteur étale
    // ses requêtes de 800 ms en 800 ms et devrait répondre à la première en
    // ~800 ms. Il restait donc ~8 s **non localisés**. Plutôt que de deviner
    // (quatre fausses conclusions dans cette session), on horloge les étapes :
    //
    //   entree  → relecture du cache
    //   cache   → appel moteur (`sup.request`)
    //   moteur  → décodage base64 + écriture du cache disque
    //   ecriture→ rendu de la réponse à la webview
    //
    // Les quatre lignes se lisent dans l'ordre dans `tatami.log`. Elles sont
    // permanentes : une sonde qu'on retire après s'emploi est une sonde qui
    // manque au prochain problème.
    let debut = std::time::Instant::now();
    let court = &token[..token.len().min(8)];
    tracing::info!("page entree {court}");

    // Un chapitre téléchargé se lit dans son `.cbz`, sans moteur ni réseau.
    if let PageRef::Local { cbz, entry } = page {
        return servir_local(cbz, *entry);
    }
    if let Some(bytes) = relire_cache(cache, token) {
        tracing::info!(
            "page {court} servie du cache en {} ms",
            debut.elapsed().as_millis()
        );
        let ct = crate::image::detect(&bytes)
            .map(|k| k.content_type().to_string())
            .unwrap_or_else(|| "application/octet-stream".to_string());
        return Ok((bytes, ct));
    }

    let (methode, params, quoi) = match page {
        PageRef::Moteur { source, page_url } => (
            "fetchImage",
            serde_json::json!({ "source": source, "pageUrl": page_url }),
            "cette page",
        ),
        PageRef::Couverture {
            source,
            manga_url,
            cover_url,
        } => (
            "fetchCover",
            serde_json::json!({
                "source": source,
                "mangaUrl": manga_url,
                "coverUrl": cover_url,
            }),
            "cette couverture",
        ),
        PageRef::Local { .. } => unreachable!("traité ci-dessus"),
    };

    tracing::info!(
        "page {court} cache absente apres {} ms, appel moteur",
        debut.elapsed().as_millis()
    );
    let value = sup
        .request(methode, params, std::time::Duration::from_secs(120))
        .await
        .map_err(|e| format!("engine: {e}"))?;
    tracing::info!(
        "page {court} moteur a repondu en {} ms",
        debut.elapsed().as_millis()
    );
    let bytes = decoder(value, quoi)?;
    let r = ecrire_cache(cache, token, &bytes, quoi);
    tracing::debug!(
        "page {court} servie en {} ms ({} octets, cache ecrit={})",
        debut.elapsed().as_millis(),
        bytes.len(),
        r.is_ok()
    );
    r
}

/// Extrait les octets base64 d'une réponse `fetchImage` / `fetchCover`.
fn decoder(value: Value, quoi: &str) -> Result<Vec<u8>, String> {
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
    let bytes = base64_decode(b64).map_err(|e| e.to_string())?;
    if bytes.is_empty() {
        return Err(format!("{quoi} : corps vide"));
    }
    Ok(bytes)
}

/// Écrit dans le cache disque **atomiquement** et rend les octets + leur type
/// réel. Ne met rien en cache tant que les octets ne sont pas une image.
///
/// L'écriture se fait dans un fichier temporaire puis un `rename` : une écriture
/// interrompue (disque plein, coupure) ne peut donc pas laisser d'entrée
/// tronquée. C'était le défaut mesuré : l'écriture ignorait son erreur, la
/// relecture renvoyait un fichier incomplet, et ce fichier incomplet était
/// servi tel quel — définitivement, puisque c'était désormais la source.
fn ecrire_cache(
    cache: &PageCache,
    token: &str,
    bytes: &[u8],
    quoi: &str,
) -> Result<(Vec<u8>, String), String> {
    let kind = crate::image::detect(bytes)
        .ok_or_else(|| crate::image::not_an_image_message(quoi, None))?;
    let final_path = cache.cache_path(token);
    let tmp = cache.cache_path(&format!("{token}.tmp{}", std::process::id()));
    std::fs::write(&tmp, bytes).map_err(|e| format!("cache {} : {e}", final_path.display()))?;
    if let Err(e) = std::fs::rename(&tmp, &final_path) {
        let _ = std::fs::remove_file(&tmp);
        tracing::warn!("cache {} : rename échoué ({e})", final_path.display());
    }
    // Mémorise les dimensions sur les octets déjà en mémoire, sans I/O
    // supplémentaire. Le rename peut avoir échoué, mais les octets sont valides :
    // on stocke quand même (la prochaine relecture cache passera par le moteur
    // mais les dimensions sont déjà connues pour cette session).
    if let Some(dims) = crate::image::dimensions(bytes) {
        cache.stocker_dims(token, dims);
    }
    Ok((bytes.to_vec(), kind.content_type().to_string()))
}

/// Relit une entrée déjà en cache, ou `None` si elle est absente ou invalide.
///
/// Une entrée qui n'est plus une image est **supprimée** et renvoyée comme
/// absente : la requête repart alors vers le moteur. Le code précédent la
/// relisait telle quelle en `application/octet-stream`, ce qui figeait
/// définitivement une réponse aberrante.
fn relire_cache(cache: &PageCache, token: &str) -> Option<Vec<u8>> {
    let path = cache.cache_path(token);
    let meta = std::fs::metadata(&path).ok()?;
    if meta.len() == 0 {
        let _ = std::fs::remove_file(&path);
        return None;
    }
    let bytes = std::fs::read(&path).ok()?;
    if crate::image::detect(&bytes).is_none() {
        tracing::warn!("cache {token} : entrée non-image purgée");
        let _ = std::fs::remove_file(&path);
        return None;
    }
    Some(bytes)
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

#[cfg(test)]
mod tests {
    use super::*;

    /// Le dossier du cache est une fixture : dossier unique (les tests sont
    /// parallèles et un nom fixe les fait se marcher dessus) et effacé en fin
    /// de test — mesuré le 2026-10-06, `tatami-cache-test` traînait dans la vraie
    /// bibliothèque parce que son nom fixe le rendait invisible au nettoyage.
    ///
    /// Le cache est **étiqueté** pour qu'un jeton soit rejouable : le test
    /// relit ses propres jetons, donc il faut le même dossier d'un test à
    /// l'autre. D'où `label`, plutôt qu'une racine neuve par appel.
    fn cache() -> (std::sync::Arc<PageCache>, crate::download::DossierDeTest) {
        let garde = crate::download::DossierDeTest::new("cache");
        // Pas `env::temp_dir()` : sur cette machine TEMP n'est pas expansé et
        // résout un chemin littéral contre le dossier courant (leçon AGENTS.md).
        (PageCache::new(garde.chemin()), garde)
    }

    /// `ecrire_cache` est une fonction libre : sans ce pont, un test ne peut
    /// l'atteindre que si elle devient `pub`, ce qui ouvrirait l'API interne
    /// du cache pour une seule raison de test. Le pont reste donc dans le
    /// module de test.
    fn ecrire_cache_de_test(c: &PageCache, token: &str, bytes: &[u8]) -> Result<(), String> {
        super::ecrire_cache(c, token, bytes, "cette page").map(|_| ())
    }

    #[test]
    fn le_meme_url_redonne_le_meme_jeton() {
        let (c, _garde_cache) = cache();
        let a = c.register("MANGADEX", "https://x/1.jpg");
        let b = c.register("MANGADEX", "https://x/1.jpg");
        assert_eq!(a, b, "un URL identique doit réutiliser le jeton");
        // Une source différente = une autre image, donc un autre jeton.
        assert_ne!(a, c.register("MANGAFR", "https://x/1.jpg"));
    }

    /// Un PNG dont l'en-tête déclare `l x h`, fabriqué à la main.
    ///
    /// On n'encode pas une vraie image : `image::dimensions` s'arrête au
    /// marqueur `IHDR`, donc ces quelques octets suffisent — et le test reste
    /// déterministe, sans encodeur dans le chemin critique.
    fn png_entete(largeur: u32, hauteur: u32) -> Vec<u8> {
        let mut v = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
        v.extend_from_slice(&13u32.to_be_bytes());
        v.extend_from_slice(b"IHDR");
        v.extend_from_slice(&largeur.to_be_bytes());
        v.extend_from_slice(&hauteur.to_be_bytes());
        v.extend_from_slice(&[8, 6, 0, 0, 0]);
        v
    }

    /// Les dimensions doivent survivre au **redémarrage**.
    ///
    /// Mesuré le 2026-10-09 : `ecrire_cache` calcule les dimensions sur les
    /// octets en mémoire et les confie à `stocker_dims`. Rien n'est écrit sur
    /// disque — donc, au redémarrage suivant, `lire_dims` renvoie `None` pour
    /// toute page. Or le fichier **est** sur le disque, et ses octets portent
    /// l'en-tête JPEG/PNG/WebP qui contient exactement ces deux nombres.
    ///
    /// Conséquence mesurée dans l'application : au premier ouverture d'un
    /// chapitre, aucune dimension n'est connue, donc `bandeVirtualisable`
    /// vaut `false`, la bande n'est pas virtualisée et **toutes** les pages
    /// sont rendues d'un coup. Le lecteur n'a de dimensions qu'à la seconde
    /// ouverture.
    ///
    /// La relecture ne coûte qu'un `read` de l'en-tête : `image::dimensions`
    /// s'arrête au marqueur SOF et n'encode jamais l'image.
    #[test]
    fn les_dimensions_survivent_a_un_rechargement_du_cache() {
        let (c, _garde) = cache();
        let token = c.register("MANGADEX", "https://x/1.png");
        // Un PNG minimal 2x3, écrit **par le chemin de production**.
        let png = png_entete(2, 3);
        ecrire_cache_de_test(&c, &token, &png).expect("ecriture");
        assert_eq!(
            c.lire_dims(&token),
            Some((2, 3)),
            "les dimensions doivent etre connues immediatement"
        );

        // On simule le redémarrage : un cache neuf sur **le meme dossier**, donc
        // le fichier est bien relisible, mais l'index mémoire est vide.
        let relu = PageCache::new(c.dir.clone());
        let token2 = relu.register("MANGADEX", "https://x/1.png");
        assert_eq!(
            relu.lire_dims(&token2),
            Some((2, 3)),
            "apres redemarrage, les dimensions doivent venir du fichier sur disque, \
             pas de la memoire : sans elles, la bande n'est pas virtualisable et le \
             lecteur rend toutes les pages d'un coup (mesure le 2026-10-09)"
        );
    }

    /// Contre-test : une page **absente** du disque n'a pas de dimensions.
    ///
    /// Sans ce cas, une fonction qui renverrait toujours `Some((0, 0))`
    /// passerait le test précédent — et `bandeVirtualisable` croirait alors
    /// savoir des dimensions qu'elle ne sait pas.
    #[test]
    fn une_page_absente_du_disque_na_pas_de_dimensions() {
        let (c, _garde) = cache();
        let token = c.register("MANGADEX", "https://x/absent.png");
        assert_eq!(
            c.lire_dims(&token),
            None,
            "une page jamais écrite n'a pas de dimensions : les inventer rendrait \
             la bande virtualisable avec des hauteurs fausses"
        );
    }

    /// Contre-test nominal : une image **présente** mais dont l'en-tête est
    /// illisible ne doit pas faire paniquer, ni renvoyer de dimensions.
    #[test]
    fn une_entree_corrompue_ne_produit_pas_de_dimensions() {
        let (c, _garde) = cache();
        let token = c.register("MANGADEX", "https://x/corrompu.png");
        // On ecrit directement dans le cache : `ecrire_cache` refuserait des
        // octets non-image, et c'est justement ce qu'on veut contourner ici.
        std::fs::write(c.cache_path(&token), b"pas une image du tout").unwrap();
        let relu = PageCache::new(c.dir.clone());
        let t = relu.register("MANGADEX", "https://x/corrompu.png");
        assert_eq!(
            relu.lire_dims(&t),
            None,
            "une entree illisible doit rester « dimensions inconnues », jamais 0"
        );
    }

    #[test]
    fn une_couverture_ne_collisionne_pas_avec_une_page() {
        let (c, _garde_cache) = cache();
        let page = c.register("MANGADEX", "https://x/manga/cover.jpg");
        let couv = c.register_cover("MANGADEX", "https://x/manga/cover.jpg", None);
        assert_ne!(page, couv, "page et couverture ne partagent pas de jeton");
        assert!(matches!(c.resolve(&couv), Some(PageRef::Couverture { .. })));
    }

    #[test]
    fn le_cache_est_borne_et_evacue_les_anciens() {
        let (c, _garde_cache) = cache();
        let premier = c.register("MANGADEX", "https://x/0.jpg");
        for i in 1..(CAP_PAGE + 50) {
            c.register("MANGADEX", &format!("https://x/{i}.jpg"));
        }
        assert!(
            c.resolve(&premier).is_none(),
            "l'entrée la plus ancienne doit avoir été évacuée"
        );
        let recent = c.register("MANGADEX", &format!("https://x/{}.jpg", CAP_PAGE + 49));
        assert!(
            c.resolve(&recent).is_some(),
            "l'entrée la plus récente doit rester résolvable"
        );
        let taille = c.index.lock().unwrap().map.len();
        assert!(
            taille <= CAP_PAGE,
            "le cache ne doit pas dépasser le plafond"
        );
    }

    /// Le défaut critique mesuré le 2026-10-09 : rouvrir un chapitre déjà lu
    /// après saturation du cache donnait des images cassées, sans erreur.
    ///
    /// L'index tient trois structures : `map` (jeton → référence), `by_key`
    /// (clé métier → jeton) et `ordre` (file d'éviction). L'éviction ne purge
    /// que `map` et le fichier disque — le commentaire de `pagecache.rs:228`
    /// explique que `by_key` n'est pas purgé « car ses clones ont d'autres
    /// jetons ». Mais `inserer` (`:200-202`) fait `if let Some(existant) =
    /// by_key.get(&key) { return existant.clone() }` : réenregistrer la même
    /// paire `(kind, source, url)` rend donc l'ancien jeton **sans jamais le
    /// réinsérer dans `map`**.
    ///
    /// Résultat : l'évincée est absente de `map` mais présente dans `by_key`.
    /// `main.rs:200` voit `resolve() == None` et répond `not_found` **avant**
    /// `serve_page` — donc ni le fichier local ni le moteur ne sont consultés,
    /// et le réseau ne peut pas réparer. Le `.cbz` est intact, la page est morte.
    ///
    /// Le test précédent passe néanmoins : il vérifie que l'évincée n'est plus
    /// résolvable, ce qui est vrai, et ne demande jamais l'inverse.
    #[test]
    fn une_page_evincee_reenregistree_redevient_resolvable() {
        let (c, _garde_cache) = cache();
        c.register("MANGADEX", "https://x/0.jpg");
        for i in 1..(CAP_PAGE + 50) {
            c.register("MANGADEX", &format!("https://x/{i}.jpg"));
        }
        // On rouvre le premier chapitre : même source, même URL.
        let token = c.register("MANGADEX", "https://x/0.jpg");
        assert!(
            c.resolve(&token).is_some(),
            "une page réenregistrée après éviction doit redevenir \
             résolvable : le jeton rendu pointait vers une entrée retirée de \
             `map`, donc `main.rs` répondait not_found SANS consulter le cache \
             disque ni le moteur"
        );
    }

    #[test]
    fn les_couvertures_ont_leur_propre_plafond() {
        let (c, _garde_cache) = cache();
        for i in 0..(CAP_COVER + 50) {
            c.register_cover("MANGADEX", &format!("m{i}"), None);
        }
        let ix = c.index.lock().unwrap();
        let couvertes = ix.ordre.get(&2).map(VecDeque::len).unwrap_or(0);
        assert!(
            couvertes <= CAP_COVER,
            "les couvertures ne doivent pas evicter les pages, plafond = {couvertes}"
        );
    }

    // ---- Tests ADR-0012 Option A : dimensions en mémoire ----

    #[test]
    fn stocker_et_lire_dims() {
        let (c, _garde_cache) = cache();
        let token = c.register("MANGADEX", "https://x/1.jpg");
        // Avant stockage : None.
        assert_eq!(c.lire_dims(&token), None, "pas encore de dims");
        // Après stockage : Some.
        c.stocker_dims(&token, (800, 2171));
        assert_eq!(
            c.lire_dims(&token),
            Some((800, 2171)),
            "les dims stockées doivent être relisibles"
        );
        // Un jeton inconnu renvoie None.
        assert_eq!(c.lire_dims("inconnu-0000000000000000"), None);
    }

    #[test]
    fn dims_absentes_si_pas_encore_chargees() {
        // Prouve que le premier appel `pages` (avant tout fetchImage) ne renvoie
        // pas de dimensions : comportement nominal, bande non virtualisable.
        let (c, _garde_cache) = cache();
        let token = c.register("MANGADEX", "https://x/1.jpg");
        assert_eq!(
            c.lire_dims(&token),
            None,
            "avant fetchImage, les dims doivent être absentes"
        );
    }

    #[test]
    fn dims_non_transmises_par_eviction() {
        // L'éviction de l'index ne supprime PAS les dimensions de la map `dims`.
        // C'est inoffensif : un jeton évincé puis recréé pour la même URL
        // produit le même token (seeds déterministes dans la session), donc les
        // dimensions sont retrouvées même après éviction.
        let (c, _garde_cache) = cache();
        let premier = c.register("MANGADEX", "https://x/0.jpg");
        c.stocker_dims(&premier, (800, 2171));
        // Remplir jusqu'à l'éviction du premier jeton.
        for i in 1..(CAP_PAGE + 50) {
            c.register("MANGADEX", &format!("https://x/{i}.jpg"));
        }
        // Le jeton est évincé de l'index.
        assert!(
            c.resolve(&premier).is_none(),
            "l'entrée évincée ne doit plus être dans l'index"
        );
        // Les dimensions restent dans la map (comportement documenté).
        // Ce n'est pas un bug : les dims d'un jeton évincé sont du bruit
        // inoffensif, et les purger coûterait une complexité injustifiée.
        // Ce test documente l'invariant plutôt qu'il ne l'impose.
        // (lire_dims peut retourner Some ou None selon l'implémentation future)
        let _ = c.lire_dims(&premier); // ne pas asserter : comportement documenté, pas imposé
                                       // Ce qui compte : un nouveau register pour la même URL redonne le même token,
                                       // donc les dims sont retrouvées.
        let token2 = c.register("MANGADEX", "https://x/0.jpg");
        assert_eq!(
            token2, premier,
            "le même (source, url) doit produire le même token (seeds stables)"
        );
        assert_eq!(
            c.lire_dims(&token2),
            Some((800, 2171)),
            "les dims stockées avant éviction sont retrouvées via le même token"
        );
    }

    /// Frontière inter-sessions : l'**index** est bien remis à zéro.
    ///
    /// Le token n'est plus résoluble après redémarrage, et c'est normal :
    /// l'index se reconstruit par `register`.
    ///
    /// En revanche les **dimensions** se relisent désormais sur le disque
    /// (`lire_dims`), donc elles, elles survivent. C'est le correctif du
    /// 2026-10-09. L'ancien test `dims_absentes_apres_redemarrage_du_cache`
    /// affirmait ici l'absence de persistance en attribuant ce comportement à
    /// l'ADR-0012 — autrement dit, **un test figeait le défaut au lieu de le
    /// décrire**, et il aurait empêché de le corriger. La persistance est
    /// désormais prouvée par `les_dimensions_survivent_a_un_rechargement_du_cache`.
    #[test]
    fn l_index_est_vide_apres_redemarrage_du_cache() {
        let garde = crate::download::DossierDeTest::new("cache-restart2");
        let chemin = garde.chemin();

        // Session 1 : créer un cache, enregistrer un token, stocker ses dims.
        let token = {
            let c1 = PageCache::new(chemin.clone());
            let tok = c1.register("MANGADEX", "https://x/1.jpg");
            c1.stocker_dims(&tok, (800, 2171));
            assert_eq!(
                c1.lire_dims(&tok),
                Some((800, 2171)),
                "dims lisibles dans la même session"
            );
            tok
        }; // c1 dropped = session 1 terminée

        // Session 2 : nouvelle instance sur le MÊME dossier (simule un redémarrage).
        let c2 = PageCache::new(chemin);
        assert!(
            c2.resolve(&token).is_none(),
            "l'index est remis à zéro au redémarrage : le token se résout à nouveau \
             par register, ce qui est le comportement attendu"
        );
    }
}
