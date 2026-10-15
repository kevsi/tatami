//! Recherche globale : un titre cherché sur **toutes les sources actives**.
//!
//! Demande du 2026-10-15. Le catalogue ne savait filtrer que les **sources**
//! par nom, et la recherche par titre n'existait qu'**à l'intérieur** d'une
//! source ouverte (`searchInSource` dans l'interface). Un webtoon introuvable
//! depuis le catalogue obligeait à deviner la source qui le porte.
//!
//! ## Pourquoi ici, et pas dans l'interface
//!
//! Une passe interroge jusqu'à 1370 sources. Tenir 1370 `invoke` depuis la
//! webview mettrait la politesse (AGENTS.md §10 : « limiter le débit par
//! source […] et rester un client poli ») entre les mains de celle qui n'a
//! aucune visibilité sur le moteur. Rust est le seul client du sidecar : la
//! borne, le délai et l'annulation vivent donc ici, et l'interface ne fait
//! qu'écouter.
//!
//! ## Progression plutôt qu'attente
//!
//! Le moteur traite au plus `MAX_WORKERS` requêtes en parallèle
//! (`Server.kt:600`), et une source morte coûte son délai complet. Une passe
//! entièrement séquentielle tiendrait donc des minutes sans rien montrer.
//! Chaque source qui répond émet donc `recherche://resultat` **dès qu'elle
//! répond** : l'utilisateur voit les premiers résultats en quelques secondes,
//! et la commande rend son résumé en fin de passe.
//!
//! ## Ce qui est testé sans moteur
//!
//! Tout ce qui décide est pur ou isolé du réseau : validation de la requête,
//! sources effectivement interrogées, annulation d'une passe au profit de la
//! suivante. Le moteur n'intervient que dans `interroger_source`.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use serde::Serialize;
use serde_json::{json, Value};
use tauri::{AppHandle, Emitter, State};
use tokio::task::JoinSet;

use crate::commands::{ajouter_jetons_couverture, into_result, requested_rating, LOCAL_TIMEOUT};
use crate::pagecache::PageCache;
use crate::store::Store;
use crate::supervisor::Supervisor;

/// Requêtes simultanées vers le moteur.
///
/// Le moteur borne déjà à `MAX_WORKERS = availableProcessors().coerceIn(2, 8)`
/// (`Server.kt:600`). Au-delà de 8, Rust ne ferait que tenir une file de
/// futures sans accélérer quoi que ce soit ; en dessous, le moteur aurait des
/// travailleurs oisifs.
const CONCURRENCE: usize = 8;

/// Résultats demandés **par source**.
///
/// La recherche globale cherche la largeur — retrouver le webtoon, pas vider un
/// catalogue. Cinq fiches par source suffisent à le trouver, et 1370 sources ×
/// 25 résultats seraient des milliers de cartes à rendre.
const PAR_SOURCE: u32 = 5;

/// Délai d'une requête `search`.
///
/// 20 s : la moitié du `LIST_TIMEOUT` de `latest` (60 s, dimensionné pour des
/// catalogues qui scannent réellement leurs listes). Une passe de recherche
/// attend devant 1370 sources ; un délai plus long ferait attendre la passe
/// entière pour les sources mortes.
const DELAI_REQUETE: Duration = Duration::from_secs(20);

/// Durée maximale d'une passe.
///
/// Le résumé le dit (`partielle: true`) : au-delà, les sources non encore
/// parties ne sont pas interrogées. Sans cette borne, une passe où beaucoup de
/// sources sont mortes tiendrait `1370 / 8 × 20 s`, soit près d'une heure,
/// pendant qu'aucune nouvelle recherche ne pourrait démarrer.
const DUREE_MAX: Duration = Duration::from_secs(120);

/// Longueur minimale d'une requête, en caractères.
///
/// En dessous, une seule lettre ferait partir 1370 requêtes pour un résultat
/// ingouvernable — le coût est celui du réseau des autres, pas le nôtre.
const LONGUEUR_MIN: usize = 2;

/// Canal de l'événement portant les résultats d'une source.
const CANAL_RESULTAT: &str = "recherche://resultat";

/// Valide la requête de l'utilisateur : rognée, et d'une longueur minimale.
///
/// Le message d'erreur **nomme le minimum** : « trop court » laisse deviner,
/// un chiffre permet de corriger.
pub fn valider_requete(query: &str) -> Result<String, String> {
    let rognee = query.trim();
    if rognee.chars().count() < LONGUEUR_MIN {
        return Err(format!(
            "requête trop courte : {LONGUEUR_MIN} caractères minimum"
        ));
    }
    Ok(rognee.to_string())
}

/// Noms des sources à interroger : tout ce que rend `listSources`, sauf les
/// désactivées, dédupliqué, dans l'ordre du moteur.
///
/// Le préfixe `custom:` des sources scriptées n'est **pas** un cas particulier :
/// le moteur route `search` vers le script exactement comme vers un parser
/// natif (`Server.kt:804-811`). Les écarter aurait rendu les sources importées
/// invisibles à la recherche globale.
pub fn sources_a_interroger(sources: &[Value], desactivees: &[String]) -> Vec<String> {
    let ecartees: std::collections::HashSet<&str> =
        desactivees.iter().map(String::as_str).collect();
    let mut vues = std::collections::HashSet::new();
    let mut noms = Vec::new();
    for source in sources {
        let Some(nom) = source.get("name").and_then(Value::as_str) else {
            continue;
        };
        if ecartees.contains(nom) || !vues.insert(nom.to_string()) {
            continue;
        }
        noms.push(nom.to_string());
    }
    noms
}

/// Annulation d'une passe de recherche.
///
/// Une nouvelle passe **annule la précédente** : l'utilisateur qui retape ne
/// veut pas que 1370 requêtes de l'ancienne continuent en arrière-plan. Les
/// requêtes déjà parties vont jusqu'à leur propre délai — l'annulation est
/// vérifiée entre les lots, pas au milieu d'un appel moteur.
#[derive(Default)]
pub struct PasseEnCours {
    annulee: Mutex<Option<Arc<AtomicBool>>>,
}

impl PasseEnCours {
    /// Annule la passe en cours (s'il y en a une) et rend le drapeau de la
    /// nouvelle. Un drapeau déjà rendu vaut `false` : la passe qui le détient
    /// est la courante.
    pub fn demarrer(&self) -> Arc<AtomicBool> {
        // Un verrou empoisonné signifie qu'une passe a paniqué en le tenant ;
        // l'état reste lisible (un `Option`), donc on récupère le contenu
        // plutôt que de propager la panne dans une commande sans rapport.
        let mut garde = match self.annulee.lock() {
            Ok(g) => g,
            Err(empoisonne) => empoisonne.into_inner(),
        };
        if let Some(precedente) = garde.take() {
            precedente.store(true, Ordering::SeqCst);
        }
        let drapeau = Arc::new(AtomicBool::new(false));
        *garde = Some(drapeau.clone());
        drapeau
    }
}

/// Résumé d'une passe, rendu à l'interface en fin de commande.
///
/// `partielle` distingue « tout a été interrogé » de « la durée maximale a
/// coupé la passe » : les deux nombres se ressemblent, et seul ce drapeau dit
/// qu'il en manque.
#[derive(Debug, Serialize)]
pub struct Resume {
    pub id: String,
    pub sources_interrogees: usize,
    pub sources_avec_resultats: usize,
    pub sources_en_echec: usize,
    pub partielle: bool,
}

/// Charge utile d'un `recherche://resultat` : les fiches d'**une** source.
///
/// `Clone` est exigé par `Emitter::emit` (la charge utile est clonée vers la
/// webview).
#[derive(Debug, Serialize, Clone)]
struct EvenementResultat {
    id: String,
    source: String,
    list: Vec<Value>,
}

/// Issue d'une source : suffisante pour le résumé, sans transmettre de fiches.
enum Issue {
    Resultats,
    Vide,
    Echec,
}

/// Interroge **une** source, et émet son événement si elle répond quelque chose.
async fn interroger_source(
    sup: Arc<Supervisor>,
    cache: Arc<PageCache>,
    app: AppHandle,
    id: String,
    source: String,
    query: String,
    content_rating: Option<String>,
) -> Issue {
    let mut params = json!({
        "source": source,
        "query": query,
        "offset": 0,
        "limit": PAR_SOURCE,
        // Même désarmement du repli `SAFE` que `latest` : sans cette clé, une
        // option « Toutes » devenait du SAFE et les sources adultes ne
        // renvoyaient rien (mesuré sur WAMANGA, cf. `latest_params`).
        "safeOnly": false,
    });
    // La classification est envoyée **exactement** comme `latest` la construit
    // : la divergence `search` / `latest` sur ce réglage avait déjà coûté une
    // régression (commandes mortes retirées le 2026-10-04).
    if let Some(rating) = content_rating {
        params["contentRating"] = json!(rating);
    }
    let reponse = match sup.request("search", params, DELAI_REQUETE).await {
        Ok(enveloppe) => match into_result(enveloppe) {
            Ok(mut resultat) => {
                // Le jeton de couverture est posé par Rust pour la même raison
                // que partout ailleurs : la webview ne doit jamais appeler un
                // site de manga (pas de user-agent, pas de cookies, échec
                // silencieux). Sans cet appel, toutes les vignettes de la
                // recherche globale seraient vides, sans aucune erreur.
                ajouter_jetons_couverture(&cache, &source, &mut resultat);
                let list = resultat
                    .get("list")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                if list.is_empty() {
                    Issue::Vide
                } else {
                    app.emit(
                        CANAL_RESULTAT,
                        EvenementResultat {
                            id,
                            source: source.clone(),
                            list,
                        },
                    )
                    .ok();
                    Issue::Resultats
                }
            }
            Err(raison) => {
                tracing::debug!("recherche {source} : {raison}");
                Issue::Echec
            }
        },
        Err(raison) => {
            // `request`, pas `request_avec_transport` ni `request_with_retry` :
            // une passe globale ne doit pas ouvrir 1370 fenêtres de
            // vérification, ni tripler le trafic vers des hôtes morts. Un
            // échec est compté, c'est tout.
            tracing::debug!("recherche {source} : {raison}");
            Issue::Echec
        }
    };
    reponse
}

/// Cherche `query` sur toutes les sources actives.
///
/// `id` est généré par l'interface et renvoyé tel quel dans chaque événement :
/// c'est ce qui lui permet d'ignorer les résultats d'une passe qu'elle a
/// remplacée. Une passe périmée continue jusqu'à son prochain lot — assez
/// court pour ne pas peser, et sans risque de mélange grâce à l'`id`.
// 8 paramètres, comme `latest` : chaque état Tauri est injecté par le
// gestionnaire, et en retirer un (regrouper `store` et `sup` dans une seule
// structure) déplacerait la complexité sans la réduire.
#[allow(clippy::too_many_arguments)]
#[tauri::command]
pub async fn recherche_globale(
    app: AppHandle,
    sup: State<'_, Arc<Supervisor>>,
    store: State<'_, Arc<Store>>,
    cache: State<'_, Arc<PageCache>>,
    passe: State<'_, Arc<PasseEnCours>>,
    id: String,
    query: String,
    content_rating: Option<String>,
) -> Result<Resume, String> {
    let query = valider_requete(&query)?;
    let annulee = passe.demarrer();
    let debut = Instant::now();

    let catalogue = into_result(
        sup.request_with_retry("listSources", json!({}), LOCAL_TIMEOUT)
            .await?,
    )?;
    let desactivees = store.disabled_sources()?;
    let sources = sources_a_interroger(
        catalogue
            .get("sources")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .unwrap_or(&[]),
        &desactivees,
    );
    let rating = requested_rating(content_rating.as_deref());

    let mut sources_interrogees = 0usize;
    let mut sources_avec_resultats = 0usize;
    let mut sources_en_echec = 0usize;
    let mut partielle = false;

    for lot in sources.chunks(CONCURRENCE) {
        // L'arrêt est vérifié **entre** les lots. Une requête déjà partie va
        // jusqu'à son propre délai : l'interrompre aurait laissé un thread
        // moteur écrire sur un récepteur abandonné.
        if annulee.load(Ordering::SeqCst) || debut.elapsed() >= DUREE_MAX {
            partielle = true;
            break;
        }
        let mut taches = JoinSet::new();
        for source in lot {
            let sup = sup.inner().clone();
            let cache = cache.inner().clone();
            let app = app.clone();
            let id = id.clone();
            let source = source.clone();
            let query = query.clone();
            let rating = rating.clone();
            taches.spawn(async move {
                interroger_source(sup, cache, app, id, source, query, rating).await
            });
        }
        while let Some(issue) = taches.join_next().await {
            sources_interrogees += 1;
            match issue {
                Ok(Issue::Resultats) => sources_avec_resultats += 1,
                Ok(Issue::Vide) => {}
                // Une tâche annulée (panique du runtime) compte en échec :
                // elle n'a rendu aucun résultat.
                Ok(Issue::Echec) | Err(_) => sources_en_echec += 1,
            }
        }
    }

    tracing::info!(
        "recherche {id} « {query} » : {sources_interrogees} sources, \
         {sources_avec_resultats} avec résultats, {sources_en_echec} en échec, \
         partielle={partielle}"
    );
    Ok(Resume {
        id,
        sources_interrogees,
        sources_avec_resultats,
        sources_en_echec,
        partielle,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source(nom: &str) -> Value {
        json!({ "name": nom, "lang": "fr" })
    }

    // ---- valider_requete -------------------------------------------------

    #[test]
    fn une_requete_vide_est_refusee() {
        assert!(valider_requete("").is_err());
        assert!(valider_requete("   ").is_err());
    }

    #[test]
    fn une_requete_trop_courte_est_refusee_et_nomme_le_minimum() {
        let erreur = valider_requete(" a ").expect_err("une lettre doit être refusée");
        assert!(
            erreur.contains(&LONGUEUR_MIN.to_string()),
            "le message doit nommer le minimum, obtenu : {erreur}"
        );
    }

    #[test]
    fn la_requete_est_rognee() {
        assert_eq!(valider_requete("  one piece ").unwrap(), "one piece");
    }

    #[test]
    fn une_requette_de_la_longueur_minimale_est_acceptee() {
        assert_eq!(valider_requete("go").unwrap(), "go");
    }

    // ---- sources_a_interroger ---------------------------------------------

    #[test]
    fn une_source_desactivee_est_ecartee() {
        let liste = vec![source("MANGADEX"), source("REMANGA")];
        assert_eq!(
            sources_a_interroger(&liste, &["REMANGA".to_string()]),
            vec!["MANGADEX".to_string()]
        );
    }

    #[test]
    fn les_sources_perso_sont_conservees() {
        // `custom:` est un préfixe de nom, pas une exclusion : le moteur route
        // `search` vers le script (Server.kt:804-811). Une source importée
        // absente de la recherche globale serait un défaut silencieux.
        let liste = vec![source("custom:mon-script")];
        assert_eq!(
            sources_a_interroger(&liste, &[]),
            vec!["custom:mon-script".to_string()]
        );
    }

    #[test]
    fn un_doublon_nest_interroge_quune_fois() {
        let liste = vec![source("MANGADEX"), source("MANGADEX")];
        assert_eq!(
            sources_a_interroger(&liste, &[]),
            vec!["MANGADEX".to_string()]
        );
    }

    #[test]
    fn sans_source_la_liste_est_vide() {
        assert!(sources_a_interroger(&[], &[]).is_empty());
    }

    #[test]
    fn toutes_les_sources_desactivees_rendent_une_liste_vide() {
        let liste = vec![source("A"), source("B")];
        assert!(sources_a_interroger(&liste, &["A".to_string(), "B".to_string()]).is_empty());
    }

    #[test]
    fn une_entree_sans_nom_est_ignoree() {
        let liste = vec![json!({ "lang": "fr" }), source("OK")];
        assert_eq!(sources_a_interroger(&liste, &[]), vec!["OK".to_string()]);
    }

    // ---- PasseEnCours -------------------------------------------------------

    #[test]
    fn une_nouvelle_passe_annule_la_precedente() {
        let passe = PasseEnCours::default();
        let premiere = passe.demarrer();
        assert!(
            !premiere.load(Ordering::SeqCst),
            "une passe neuve n'est pas annulée"
        );
        let seconde = passe.demarrer();
        assert!(
            premiere.load(Ordering::SeqCst),
            "l'ancienne passe doit être annulée par la nouvelle"
        );
        assert!(
            !seconde.load(Ordering::SeqCst),
            "la nouvelle passe doit rester active"
        );
    }

    #[test]
    fn trois_passes_ne_laissent_que_la_derniere_active() {
        let passe = PasseEnCours::default();
        let un = passe.demarrer();
        let deux = passe.demarrer();
        let trois = passe.demarrer();
        assert!(un.load(Ordering::SeqCst));
        assert!(deux.load(Ordering::SeqCst));
        assert!(!trois.load(Ordering::SeqCst));
    }
}
