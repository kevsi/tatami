# Fixtures du contrat (tâche 5.3)

Réponses **réellement enregistrées** du moteur, rejouées par `tests/contrat_moteur.rs`.

## Pourquoi des réponses enregistrées et non des réponses écrites

Une fixture écrite à la main ne prouve qu'une chose : qu'elle est conforme au
schéma. Une réponse conforme au schéma mais fausse passerait le test sans que
rien ne le révèle. Celles-ci viennent du moteur compilé
(`engine/dist/tatami-engine`), interrogé en NDJSON le **2026-10-05** :

```powershell
node .mesures/enregistrer-reponses.mjs     # passe 1 : local + erreurs
node .mesures/enregistrer-reponses-2.mjs   # passe 2 : latest -> ... -> fetchImage
```

Leurs réponses brutes restent **hors du dépôt** (`.mesures/` est ignoré par
git). Ce qui est committé ici est la version anonymisée.

## Anonymisation (AGENTS.md §2.7)

« Pas de pages de manga, de couvertures ni de captures d'œuvres en fixtures.
Seules des métadonnées JSON minimales (titres, identifiants, URLs fictives ou
anonymisées) sont autorisées. »

| Ce qui a été remplacé | Par quoi |
|---|---|
| Titres, sous-titres, résumés, noms d'auteurs | valeurs factices |
| Noms de genres (`tags[].title`) | « Genre Factice N » |
| UUID (mangas, chapitres, couvertures) | `00000000-0000-4000-8000-…` |
| URLs de sites et de CDN | `https://exemple.test/…` |
| **Octets de `fetchImage` / `fetchCover`** | **image de synthèse 1×1** |
| `listSources` : 1370 entrées | 4, de langues différentes |
| `filterOptions` : 77 tags / 13 tris | 4 tags, les 13 tris |
| `chapters` : 222 chapitres | 4, couvrant les 3 formes mesurées |

**Ce qui n'a PAS été touché** : la forme, les noms de champs, les types, les
valeurs `null`, les noms de sources (`MANGADEX`…), les codes d'erreur, les
longueurs mesurées. C'est ce que le test vérifie.

## Les six fixtures du sidecar vision (2026-10-07)

`20-vision-*` à `25-vision-*` viennent du **sidecar vision**, pas du moteur de
sources : même contrat, autre processus. Elles ont été enregistrées par
pilotage NDJSON réel de `sidecar-vision/target/release/tatami-vision.exe`, avec
un pack local assemblé depuis les poids de l'évaluation (les poids, eux, ne sont
**jamais** dans le dépôt).

| Fixture | Méthode | Ce qu'elle prouve |
|---|---|---|
| `20-vision-hello` | `hello` | même forme que le hello du moteur |
| `21-vision-loadPack` | `loadPack` | identité du pack (`packId`, `version`) |
| `22-vision-capabilities` | `capabilities` | réponse **avant** `loadPack` : aucun modèle |
| `23-vision-detect` | `detect` | deux boîtes sur six, pour la forme |
| `24-vision-ocr` | `ocr` | trois lignes d'anglais lu sur une bulle |
| `25-vision-inpaint` | `inpaint` | **`ok: false`**, `MODEL_MISSING` |

Deux points qui méritent d'être lus :

- **`25-vision-inpaint` est une erreur, et c'est voulu.** L'effacement (T2.5)
  n'est pas intégré ; le sidecar le dit au lieu de faire semblant. La porte
  `les_erreurs_enregistrees_ne_sont_pas_rejouees` exige que cette erreur soit
  **définitive** : rejouer ne changera rien tant que le modèle est absent, donc
  elle ne doit pas être rejouée trois fois.
- **`23-vision-detect` contient des coordonnées qui débordent de la page** (ici
  `x: -144`). Ce n'est pas une faute de frappe : le détecteur travaille en
  coordonnées normalisées sur 640 et ne raint pas l'image d'origine. La fixture
  le montre tel quel, parce que c'est ce que fait vraiment le modèle, et que le
  bornage est une responsabilité de l'appelant.

**Texte des lignes OCR** : le texte anglais lu sur la bulle est conservé tel
quel, c'est-à-dire **non anonymisé** — c'est du texte de dialogue de trois mots
par ligne, sans titre ni identifiant d'œuvre, et c'est la seule preuve que la
chaîne vision lit vraiment. Le reste (images, poids, corpus) reste hors du dépôt.

### Le cas de `fetchCover`

Le `contentType` annoncé est `image/jpeg`, mais les octets substitués sont un
**PNG**. C'est volontaire : c'est exactement le mensonge du site mesuré le
2026-10-04 (une réponse `200 OK` en `text/html`), et la règle de Tatami est que
**l'extension écrite vient des octets, jamais de l'en-tête**. Une fixture
concordante ne testerait rien.

## Ce que le test en retient

1. Chaque réponse est **valide** contre `contract/protocol.json`.
2. Chaque réponse est **acceptée par le code Rust réel** : `into_result`
   (`commands.rs`), `is_transient` (`supervisor.rs`), `image::detect`
   (`image.rs`), `PageCache` (`pagecache.rs`).
3. Chaque méthode de l'`enum` `method` du schéma a **au moins une** réponse
   enregistrée. C'est la porte qui aurait attrapé `latest` et `fetchCover`,
   implémentées et utilisées mais absentes du contrat jusqu'au 2026-10-04.
4. Le nombre de réponses enregistrées est **stable** (15). Le changer est
   normal — nouvelle méthode, nouveau cas mesuré — mais il faut le faire exprès
   et le noter ici.

## Une fixture retirée : `14-health-source` (2026-10-10)

`health` rendait **deux** formes : `{ok, engineVersion}` pour le moteur global,
et `{source, reachable}` pour une source précise. La seconde a été retirée avec
sa fixture, pour deux raisons mesurées :

- elle répondait `reachable: true` après une simple validation d'enum
  (`Server.kt`, `parseSource`), **sans aucune requête réseau** — un champ qui
  affirmait une disponibilité sans la mesurer ;
- la méthode n'a **aucun appelant** dans l'application : aucune des commandes
  du cœur ne l'expose, et l'écran « état des sources » lit les erreurs réelles
  capturées pendant l'usage (`noteHealth`, en `localStorage`).

Le contrat ne décrit plus qu'une forme, `additionalProperties: false`, et le
contre-exemple de `le_validateur_refuse_les_reponses_invalides` prouve que
l'ancienne forme est désormais **refusée** — sinon le changement de contrat
n'aurait rien changé.

C'est la seule fixture supprimée depuis le retrait du sidecar vision : le compte
passe de 16 à 15.

## Régénérer

Réécrire les fixtures après une évolution de `Server.kt` est normal et attendu :
un test de contrat doit **échouer** quand la forme change, pour qu'on décide.
Ne pas « corriger » la fixture pour la faire passer — mettre à jour
`contract/README.md` et, si le contrat change vraiment, le `protocol.json` avec
une ADR (AGENTS.md §5).
### Une fixture doit être une réponse réelle, pas une synthèse

`23-vision-detect` enregistre **une réponse entière telle que le sidecar l'a
rendue**, pas une fusion des six tuiles. La première version fusionnait et
reformait les coordonnées à plat ; le résultat ne correspondait plus à rien de
réel, et le type Rust refusait de le lire. Corollaire : à l'enregistrement, ne
jamais réécrire la réponse — la **copier**.

### detect rend des coordonnées négatives, et c'est normal

La fixture porte `x: -144` et `y: -46` pour une tuile de 640×640 : la bulle
déborde de la fenêtre. Ce n'est pas une erreur, c'est ce que fait un détecteur
qui raisonne sur sa fenêtre. Le `bbox` général du contrat porte
`minimum: 0`, un choix écrit **avant** que detect existe et jamais vérifié ;
une boîte de détection a donc son propre schéma (`detectionBbox`), sans borne
négative, avec `w` et `h` strictement positifs.