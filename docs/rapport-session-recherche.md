# Rapport de session de recherche — Tatami

Date : **2026-10-04**. Session de **mesure seule** : aucun code, aucun test, aucun
`PLAN.md` modifié, aucune application lancée, aucun port 3000/9223 touché, aucun
Playwright. Le seul instrument a été le moteur déjà construit
(`engine/dist/tatami-engine/runtime/bin/java.exe` + ses 18 JAR), piloté en NDJSON sur
stdin/stdout (protocole `0.1`) par des scripts Node dans `%LOCALAPPDATA%\Temp\opencode\tatami\`.
Les réponses stdout arrivent en morceaux de 8 Ko : les scripts bufferisent avant de
découper sur `\n` (`p01-hello-sources.mjs` → 1 625 ms pour `hello` + `listSources`).

Commandes citées ci-dessous sont reproductibles telles quelles.

---

## 1. Périmètre mesuré

| Mesure | Moyen | Résultat |
|---|---|---|
| Volontaire du moteur | `java -Xmx256m -XX:+UseSerialGC -cp lib/* org.tatami.engine.MainKt serve` | 1 370 sources listées |
| Signatures de l'API parsers | `javap -p -classpath <JAR extrait>` (JDK Temurin 21.0.12) | voir `docs/notes/loader-context.md` |
| Recensement d'usage | scan des 4 000 `.class` du JAR | voir `docs/notes/loader-context.md` §4 |
| État réel des sources | chaîne `latest → details → chapters → pages → fetchImage` | **31** sources en chaîne complète, **+8** sondées sur un point précis (39 distinctes) |
| Empreinte mémoire / démarrage / arrêt | `Get-Process` sur le PID enfant | § 7 |
| Disponibilité du nom | `api.github.com`, `crates.io`, `registry.npmjs.org` | § 6.5 |

Images : **aucune n'est écrite sur disque**. `fetchImage` est mesuré sur le nombre
d'octets, le `contentType` renvoyé et les 4 à 26 premiers octets (signature JPEG /
PNG / WebP / GIF), conformément à AGENTS.md §2.7.

---

## 2. Les deux affirmations de AGENTS.md §14 qui ne sont plus vraies

AGENTS.md §14 et PLAN.md (journal 2026-10-03) affirment : « REMANGA et SEIMANGA
renvoient leurs pages mais `fetchImage` répond **403** (le CDN refuse l'user-agent du
moteur) ». Mesuré aujourd'hui, avec le même moteur :

- **REMANGA** : les images répondent **`CHALLENGE_REQUIRED`**, pas « 403 » — donc
  l'erreur *est* typée et *déclencherait* la fenêtre de résolution de 0.11.
- **SEIMANGA** : les images répondent **OK** (444 832 octets `image/webp`, puis
  779 390 et 549 876 octets sur deux autres pages). Elle ne demande plus rien.

Je n'ai pas corrigé AGENTS.md (hors périmètre autorisé) : **la leçon §14 est périmée
et devrait être remplacée**.

## 3. Question 1 de la mission : REMANGA / SEIMANGA — quel user-agent passe ?

`p02-remanuga-ua.mjs` — chaîne complète, puis 5 user-agents sur la même image, via
`setCookies { userAgent }` (le moteur lit `userAgentOverride` à chaque requête) :

```
UA par defaut du moteur = Mozilla/5.0 (X11; Linux x86_64; rv:109.0) Gecko/20100101 Firefox/116.0

REMANGA   pages OK 53 | hote image = img.reimg.org
  fetchImage defaut (non surcharge)   KO CHALLENGE_REQUIRED : challenge at https://img.reimg.org/...
  fetchImage Chrome 131 / Windows     KO CHALLENGE_REQUIRED : challenge at https://img.reimg.org/...
  fetchImage Firefox 115 / Windows    KO CHALLENGE_REQUIRED : challenge at https://img.reimg.org/...
  fetchImage Chrome 131 / Linux X11   KO CHALLENGE_REQUIRED : challenge at https://img.reimg.org/...
  fetchImage Tatami/0.1 (robot)       KO CHALLENGE_REQUIRED : challenge at https://img.reimg.org/...
  [UA defaut] page[1]  hote=img.reimg.org -> KO CHALLENGE_REQUIRED
  [UA defaut] page[26] hote=img.reimg.org -> KO CHALLENGE_REQUIRED

SEIMANGA  pages OK 15 | hote image = sei.one-way.work
  fetchImage defaut (non surcharge)   OK 444832 o image/webp (1746ms)
  fetchImage Chrome 131 / Windows     OK 444832 o image/webp (334ms)
  fetchImage Firefox 115 / Windows    OK 444832 o image/webp (268ms)
  fetchImage Chrome 131 / Linux X11   OK 444832 o image/webp (233ms)
  fetchImage Tatami/0.1 (robot)       OK 444832 o image/webp (235ms)
  [UA defaut] page[1] -> OK 779390 o     [UA defaut] page[7] -> OK 549876 o
```

**Réponses** : *aucun* user-agent ne débloque REMANGA (5 sur 5 échec, 3 pages
différentes) ; *aucun* ne casse SEIMANGA, y compris `Tatami/0.1` qui est refused par
la plupart des sites. **L'agent utilisateur n'est pas le facteur** dans les deux cas.

Ce que l'on sait de REMANGA, en plus :

- `CHALLENGE_REQUIRED` ne peut être émis que si la réponse est **403 ou 503** *et*
  porte un marqueur HTML Cloudflare (`network/CloudFlareHelper.kt`). Le corps est donc
  une vraie page de challenge, pas un refus nu.
- **Statut exact (403 ou 503) : non mesuré** — le moteur ne le transmet pas et je
  n'ai pas utilisé d'autre client HTTP.
- Le moteur envoie `Referer = <url de l'image>` (`engine/src/main/kotlin/org/tatami/engine/Server.kt:405`),
  ce qui est fausses valeurs. C'est l'hypothèse restante, **non isolée** : le moteur
  n'expose aucun moyen de changer un en-tête autre que le UA.
- Conséquence positive : **REMANGA est aujourd'hui un candidat réel pour la fin de
  chaîne 0.11** (voir §6.7).

## 4. Question 2 de la mission : MANGABUFF — quelle erreur exacte ?

`p03-mangabuff.mjs`, puis `p05-misc.mjs` (même moteur, plusieurs chapitres) :

```
latest   -> OK 3 item(s)     1er = "Слезы на увядших цветах"
details  -> OK               contentRating=UNKNOWN state=ONGOING tags=9
chapters -> OK 121
pages(ch="Том 1 Глава 121") -> OK 10 pages ; fetchImage hote=c3.mangabuff.ru -> OK 456705 o image/jpeg
pages(ch="Том 1 Глава 61")  -> OK 109 pages
pages(ch="Том 1 Глава 1")   -> {"code":"INTERNAL","message":"AuthRequiredException: Authorization required"}
```

**Réponses** :

1. « Authentification exigée dès `pages` » n'est vrai que pour **les chapitres
   anciens** : le chapitre 1 échoue, les chapitres 61 et 121 passent. Ce n'est donc
   pas un blocage de source mais un **blocage par chapitre**.
2. Le code d'erreur est **`INTERNAL`**, message `AuthRequiredException: Authorization required`.
3. **Cela ne correspond pas à `CHALLENGE_REQUIRED`**, et ne le peut pas :
   `AuthRequiredException` est levée par `OkHttpWebClient.ensureSuccess()`
   **uniquement sur HTTP 401** (`exception/AuthRequiredException.kt` +
   `network/OkHttpWebClient.kt`), tandis que `CHALLENGE_REQUIRED` ne peut venir que de
   `CloudFlareProtectionException` (403/503 + marqueur HTML). Deux notions distinctes,
   et le contrat n'a **aucun** code pour « connexion requise ».
   → Conséquence concrète : un utilisateur qui ouvre le chapitre 1 de MANGABUFF voit
   une erreur générique, et Rust n'a rien à lui proposer. 16 parseurs lèvent cette
   exception (recensement du JAR).

## 5. Question 3 de la mission : échantillon FR/EN/ES/PT/DE

`p04-sample.mjs` — 4 sources par langue, chaîne complète jusqu'à l'octet d'image.
Date : 2026-10-04. Langues telles que `listSources` les infère.

| Langue | Source | Chaîne | Image servie |
|---|---|---|---|
| fr | MANGASORIGINES | latest 3 / 249 ch. / 44 p. | **OK** 191 408 o WEBP |
| fr | TOOMICSFR | 3 / 3 ch. / 89 p. | **OK** 38 853 o JPEG (`toon-g2.toomics.com`) |
| fr | FRSCANSCOM | `KO INTERNAL: UnknownHostException (frscans.com)` | — |
| fr | MANGAFR | liste vide | — |
| en | MANGADEX | 3 / 842 ch. / 1 p. | **OK** 472 764 o PNG |
| en | MANGANATO | `KO INTERNAL: IOException: canceled due to CloudFlareProtectionException` | — |
| en | MANGAKISS | `KO INTERNAL: HttpStatusException Status=522` | — |
| en | MANGAIRO | liste vide | — |
| es | ANZMANGASHD | 3 / 347 ch. / 19 p. | **OK** 1 429 969 o PNG |
| es | TOOMICSES | 3 / 6 ch. / 108 p. | **OK** 9 036 o JPEG |
| es | MANGADOOR | `KO INTERNAL: HttpStatusException Status=403` | — |
| es | DAPROB | `KO INTERNAL: ParseException: Root not found at https://terbangterusninja.org/` | — |
| pt | MUNDOHENTAIOFICIAL | 3 / 1 ch. / 75 p. | **OK** 219 712 o JPEG |
| pt | TOOMICSPT | 3 / 3 ch. / 47 p. | **OK** 32 571 o JPEG |
| pt | MANGANANQUIM | `KO INTERNAL: UnknownHostException (mangananquim.site)` | — |
| pt | LINKSTARTSCAN | `KO INTERNAL: ParseException: Root not found` | — |
| de | TOOMICSDE | 3 / 3 ch. / 166 p. | **OK** 39 074 o JPEG |
| de | WEBTOONS_DE | 3 / 37 ch. / 117 p. | **OK** 92 264 o JPEG |
| de | MANGAHAUS | `KO INTERNAL: SSLHandshakeException (certificate_unknown) PKIX path building failed` | — |
| de | MANGALESEN | `KO INTERNAL: SSLHandshakeException (certificate_unknown) PKIX path building failed` | — |

**Bilan de l'échantillon de langues : 9 / 20 sources servent une image réellement
décodable (45 %)** — fr 2/4, en 1/4, es 2/4, pt 2/4, de 2/4.

Sources hors échantillon de langue, même jour :

| Source | Chaîne | Image |
|---|---|---|
| REMANGA | 3 / 31 ch. / 53 p. | **KO `CHALLENGE_REQUIRED`** (`img.reimg.org`) |
| SEIMANGA | 3 / 70 ch. / 15 p. | **OK** 444 832 o WEBP (`sei.one-way.work`) |
| MANGABUFF | 3 / 121 ch. / 10 p. | **OK** 456 705 o JPEG (`c3.mangabuff.ru`), sauf chapitre 1 |
| WAMANGA | 3 / 286 ch. / 20 p. | **OK** 547 792 o WEBP |
| ZENMANGA | 3 / 240 ch. / 19 p. | **octets servis mais PAS une image** (voir ci-dessous) |
| LELMANGA | 3 / 2 ch. / 66 p. | **OK** 69 580 o WEBP |
| MADARADEX | 3 / 37 ch. / 24 p. | **KO `CHALLENGE_REQUIRED`** (`cdn.madaradex.org`) |
| COMX | `KO NotFoundException Status=404 (com-x.life)` | — |
| MINTMANGA | `KO NotFoundException Status=404 (2.mintmanga.one)` | — |
| MANGALIB | `KO HttpStatusException Status=403 (api.cdnlibs.org)` | — |
| BAOZIMH | `KO HttpStatusException Status=403 (www.baozimh.com)` | — |
| SELFMANGA | liste vide | — |
| MANGASORIGINESUNOFFICIAL | `KO SSLHandshakeException PKIX path building failed` | — |
| SUSHISCANFR | 3 / 10 ch. / **504 Gateway Time-out** | — |

**Total sur 31 sources distinctes : 12 servent une image décodable (39 %), 1 sert
des octets illisibles, 18 n'en servent aucune.**

### 5.1 Répartition des échecs (17 à la toute première étape `latest`)

| Cause | Nombre | Sources |
|---|---|---|
| Domaine disparu (`UnknownHostException`) | 2 | FRSCANSCOM, MANGANANQUIM |
| **Chaîne TLS** (`PKIX path building failed`) | 3 | MANGAHAUS, MANGALESEN, MANGASORIGINESUNOFFICIAL |
| 404 | 2 | COMX, MINTMANGA |
| 403 « nu » | 3 | MANGADOOR, MANGALIB, BAOZIMH |
| 522 | 1 | MANGAKISS |
| DOM changé (`ParseException: Root not found`) | 2 | DAPROB, LINKSTARTSCAN |
| Cloudflare **empaqueté** dans une `IOException` → `INTERNAL` | 1 | MANGANATO |
| Liste vide | 3 | MANGAFR, MANGAIRO, SELFMANGA |

Deux enseignements :

1. **Le runtime `jlink` échoue sur 3 sites en TLS** (`PKIX path building failed`).
   Les 3 autres mesures de cette session sur d'autres hôtes passent, donc ce n'est pas
   un `cacerts` absent : c'est, par hôte, une chaîne de certificats que le magasin de
   confiance du runtime réduit ne reconstruit pas. **À ne pas résoudre en désactivant
   la validation TLS** (AGENTS.md §10) — investigation à faire, non faite ici.
2. **`CHALLENGE_REQUIRED` se perd quand le parser emballe l'exception** :
   MANGANATO et KAGANE renvoient `INTERNAL: IOException: canceled due to
   CloudFlareProtectionException`. Un vrai challenge Cloudflare est donc rapporté
   comme une panne interne, et la fenêtre de résolution ne s'ouvrirait pas. Le mappings
   d'erreurs du moteur est le point à durcir (dans le moteur, pas dans l'app).

### 5.2 ZENMANGA : des octets, pas une image

```
page[0] = https://static.inuko.me/chapters/…/….webp
contentType=application/octet-stream  bytes=256702
26 premiers octets (hex) = 0730152dc6da417a072d353c20000a76ff93506b80384de75142c768d65f0c674df317495193e7
page[1] -> 387334 o, 8 premiers octets 0730152d8ed8477a072d353c
```

L'URL se termine par `.webp` mais aucun en-tête `RIFF…WEBP` n'est présent : les 12
premiers octets sont **identiques sur deux pages différentes** (`0730152d c6da417a
072d353c`), ce qui ressemble à un en-tête de chiffrement/obfuscation, pas à de l'aléa.
Le lecteur y afficherait une image cassée. Le parser responsable n'a pas pu être
identifié avec les outils autorisés → **« le désembrouillage était-il attendu ? »
reste non mesuré**. Élément connexe écrit dans `docs/notes/loader-context.md` §6 :
`redrawImageResponse` n'est **jamais** appelé aujourd'hui, parce qu'il n'est invoqué que
depuis `intercept()` du parser, et que le parser n'est pas branché comme intercepteur.

---

## 6. Tour de la section 6 de `PLAN.md`

| # | Question ouverte | Tranché par la mesure | Reste ouvert |
|---|---|---|---|
| 6.1 | Coordonnées exactes de `kotatsu-parsers-redo` | **Oui.** `engine/build.gradle.kts:17` déclare `com.github.Kotatsu-Redo:kotatsu-parsers-redo:b3c0000` ; le JAR chargé s'appelle `kotatsu-parsers-redo-b3c0000.jar` ; versions alignées sur le contenu réel de `lib/` (okhttp 5.1.0, okio 3.16.0, kotlin 2.2.10, coroutines 1.10.2, nashorn 15.6) ; 4 000 classes, 1 370 sources | Le `README.md` du fork reste périmé (cite `KotatsuApp`) : à ne pas suivre |
| 6.2 | Ce que `MangaLoaderContext` exige + kotatsu-dl | **Oui, en grande partie.** Voir `docs/notes/loader-context.md` (signatures `javap`, recensement, divergences). Points durs : JS appelé mais script BATCAVE rejeté par Nashorn ; WebView atteint par 5 sources qui échouent mesurées ; `redrawImageResponse` = code mort ; config figée aux défauts | Brancher le parser comme intercepteur (équivalent `CommonHeadersInterceptor`) ; contrat d'authentification par source |
| 6.3 | API Tauri v2 (externalBin, protocole URI, cookies, notifications, deep link, updater) | **Non mesuré** — hors outil autorisé (interdisait l'app, les ports 3000/9223 et Playwright). Aucun correctif assumé | Tout §6.3 ; de même l'API de `WebviewWindow::cookies_for_url` citée au journal 0.11 |
| 6.4 | Protocole `kotatsu-syncserver` + backup mobile | **Non mesuré** (aucune lecture de code, aucun dépôt cloné) | Tout §6.4 |
| 6.5 | Disponibilité du nom « Tatami » | **Oui, partiellement.** `api.github.com/orgs/tatami` → HTTP 200, organisation **Tatami** créée le 2012-10-11, 0 dépôt public → **le nom GitHub est pris**. `crates.io/api/v1/crates/tatami` → HTTP 200 (**nom Rust pris**). `registry.npmjs.org/tatami` → HTTP 200 (**nom npm pris**). 438 dépôts GitHub contiennent « tatami » dans leur nom | Disponibilité d'un **domaine** et des **stores** : non mesuré. Décision humaine requise |
| 6.6 | Empreinte mémoire réelle du sidecar (12 Go, outils ouverts) | **Oui, mesurée** (§ 7 ci-dessous) | Rien sur la mesure elle-même ; l'interprétation (« 4 Go libres pendant la mesure ») est à refaire sur une machine calme |
| 6.7 | 0.11 de bout en bout sur un vrai site protégé | **Partiellement.** Il existe enfin des sources qui renvoient **vraiment** `CHALLENGE_REQUIRED` : **REMANGA** (`img.reimg.org`) et **MADARADEX** (`cdn.madaradex.org`). En revanche MANGANATO et KAGANE renvoient `INTERNAL` en enveloppant l'exception Cloudflare → la fenêtre ne s'ouvrirait pas | La fin de chaîne (ouvrir la fenêtre, résoudre, `setCookies`, rejouer) **n'a pas pu être testée** : elle demande l'application, interdite ici. Reste aussi l'hypothèse « Referer = URL de l'image » sur REMANGA, non isolée |

## 7. Mesures d'exploitation du moteur (question 6.6)

`p06-memoire-arrete.mjs`, moteur en `-Xmx256m -XX:+UseSerialGC`, **une autre session
tenant l'application ouverte** (donc machine chargée, chiffres à lire comme des ordres
de grandeur, pas des records absolus) :

| État | Working Set | Privé |
|---|---|---|
| au repos (après `hello`) | **72,1 Mo** | 231,4 Mo |
| après `listSources` (1 370 sources) | 79,2 Mo | 230,5 Mo |
| `latest` × 20 résultats, 5 sources | **pic 137,1 Mo** (TOOMICSFR) | **pic 261,5 Mo** |
| fin des requêtes | 123,3 Mo | 252,8 Mo |

- Cohérent avec la mesure de la phase 0.7 (`idle WS 71,9 / privé 231,7` ; `search`
  `WS 114,5 / privé 256,2`). **Le plafond `-Xmx256m` tient** : le privé dépasse
  256 Mo parce qu'il inclut la réserve d'adresses, pas le tas.
- **Démarrage à froid** (spawn → `hello` répondu), 5 essais : 3 595 / 2 890 / 3 822 / 2 334 /
  2 224 ms, contre **1 576 ms** mesuré en 0.7 : machine chargée, ne pas y voir une
  régression.
- **Arrêt** — mesure utile pour le superviseur Rust : **fermer stdin ne tue pas le
  moteur** (le process est encore vivant 4 s après, il faut le tuer) ; la commande
  `shutdown` donne `exit 0` en **137 ms**. Le superviseur doit donc envoyer
  `shutdown` avant de fermer stdin, et garder un `kill` en secours.

## 8. Ce que je n'ai pas fait (et pourquoi)

- Aucun code, aucun test, aucun `PLAN.md`, aucun `AGENTS.md` modifié : contrainte de la
  session. **Les deux affirmations fausses de AGENTS.md §14 sont signalées, pas
  corrigées** (§ 2).
- Aucune correction des défauts trouvés (mapping `INTERNAL` au lieu de
  `CHALLENGE_REQUIRED`, `Referer` faux, `redrawImageResponse` inatteignable,
  authentification par source absente du contrat) : cette session mesure.
- `fetchImage` sur REMANGA avec un vrai cookie `cf_clearance` : il faudrait l'app.
- Statut HTTP exact du refus de `img.reimg.org` (403 ou 503) : le moteur ne
  l'expose pas et aucun autre client HTTP n'a été utilisé.

## 9. Reproduction

```
$ node p01-hello-sources.mjs        # hello + listSources + répartition par langue
$ node p02-remanuga-ua.mjs          # matrice de 5 user-agents sur REMANGA et SEIMANGA
$ node p03-mangabuff.mjs            # erreurs exactes de MANGABUFF
$ node p04-sample.mjs [groupe...]   # échantillon FR/EN/ES/PT/DE + état connu
$ node p05-misc.mjs                 # WebView, JS, ZENMANGA, MANGABUFF par chapitre
$ node p06-memoire-arrete.mjs       # mémoire, démarrage à froid, arrêt
$ node scan-parsers.mjs evaluateJs interceptWebViewRequests redrawImageResponse …
$ javap -p -classpath <kotatsu-parsers-redo-b3c0000.jar extrait> org.koitharu.kotatsu.parsers.MangaLoaderContext
```

Le `stdout` du moteur arrive en morceaux de 8 Ko : **bufferiser avant de découper sur
`\n`**, sinon `listSources` « ne répond pas » alors qu'il a répondu (47 Ko).