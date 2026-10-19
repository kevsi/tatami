# 0019 — Télécharger les pages hors du verrou par source
- Date : 2026-10-18
- Statut : acceptée

## Contexte

`ParserPool.avec(source)` tient un `Mutex` par source pendant tout l'`execute()`
bloquant du parser. `fetchImage` l'entourait, donc les 4 requêtes d'images que
le cœur lance en parallèle (`CONCURRENCE = 4` à l'époque de cette décision,
`download.rs` — 6 depuis la mise à jour du 2026-10-20 ci-dessous) se
sérialisaient :
mesuré sur HARMONYSCAN, 4 `fetchImage` **de la même page** en parallèle coûtent
762 ms contre 669 ms en série.

Or le verrou ne sert pas à ça. Ce qui débrouille une image (interception TLS,
source ZENMANGA), c'est le `ParserActif` posé sur le fil courant ; le verrou
protège la construction et le cycle de vie de l'entrée du pool.

## Décision

- `ParserPool.horsVerrou(source) { … }` : réserve l'entrée (LRU), pose le parser
  sur le fil, exécute le bloc **sans** prendre le mutex, puis relâche l'entrée.
  `relacher(entry)` est extrait pour être partagé par les deux chemins.
- `fetchImage` natif : `getPageUrl` reste sous `avec` (rapide, et c'est le parser
  qui le produit) ; le téléchargement des octets passe par `horsVerrou`.
- Garde-fou `estImageProbable(octets)` (signatures JPEG, PNG, GIF, WEBP, BMP,
  TIFF, ICO, JXL, `ftyp`, SVG) : des octets qui ne sont pas une image sont
  **retéléchargés sous le verrou**, c'est-à-dire par le chemin d'avant. Un
  parser qu'on aurait privé de son contexte de fil ne peut donc pas produire un
  `.cbz` faux en silence — au pire il coûte une requête de plus.
- Le repli ne couvre **que** « octets qui ne sont pas une image ». Une exception
  (HTTP, corps vide, 404) n'est pas rejouée : les pages d'un chapitre cassé
  seraient demandées deux fois.
- Politesse inchangée : `fetch_octets` reste borné par `LIMITES_IMAGES`
  (4 images simultanées à l'époque de cette décision ; les deux bornes sont
  relevées ensemble à 6 par la mise à jour du 2026-10-20), et `MAX_WORKERS`
  reste 4.

## Alternatives étudiées

- **Augmenter `CONCURRENCE` / `LIMITES_IMAGES`** : rejeté pour cette itération.
  Le verrou tenait la page entière, donc augmenter la borne n'aurait produit
  aucun gain et aurait seulement sollicité la source davantage. (Fait depuis,
  une fois le verrou desserré, et **mesuré** — voir la mise à jour du
  2026-10-20.)
- **Supprimer le verrou** : rejeté. Il porte le cycle de vie de l'entrée et
  l'état du parser ; le retirer demande de prouver fil par fil, par source, ce
  que `horsVerrou` évite.
- **Rejouer toute erreur sous verrou** : rejeté. Un 404 de page est définitif ;
  le doubler doublerait la charge sur les chapitres cassés.

## Conséquences

Mesuré dans l'application, même chapitre (HARMONYSCAN « The Novice's
Frontline »), 19 pages, 15 822 335 octets, `.cbz` relu par le lecteur strict
(19 entrées, 19 tailles distinctes, 19 JPEG) :

| mesure | avant | après |
|---|---|---|
| latence médiane d'un `fetchImage` (epoch de la requête → arrivée, journal du cœur) | 2 810 ms | 606 ms |
| chapitre complet, à froid | 12 545 ms | 7 333 ms |
| chapitre complet, à chaud | 19 138 ms | 6 756 ms |

Le gain visible est ×1,7 à ×2,8 — pas ×4 : le reste du temps n'est plus dans le
verrou. Il est dans la requête `pages` qui précède, et dans l'écriture de
l'archive suivie de `sync_all`. Le parallélisme réseau est réel (chevauchement
de 4 requêtes côté cœur), mais la latence par page augmente quand la source est
sollicitée à 4, ce qui rogne le théorique.

Suite annoncée ici : mesurer un relèvement de la concurrence réseau par source,
avec sa propre mesure des 403/429 avant d'être retenue. Faite le 2026-10-20 —
mesurée, zéro refus, retenue : voir ci-dessous.

## Mise à jour 2026-10-20 — concurrence réseau 4→6

Les trois plafonds bougent **ensemble** : le plus bas d'entre eux garde la
main, donc en relever un seul ne change rien de visible.

| où | avant | après |
|---|---|---|
| cœur — `CONCURRENCE` (`download.rs`) | 4 | 6 |
| moteur — `LIMITES_IMAGES` (`LimiteDeConcurrence.kt`) | 4 | 6 |
| moteur — `MAX_WORKERS` (`Server.kt`) | cœurs logiques (4) | cœurs + 2 (6 sur ce CPU) |

**Protocole** — dans l'application (artefact livré servi sur 3001, CDP 9224) :
3 passes par niveau (2 sur HARMONYSCAN « The Novice's Frontline » ch. 1, 19
pages ; 1 sur SEIMANGA ch. 0, 23 pages), clic seulement après **quiescence du
moteur** (6 s de silence du journal — à 2,5 s le clic partait pendant les
revalidations de fiche, mesuré) ; `.cbz` du dossier de test supprimé avant
chaque passe (la garde d'idempotence répondrait « Téléchargé » en 1 s, leçon du
2026-10-18) ; aucune suppression hors des dossiers de test ; `.cbz` finaux
relus par `verifier-cbz.py`.

**Mesure** (chevauchement et latences lus dans le journal du cœur : l'id
`fetchImage-<epoch>` porte l'instant de départ, la ligne d'arrivée l'instant de
fin) :

| passe | niveau | pages | chevauchement | médiane img | réseau images | total | refus |
|---|---|---|---|---|---|---|---|
| a1 | 4 | 19 | 4 | 699 ms | 3,8 s | 7 535 ms | 0 |
| a2 | 4 | 19 | 4 | 604 ms | 3,3 s | 6 974 ms | 0 |
| b1 | 6 | 19 | **6** | 875 ms | 3,2 s | 6 839 ms | 0 |
| b2 | 6 | 19 | **6** | 980 ms | 2,9 s | 6 582 ms | 0 |
| a3 | 4 | 23 | 4 | 303 ms | 2,6 s | 4 134 ms | 0 |
| b3 | 6 | 23 | **6** | 439 ms | 2,0 s | 3 534 ms | 0 |

- gain sur la phase images : **−0,5 s (~14 %)** sur HARMONYSCAN, **−0,6 s
  (~23 %)** sur SEIMANGA ; sur le total du chapitre : −544 ms et −600 ms ;
- **zéro refus** aux deux niveaux (aucun 403/429, `RATE_LIMITED`,
  `CHALLENGE_REQUIRED`, réessai transitoire) — la condition fixée d'avance ;
- la latence **par image monte** (699→875, 604→980, 303→439 ms) : à 6 en vol la
  file s'allonge côté CDN, c'est le prix du débit ; le mur de temps baisse
  quand même, c'est l'objectif ;
- `.cbz` relus stricts : 19 entrées / 19 tailles distinctes / 19 JPEG
  (15 822 335 o) et 23 / 23 / 23 (8 740 444 o) — pas de page répétée ni de
  contenu non-image.

**Décision** : retenu — chevauchement 6 atteint, gain au-dessus du seuil fixé à
~5 %, aucun refus. `LIMITES_IMAGES` reste la **borne partagée toutes sources
confondues** : un hôte voit au plus 6 requêtes simultanées de notre part.

Portes au commit : moteur `gradle test --rerun-tasks` (226 tests, 0 échec) ;
cœur `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`,
`cargo test` (285 tests, 0 échec).
