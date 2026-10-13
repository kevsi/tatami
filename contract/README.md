# Contrat Rust ↔ moteur v0.2 (draft)

Protocole : NDJSON sur stdio du sidecar (décision [ADR 0001](../docs/decisions/0001-transport-sidecar.md)).
`contract/protocol.json` valide l'enveloppe (requête/réponse), les codes d'erreur et les types communs.
Ce fichier fige la forme de chaque méthode. Version : **0.2** (même MAJEUR = compatible).

Règles : chaque requête a `id` + `timeoutMs` ; pas de breaking change sans MAJEUR + ADR ;
réponses validées contre ce schéma des deux côtés en mode debug.

> **Le schéma et ce tableau sont vérifiés l'un contre l'autre.** Une méthode
> présente ici doit exister dans `Server.kt` _et_ être déclarée dans l'`enum`
> `method` de `protocol.json`. L'écart a duré jusqu'au 2026-10-04 :
> `latest` et `filterOptions` étaient implémentées et utilisées par
> l'interface, absentes du contrat — `latest` portant l'essentiel du parcours
> (parcourir **et** chercher par mots-clés).

## Méthodes

| Méthode         | `params`                                                                                        | `result` (si `ok`)                                                            | Notes                                                                                                                                                                                                                                                                                                                                                                                                                                                |
| --------------- | ----------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `hello`         | `{}`                                                                                            | `{protocolVersion, engineVersion, parsersVersion, defaultUserAgent}`          | 1er échange. Rust refuse si MAJEUR différent. `defaultUserAgent` est réinjecté dans la fenêtre de résolution : une clearance est émise pour un agent donné.                                                                                                                                                                                                                                                                                          |
| `listSources`   | `{}`                                                                                            | `{sources: sourceInfo[]}`                                                     | `sourceInfo` = `{name, lang, domains?}`. `domains` contient l'hôte courant et les alternatives préconfigurées, sans schéma, port ni chemin ; pour les sources scriptées, il vient de l'hôte de `baseUrl`. Absent si aucun hôte HTTP(S) valide n'est déclaré.                                                                                                                                                                                         |
| `filterOptions` | `{source}`                                                                                      | `{tags: [{title, key}], sortOrders: [string]}`                                | Genres + tris proposés par la source.                                                                                                                                                                                                                                                                                                                                                                                                                |
| `latest`        | `{source, offset?, limit?, query?, tags?[], sortOrder?, contentRating?, safeOnly?}`             | `{list: manga[]}`                                                             | **C'est le parcours principal** : parcourir le catalogue ET chercher, avec filtres, tri et pagination. `sortOrder` est un rang dans `sortOrders`. `contentRating` = une seule valeur, ou absent = pas de filtre ; `safeOnly` désactive le repli `SAFE` du moteur.                                                                                                                                                                                    |
| `search`        | `{source, query, offset?, limit?, contentRating?}`                                              | `{list: manga[]}`                                                             | Tri : défaut de la source (`availableSortOrders.first()`). Non appelée par l'interface 0.9+ (elle passe par `latest` avec `query`).                                                                                                                                                                                                                                                                                                                  |
| `details`       | `{source, url, mangaTitle?}`                                                                    | `{manga: manga}`                                                              | `url` = clé stable rendue par `latest`/`search`. `mangaTitle` permet de reconstruire une série hors cache de session (redémarrage).                                                                                                                                                                                                                                                                                                                  |
| `chapters`      | `{source, mangaUrl, mangaTitle?}`                                                               | `{chapters: chapter[]}`                                                       | Ordre source, le plus récent d'abord si la source le fait. Appelé après chaque `details` : `pages` exige que le chapitre soit connu de la session.                                                                                                                                                                                                                                                                                                   |
| `pages`         | `{source, chapterUrl}`                                                                          | `{pages: page[]}`                                                             | `page.url` est la **clé** de page, pas une image. Résoudre via `fetchImage`, jamais directe en webview.                                                                                                                                                                                                                                                                                                                                              |
| `fetchImage`    | `{source, pageUrl}`                                                                             | `{data: base64, contentType}`                                                 | Le moteur appelle `getPageUrl` sur le parser. URL d'image vide → `PARSE_ERROR` (mesuré : sans ce garde-fou, 45 pages HTML identiques entraient dans un `.cbz`). v0 renvoie les octets ; si la taille pose problème, passer aux fichiers temp (décision à prendre en 0.6+, avec ADR).                                                                                                                                                                 |
| `fetchCover`    | `{source, mangaUrl, coverUrl?, referer?}`                                                       | `{data: base64, contentType}`                                                 | Couverture d'une série. Résout la série dans le cache de session ; sinon retombe sur `coverUrl`, qui doit être `http(s)` (un `file://` est refusé). Les couvertures passent par le moteur pour la même raison que les pages (AGENTS.md §4 règle 4).                                                                                                                                                                                                  |
| `setCookies`    | `{source, cookies: [{name, value, domain, path?, secure?, httpOnly?, expiresAt?}], userAgent?, browserProfile?}` | `{ok: true}` | Alimentation après résolution 0.11. Le moteur ne persiste rien (mémoire vive). `domain` est normalisé **sans point initial** par le producteur (OkHttp ne matche rien avec le point). `expiresAt` (epoch ms), `secure` et `httpOnly` viennent de la webview. `browserProfile` est optionnel et contient `brands`, `mobile`, `platform`, `fullVersionList`, `platformVersion`, `architecture`, `bitness`, `model`, `language` et `languages`, tous lus de la WebView2 (valeurs haute entropie via `getHighEntropyValues`). Le profil reste en mémoire sous le même domaine que `cf_clearance` et n'est jamais journalisé. |
| `health`        | `{source?}`                                                                                     | `{ok: true, engineVersion}` global, ou `{source, reachable: true}` par source | Implémentée par le moteur mais **jamais appelée** par Rust ni par l'interface (vérifié le 2026-10-04). L'indicateur « hors service » du catalogue repose pour l'instant sur un cache `localStorage`, pas sur ce sonde.                                                                                                                                                                                                                               |
| `shutdown`      | `{}`                                                                                            | `{ok: true}` puis le moteur sort en 0                                         | Arrêt propre. **Fermer stdin ne tue pas le JVM** (mesuré : encore vivant 4 s après) — envoyer `shutdown` puis fermer stdin, avec `kill` en secours.                                                                                                                                                                                                                                                                                                  |

## État par défaut des listes

`contentRating` absent **et** `safeOnly` absent ou vrai → le moteur ne demande que
`SAFE`. Une chaîne vide n'est jamais un sentinelle : `""` = « aucune
classification », pas `SAFE`. Beaucoup de parseurs n'acceptent qu'une seule
classification à la fois (« Multiple Content ratings are not supported by this
source », MangaDex) : ne jamais envoyer un set.

## Erreurs (`ok: false`)

`code` stable parmi : `CHALLENGE_REQUIRED`
(+ `url` : ouvrir la fenêtre de résolution 0.11, puis `setCookies` et rejouer),
`PARSE_ERROR` (paramètre invalide), `RATE_LIMITED` (reculer et ralentir),
`TIMEOUT`, `CANCELLED`, `INTERNAL`.

Trois codes de **panne de source** ont été ajoutés le 2026-10-12
(`Erreurs.kt::classerEchecSource`), sur la mesure des 98 sources FR : 83
échouaient en `INTERNAL` avec une exception Java brute.

| Code              | Origine                                                          | Interface                    |
| ----------------- | ---------------------------------------------------------------- | ---------------------------- |
| `SOURCE_DEAD`     | `UnknownHostException`, `NotFoundException`, HTTP 404 / 410      | `erreur.source.morte`        |
| `ORIGIN_DOWN`     | HTTP 521 / 522 / 523 / 525 (Cloudflare n'atteint plus l'origine) | `erreur.origine.injoignable` |
| `PARSER_OUTDATED` | `ParseException` — le site a changé de page                      | `erreur.parseur.perime`      |

`SOURCE_DEAD` et `ORIGIN_DOWN` sont **volontairement distincts** : le domaine a
disparu dans le premier cas et ne reviendra pas, alors que dans le second le
domaine existe et le serveur peut être rallumé. Les confondre décourageait de
réessayer.

Le classement se fait sur le **type** de l'exception et son **code de statut**,
jamais sur le texte du message : `HttpStatusException` expose `getStatusCode`,
donc un parseur qui réécrit son message ne change pas le verdict. Le message
transmis reste `Exception: texte` — c'est ce détail que
`Supervisor::is_transient` reconnaît côté Rust pour rejouer un échec réseau,
donc le typage n'altère aucun comportement de retry.

`SOURCE_UNAVAILABLE` avait été retiré de l'enum le 2026-10-10 parce qu'aucune
couche ne l'émettait. `SOURCE_DEAD` le réintroduit avec un émetteur réel et une
définition mesurée.

## Points ouverts (v0 → v1)

- Annulation par requête : v0 = aucune (fermer stdin annule tout). Si besoin réel, ajouter `cancel{targetId}` en v0.2+ avec ADR.
  Attention : la fermeture de stdin appelle `workers.shutdownNow()`, qui **jette les requêtes en vol sans réponse**. Mesuré : cinq requêtes envoyées d'un bloc, stdout vide. Le superviseur Rust s'en sort par `kill()`, mais une annulation propre reste à faire.
- Événements moteur → Rust (ex. progression) : aucun en v0 ; les logs vont sur stderr, jamais sur stdout.
- Pagination `latest`/`search` : `offset`/`limit` sont des intentions — le moteur fait au mieux selon la source (MangaDex pagine, d'autres non). `latest` lit bien `offset` (mesuré) ; `search` ne le transmet pas encore.
- Champs `manga`/`chapter`/`page` : `additionalProperties: true` — les sources exposent plus que le noyau ; le noyau ci-dessus suffit à la v1.
- `AUTH_REQUIRED` : `AuthRequiredException` n'est levée que sur HTTP 401 et arrive aujourd'hui en `INTERNAL`. 16 parseurs la lèvent. Aucun code correspondant dans l'enum (tâche 0.13, ADR à écrire).
- `sourceInfo.lang` est déduit par heuristique (`inferLang`) : suffixe explicite (`_FR`) puis paquet du nom de classe. Ce n'est pas une valeur déclarée par la source.
