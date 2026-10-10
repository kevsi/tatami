# 0012 — Dimensions des pages distantes

- Date : 2026-10-10
- Statut : **remplacée par 0014** (option B : le lecteur mesure au chargement)

## Contexte

`MangaPage` renvoyé par `kotatsu-parsers-redo` ne porte pas de champ dimension (`largeur`, `hauteur`). `Server.kt::pageToJson` ne sérialise donc que `index`, `url` et `previewUrl`. Côté TypeScript, `largeur` et `hauteur` sont `undefined` pour toutes les sources distantes, ce qui entraîne `hauteurs[i] = 0` pour toutes les pages, et donc `bandeVirtualisable = false`. La virtualisation de la bande webtoon — qui améliore significativement les performances sur les longs chapitres — est donc impossible pour toutes les sources distantes.

Le correctif immédiat (tâche P0) est de passer `loading="eager"` quand `bandeVirtualisable` est false, ce qui débloque le chargement des images mais ne résout pas le problème de fond : sans dimensions, la virtualisation reste désactivée.

## Décision

Option A retenue : Rust lit les dimensions depuis les octets déjà en mémoire lors de l'écriture dans le cache disque (`ecrire_cache`), les stocke dans un `Mutex<HashMap<String, (u32, u32)>>` dans `PageCache`, et les injecte dans la réponse de la commande `pages` si disponibles.

La logique d'injection est extraite en une fonction pure `injecter_dims_et_token` testable indépendamment de `tauri::State`.

## Décision finale

Aucun changement côté moteur Kotlin ni dans le contrat du protocole `pages` (le moteur ne connaît pas les dimensions — c'est Rust qui les lit depuis le cache). Deux champs optionnels `largeur` et `hauteur` ajoutés dans `$defs/page` de `contract/protocol.json`.

Les dimensions sont mémorisées en mémoire uniquement (pas de persistance entre sessions) : même pattern que l'index `PageCache`. Pas de migration SQLite : les dimensions sont recalculées à chaque session au fil du défilement.

Comportement attendu : premier appel `pages` → pas de dimensions (bande en mode `eager`) → l'utilisateur défile → `serve_page` télécharge et mémorise → retour sur le chapitre ou chapitre suivant → dimensions présentes → virtualisation active.

## Alternatives étudiées

### Option A — Rust lit les dimensions depuis le cache disque

Après le premier `fetchImage` réussi, Rust lit les dimensions de l'image depuis le cache disque (décodage partiel du JPEG/PNG/WebP pour extraire la taille), les stocke dans `pagecache`, et les renvoie dans la réponse de la commande `pages` aux champs `largeur` et `hauteur`.

**Avantages** : aucun changement côté moteur Kotlin ni dans le contrat du protocole `pages`. Les dimensions sont les dimensions réelles de l'image telle que servie.

**Inconvénients** : nécessite deux passes — un premier appel `pages` sans dimensions (avant le cache), puis un second après que les images ont été pre-fetchées. Complexifie la logique côté Rust (`pagecache.rs`). Le pre-fetch initial consomme de la bande passante avant que l'utilisateur commence à lire.

### Option B — Le moteur Kotlin calcule les dimensions lors de `fetchImage`

Lors du téléchargement de l'image dans `fetchImage`, le moteur Kotlin décode les dimensions (depuis les en-têtes du flux image) et les inclut dans la réponse.

**Avantages** : une seule passe, dimensions disponibles dès le premier appel `pages`. Cohérent avec l'architecture « le moteur fait les requêtes ».

**Inconvénients** : modifie le contrat du protocole `pages` (une ADR de protocole serait nécessaire). À VÉRIFIER : ce que `MangaPage` de `kotatsu-parsers-redo` expose exactement, et si `fetchImage` peut être instrumenté sans modifier l'interface du parser.

## Conséquences

- La virtualisation de la bande webtoon devient possible pour les sources distantes dès que les images sont mises en cache (à partir du deuxième affichage du chapitre, ou après défilement).
- Pas de breaking change dans le protocole moteur ↔ Rust.
- `bandeVirtualisable` reste le seul point de décision côté TypeScript.
- La décision impacte le protocole Rust ↔ moteur (`contract/`) : toute évolution doit être documentée dans une ADR de protocole avant implémentation.
