# Contrat Rust ↔ moteur v0 (draft)

Protocole : NDJSON sur stdio du sidecar (décision [ADR 0001](../docs/decisions/0001-transport-sidecar.md)).
`contract/protocol.json` valide l'enveloppe (requête/réponse), les codes d'erreur et les types communs.
Ce fichier fige la forme de chaque méthode. Version : **0.1** (même MAJEUR = compatible).

Règles : chaque requête a `id` + `timeoutMs` ; pas de breaking change sans MAJEUR + ADR ;
réponses validées contre ce schéma des deux côtés en mode debug.

## Méthodes

| Méthode | `params` | `result` (si `ok`) | Notes |
|---|---|---|---|
| `hello` | `{}` | `{protocolVersion, engineVersion, parsersVersion}` ex. `{"protocolVersion":"0.1","engineVersion":"0.1.0-SNAPSHOT","parsersVersion":"b3c0000"}` | 1er échange. Rust refuse si MAJEUR différent. |
| `listSources` | `{}` | `{sources: sourceInfo[]}` | 1370 entrées constatées en 0.4. |
| `search` | `{source, query, offset?, limit?}` | `{list: manga[]}` | Tri : défaut de la source (`availableSortOrders.first()`). |
| `details` | `{source, url}` | `{manga: manga}` | `url` = clé stable rendue par `search`. |
| `chapters` | `{source, mangaUrl}` | `{chapters: chapter[]}` | Ordre source, le plus récent d'abord si la source le fait. |
| `pages` | `{source, chapterUrl}` | `{pages: page[]}` | `page.url` → résoudre via `fetchImage`, jamais directe en webview. |
| `fetchImage` | `{source, pageUrl}` | `{data: base64, contentType}` ex. `image/png` (recomposée si la source embrouille) | v0 renvoie les octets ; si la taille pose problème, passer aux fichiers temp (décision à prendre en 0.6+, avec ADR). |
| `setCookies` | `{source, cookies: [{name, value, domain, path?}], userAgent?}` | `{ok: true}` | Alimentation après résolution 0.11. Le moteur ne persiste rien (mémoire vive). |
| `health` | `{source?}` | `{ok: true, version?}` global, ou `{source, reachable: bool, lastError?}` par source | Sondage superviseur Rust. |
| `shutdown` | `{}` | `{ok: true}` puis le moteur sort en 0 | Arrêt propre (sinon SIGKILL après délai). |

## Erreurs (`ok: false`)

`code` stable parmi : `SOURCE_UNAVAILABLE` (site HS / 5xx persistants), `CHALLENGE_REQUIRED`
(+ `url` : ouvrir la fenêtre de résolution 0.11, puis `setCookies` et rejouer),
`PARSE_ERROR` (sélecteurs cassés → signaler via l'écran d'état 5.4),
`RATE_LIMITED` (reculer et ralentir), `TIMEOUT`, `CANCELLED`, `INTERNAL`.

## Points ouverts (v0 → v1)

- Annulation par requête : v0 = aucune (fermer stdin annule tout). Si besoin réel, ajouter `cancel{targetId}` en v0.2+ avec ADR.
- Événements moteur → Rust (ex. progression) : aucun en v0 ; les logs vont sur stderr, jamais sur stdout.
- Pagination `search` : `offset`/`limit` sont des intentions — le moteur fait au mieux selon la source (MangaDex pagine, d'autres non).
- Champs `manga`/`chapter`/`page` : `additionalProperties: true` — les sources exposent plus que le noyau ; le noyau ci-dessus suffit à la v1.
