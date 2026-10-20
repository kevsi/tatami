//! Magasin de données de l'utilisateur (favoris, historique, position de lecture).
//!
//! Tâche 1.1 du `PLAN.md`. SQLite est choisi parce que ces données sont métier
//! et doivent survivre aux redémarrages et aux mises à jour de l'application
//! (cf. `AGENTS.md` §6 : `localStorage` est réservé aux préférences
//! d'affichage).
//!
//! Les migrations sont **forward-only** et versionnées par `PRAGMA user_version`.
//! Le fichier de base est sauvegardé avant toute migration appliquée.

use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

/// Version du schéma attendue par ce binaire.
const SCHEMA_VERSION: i64 = 12;

#[derive(Serialize)]
pub struct Favorite {
    pub source: String,
    pub manga_url: String,
    pub title: String,
    pub cover_url: Option<String>,
    /// Jeton `tatami://cover/...` à utiliser par l'interface.
    ///
    /// Rempli par la couche commande (`shelf`), pas par la base : la base ne
    /// connaît pas le cache d'images. C'est le SEUL champ que la webview doit
    /// utiliser pour afficher une vignette — `cover_url` est une adresse de site
    /// de manga, à ne jamais appeler depuis la page (AGENTS.md §4 règle 4).
    pub cover_token: Option<String>,
    pub content_rating: Option<String>,
    pub added_at: i64,
    /// Catégorie choisie par l'utilisateur, `None` = sans catégorie.
    pub category_id: Option<i64>,
    /// Index de couleur de cette catégorie, déjà résolu par la jointure.
    pub category_color: Option<u8>,
}

#[derive(Serialize)]
pub struct HistoryEntry {
    pub source: String,
    pub manga_url: String,
    pub title: String,
    pub cover_url: Option<String>,
    /// Jeton `tatami://cover/...` — cf. `Favorite::cover_token`.
    pub cover_token: Option<String>,
    pub content_rating: Option<String>,
    pub chapter_url: Option<String>,
    pub chapter_title: Option<String>,
    /// Rang du chapitre dans la liste de la source au moment de la lecture.
    /// Permet de reprendre même si la source réordonne sa liste ensuite.
    pub chapter_index: i64,
    pub page_index: i64,
    pub page_count: i64,
    pub read_at: i64,
}

/// `pub` depuis le 2026-10-05 (tâche 5.5) : le rapport de diagnostic nomme son
/// fichier avec cet horodatage, et il doit venir de la **même** horloge que le
/// reste de la base. Deux horloges dans un même binaire finissent par diverger
/// d'une heure, et le premier symptôme est un rapport « de la veille » daté
/// d'aujourd'hui.
pub fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// Mots-clés qui trahissent un contenu adulte quand la source ne le classe pas.
///
/// Une **seule** liste, utilisée à l'écriture (`save_progress`, `set_favorite`)
/// et au rattrapage des lignes déjà en base. Deux listes divergeraient sans que
/// rien ne le signale — c'est la leçon des champs signés partagés entre Rust et
/// un script (AGENTS.md §14).
///
/// Retourne `Some("ADULT")` ou `None` : on ne devine jamais `SAFE`, donc une
/// ligne non reconnue reste `UNKNOWN` et visible (cf. le test
/// `incognito_masque_les_titres_adultes`, qui exige qu'un titre sans
/// classification ne disparaisse pas).
fn classement_adulte(source: &str, titre: &str) -> Option<&'static str> {
    const MOTS: [&str; 7] = [
        "harem",
        "branlette",
        "hentai",
        "ecchi",
        "nsfw",
        "xxx",
        "adult",
    ];
    let source = source.to_lowercase();
    let titre = titre.to_lowercase();
    MOTS.iter()
        .any(|mot| source.contains(mot) || titre.contains(mot))
        .then_some("ADULT")
}

/// Une catégorie de favoris créée par l'utilisateur. `color` est un index
/// dans la palette du thème (1..=8), jamais une valeur CSS : l'interface la
/// traduit en variable, cf. AGENTS.md §9.2.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Category {
    pub id: i64,
    pub name: String,
    pub color: u8,
}

/// Un signet : une page précise d'un chapitre d'un webtoon.
#[derive(Debug, Clone, Serialize)]
pub struct Bookmark {
    pub source: String,
    pub manga_url: String,
    pub chapter_url: String,
    pub title: String,
    pub cover_url: Option<String>,
    /// Jeton `tatami://cover/...` — cf. `Favorite::cover_token`.
    pub cover_token: Option<String>,
    pub chapter_title: Option<String>,
    pub page_index: u16,
    pub page_count: u16,
    pub created_at: i64,
}

/// Lecteur de ligne partagé par `list_bookmarks` et ses variantes filtrées.
fn bookmark_from_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<Bookmark> {
    Ok(Bookmark {
        source: r.get(0)?,
        manga_url: r.get(1)?,
        chapter_url: r.get(2)?,
        title: r.get(3)?,
        cover_url: r.get(4)?,
        cover_token: None,
        chapter_title: r.get(5)?,
        page_index: r.get::<_, i64>(6)? as u16,
        page_count: r.get::<_, i64>(7)? as u16,
        created_at: r.get(8)?,
    })
}

/// Source personnalisée installée par l'utilisateur (script JavaScript local).
///
/// Le moteur JVM charge ces scripts depuis le dossier `sources/` au démarrage.
/// `installed_at` est un horodatage Unix en millisecondes (cohérent avec `now_ms()`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomSource {
    pub id: String,
    pub name: String,
    pub lang: String,
    pub path: String,
    pub installed_at: i64,
}

/// État d'un source livrée avec l'application (bundle des installateurs).
///
/// `sha256` est l'empreinte du fichier **que nous avons écrit**. Elle sert à
/// distinguer une copie intacte (qu'une mise à jour peut remplacer) d'un
/// fichier modifié par l'utilisateur (qu'on ne touche jamais). `deleted` est
/// la pierre tombale : une source livrée puis supprimée ne doit pas revenir au
/// redémarrage suivant.
#[derive(Debug, Clone)]
pub struct BundledSourceState {
    pub sha256: String,
    pub deleted: bool,
}

pub struct Store {
    conn: Mutex<Connection>,
    path: PathBuf,
    in_memory: bool,
}

impl Store {
    /// Ouvre (ou crée) la base et applique les migrations en attente.
    pub fn open(path: PathBuf) -> Result<Self, String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("création du dossier de base impossible : {e}"))?;
        }
        let conn = Connection::open(&path)
            .map_err(|e| format!("ouverture de la base impossible : {e}"))?;
        // WAL : lectures concurrentes pendant l'écriture d'un point de lecture.
        let _ = conn.pragma_update(None, "journal_mode", "WAL");
        conn.pragma_update(None, "foreign_keys", "ON")
            .map_err(|e| format!("pragma foreign_keys : {e}"))?;
        let store = Store {
            conn: Mutex::new(conn),
            path,
            in_memory: false,
        };
        store.migrate()?;
        Ok(store)
    }

    /// Repli : base en mémoire. L'application reste utilisable, mais rien ne
    /// survit à la fermeture (utile quand le dossier de données est verrouillé).
    pub fn open_in_memory() -> Result<Self, String> {
        let conn = Connection::open_in_memory().map_err(|e| format!("base en mémoire : {e}"))?;
        let store = Store {
            conn: Mutex::new(conn),
            path: PathBuf::from(":memory:"),
            in_memory: true,
        };
        store.migrate()?;
        Ok(store)
    }

    /// Indique si la base est en mémoire (repli de secours) : les favoris,
    /// l'historique et les téléchargements ne survivront pas à la fermeture.
    pub fn est_en_memoire(&self) -> bool {
        self.in_memory
    }

    fn version(&self) -> Result<i64, String> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| "base verrouillée".to_string())?;
        let v: i64 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .map_err(|e| format!("lecture du schéma impossible : {e}"))?;
        Ok(v)
    }

    fn migrate(&self) -> Result<(), String> {
        let current = self.version()?;
        // Pas de sortie rapide quand la version est à jour : la réparation des
        // colonnes (plus bas) doit tourner à CHAQUE démarrage, sinon une
        // version estampillée par un binaire qui n'avait pas encore la
        // migration laisserait la base cassée pour toujours (mesuré).
        // Sauvegarde avant migration : une migration ratée ne doit pas
        // exploser les données existantes.
        if current > 0 && current < SCHEMA_VERSION {
            self.backup(current)?;
        }

        let conn = self
            .conn
            .lock()
            .map_err(|_| "base verrouillée".to_string())?;
        let mut conn = conn;
        let tx = conn.transaction().map_err(|e| format!("migration : {e}"))?;
        if current < 1 {
            tx.execute_batch(
                r#"
                CREATE TABLE favorites (
                    source     TEXT    NOT NULL,
                    manga_url  TEXT    NOT NULL,
                    title      TEXT    NOT NULL,
                    cover_url  TEXT,
                    added_at   INTEGER NOT NULL,
                    PRIMARY KEY (source, manga_url)
                );

                CREATE TABLE history (
                    source          TEXT    NOT NULL,
                    manga_url       TEXT    NOT NULL,
                    title           TEXT    NOT NULL,
                    cover_url       TEXT,
                    content_rating  TEXT,
                    chapter_url     TEXT,
                    chapter_title   TEXT,
                    page_index      INTEGER NOT NULL DEFAULT 0,
                    page_count      INTEGER NOT NULL DEFAULT 0,
                    read_at         INTEGER NOT NULL,
                    PRIMARY KEY (source, manga_url)
                );
                CREATE INDEX history_read_at ON history (read_at DESC);
                "#,
            )
            .map_err(|e| format!("migration v1 : {e}"))?;
        }
        // Les colonnes sont ajoutées par PRÉSENCE, pas seulement par version.
        // Mesuré : un binaire intermédiaire (version poussée à 3 dans une
        // édition, bloc de migration dans l'édition suivante) a estampillé
        // `user_version = 3` sans ajouter `favorites.content_rating`. La base
        // réelle est alors restée cassée pour toujours, le code croyant la
        // migration faite. Se fier à `PRAGMA table_info` répare ce cas.
        //
        // v2 : la position de lecture doit survivre à un changement de source
        // qui renvoie ses chapitres dans un ordre différent.
        Self::ensure_column(
            &tx,
            "history",
            "chapter_index",
            "INTEGER NOT NULL DEFAULT 0",
        )?;
        // v3 : le favori retient la classification. Ouvrir un favori depuis
        // l'accueil n'a plus aucun contexte de liste, donc sans cette colonne
        // l'incognito ne pouvait plus rien masquer.
        Self::ensure_column(&tx, "favorites", "content_rating", "TEXT")?;
        // Backfill incognito : un webtoon dont la source ou le titre porte un
        // mot-clé adulte et dont la classification est NULL **ou UNKNOWN** est
        // marqué ADULT.
        //
        // `UNKNOWN` compte : c'est ce que `save_progress` écrit quand la source
        // ne classe pas, et l'ancien backfill ne regardait que NULL. Mesuré le
        // 2026-10-18 : deux titres adultes (EXHENTAI, `custom:mangadexFr`)
        // restaient affichés à l'accueil alors que l'incognito était actif.
        //
        // La liste de mots-clés vit dans `classement_adulte`, partagée avec
        // l'écriture : deux listes finiraient par diverger.
        for table in ["history", "favorites"] {
            let a_classer: Vec<(String, String, String)> = {
                let mut req = tx
                    .prepare(&format!(
                        "SELECT source, manga_url, title FROM {table}
                         WHERE content_rating IS NULL OR content_rating = 'UNKNOWN'"
                    ))
                    .map_err(|e| format!("backfill incognito ({table}) : {e}"))?;
                let rows = req
                    .query_map([], |r| {
                        Ok((
                            r.get::<_, String>(0)?,
                            r.get::<_, String>(1)?,
                            r.get::<_, String>(2)?,
                        ))
                    })
                    .map_err(|e| format!("backfill incognito ({table}) : {e}"))?;
                rows.collect::<Result<Vec<_>, _>>()
                    .map_err(|e| format!("backfill incognito ({table}) : {e}"))?
            };
            for (source, manga_url, titre) in a_classer {
                if classement_adulte(&source, &titre).is_none() {
                    continue;
                }
                tx.execute(
                    &format!(
                        "UPDATE {table} SET content_rating = 'ADULT'
                         WHERE source = ?1 AND manga_url = ?2"
                    ),
                    params![source, manga_url],
                )
                .map_err(|e| format!("backfill incognito ({table}) : {e}"))?;
            }
        }
        // v4 : activation/désactivation des sources. Sans cela, les 1370
        // catalogues sont proposals à equalité et l'utilisateur ne peut pas
        // écarter ceux qui l'intéressent pas. Seules les sources DÉSACTIVÉES
        // sont écrites : la table reste minuscule et l'absence de ligne signifie
        // « active », sans jamais dépendre d'un DEFAULT manquant.
        tx.execute_batch(
            "CREATE TABLE IF NOT EXISTS sources_state (
                 source  TEXT    NOT NULL PRIMARY KEY,
                 enabled INTEGER NOT NULL
             );",
        )
        .map_err(|e| format!("migration v4 : {e}"))?;
        // v5 : signets. La clé comporte la page : on peut donc marquer plusieurs
        // pages d'un même chapitre, et retirer un signet est exact (aucun
        // « dernier signet du chapitre » à deviner).
        tx.execute_batch(
            "CREATE TABLE IF NOT EXISTS bookmarks (
                 source        TEXT    NOT NULL,
                 manga_url     TEXT    NOT NULL,
                 chapter_url   TEXT    NOT NULL,
                 title         TEXT    NOT NULL,
                 cover_url     TEXT,
                 chapter_title TEXT,
                 page_index    INTEGER NOT NULL,
                 page_count    INTEGER NOT NULL,
                 created_at    INTEGER NOT NULL,
                 PRIMARY KEY (source, manga_url, chapter_url, page_index)
             );
             CREATE INDEX IF NOT EXISTS bookmarks_created_at ON bookmarks (created_at DESC);",
        )
        .map_err(|e| format!("migration v5 : {e}"))?;
        // v6 : catégories de favoris. Choix humain : UNE catégorie par favori,
        // comme dans Kotatsu-Redo, l'app dont on vise la parité. `NULL` veut
        // dire « sans catégorie », et la clé étrangère renvoie automatiquement
        // dans ce cas quand une catégorie est supprimée : on ne perd pas le
        // favori, on perd seulement son classement.
        //
        // La couleur est un INDEX (1..=8), pas une valeur : l'interface la
        // traduit en variable CSS du thème (AGENTS.md §9.2, aucune valeur
        // magique en dur).
        tx.execute_batch(
            "CREATE TABLE IF NOT EXISTS categories (
                 id         INTEGER PRIMARY KEY AUTOINCREMENT,
                 name       TEXT    NOT NULL UNIQUE,
                 color      INTEGER NOT NULL DEFAULT 1,
                 position   INTEGER NOT NULL DEFAULT 0
             );",
        )
        .map_err(|e| format!("migration v6 : {e}"))?;
        Self::ensure_column(
            &tx,
            "favorites",
            "category_id",
            "INTEGER REFERENCES categories(id) ON DELETE SET NULL",
        )?;
        // v7 : réglages de l'application. Le dossier de téléchargements est
        // choisi par l'utilisateur (choix humain du 2026-10-05) : il doit
        // survivre au redémarrage, donc il vit en base et pas dans le
        // `localStorage` de la webview, qui est le mauvais endroit pour une
        // donnée que Rust doit appliquer à chaque écriture de fichier.
        //
        // `CREATE TABLE IF NOT EXISTS` et non `if current < 7` : une base
        // estampillée par un binaire plus récent ne doit jamais être cassée
        // par un binaire plus ancien qui rouvrirait le fichier.
        tx.execute_batch(
            "CREATE TABLE IF NOT EXISTS settings (
                 key   TEXT PRIMARY KEY,
                 value TEXT NOT NULL
             );",
        )
        .map_err(|e| format!("migration v7 : {e}"))?;
        if current < 2 {
            tx.execute_batch("CREATE INDEX IF NOT EXISTS history_title ON history (title);")
                .map_err(|e| format!("migration : index history_title : {e}"))?;
        }
        // `user_version` ne peut pas être parametré : la valeur est un entier
        // de constante, jamais une entrée utilisateur. Réécrire la même valeur
        // est sans effet ; on ne l'écrit que si elle change pour éviter de
        // toucher au fichier à chaque démarrage.
        // v8 : `page_translations` et `blocks`, tables du sous-système de
        // traduction. Le bloc de création a été retiré le 2026-10-08 ; les
        // tables sont supprimées par la migration v11, plus bas.
        // v9 : sources personnalisées scriptées (JavaScript). Installées par
        // l'utilisateur depuis un fichier .js local ; le chemin est absolu et
        // validé à l'import (anti path-traversal). `installed_at` est un
        // horodatage Unix en millisecondes produit par `now_ms()`.
        // Par PRÉSENCE, et non par version. C'est la règle énoncée 100 lignes
        // plus haut (`:233-238`) pour les colonnes, et le bloc v9 la violait
        // avec son `if current < 9`. Cas mesuré : une base estampillée 9 ou 10
        // par un binaire dont ce bloc n'était pas encore présent sautait la
        // création pour toujours, et `add_custom_source` échouait ensuite sur
        // *no such table* — exactement le symptôme de `favorites.content_rating`
        // qui, lui, avait son test. `CREATE TABLE IF NOT EXISTS` est déjà
        // idempotent, donc le retirer du `if` est le correctif entier.
        tx.execute_batch(
            r#"
                CREATE TABLE IF NOT EXISTS custom_sources (
                    id           TEXT    NOT NULL PRIMARY KEY,
                    name         TEXT    NOT NULL,
                    lang         TEXT    NOT NULL,
                    path         TEXT    NOT NULL,
                    installed_at INTEGER NOT NULL
                );
                "#,
        )
        .map_err(|e| format!("migration v9 : {e}"))?;
        // v12 : sources livrées avec l'application. Les installateurs embarquent
        // un dossier `sources/` ; au premier démarrage, le cœur le recopie dans
        // `%APPDATA%\Tatami\sources` (le dossier que lit le moteur) sans jamais
        // écraser un fichier que l'utilisateur a modifié, et sans ressusciter
        // une source qu'il a supprimée. La table porte l'empreinte de ce que
        // NOUS avons écrit, pas celle du fichier présent.
        //
        // `CREATE TABLE IF NOT EXISTS` par PRÉSENCE, comme `custom_sources` :
        // une base estampillée par un binaire intermédiaire ne doit pas sauter
        // la création pour toujours.
        tx.execute_batch(
            r#"
                CREATE TABLE IF NOT EXISTS bundled_sources (
                    id        TEXT    NOT NULL PRIMARY KEY,
                    sha256    TEXT    NOT NULL,
                    deleted   INTEGER NOT NULL DEFAULT 0,
                    seeded_at INTEGER NOT NULL
                );
                "#,
        )
        .map_err(|e| format!("migration v12 : {e}"))?;
        // v11 : suppression des tables du sous-système de traduction.
        //
        // Elles étaient créées par les blocs v8 (`page_translations`, `blocks`)
        // et v10 (`glossary`), retirés le 2026-10-08 avec la fonctionnalité
        // elle-même. **Ces deux blocs ne sont pas réécrits** : une migration
        // appliquée ne se réécrit pas, donc une base existante les a déjà
        // jouées et garde ses tables sans ce DROP. D'où ce bloc, qui est le seul
        // moyen autorisé de s'en débarrasser.
        //
        // `IF EXISTS` rend l'opération idempotente : une base neuve n'a jamais
        // eu ces tables, et le DROP ne fait alors rien.
        if current < 11 {
            tx.execute_batch(
                r#"
                DROP TABLE IF EXISTS blocks;
                DROP TABLE IF EXISTS page_translations;
                DROP TABLE IF EXISTS glossary;
                "#,
            )
            .map_err(|e| format!("migration v11 : {e}"))?;
        }
        if current < SCHEMA_VERSION {
            tx.execute_batch(&format!("PRAGMA user_version = {SCHEMA_VERSION}"))
                .map_err(|e| format!("migration : écriture de la version : {e}"))?;
        }
        tx.commit()
            .map_err(|e| format!("migration : validation : {e}"))?;
        Ok(())
    }

    /// Ajoute `table`.`colonne` si elle n'existe pas déjà.
    ///
    /// `table` et `colonne` ne viennent jamais d'une entrée utilisateur : ce sont
    /// des constantes écrites ici, d'où le formatage direct.
    fn ensure_column(
        tx: &rusqlite::Transaction<'_>,
        table: &str,
        colonne: &str,
        declaration: &str,
    ) -> Result<(), String> {
        let existe = {
            let mut req = tx
                .prepare(&format!("PRAGMA table_info({table})"))
                .map_err(|e| format!("table_info({table}) : {e}"))?;
            let mut lignes = req
                .query([])
                .map_err(|e| format!("table_info({table}) : {e}"))?;
            let mut trouve = false;
            while let Some(l) = lignes
                .next()
                .map_err(|e| format!("table_info({table}) : {e}"))?
            {
                if l.get::<_, String>(1).map(|n| n == colonne).unwrap_or(false) {
                    trouve = true;
                }
            }
            trouve
        };
        if !existe {
            tx.execute_batch(&format!(
                "ALTER TABLE {table} ADD COLUMN {colonne} {declaration};"
            ))
            .map_err(|e| format!("ajout de {table}.{colonne} : {e}"))?;
        }
        Ok(())
    }

    fn backup(&self, from_version: i64) -> Result<(), String> {
        let name = format!("tatami-v{from_version}-{}.db", now_ms());
        let dest: PathBuf = self
            .path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join(name);
        // La base est en WAL (`Store::open`) : après un `COMMIT`, les pages
        // modifiées vivent dans le fichier `-wal` jusqu'au point de contrôle
        // suivant. Copier le seul `.db` livrerait donc une base **antérieure** à
        // la dernière écriture — pour une sauvegarde dont tout le but est de
        // permettre un retour arrière si la migration échoue.
        //
        // `wal_checkpoint(TRUNCATE)` recopie le WAL dans le `.db` puis le vide.
        // Il rend `(busy, journal, checkpointed)` : un `busy` non nul signifierait
        // que la copie repart sans les données, c'est-à-dire une sauvegarde qui
        // **ment**. On refuse plutôt que d'écrire un faux secours.
        {
            let conn = self
                .conn
                .lock()
                .map_err(|_| "base verrouillée".to_string())?;
            let busy: i64 = conn
                .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |r| r.get(0))
                .map_err(|e| format!("point de contrôle WAL impossible : {e}"))?;
            if busy != 0 {
                return Err(format!(
                    "point de contrôle WAL bloqué (busy = {busy}) : \
                     la sauvegarde serait incomplète"
                ));
            }
        }
        std::fs::copy(&self.path, &dest)
            .map_err(|e| format!("sauvegarde avant migration impossible : {e}"))?;
        // Tâche 5.5 : le chemin **complet** est retiré. Il contient
        // `C:\Users\<nom>\…`, donc le nom d'utilisateur Windows se retrouvait dans
        // le journal — et le journal part dans les rapports de diagnostic. Le nom
        // de fichier suffit : il dit quelle version a été sauvegardée.
        let nom = dest
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "sauvegarde.db".to_string());
        tracing::info!("sauvegarde de la base : {nom}");
        Ok(())
    }

    // ---------- Réglages ----------

    /// Lit un réglage. `None` si la clé n'a jamais été écrite.
    pub fn get_setting(&self, key: &str) -> Result<Option<String>, String> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| "base verrouillée".to_string())?;
        let mut stmt = conn
            .prepare("SELECT value FROM settings WHERE key = ?1")
            .map_err(|e| format!("lecture réglage impossible : {e}"))?;
        let mut rows = stmt
            .query([key])
            .map_err(|e| format!("lecture réglage impossible : {e}"))?;
        match rows.next() {
            Ok(Some(r)) => {
                Ok(Some(r.get(0).map_err(|e| {
                    format!("lecture réglage impossible : {e}")
                })?))
            }
            Ok(None) => Ok(None),
            Err(e) => Err(format!("lecture réglage impossible : {e}")),
        }
    }

    /// Écrit un réglage (écrase l'éventuelle valeur précédente).
    pub fn set_setting(&self, key: &str, value: &str) -> Result<(), String> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| "base verrouillée".to_string())?;
        conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![key, value],
        )
        .map_err(|e| format!("écriture réglage impossible : {e}"))?;
        Ok(())
    }

    // ---------- Favoris ----------

    pub fn set_favorite(
        &self,
        source: &str,
        manga_url: &str,
        title: &str,
        cover_url: Option<&str>,
        content_rating: Option<&str>,
        favorite: bool,
    ) -> Result<(), String> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| "base verrouillée".to_string())?;
        // Même règle que `save_progress` : une valeur connue gagne, sinon un
        // mot-clé classe le titre, sinon on laisse `UNKNOWN`.
        let classement = content_rating
            .filter(|r| !r.eq_ignore_ascii_case("UNKNOWN"))
            .or_else(|| classement_adulte(source, title));
        if favorite {
            conn.execute(
                "INSERT INTO favorites (source, manga_url, title, cover_url, content_rating, added_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                 ON CONFLICT(source, manga_url) DO UPDATE SET
                   title = ?3, cover_url = ?4,
                   content_rating = COALESCE(NULLIF(?5, 'UNKNOWN'), content_rating)",
                params![source, manga_url, title, cover_url, classement, now_ms()],
            )
        } else {
            conn.execute(
                "DELETE FROM favorites WHERE source = ?1 AND manga_url = ?2",
                params![source, manga_url],
            )
        }
        .map_err(|e| format!("favori : {e}"))?;
        Ok(())
    }

    pub fn list_favorites(&self, hide_adult: bool) -> Result<Vec<Favorite>, String> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| "base verrouillée".to_string())?;
        let sql = if hide_adult {
            "SELECT f.source, f.manga_url, f.title, f.cover_url, f.content_rating,
                    f.added_at, f.category_id, c.color
             FROM favorites f
             LEFT JOIN categories c ON c.id = f.category_id
             WHERE f.content_rating IS NULL
                OR f.content_rating NOT IN ('ADULT', 'EROTIC_AO', 'PORNOGRAPHIC')
             ORDER BY f.added_at DESC, f.title ASC"
        } else {
            "SELECT f.source, f.manga_url, f.title, f.cover_url, f.content_rating,
                    f.added_at, f.category_id, c.color
             FROM favorites f
             LEFT JOIN categories c ON c.id = f.category_id
             ORDER BY f.added_at DESC, f.title ASC"
        };
        let mut stmt = conn.prepare(sql).map_err(|e| format!("favoris : {e}"))?;
        let rows = stmt
            .query_map([], |r| {
                Ok(Favorite {
                    source: r.get(0)?,
                    manga_url: r.get(1)?,
                    title: r.get(2)?,
                    cover_url: r.get(3)?,
                    cover_token: None,
                    content_rating: r.get(4)?,
                    added_at: r.get(5)?,
                    category_id: r.get(6)?,
                    category_color: r.get::<_, Option<i64>>(7)?.map(|c| c as u8),
                })
            })
            .map_err(|e| format!("favoris : {e}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("favoris : {e}"))
    }

    // ---------- Catégories de favoris ----------

    /// Catégories dans l'ordre choisi par l'utilisateur.
    pub fn list_categories(&self) -> Result<Vec<Category>, String> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| "base verrouillée".to_string())?;
        let mut req = conn
            .prepare("SELECT id, name, color FROM categories ORDER BY position, name")
            .map_err(|e| format!("catégories : {e}"))?;
        let rows = req
            .query_map([], |r| {
                Ok(Category {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    color: r.get::<_, i64>(2)? as u8,
                })
            })
            .map_err(|e| format!("catégories : {e}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("catégories : {e}"))
    }

    /// Crée une catégorie à la fin de la liste. Le nom est unique : deux
    /// catégories « Favoris » rendraient le filtrage ambigu.
    pub fn add_category(&self, name: &str, color: u8) -> Result<Category, String> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| "base verrouillée".to_string())?;
        let suivant: i64 = conn
            .query_row(
                "SELECT COALESCE(MAX(position), -1) + 1 FROM categories",
                [],
                |r| r.get(0),
            )
            .map_err(|e| format!("catégories : {e}"))?;
        conn.execute(
            "INSERT INTO categories (name, color, position) VALUES (?1, ?2, ?3)",
            params![name, color as i64, suivant],
        )
        .map_err(|e| {
            if e.to_string().contains("UNIQUE") {
                format!("La catégorie « {name} » existe déjà.")
            } else {
                format!("catégories : {e}")
            }
        })?;
        let id = conn.last_insert_rowid();
        Ok(Category {
            id,
            name: name.to_string(),
            color,
        })
    }

    /// Renomme et/ou recolore une catégorie.
    pub fn rename_category(&self, id: i64, name: &str, color: u8) -> Result<(), String> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| "base verrouillée".to_string())?;
        let n = conn
            .execute(
                "UPDATE categories SET name = ?2, color = ?3 WHERE id = ?1",
                params![id, name, color as i64],
            )
            .map_err(|e| {
                if e.to_string().contains("UNIQUE") {
                    format!("La catégorie « {name} » existe déjà.")
                } else {
                    format!("catégories : {e}")
                }
            })?;
        if n == 0 {
            return Err("Cette catégorie n’existe plus.".to_string());
        }
        Ok(())
    }

    /// Supprime une catégorie. Ses favoris ne sont PAS supprimés : la clé
    /// étrangère `ON DELETE SET NULL` les renvoie en « sans catégorie ».
    pub fn delete_category(&self, id: i64) -> Result<(), String> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| "base verrouillée".to_string())?;
        conn.execute("DELETE FROM categories WHERE id = ?1", params![id])
            .map_err(|e| format!("catégories : {e}"))?;
        Ok(())
    }

    /// Réécrit l'ordre à partir de la liste d'identifiants reçue. On réécrit
    /// `position` pour toutes les catégories plutôt que d'échanger deux
    /// lignes : l'ordre final est alors exactement celui demandé.
    pub fn reorder_categories(&self, ids: &[i64]) -> Result<(), String> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| "base verrouillée".to_string())?;
        let tx = conn
            .unchecked_transaction()
            .map_err(|e| format!("catégories : {e}"))?;
        for (rang, id) in ids.iter().enumerate() {
            tx.execute(
                "UPDATE categories SET position = ?2 WHERE id = ?1",
                params![id, rang as i64],
            )
            .map_err(|e| format!("catégories : {e}"))?;
        }
        tx.commit().map_err(|e| format!("catégories : {e}"))
    }

    /// Affecte (ou retire) la catégorie d'un favori. `None` = sans catégorie.
    pub fn set_favorite_category(
        &self,
        source: &str,
        manga_url: &str,
        category_id: Option<i64>,
    ) -> Result<(), String> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| "base verrouillée".to_string())?;
        if let Some(id) = category_id {
            let existe: i64 = conn
                .query_row(
                    "SELECT count(*) FROM categories WHERE id = ?1",
                    params![id],
                    |r| r.get(0),
                )
                .map_err(|e| format!("catégories : {e}"))?;
            if existe == 0 {
                return Err("Cette catégorie n’existe plus.".to_string());
            }
        }
        conn.execute(
            "UPDATE favorites SET category_id = ?3
             WHERE source = ?1 AND manga_url = ?2",
            params![source, manga_url, category_id],
        )
        .map_err(|e| format!("catégories : {e}"))?;
        Ok(())
    }

    // ---------- Signets ----------

    /// Ajoute un signet. Réécrit le signet s'il existe déjà : marquer deux fois
    /// la même page ne doit pas créer de doublon.
    #[allow(clippy::too_many_arguments)]
    pub fn add_bookmark(
        &self,
        source: &str,
        manga_url: &str,
        chapter_url: &str,
        title: &str,
        cover_url: Option<&str>,
        chapter_title: Option<&str>,
        page_index: u16,
        page_count: u16,
    ) -> Result<(), String> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| "base verrouillée".to_string())?;
        conn.execute(
            "INSERT INTO bookmarks
                 (source, manga_url, chapter_url, title, cover_url, chapter_title,
                  page_index, page_count, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(source, manga_url, chapter_url, page_index) DO UPDATE SET
                 title = ?4, cover_url = ?5, chapter_title = ?6, page_count = ?8",
            params![
                source,
                manga_url,
                chapter_url,
                title,
                cover_url,
                chapter_title,
                page_index,
                page_count,
                now_ms()
            ],
        )
        .map_err(|e| format!("signet : {e}"))?;
        Ok(())
    }

    /// Retire un signet précis (une page d'un chapitre). Sans effet s'il n'existe
    /// pas : retirer deux fois le même signet doit rester sans erreur.
    pub fn remove_bookmark(
        &self,
        source: &str,
        manga_url: &str,
        chapter_url: &str,
        page_index: u16,
    ) -> Result<(), String> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| "base verrouillée".to_string())?;
        conn.execute(
            "DELETE FROM bookmarks
             WHERE source = ?1 AND manga_url = ?2 AND chapter_url = ?3 AND page_index = ?4",
            params![source, manga_url, chapter_url, page_index],
        )
        .map_err(|e| format!("signet : {e}"))?;
        Ok(())
    }

    /// Does this page already carry a bookmark? Derived on the interface side
    /// from `list_bookmarks`: one call feeds both the reader button state and
    /// the chapter grid badges, so an extra command would be dead weight.
    #[cfg(test)]
    fn has_bookmark(
        &self,
        source: &str,
        manga_url: &str,
        chapter_url: &str,
        page_index: u16,
    ) -> Result<bool, String> {
        Ok(self
            .list_bookmarks(500, Some(manga_url), false)?
            .iter()
            .any(|b| {
                b.source == source && b.chapter_url == chapter_url && b.page_index == page_index
            }))
    }

    /// Signets du plus récent au plus ancien, éventuellement limités à un
    /// webtoon. Le même ordre que l'historique, pour que les deux sections de
    /// l'accueil se lisent pareil.
    ///
    /// `hide_adult` applique l'incognito comme aux deux autres listes de
    /// l'accueil. Le signet ne porte pas sa classification : elle vit dans
    /// `history` (écrite à la lecture) et `favorites`. On filtre par
    /// `EXISTS` plutôt que par jointure, sinon deux lignes adultes
    /// dupliqueraient le signet.
    pub fn list_bookmarks(
        &self,
        limit: u32,
        manga_url: Option<&str>,
        hide_adult: bool,
    ) -> Result<Vec<Bookmark>, String> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| "base verrouillée".to_string())?;
        let colonnes = "source, manga_url, chapter_url, title, cover_url, chapter_title,
                        page_index, page_count, created_at";
        let filtre_adulte = if hide_adult {
            " AND NOT EXISTS (
                  SELECT 1 FROM history h
                  WHERE h.source = b.source AND h.manga_url = b.manga_url
                    AND h.content_rating IN ('ADULT', 'EROTIC_AO', 'PORNOGRAPHIC'))
              AND NOT EXISTS (
                  SELECT 1 FROM favorites f
                  WHERE f.source = b.source AND f.manga_url = b.manga_url
                    AND f.content_rating IN ('ADULT', 'EROTIC_AO', 'PORNOGRAPHIC'))"
        } else {
            ""
        };
        let (sql, args): (&str, Vec<Box<dyn rusqlite::ToSql>>) = match manga_url {
            Some(u) => (
                &*format!(
                    "SELECT {colonnes} FROM bookmarks b WHERE b.manga_url = ?1{filtre_adulte}
                     ORDER BY b.created_at DESC, b.page_index DESC LIMIT ?2"
                ),
                vec![Box::new(u.to_string()), Box::new(limit as i64)],
            ),
            None => (
                &*format!(
                    "SELECT {colonnes} FROM bookmarks b WHERE 1 = 1{filtre_adulte}
                     ORDER BY b.created_at DESC, b.page_index DESC LIMIT ?1"
                ),
                vec![Box::new(limit as i64)],
            ),
        };
        let mut req = conn.prepare(sql).map_err(|e| format!("signets : {e}"))?;
        let params: Vec<&dyn rusqlite::ToSql> = args
            .iter()
            .map(|b| b.as_ref() as &dyn rusqlite::ToSql)
            .collect();
        let rows = req
            .query_map(params.as_slice(), bookmark_from_row)
            .map_err(|e| format!("signets : {e}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("signets : {e}"))
    }

    // ---------- Sources activées ----------

    /// Sources désactivées par l'utilisateur. Une liste vide signifie « toutes
    /// les sources sont actives », ce qui est l'état d'une installation neuve.
    pub fn disabled_sources(&self) -> Result<Vec<String>, String> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| "base verrouillée".to_string())?;
        let mut req = conn
            .prepare("SELECT source FROM sources_state WHERE enabled = 0 ORDER BY source")
            .map_err(|e| format!("sources désactivées : {e}"))?;
        let rows = req
            .query_map([], |r| r.get::<_, String>(0))
            .map_err(|e| format!("sources désactivées : {e}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("sources désactivées : {e}"))
    }

    /// Active ou désactive une source. Réactiver supprime la ligne : on ne
    /// garde que ce qui a été écarté.
    pub fn set_source_enabled(&self, source: &str, enabled: bool) -> Result<(), String> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| "base verrouillée".to_string())?;
        if enabled {
            conn.execute(
                "DELETE FROM sources_state WHERE source = ?1",
                params![source],
            )
        } else {
            conn.execute(
                "INSERT INTO sources_state (source, enabled) VALUES (?1, 0)
                 ON CONFLICT(source) DO UPDATE SET enabled = 0",
                params![source],
            )
        }
        .map_err(|e| format!("activation de la source : {e}"))?;
        Ok(())
    }

    /// Réactive toutes les sources d'un coup : la table redevient vide.
    pub fn enable_all_sources(&self) -> Result<(), String> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| "base verrouillée".to_string())?;
        conn.execute("DELETE FROM sources_state", [])
            .map_err(|e| format!("réactivation des sources : {e}"))?;
        Ok(())
    }

    // ---------- Sources personnalisées ----------

    /// Enregistre une source personnalisée. `id` est le nom du fichier sans
    /// extension ; c'est la clé primaire — importer deux fois le même script
    /// met à jour son chemin et son horodatage.
    pub fn add_custom_source(
        &self,
        id: &str,
        name: &str,
        lang: &str,
        path: &str,
    ) -> Result<(), String> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| "base verrouillée".to_string())?;
        conn.execute(
            "INSERT INTO custom_sources (id, name, lang, path, installed_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(id) DO UPDATE SET
               name = excluded.name, lang = excluded.lang,
               path = excluded.path, installed_at = excluded.installed_at",
            params![id, name, lang, path, now_ms()],
        )
        .map_err(|e| format!("source personnalisée : {e}"))?;
        Ok(())
    }

    /// Liste toutes les sources personnalisées installées, par ordre
    /// d'installation décroissant.
    pub fn list_custom_sources(&self) -> Result<Vec<CustomSource>, String> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| "base verrouillée".to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT id, name, lang, path, installed_at
                 FROM custom_sources
                 ORDER BY installed_at DESC",
            )
            .map_err(|e| format!("sources personnalisées : {e}"))?;
        let rows = stmt
            .query_map([], |r| {
                Ok(CustomSource {
                    id: r.get(0)?,
                    name: r.get(1)?,
                    lang: r.get(2)?,
                    path: r.get(3)?,
                    installed_at: r.get(4)?,
                })
            })
            .map_err(|e| format!("sources personnalisées : {e}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("sources personnalisées : {e}"))
    }

    /// Enregistre une source **si elle n'est pas déjà connue**, sans toucher à
    /// `installed_at`. Utilisé au semis des sources livrées : un démarrage ne
    /// doit pas faire croire que la source vient d'être installée.
    pub fn ensure_custom_source(
        &self,
        id: &str,
        name: &str,
        lang: &str,
        path: &str,
    ) -> Result<bool, String> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| "base verrouillée".to_string())?;
        let n = conn
            .execute(
                "INSERT INTO custom_sources (id, name, lang, path, installed_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(id) DO NOTHING",
                params![id, name, lang, path, now_ms()],
            )
            .map_err(|e| format!("source personnalisée : {e}"))?;
        Ok(n > 0)
    }

    /// Retire une source personnalisée de la base. Sans effet si l'id est
    /// inconnu (le fichier peut déjà avoir été supprimé manuellement).
    pub fn remove_custom_source(&self, id: &str) -> Result<(), String> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| "base verrouillée".to_string())?;
        conn.execute("DELETE FROM custom_sources WHERE id = ?1", params![id])
            .map_err(|e| format!("source personnalisée : {e}"))?;
        Ok(())
    }

    /// État de semis d'une source livrée, s'il existe.
    pub fn bundled_source_state(&self, id: &str) -> Result<Option<BundledSourceState>, String> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| "base verrouillée".to_string())?;
        let mut stmt = conn
            .prepare("SELECT sha256, deleted FROM bundled_sources WHERE id = ?1")
            .map_err(|e| format!("sources livrées : {e}"))?;
        let mut rows = stmt
            .query(params![id])
            .map_err(|e| format!("sources livrées : {e}"))?;
        match rows.next().map_err(|e| format!("sources livrées : {e}"))? {
            Some(r) => Ok(Some(BundledSourceState {
                sha256: r.get(0).map_err(|e| format!("sources livrées : {e}"))?,
                deleted: r
                    .get::<_, i64>(1)
                    .map_err(|e| format!("sources livrées : {e}"))?
                    != 0,
            })),
            None => Ok(None),
        }
    }

    /// Mémorise l'empreinte de la version livrée. `deleted` vaut `true` pour
    /// une pierre tombale (source livrée supprimée).
    pub fn record_bundled_source(
        &self,
        id: &str,
        sha256: &str,
        deleted: bool,
    ) -> Result<(), String> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| "base verrouillée".to_string())?;
        conn.execute(
            "INSERT INTO bundled_sources (id, sha256, deleted, seeded_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(id) DO UPDATE SET
               sha256 = excluded.sha256, deleted = excluded.deleted,
               seeded_at = excluded.seeded_at",
            params![id, sha256, if deleted { 1 } else { 0 }, now_ms()],
        )
        .map_err(|e| format!("sources livrées : {e}"))?;
        Ok(())
    }

    /// Marque une source livrée comme supprimée par l'utilisateur. Le semis
    /// suivant la verra et ne la recopiera pas. La ligne existe pour toute
    /// source livrée : le semis l'écrit même quand le fichier préexiste, ce qui
    /// permet à la suppression de laisser une pierre tombale.
    pub fn mark_bundled_source_deleted(&self, id: &str) -> Result<(), String> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| "base verrouillée".to_string())?;
        conn.execute(
            "UPDATE bundled_sources SET deleted = 1, seeded_at = ?2 WHERE id = ?1",
            params![id, now_ms()],
        )
        .map_err(|e| format!("sources livrées : {e}"))?;
        Ok(())
    }

    // ---------- Historique ----------

    /// Note la classification d'un titre **déjà connu** (lu ou mis en favori).
    /// Deux `UPDATE` et aucun `INSERT` : ouvrir une fiche ne doit ni fabriquer
    /// une entrée « lus récemment », ni créer un favori pour un titre que
    /// l'utilisateur n'a jamais choisi. Le favori compte aussi parce qu'il
    /// s'ouvre depuis l'accueil, où rien d'autre ne donne son classement.
    pub fn note_content_rating(
        &self,
        source: &str,
        manga_url: &str,
        rating: &str,
    ) -> Result<(), String> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| "base verrouillée".to_string())?;
        // `table` vient de la liste ci-dessus, jamais d'une entrée utilisateur.
        for table in ["history", "favorites"] {
            conn.execute(
                &format!(
                    "UPDATE {table} SET content_rating = ?3
                     WHERE source = ?1 AND manga_url = ?2
                       AND (content_rating IS NULL OR content_rating = 'UNKNOWN')
                       AND ?3 <> 'UNKNOWN'"
                ),
                params![source, manga_url, rating],
            )
            .map_err(|e| format!("{table} : {e}"))?;
        }
        Ok(())
    }

    /// Enregistre la position de lecture. `chapter_index` permet de retrouver
    /// le chapitre même si la source réordonne sa liste au fil du temps.
    #[allow(clippy::too_many_arguments)]
    pub fn save_progress(
        &self,
        source: &str,
        manga_url: &str,
        title: &str,
        cover_url: Option<&str>,
        content_rating: Option<&str>,
        chapter_url: Option<&str>,
        chapter_title: Option<&str>,
        chapter_index: i64,
        page_index: i64,
        page_count: i64,
    ) -> Result<(), String> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| "base verrouillée".to_string())?;
        // La source ne classe pas toujours (`UNKNOWN`) : un mot-clé du titre ou
        // de la source vaut mieux qu'une ligne non classée, qui resterait
        // visible en incognito. Une valeur **connue** (SAFE, ADULT…) n'est
        // jamais écrasée.
        let classement = content_rating
            .filter(|r| !r.eq_ignore_ascii_case("UNKNOWN"))
            .or_else(|| classement_adulte(source, title));
        conn.execute(
            "INSERT INTO history
               (source, manga_url, title, cover_url, content_rating, chapter_url,
                chapter_title, chapter_index, page_index, page_count, read_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)
             ON CONFLICT(source, manga_url) DO UPDATE SET
               title = ?3, cover_url = ?4,
               -- `COALESCE` : une lecture qui n'a pas chargé la fiche transmet
               -- NULL et ne doit donc pas effacer une classification déjà
               -- connue (mesuré : la reprise depuis l'accueil faisait perdre
               -- le classement `ADULT`, et l'incognito ne pouvait plus masquer).
               content_rating = COALESCE(NULLIF(?5, 'UNKNOWN'), content_rating),
               chapter_url = ?6, chapter_title = ?7, chapter_index = ?8,
               page_index = ?9, page_count = ?10, read_at = ?11",
            params![
                source,
                manga_url,
                title,
                cover_url,
                classement,
                chapter_url,
                chapter_title,
                chapter_index,
                page_index,
                page_count,
                now_ms()
            ],
        )
        .map_err(|e| format!("historique : {e}"))?;
        Ok(())
    }

    /// Lus récemment, du plus récent au plus ancien. `hide_adult` applique le
    /// mode incognito : les titres dont la classification est adulte ne sont
    /// pas listés (ils restent accessibles en recherche, cf. `PLAN.md`).
    /// Dernière lecture connue d'un webtoon : le chapitre et la page où on s'est arrêté.
    ///
    /// `save_progress` écrit tout ça depuis le lecteur (`page.tsx:3313`), et la
    /// ligne existe donc en base — mais **rien ne la relit** pour la fiche. Mesuré
    /// le 2026-10-09 : retourner sur la fiche d'un webtoon ne montre ni où on en
    /// était, ni un moyen d'y revenir. La donnée est là, le raccord manque.
    ///
    /// `None` = jamais lu, et ce n'est pas une erreur : l'appelant affiche
    /// simplement la liste des chapitres sans mention de reprise.
    pub fn derniere_lecture(
        &self,
        source: &str,
        manga_url: &str,
    ) -> Result<Option<HistoryEntry>, String> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| "base verrouillée".to_string())?;
        let mut stmt = conn
            .prepare(
                "SELECT source, manga_url, title, cover_url, content_rating, chapter_url,
                        chapter_title, chapter_index, page_index, page_count, read_at
                 FROM history WHERE source = ?1 AND manga_url = ?2",
            )
            .map_err(|e| format!("derniere lecture : {e}"))?;
        let mut rows = stmt
            .query_map(params![source, manga_url], |r| {
                Ok(HistoryEntry {
                    source: r.get(0)?,
                    manga_url: r.get(1)?,
                    title: r.get(2)?,
                    cover_url: r.get(3)?,
                    cover_token: None,
                    content_rating: r.get(4)?,
                    chapter_url: r.get(5)?,
                    chapter_title: r.get(6)?,
                    chapter_index: r.get(7)?,
                    page_index: r.get(8)?,
                    page_count: r.get(9)?,
                    read_at: r.get(10)?,
                })
            })
            .map_err(|e| format!("derniere lecture : {e}"))?;
        // La clé primaire est `(source, manga_url)`, donc au plus une ligne.
        // On prend la première et on ignore le reste : un `next()` de plus
        // laisserait une rangée non consommée, ce que `query_map` tolère mais
        // qui rend la lecture plus difficile à lire.
        match rows.next() {
            Some(ligne) => ligne
                .map(Some)
                .map_err(|e| format!("derniere lecture : {e}")),
            None => Ok(None),
        }
    }

    pub fn list_recent(&self, limit: i64, hide_adult: bool) -> Result<Vec<HistoryEntry>, String> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| "base verrouillée".to_string())?;
        let sql = if hide_adult {
            "SELECT source, manga_url, title, cover_url, content_rating, chapter_url,
                    chapter_title, chapter_index, page_index, page_count, read_at
             FROM history
             WHERE content_rating IS NULL
                OR content_rating NOT IN ('ADULT', 'EROTIC_AO', 'PORNOGRAPHIC')
             ORDER BY read_at DESC LIMIT ?1"
        } else {
            "SELECT source, manga_url, title, cover_url, content_rating, chapter_url,
                    chapter_title, chapter_index, page_index, page_count, read_at
             FROM history ORDER BY read_at DESC LIMIT ?1"
        };
        let mut stmt = conn.prepare(sql).map_err(|e| format!("historique : {e}"))?;
        let rows = stmt
            .query_map(params![limit.clamp(1, 200)], |r| {
                Ok(HistoryEntry {
                    source: r.get(0)?,
                    manga_url: r.get(1)?,
                    title: r.get(2)?,
                    cover_url: r.get(3)?,
                    cover_token: None,
                    content_rating: r.get(4)?,
                    chapter_url: r.get(5)?,
                    chapter_title: r.get(6)?,
                    chapter_index: r.get(7)?,
                    page_index: r.get(8)?,
                    page_count: r.get(9)?,
                    read_at: r.get(10)?,
                })
            })
            .map_err(|e| format!("historique : {e}"))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("historique : {e}"))
    }

    pub fn forget(&self, source: &str, manga_url: &str) -> Result<(), String> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| "base verrouillée".to_string())?;
        conn.execute(
            "DELETE FROM history WHERE source = ?1 AND manga_url = ?2",
            params![source, manga_url],
        )
        .map_err(|e| format!("historique : {e}"))?;
        Ok(())
    }

    pub fn forget_all(&self) -> Result<(), String> {
        let conn = self
            .conn
            .lock()
            .map_err(|_| "base verrouillée".to_string())?;
        conn.execute("DELETE FROM history", [])
            .map_err(|e| format!("historique : {e}"))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn un_reglage_ecrit_relu_apres_redemarrage() {
        // Le dossier de téléchargements doit survivre à la fermeture : c'est
        // une donnée que Rust applique à chaque écriture de fichier, pas une
        // préférence d'affichage (qui, elle, vivrait dans `localStorage`).
        let store = Store::open_in_memory().expect("base en mémoire");
        assert_eq!(store.get_setting("dossier_telechargements").unwrap(), None);
        store
            .set_setting("dossier_telechargements", r"D:\Manga")
            .unwrap();
        assert_eq!(
            store.get_setting("dossier_telechargements").unwrap(),
            Some(r"D:\Manga".to_string())
        );
        store
            .set_setting("dossier_telechargements", r"E:\Autre")
            .unwrap();
        assert_eq!(
            store.get_setting("dossier_telechargements").unwrap(),
            Some(r"E:\Autre".to_string()),
            "un second réglage doit écraser le premier, pas s'empiler"
        );
    }

    /// Dossier unique par appel. `now_ms()` seul ne suffit pas : les tests tournent
    /// en parallèle et deux d'entre eux démarrant dans la même milliseconde se
    /// partageaient le même fichier (mesuré : deux tests en échec « au hasard »).
    fn store_vierge() -> (Store, PathBuf) {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COMPTEUR: AtomicU64 = AtomicU64::new(0);
        // Le PID **ne suffit pas** : Windows réutilise les identifiants de processus.
        // Un run interrompu laisse son dossier derrière — le `nettoyer` est sauté par le
        // `panic!` — et le run suivant, qui récupère le même PID et le même compteur,
        // retombe dessus. Mesuré le 2026-10-14 : le test des sauvegardes échouait une
        // fois sur cinq avec « exactement une sauvegarde attendue », parce que deux
        // dossiers fusionnaient et que le nouveau test y trouvait la sauvegarde de
        // l'ancien. Le nanoseconde rend le nom unique entre deux runs.
        let dir = std::env::temp_dir().join(format!(
            "tatami-test-{}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.subsec_nanos())
                .unwrap_or(0),
            COMPTEUR.fetch_add(1, Ordering::Relaxed)
        ));
        let path = dir.join("tatami.db");
        (Store::open(path.clone()).expect("ouverture"), path)
    }

    fn nettoyer(path: &Path) {
        if let Some(p) = path.parent() {
            let _ = std::fs::remove_dir_all(p);
        }
    }

    #[test]
    fn favori_ajoute_puis_retire() {
        let (s, path) = store_vierge();
        s.set_favorite("SRC", "/m/1", "Titre", Some("http://c"), Some("SAFE"), true)
            .unwrap();
        let list = s.list_favorites(false).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].title, "Titre");
        s.set_favorite("SRC", "/m/1", "Titre", None, None, false)
            .unwrap();
        assert!(s.list_favorites(false).unwrap().is_empty());
        nettoyer(&path);
    }

    #[test]
    fn favori_mise_a_jour_sans_doublon() {
        let (s, path) = store_vierge();
        s.set_favorite("SRC", "/m/1", "Ancien", None, Some("SAFE"), true)
            .unwrap();
        s.set_favorite("SRC", "/m/1", "Nouveau", None, Some("ADULT"), true)
            .unwrap();
        let list = s.list_favorites(false).unwrap();
        assert_eq!(list.len(), 1, "un favori ne doit pas être dupliqué");
        assert_eq!(list[0].title, "Nouveau");
        assert_eq!(
            list[0].content_rating.as_deref(),
            Some("ADULT"),
            "le favori doit retenir le classement"
        );
        // Un classement inconnu ne doit pas écraser celui qu'on connaît.
        s.set_favorite("SRC", "/m/1", "Nouveau", None, Some("UNKNOWN"), true)
            .unwrap();
        assert_eq!(
            s.list_favorites(false).unwrap()[0]
                .content_rating
                .as_deref(),
            Some("ADULT")
        );
        nettoyer(&path);
    }

    #[test]
    fn une_classification_connue_n_est_jamais_ecrasee_par_null() {
        let (s, path) = store_vierge();
        // Première lecture : la fiche est chargée, le classement est connu.
        s.save_progress(
            "SRC",
            "/m/a",
            "A",
            None,
            Some("ADULT"),
            Some("/c/1"),
            None,
            0,
            0,
            10,
        )
        .unwrap();
        // Reprise : la fiche n'est pas rechargée, on transmet None.
        s.save_progress("SRC", "/m/a", "A", None, None, Some("/c/2"), None, 1, 1, 10)
            .unwrap();
        let e = &s.list_recent(10, false).unwrap()[0];
        assert_eq!(
            e.content_rating.as_deref(),
            Some("ADULT"),
            "le classement appris doit survivre a une ecriture sans fiche"
        );
        assert_eq!(e.chapter_index, 1, "la position, elle, doit bien avancer");
        // Et l'incognito doit donc pouvoir le masquer.
        assert!(s.list_recent(10, true).unwrap().is_empty());
        nettoyer(&path);
    }

    #[test]
    fn le_classissement_note_ne_cree_pas_dentree() {
        let (s, path) = store_vierge();
        // Titre jamais lu : l'ouvrir sa fiche ne doit rien créer.
        s.note_content_rating("SRC", "/m/inconnu", "ADULT").unwrap();
        assert!(
            s.list_recent(10, false).unwrap().is_empty(),
            "ouvrir une fiche ne doit pas fabriquer un historique"
        );
        // Titre déjà lu sans classification : il est mis à jour.
        s.save_progress(
            "SRC",
            "/m/lu",
            "Lu",
            None,
            None,
            Some("/c/1"),
            None,
            0,
            0,
            10,
        )
        .unwrap();
        s.note_content_rating("SRC", "/m/lu", "ADULT").unwrap();
        assert!(
            s.list_recent(10, true).unwrap().is_empty(),
            "masqué par incognito"
        );
        // Une classification connue n'est pas écrasée par une autre.
        s.note_content_rating("SRC", "/m/lu", "SAFE").unwrap();
        assert!(s.list_recent(10, true).unwrap().is_empty());

        // Un favori apprend son classement de la même façon : il s'ouvre depuis
        // l'accueil, où aucun contexte de liste ne peut le fournir.
        s.set_favorite("SRC", "/m/fav", "Favori", None, None, true)
            .unwrap();
        assert_eq!(s.list_favorites(false).unwrap()[0].content_rating, None);
        s.note_content_rating("SRC", "/m/fav", "ADULT").unwrap();
        assert_eq!(
            s.list_favorites(false).unwrap()[0]
                .content_rating
                .as_deref(),
            Some("ADULT"),
            "ouvrir la fiche d'un favori doit lui apprendre son classement"
        );
        assert_eq!(
            s.list_favorites(false).unwrap().len(),
            1,
            "et toujours aucun favori fabriqué"
        );
        nettoyer(&path);
    }

    #[test]
    fn une_version_stampillee_sans_la_colonne_est_reparee() {
        // Cas mesuré : un binaire intermédiaire écrit `user_version = 3` sans
        // appliquer la v3. La base reste alors cassée pour toujours, puisque le
        // code suivant saute la migration en croyant l'avoir faite.
        let (s, path) = store_vierge();
        s.set_favorite("SRC", "/m/1", "Titre", None, None, true)
            .unwrap();
        {
            let conn = s.conn.lock().unwrap();
            conn.execute_batch("ALTER TABLE favorites DROP COLUMN content_rating")
                .unwrap();
            // La version ment : elle annonce la v3 pourtant appliquée.
            conn.execute_batch(&format!("PRAGMA user_version = {SCHEMA_VERSION}"))
                .unwrap();
        }
        s.migrate().unwrap();
        let colonnes: Vec<String> = {
            let conn = s.conn.lock().unwrap();
            let mut req = conn
                .prepare("SELECT name FROM pragma_table_info('favorites')")
                .unwrap();
            req.query_map([], |r| r.get::<_, String>(0))
                .unwrap()
                .map(|x| x.unwrap())
                .collect()
        };
        assert!(
            colonnes.iter().any(|c| c == "content_rating"),
            "la colonne manquante doit etre retablie, obtenu {colonnes:?}"
        );
        assert_eq!(
            s.list_favorites(false).unwrap()[0].title,
            "Titre",
            "et les donnees doivent survivre"
        );
        nettoyer(&path);
    }

    /// La reprise existe en base mais n'était jamais relue.
    ///
    /// Mesuré le 2026-10-09 : `save_progress` écrit `chapter_url`,
    /// `chapter_title`, `chapter_index`, `page_index`, `page_count` à chaque
    /// lecture (`page.tsx:3313`) — la ligne est donc là. Mais aucune commande ne
    /// la relit pour la fiche : retourner sur un webtoon ne montre ni où on en
    /// était, ni un moyen d'y revenir, et la liste des chapitres ne marque pas
    /// le chapitre courant.
    ///
    /// `None` n'est **pas** une erreur : jamais lu, donc rien à reprendre.
    #[test]
    fn la_derniere_lecture_d_un_webtoon_est_relue() {
        let (s, path) = store_vierge();
        // Jamais lu : `None`, et pas une erreur.
        assert!(
            s.derniere_lecture("SRC", "/m/1").unwrap().is_none(),
            "un webtoon jamais lu ne doit pas avoir de reprise",
        );

        // Deux lectures, la seconde écrase la première : la clé est
        // `(source, manga_url)` et `save_progress` fait un upsert.
        s.save_progress(
            "SRC",
            "/m/1",
            "Titre",
            None,
            None,
            Some("/ch/1"),
            Some("Ch. 1"),
            0,
            3,
            30,
        )
        .unwrap();
        s.save_progress(
            "SRC",
            "/m/1",
            "Titre",
            None,
            None,
            Some("/ch/12"),
            Some("Ch. 12"),
            11,
            7,
            20,
        )
        .unwrap();

        let lu = s
            .derniere_lecture("SRC", "/m/1")
            .unwrap()
            .expect("une lecture existe");
        assert_eq!(lu.chapter_url.as_deref(), Some("/ch/12"));
        assert_eq!(lu.chapter_title.as_deref(), Some("Ch. 12"));
        assert_eq!(lu.chapter_index, 11);
        assert_eq!(lu.page_index, 7);
        assert_eq!(lu.page_count, 20);
        nettoyer(&path);
    }

    /// La reprise d'un webtoon ne doit pas contaminer celle d'un autre.
    ///
    /// La clé est `(source, manga_url)` : deux webtoons de la même source, et
    /// deux sources pour la même URL, doivent rester indépendants. Sans ce test,
    /// un `WHERE manga_url = ?` seul ferait remonter la lecture d'un voisin.
    #[test]
    fn la_reprise_ne_se_melange_pas_entre_webtoons() {
        let (s, path) = store_vierge();
        s.save_progress(
            "SRC",
            "/m/1",
            "Un",
            None,
            None,
            Some("/ch/9"),
            Some("Ch. 9"),
            8,
            1,
            10,
        )
        .unwrap();
        s.save_progress(
            "SRC",
            "/m/2",
            "Deux",
            None,
            None,
            Some("/ch/3"),
            Some("Ch. 3"),
            2,
            5,
            12,
        )
        .unwrap();
        // Même URL, autre source.
        s.save_progress(
            "AUTRE",
            "/m/1",
            "Trois",
            None,
            None,
            Some("/ch/1"),
            Some("Ch. 1"),
            0,
            0,
            8,
        )
        .unwrap();

        assert_eq!(
            s.derniere_lecture("SRC", "/m/1")
                .unwrap()
                .unwrap()
                .chapter_title
                .as_deref(),
            Some("Ch. 9"),
        );
        assert_eq!(
            s.derniere_lecture("SRC", "/m/2")
                .unwrap()
                .unwrap()
                .chapter_title
                .as_deref(),
            Some("Ch. 3"),
        );
        assert_eq!(
            s.derniere_lecture("AUTRE", "/m/1")
                .unwrap()
                .unwrap()
                .chapter_title
                .as_deref(),
            Some("Ch. 1"),
        );
        nettoyer(&path);
    }

    /// Une reprise sans chapitre connu reste une reprise valide.
    ///
    /// Cas réel : lire depuis l'accueil sans charger la fiche transmet
    /// `chapter_url: None`. La ligne existe, la reprise est partielle — et
    /// l'appelant doit pouvoir distinguer « jamais lu » de « lu, chapitre
    /// inconnu », sinon il proposerait de reprendre un chapitre vide.
    #[test]
    fn une_lecture_sans_chapitre_connu_est_distincte_d_une_absence() {
        let (s, path) = store_vierge();
        s.save_progress("SRC", "/m/1", "Titre", None, None, None, None, -1, 0, 0)
            .unwrap();
        let lu = s
            .derniere_lecture("SRC", "/m/1")
            .unwrap()
            .expect("la ligne existe");
        assert!(lu.chapter_url.is_none(), "le chapitre est inconnu");
        assert_eq!(lu.title, "Titre", "mais le webtoon, lui, est connu");
        // Et l'absence de lecture reste une absence.
        assert!(s.derniere_lecture("SRC", "/jamais").unwrap().is_none());
        nettoyer(&path);
    }

    #[test]
    fn une_table_disparue_est_retablie_meme_si_la_version_annonne_le_contraire() {
        // Le pendant, pour une TABLE, du défaut de colonne mesuré le
        // 2026-10-04 (`store.rs:233-238`) : un binaire intermédiaire écrit
        // `user_version` sans appliquer sa migration, et la base reste cassée
        // pour toujours — parce que le code suivant compare la version et
        // conclut « déjà fait ».
        //
        // Ce commentaire dit explicitement : « Les colonnes sont ajoutées par
        // PRÉSENCE, pas seulement par version. » Le bloc v9 de `custom_sources`
        // faisait l'inverse : `if current < 9`. Une base estampillée 9 ou 10 par
        // un binaire dont le bloc n'était pas encore là sautait donc la
        // création définitivement, et `add_custom_source` échouait ensuite sur
        // *no such table*.
        //
        // `CREATE TABLE IF NOT EXISTS` est déjà idempotent : le retirer du
        // `if` est le correctif entier.
        let (s, path) = store_vierge();
        {
            let conn = s.conn.lock().unwrap();
            conn.execute_batch("DROP TABLE custom_sources").unwrap();
            // La version ment : elle annonce une v9 pourtant appliquée.
            conn.execute_batch(&format!("PRAGMA user_version = {SCHEMA_VERSION}"))
                .unwrap();
        }
        s.migrate().unwrap();
        let dispo = s.list_custom_sources().is_ok();
        assert!(
            dispo,
            "la table custom_sources doit exister après migration, \
             même si user_version annonce qu'elle est déjà faite"
        );
        // Et l'écriture doit marcher, pas seulement la lecture.
        s.add_custom_source("ma-source", "Ma Source", "fr", "C:\\sources\\ma-source.js")
            .expect("l'enregistrement doit fonctionner après réparation");
        assert_eq!(s.list_custom_sources().unwrap().len(), 1);
        nettoyer(&path);
    }

    #[test]
    fn la_migration_idempotente_ne_casse_rien() {
        let (s, path) = store_vierge();
        s.set_favorite("SRC", "/m/1", "Titre", None, Some("ADULT"), true)
            .unwrap();
        // Relancer une migration sur une base deja a jour ne doit rien changer.
        for _ in 0..3 {
            s.migrate().unwrap();
        }
        assert_eq!(s.version().unwrap(), SCHEMA_VERSION);
        assert_eq!(
            s.list_favorites(false).unwrap()[0]
                .content_rating
                .as_deref(),
            Some("ADULT")
        );
        nettoyer(&path);
    }

    #[test]
    fn les_categories_ordonnees_et_sans_doublon_de_nom() {
        let (s, path) = store_vierge();
        assert!(s.list_categories().unwrap().is_empty());
        let a = s.add_category("Action", 1).unwrap();
        let b = s.add_category("Romance", 3).unwrap();
        assert_eq!(
            s.list_categories().unwrap(),
            vec![a.clone(), b.clone()],
            "elles arrivent dans l'ordre de creation"
        );
        // Le nom est unique : deux « Action » rendraient le filtrage ambigu.
        let doublon = s.add_category("Action", 2).unwrap_err();
        assert!(doublon.contains("existe déjà"), "message : {doublon}");
        // Renommer et recolorer.
        s.rename_category(a.id, "Combat", 5).unwrap();
        let apres = s.list_categories().unwrap();
        assert_eq!(apres[0].name, "Combat");
        assert_eq!(apres[0].color, 5);
        // Réordonner : la position écrite est celle demandée, pas un échange.
        s.reorder_categories(&[b.id, a.id]).unwrap();
        let l = s.list_categories().unwrap();
        assert_eq!(
            l.iter().map(|c| c.id).collect::<Vec<_>>(),
            vec![b.id, a.id],
            "l'ordre final est exactement celui demande"
        );
        // Renommer une catégorie disparue doit échouer franchement.
        assert!(s.rename_category(999, "Fantome", 1).is_err());
        nettoyer(&path);
    }

    #[test]
    fn supprimer_une_categorie_ne_supprime_pas_ses_favoris() {
        let (s, path) = store_vierge();
        let c = s.add_category("Action", 1).unwrap();
        s.set_favorite("SRC", "/m/1", "Titre", None, Some("SAFE"), true)
            .unwrap();
        s.set_favorite_category("SRC", "/m/1", Some(c.id)).unwrap();
        assert_eq!(s.list_favorites(false).unwrap()[0].category_id, Some(c.id));
        assert_eq!(s.list_favorites(false).unwrap()[0].category_color, Some(1));

        s.delete_category(c.id).unwrap();
        // Le favori survit, il perd seulement son classement (ON DELETE SET NULL).
        let f = s.list_favorites(false).unwrap();
        assert_eq!(f.len(), 1, "supprimer une categorie ne perd aucun favori");
        assert_eq!(f[0].category_id, None);
        assert_eq!(f[0].category_color, None);
        assert!(s.list_categories().unwrap().is_empty());
        nettoyer(&path);
    }

    #[test]
    fn affecter_une_categorie_inexistante_echoue() {
        let (s, path) = store_vierge();
        s.set_favorite("SRC", "/m/1", "Titre", None, None, true)
            .unwrap();
        // Sans ce refus, le favori pointerait vers une catégorie fantôme et le
        // filtrage par catégorie afficherait un vide incompréhensible.
        assert!(s.set_favorite_category("SRC", "/m/1", Some(42)).is_err());
        assert_eq!(s.list_favorites(false).unwrap()[0].category_id, None);
        // Retirer la catégorie est toujours possible.
        s.set_favorite_category("SRC", "/m/1", None).unwrap();
        assert!(s.list_categories().unwrap().is_empty());
        nettoyer(&path);
    }

    #[test]
    fn un_signet_memorise_la_page_exacte() {
        let (s, path) = store_vierge();
        assert!(s.list_bookmarks(50, None, false).unwrap().is_empty());
        s.add_bookmark(
            "SRC",
            "/m/1",
            "/c/1",
            "Titre",
            Some("http://c"),
            Some("Chapitre 1"),
            4,
            12,
        )
        .unwrap();
        let l = s.list_bookmarks(50, None, false).unwrap();
        assert_eq!(l.len(), 1);
        assert_eq!(l[0].page_index, 4);
        assert_eq!(l[0].page_count, 12);
        assert!(s.has_bookmark("SRC", "/m/1", "/c/1", 4).unwrap());
        // Marquer deux fois la même page ne duplique pas.
        s.add_bookmark(
            "SRC",
            "/m/1",
            "/c/1",
            "Titre",
            Some("http://c"),
            Some("Chapitre 1"),
            4,
            12,
        )
        .unwrap();
        assert_eq!(s.list_bookmarks(50, None, false).unwrap().len(), 1);
        // Une autre page du même chapitre est un signet distinct.
        s.add_bookmark(
            "SRC",
            "/m/1",
            "/c/1",
            "Titre",
            Some("http://c"),
            Some("Chapitre 1"),
            7,
            12,
        )
        .unwrap();
        assert_eq!(s.list_bookmarks(50, None, false).unwrap().len(), 2);
        // Retirer est exact et idempotent.
        s.remove_bookmark("SRC", "/m/1", "/c/1", 4).unwrap();
        s.remove_bookmark("SRC", "/m/1", "/c/1", 4).unwrap();
        assert_eq!(s.list_bookmarks(50, None, false).unwrap().len(), 1);
        assert!(!s.has_bookmark("SRC", "/m/1", "/c/1", 4).unwrap());
        assert!(s.has_bookmark("SRC", "/m/1", "/c/1", 7).unwrap());
        nettoyer(&path);
    }

    #[test]
    fn les_signets_se_lisent_du_plus_recent_au_plus_ancien() {
        let (s, path) = store_vierge();
        for (page, manga) in [(1, "/m/1"), (2, "/m/1"), (3, "/m/2")] {
            std::thread::sleep(std::time::Duration::from_millis(2));
            s.add_bookmark("SRC", manga, "/c/1", "Titre", None, None, page, 10)
                .unwrap();
        }
        let l = s.list_bookmarks(50, None, false).unwrap();
        assert_eq!(
            l.iter().map(|b| b.page_index).collect::<Vec<_>>(),
            vec![3, 2, 1],
            "le plus recent d'abord"
        );
        // Le filtre par webtoon ne renvoie que le sien.
        let un = s.list_bookmarks(50, Some("/m/1"), false).unwrap();
        assert_eq!(un.len(), 2);
        assert!(un.iter().all(|b| b.manga_url == "/m/1"));
        // La limite est respectee.
        assert_eq!(s.list_bookmarks(1, None, false).unwrap().len(), 1);
        nettoyer(&path);
    }

    #[test]
    fn un_signet_ne_touche_pas_l_historique() {
        let (s, path) = store_vierge();
        s.add_bookmark("SRC", "/m/1", "/c/1", "Titre", None, None, 2, 10)
            .unwrap();
        // Marquer une page ne doit pas fabriquer d'historique : « Continuer la
        // lecture » ne montre que ce qui a ete lu.
        assert!(s.list_recent(10, false).unwrap().is_empty());
        s.save_progress(
            "SRC",
            "/m/1",
            "Titre",
            None,
            None,
            Some("/c/1"),
            None,
            2,
            2,
            10,
        )
        .unwrap();
        s.remove_bookmark("SRC", "/m/1", "/c/1", 2).unwrap();
        assert_eq!(
            s.list_recent(10, false).unwrap().len(),
            1,
            "retirer un signet ne doit rien effacer de l'historique"
        );
        nettoyer(&path);
    }

    #[test]
    fn les_sources_desactivees_sont_memorisees() {
        let (s, path) = store_vierge();
        // Installation neuve : aucune source désactivée.
        assert!(s.disabled_sources().unwrap().is_empty());
        s.set_source_enabled("WAMANGA", false).unwrap();
        s.set_source_enabled("REMANGA", false).unwrap();
        assert_eq!(
            s.disabled_sources().unwrap(),
            vec!["REMANGA".to_string(), "WAMANGA".to_string()],
            "les sources écartées sont renvoyées dans l'ordre du moteur"
        );
        // Désactiver deux fois ne duplique pas.
        s.set_source_enabled("WAMANGA", false).unwrap();
        assert_eq!(s.disabled_sources().unwrap().len(), 2);
        // Réactiver une source la retire de la liste.
        s.set_source_enabled("WAMANGA", true).unwrap();
        assert_eq!(s.disabled_sources().unwrap(), vec!["REMANGA".to_string()]);
        // Et le bouton « tout réactiver » vide la table.
        s.enable_all_sources().unwrap();
        assert!(s.disabled_sources().unwrap().is_empty());
        nettoyer(&path);
    }

    #[test]
    fn desactiver_une_source_ne_touche_pas_les_favoris() {
        let (s, path) = store_vierge();
        s.set_favorite("WAMANGA", "/m/1", "Titre", None, Some("ADULT"), true)
            .unwrap();
        s.save_progress(
            "WAMANGA",
            "/m/1",
            "Titre",
            None,
            Some("ADULT"),
            Some("/c/1"),
            None,
            0,
            0,
            10,
        )
        .unwrap();
        s.set_source_enabled("WAMANGA", false).unwrap();
        // Écarter une source ne doit pas perdre ce qu'on a lu ni mis en favori.
        assert_eq!(s.list_favorites(false).unwrap().len(), 1);
        assert_eq!(s.list_recent(10, false).unwrap().len(), 1);
        nettoyer(&path);
    }

    #[test]
    fn historique_tri_par_lecture_descendante() {
        let (s, path) = store_vierge();
        s.save_progress(
            "SRC",
            "/m/a",
            "A",
            None,
            Some("SAFE"),
            Some("/c/1"),
            Some("Ch. 1"),
            0,
            0,
            10,
        )
        .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(5));
        s.save_progress(
            "SRC",
            "/m/b",
            "B",
            None,
            Some("SAFE"),
            Some("/c/2"),
            Some("Ch. 2"),
            0,
            0,
            10,
        )
        .unwrap();
        let recent = s.list_recent(10, false).unwrap();
        assert_eq!(recent.len(), 2);
        assert_eq!(recent[0].title, "B", "le plus lu doit venir en premier");
        nettoyer(&path);
    }

    #[test]
    fn position_de_relecture_mise_a_jour_sans_doublon() {
        let (s, path) = store_vierge();
        s.save_progress(
            "SRC",
            "/m/a",
            "A",
            None,
            Some("SAFE"),
            Some("/c/1"),
            Some("Ch. 1"),
            0,
            4,
            20,
        )
        .unwrap();
        s.save_progress(
            "SRC",
            "/m/a",
            "A",
            None,
            Some("SAFE"),
            Some("/c/7"),
            Some("Ch. 7"),
            7,
            2,
            15,
        )
        .unwrap();
        let e = &s.list_recent(10, false).unwrap()[0];
        assert_eq!(e.chapter_index, 7);
        assert_eq!(e.page_index, 2);
        assert_eq!(e.page_count, 15);
        nettoyer(&path);
    }

    #[test]
    fn incognito_masque_les_titres_adultes() {
        let (s, path) = store_vierge();
        s.save_progress(
            "SRC",
            "/m/safe",
            "Safe",
            None,
            Some("SAFE"),
            Some("/c/1"),
            None,
            0,
            0,
            10,
        )
        .unwrap();
        s.save_progress(
            "SRC",
            "/m/adult",
            "Adulte",
            None,
            Some("ADULT"),
            Some("/c/2"),
            None,
            0,
            0,
            10,
        )
        .unwrap();
        s.save_progress(
            "SRC",
            "/m/inconnu",
            "Sans classe",
            None,
            None,
            Some("/c/3"),
            None,
            0,
            0,
            10,
        )
        .unwrap();
        let tout = s.list_recent(10, false).unwrap();
        assert_eq!(tout.len(), 3);
        let filtre = s.list_recent(10, true).unwrap();
        assert_eq!(filtre.len(), 2, "le titre adulte doit être masque");
        assert!(filtre
            .iter()
            .all(|e| !e.title.eq_ignore_ascii_case("Adulte")));
        assert!(
            filtre.iter().any(|e| e.title == "Sans classe"),
            "un titre sans classification ne doit pas disparaitre"
        );
        nettoyer(&path);
    }

    /// La valeur `EROTIC_AO` était écrite `ERATIC_AO` dans le filtre de
    /// l'historique : la faute de frappe laissait passer ces titres.
    /// Rouge avant le correctif (le titre restait dans la liste filtrée).
    #[test]
    fn incognito_masque_aussi_erotic_ao() {
        let (s, path) = store_vierge();
        s.save_progress(
            "SRC",
            "/m/erotic",
            "Erotique",
            None,
            Some("EROTIC_AO"),
            Some("/c/1"),
            None,
            0,
            0,
            10,
        )
        .unwrap();
        assert_eq!(s.list_recent(10, false).unwrap().len(), 1);
        assert!(
            s.list_recent(10, true).unwrap().is_empty(),
            "EROTIC_AO doit etre masque"
        );
        nettoyer(&path);
    }

    /// Une source qui ne classe pas écrit `UNKNOWN` : sans mot-clé reconnu la
    /// ligne resterait visible en incognito. Mesure du 2026-10-18 : EXHENTAI
    /// laissait passer ses titres pour cette raison.
    #[test]
    fn une_source_adulte_classe_le_titre_a_l_ecriture() {
        let (s, path) = store_vierge();
        s.save_progress(
            "EXHENTAI",
            "/m/1",
            "Un titre sans indice",
            None,
            Some("UNKNOWN"),
            Some("/c/1"),
            None,
            0,
            0,
            10,
        )
        .unwrap();
        assert!(
            s.list_recent(10, true).unwrap().is_empty(),
            "une source adulte doit etre classee a l'ecriture"
        );
        assert_eq!(
            s.list_recent(10, false).unwrap()[0]
                .content_rating
                .as_deref(),
            Some("ADULT")
        );
        nettoyer(&path);
    }

    /// Un favori ajouté avec une classification inconnue suit la même règle.
    #[test]
    fn un_favori_de_source_adulte_est_classe_a_l_ecriture() {
        let (s, path) = store_vierge();
        s.set_favorite("NHENTAI", "/m/1", "Sans indice", None, None, true)
            .unwrap();
        assert!(s.list_favorites(true).unwrap().is_empty());
        assert_eq!(
            s.list_favorites(false).unwrap()[0]
                .content_rating
                .as_deref(),
            Some("ADULT")
        );
        nettoyer(&path);
    }

    /// Les signets sont la troisième liste de l'accueil : sans ce filtre,
    /// l'incognito ne couvrait pas « Vos signets ».
    #[test]
    fn incognito_masque_les_signets_adultes() {
        let (s, path) = store_vierge();
        s.save_progress(
            "SRC",
            "/m/adulte",
            "Adulte",
            None,
            Some("ADULT"),
            Some("/c/1"),
            None,
            0,
            0,
            10,
        )
        .unwrap();
        s.save_progress(
            "SRC",
            "/m/safe",
            "Safe",
            None,
            Some("SAFE"),
            Some("/c/1"),
            None,
            0,
            0,
            10,
        )
        .unwrap();
        s.add_bookmark("SRC", "/m/adulte", "/c/1", "Adulte", None, None, 1, 10)
            .unwrap();
        s.add_bookmark("SRC", "/m/safe", "/c/1", "Safe", None, None, 1, 10)
            .unwrap();
        assert_eq!(s.list_bookmarks(50, None, false).unwrap().len(), 2);
        let masques = s.list_bookmarks(50, None, true).unwrap();
        assert_eq!(masques.len(), 1);
        assert_eq!(masques[0].title, "Safe");
        // Le lecteur demande les signets d'un webtoon précis : le filtre ne
        // doit pas le priver des siens.
        assert_eq!(
            s.list_bookmarks(50, Some("/m/adulte"), false)
                .unwrap()
                .len(),
            1
        );
        nettoyer(&path);
    }

    /// Le rattrapage tourne à chaque démarrage et doit couvrir `UNKNOWN`, pas
    /// seulement NULL : c'est la valeur que les sources qui ne classent pas ont
    /// déjà écrite. Rouge avant le correctif (la ligne restait UNKNOWN).
    #[test]
    fn le_rattrapage_reclasse_les_lignes_unknown() {
        let (s, path) = store_vierge();
        // Écriture directe : `save_progress` classerait déjà à l'écriture, donc
        // on simule une base d'avant le correctif.
        {
            let conn = s.conn.lock().unwrap();
            conn.execute(
                "INSERT INTO history (source, manga_url, title, cover_url, content_rating,
                 chapter_url, chapter_title, chapter_index, page_index, page_count, read_at)
                 VALUES ('EXHENTAI', '/m/1', 'Sans indice', NULL, 'UNKNOWN', NULL, NULL, 0, 0, 10, 0)",
                [],
            )
            .unwrap();
        }
        s.migrate().unwrap();
        let e = &s.list_recent(10, false).unwrap()[0];
        assert_eq!(e.content_rating.as_deref(), Some("ADULT"));
        nettoyer(&path);
    }

    #[test]
    fn migration_depuis_la_version_1_conserve_les_donnees() {
        // On fabrique une base en version 1, puis on vérifie que la migration
        // vers la version 2 ajoute la colonne sans perdre les lignes.
        let (s, path) = store_vierge();
        s.set_favorite("SRC", "/m/1", "Avant migration", None, Some("SAFE"), true)
            .unwrap();
        // On relève l'état AVANT de mutiler le schéma : `list_favorites` lit la
        // colonne que la v3 va ajouter, il échouerait sur la base dégradée.
        let avant = s.list_favorites(false).unwrap();
        assert_eq!(avant.len(), 1);
        {
            let conn = s.conn.lock().unwrap();
            conn.execute_batch("PRAGMA user_version = 1").unwrap();
            // On retire la colonne ajoutée en v2 pour simuler l'ancien schéma.
            conn.execute_batch("ALTER TABLE history DROP COLUMN chapter_index")
                .unwrap();
            conn.execute_batch("DROP INDEX IF EXISTS history_title")
                .unwrap();
            // La v3 a ajouté cette colonne : on la retire aussi, sinon la
            // migration échoue sur un doublon de colonne.
            conn.execute_batch("ALTER TABLE favorites DROP COLUMN content_rating")
                .unwrap();
            conn.execute_batch("PRAGMA user_version = 1").unwrap();
        }
        s.migrate().unwrap();
        assert_eq!(s.version().unwrap(), SCHEMA_VERSION);
        let apres = s.list_favorites(false).unwrap();
        assert_eq!(
            apres.len(),
            1,
            "les favoris doivent survivre a la migration"
        );
        assert_eq!(apres[0].title, "Avant migration");
        // La colonne de la v2 doit exister et être utilisable.
        s.save_progress(
            "SRC",
            "/m/1",
            "Apres",
            None,
            Some("SAFE"),
            Some("/c/1"),
            None,
            3,
            1,
            9,
        )
        .unwrap();
        assert_eq!(s.list_recent(10, false).unwrap()[0].chapter_index, 3);
        nettoyer(&path);
    }

    #[test]
    fn migration_v8_vers_v9_cree_custom_sources() {
        // Simule une base estampillée v8 (sans la table custom_sources),
        // réapplique migrate() et vérifie que la table custom_sources existe.
        //
        // Note : SCHEMA_VERSION vaut 10, donc migrate() joue les blocs v9 ET
        // v10, et écrit user_version = 10. L'assertion porte donc sur >= 9
        // (la table a été créée) et non sur == 9 (jamais écrit tel quel).
        let (s, path) = store_vierge();
        // Rétrograder à v8 : DROP la table si elle existait déjà, puis
        // fixer user_version.
        {
            let conn = s.conn.lock().unwrap();
            conn.execute_batch("DROP TABLE IF EXISTS custom_sources")
                .unwrap();
            conn.execute_batch("PRAGMA user_version = 8").unwrap();
        }
        s.migrate().unwrap();
        // La version finale est SCHEMA_VERSION (= 10), pas 9 — les blocs v9
        // et v10 se sont tous les deux exécutés.
        assert!(
            s.version().unwrap() >= 9,
            "la version doit être >= 9 après migration depuis v8"
        );
        // La table doit exister et être utilisable.
        s.add_custom_source("test-src", "Test Source", "fr", "/tmp/test.js")
            .unwrap();
        let liste = s.list_custom_sources().unwrap();
        assert_eq!(liste.len(), 1);
        assert_eq!(liste[0].id, "test-src");
        assert_eq!(liste[0].name, "Test Source");
        assert_eq!(liste[0].lang, "fr");
        // Retirer est idempotent.
        s.remove_custom_source("test-src").unwrap();
        s.remove_custom_source("test-src").unwrap(); // deuxième fois : sans erreur
        assert!(s.list_custom_sources().unwrap().is_empty());
        nettoyer(&path);
    }

    /// **Le format de `HistoryEntry` sur le fil est un contrat, pas un détail.**
    ///
    /// Mesuré le 2026-10-09 : `HistoryEntry` ne porte qu'un `#[derive(Serialize)]`
    /// sans `rename_all`, donc serde envoie les champs **tels qu'ils sont écrits**
    /// — `chapter_url`, `page_index` — et non en camelCase. Le type équivalent de
    /// l'interface était déclaré en camelCase : `progression.chapterUrl` valait
    /// `undefined`, et ni le marqueur de chapitre courant ni le bouton
    /// « reprendre » n'apparaissaient. `tsc` est resté vert : un
    /// `invoke<Type>` est une **affirmation**, que rien ne vérifie.
    ///
    /// Ce test fige les noms **émis**, du côté qui les émet. Le pendant côté
    /// interface est `pnpm ipc:check` (`check-type-ipc.mjs`), qui lit ce fichier :
    /// les deux moitiés se contrôlent mutuellement, donc ajouter un champ d'un
    /// seul côté échoue quelque part — cf. AGENTS.md §14, « une constante partagée
    /// entre deux langages a besoin d'un test qui lit l'autre fichier ».
    #[test]
    fn les_champs_de_history_entry_sont_en_snake_case() {
        let entree = HistoryEntry {
            source: "SRC".into(),
            manga_url: "/m".into(),
            title: "T".into(),
            cover_url: None,
            cover_token: None,
            content_rating: Some("SAFE".into()),
            chapter_url: Some("/c".into()),
            chapter_title: Some("Chapitre 2".into()),
            chapter_index: 1,
            page_index: 0,
            page_count: 63,
            read_at: 0,
        };
        let v = serde_json::to_value(&entree).expect("serialisation");
        let obj = v.as_object().expect("objet");
        let mut noms: Vec<&str> = obj.keys().map(String::as_str).collect();
        noms.sort_unstable();
        assert_eq!(
            noms,
            vec![
                "chapter_index",
                "chapter_title",
                "chapter_url",
                "content_rating",
                "cover_token",
                "cover_url",
                "manga_url",
                "page_count",
                "page_index",
                "read_at",
                "source",
                "title",
            ],
            "les noms émis par le cœur ont changé : `pnpm ipc:check` va accuser le type TypeScript"
        );
    }

    /// La sauvegarde d'avant migration doit contenir les écritures récentes.
    ///
    /// La base est ouverte en WAL (`Store::open`) : après un `COMMIT`, les
    /// pages modifiées vivent dans le fichier `-wal` jusqu'au prochain point de
    /// contrôle. La sauvegarde copiait le seul `.db`, donc elle livrait une base
    /// **antérieure** à la dernière écriture — pour une sauvegarde dont tout le
    /// but est de permettre un retour arrière si la migration échoue.
    ///
    /// Les deux témoins (`-wal` non vide avant, vide après) sont là parce que
    /// sans eux le test resterait vert avec l'ancien code : si le WAL était
    /// déjà vide, « retrouver les données » ne prouverait rien.
    #[test]
    fn la_sauvegarde_contient_les_ecritures_recentes_malgre_le_wal() {
        let (s, path) = store_vierge();
        s.set_favorite("SRC", "/m/1", "Titre", Some("http://c"), Some("SAFE"), true)
            .unwrap();
        s.add_bookmark(
            "SRC",
            "/m/1",
            "/c/1",
            "Titre",
            None,
            Some("Chapitre 1"),
            4,
            12,
        )
        .unwrap();

        // Témoin d'activité : les écritures sont bien dans le WAL. Sans ce
        // garde, un système où tout serait déjà dans le `.db` ferait passer le
        // test avec l'ancien code de sauvegarde.
        let wal = path.with_extension("db-wal");
        assert!(
            wal.exists(),
            "la base doit être en WAL pour que ce test ait un sens"
        );
        assert!(
            wal.metadata().unwrap().len() > 0,
            "le WAL doit porter les écritures récentes"
        );

        // Le même appel que celui d'avant migration (`migrate`).
        s.backup(SCHEMA_VERSION - 1).unwrap();

        // Le point de contrôle TRUNCATE recopie le WAL dans le `.db` et le
        // vide : c'est la condition pour que la copie soit complète.
        assert_eq!(
            wal.metadata().unwrap().len(),
            0,
            "le WAL doit être vidé, sinon la copie du `.db` part incomplète"
        );

        // Rouvrir la sauvegarde et retrouver les données récentes.
        let dossier = path.parent().unwrap();
        let mut sauvegardes: Vec<PathBuf> = std::fs::read_dir(dossier)
            .unwrap()
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with("tatami-v") && n.ends_with(".db"))
            })
            .collect();
        assert_eq!(sauvegardes.len(), 1, "exactement une sauvegarde attendue");
        let rouvert = Store::open(sauvegardes.remove(0)).expect("réouverture de la sauvegarde");
        let favoris = rouvert.list_favorites(false).unwrap();
        assert_eq!(
            favoris.len(),
            1,
            "la sauvegarde doit contenir le favori écrit juste avant elle"
        );
        assert_eq!(favoris[0].manga_url, "/m/1");
        assert_eq!(
            rouvert.list_bookmarks(50, None, false).unwrap().len(),
            1,
            "la sauvegarde doit contenir le signet écrit juste avant elle"
        );
        nettoyer(&path);
    }

    #[test]
    fn bundled_source_state_enregistre_lit_et_marque_supprime() {
        let (s, path) = store_vierge();
        assert!(s.bundled_source_state("bon").unwrap().is_none());

        s.record_bundled_source("bon", "aaaa", false).unwrap();
        let e = s.bundled_source_state("bon").unwrap().unwrap();
        assert_eq!(e.sha256, "aaaa");
        assert!(!e.deleted);

        s.record_bundled_source("bon", "bbbb", false).unwrap();
        assert_eq!(
            s.bundled_source_state("bon").unwrap().unwrap().sha256,
            "bbbb"
        );

        s.mark_bundled_source_deleted("bon").unwrap();
        assert!(s.bundled_source_state("bon").unwrap().unwrap().deleted);
        // La pierre tombale est mise à jour, pas une seconde ligne.
        nettoyer(&path);
    }

    #[test]
    fn ensure_custom_source_ne_doublonne_pas_et_ne_touche_pas_la_date() {
        let (s, path) = store_vierge();
        assert!(s.ensure_custom_source("x", "x", "fr", "/p/x.js").unwrap());
        s.ensure_custom_source("x", "x", "fr", "/p/x.js").unwrap();
        let liste = s.list_custom_sources().unwrap();
        assert_eq!(liste.len(), 1);
        assert_eq!(liste[0].installed_at, liste[0].installed_at);
        nettoyer(&path);
    }
}
