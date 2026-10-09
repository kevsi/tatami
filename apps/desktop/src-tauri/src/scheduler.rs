//! Planificateur : vérification périodique des nouveaux chapitres des favoris.
//!
//! Tâche 3.1 du `PLAN.md`. C'est la seule brique de la phase 3 qui porte de la
//! logique métier : le reste (flux « mises à jour », notifications système,
//! réglages) n'est que son affichage. Le module est donc écrit pour être testé
//! **sans moteur** — tout ce qui décide (quels favoris, dans quel ordre, quels
//! chapitres sont nouveaux, quand une source a le droit d'être interrogée) est
//! isolé du réseau, et le moteur n'est qu'un trait.
//!
//! ## Les trois règles qui structurent le module
//!
//! 1. **Rien n'est signalé deux fois, et rien n'est signalé à tort.** Un
//!    chapitre est identifié par son URL chez la source, jamais par son numéro
//!    ni par sa date : les sources réordonnent leurs listes. Le **premier
//!    relevé** d'un favori ne signale rien — sinon le premier lancement annonce
//!    les 286 chapitres d'une série, et les réannonce à chaque démarrage.
//! 2. **Débit borné par source** (AGENTS.md §10 : « limiter le débit par source
//!    […] et rester un client poli »). Un délai minimum sépare deux requêtes à
//!    la même source, et un échec **allonge** ce délai au lieu de répéter
//!    l'appel : une source morte ne doit pas être martelée.
//! 3. **Un succès aberrant n'est pas un succès.** Une source qui répond
//!    `{chapters: []}` pour une série qui en a ne doit pas vider l'état de
//!    référence : elle est comptée en échec et les chapitres connus sont
//!    conservés. C'est la mesure du 2026-10-04 sur un interstitiel d'âge servi
//!    en `200 OK` avec une liste d'URLs vides.
//!
//! ## L'heure est injectée
//!
//! `passe` ne lit pas l'horloge : elle reçoit une [`Horloge`]. C'est ce qui rend
//! le débit **mesurable** au lieu d'être promis — un test avance le temps de
//! 4,9 s puis de 0,2 s et vérifie qu'une requête passe exactement à la limite.
//! L'heure système n'est lue qu'au plus haut niveau, par [`HorlogeSysteme`].

use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, State};

use crate::commands::{into_result, DEEP_TIMEOUT};
use crate::store::{Favorite, Store};
use crate::supervisor::Supervisor;

/// Clé du réglage qui porte la configuration du planificateur.
const CLE_REGLAGES: &str = "planificateur.reglages";
/// Clé du réglage qui porte l'état persisté (chapitres vus, débit, horodatage).
const CLE_ETAT: &str = "planificateur.etat";

/// Intervalle entre deux passes, par défaut. La tâche 3.5 rend ce délai
/// réglable ; on part sur une valeur prudente (Kotatsu-Redo tourne à quelques
/// heures) plutôt que sur une valeur agressive.
pub const PERIODE_PAR_DEFAUT: Duration = Duration::from_secs(6 * 3600);
/// Plancher de l'intervalle, quelle que soit la valeur enregistrée. Sans ce
/// plancher, un réglage erroné (ou un `0`) transformerait l'application en
/// client qui martèle les sites — le contraire de ce qu'un manga reader doit
/// être.
pub const PERIODE_MIN: Duration = Duration::from_secs(5 * 60);

/// Délai minimum entre deux requêtes à la **même** source, par défaut.
pub const DELAI_SOURCE_PAR_DEFAUT: Duration = Duration::from_secs(20);
/// Plancher du délai par source : en dessous, on n'est plus un client poli.
pub const DELAI_SOURCE_MIN: Duration = Duration::from_secs(5);

/// **Plafonds**, en un seul endroit.
///
/// Ils vivaient en littéraux dans `bornes()`, et l'écran de 3.5 avait besoin de
/// les connaître pour afficher `max`. Les recopier ailleurs avait déjà produit
/// 200 au lieu de 5 000 dans la vue : deux listes de bornes finissent toujours par
/// diverger. Elles sont donc nommées, et `bornes()` s'y réfère — donc la borne
/// affichée et la borne appliquée ne peuvent pas différer.
///
/// `bornes()` **écrase** les valeurs au lieu de seulement les lire : s'en servir
/// pour *lire* un maximum donnerait la valeur courante, pas le maximum.
pub const PERIODE_MAX_S: u64 = 24 * 3600;
pub const DELAI_SOURCE_MAX_S: u64 = 3600;
pub const MAX_PAR_PASSE: usize = 5000;
pub const MAX_SOURCES_PAR_PASSE: usize = 200;
/// Plafond du retour arrière après échecs, pour ne pas laisser une source
/// morte de côté pendant des jours.
pub const RETOUR_ARRIERE_MAX: Duration = Duration::from_secs(30 * 60);
/// Nombre d'échecs consécutifs au-delà duquel l'attente ne croît plus.
const ECHECS_PLAFOND: u32 = 6;

/// Favoris vérifiés par passe, par défaut. Un utilisateur a rarement plus de
/// quelques centaines de favoris ; au-delà, la passe n'aurait pas le temps de
/// finir avant la suivante.
pub const MAX_PAR_PASSE_PAR_DEFAUT: usize = 200;
/// Sources **distinctes** touchées par passe, par défaut. C'est la borne qui
/// protège le moteur — et les 12 Go de la machine — quand la bibliothèque
/// couvre 1370 catalogues.
pub const MAX_SOURCES_PAR_PASSE_PAR_DEFAUT: usize = 20;

/// Chapitres mémorisés par favori.
///
/// Au-delà, la fenêtre est tronquée dans l'ordre de la source. Une série de
/// plus de 2 000 chapitres est théorique (record mesuré sur ce projet : 286) ;
/// le plafond existe pour qu'un cas dégénéré ne fasse pas grossir un réglage
/// sans borne. Limite assumée : sur une série plus longue, un chapitre peut
/// être signalé une seconde fois quand il retombe hors de la fenêtre.
pub const MAX_VUS: usize = 2000;

/// Nouveautés effectivement annoncées par favori. Au-delà, l'interface est
/// censée écrire « et N de plus » — c'est le rôle de 3.2, pas celui d'ici.
pub const MAX_NOVEAUX_RAPPORTES: usize = 10;

/// Motif porté par un échec qui n'est pas une erreur du moteur mais un
/// résultat aberrant. Distinct d'une chaîne vide : une source qui répond
/// `{chapters: []}` n'a **pas** vérifié le favori.
const MOTIF_AUCUN_CHAPITRE: &str = "aucun chapitre renvoyé";

// =========================================================================
// Réglages
// =========================================================================

/// Configuration du planificateur, persistée dans la table `settings`.
///
/// Les durées sont stockées en **secondes** : un registre sérialise cela
/// lisiblement, et c'est une unité que l'interface sait afficher. Le
/// `#[serde(default)]` au niveau du conteneur fait que chaque champ absent
/// prend sa valeur par défaut : un réglage écrit par une version antérieure, ou
/// tronqué, reste lisible au lieu de faire échouer le chargement.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Reglages {
    /// Intervalle entre deux passes, en secondes. Borné par [`PERIODE_MIN`].
    pub periode_s: u64,
    /// Délai minimum entre deux requêtes à la même source, en secondes.
    pub delai_source_s: u64,
    /// Favoris effectivement interrogés par passe. Les autres sont ignorés,
    /// pas perdus : ils remontent en tête à la passe suivante.
    pub max_par_passe: usize,
    /// Sources distinctes interrogées par passe.
    pub max_sources_par_passe: usize,
    /// Coupe toute requête. 3.5 expose ce réglage dans l'interface ; il est lu
    /// ici parce que l'arrêt doit être possible **sans** elle.
    pub actif: bool,
}

impl Default for Reglages {
    fn default() -> Self {
        Self {
            periode_s: PERIODE_PAR_DEFAUT.as_secs(),
            delai_source_s: DELAI_SOURCE_PAR_DEFAUT.as_secs(),
            max_par_passe: MAX_PAR_PASSE_PAR_DEFAUT,
            max_sources_par_passe: MAX_SOURCES_PAR_PASSE_PAR_DEFAUT,
            actif: true,
        }
    }
}

impl Reglages {
    pub fn periode(&self) -> Duration {
        Duration::from_secs(self.periode_s).max(PERIODE_MIN)
    }

    pub fn delai_source(&self) -> Duration {
        Duration::from_secs(self.delai_source_s).max(DELAI_SOURCE_MIN)
    }

    /// Force les planchers et les plafonds sur ce qui vient d'être lu.
    ///
    /// Le nom dit ce qu'il fait : `0` ou une valeur absurde ne doivent jamais
    /// produire une cadence agressive. Le plafond sur `max_par_passe` évite
    /// symétriquement de croire une valeur qui condamnerait la passe à
    /// n'interroger qu'un seul favori à vie.
    pub fn bornes(mut self) -> Self {
        self.periode_s = self.periode_s.clamp(PERIODE_MIN.as_secs(), PERIODE_MAX_S);
        self.delai_source_s = self
            .delai_source_s
            .clamp(DELAI_SOURCE_MIN.as_secs(), DELAI_SOURCE_MAX_S);
        self.max_par_passe = self.max_par_passe.clamp(1, MAX_PAR_PASSE);
        self.max_sources_par_passe = self.max_sources_par_passe.clamp(1, MAX_SOURCES_PAR_PASSE);
        self
    }

    /// Raccourci de test : cadence et plafonds explicites, sans borner.
    pub fn de_test(periode_s: u64, delai_source_s: u64, max_par_passe: usize) -> Self {
        Self {
            periode_s,
            delai_source_s,
            max_par_passe,
            max_sources_par_passe: MAX_SOURCES_PAR_PASSE_PAR_DEFAUT,
            actif: true,
        }
    }
}

/// Configuration en vigueur. Une chaîne **vide** ou un JSON invalide ne veut pas
/// dire « désactivé » : cela veut dire « réglage absent », donc défauts.
///
/// Une chaîne vide utilisée comme sentinelle est déjà un piège dans ce dépôt
/// (mesuré le 2026-10-05 sur le filtre de classification, où l'option « Toutes »
/// envoyait `SAFE` et vidait les sources adultes). Ici la règle est inverse et
/// explicite : seule la présence de la clé décide du comportement.
pub fn reglages(store: &Store) -> Reglages {
    match store.get_setting(CLE_REGLAGES) {
        Ok(Some(v)) if !v.trim().is_empty() => match serde_json::from_str::<Reglages>(&v) {
            Ok(r) => r.bornes(),
            Err(e) => {
                tracing::warn!("réglage du planificateur illisible ({e}), valeurs par défaut");
                Reglages::default()
            }
        },
        _ => Reglages::default(),
    }
}

// =========================================================================
// État persisté
// =========================================================================

/// Ce que le planificateur retient d'un favori.
///
/// `verifie_le` est la **seule** source de vérité sur « déjà vérifié » : pas de
/// drapeau `premier_releve` en plus, qui pourrait lui contredire. Une absence
/// totale (`{}` à la désérialisation) donne `verifie_le = 0`, donc un premier
/// relevé — c'est exactement le piège qu'un `#[serde(default)]` mal posé
/// transformerait en « tout est nouveau » dès la première lecture d'un état
/// écrit par une autre version.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Suivi {
    /// URLs de chapitres déjà vus, dans l'ordre où la source les a données.
    #[serde(default)]
    pub vus: Vec<String>,
    /// Dernière vérification **réussie**, epoch millis. `0` = jamais vérifié.
    #[serde(default)]
    pub verifie_le: i64,
    /// Message de la dernière vérification en échec, effacé au succès.
    #[serde(default)]
    pub erreur: Option<String>,
}

impl Suivi {
    /// Vrai tant que ce favori n'a jamais été vérifié avec succès.
    pub fn jamais_verifie(&self) -> bool {
        self.verifie_le == 0
    }

    /// Ajoute les chapitres vus à l'état, sans doublon, en bornant la fenêtre.
    fn ajouter(&mut self, chapitres: &[Chapitre]) {
        // Cas courant : rien à tronquer, un aller simple.
        if self.vus.len().saturating_add(chapitres.len()) <= MAX_VUS {
            let mut vus: BTreeSet<&str> = self.vus.iter().map(String::as_str).collect();
            let mut ajout: Vec<String> = Vec::with_capacity(chapitres.len());
            for c in chapitres {
                if vus.insert(c.url.as_str()) {
                    ajout.push(c.url.clone());
                }
            }
            self.vus.extend(ajout);
            return;
        }
        // Trop de chapitres : on garde d'abord la fenêtre de la réponse
        // courante, dans l'ORDRE DE LA SOURCE, puis ce qui n'y figure plus
        // (une source paginée ne renvoie pas tout à chaque passe).
        let ancien = std::mem::take(&mut self.vus);
        let mut vus: BTreeSet<&str> = BTreeSet::new();
        let mut gardes: Vec<String> = Vec::with_capacity(MAX_VUS);
        for u in chapitres
            .iter()
            .map(|c| c.url.as_str())
            .chain(ancien.iter().map(String::as_str))
        {
            if gardes.len() == MAX_VUS {
                break;
            }
            if vus.insert(u) {
                gardes.push(u.to_string());
            }
        }
        self.vus = gardes;
    }
}

/// État du débit retenu pour une source : quand elle peut être réinterrogée, et
/// combien elle a échoué d'affilée.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct EtatSource {
    /// Epoch millis de la prochaine requête autorisée.
    #[serde(default)]
    pub prochaine: i64,
    /// Échecs consécutifs, remis à zéro au succès.
    #[serde(default)]
    pub echecs: u32,
}

/// État complet du planificateur, tel que persisté dans `settings`.
///
/// Une seule clé, une lecture et une écriture par passe : la base tient un
/// `Mutex` (AGENTS.md §6) et il n'y a aucune raison de le prendre vingt fois
/// pour un tour de boucle.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Etat {
    /// Suivi par favori, indexé par [`cle`].
    pub suivis: BTreeMap<String, Suivi>,
    /// Epoch millis du dernier passage, quel qu'en soit le résultat.
    pub dernier_pass: i64,
    /// Compteurs de débit par source.
    pub debit: BTreeMap<String, EtatSource>,
}

impl Etat {
    pub fn suivi(&self, source: &str, manga_url: &str) -> Option<&Suivi> {
        self.suivis.get(&cle(source, manga_url))
    }

    fn suivre_mut(&mut self, source: &str, manga_url: &str) -> &mut Suivi {
        self.suivis.entry(cle(source, manga_url)).or_default()
    }
}

/// Clé d'un favori dans [`Etat::suivis`].
///
/// Le séparateur est `U+001F` (séparateur d'unités) : il n'apparaît ni dans un
/// nom de source ni dans une URL de manga, donc `("A", "B/C")` et `("A/B", "C")`
/// ne peuvent pas se confondre en une seule clé.
fn cle(source: &str, manga_url: &str) -> String {
    format!("{source}\u{1f}{manga_url}")
}

/// Lit l'état persisté. Un état absent ou illisible repart de zéro : repartir de
/// zéro ré-annoncera les chapitres d'un seul tour, alors que repartir d'un état à
/// moitié lu pourrait en cacher définitivement.
pub fn charger(store: &Store) -> Result<Etat, String> {
    match store.get_setting(CLE_ETAT) {
        Ok(Some(v)) if !v.trim().is_empty() => serde_json::from_str::<Etat>(&v)
            .map_err(|e| format!("état du planificateur illisible : {e}")),
        _ => Ok(Etat::default()),
    }
}

pub fn sauver(store: &Store, etat: &Etat) -> Result<(), String> {
    let v = serde_json::to_string(etat).map_err(|e| format!("état non sérialisable : {e}"))?;
    store.set_setting(CLE_ETAT, &v)
}

// =========================================================================
// Limites de débit
// =========================================================================

/// Qui décide, à l'instant présent, qu'une source peut être réinterrogée.
///
/// Le délai est **daté**, pas « calculé depuis le dernier appel » : c'est ce qui
/// rend l'état lisible et identique après un redémarrage — une source qui
/// échouait le reste au lieu de repartir à zéro.
struct Limites {
    delai: Duration,
    etat: BTreeMap<String, EtatSource>,
}

impl Limites {
    fn neuf(delai: Duration, etat: BTreeMap<String, EtatSource>) -> Self {
        Self { delai, etat }
    }

    /// La source a-t-elle le droit d'être interrogée à `maintenant` ?
    fn autorisee(&self, source: &str, maintenant: i64) -> bool {
        self.etat
            .get(source)
            .is_none_or(|e| maintenant >= e.prochaine)
    }

    /// Une requête a abouti : on arme le délai nominal et on oublie les échecs.
    fn succes(&mut self, source: &str, maintenant: i64) {
        self.etat.insert(
            source.to_string(),
            EtatSource {
                prochaine: maintenant + self.delai.as_millis() as i64,
                echecs: 0,
            },
        );
    }

    /// Une requête a échoué : on allonge l'attente, jusqu'au plafond.
    ///
    /// Le retour arrière est exponentiel **et** borné par
    /// [`RETOUR_ARRIERE_MAX`] : une source morte ne doit pas être quittée pour
    /// toujours non plus, sinon un correctif côté site ne serait jamais vu.
    fn echec(&mut self, source: &str, maintenant: i64) {
        let precedent = self.etat.get(source).map_or(0, |e| e.echecs);
        let echecs = precedent.saturating_add(1).min(ECHECS_PLAFOND);
        // `saturating_mul` puis `min` : le facteur `2^n` appliqué à un délai en
        // millisecondes déborderait `i64` bien avant qu'on atteigne le plafond.
        let facteur = 1u32 << (echecs - 1).min(31);
        let attente = Duration::from_millis(self.delai.as_millis() as u64)
            .saturating_mul(facteur)
            .min(RETOUR_ARRIERE_MAX);
        self.etat.insert(
            source.to_string(),
            EtatSource {
                prochaine: maintenant + attente.as_millis() as i64,
                echecs,
            },
        );
    }
}

// =========================================================================
// Horloge
// =========================================================================

/// Source d'heure du planificateur.
pub trait Horloge: Send + Sync {
    /// Epoch millis.
    fn maintenant_ms(&self) -> i64;
}

/// L'heure du système. Seul [`HorlogeSysteme`] lit le temps réel ; tout le
/// reste du module — donc tout test — passe par un [`Horloge`] injecté.
pub struct HorlogeSysteme;

impl Horloge for HorlogeSysteme {
    fn maintenant_ms(&self) -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0)
    }
}

// =========================================================================
// Moteur
// =========================================================================

/// Future d'une requête de chapitres.
///
/// Un `async fn` dans un trait rendrait le module dépendant d'`async-trait` — une
/// dépendance pour le seul bénéfice d'un sucre syntaxique. On écrit le type à la
/// main, ce qui tient en une ligne.
pub type RequeteChapters<'a> =
    std::pin::Pin<Box<dyn std::future::Future<Output = Result<Value, String>> + Send + 'a>>;

/// L'accès aux chapitres d'un manga, tel que le planificateur le consomme.
///
/// C'est le **seul** endroit où le module touche au moteur : tout le reste
/// travaille sur des valeurs, ce qui rend la passe testable sans JVM.
pub trait Motor: Send + Sync {
    fn chapitres<'a>(
        &'a self,
        source: &'a str,
        manga_url: &'a str,
        manga_title: &'a str,
    ) -> RequeteChapters<'a>;
}

/// Le moteur réel, derrière le superviseur.
struct MoteurSuperviseur {
    sup: Arc<Supervisor>,
}

impl Motor for MoteurSuperviseur {
    fn chapitres<'a>(
        &'a self,
        source: &'a str,
        manga_url: &'a str,
        manga_title: &'a str,
    ) -> RequeteChapters<'a> {
        Box::pin(async move {
            // `mangaTitle` n'est pas décoratif : il permet au moteur de
            // reconstruire le webtoon quand sa session est vide. Un
            // planificateur tourne justement dans ce cas — au démarrage de
            // l'application — donc sans ce champ chaque passe échouerait.
            let params = json!({
                "source": source,
                "mangaUrl": manga_url,
                "mangaTitle": manga_title,
            });
            let enveloppe = self
                .sup
                .request_with_retry("chapters", params, DEEP_TIMEOUT)
                .await?;
            into_result(enveloppe)
        })
    }
}

/// Le moteur branché sur le superviseur réel.
pub fn moteur(sup: Arc<Supervisor>) -> Arc<dyn Motor> {
    Arc::new(MoteurSuperviseur { sup })
}

// =========================================================================
// Chapitres
// =========================================================================

/// Un chapitre, réduit à ce dont le planificateur a besoin.
///
/// `Eq` est volontairement absent : `numero` est un `f64` (les sources
/// envoient `38.09999847`) et `f64` ne définit pas `Eq`. Le module n'utilise
/// que l'égalité, jamais l'ordre sur ce type.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Chapitre {
    /// Clé stable du chapitre **chez la source** : c'est le seul identifiant
    /// sur lequel deux passes se comparent. Un numéro de chapitre n'en est pas
    /// un — les sources le réutilisent, le réordonnent, ou renvoient des
    /// flottants (`38.09999847` mesuré).
    pub url: String,
    pub titre: String,
    /// Numéro arrondi : arrondi ici, pas à l'affichage, parce que c'est la
    /// comparaison qui en pâtit sinon.
    pub numero: Option<f64>,
    /// Epoch millis si la source le fournit.
    pub depose_le: Option<i64>,
}

impl Chapitre {
    /// Libellé affichable, qui **ne retombe jamais sur l'URL ni sur un UUID**.
    ///
    /// Mesuré le 2026-10-04 : une source sans titre de chapitre produisait un
    /// nom de fichier `22d20970-38d1-48aa-bbd3-9ca9afbbb3dc.cbz` — valide,
    /// illisible. Ici le repli est le numéro, puis le rang, jamais l'identifiant
    /// technique.
    pub fn libelle(&self, rang: usize) -> String {
        let titre = self.titre.trim();
        if !titre.is_empty() {
            return titre.to_string();
        }
        match self.numero {
            Some(n) => format!("Chapitre {}", arrondi(n)),
            None => format!("Chapitre {}", rang + 1),
        }
    }
}

/// Arrondi d'un numéro de chapitre.
///
/// Les nombres flottants des sources sont des identifiants, pas des données
/// (`38.09999847`), et le message d'erreur le plus cher de cette phase serait
/// « Ch. 38.09999847 » dans une notification système. `round` et non `floor` :
/// `floor` transforme 38,6 en « Chapitre 38 ».
pub fn arrondi(n: f64) -> i64 {
    if n.is_finite() {
        n.round() as i64
    } else {
        0
    }
}

/// Lit la liste de chapitres d'une réponse `chapters`.
///
/// Deux filtres, tous deux mesurés :
///
/// - un chapitre **sans URL exploitable** est écarté. Une source peut renvoyer
///   45 entrées dont `url` est vide (interstitiel d'âge servi en `200 OK`) ; sans
///   URL il n'y a rien à comparer, et le retenir polluerait l'état de référence
///   avec des clés qui ne distinguent rien ;
/// - un `number` non fini est ramené à `None` plutôt qu'à `NaN`, qui n'est pas
///   sérialisable et ferait échouer l'écriture de l'état à la passe suivante.
pub fn lire_chapitres(resultat: &Value) -> Vec<Chapitre> {
    let Some(liste) = resultat.get("chapters").and_then(Value::as_array) else {
        return Vec::new();
    };
    liste
        .iter()
        .filter_map(|c| {
            let url = c.get("url").and_then(Value::as_str)?.trim();
            if url.is_empty() {
                return None;
            }
            let numero = c
                .get("number")
                .and_then(Value::as_f64)
                .filter(|n| n.is_finite())
                .map(arrondi)
                .map(|n| n as f64);
            Some(Chapitre {
                url: url.to_string(),
                titre: c
                    .get("title")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .trim()
                    .to_string(),
                numero,
                depose_le: c.get("uploadDate").and_then(Value::as_i64),
            })
        })
        .collect()
}

// =========================================================================
// Résultats d'une passe
// =========================================================================

/// Un favori qui a reçu des chapitres nouveaux.
///
/// Pas de `Eq` : `Chapitre` porte un `f64` (cf. [`Chapitre`]).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Nouveaute {
    pub source: String,
    pub manga_url: String,
    pub titre: String,
    /// Les nouveaux chapitres, du plus récent au plus ancien quand les dates
    /// sont disponibles. Tronqué à [`MAX_NOVEAUX_RAPPORTES`].
    pub chapitres: Vec<Chapitre>,
    /// Nombre **total** de chapitres nouveaux, troncature comprise. C'est ce
    /// nombre que l'interface doit afficher (« 3 nouveaux chapitres »), jamais
    /// `chapitres.len()`.
    pub total: usize,
    /// Vrai quand `chapitres` a été tronqué : l'interface doit dire « et N de
    /// plus » plutôt que d'annoncer un nombre faux.
    pub tronque: bool,
}

impl Nouveaute {
    fn nouveau(favori: &Favorite, mut nouveaux: Vec<Chapitre>) -> Self {
        // Tri du plus récent au plus ancien quand on peut faire confiance aux
        // dates. S'il en manque une seule, on **conserve l'ordre de la source** :
        // un tri partiel ne serait l'ordre de rien.
        if !nouveaux.is_empty() && nouveaux.iter().all(|c| c.depose_le.is_some()) {
            // `sort_by_key` inverse : le plus récent d'abord.
            nouveaux.sort_by_key(|c| std::cmp::Reverse(c.depose_le));
        }
        let total = nouveaux.len();
        let tronque = total > MAX_NOVEAUX_RAPPORTES;
        Self {
            source: favori.source.clone(),
            manga_url: favori.manga_url.clone(),
            titre: favori.title.clone(),
            chapitres: nouveaux.into_iter().take(MAX_NOVEAUX_RAPPORTES).collect(),
            total,
            tronque,
        }
    }
}

/// Un favori qui n'a pas pu être vérifié.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rate {
    pub source: String,
    pub manga_url: String,
    /// Motif lisible. Pour une liste vide c'est [`MOTIF_AUCUN_CHAPITRE`] : le
    /// flux 3.2 et l'écran « état des sources » (5.4) doivent pouvoir distinguer
    /// « le site a échoué » de « le site a renvoyé n'importe quoi ».
    pub motif: String,
}

/// Bilan d'une passe. Pas de `Eq` : [`Nouveaute`] en contient.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ResultatPasse {
    pub nouveaux: Vec<Nouveaute>,
    pub rates: Vec<Rate>,
    /// Favoris effectivement interrogés.
    pub verifies: usize,
    /// Sources distinctes interrogées.
    pub sources: usize,
    /// Favoris **écartés** par le débit, par un plafond, ou parce que leur
    /// source est désactivée. Ce n'est pas une perte : ils sont en tête à la
    /// passe suivante, ce que `verifies` ne dit pas.
    pub ignores: usize,
    /// Vrai quand le planificateur est coupé par son réglage.
    pub desactive: bool,
}

// =========================================================================
// La passe
// =========================================================================

/// Ordre de passage des favoris : le moins récemment vérifié d'abord, puis le
/// plus ancien favori.
fn ordre(favoris: &[Favorite], etat: &Etat) -> Vec<usize> {
    let mut cles: Vec<(i64, i64, usize)> = favoris
        .iter()
        .enumerate()
        .map(|(i, f)| {
            let verifie = etat
                .suivi(&f.source, &f.manga_url)
                .map_or(0, |s| s.verifie_le);
            (verifie, f.added_at, i)
        })
        .collect();
    // `verifie_le = 0` place d'abord tous les favoris jamais vérifiés : c'est ce
    // qui garantit qu'aucun n'est **affamé** par le plafond. Sans cela, une
    // bibliothèque où un seul favori se met à jour à chaque passeecarte les autres
    // qui ne sortiraient jamais de la file.
    cles.sort_unstable();
    cles.into_iter().map(|(_, _, i)| i).collect()
}

/// Exécute une passe complète.
///
/// Un tour = la liste des favoris, un `chapters` par favori autorisé, une
/// comparaison avec l'état connu, une mise à jour en mémoire. La persistance est
/// à la charge de l'appelant ([`boucle_tache`]) : la passe ne touche pas au
/// disque, ce qui la rend testable avec une base en mémoire.
pub async fn passe(
    motor: &dyn Motor,
    horloge: &dyn Horloge,
    store: &Store,
    etat: &mut Etat,
    reglages: &Reglages,
) -> Result<ResultatPasse, String> {
    let mut resultat = ResultatPasse::default();
    if !reglages.actif {
        resultat.desactive = true;
        return Ok(resultat);
    }
    let favoris = store.list_favorites()?;
    let desactivees = store.disabled_sources()?;
    let mut limites = Limites::neuf(reglages.delai_source(), std::mem::take(&mut etat.debit));
    let mut sources: BTreeSet<String> = BTreeSet::new();

    for i in ordre(&favoris, etat) {
        // Le plafond compte les **tentatives**, pas les favoris parcourus : un
        // favori écarté par le débit ne doit pas consommer le budget des autres,
        // sinon la passe finirait par ne rien faire du tout.
        if resultat.verifies + resultat.rates.len() >= reglages.max_par_passe {
            break;
        }
        let favori = &favoris[i];
        if desactivees
            .iter()
            .any(|s| s.eq_ignore_ascii_case(&favori.source))
        {
            resultat.ignores += 1;
            continue;
        }
        if !sources.contains(&favori.source) && sources.len() >= reglages.max_sources_par_passe {
            resultat.ignores += 1;
            continue;
        }
        // L'heure est relue **à chaque itération**, et non figée au début de la
        // passe : une passe qui dure 400 s (20 sources × 20 s) verrait sinon
        // toutes ses requêtes datées du même instant, et le délai par source
        // ne s'appliquerait jamais — c'est-à-dire que la politesse serait
        // fictive.
        let maintenant = horloge.maintenant_ms();
        if !limites.autorisee(&favori.source, maintenant) {
            resultat.ignores += 1;
            continue;
        }
        sources.insert(favori.source.clone());
        resultat.sources = sources.len();

        // Lecture de l'état AVANT la requête : c'est elle qui décide s'il y a
        // un état de référence à comparer. Le bloc termine le emprunt, pour que
        // `suivre_mut` puisse reprendre `etat` juste après.
        let connu = etat.suivi(&favori.source, &favori.manga_url);
        let premier_releve = connu.is_none_or(Suivi::jamais_verifie);
        let deja_vus: BTreeSet<&str> = connu
            .map(|s| s.vus.iter().map(String::as_str).collect())
            .unwrap_or_default();

        let reponse = motor
            .chapitres(&favori.source, &favori.manga_url, &favori.title)
            .await;

        match reponse {
            Err(motif) => {
                limites.echec(&favori.source, maintenant);
                etat.suivre_mut(&favori.source, &favori.manga_url).erreur = Some(motif.clone());
                resultat.rates.push(Rate {
                    source: favori.source.clone(),
                    manga_url: favori.manga_url.clone(),
                    motif,
                });
            }
            Ok(valeur) => {
                let chapitres = lire_chapitres(&valeur);
                if chapitres.is_empty() {
                    // Règle 3 : résultat aberrant = échec. On n'efface surtout
                    // pas `vus` — sinon la source qui répond vide une fois fait
                    // re-signaler TOUTE la bibliothèque à la passe suivante.
                    limites.echec(&favori.source, maintenant);
                    etat.suivre_mut(&favori.source, &favori.manga_url).erreur =
                        Some(MOTIF_AUCUN_CHAPITRE.to_string());
                    resultat.rates.push(Rate {
                        source: favori.source.clone(),
                        manga_url: favori.manga_url.clone(),
                        motif: MOTIF_AUCUN_CHAPITRE.to_string(),
                    });
                    continue;
                }
                let nouveaux: Vec<Chapitre> = chapitres
                    .iter()
                    .filter(|c| !deja_vus.contains(c.url.as_str()))
                    .cloned()
                    .collect();
                let suivi = etat.suivre_mut(&favori.source, &favori.manga_url);
                suivi.ajouter(&chapitres);
                suivi.verifie_le = maintenant;
                suivi.erreur = None;
                limites.succes(&favori.source, maintenant);
                resultat.verifies += 1;
                // Premier relevé : on mémorise, on ne signale pas. Sinon la
                // première annonce serait « 286 nouveaux chapitres » au
                // démarrage, pour une série suivie depuis des mois.
                if !nouveaux.is_empty() && !premier_releve {
                    resultat.nouveaux.push(Nouveaute::nouveau(favori, nouveaux));
                }
            }
        }
    }

    etat.debit = limites.etat;
    etat.dernier_pass = horloge.maintenant_ms();
    Ok(resultat)
}

// =========================================================================
// Annonce
// =========================================================================

/// Ce que la boucle fait d'un résultat de passe.
///
/// L'interface est **hors** de ce module (tâche 3.2) : on ne dépend donc pas de
/// Tauri ici, et la boucle se teste avec un [`Annonce`] collecteur. Le
/// branchement Tauri est [`AnnonceTauri`], qui ne fait qu'émettre un évènement.
pub trait Annonce: Send + Sync + 'static {
    fn annoncer(&self, resultat: &ResultatPasse);
}

/// Annonceur Tauri : émet `maj://nouveaux-chapitres`.
///
/// Le nom de l'évènement est un contrat avec 3.2 ; il est documenté ici parce
/// que le planificateur en est le **seul** producteur. Rien n'est émis quand il
/// n'y a rien de neuf : réveiller une webview toutes les six heures pour lui
/// remettre un message vide est un gaspillage, pas une veille.
pub struct AnnonceTauri {
    app: AppHandle,
}

impl Annonce for AnnonceTauri {
    fn annoncer(&self, resultat: &ResultatPasse) {
        if resultat.nouveaux.is_empty() {
            return;
        }
        if let Err(e) = self.app.emit("maj://nouveaux-chapitres", resultat) {
            tracing::warn!("annonce des nouveautés impossible : {e}");
        }
    }
}

// =========================================================================
// Commandes Tauri (3.5)
// =========================================================================

/// Ce que l'interface reçoit pour dessiner l'écran des réglages.
///
/// `bornes` est appliqué **ici** et pas seulement à la lecture : `reglages()`
/// corrige à la lecture, mais une écriture doit être bornée au moment où elle
/// arrive, sinon la base contient une valeur que rien ne relit correctement.
#[derive(Serialize)]
pub struct ReglagesVue {
    pub periode_s: u64,
    pub delai_source_s: u64,
    pub max_par_passe: usize,
    pub max_sources_par_passe: usize,
    pub actif: bool,
    /// Bornes affichées, pour que l'interface n'invente pas ses propres minimums
    /// et ne propose pas une valeur que le moteur corrigerait en silence.
    ///
    /// Elles viennent des **mêmes constantes** que [`Reglages::bornes`], pas
    /// d'un second appel à `bornes()` : cette fonction *écrase* les valeurs au
    /// lieu de seulement les lire, donc s'en servir comme borne transforme «
    /// maximum autorisé » en « valeur actuelle ». Mesuré : elle renvoyait 20
    /// sources au lieu de 200.
    pub periode_min_s: u64,
    pub delai_source_min_s: u64,
    pub max_par_passe_max: usize,
    pub max_sources_max: usize,
    /// Dernière passe observée, pour que l'utilisateur puisse voir que la veille
    /// tourne — sans elle, « actif » ne prouve rien.
    pub dernier_pass: i64,
}

impl ReglagesVue {
    fn depuis(r: &Reglages, dernier_pass: i64) -> Self {
        Self {
            periode_s: r.periode_s,
            delai_source_s: r.delai_source_s,
            max_par_passe: r.max_par_passe,
            max_sources_par_passe: r.max_sources_par_passe,
            actif: r.actif,
            periode_min_s: PERIODE_MIN.as_secs(),
            delai_source_min_s: DELAI_SOURCE_MIN.as_secs(),
            max_par_passe_max: MAX_PAR_PASSE,
            max_sources_max: MAX_SOURCES_PAR_PASSE,
            dernier_pass,
        }
    }
}

/// Règles à Tauri : `State<'_, …>` est indispensable, la commande n'est pas
/// alors invoquable et l'appel échoue à l'exécution.
#[tauri::command]
pub fn reglages_planificateur(store: State<'_, Arc<Store>>) -> Result<ReglagesVue, String> {
    let r = reglages(&store);
    let dernier_pass = match charger(&store) {
        Ok(e) => e.dernier_pass,
        // **Un état illisible n'est pas « jamais tourné ».** Mesuré le
        // 2026-10-09 (audit) : `unwrap_or(0)` faisait afficher `0` — « la veille
        // n'a jamais tourné » — alors que `boucle_tache` journalise `état du
        // planificateur illisible`. L'écran et le journal se contredisent, et
        // l'écran ment le premier : c'est lui que l'utilisateur lit.
        //
        // On journalise au même niveau, et on rend `0` faute de mieux. Il n'y a
        // pas de valeur honnête pour un état qu'on ne sait pas lire, mais au
        // moins la cause est traçable des deux côtés.
        Err(e) => {
            tracing::warn!("reglages_planificateur: {e}");
            0
        }
    };
    tracing::info!("reglages_planificateur: {r:?}");
    Ok(ReglagesVue::depuis(&r, dernier_pass))
}

/// Écrit les réglages. **Partielle** : chaque champ absent garde sa valeur
/// actuelle.
///
/// Partielle et non complète parce que l'interface envoie des `select`, donc
/// elle ne connaît que certains champs ; une écriture complète depuis une
/// version antérieure de l'écran remettrait à zéro ce qu'elle ignore.
#[tauri::command]
pub fn set_reglages_planificateur(
    store: State<'_, Arc<Store>>,
    periode_s: Option<u64>,
    delai_source_s: Option<u64>,
    max_par_passe: Option<usize>,
    max_sources_par_passe: Option<usize>,
    actif: Option<bool>,
) -> Result<ReglagesVue, String> {
    let mut r = reglages(&store);
    if let Some(v) = periode_s {
        r.periode_s = v;
    }
    if let Some(v) = delai_source_s {
        r.delai_source_s = v;
    }
    if let Some(v) = max_par_passe {
        r.max_par_passe = v;
    }
    if let Some(v) = max_sources_par_passe {
        r.max_sources_par_passe = v;
    }
    if let Some(v) = actif {
        r.actif = v;
    }
    let r = r.bornes();
    let json = serde_json::to_string(&r).map_err(|e| format!("réglages illisibles : {e}"))?;
    store.set_setting(CLE_REGLAGES, &json)?;
    tracing::info!("set_reglages_planificateur: {r:?}");
    let dernier_pass = match charger(&store) {
        Ok(e) => e.dernier_pass,
        Err(e) => {
            tracing::warn!("set_reglages_planificateur: {e}");
            0
        }
    };
    Ok(ReglagesVue::depuis(&r, dernier_pass))
}

// =========================================================================
// Boucle
// =========================================================================

/// Drapeau d'arrêt de la boucle.
pub type Arret = Arc<AtomicBool>;

/// Un drapeau d'arrêt neuf. Passé en paramètre plutôt que global pour que le
/// test puisse arrêter la boucle qu'il a lui-même démarrée.
pub fn nouvel_arret() -> Arret {
    Arc::new(AtomicBool::new(false))
}

/// Boucle de fond : une passe, puis la suivante.
///
/// Le `Skip` des ticks manqués est **indispensable** : `interval` rattrape par
/// défaut les ticks perdus, donc une passe de 400 s avec une période de 6 h est
/// ici un cas normal, mais une période de 5 min déclencherait 1,3 passe
/// supplémentaire d'affilée, en rafale. On saute ce qui a été manqué : le
/// planificateur n'a aucun intérêt à rattraper, il a intérêt à être discret.
pub async fn boucle_tache(
    motor: Arc<dyn Motor>,
    horloge: Arc<dyn Horloge>,
    store: Arc<Store>,
    annonce: Arc<dyn Annonce>,
    reglages: Arc<Reglages>,
    periode: Duration,
    arret: Arret,
) {
    let mut tick = tokio::time::interval(periode);
    tick.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    let mut numero = 0u64;
    loop {
        // Le premier `tick` est **immédiat** : la passe de démarrage pose
        // l'état de référence, donc la première passe qui sert à quelque chose
        // ne remonte que du réellement nouveau. Retarder ce premier relevé
        // ferait annoncer toute la bibliothèque à la première passe utile.
        tick.tick().await;
        if arret.load(Ordering::SeqCst) {
            return;
        }
        numero += 1;
        let debut = Instant::now();
        let mut etat = match charger(&store) {
            Ok(e) => e,
            Err(e) => {
                tracing::warn!("état du planificateur illisible ({e}), on repart de zéro");
                Etat::default()
            }
        };
        match passe(&*motor, &*horloge, &store, &mut etat, &reglages).await {
            Ok(resultat) => {
                tracing::info!(
                    passe = numero,
                    verifies = resultat.verifies,
                    ignores = resultat.ignores,
                    sources = resultat.sources,
                    nouveaux = resultat.nouveaux.len(),
                    echecs = resultat.rates.len(),
                    duree_ms = debut.elapsed().as_millis() as u64,
                    "planificateur",
                );
                if let Err(e) = sauver(&store, &etat) {
                    tracing::warn!("état du planificateur non écrit : {e}");
                }
                annonce.annoncer(&resultat);
            }
            Err(e) => tracing::warn!("planificateur : passe impossible ({e})"),
        }
    }
}

/// Point d'entrée appelé au démarrage de l'application.
///
/// Lit les réglages **une fois** pour la période (le reste est relu à chaque
/// passe) et rend la main quand `arret` est levé.
pub async fn demarrer(motor: Arc<dyn Motor>, store: Arc<Store>, app: AppHandle, arret: Arret) {
    let r = reglages(&store);
    let periode = r.periode();
    tracing::info!(
        "planificateur : période {periode:?}, délai source {:?}, {} favoris max par passe, {} sources max",
        r.delai_source(),
        r.max_par_passe,
        r.max_sources_par_passe,
    );
    boucle_tache(
        motor,
        Arc::new(HorlogeSysteme),
        store,
        Arc::new(AnnonceTauri { app }),
        Arc::new(r),
        periode,
        arret,
    )
    .await
}

// =========================================================================
// Tests
// =========================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicI64;
    use std::sync::Mutex;

    // ------------------------------------------------------------- Doublures

    /// Moteur de test : une réponse par URL de manga, et un relevé horodaté de
    /// chaque appel.
    ///
    /// L'horodatage est ce qui prouve le débit. Un compteur d'appels ne dit pas
    /// *quand* la source a été interrogée : « 3 appels » peut vouloir dire
    /// « 3 appels en 400 ms ».
    struct MoteurFaux {
        reponses: Mutex<BTreeMap<String, Vec<Result<Value, String>>>>,
        appels: Mutex<Vec<(String, i64)>>,
        horloge: Option<Arc<HorlogeFixe>>,
    }

    impl MoteurFaux {
        fn nouveau() -> Arc<Self> {
            Self::avec_horloge(None)
        }

        /// Variante qui horodate ses appels avec l'heure du test.
        fn horodate(horloge: &Arc<HorlogeFixe>) -> Arc<Self> {
            Self::avec_horloge(Some(horloge.clone()))
        }

        fn avec_horloge(horloge: Option<Arc<HorlogeFixe>>) -> Arc<Self> {
            Arc::new(Self {
                reponses: Mutex::new(BTreeMap::new()),
                appels: Mutex::new(Vec::new()),
                horloge,
            })
        }

        fn repond(&self, manga_url: &str, r: Result<Value, String>) {
            self.reponses
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .insert(manga_url.to_string(), vec![r]);
        }

        /// Réponses **consécutives** : la première au premier appel, la
        /// deuxième au second, et la dernière indéfiniment.
        ///
        /// C'est ce qui rend le test de la boucle déterministe. Avec une
        /// réponse unique modifiée « juste après » le démarrage, la première
        /// passe peut la voir déjà mise à jour : elle mémorise alors le
        /// chapitre comme vu, et il n'est jamais signalé. Le test passerait une
        /// fois sur deux, pour une raison absente du code.
        fn repond_serie(&self, manga_url: &str, r: Vec<Result<Value, String>>) {
            self.reponses
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .insert(manga_url.to_string(), r);
        }

        fn repond_ok(&self, manga_url: &str, chapitres: Vec<Value>) {
            self.repond(manga_url, Ok(json!({ "chapters": chapitres })));
        }

        fn nb_appels(&self) -> usize {
            self.appels.lock().unwrap_or_else(|e| e.into_inner()).len()
        }

        /// Instants de **toutes** les requêtes, dans l'ordre d'appel. Indépendant
        /// de l'ordre des favoris, que l'implémentation est libre de changer.
        fn instants(&self) -> Vec<i64> {
            self.appels
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .iter()
                .map(|(_, t)| *t)
                .collect()
        }

        /// URL des requêtes, **dans l'ordre réel d'appel**.
        ///
        /// C'est le seul relevé qui dise quel favori est passé en premier :
        /// relire l'état ne le dit pas, car celui-ci ne retient qu'une date —
        /// plusieurs favoris peuvent ainsi partager le même `verifie_le` au
        /// millième.
        fn urls_appelees(&self) -> Vec<String> {
            self.appels
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .iter()
                .map(|(u, _)| u.clone())
                .collect()
        }
    }

    impl Motor for MoteurFaux {
        fn chapitres<'a>(
            &'a self,
            _source: &'a str,
            manga_url: &'a str,
            _titre: &'a str,
        ) -> RequeteChapters<'a> {
            // L'appel est journalisé AVANT de répondre, et même s'il échoue :
            // sinon « aucune requête après un échec » passerait pour un débit
            // respecté.
            let instant = self.horloge.as_ref().map_or(0, |h| h.maintenant_ms());
            self.appels
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push((manga_url.to_string(), instant));
            let r = {
                let mut reponses = self.reponses.lock().unwrap_or_else(|e| e.into_inner());
                match reponses.get_mut(manga_url) {
                    // On consomme la file, et la dernière réponse sert ensuite
                    // indéfiniment (cf. `repond_serie`).
                    Some(file) if file.len() > 1 => file.remove(0),
                    Some(file) => file[0].clone(),
                    None => Err(format!("aucune réponse prévue pour {manga_url}")),
                }
            };
            Box::pin(async move { r })
        }
    }

    /// Horloge pilotée par le test : avancer le temps est une affectation.
    struct HorlogeFixe(AtomicI64);

    impl HorlogeFixe {
        fn new(t: i64) -> Arc<Self> {
            Arc::new(Self(AtomicI64::new(t)))
        }

        fn set(&self, t: i64) {
            self.0.store(t, Ordering::SeqCst);
        }

        /// Avance de `secondes` et rend la nouvelle heure.
        fn avance(&self, secondes: i64) -> i64 {
            let delta = secondes * 1000;
            self.0.fetch_add(delta, Ordering::SeqCst) + delta
        }
    }

    impl Horloge for HorlogeFixe {
        fn maintenant_ms(&self) -> i64 {
            self.0.load(Ordering::SeqCst)
        }
    }

    /// Annonceur collecteur : ce qu'annonce la boucle, sans Tauri.
    #[derive(Default)]
    struct AnnonceurCollecteur(Mutex<Vec<ResultatPasse>>);

    impl AnnonceurCollecteur {
        fn annonces(&self) -> Vec<ResultatPasse> {
            self.0.lock().unwrap_or_else(|e| e.into_inner()).clone()
        }
    }

    impl Annonce for AnnonceurCollecteur {
        fn annoncer(&self, resultat: &ResultatPasse) {
            self.0
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .push(resultat.clone());
        }
    }

    // -------------------------------------------------------------- Fixtures

    fn chapitre(url: &str, titre: &str, numero: Option<f64>, date: Option<i64>) -> Value {
        let mut o = serde_json::Map::new();
        o.insert("url".into(), Value::String(url.to_string()));
        o.insert("title".into(), Value::String(titre.to_string()));
        if let Some(n) = numero {
            o.insert("number".into(), json!(n));
        }
        if let Some(d) = date {
            o.insert("uploadDate".into(), json!(d));
        }
        Value::Object(o)
    }

    /// Liste de chapitres tous datés pareil, pour les tests où la date de
    /// publication n'a aucun intérêt.
    fn chapitres(urls: &[&str]) -> Vec<Value> {
        urls.iter()
            .map(|u| chapitre(u, "Chapitre", Some(1.0), Some(1000)))
            .collect()
    }

    fn store_avec(source: &str, urls: &[&str]) -> Store {
        store_de(&[(source, urls)])
    }

    /// Base en mémoire avec les favoris donnés, tous ajoutés dans l'ordre fourni.
    fn store_de(paires: &[(&str, &[&str])]) -> Store {
        let s = Store::open_in_memory().expect("base en mémoire");
        for (source, urls) in paires {
            for u in *urls {
                s.set_favorite(source, u, "Titre", None, None, true)
                    .expect("ajout du favori");
            }
        }
        s
    }

    /// Réglages de test : une source à la fois, sans plafond de passe.
    ///
    /// Le délai de source est mis à son plancher (5 s) mais l'heure est pilotée
    /// par [`HorlogeFixe`] : aucune passe de test n'attend réellement quoi que ce
    /// soit.
    fn r_de_test() -> Reglages {
        Reglages::de_test(3600, 5, 100)
    }

    fn favorite_de_test() -> Favorite {
        Favorite {
            source: "MANGADEX".into(),
            manga_url: "m/1".into(),
            title: "Titre".into(),
            cover_url: None,
            cover_token: None,
            content_rating: None,
            added_at: 1,
            category_id: None,
            category_color: None,
        }
    }

    // -------------------------------------------------- Détection de nouveautés

    /// Le **premier** relevé d'un favori ne signale rien, et il pose l'état de
    /// référence.
    ///
    /// C'est la règle qui évite l'orage d'ouverture : sans elle, le premier
    /// lancement annonce les 286 chapitres d'une série, et les réannonce à chaque
    /// démarrage puisque rien ne serait jamais mémorisé.
    #[tokio::test]
    async fn le_premier_releve_annonce_rien_et_pose_l_etat_de_reference() {
        let store = store_avec("MANGADEX", &["manga/1"]);
        let moteur = MoteurFaux::nouveau();
        moteur.repond_ok("manga/1", chapitres(&["c1", "c2", "c3"]));
        let horloge = HorlogeFixe::new(1_000_000);
        let mut etat = Etat::default();

        let r = passe(&*moteur, &*horloge, &store, &mut etat, &r_de_test())
            .await
            .expect("passe");

        assert!(
            r.nouveaux.is_empty(),
            "le premier relevé ne doit rien signaler"
        );
        assert_eq!(r.verifies, 1);
        assert!(r.rates.is_empty());
        let suivi = etat.suivi("MANGADEX", "manga/1").expect("suivi créé");
        assert_eq!(
            suivi.vus.len(),
            3,
            "les trois chapitres doivent être mémorisés"
        );
        assert_eq!(suivi.verifie_le, 1_000_000);
        assert!(
            !suivi.jamais_verifie(),
            "après un relevé réussi, le favori n'est plus « jamais vérifié »"
        );
    }

    /// Le contrôle **positif** du précédent : une fois l'état de référence posé,
    /// un chapitre supplémentaire EST signalé, et lui seul.
    ///
    /// Les deux contrôles vont ensemble. « Rien n'est annoncé » se satisferrait
    /// aussi avec un planificateur cassé qui ne renvoie jamais rien.
    #[tokio::test]
    async fn un_chapitre_apparu_est_signale_et_lui_seul() {
        let store = store_avec("MANGADEX", &["manga/1"]);
        let moteur = MoteurFaux::nouveau();
        moteur.repond_ok("manga/1", chapitres(&["c1", "c2", "c3"]));
        let horloge = HorlogeFixe::new(1_000_000);
        let mut etat = Etat::default();
        let r = r_de_test();

        passe(&*moteur, &*horloge, &store, &mut etat, &r)
            .await
            .expect("passe 1");
        horloge.avance(60);
        moteur.repond_ok("manga/1", chapitres(&["c1", "c2", "c3", "c4"]));

        let r2 = passe(&*moteur, &*horloge, &store, &mut etat, &r)
            .await
            .expect("passe 2");

        assert_eq!(r2.nouveaux.len(), 1, "un seul favori concerné");
        let n = &r2.nouveaux[0];
        assert_eq!(n.source, "MANGADEX");
        assert_eq!(n.manga_url, "manga/1");
        assert_eq!(n.total, 1, "un seul chapitre nouveau");
        assert_eq!(n.chapitres.len(), 1);
        assert_eq!(n.chapitres[0].url, "c4");
        assert!(!n.tronque);
    }

    /// Le contrôle négatif de la détection : une liste **inchangée** ne signale
    /// rien. Sans ce test, « zéro nouveauté » ne prouverait rien du tout.
    #[tokio::test]
    async fn une_passe_sans_changement_ne_signale_rien() {
        let store = store_avec("MANGADEX", &["manga/1"]);
        let moteur = MoteurFaux::nouveau();
        moteur.repond_ok("manga/1", chapitres(&["c1", "c2", "c3"]));
        let horloge = HorlogeFixe::new(1_000_000);
        let mut etat = Etat::default();
        let r = r_de_test();

        for _ in 0..4 {
            horloge.avance(3600);
            let resultat = passe(&*moteur, &*horloge, &store, &mut etat, &r)
                .await
                .expect("passe");
            assert!(
                resultat.nouveaux.is_empty(),
                "aucun chapitre n'a changé : rien ne doit être signalé"
            );
            assert_eq!(resultat.verifies, 1, "le favori doit rester vérifié");
        }
        assert_eq!(
            moteur.nb_appels(),
            4,
            "chaque passe doit bien réinterroger la source"
        );
    }

    /// Un chapitre est signalé **une seule fois**, même si les passes s'enchaînent.
    /// C'est la propriété qui rend le planificateur utilisable : une notification
    /// par chapitre, pas une par passe.
    #[tokio::test]
    async fn un_chapitre_ne_signale_quune_seule_fois() {
        let store = store_avec("MANGADEX", &["manga/1"]);
        let moteur = MoteurFaux::nouveau();
        moteur.repond_ok("manga/1", chapitres(&["c1"]));
        let horloge = HorlogeFixe::new(1_000_000);
        let mut etat = Etat::default();
        let r = r_de_test();

        passe(&*moteur, &*horloge, &store, &mut etat, &r)
            .await
            .expect("passe 1");

        let mut annonces = 0;
        for _ in 0..3 {
            horloge.avance(3600);
            let resultat = passe(&*moteur, &*horloge, &store, &mut etat, &r)
                .await
                .expect("passe");
            annonces += resultat.nouveaux.len();
        }
        assert_eq!(annonces, 0, "aucune annonce après la première passe");
    }

    /// Un chapitre sans URL exploitable est écarté : il n'y a rien à comparer,
    /// et le retenir polluerait l'état de référence.
    ///
    /// Le nombre de chapitres retenus est mesuré **explicitement** : face à une
    /// liste qui revient vide, une assertion du genre « rien n'a été signalé »
    /// passerait aussi avec un module entièrement cassé.
    #[tokio::test]
    async fn un_chapitre_sans_url_est_ecarte_de_l_etat_de_reference() {
        let store = store_avec("MANGADEX", &["manga/1"]);
        let moteur = MoteurFaux::nouveau();
        moteur.repond_ok(
            "manga/1",
            vec![
                chapitre("c1", "Un", Some(1.0), Some(10)),
                chapitre("", "Deux", Some(2.0), Some(20)),
                chapitre("   ", "Trois", Some(3.0), Some(30)),
                chapitre("c4", "Quatre", Some(4.0), Some(40)),
            ],
        );
        let horloge = HorlogeFixe::new(1_000_000);
        let mut etat = Etat::default();

        let r = passe(&*moteur, &*horloge, &store, &mut etat, &r_de_test())
            .await
            .expect("passe");

        let suivi = etat.suivi("MANGADEX", "manga/1").expect("suivi");
        assert_eq!(suivi.vus.len(), 2, "seuls c1 et c4 sont retenables");
        assert!(suivi.vus.contains(&"c1".to_string()));
        assert!(suivi.vus.contains(&"c4".to_string()));
        assert!(
            !suivi.vus.iter().any(|u| u.trim().is_empty()),
            "aucune clé vide ne doit entrer dans l'état"
        );
        assert!(r.nouveaux.is_empty());
    }

    /// Un résultat aberrant n'est pas un succès : l'état de référence survit et
    /// la source passe en retour arrière.
    ///
    /// Le cas mesuré le 2026-10-04 : une source répondait `200 OK` avec la page
    /// HTML de son interstitiel d'âge, et renvoyait 45 chapitres dont **toutes**
    /// les URL étaient vides. Prendre ça pour un succès viderait l'état, et toute
    /// la bibliothèque serait ré-annoncée à la passe suivante.
    #[tokio::test]
    async fn une_liste_vide_est_un_echec_qui_preserve_l_etat_de_reference() {
        let store = store_avec("MANGADEX", &["manga/1"]);
        let moteur = MoteurFaux::nouveau();
        moteur.repond_ok("manga/1", chapitres(&["c1", "c2", "c3"]));
        let horloge = HorlogeFixe::new(1_000_000);
        let mut etat = Etat::default();
        let r = r_de_test();

        passe(&*moteur, &*horloge, &store, &mut etat, &r)
            .await
            .expect("passe 1");

        // La source répond vide.
        horloge.avance(60);
        moteur.repond_ok("manga/1", Vec::new());
        let r2 = passe(&*moteur, &*horloge, &store, &mut etat, &r)
            .await
            .expect("passe 2");

        assert_eq!(
            r2.rates.len(),
            1,
            "une liste vide doit être comptée en échec"
        );
        assert_eq!(r2.rates[0].motif, MOTIF_AUCUN_CHAPITRE);
        assert_eq!(r2.verifies, 0, "rien n'a été vérifié avec succès");
        let suivi = etat.suivi("MANGADEX", "manga/1").expect("suivi");
        assert_eq!(
            suivi.vus.len(),
            3,
            "l'état de référence doit survivre à une réponse vide"
        );
        assert_eq!(
            suivi.verifie_le, 1_000_000,
            "un échec ne doit pas avancer l'horodatage du dernier succès"
        );
        assert_eq!(suivi.erreur.as_deref(), Some(MOTIF_AUCUN_CHAPITRE));

        // Contrôle POSITIF : la source repart, et son nouveau chapitre est bien
        // signalé — donc l'échec n'a pas figé l'état.
        horloge.avance(3600);
        moteur.repond_ok("manga/1", chapitres(&["c1", "c2", "c3", "c4"]));
        let r3 = passe(&*moteur, &*horloge, &store, &mut etat, &r)
            .await
            .expect("passe 3");
        assert_eq!(r3.rates.len(), 0, "la source doit être revalidée");
        assert_eq!(r3.nouveaux.len(), 1);
        assert_eq!(r3.nouveaux[0].chapitres[0].url, "c4");
    }

    /// L'erreur du moteur est **conservée** et non perdue : c'est elle que
    /// l'écran « état des sources » (5.4) et le flux 3.2 doivent afficher. Une
    /// erreuruniquement loguée dans `tracing` serait invisible pour l'utilisateur.
    #[tokio::test]
    async fn l_erreur_du_moteur_est_conservee_dans_l_etat() {
        let store = store_avec("MANGADEX", &["manga/1"]);
        let moteur = MoteurFaux::nouveau();
        moteur.repond(
            "manga/1",
            Err("CHALLENGE_REQUIRED: vérification anti-robot".to_string()),
        );
        let horloge = HorlogeFixe::new(1_000_000);
        let mut etat = Etat::default();

        let r = passe(&*moteur, &*horloge, &store, &mut etat, &r_de_test())
            .await
            .expect("passe");

        assert_eq!(r.rates.len(), 1);
        assert_eq!(
            r.rates[0].motif, "CHALLENGE_REQUIRED: vérification anti-robot",
            "le code d'erreur du moteur doit arriver tel quel"
        );
        let suivi = etat.suivi("MANGADEX", "manga/1").expect("suivi");
        assert_eq!(
            suivi.erreur.as_deref(),
            Some("CHALLENGE_REQUIRED: vérification anti-robot")
        );
        assert!(
            suivi.jamais_verifie(),
            "un échec n'est pas une vérification"
        );
    }

    /// Le libellé d'un chapitre ne retombe **jamais** sur son URL.
    ///
    /// Mesuré le 2026-10-04 : une source sans titre produisait un nom de fichier
    /// `22d20970-38d1-48aa-bbd3-9ca9afbbb3dc.cbz`. Le repli est le numéro, puis le
    /// rang — jamais l'identifiant technique.
    #[test]
    fn le_libelle_ne_retombe_jamais_sur_l_url() {
        let avec_titre = Chapitre {
            url: "chapter/1".into(),
            titre: "Le titre de la source".into(),
            numero: Some(12.0),
            depose_le: None,
        };
        assert_eq!(avec_titre.libelle(0), "Le titre de la source");

        let avec_numero = Chapitre {
            url: "chapter/2".into(),
            titre: "   ".into(),
            numero: Some(38.099_998_47),
            depose_le: None,
        };
        let l = avec_numero.libelle(0);
        assert_eq!(l, "Chapitre 38", "le flottant doit être arrondi");
        assert!(!l.contains("0.09"), "aucun résidu de flottant : {l}");
        assert!(!l.contains("chapter"), "jamais l'URL : {l}");

        let sans_rien = Chapitre {
            url: "22d20970-38d1-48aa-bbd3-9ca9afbbb3dc".into(),
            titre: String::new(),
            numero: None,
            depose_le: None,
        };
        let l = sans_rien.libelle(4);
        assert_eq!(l, "Chapitre 5", "le rang prend le relais");
        assert!(
            !l.contains("22d20970"),
            "un identifiant ne doit jamais atteindre un libellé : {l}"
        );
    }

    // ------------------------------------------------------- Limites de débit

    /// Le délai par source est réellement appliqué, à la seconde près.
    ///
    /// Trois favoris de la **même** source : un seul passe par fenêtre de délai.
    /// Les horodatages sont relevés par le moteur de test, donc la preuve ne
    /// repose pas sur un compteur mais sur l'instant de chaque appel.
    #[tokio::test]
    async fn le_delai_par_source_est_applique_a_la_seconde_pres() {
        let store = store_avec("MANGADEX", &["m/1", "m/2", "m/3"]);
        let horloge = HorlogeFixe::new(1_000_000);
        let moteur = MoteurFaux::horodate(&horloge);
        for u in ["m/1", "m/2", "m/3"] {
            moteur.repond_ok(u, chapitres(&["c1"]));
        }
        let mut etat = Etat::default();
        let r = Reglages::de_test(3600, 5, 10);

        let p1 = passe(&*moteur, &*horloge, &store, &mut etat, &r)
            .await
            .expect("passe 1");
        assert_eq!(p1.verifies, 1, "une seule requête par source et par délai");
        assert_eq!(p1.ignores, 2, "les deux autres sont écartés, pas perdus");
        assert_eq!(p1.sources, 1);

        // 4,9 s plus tard : toujours pas le droit.
        horloge.set(1_000_000 + 4_900);
        let p2 = passe(&*moteur, &*horloge, &store, &mut etat, &r)
            .await
            .expect("passe 2");
        assert_eq!(p2.verifies, 0, "le délai n'est pas encore écoulé");
        assert_eq!(p2.ignores, 3);

        // 5,1 s plus tard : une requête repart.
        horloge.set(1_000_000 + 5_100);
        let p3 = passe(&*moteur, &*horloge, &store, &mut etat, &r)
            .await
            .expect("passe 3");
        assert_eq!(p3.verifies, 1);
        assert_eq!(
            moteur.instants(),
            vec![1_000_000, 1_005_100],
            "chaque requête doit partir à l'instant où elle est autorisée : \
             une requête partie avant le délai romprait la politesse"
        );
        // Et ce ne sont pas deux fois le même favori : un favori vérifié doit
        // céder la place aux deux autres, pas être ré-vérifié indéfiniment.
        let verifies: Vec<&str> = ["m/1", "m/2", "m/3"]
            .into_iter()
            .filter(|u| etat.suivi("MANGADEX", u).is_some_and(|s| s.verifie_le != 0))
            .collect();
        assert_eq!(
            verifies.len(),
            2,
            "les deux passages doivent porter sur deux favoris différents : {verifies:?}"
        );
    }

    /// Un échec allonge le délai, un succès le réarme. Les valeurs sont
    /// vérifiées au millième, pas « croissantes ».
    #[tokio::test]
    async fn un_echec_allonge_le_delai_et_un_succes_le_rearme() {
        let store = store_avec("MANGADEX", &["m/1"]);
        let horloge = HorlogeFixe::new(1_000_000);
        let moteur = MoteurFaux::horodate(&horloge);
        moteur.repond("m/1", Err("TIMEOUT: source muette".to_string()));
        let mut etat = Etat::default();
        let r = Reglages::de_test(3600, 5, 10);

        // Échec 1 : attente de 5 s (1 × le délai).
        passe(&*moteur, &*horloge, &store, &mut etat, &r)
            .await
            .expect("passe 1");
        assert_eq!(
            etat.debit.get("MANGADEX").expect("compteur").prochaine,
            1_000_000 + 5_000
        );

        // Échec 2 : 10 s (2 × le délai) — le retour arrière est exponentiel.
        horloge.set(1_000_000 + 5_000);
        passe(&*moteur, &*horloge, &store, &mut etat, &r)
            .await
            .expect("passe 2");
        assert_eq!(
            etat.debit.get("MANGADEX").expect("compteur").prochaine,
            1_000_000 + 5_000 + 10_000
        );
        assert_eq!(etat.debit.get("MANGADEX").expect("compteur").echecs, 2);

        // Un succès réarme le délai nominal et oublie les échecs.
        horloge.set(1_000_000 + 30_000);
        moteur.repond_ok("m/1", chapitres(&["c1"]));
        let ok = passe(&*moteur, &*horloge, &store, &mut etat, &r)
            .await
            .expect("passe 3");
        assert!(ok.rates.is_empty(), "la source doit être revalidée");
        let e = etat.debit.get("MANGADEX").expect("compteur");
        assert_eq!(e.echecs, 0, "un succès oublie les échecs");
        assert_eq!(e.prochaine, 1_000_000 + 30_000 + 5_000);
    }

    /// Le retour arrière est **borné** : une source morte ne doit pas être
    /// quittée pour toujours non plus, sinon un correctif côté site ne serait
    /// jamais vu.
    #[tokio::test]
    async fn le_retour_arriere_est_borne() {
        let store = store_avec("MANGADEX", &["m/1"]);
        let horloge = HorlogeFixe::new(1_000_000);
        let moteur = MoteurFaux::horodate(&horloge);
        moteur.repond("m/1", Err("INTERNAL: casse".to_string()));
        let mut etat = Etat::default();
        let r = Reglages::de_test(3600, 5, 10);

        // Chaque passe est replacée exactement à l'heure où la source est
        // réautorisée : on regarde ainsi l'attente demandée, et non l'horloge.
        let mut t = 1_000_000i64;
        for _ in 0..12 {
            horloge.set(t);
            passe(&*moteur, &*horloge, &store, &mut etat, &r)
                .await
                .expect("passe");
            t = etat.debit.get("MANGADEX").expect("compteur").prochaine;
        }
        let e = etat.debit.get("MANGADEX").expect("compteur");
        assert_eq!(e.echecs, ECHECS_PLAFOND, "le compteur d'échecs est borné");
        assert_eq!(
            t - (t - RETOUR_ARRIERE_MAX.as_millis() as i64),
            RETOUR_ARRIERE_MAX.as_millis() as i64,
            "l'attente doit rester plafonnée, pas croître sans fin"
        );
    }

    /// Le plafond par passe borne les requêtes **sans affamer** les favoris : les
    /// moins récemment vérifiés remontent, donc chacun passe une fois avant
    /// qu'un autre soit re-vérifié.
    ///
    /// C'est la propriété qui distingue un plafond d'un tri : un simple
    /// `truncate(max)` sur une liste figée laisserait le dernier favori jamais
    /// vu de sa vie.
    ///
    /// **Une source par favori**, volontairement : à source unique, c'est le
    /// délai par source qui limiterait la passe à un seul favori, et le test
    /// mesurerait le débit (déjà mesuré ailleurs) au lieu de l'ordre. Les deux
    /// bornes doivent pouvoir être observées séparément.
    #[tokio::test]
    async fn le_plafond_par_passe_ne_famine_aucun_favori() {
        let sources = ["MANGA1", "MANGA2", "MANGA3", "MANGA4", "MANGA5"];
        let store = store_de(&[
            (sources[0], &["m/1"]),
            (sources[1], &["m/2"]),
            (sources[2], &["m/3"]),
            (sources[3], &["m/4"]),
            (sources[4], &["m/5"]),
        ]);
        let horloge = HorlogeFixe::new(1_000_000);
        let moteur = MoteurFaux::horodate(&horloge);
        for i in 1..=5 {
            moteur.repond_ok(&format!("m/{i}"), chapitres(&["c1"]));
        }
        let mut etat = Etat::default();
        // Deux favoris par passe, une source par favori, aucun plafond de
        // sources : c'est l'ORDRE qu'on veut vérifier ici.
        let r = Reglages {
            periode_s: 3600,
            delai_source_s: 5,
            max_par_passe: 2,
            max_sources_par_passe: 200,
            actif: true,
        };

        for tour in 0..3 {
            horloge.avance(3600);
            let resultat = passe(&*moteur, &*horloge, &store, &mut etat, &r)
                .await
                .expect("passe");
            assert_eq!(
                resultat.verifies, 2,
                "tour {tour} : deux par passe, pas plus"
            );
        }
        // L'ordre réel des requêtes est la seule preuve de l'équité : l'état ne
        // retient qu'une date, et plusieurs favoris partagent le même
        // `verifie_le` au millième — les relire ne dit donc pas qui est passé
        // en premier.
        let reels = moteur.urls_appelees();
        assert_eq!(
            reels.len(),
            6,
            "3 passes × 2 requêtes, une source par favori (le débit n'est pas \
             en jeu ici)"
        );
        let cinq_premiers: BTreeSet<&String> = reels.iter().take(5).collect();
        assert_eq!(
            cinq_premiers.len(),
            5,
            "aucun favori ne doit être revérifié avant que les quatre autres \
             l'aient été : ordre réel {reels:?}"
        );
        // Le 6ᵉ créneau ne peut être qu'une revérification, puisqu'il n'en reste
        // plus : 6 requêtes pour 5 favoris.
        assert_eq!(
            reels[5], reels[0],
            "le 6ᵉ passage est nécessairement une revérification"
        );
    }

    /// Le plafond de sources distinctes protège le moteur quand la bibliothèque
    /// couvre beaucoup de catalogues.
    #[tokio::test]
    async fn le_plafond_de_sources_distinctes_est_respecte() {
        let store = store_de(&[
            ("MANGA1", &["m/1"]),
            ("MANGA2", &["m/2"]),
            ("MANGA3", &["m/3"]),
            ("MANGA4", &["m/4"]),
        ]);
        let horloge = HorlogeFixe::new(1_000_000);
        let moteur = MoteurFaux::horodate(&horloge);
        for i in 1..=4 {
            moteur.repond_ok(&format!("m/{i}"), chapitres(&["c1"]));
        }
        let mut etat = Etat::default();
        let r = Reglages {
            periode_s: 3600,
            delai_source_s: 5,
            max_par_passe: 100,
            max_sources_par_passe: 2,
            actif: true,
        };

        let resultat = passe(&*moteur, &*horloge, &store, &mut etat, &r)
            .await
            .expect("passe");

        assert_eq!(resultat.sources, 2, "deux sources distinctes au maximum");
        assert_eq!(resultat.verifies, 2);
        assert_eq!(resultat.ignores, 2, "les deux autres attendent leur tour");
        assert_eq!(moteur.nb_appels(), 2, "et aucune requête en trop");
    }

    /// Une source désactivée par l'utilisateur n'est jamais interrogée, et son
    /// favori ne reçoit aucun suivi : un favori jamais vérifié ne doit pas se
    /// retrouver avec une entrée d'état, sinon il passerait pour déjà vu.
    #[tokio::test]
    async fn une_source_desactivee_nest_jamais_interrogee() {
        let store = Store::open_in_memory().expect("base");
        store
            .set_favorite("MANGADEX", "m/1", "T", None, None, true)
            .expect("favori");
        store
            .set_source_enabled("MANGADEX", false)
            .expect("désactivation");
        let horloge = HorlogeFixe::new(1_000_000);
        let moteur = MoteurFaux::horodate(&horloge);
        moteur.repond_ok("m/1", chapitres(&["c1"]));
        let mut etat = Etat::default();

        let r = passe(&*moteur, &*horloge, &store, &mut etat, &r_de_test())
            .await
            .expect("passe");

        assert_eq!(
            moteur.nb_appels(),
            0,
            "aucune requête vers une source écartée"
        );
        assert_eq!(r.verifies, 0);
        assert_eq!(r.ignores, 1);
        assert!(
            etat.suivi("MANGADEX", "m/1").is_none(),
            "un favori jamais vérifié ne doit pas laisser d'état"
        );
    }

    /// Le réglage d'activation coupe **toute** requête, y compris le relevé de
    /// référence. C'est ce qui rend la tâche 3.5 possible sans toucher au code.
    #[tokio::test]
    async fn un_planificateur_desactive_ne_requete_rien() {
        let store = store_avec("MANGADEX", &["m/1"]);
        let horloge = HorlogeFixe::new(1_000_000);
        let moteur = MoteurFaux::horodate(&horloge);
        moteur.repond_ok("m/1", chapitres(&["c1"]));
        let mut etat = Etat::default();
        let r = Reglages {
            actif: false,
            ..r_de_test()
        };

        let resultat = passe(&*moteur, &*horloge, &store, &mut etat, &r)
            .await
            .expect("passe");

        assert!(resultat.desactive);
        assert_eq!(moteur.nb_appels(), 0);
        assert!(resultat.nouveaux.is_empty());
    }

    // ------------------------------------------------------------------ État

    /// L'état doit survivre au redémarrage : c'est la seule raison de l'écrire.
    /// Test de l'aller-retour **réel** (sérialisation puis désérialisation), pas
    /// d'une structure partagée en mémoire qui ne prouverait rien.
    #[tokio::test]
    async fn l_etat_survit_a_un_redemarrage() {
        let store = store_avec("MANGADEX", &["m/1"]);
        let horloge = HorlogeFixe::new(1_000_000);
        let moteur = MoteurFaux::horodate(&horloge);
        moteur.repond_ok("m/1", chapitres(&["c1", "c2"]));
        let r = r_de_test();

        let mut etat = Etat::default();
        passe(&*moteur, &*horloge, &store, &mut etat, &r)
            .await
            .expect("passe 1");
        sauver(&store, &etat).expect("écriture de l'état");

        // « Redémarrage » : on relit l'état depuis la base, dans une autre
        // instance, et on repart de zéro en mémoire.
        let relu = charger(&store).expect("chargement");
        assert_eq!(relu, etat, "l'état relu doit être identique à l'état écrit");

        horloge.avance(3600);
        moteur.repond_ok("m/1", chapitres(&["c1", "c2", "c3"]));
        let mut etat2 = relu;
        let r2 = passe(&*moteur, &*horloge, &store, &mut etat2, &r)
            .await
            .expect("passe 2");

        assert_eq!(
            r2.nouveaux.len(),
            1,
            "un chapitre nouveau doit être signalé après redémarrage"
        );
        assert_eq!(r2.nouveaux[0].chapitres[0].url, "c3");
    }

    /// Un suivi désérialisé **sans aucun champ** doit se lire comme un premier
    /// relevé, jamais comme « déjà vérifié avec zéro chapitre vu ».
    ///
    /// C'est le piège du sentinelle : un `#[serde(default)]` mal posé donnerait
    /// `premier_releve = false` et l'application annoncerait toute la
    /// bibliothèque au premier tour suivant la mise à jour.
    #[test]
    fn un_suivi_sans_champ_consacre_est_un_premier_releve() {
        let suivi: Suivi = serde_json::from_str("{}").expect("désérialisation");
        assert!(suivi.jamais_verifie());
        assert!(suivi.vus.is_empty());
        assert!(suivi.erreur.is_none());

        let etat: Etat = serde_json::from_str("{}").expect("désérialisation");
        assert!(etat.suivis.is_empty());
        assert_eq!(etat.dernier_pass, 0);
        assert!(etat.debit.is_empty());
    }

    /// La fenêtre de chapitres vus est **bornée**, sans quoi un cas dégénéré
    /// ferait grossir un réglage en base sans limite.
    #[test]
    fn la_fenetre_de_chapitres_vus_est_bornee() {
        let mut suivi = Suivi::default();
        let url = |i: usize| format!("chapitre-{i}");
        let lots: Vec<Vec<Chapitre>> = (0..MAX_VUS + 250)
            .map(|i| {
                let u = url(i);
                vec![Chapitre {
                    url: u,
                    ..Chapitre::default()
                }]
            })
            .collect();
        for lot in &lots {
            suivi.ajouter(lot);
        }
        assert_eq!(
            suivi.vus.len(),
            MAX_VUS,
            "la fenêtre doit rester bornée à MAX_VUS"
        );
        assert!(
            suivi.vus.contains(&url(MAX_VUS + 249)),
            "les chapitres les plus récents doivent être conservés"
        );
    }

    /// Un réglage illisible ou vide retombe sur les valeurs par défaut, et
    /// **jamais** sur un sentinelle : une chaîne vide ne veut pas dire
    /// « désactivé ».
    #[test]
    fn un_reglage_absent_ou_illisible_retombe_sur_les_defauts() {
        let store = Store::open_in_memory().expect("base");
        assert_eq!(reglages(&store), Reglages::default());

        store.set_setting(CLE_REGLAGES, "").expect("écriture");
        assert_eq!(
            reglages(&store),
            Reglages::default(),
            "une chaîne vide ne doit pas être lue comme « désactivé »"
        );

        store
            .set_setting(CLE_REGLAGES, "{ pas du json")
            .expect("écriture");
        assert_eq!(reglages(&store), Reglages::default());

        store
            .set_setting(CLE_REGLAGES, r#"{"periode_s":0,"delai_source_s":0}"#)
            .expect("écriture");
        let r = reglages(&store);
        assert_eq!(
            r.periode(),
            PERIODE_MIN,
            "une période nulle ou absurde doit être ramenée au plancher"
        );
        assert_eq!(r.delai_source(), DELAI_SOURCE_MIN);
    }

    /// Reproduit le corps de `set_reglages_planificateur` sans le wrapper Tauri
    /// (aucun `State` hors application) : chaque `Option` absent laisse le champ
    /// en place, chaque `Some` écrase. Et ça **écrit**, sinon le test ne
    /// prouverait qu'un calcul, pas la persistance.
    ///
    /// Le premier jet prenait `Reglages::default()` pour « rien de fourni », alors
    /// que `default()` a `actif: true` — le test échouait donc sur une valeur par
    /// défaut prise pour une absence de champ. D'où le paramètre par champ ici :
    /// « rien » doit vouloir dire `None`, pas « la valeur par défaut ».
    fn ecrire(
        store: &Store,
        courant: &mut Reglages,
        champs: &[(&str, Option<u64>)],
        actif: Option<bool>,
    ) -> Reglages {
        for &(nom, v) in champs {
            match (nom, v) {
                ("periode_s", Some(x)) => courant.periode_s = x,
                ("delai_source_s", Some(x)) => courant.delai_source_s = x,
                ("max_par_passe", Some(x)) => courant.max_par_passe = x as usize,
                ("max_sources_par_passe", Some(x)) => courant.max_sources_par_passe = x as usize,
                (_, None) => {}
                _ => unreachable!("champ inconnu : {nom}"),
            }
        }
        if let Some(a) = actif {
            courant.actif = a;
        }
        let borne = courant.clone().bornes();
        store
            .set_setting(CLE_REGLAGES, &serde_json::to_string(&borne).unwrap())
            .expect("écriture des réglages");
        borne
    }

    #[test]
    fn ecrire_un_seul_champ_laisse_les_autres_intacts() {
        let store = Store::open_in_memory().expect("base");
        let avant = Reglages {
            periode_s: 3600,
            delai_source_s: 25,
            max_par_passe: 50,
            max_sources_par_passe: 8,
            actif: true,
        };
        store
            .set_setting(CLE_REGLAGES, &serde_json::to_string(&avant).unwrap())
            .expect("écriture initiale");

        // L'interface ne renvoie que `actif`.
        let r = ecrire(&store, &mut reglages(&store), &[], Some(false));
        assert!(!r.actif, "le champ fourni doit être pris");
        assert_eq!(r.periode_s, 3600, "période inchangée");
        assert_eq!(r.delai_source_s, 25, "délai par source inchangé");
        assert_eq!(r.max_par_passe, 50, "plafond par passe inchangé");
        assert_eq!(r.max_sources_par_passe, 8, "plafond de sources inchangé");

        // Et surtout : c'est **écrit**, pas seulement calculé.
        let relu = reglages(&store);
        assert!(!relu.actif, "la base doit porter la nouvelle valeur");
        assert_eq!(relu.periode_s, 3600, "et les autres, toujours");
        assert_eq!(relu.max_par_passe, 50);
    }

    /// Une valeur absurde doit être bornée **au moment de l'écriture** : sinon la
    /// base contient une valeur que seul `reglages()` sait corriger, et une
    /// lecture directe verrait l'absurde.
    #[test]
    fn ecrire_borne_les_valeurs_absurdes() {
        let store = Store::open_in_memory().expect("base");
        let r = ecrire(
            &store,
            &mut Reglages::default(),
            &[
                ("periode_s", Some(0)),
                ("delai_source_s", Some(0)),
                ("max_par_passe", Some(0)),
                ("max_sources_par_passe", Some(0)),
            ],
            Some(false),
        );
        assert_eq!(
            r.periode_s,
            PERIODE_MIN.as_secs(),
            "période ramenée au plancher"
        );
        assert_eq!(r.delai_source_s, DELAI_SOURCE_MIN.as_secs());
        assert_eq!(
            r.max_par_passe, 1,
            "un plafond de 0 condamnerait la passe à ne rien faire"
        );
        assert_eq!(r.max_sources_par_passe, 1);
        assert!(!r.actif, "l'arrêt reste respecté");

        // Ce qui est relu doit être déjà borné, sans repasser par `bornes()`.
        let relu = reglages(&store);
        assert_eq!(relu.periode_s, PERIODE_MIN.as_secs());
        assert_eq!(relu.max_par_passe, 1);
    }

    /// La vue porte les **bornes réelles**, et elles ne sont pas confondues avec
    /// la valeur courante.
    ///
    /// Régression mesurée le 2026-10-05 : la vue s'appuyait sur un second
    /// `bornes()` pour obtenir les maximums, donc `max_sources_max` valait **20**
    /// — la valeur courante — au lieu de 200. L'écran affichait alors `max="20"`
    /// alors que le moteur en accepte 200, et le réglage du plafond de sources
    /// était bloqué à sa valeur par défaut.
    #[test]
    fn la_vue_expose_les_bornes_et_non_les_valeurs() {
        let r = Reglages {
            periode_s: 6 * 3600,
            delai_source_s: 20,
            max_par_passe: 200,
            max_sources_par_passe: 20,
            actif: true,
        };
        let v = ReglagesVue::depuis(&r, 42);
        assert_eq!(v.max_par_passe_max, 5000, "maximum autorisé, pas la valeur");
        assert_eq!(v.max_sources_max, 200, "maximum autorisé, pas la valeur");
        assert_eq!(v.periode_min_s, PERIODE_MIN.as_secs());
        assert_eq!(v.delai_source_min_s, DELAI_SOURCE_MIN.as_secs());
        assert_eq!(
            v.dernier_pass, 42,
            "l'interface doit dire quand la veille a tourné"
        );

        // Et les bornes affichées doivent être celles que `bornes()` applique.
        let absurde = Reglages {
            periode_s: u64::MAX,
            delai_source_s: u64::MAX,
            max_par_passe: usize::MAX,
            max_sources_par_passe: usize::MAX,
            actif: true,
        };
        let bornee = absurde.bornes();
        assert_eq!(bornee.max_par_passe, v.max_par_passe_max);
        assert_eq!(bornee.max_sources_par_passe, v.max_sources_max);
        assert_eq!(
            bornee.periode_s,
            v.periode_min_s.max(bornee.periode_s.min(PERIODE_MAX_S))
        );
        assert_eq!(
            bornee.delai_source_s,
            v.delai_source_min_s
                .max(bornee.delai_source_s.min(DELAI_SOURCE_MAX_S))
        );
    }

    /// Un réglage absent du JSON prend sa valeur par défaut au lieu de faire
    /// échouer la lecture : c'est ce qui permet d'ajouter un réglage plus tard
    /// sans casser les bases existantes.
    #[test]
    fn un_reglage_incomplet_herite_des_defauts() {
        let store = Store::open_in_memory().expect("base");
        store
            .set_setting(CLE_REGLAGES, r#"{"actif":false}"#)
            .expect("écriture");
        let r = reglages(&store);
        assert!(!r.actif, "la seule clé présente doit être respectée");
        assert_eq!(r.periode_s, PERIODE_PAR_DEFAUT.as_secs());
        assert_eq!(r.max_par_passe, MAX_PAR_PASSE_PAR_DEFAUT);
    }

    // ------------------------------------------------------------ Nouveautés

    /// L'ordre d'annonce va du plus récent au plus ancien quand les dates
    /// existent, et suit l'ordre de la source quand elles manquent — un tri
    /// partiel ne serait l'ordre de rien.
    #[tokio::test]
    async fn les_nouveautes_sont_ordonnees_du_plus_recent_au_plus_ancien() {
        let store = store_avec("MANGADEX", &["m/1"]);
        let horloge = HorlogeFixe::new(1_000_000);
        let moteur = MoteurFaux::horodate(&horloge);
        moteur.repond_ok("m/1", chapitres(&["c1"]));
        let r = r_de_test();
        let mut etat = Etat::default();
        passe(&*moteur, &*horloge, &store, &mut etat, &r)
            .await
            .expect("passe 1");

        horloge.avance(3600);
        moteur.repond_ok(
            "m/1",
            vec![
                chapitre("c2", "Deux", Some(2.0), Some(2_000)),
                chapitre("c3", "Trois", Some(3.0), Some(3_000)),
                chapitre("c4", "Quatre", Some(4.0), Some(4_000)),
            ],
        );
        let r2 = passe(&*moteur, &*horloge, &store, &mut etat, &r)
            .await
            .expect("passe 2");

        let urls: Vec<&str> = r2.nouveaux[0]
            .chapitres
            .iter()
            .map(|c| c.url.as_str())
            .collect();
        assert_eq!(
            urls,
            vec!["c4", "c3", "c2"],
            "du plus récent au plus ancien"
        );
    }

    /// Le total rapporté est le nombre **complet** de nouveaux chapitres, même
    /// quand la liste est tronquée : c'est ce nombre que l'interface affiche.
    /// Confondre les deux ferait annoncer « 3 nouveaux » pour 15 chapitres.
    #[tokio::test]
    async fn le_total_reste_complet_meme_si_la_liste_est_tronquee() {
        let store = store_avec("MANGADEX", &["m/1"]);
        let horloge = HorlogeFixe::new(1_000_000);
        let moteur = MoteurFaux::horodate(&horloge);
        moteur.repond_ok("m/1", chapitres(&["c0"]));
        let r = r_de_test();
        let mut etat = Etat::default();
        passe(&*moteur, &*horloge, &store, &mut etat, &r)
            .await
            .expect("passe 1");

        let liste: Vec<Value> = (1..=15)
            .map(|i| chapitre(&format!("c{i}"), "x", Some(1.0), Some(1_000)))
            .collect();
        horloge.avance(3600);
        moteur.repond_ok("m/1", liste);

        let r2 = passe(&*moteur, &*horloge, &store, &mut etat, &r)
            .await
            .expect("passe 2");
        let n = &r2.nouveaux[0];
        assert_eq!(n.total, 15, "le total doit être le nombre complet");
        assert_eq!(n.chapitres.len(), MAX_NOVEAUX_RAPPORTES);
        assert!(n.tronque, "la troncature doit être annoncée");
    }

    /// La charge utile de l'évènement est sérialisable : 3.2 consommera
    /// exactement ce JSON, et un `NaN` dans un `Option<f64>` le ferait échouer à
    /// l'émission — donc **silencieusement**, chez l'utilisateur.
    #[test]
    fn le_bilan_est_serialisable_pour_levenement_tauri() {
        let resultat = ResultatPasse {
            nouveaux: vec![Nouveaute::nouveau(
                &favorite_de_test(),
                vec![Chapitre {
                    url: "c1".into(),
                    titre: "Un".into(),
                    numero: Some(1.0),
                    depose_le: Some(10),
                }],
            )],
            rates: vec![Rate {
                source: "MANGADEX".into(),
                manga_url: "m/1".into(),
                motif: MOTIF_AUCUN_CHAPITRE.into(),
            }],
            verifies: 1,
            sources: 1,
            ignores: 0,
            desactive: false,
        };
        let v = serde_json::to_value(&resultat).expect("sérialisation");
        assert_eq!(v["nouveaux"][0]["chapitres"][0]["url"], "c1");
        assert_eq!(v["nouveaux"][0]["total"], 1);
        assert_eq!(v["rates"][0]["motif"], MOTIF_AUCUN_CHAPITRE);
        assert!(v.get("verifies").is_some());
    }

    // ---------------------------------------------------------------- Boucle

    /// La boucle tourne, annonce, et s'arrête sur demande.
    ///
    /// Le test attend l'annonce d'un chapitre **réellement nouveau**, pas
    /// « une passe » : attendre « une passe » ne prouverait que que la boucle
    /// existe. L'état est relu depuis la base, ce qui prouve au passage que la
    /// boucle écrit bien son état.
    ///
    /// Deux réglages rendent le test déterministe. L'**horloge est injectée**
    /// et avancée par le test : le délai par source (plancher 5 s) est ainsi
    /// franchi en quelques dizaines de millisecondes réelles, sans attente, et
    /// sans dépendre de la vitesse de la machine. Les **réponses sont
    /// consécutives** : la première passe prend l'état de référence, la
    /// suivante trouve `c3`. Sans cela la première passe pourrait voir la
    /// réponse déjà mise à jour, mémoriser `c3` comme vu, et ne jamais
    /// l'annoncer — le test passerait une fois sur deux, pour une raison
    /// absente du code.
    #[tokio::test]
    async fn la_boucle_annonce_puis_s_arrete() {
        let store = Arc::new(store_avec("MANGADEX", &["m/1"]));
        let moteur = MoteurFaux::nouveau();
        moteur.repond_serie(
            "m/1",
            vec![
                Ok(json!({ "chapters": chapitres(&["c1", "c2"]) })),
                Ok(json!({ "chapters": chapitres(&["c1", "c2", "c3"]) })),
            ],
        );
        let annonceur = Arc::new(AnnonceurCollecteur::default());
        let horloge = HorlogeFixe::new(1_000_000);
        let stop = nouvel_arret();
        let t = tokio::spawn(boucle_tache(
            moteur.clone(),
            horloge.clone(),
            store.clone(),
            annonceur.clone(),
            Arc::new(Reglages::de_test(1, 5, 10)),
            Duration::from_millis(20),
            stop.clone(),
        ));

        // On avance l'heure de 6 s à chaque tour : au-delà du délai de source,
        // donc la source est réautorisée.
        tokio::time::timeout(Duration::from_secs(20), async {
            loop {
                horloge.avance(6);
                if annonceur.annonces().iter().any(|r| !r.nouveaux.is_empty()) {
                    return;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        })
        .await
        .expect("annonce dans les 20 s");

        let annonces = annonceur.annonces();
        // Aplatissement : la liste contient aussi les passes sans nouveauté, et
        // `r.nouveaux[0]` sur l'une d'elles planterait au lieu d'échouer.
        let annonces_chapitres: Vec<&str> = annonces
            .iter()
            .flat_map(|r| r.nouveaux.iter())
            .flat_map(|n| n.chapitres.iter())
            .map(|c| c.url.as_str())
            .collect();
        assert_eq!(
            annonces_chapitres,
            vec!["c3"],
            "le seul chapitre annoncé doit être le nouveau"
        );
        let etat = charger(&store).expect("état");
        assert!(
            !etat.suivis.is_empty(),
            "la boucle doit persister son état en base"
        );
        assert_eq!(
            etat.suivi("MANGADEX", "m/1").expect("suivi").vus.len(),
            3,
            "les trois chapitres doivent être mémorisés par la boucle"
        );

        stop.store(true, Ordering::SeqCst);
        tokio::time::timeout(Duration::from_secs(20), t)
            .await
            .expect("la boucle doit s'arrêter")
            .expect("pas de panique dans la boucle");
    }
}
