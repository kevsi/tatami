# 0014 — Dimensions des pages distantes : le lecteur mesure
- Date : 2026-10-10
- Statut : acceptée — remplace 0012

## Contexte

`MangaPage` de `kotatsu-parsers-redo` ne porte que `index`, `url` et
`previewUrl`. Aucune dimension n'existe à émettre, donc la virtualisation de la
bande était considérée inactivable pour toute source distante : la mesure
du 2026-10-08 comptait **22 pages montées** au lieu d'une fenêtre glissante.

0012 retenait l'**option A** : le moteur (ou Rust) lit les dimensions dans les
octets et les injecte dans la réponse de `pages`.

Cette option est devenue **coûteuse** après la mesure du 2026-10-10 : le moteur
sérialise les requêtes d'une même source à **~260 ms** chacune (`Mutex` par
source, `ParserPool.kt`). Demander la dimension de chaque page, c'est donc
**~260 ms × N** — soit **~14 s pour un chapitre de 53 pages**, à quoi s'ajoute une
requête gaspillée par page déjà servie.

## Décision

**Option B : le lecteur mesure au chargement.** C'est déjà ce que le code fait —
`onLoad` remonte `naturalWidth`/`naturalHeight` (`noterDims`), et
`bandeVirtualisableEst` autorise la virtualisation dès qu'**une** page est
connue.

Zéro requête supplémentaire, zéro attente, et le navigateur connaît le ratio de
tout ce qu'il a décodé — donc une source plus fraîche que l'en-tête du cache.

### Ce qui n'est PAS retenu

- **Option A (mesure par le moteur)** : ~14 s par chapitre de 53 pages, mesuré.
- **Attendre que toutes les pages soient connues** avant de virtualiser : c'était
  le défaut d'origine, et il a été corrigé en acceptant qu'**une seule** page
  connue suffise.

## Conséquences

Mesuré dans l'application réelle le 2026-10-10, chapitre froid :

| t | pages montées | décodées | bande | état |
|---|---|---|---|---|
| 1,0 s | 8 | 0 | 9 003 px | flux normal |
| **2,1 s** | 9 | **1** | 47 478 px | **positionnée** |
| 18,3 s | **12** | 6 | 44 415 px | positionnée |

Donc la virtualisation **s'active** — et la mesure « 22 pages montées » du
2026-10-08 est **périmée**. Sur un chapitre d'une puluhan de pages, **12 pages
sont montées**, pas toutes.

Deux conséquences à garder en tête :

- **La bande passe de 9 003 à 47 478 px** au moment où la virtualisation
  s'active. C'est le passage du flux normal aux pages positionnées ; la position
  de défilement doit être recalée, sinon la page lue saute. Une correction
  d'ancrage existe (`decalageAncrage`) — à vérifier sur un chapitre froid.
- **La virtualisation dépend du premier décodage**, donc du délai de première
  image d'une source. Tant que ce délai n'est pas réduit (mesuré : **8 s**),
  l'ouverture d'un chapitre froid reste en flux normal pendant ce délai.