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

## Régénérer

Réécrire les fixtures après une évolution de `Server.kt` est normal et attendu :
un test de contrat doit **échouer** quand la forme change, pour qu'on décide.
Ne pas « corriger » la fixture pour la faire passer — mettre à jour
`contract/README.md` et, si le contrat change vraiment, le `protocol.json` avec
une ADR (AGENTS.md §5).