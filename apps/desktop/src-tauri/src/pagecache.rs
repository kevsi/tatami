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
}

impl PageCache {
    pub fn new(dir: PathBuf) -> Arc<Self> {
        let _ = std::fs::create_dir_all(&dir);
        Arc::new(Self {
            index: Mutex::new(Index::vide()),
            dir,
        })
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
            if let Some(r) = ix.map.remove(&victime) {
                if r.est_cache() {
                    let _ = std::fs::remove_file(self.cache_path(&victime));
                }
            }
            // L'index inversé n'est pas purgé pour la victime : ses clones ont
            // d'autres jetons, et le reconstruire coûterait plus qu'il ne rend.
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
    // Un chapitre téléchargé se lit dans son `.cbz`, sans moteur ni réseau.
    if let PageRef::Local { cbz, entry } = page {
        return servir_local(cbz, *entry);
    }
    if let Some(bytes) = relire_cache(cache, token) {
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

    let value = sup
        .request(methode, params, std::time::Duration::from_secs(120))
        .await
        .map_err(|e| format!("engine: {e}"))?;
    let bytes = decoder(value, quoi)?;
    ecrire_cache(cache, token, &bytes, quoi)
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

    #[test]
    fn le_meme_url_redonne_le_meme_jeton() {
        let (c, _garde_cache) = cache();
        let a = c.register("MANGADEX", "https://x/1.jpg");
        let b = c.register("MANGADEX", "https://x/1.jpg");
        assert_eq!(a, b, "un URL identique doit réutiliser le jeton");
        // Une source différente = une autre image, donc un autre jeton.
        assert_ne!(a, c.register("MANGAFR", "https://x/1.jpg"));
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
}
