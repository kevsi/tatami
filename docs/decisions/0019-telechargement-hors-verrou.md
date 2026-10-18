# 0019 — Télécharger les pages hors du verrou par source
- Date : 2026-10-18
- Statut : acceptée

## Contexte

`ParserPool.avec(source)` tient un `Mutex` par source pendant tout l'`execute()`
bloquant du parser. `fetchImage` l'entourait, donc les 4 requêtes d'images que
le cœur lance en parallèle (`CONCURRENCE = 4`, `download.rs`) se sérialisaient :
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
  (4 images simultanées), et `MAX_WORKERS` reste 4.

## Alternatives étudiées

- **Augmenter `CONCURRENCE` / `LIMITES_IMAGES`** : rejeté pour cette itération.
  Le verrou tenait la page entière, donc augmenter la borne n'aurait produit
  aucun gain et aurait seulement sollicité la source davantage.
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

Suite possible, non faite : mesurer un relèvement de la concurrence réseau par
source. Elle touche la politesse envers les sites, donc elle demande sa propre
mesure des 403/429 avant d'être retenue.
