# 0012 — Dimensions des pages distantes

- Date : 2026-10-09
- Statut : proposée

## Contexte

`MangaPage` renvoyé par `kotatsu-parsers-redo` ne porte pas de champ dimension (`largeur`, `hauteur`). `Server.kt::pageToJson` ne sérialise donc que `index`, `url` et `previewUrl`. Côté TypeScript, `largeur` et `hauteur` sont `undefined` pour toutes les sources distantes, ce qui entraîne `hauteurs[i] = 0` pour toutes les pages, et donc `bandeVirtualisable = false`. La virtualisation de la bande webtoon — qui améliore significativement les performances sur les longs chapitres — est donc impossible pour toutes les sources distantes.

Le correctif immédiat (tâche P0) est de passer `loading="eager"` quand `bandeVirtualisable` est false, ce qui débloque le chargement des images mais ne résout pas le problème de fond : sans dimensions, la virtualisation reste désactivée.

## Décision

Proposée — à valider avec l'humain. Deux alternatives étudiées ci-dessous, pas de décision prise.

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

- En attendant une décision, `bandeVirtualisable` reste `false` pour toutes les sources distantes, et les images sont chargées en mode `eager` (correctif P0).
- La décision impacte le protocole Rust ↔ moteur (`contract/`) : toute évolution doit être documentée dans une ADR de protocole avant implémentation.
- La validation de l'option choisie nécessite de mesurer l'impact mémoire et le temps de démarrage du lecteur sur un chapitre long (~200 pages).
