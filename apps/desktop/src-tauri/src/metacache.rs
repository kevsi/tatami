//! Cache disque des **métadonnées** de source : listes `chapters`, `pages`,
//! `details`, `latest`.
//!
//! **Pourquoi ce module existe.** Mesuré le 2026-10-19 sur l'artefact livré :
//! tout ce qui est local répond en millisecondes (rechargement 119 ms, relecture
//! d'un chapitre 18-119 ms, bascule pré-chargée 10 ms) et tout ce qui reste lent
//! est le réseau des sources — `chapters` 3,2 s, `pages` 3,5 s, `latest`
//! 6,4-10,8 s, `details` 6,5 s. Ces secondes étaient repayées **à chaque
//! nouvelle session**, parce que les listes ne vivaient qu'en mémoire côté
//! interface (`page.tsx`, `cacheChapitres`/`cachePages`). Le cache d'images du
//! protocole (`pagecache`) couvrait déjà les octets ; ce module couvre les
//! listes, au même endroit sur le disque.
//!
//! **Un fichier par clé**, comme `pagecache` : le nom est l'empreinte de
//! `(genre, cle)`, donc aucune donnée de source ne touche le système de
//! fichiers (une cle qui contient `\`, `..` ou un titre n'écrit jamais un
//! chemin). L'enveloppe porte le genre et la cle en clair : une collision
//! d'empreinte ou une version antérieure se lit comme un **manque**, jamais
//! comme une donnée d'un autre manga.
//!
//! **Ce module ne décide pas la fraîcheur.** Il rend ce qu'il a et son instant
//! d'écriture (`at`) ; c'est la commande appelante qui décide de servir ou de
//! forcer le réseau (`refresh`), et l'interface qui décide de rafraîchir en
//! arrière-plan (champ `cacheAt`). Un cache avec une politique serait deux
//! endroits qui décident de la même chose.
//!
//! **C'est un cache jetable.** Borne volontaire : purge au démarrage des
//! fichiers plus vieux que `AGE_MAX_MS`, et plafond par genre sur les plus
//! récents. La supprimer à la main (dossier `cache/meta`) ne perd rien
//! d'irremplaçable — c'est ce que la preuve rouge/verte utilise pour vérifier
//! que c'est bien lui qui rend les secondes session rapides.

use std::collections::HashMap;
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::UNIX_EPOCH;

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Durée de vie maximale d'une entrée, en millisecondes. Au-delà, la liste
/// relève d'une autre époque (nouveaux chapitres, URLs signées) : la resservir
/// ferait plus de mal que de bien, et le réseau de la source la rafraîchit en
/// une requête. 7 jours = « la semaine dernière » est encore plausible.
const AGE_MAX_MS: i64 = 7 * 24 * 3600 * 1000;

/// Plafond de l'index de session chapitre→manga (cf. `indexer_chapitres`).
/// Ordre de grandeur : une fiche ≈ 100-2 000 chapitres, donc 50 000 couvre des
/// dizaines de fiches par session (quelques Mo au pire).
const PLAFOND_INDEX_CHAPITRES: usize = 50_000;

/// Genres de métadonnées cachées. Chaque genre a son préfixe de fichier et son
/// plafond — les ordres de grandeur viennent des tailles mesurées : une liste
/// de 100 chapitres ≈ 98 ko, une liste de pages ≈ 4-50 ko, une page de
/// catalogue ≈ 20-50 ko.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Genre {
    Chapters,
    Details,
    Pages,
    Latest,
}

impl Genre {
    /// Préfixe du nom de fichier. Court, stable, jamais réutilisé pour un autre
    /// genre — deux préfixes identiques feraient compter les plafonds ensemble.
    fn prefixe(self) -> &'static str {
        match self {
            Genre::Chapters => "ch",
            Genre::Details => "dt",
            Genre::Pages => "pg",
            Genre::Latest => "lt",
        }
    }

    /// Plafond du nombre de fichiers du genre. Ordres de grandeur : la même
    /// échelle que le cache d'images (2048 pages, 384 couvertures), en plus
    /// généreux pour `pages` parce qu'un utilisateur lit beaucoup de chapitres
    /// et peu de catalogues.
    fn plafond(self) -> usize {
        match self {
            Genre::Chapters => 300,
            Genre::Details => 300,
            Genre::Pages => 2000,
            Genre::Latest => 150,
        }
    }
}

/// Enveloppe écrite sur le disque. `genre` et `cle` sont relus et comparés à
/// l'attente : une empreinte identique pour deux clés différentes (collision)
/// ou un fichier d'une version antérieure se solde par un **manque**.
#[derive(Serialize, Deserialize)]
struct Enveloppe {
    genre: String,
    cle: String,
    at: i64,
    donnees: Value,
}

pub struct MetaCache {
    dir: PathBuf,
    /// Index de session `(source, url de chapitre)` → `(url du manga, titre)`.
    /// Jamais écrit sur disque : vide après un redémarrage, c'est voulu — il ne
    /// sert qu'aux refus du moteur pendant une même session (cf.
    /// `indexer_chapitres`, et `commands.rs::pages` qui l'interroge).
    chapitres: Mutex<HashMap<String, (String, String)>>,
}

impl MetaCache {
    /// Ouvre (crée) le dossier et purge : d'abord les fichiers plus vieux que
    /// `AGE_MAX_MS`, ensuite chaque genre au-delà de son plafond en gardant les
    /// plus récents. La purge est ici et pas « à la lecture » : un cache se
    /// range quand on l'ouvre, pas à chaque affichage.
    pub fn new(dir: PathBuf) -> Arc<Self> {
        // Sans cette création, toute écriture échouerait en silence et le cache
        // ne servirait jamais — un cache qui ne s'écrit pas ne se voit qu'à la
        // deuxième session.
        let _ = fs::create_dir_all(&dir);
        let cache = Self {
            dir,
            chapitres: Mutex::new(HashMap::new()),
        };
        cache.purger(None);
        Arc::new(cache)
    }

    /// Indexe les chapitres d'une réponse `chapters` : `(source, url de
    /// chapitre)` → `(url du manga, titre)`.
    ///
    /// **À quoi ça sert.** Le moteur ne reconnaît un chapitre que si la fiche a
    /// été listée **pendant sa session** (son cache est en mémoire). Quand le
    /// cache disque sert `chapters` sans toucher le moteur, `pages` répond
    /// « unknown chapter url » ; cet index est ce qui permet à
    /// `commands.rs::pages` de retrouver le manga à réamorcer. Il est alimenté
    /// dans les **deux** chemins (cache comme réseau) — un index qui ne se
    /// remplirait qu'au réseau couvrirait exactement le cas où il ne sert pas.
    pub fn indexer_chapitres(&self, source: &str, manga_url: &str, titre: &str, reponse: &Value) {
        self.indexer_chapitres_plafond(source, manga_url, titre, reponse, PLAFOND_INDEX_CHAPITRES)
    }

    /// Même chose avec un plafond injectable — pour les tests, qui n'insèrent
    /// pas 50 000 entrées (même motif que `evacuer`).
    fn indexer_chapitres_plafond(
        &self,
        source: &str,
        manga_url: &str,
        titre: &str,
        reponse: &Value,
        plafond: usize,
    ) {
        let Some(arr) = reponse.get("chapters").and_then(Value::as_array) else {
            return;
        };
        let mut index = self.verrou_index();
        // Un index de session est une optimisation : le vider d'un bloc à
        // saturation est correct (au pire, le prochain refus du moteur repart
        // sans amorçage) et tient en une ligne, là où une éviction fine
        // demanderait une structure dédiée pour un chemin rare.
        if index.len() + arr.len() > plafond {
            index.clear();
        }
        for c in arr {
            if let Some(url) = c.get("url").and_then(Value::as_str) {
                index.insert(
                    format!("{source}|{url}"),
                    (manga_url.to_string(), titre.to_string()),
                );
            }
        }
    }

    /// Le manga d'un chapitre, tel que vu par la dernière liste qui l'a
    /// contenue. `None` = jamais vu cette session : l'appelant doit rendre le
    /// refus du moteur tel quel plutôt que d'inventer un manga.
    pub fn manga_du_chapitre(&self, source: &str, chapter_url: &str) -> Option<(String, String)> {
        self.verrou_index()
            .get(&format!("{source}|{chapter_url}"))
            .cloned()
    }

    /// Verrou de l'index, immunisé contre l'empoisonnement : un panic d'un
    /// autre fil ne doit pas transformer une lecture d'index en panne du cache.
    fn verrou_index(&self) -> std::sync::MutexGuard<'_, HashMap<String, (String, String)>> {
        self.chapitres.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Relit une entrée. Rend `None` dans tous les cas douteux : fichier
    /// absent, illisible, JSON invalide, enveloppe d'un autre genre ou d'une
    /// autre cle. Un fichier corrompu est supprimé — il ne sera jamais valide,
    /// et l'espace revient.
    pub fn lire(&self, genre: Genre, cle: &str) -> Option<(i64, Value)> {
        let chemin = self.chemin(genre, cle);
        let texte = fs::read_to_string(&chemin).ok()?;
        let Ok(env) = serde_json::from_str::<Enveloppe>(&texte) else {
            let _ = fs::remove_file(&chemin);
            tracing::warn!("meta corrompu, jete : {}", chemin.display());
            return None;
        };
        if env.genre != nom_genre(genre) || env.cle != cle {
            // Empreinte partagée (collision) ou fichier d'une version où la
            // convention de nommage différait. Ce n'est pas notre donnée.
            let _ = fs::remove_file(&chemin);
            return None;
        }
        Some((env.at, env.donnees))
    }

    /// Écrit une entrée (remplace toute entrée existante de la même cle), puis
    /// fait respecter le plafond du genre. Silencieux en cas d'échec disque :
    /// un cache qui s'écrit mal ne doit pas casser une réponse réseau correcte.
    pub fn ecrire(&self, genre: Genre, cle: &str, donnees: &Value) {
        let env = Enveloppe {
            genre: nom_genre(genre).to_string(),
            cle: cle.to_string(),
            at: crate::store::now_ms(),
            donnees: donnees.clone(),
        };
        let Ok(octets) = serde_json::to_vec(&env) else {
            return;
        };
        if let Err(e) = fs::write(self.chemin(genre, cle), octets) {
            tracing::warn!("meta non ecrit ({e})");
            return;
        }
        self.evacuer(genre, genre.plafond());
    }

    /// Chemin : `meta/<prefixe>_<empreinte>.json`. L'empreinte est celle de
    /// `(genre, cle)` — jamais la cle elle-même, qui peut contenir un titre,
    /// des `/` et des `\`.
    fn chemin(&self, genre: Genre, cle: &str) -> PathBuf {
        use std::collections::hash_map::DefaultHasher;
        let mut h = DefaultHasher::new();
        nom_genre(genre).hash(&mut h);
        cle.hash(&mut h);
        self.dir
            .join(format!("{}_{:016x}.json", genre.prefixe(), h.finish()))
    }

    /// Purge : supprime les fichiers plus vieux que `AGE_MAX_MS`, puis, par
    /// genre, les plus anciens au-delà du plafond. `maintenant` est injectable
    /// pour les tests ; en production c'est l'heure courante.
    fn purger(&self, maintenant: Option<i64>) {
        let maintenant = maintenant.unwrap_or_else(crate::store::now_ms);
        let Ok(lecture) = fs::read_dir(&self.dir) else {
            return;
        };
        // (mtime_ms, chemin) par genre.
        let mut par_genre: Vec<(Genre, Vec<(i64, PathBuf)>)> = Vec::new();
        for entree in lecture.flatten() {
            let chemin = entree.path();
            let Some(genre) = genre_du_fichier(&chemin) else {
                continue;
            };
            let Ok(meta) = entree.metadata() else {
                continue;
            };
            let Ok(mtime) = meta.modified() else {
                continue;
            };
            let ms = mtime
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0);
            if maintenant - ms > AGE_MAX_MS {
                let _ = fs::remove_file(&chemin);
                continue;
            }
            match par_genre.iter_mut().find(|(g, _)| *g == genre) {
                Some((_, liste)) => liste.push((ms, chemin)),
                None => par_genre.push((genre, vec![(ms, chemin)])),
            }
        }
        let mut jetes = 0usize;
        for (genre, mut liste) in par_genre {
            let plafond = genre.plafond();
            if liste.len() > plafond {
                liste.sort_by_key(|(ms, _)| *ms);
                for (_, chemin) in liste.drain(..liste.len() - plafond) {
                    let _ = fs::remove_file(&chemin);
                    jetes += 1;
                }
            }
        }
        if jetes > 0 {
            tracing::info!("meta purge : {jetes} fichier(s)");
        }
    }

    /// Évacue les plus anciens d'un genre au-delà de `plafond`. Le plafond est
    /// un paramètre (et pas seulement `genre.plafond()`) pour que les tests
    /// puissent vérifier l'ordre d'évacuation sans écrire 300 fichiers.
    fn evacuer(&self, genre: Genre, plafond: usize) {
        let Ok(lecture) = fs::read_dir(&self.dir) else {
            return;
        };
        let mut liste: Vec<(i64, PathBuf)> = Vec::new();
        for entree in lecture.flatten() {
            let chemin = entree.path();
            if genre_du_fichier(&chemin) != Some(genre) {
                continue;
            }
            let Ok(meta) = entree.metadata() else {
                continue;
            };
            let Ok(mtime) = meta.modified() else {
                continue;
            };
            let ms = mtime
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_millis() as i64)
                .unwrap_or(0);
            liste.push((ms, chemin));
        }
        if liste.len() <= plafond {
            return;
        }
        liste.sort_by_key(|(ms, _)| *ms);
        for (_, chemin) in liste.drain(..liste.len() - plafond) {
            let _ = fs::remove_file(&chemin);
        }
    }
}

/// Nom du genre dans l'enveloppe — lisible, distinct du préfixe de fichier.
fn nom_genre(genre: Genre) -> &'static str {
    match genre {
        Genre::Chapters => "chapters",
        Genre::Details => "details",
        Genre::Pages => "pages",
        Genre::Latest => "latest",
    }
}

/// Genre déduit du nom de fichier, ou `None` pour tout autre fichier (étranger
/// au module — il n'est jamais touché).
fn genre_du_fichier(chemin: &std::path::Path) -> Option<Genre> {
    let nom = chemin.file_name()?.to_str()?;
    [
        (Genre::Chapters, "ch_"),
        (Genre::Details, "dt_"),
        (Genre::Pages, "pg_"),
        (Genre::Latest, "lt_"),
    ]
    .into_iter()
    .find(|(_, prefixe)| nom.starts_with(prefixe) && nom.ends_with(".json"))
    .map(|(genre, _)| genre)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::time::{Duration, SystemTime};

    fn cache() -> (Arc<MetaCache>, crate::download::DossierDeTest) {
        let garde = crate::download::DossierDeTest::new("meta");
        // Pas `env::temp_dir()` : sur cette machine TEMP n'est pas expansé et
        // résout un chemin littéral contre le dossier courant (leçon AGENTS.md).
        (MetaCache::new(garde.chemin()), garde)
    }

    fn nb_fichiers(c: &MetaCache) -> usize {
        fs::read_dir(&c.dir)
            .map(|l| l.flatten().count())
            .unwrap_or(0)
    }

    /// Vieillit un fichier en reculant son `mtime` — `File::set_modified` est
    /// stable depuis Rust 1.75, donc la purge d'âge se teste pour de vrai, sans
    /// dépendance de test supplémentaire.
    fn vieillir(chemin: &std::path::Path, jours: u64) {
        let quand = SystemTime::now() - Duration::from_secs(jours * 24 * 3600);
        let f = fs::OpenOptions::new()
            .write(true)
            .open(chemin)
            .expect("ouvrir");
        f.set_modified(quand).expect("mtime");
    }

    #[test]
    fn un_ecrit_se_relit() {
        let (c, _garde) = cache();
        let donnees = json!({ "chapters": [{ "url": "https://x/1", "title": "Ch. 1" }] });
        c.ecrire(Genre::Chapters, "SUSHISCANFR|https://x/manga", &donnees);
        let (at, relu) = c
            .lire(Genre::Chapters, "SUSHISCANFR|https://x/manga")
            .expect("relu");
        assert_eq!(relu, donnees);
        assert!(at > 0, "l'instant d'écriture doit être renseigné");
    }

    #[test]
    fn une_autre_cle_ou_un_autre_genre_est_un_manque() {
        let (c, _garde) = cache();
        c.ecrire(Genre::Chapters, "A|u1", &json!({ "chapters": [] }));
        assert!(c.lire(Genre::Chapters, "A|u2").is_none(), "autre cle");
        assert!(c.lire(Genre::Details, "A|u1").is_none(), "autre genre");
        // Le cas nominal reste, lui, servi : un test qui ne vérifie que des
        // refus ne prouve pas qu'un cas légitime passe.
        assert!(c.lire(Genre::Chapters, "A|u1").is_some());
    }

    #[test]
    fn un_fichier_corrompu_est_un_manque_et_est_jete() {
        let (c, _garde) = cache();
        c.ecrire(Genre::Pages, "S|u", &json!({ "pages": [] }));
        let chemin = c.chemin(Genre::Pages, "S|u");
        fs::write(&chemin, b"{ pas du json").unwrap();
        assert!(c.lire(Genre::Pages, "S|u").is_none());
        assert!(
            !chemin.exists(),
            "un fichier corrompu est supprimé, pas relu en boucle"
        );
    }

    #[test]
    fn une_cle_hostile_n_ecrit_jamais_hors_du_dossier() {
        let (c, _garde) = cache();
        // La cle peut contenir un titre de manga : séparateurs Windows, `..`,
        // tout y passe. Le nom de fichier est une empreinte, donc rien de la
        // cle n'atteint le chemin.
        let cle = r"X|..\..\evil/:\0 et un titre très long ".repeat(20);
        c.ecrire(Genre::Chapters, &cle, &json!({ "ok": true }));
        assert!(c.lire(Genre::Chapters, &cle).is_some());
        assert_eq!(
            nb_fichiers(&c),
            1,
            "un seul fichier, dans le dossier du cache"
        );
    }

    #[test]
    fn la_purge_d_age_jette_le_vieux_et_garde_le_recent() {
        let (c, _garde) = cache();
        c.ecrire(Genre::Chapters, "vieux", &json!({ "n": 1 }));
        c.ecrire(Genre::Chapters, "recent", &json!({ "n": 2 }));
        vieillir(&c.chemin(Genre::Chapters, "vieux"), 8);
        c.purger(None);
        assert!(
            c.lire(Genre::Chapters, "vieux").is_none(),
            "8 jours > 7 : jeté"
        );
        assert!(
            c.lire(Genre::Chapters, "recent").is_some(),
            "du jour : gardé"
        );
    }

    #[test]
    fn le_plafond_evacue_le_surplus_par_ordre_d_anciennete() {
        let (c, _garde) = cache();
        c.ecrire(Genre::Pages, "a", &json!({ "n": 1 }));
        c.ecrire(Genre::Pages, "b", &json!({ "n": 2 }));
        vieillir(&c.chemin(Genre::Pages, "a"), 3);
        c.ecrire(Genre::Pages, "c", &json!({ "n": 3 }));
        vieillir(&c.chemin(Genre::Pages, "c"), 2);
        c.ecrire(Genre::Pages, "d", &json!({ "n": 4 }));
        vieillir(&c.chemin(Genre::Pages, "d"), 1);
        vieillir(&c.chemin(Genre::Pages, "b"), 0);
        // 4 entrées, plafond 2 : le surplus exact part par ancienneté — « a »
        // puis « c » — et les deux plus récentes restent. Une évacuation qui
        // jetterait tout ne serait pas un plafond.
        c.evacuer(Genre::Pages, 2);
        assert!(c.lire(Genre::Pages, "a").is_none(), "la plus ancienne part");
        assert!(
            c.lire(Genre::Pages, "c").is_none(),
            "la suivante part aussi"
        );
        assert!(c.lire(Genre::Pages, "d").is_some(), "la troisième reste");
        assert!(c.lire(Genre::Pages, "b").is_some(), "la plus récente reste");
    }

    #[test]
    fn la_purge_ne_touche_pas_les_fichiers_etrangers() {
        let (c, _garde) = cache();
        let etranger = c.dir.join("autre.txt");
        fs::write(&etranger, b"x").unwrap();
        c.ecrire(Genre::Pages, "a", &json!({ "n": 1 }));
        vieillir(&etranger, 30);
        c.purger(None);
        assert!(
            etranger.exists(),
            "un fichier qui n'est pas du module n'est jamais purgé"
        );
    }

    #[test]
    fn l_index_retrouve_le_manga_d_un_chapitre() {
        let (c, _garde) = cache();
        c.indexer_chapitres(
            "SUSHISCANFR",
            "https://x/manga",
            "Un manga",
            &json!({ "chapters": [{ "url": "https://x/manga/ch-1" }, { "url": "https://x/manga/ch-2" }] }),
        );
        assert_eq!(
            c.manga_du_chapitre("SUSHISCANFR", "https://x/manga/ch-2"),
            Some(("https://x/manga".to_string(), "Un manga".to_string()))
        );
        // Un chapitre jamais vu, ou vu d'une autre source, n'a pas de manga :
        // le refus du moteur sera rendu tel quel, sans amorçage à l'aveugle.
        assert!(c
            .manga_du_chapitre("SUSHISCANFR", "https://x/manga/ch-9")
            .is_none());
        assert!(c
            .manga_du_chapitre("TOONFR", "https://x/manga/ch-1")
            .is_none());
    }

    #[test]
    fn un_index_sature_repart_de_zero() {
        let (c, _garde) = cache();
        // Le plafond est un paramètre : pas besoin d'insérer 50 000 entrées
        // pour vérifier la remise à zéro (même motif que `evacuer`).
        c.indexer_chapitres_plafond(
            "S",
            "m1",
            "t1",
            &json!({ "chapters": [{ "url": "u1" }, { "url": "u2" }] }),
            4,
        );
        assert!(c.manga_du_chapitre("S", "u1").is_some());
        // 2 + 3 > 4 : la saturation vide l'index entier, puis la liste
        // courante y entre — l'ancienne n'y est plus.
        c.indexer_chapitres_plafond(
            "S",
            "m2",
            "t2",
            &json!({ "chapters": [{ "url": "u3" }, { "url": "u4" }, { "url": "u5" }] }),
            4,
        );
        assert!(
            c.manga_du_chapitre("S", "u1").is_none(),
            "l'ancien index est parti"
        );
        assert_eq!(
            c.manga_du_chapitre("S", "u5"),
            Some(("m2".to_string(), "t2".to_string()))
        );
    }

    #[test]
    fn une_reponse_sans_chapitres_n_alimente_pas_l_index() {
        let (c, _garde) = cache();
        // Une réponse `pages` ou `details` passée par erreur, ou un échec du
        // moteur, ne doivent pas créer d'entrées fantômes.
        c.indexer_chapitres("S", "m", "t", &json!({ "pages": [{ "url": "u1" }] }));
        assert!(c.manga_du_chapitre("S", "u1").is_none());
    }
}
