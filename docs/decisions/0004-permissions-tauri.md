# 0004 — Permissions Tauri : une seule capability, aucun plugin
- Date : 2026-10-04 (consignée après coup ; le code applique déjà cette décision depuis la tâche 0.8)
- Statut : acceptée
> Chemins cités : `apps/desktop/src-tauri/capabilities/default.json`,
> `apps/desktop/src-tauri/tauri.conf.json`, `apps/desktop/src-tauri/Cargo.toml`,
> `apps/desktop/src-tauri/src/{main,supervisor,challenge,download}.rs`,
> `apps/desktop/app/page.tsx`, `apps/desktop/next.config.mjs`.
## Contexte
`AGENTS.md` §7 (Tauri) : permissions et capabilities **au strict minimum** nécessaire, toute
extension se justifie dans l'ADR ou le commit. `AGENTS.md` §13 : ne jamais contourner les
permissions ou la CSP « pour faire passer » une fonctionnalité. `AGENTS.md` §10 : pas de port
ouvert entre le moteur et l'app.

Tatami a une contrainte supplémentaire par rapport à une application Tauri classique : le cœur
Rust doit **lancer et superviser un sidecar JVM**, **lire et écrire des fichiers dans
`%APPDATA%`**, **ouvrir une fenêtre WebView2 vers un site tiers** et **lire les cookies de cette
fenêtre**. Chacune de ces capacités a, dans Tauri v2, une contrepartie « officielle » par plugin
(`shell`, `fs`, `updater`, `http`, …). La question à trancher est simple : ces plugins sont-ils
nécessaires, ou le code Rust suffit-il ?

## Décision
**Aucun plugin Tauri. Une seule capability, `core:event:default`.**

1. **Aucun plugin dans `Cargo.toml`** : les dépendances sont `tauri`, `tokio`, `serde`,
   `serde_json`, `base64`, `tracing`, `tracing-subscriber`, `rusqlite` (`Cargo.toml:9-17`). Pas de
   `tauri-plugin-*`.
2. **Une capability, une permission** (`capabilities/default.json`, fichier de 6 lignes) :
   ```json
   { "identifier": "default", "windows": ["main"], "permissions": ["core:event:default"] }
   ```
   Elle est limitée à la fenêtre `main` (`default.json:4`). Elle a été **ajoutée après un échec
   mesuré** : `permissions: []` faisait lever `core:event:allow-listen` au premier `listen()` de
   l'interface (`PLAN.md`, journal 2026-10-02, audit UX).
3. **La seule permission demandée est l'écoute d'événements**, parce que le cœur Rust émet la
   progression des téléchargements : `app.emit("download://progress", …)`
   (`download.rs:265`, `:278`, `:329`), écouté par l'interface (`page.tsx:528`).
4. **Le sidecar est lancé en pur Rust**, par `tokio::process::Command`
   (`supervisor.rs:18`, `:121-128`), donc **aucune permission `shell`** n'est requise. Le
   `externalBin` n'est d'ailleurs pas déclaré, et ne le sera pas : le sidecar est un runtime JVM,
   pas un binaire natif (`supervisor.rs:3-5`, `PLAN.md` journal 2026-10-02 tâche 0.8). Le
   packaging le placera en `resources`.
5. **Les 28 commandes Tauri** sont enregistrées dans le handler (`main.rs:91-121`) : `search`,   `details`, `chapters`, `pages`, `list_sources`, `latest`, `filter_options`,
   `download_chapter`, `missing_chapters`, `clean_partials`, `list_library`,
   `existing_chapters`, `remove_download`, `list_favorites`, `set_favorite`, `save_progress`,
   `note_content_rating`, `list_recent`, `list_disabled_sources`, `set_source_enabled`,
   `enable_all_sources`, `add_bookmark`, `remove_bookmark`, `list_bookmarks`, `forget_history`,
   `clear_history`, `open_challenge`, `solve_challenge`, `cancel_challenge`.
   **Une permission Tauri ne s'applique pas aux commandes Rust** : le bridge `invoke` est ouvert par
   construction. Le contrôle d'accès réel est donc dans le code Rust
   (`main.rs:91-121`) — d'où `AGENTS.md` §7 : « commandes Tauri fines : elles valident, appellent
   un service, retournent un type sérialisable ».
6. **La fenêtre de vérification anti-bot est créée depuis Rust**, avec
   `WebviewWindowBuilder::new(…).user_agent(ua)` sur une `WebviewUrl::External` dont le schéma est
   validé `http`/`https` (`challenge.rs:57-81`) ; ses cookies sont lus par
   `WebviewWindow::cookies_for_url`, y compris `HttpOnly` (`challenge.rs:106-121`). Ce sont des
   API Rust : aucune permission. Conséquence notable : son label est `challenge-<source>` et la
   capability ne porte que sur `main` (`default.json:4`, `challenge.rs:25-27`) — **la fenêtre de
   vérification n'a donc aucune permission**. C'est voulu : elle ne fait qu'afficher un site.
7. **Le protocole d'URI est enregistré en Rust** (`main.rs:64-90`), avec une CSP qui n'autorise que
   `http://tatami.localhost` pour les images (`tauri.conf.json:21`, voir
   [ADR 0003](0003-images-protocole-tatami.md)).
8. **CSP** (`tauri.conf.json:21`) :
   ```
   default-src 'self'; img-src 'self' https: data: http://tatami.localhost asset: http://asset.localhost;
   style-src 'unsafe-inline' 'self';
   connect-src 'self' ipc: http://ipc.localhost tauri: http://tauri.localhost
   ```
   `connect-src` ne contient **aucun `https:`** : l'interface ne peut pas appeler un site
   directement, elle ne peut qu'invoquer Rust.

## Alternatives étudiées
- **Plugin `tauri-plugin-shell` pour lancer le sidecar** : c'est la voie prévue pour les *sidecars*
  natifs via `externalBin`. Elle n'apporte rien ici (le processus est déjà lancé et supervisé en
  Rust, avec chien de garde et backoff, `supervisor.rs:101-119`) et ajouterait une permission
  `shell:allow-execute` portant sur n'importe quel exécutable. Rejeté.
- **Plugin `tauri-plugin-fs` pour `%APPDATA%\Tatami`** : les chemins sont écrits en dur côté Rust
  (`main.rs:33-47`, `download.rs:68-74`) et jamais reçus de l'interface. Une permission `fs` ne
  ferait qu'ouvrir le dossier de données de l'utilisateur à du code web. Rejeté.
- **`allowlist` / capabilities larges (`core:default`, `*`)** : ce sont des permissions inutiles
  aujourd'hui qui deviennent des permissions possibles demain. `AGENTS.md` §13 l'interdit
  explicitement. Rejeté.
- **Plugin `tauri-plugin-http`** : serait un canal réseau supplémentaire depuis la webview, donc
  exactement la voie que `AGENTS.md` §4.1 ferme (« l'interface ne parle qu'à Rust »). Rejeté.
- **Plugin `tauri-plugin-updater` / `process` / `notification`** : phases 5.2, 3.3, 3.4, pas
  commencées. **Ils devront être ajoutés avec une justification dans cette ADR ou dans le commit
  qui les introduit** — c'est le seul point où cette décision est susceptible d'évoluer, et
  elle évoluera par extension, pas par réécriture.

## Conséquences
- **Toute nouvelle capacité passe par une modification de `capabilities/default.json` ou de la
  CSP.** C'est le marqueur à relire en revue : une ligne ajoutée dans ce fichier, ou un `https:`
  qui apparaît dans `connect-src`, doit être justifié.
- **`permissions: []` n'est pas « plus sûr », c'est « cassé ».** La capability contient
  `core:event:default` parce que l'interface écoute un événement réel. La leçon à retenir : un
  démarrage d'application sans permission ne se voit pas au build, seulement au premier
  `listen()`.
- **La fenêtre de vérification n'est pas dans la capability**, donc aucun script tiers chargé par
  le site du challenge ne peut invoquer une commande Rust. C'est un bénéfice de sécurité direct de
  la limitation `windows: ["main"]`.
- **Aucune permission ne protège le disque ni le réseau** : la sécurité est dans le code Rust.
  Conséquence de revue : toute nouvelle commande qui touche un chemin ou une URL doit valider son
  entrée (`AGENTS.md` §10), comme le fait `sanitize()` pour les noms de fichiers
  (`download.rs:38-53`) et `open_challenge` pour le schéma d'URL (`challenge.rs:60-62`).
- **Le packaging n'est pas terminé** : `bundle.targets = "all"` et `category = "Utility"`
  (`tauri.conf.json:24-28`), mais le runtime du moteur n'est pas encore placé en `resources`
  (`supervisor.rs:3-5`). Une application publiée aujourd'hui démarrerait **sans moteur**. Le
  packaging et la mise à jour signée du moteur sont les tâches 5.1 et 5.6.
- **Surveillance** : `tauri.conf.json:21` autorise `https:` dans `img-src` à cause des couvertures,
  qui sont des URL distantes. Si les couvertures passent un jour par `tatami://`, `https:` doit
  disparaître de `img-src` — sans effet sur les pages, qui ne l'utilisent pas
  (`docs/decisions/0003-images-protocole-tatami.md`, § Conséquences).
- **CSP `style-src 'unsafe-inline'`** est nécessaire au style inline de Next en export statique
  (`tauri.conf.json:21`, `next.config.mjs:7`). À durcir si un jour le CSS est entièrement externalisé.
- **Aucun `asset:` n'est utilisé** : `asset:` et `http://asset.localhost` sont dans la CSP
  (`tauri.conf.json:21`) par défaut prudent de l'exemple Tauri, mais rien dans le dépôt ne les
  demande. Ils peuvent être retirés sans risque mesuré.
