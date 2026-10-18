//! Commandes Tauri du magasin de données (favoris, historique, reprise).
//!
//! Elles sont fines : valident, appellent `store::Store`, renvoient un type
//! sérialisable. Toute la logique est dans `store.rs`, testable sans Tauri.

use serde::Serialize;
use std::sync::Arc;
use tauri::State;

use crate::pagecache::jeton_cover as jeton;
use crate::pagecache::PageCache;
use crate::store::{Bookmark, Category, Favorite, HistoryEntry, Store};

/// Nombre d'entrées renvoyées quand l'interface ne le précise pas.
const RECENT_LIMIT: u32 = 24;

fn vide(v: &str) -> bool {
    v.trim().is_empty()
}

/// Résout un paramètre obligatoire sous peine d'erreur explicite.
fn requis(source: &str, manga_url: &str) -> Result<(), String> {
    if vide(source) {
        return Err("PARSE_ERROR: source vide".to_string());
    }
    if vide(manga_url) {
        return Err("PARSE_ERROR: manga_url vide".to_string());
    }
    Ok(())
}

/// Sources que l'utilisateur a écartées. Liste vide = toutes actives.
#[tauri::command]
pub async fn list_disabled_sources(store: State<'_, Arc<Store>>) -> Result<Vec<String>, String> {
    store.disabled_sources()
}

/// Active ou désactive une source. Une source jamais touchée est active.
#[tauri::command]
pub async fn set_source_enabled(
    store: State<'_, Arc<Store>>,
    source: String,
    enabled: bool,
) -> Result<(), String> {
    if vide(&source) {
        return Err("PARSE_ERROR: source vide".to_string());
    }
    store.set_source_enabled(source.trim(), enabled)
}

/// Réactive toutes les sources d'un coup.
#[tauri::command]
pub async fn enable_all_sources(store: State<'_, Arc<Store>>) -> Result<(), String> {
    store.enable_all_sources()
}

/// Catégories de favoris, dans l'ordre choisi par l'utilisateur.
#[tauri::command]
pub async fn list_categories(store: State<'_, Arc<Store>>) -> Result<Vec<Category>, String> {
    store.list_categories()
}

/// Crée une catégorie. La couleur est un index de palette (1..=8), pas une
/// valeur CSS : elle est bornée ici pour que l'interface ait toujours une
/// variable à traduire.
#[tauri::command]
pub async fn add_category(
    store: State<'_, Arc<Store>>,
    name: String,
    color: u8,
) -> Result<Category, String> {
    let nom = nom_categorie(&name)?;
    store.add_category(nom, color.clamp(1, 8))
}

/// Renomme et/ou recolore une catégorie.
#[tauri::command]
pub async fn rename_category(
    store: State<'_, Arc<Store>>,
    id: i64,
    name: String,
    color: u8,
) -> Result<(), String> {
    let nom = nom_categorie(&name)?;
    store.rename_category(id, nom, color.clamp(1, 8))
}

/// Supprime une catégorie ; ses favoris reviennent en « sans catégorie ».
#[tauri::command]
pub async fn delete_category(store: State<'_, Arc<Store>>, id: i64) -> Result<(), String> {
    store.delete_category(id)
}

/// Réécrit l'ordre des catégories à partir de la liste d'identifiants.
#[tauri::command]
pub async fn reorder_categories(store: State<'_, Arc<Store>>, ids: Vec<i64>) -> Result<(), String> {
    store.reorder_categories(&ids)
}

/// Affecte (ou retire) la catégorie d'un favori.
#[tauri::command]
pub async fn set_favorite_category(
    store: State<'_, Arc<Store>>,
    source: String,
    manga_url: String,
    category_id: Option<i64>,
) -> Result<(), String> {
    requis(&source, &manga_url)?;
    store.set_favorite_category(source.trim(), manga_url.trim(), category_id)
}

/// Nom de catégorie valide : non vide et borné. La longueur maximale évite une
/// étiquette qui déborde partout ailleurs.
fn nom_categorie(name: &str) -> Result<&str, String> {
    let n = name.trim();
    if n.is_empty() {
        return Err("PARSE_ERROR: nom de catégorie vide".to_string());
    }
    if n.chars().count() > 30 {
        return Err("PARSE_ERROR: nom de catégorie trop long (30 caractères max)".to_string());
    }
    Ok(n)
}

/// Ajoute un signet sur la page courante.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn add_bookmark(
    store: State<'_, Arc<Store>>,
    source: String,
    manga_url: String,
    chapter_url: String,
    title: String,
    cover_url: Option<String>,
    chapter_title: Option<String>,
    page_index: u16,
    page_count: u16,
) -> Result<(), String> {
    requis(&source, &manga_url)?;
    if vide(&chapter_url) {
        return Err("PARSE_ERROR: chapter_url vide".to_string());
    }
    store.add_bookmark(
        source.trim(),
        manga_url.trim(),
        chapter_url.trim(),
        title.trim(),
        cover_url.as_deref(),
        chapter_title.as_deref(),
        page_index,
        page_count,
    )
}

/// Retire le signet d'une page précise.
#[tauri::command]
pub async fn remove_bookmark(
    store: State<'_, Arc<Store>>,
    source: String,
    manga_url: String,
    chapter_url: String,
    page_index: u16,
) -> Result<(), String> {
    requis(&source, &manga_url)?;
    store.remove_bookmark(
        source.trim(),
        manga_url.trim(),
        chapter_url.trim(),
        page_index,
    )
}

/// Signets, du plus récent au plus ancien, éventuellement limités à un webtoon.
#[tauri::command]
pub async fn list_bookmarks(
    store: State<'_, Arc<Store>>,
    cache: State<'_, Arc<PageCache>>,
    limit: Option<u32>,
    manga_url: Option<String>,
    hide_adult: Option<bool>,
) -> Result<Vec<Bookmark>, String> {
    let n = limit.unwrap_or(RECENT_LIMIT).min(200);
    let filtre = manga_url
        .as_deref()
        .map(str::trim)
        .filter(|u| !u.is_empty());
    let mut lignes = store.list_bookmarks(n, filtre, hide_adult.unwrap_or(false))?;
    for l in lignes.iter_mut() {
        l.cover_token = jeton(&cache, &l.source, &l.manga_url, l.cover_url.as_deref());
    }
    Ok(lignes)
}

#[derive(Serialize)]
pub struct FavoriteState {
    pub source: String,
    pub manga_url: String,
    pub title: String,
    pub favorite: bool,
}

#[tauri::command]
pub async fn list_favorites(
    store: State<'_, Arc<Store>>,
    cache: State<'_, Arc<PageCache>>,
    hide_adult: Option<bool>,
) -> Result<Vec<Favorite>, String> {
    let mut lignes = store.list_favorites(hide_adult.unwrap_or(false))?;
    for l in lignes.iter_mut() {
        l.cover_token = jeton(&cache, &l.source, &l.manga_url, l.cover_url.as_deref());
    }
    Ok(lignes)
}

// ---------- Réglages ----------

/// Clé du réglage qui porte le dossier de téléchargements.
const CLE_DOSSIER_TELECHARGEMENTS: &str = "dossier_telechargements";

/// Dossier de téléchargements en vigueur.
///
/// `chemin` est restitué tel que l'utilisateur l'a choisi, pas sous sa forme
/// canonique : un chemin lisible dans les réglages vaut mieux qu'un
/// `\\?\C:\...`. Le confinement, lui, se fait toujours sur la forme canonique.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DossierTelechargements {
    pub chemin: String,
    pub par_defaut: bool,
}

/// Dossier de téléchargements actuellement utilisé.
#[tauri::command]
pub async fn get_downloads_dir(
    store: State<'_, Arc<Store>>,
) -> Result<DossierTelechargements, String> {
    let enregistre = store.get_setting(CLE_DOSSIER_TELECHARGEMENTS)?;
    let par_defaut = enregistre.as_deref().is_none_or(str::is_empty);
    let chemin = if par_defaut {
        crate::download::downloads_root_par_defaut()
    } else {
        std::path::PathBuf::from(enregistre.unwrap_or_default())
    };
    Ok(DossierTelechargements {
        chemin: chemin.to_string_lossy().to_string(),
        par_defaut,
    })
}

/// Change le dossier de téléchargements, définitivement.
///
/// Le chemin est **validé avant d'être enregistré** : dossier inaccessible en
/// écriture → refus, et rien n'est écrit en base. Enregistrer un chemin invalide
/// ferait échouer tous les téléchargements *ensuite*, sans retour possible depuis
/// l'interface — le pire moment pour le découvrir.
///
/// Le dossier est créé s'il n'existe pas : choisir `D:\Manga` avant d'avoir
/// créé `D:\Manga` est un geste normal, pas une erreur.
#[tauri::command]
pub async fn set_downloads_dir(
    store: State<'_, Arc<Store>>,
    chemin: String,
) -> Result<DossierTelechargements, String> {
    let demande = std::path::PathBuf::from(chemin.trim());
    if demande.as_os_str().is_empty() {
        return Err("chemin vide".to_string());
    }
    if !demande.exists() {
        std::fs::create_dir_all(&demande)
            .map_err(|e| format!("création du dossier impossible : {e}"))?;
    }
    let canonique = crate::download::verifier_dossier_ecriture(&demande)?;
    store.set_setting(CLE_DOSSIER_TELECHARGEMENTS, &canonique.to_string_lossy())?;
    crate::download::definir_racine_telechargements(canonique);
    Ok(DossierTelechargements {
        chemin: demande.to_string_lossy().to_string(),
        par_defaut: false,
    })
}

/// Restaure le dossier par défaut (`%APPDATA%\Tatami\downloads`).
#[tauri::command]
pub async fn reset_downloads_dir(store: State<'_, Arc<Store>>) -> Result<String, String> {
    store.set_setting(CLE_DOSSIER_TELECHARGEMENTS, "")?;
    let defaut = crate::download::downloads_root_par_defaut();
    crate::download::definir_racine_telechargements(defaut.clone());
    Ok(defaut.to_string_lossy().to_string())
}

/// Ajoute ou retire un favori. Le retour indique l'état obtenu, pour que
/// l'interface n'ait pas à refaire une requête.
#[tauri::command]
pub async fn set_favorite(
    store: State<'_, Arc<Store>>,
    source: String,
    manga_url: String,
    title: String,
    cover_url: Option<String>,
    content_rating: Option<String>,
    favorite: bool,
) -> Result<FavoriteState, String> {
    requis(&source, &manga_url)?;
    if vide(&title) {
        return Err("PARSE_ERROR: title vide".to_string());
    }
    // Une classification inconnue ne doit pas remplacer une valeur connue.
    let rating = content_rating.filter(|c| !c.eq_ignore_ascii_case("UNKNOWN"));
    store.set_favorite(
        source.trim(),
        manga_url.trim(),
        title.trim(),
        cover_url.as_deref(),
        rating.as_deref(),
        favorite,
    )?;
    Ok(FavoriteState {
        source: source.trim().to_string(),
        manga_url: manga_url.trim().to_string(),
        title: title.trim().to_string(),
        favorite,
    })
}

/// Enregistre la position de lecture. Appelée à chaque changement de chapitre
/// ou de page, elle doit être bon marché : c'est un `INSERT ... ON CONFLICT`.
#[tauri::command]
#[allow(clippy::too_many_arguments)]
pub async fn save_progress(
    store: State<'_, Arc<Store>>,
    source: String,
    manga_url: String,
    title: String,
    cover_url: Option<String>,
    content_rating: Option<String>,
    chapter_url: Option<String>,
    chapter_title: Option<String>,
    chapter_index: Option<i64>,
    page_index: Option<i64>,
    page_count: Option<i64>,
) -> Result<(), String> {
    requis(&source, &manga_url)?;
    if vide(&title) {
        return Err("PARSE_ERROR: title vide".to_string());
    }
    store.save_progress(
        source.trim(),
        manga_url.trim(),
        title.trim(),
        cover_url.as_deref(),
        content_rating.as_deref(),
        chapter_url.as_deref(),
        chapter_title.as_deref(),
        chapter_index.unwrap_or(0),
        page_index.unwrap_or(0),
        page_count.unwrap_or(0),
    )
}

/// Mémorise la classification d'un titre déjà présent dans l'historique.
/// Appelée quand la fiche est chargée : c'est là que le classement est fiable
/// (mesuré, la liste renvoie `UNKNOWN` là où la fiche dit `ADULT`).
#[tauri::command]
pub async fn note_content_rating(
    store: State<'_, Arc<Store>>,
    source: String,
    manga_url: String,
    content_rating: String,
) -> Result<(), String> {
    requis(&source, &manga_url)?;
    // Une source qui ne classe rien ne doit pas écraser une valeur connue.
    if vide(&content_rating) || content_rating.eq_ignore_ascii_case("UNKNOWN") {
        return Ok(());
    }
    store.note_content_rating(source.trim(), manga_url.trim(), content_rating.trim())
}

#[tauri::command]
pub async fn list_recent(
    store: State<'_, Arc<Store>>,
    cache: State<'_, Arc<PageCache>>,
    limit: Option<i64>,
    hide_adult: Option<bool>,
) -> Result<Vec<HistoryEntry>, String> {
    let mut lignes = store.list_recent(limit.unwrap_or(24), hide_adult.unwrap_or(false))?;
    for l in lignes.iter_mut() {
        l.cover_token = jeton(&cache, &l.source, &l.manga_url, l.cover_url.as_deref());
    }
    Ok(lignes)
}

/// Derniere lecture d'un webtoon, pour la fiche et la liste des chapitres.
///
/// `save_progress` ecrit la position depuis le lecteur (`page.tsx:3313`), et
/// aucune commande ne la relisait. Mesure le 2026-10-09 : retourner sur la
/// fiche ne montrait ni ou on en etait, ni un moyen d'y revenir ; et la liste
/// des chapitres ne marquait pas le chapitre courant. La donnee etait en
/// base, le raccord manquait.
#[tauri::command]
pub async fn derniere_lecture(
    store: State<'_, Arc<Store>>,
    cache: State<'_, Arc<PageCache>>,
    source: String,
    manga_url: String,
) -> Result<Option<HistoryEntry>, String> {
    requis(&source, &manga_url)?;
    let mut ligne = store.derniere_lecture(source.trim(), manga_url.trim())?;
    // Meme traitement que `list_recent` : la couverture passe par un jeton
    // `tatami://`, jamais par l'URL distante. Sans lui, une reprise affichee
    // sur la fiche ferait un appel direct a l'hote d'images.
    if let Some(l) = ligne.as_mut() {
        l.cover_token = jeton(&cache, &l.source, &l.manga_url, l.cover_url.as_deref());
    }
    Ok(ligne)
}

#[tauri::command]
pub async fn forget_history(
    store: State<'_, Arc<Store>>,
    source: String,
    manga_url: String,
) -> Result<(), String> {
    requis(&source, &manga_url)?;
    store.forget(source.trim(), manga_url.trim())
}

#[tauri::command]
pub async fn clear_history(store: State<'_, Arc<Store>>) -> Result<(), String> {
    store.forget_all()
}
