# 0002 — Base de données utilisateur : SQLite, schéma versionné, migrations forward-only
- Date : 2026-10-04 (consignée après coup ; le code applique déjà cette décision depuis la phase 1)
- Statut : acceptée
> Chemins cités : `apps/desktop/src-tauri/src/store.rs`, `apps/desktop/src-tauri/src/main.rs`,
> `apps/desktop/src-tauri/src/commands.rs`, `apps/desktop/src-tauri/Cargo.toml`, `AGENTS.md`,
> `PLAN.md`.
## Contexte
`AGENTS.md` §6 impose SQLite côté Rust pour les données métier (favoris, historique, signets,
état des sources, files de téléchargement) et des migrations **forward-only** jamais éditées
après application, avec sauvegarde avant migration et tests de migration obligatoires.

Données réelles à l'ouverture de la phase 1 : favoris et position de lecture doivent survivre à
la fermeture de l'application et au redémarrage ; la classification de contenu d'un titre doit
accompagner le favori, parce qu'un favori ouvert depuis l'accueil n'a plus aucun contexte de
liste. Un défaut mesuré sur la base réelle a également dicté la règle 5 ci-dessous :
`user_version` estampillée par un binaire qui n'avait pas encore son bloc de migration
(`store.rs:180-185`).

## Décision
**SQLite via `rusqlite` 0.40.2 (`bundled`)**, un seul fichier `%APPDATA%\Tatami\tatami.db`
(`Cargo.toml:17`, `main.rs:44-48`), ouvert par `store::Store::open`, avec migrations
appliquées à **chaque** démarrage.

1. **Versionnage** : `const SCHEMA_VERSION: i64 = 5` (`store.rs:18`), persisté dans
   `PRAGMA user_version` (`store.rs:121-130`, `store.rs:238-241`).
2. **Forward-only** : une seule fonction `migrate()` (`store.rs:132-245`). Aucun `down`, aucune
   branche de rollback. Pour revenir en arrière, on restaure la sauvegarde faite avant la
   migration.
3. **Sauvegarde obligatoire** : `backup(current)` copie le fichier en
   `tatami-v<N>-<millisecondes>.db` à côté de la base avant toute migration appliquée
   (`store.rs:140-142`, `store.rs:284-295`). Le nom est horodaté pour ne jamais écraser une
   sauvegarde existante.
4. **Aucune sortie rapide** : `migrate()` ne retourne pas tôt quand `current == SCHEMA_VERSION`.
   La réparation par présence de colonnes doit tourner à chaque démarrage (`store.rs:134-139`).
5. **Colonnes ajoutées par PRÉSENCE** : `ensure_column()` interroge `PRAGMA table_info(table)`
   et n'exécute `ALTER TABLE … ADD COLUMN` que si la colonne manque (`store.rs:251-282`).
   `table` et `colonne` sont des constantes du code, jamais une entrée utilisateur
   (`store.rs:249-250`).
6. **Une transaction** : tout le bloc de migration est une seule `transaction()`, validée à la
   fin (`store.rs:149,242-243`).
7. **WAL + clés étrangères ON** : posés à l'ouverture (`store.rs:97-100`). Le WAL permet les
   lectures concurrentes pendant l'écriture d'un point de lecture.
8. **Repli en mémoire** : si le fichier est inaccessible (dossier verrouillé, disque plein,
   droits), l'application démarre sur une base en mémoire et le dit dans les logs
   (`store.rs:111-119`, `main.rs:49-58`). L'interface reste utilisable ; seuls les favoris et
   l'historique ne survivent pas à la fermeture.
9. **Tests de migration** : `migration_depuis_la_version_1_conserve_les_donnees`
   (`store.rs:1195-1197`) fabrique une base en `user_version = 1` et vérifie que les lignes
   survivent ; `une_version_stampillee_sans_la_colonne_est_reparee` (`store.rs:856`) rejoue le
   bug mesuré ; `la_migration_idempotente_ne_casse_rien` (`store.rs:893-908`) appelle
   `migrate()` trois fois de suite et vérifie que ni la version ni les données n'ont bougé.

### Schéma au 2026-10-04 (v5)

| Objet | Colonnes / clés | Source |
|---|---|---|
| `favorites` | `source`, `manga_url`, `title`, `cover_url`, `added_at`, `content_rating` ; PK `(source, manga_url)` | `store.rs:153-160`, `store.rs:198` |
| `history` | `source`, `manga_url`, `title`, `cover_url`, `content_rating`, `chapter_url`, `chapter_title`, `chapter_index`, `page_index`, `page_count`, `read_at` ; PK `(source, manga_url)` | `store.rs:162-174`, `store.rs:189-194` |
| `sources_state` | `source` PK, `enabled` ; **seules les sources désactivées sont écrites** | `store.rs:204-210` |
| `bookmarks` | `source`, `manga_url`, `chapter_url`, `page_index`, `page_count`, `title`, `cover_url`, `chapter_title`, `created_at` ; PK `(source, manga_url, chapter_url, page_index)` | `store.rs:214-227` |
| index | `history_read_at` sur `history(read_at DESC)` | `store.rs:175` |
| index | `history_title` sur `history(title)` | `store.rs:230-233` |
| index | `bookmarks_created_at` sur `bookmarks(created_at DESC)` | `store.rs:227` |

Deux choix de clé méritent d'être justifiés :

- **`history` est clé par webtoon, pas par chapitre.** L'historique sert la carte « Continuer la
  lecture » : une entrée = un webtoon, `chapter_index` + `page_index` disent où reprendre.
  `chapter_index` a été ajouté en v2 précisément parce qu'une source peut réordonner sa liste
  entre deux visites : sans lui, la reprise visait le mauvais chapitre (`store.rs:187-194`).
- **La clé de `bookmarks` contient `page_index`.** On peut donc marquer plusieurs pages d'un même
  chapitre, et retirer un signet est exact — il n'y a pas de « dernier signet du chapitre » à
  deviner (`store.rs:211-213`).
- **`sources_state` n'écrit que les désactivées.** L'absence de ligne signifie « active » : pas de
  valeur `DEFAULT` à maintenir, donc pas d'état par défaut qui peut mentir
  (`store.rs:199-203`).

## Alternatives étudiées
- **IndexedDB côté interface** : refusé par `AGENTS.md` §6 (données métier hors `localStorage`) et
  de toute façon inaccessible au superviseur Rust, qui doit être le seul propriétaire des données.
- **`localStorage` / un fichier JSON** : simple, mais pas transactionnel (une écriture
  interrompue casse le fichier), pas de requête, et illisible depuis Rust. Rejeté par
  `AGENTS.md` §6.
- **ORM (SeaORM, Diesel) avec migrations versionnées par fichiers** : donne les migrations
  « up/down » et un DSL, mais ajoute une dépendance, une étape de génération, et son modèle de
  migration « un fichier = une version » **interdit** la règle 5 ci-dessous. Rejeté.
- **`sqlx` au lieu de `rusqlite`** : le modèle async n'apporte rien ici (le store est protégé par
  un `Mutex<Connection>`, derrière des commandes Tauri async), et `rusqlite bundled` évite d'avoir
  à gérer des dialogues de compilation ou un serveur de base. Rejeté.
- **Chiffrement du fichier de base** : écarté pour la v1, la base ne contient que des
  identifiants et des positions de lecture. **Les jetons OAuth, eux, vont dans le trousseau de
  l'OS** (`AGENTS.md` §2.6, §10) — ils ne sont pas dans cette base et n'y arriveront pas.

## Conséquences
- **Aucun retour arrière en code** : la seule voie de retour est la sauvegarde d'avant migration.
  (`AGENTS.md` §2.8.)
- **Règle de non-régression à appliquer par tout ajout de colonne** : le numéro de version et
  son `ALTER TABLE` partent dans **la même édition**, et la colonne est ajoutée par présence à
  chaque démarrage. Bug mesuré : `SCHEMA_VERSION` poussé à 3 dans une édition, `ALTER TABLE`
  livré dans l'édition suivante — le binaire intermédiaire a estampillé `user_version = 3` sans
  ajouter `favorites.content_rating`, et la base réelle est restée cassée **définitivement** car
  aucun démarrage suivant ne repassait dans la branche de migration
  (`store.rs:180-185`, `AGENTS.md` §14, `PLAN.md` journal 2026-10-04 tâches 1.1/1.4/1.6
  défaut n° 5).
- **Les colonnes ne sont pas contraintes par une FK** : `PRAGMA foreign_keys = ON` est posé
  (`store.rs:99-100`) mais aucune table ne référence une autre. Les contraintes sont vérifiées en
  Rust (`store.rs:299` et suivants). À garder en tête si une table de référence apparaît.
- **Une chaîne vide ne doit jamais servir de sentinel** : un `Option<String>` devenu `"SAFE"` par
  défaut a fait renvoyer zéro résultat aux sources adultes (`AGENTS.md` §14, test
  `une_chaine_vide_ne_signifie_pas_safe`, `commands.rs:257-265`). Même famille : ne jamais choisir
  une « source de vérité » entre la liste et la fiche, prendre la première valeur **connue** et ne
  jamais écraser une valeur apprise (`COALESCE(NULLIF(?,'UNKNOWN'), col)`, test
  `une_classification_connue_n_est_jamais_ecrasee_par_null`, `store.rs:773`).
- **Les données d'une même source peuvent se contredire** : WAMANGA classe `ADULT` dans la liste
  mais `UNKNOWN` sur la fiche, MANGABUFF l'inverse (`PLAN.md` journal 2026-10-04). Le schéma
  porte donc `content_rating` sur **les deux** tables, pour qu'aucune lecture n'ait à reconstruire
  le contexte.
- **Sauvegardes à surveiller** : une migration laisse un `tatami-v<N>-*.db` par exécution
  (`store.rs:285`). Sur une machine de dev qui migra à chaque build, ces fichiers s'accumulent.
  Le dossier `%APPDATA%\Tatami` n'est pas nettoyé automatiquement → à traiter en phase 5.5
  (journaux exportables) ou en phase 2.5 (gestion de l'espace), pas avant.
- **Limite assumée** : le store est sérialisé par un `Mutex<Connection>` (`store.rs:83-86`). Une
  commande qui écrit pendant une longue lecture de page bloquerait les autres. Acceptable à
  l'échelle actuelle (écritures = point de lecture, favori, signet) ; à reconsidérer si le
  planificateur de la phase 3 écrit en boucle.
