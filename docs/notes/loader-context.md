# Note — `MangaLoaderContext` : ce que le moteur desktop doit implémenter

> Réécrite le **2026-10-04** (session de mesures, sans build ni application).
> Complète la note de la tâche 0.3 (2026-10-02), qui lisait l'amont sur GitHub.
> Cette version ajoute : (a) les **signatures exactes** lues dans le JAR réellement
> embarqué (`javap`), (b) un **recensement** des sources qui utilisent chaque
> capacité du contexte, (c) ce que **kotatsu-dl** répond, (d) la **divergence
> mesurée** entre l'implémentation de Tatami (`engine/src/main/kotlin/org/tatami/engine/`)
> et celle de kotatsu-dl, (e) ce qui a été **exercé pour de vrai** par le moteur.

## 1. Références lues

| Dépôt / artefact | Révision | Contenu exploité |
|---|---|---|
| `Kotatsu-Redo/kotatsu-parsers-redo` | commit `b3c000024563920d3eecde5816f2e9d3734fa467` (HEAD, 0 tag, 0 release) | source Kotlin lue sur `raw.githubusercontent.com` |
| JAR embarqué | `engine/dist/tatami-engine/lib/kotatsu-parsers-redo-b3c0000.jar` | **4 000 classes**, signatures via `javap`, recensement par scan du pool de constantes |
| `KotatsuApp/kotatsu-dl` | `master` | `MangaLoaderContextImpl.kt`, `CommonHeadersInterceptor.kt` |
| Tatami (moteur) | `engine/src/main/kotlin/org/tatami/engine/` | `EngineContext.kt`, `Server.kt`, `DefaultMangaSourceConfig.kt`, `CloudFlareInterceptor.kt` |

Chemins amont cités dans cette note (racine : `src/main/kotlin/org/koitharu/kotatsu/parsers/`) :

- `MangaLoaderContext.kt` — l'interface abstraite (classe `org/koitharu/kotatsu/parsers/MangaLoaderContext.class` dans le JAR)
- `network/OkHttpWebClient.kt`, `network/CloudFlareHelper.kt`, `network/UserAgents.kt`, `network/WebClient.kt`
- `config/MangaSourceConfig.kt`, `config/ConfigKey.kt`
- `bitmap/Bitmap.kt`, `bitmap/Rect.kt`
- `webview/InterceptedRequest.kt` (contient `InterceptedRequest`, `InterceptionConfig`, `WebViewRequestInterceptor`)
- `util/WebViewHelper.kt`
- `exception/AuthRequiredException.kt`
- `site/mangareader/MangaReaderParser.kt`, `MangaParser.kt`, `core/AbstractMangaParser.kt`

kotatsu-dl : `src/main/kotlin/org/koitharu/kotatsu/dl/parsers/MangaLoaderContextImpl.kt` et `CommonHeadersInterceptor.kt`.

## 2. Coordonnées de dépendance (inchangées, revérifiées)

- `com.github.Kotatsu-Redo:kotatsu-parsers-redo:b3c0000` (JitPack), déclaré dans `engine/build.gradle.kts:17`.
- Le JAR produit s'appelle littéralement `kotatsu-parsers-redo-b3c0000.jar` → **la coordonnée est bien celle qui est chargée**, preuve par le nom de fichier du JAR dans `engine/dist/tatami-engine/lib/`.
- ⚠️ Le `README.md` du fork est toujours périmé (cite `KotatsuApp` / l'artefact d'origine). Ne pas le copier.
- Alignement mesuré sur le contenu réel de `engine/dist/tatami-engine/lib/` (18 JAR) : `kotlinx-coroutines-core-jvm 1.10.2`, `okhttp-jvm 5.1.0`, `okio-jvm 3.16.0`, `org.json 20240303`, `nashorn-core 15.6`, `kotlin-stdlib 2.2.10`, `jsoup 1.21.2`, `collection-jvm 1.5.0` (androidx.collection : `ArrayMap` est utilisé par `MangaReaderParser`, cf. `javap` → champ `androidx.collection.ArrayMap tagCache`).

## 3. `MangaLoaderContext` — inventaire exact (`javap`, JAR embarqué)

Fournies par la classe de base, **rien à implémenter** : `newParserInstance(source)`,
`newLinkResolver(HttpUrl|String)`, `encodeBase64(byte[])`, `decodeBase64(String)`,
`getPreferredLocales()`.

| Membre | Statut (`javap`) | Ce que le moteur doit fournir |
|---|---|---|
| `getHttpClient(): OkHttpClient` | abstract | Client HTTP partagé par **toutes** les sources, intercepteurs compris (voir §6) |
| `getCookieJar(): CookieJar` | abstract | `InMemoryCookieJar` (`engine/.../InMemoryCookieJar.kt`), alimenté par `setCookies` |
| `evaluateJs(String): String?` | abstract, `@Deprecated("Provide a base url")` | Nashorn (`nashorn-core:15.6`) |
| `evaluateJs(String, String, long): String?` | abstract | **3 arguments** (baseUrl, script, timeout). kotatsu-dl n'en implémente que 2 : c'est bien le fork qui a évolué |
| `getConfig(MangaSource): MangaSourceConfig` | abstract | `DefaultMangaSourceConfig` : renvoie `key.defaultValue` |
| `getDefaultUserAgent(): String` | abstract | `UserAgents.FIREFOX_DESKTOP` (exposé par `hello`) |
| `redrawImageResponse(Response, (Bitmap)->Bitmap): Response` | abstract | Recomposition via AWT/`ImageIO` + `BitmapImpl` |
| `createBitmap(int, int): Bitmap` | abstract | `BufferedImage` TYPE_INT_RGB |
| `requestBrowserAction(parser, url)` | open, `Nothing` | `throw UnsupportedOperationException("Browser is not available")` par défaut |
| `requestCloudflareVerification(parser, url)` | open, délègue à `requestBrowserAction` | idem |
| `interceptWebViewRequests(url, script, timeout)` | open, `List<InterceptedRequest>` | `UnsupportedOperationException("WebView request interception is not available")` |
| `interceptWebViewRequests(url, InterceptionConfig)` | open, retombe sur la version simple | idem |
| `captureWebViewUrls(pageUrl, Regex, timeout)` | open, `List<String>` | `UnsupportedOperationException("WebView URL capture is not available")` |

Types de retour utiles : `InterceptedRequest(url, method, headers: Map, timestamp, body)`
(+ `getQueryParameter`, `urlMatches`, `urlContains`) ; `InterceptionConfig(timeoutMs, maxRequests, urlPattern, filterScript, pageScript)`.

## 4. Recensement : qui utilise quoi (scan du JAR, 4 000 classes)

Dénominateur : **1 354 classes `site/` de premier niveau** pour **1 370 sources**
(`MangaParserSource.entries`). Un scan de chaîne dans les `.class` trouve toute classe
qui *déclare ou appelle* le membre.

| Capacité du contexte | Parsers (classes) | Sources |
|---|---|---|
| `evaluateJs` | 17 parseurs + `MangaReaderParser` (classe de base) | `MangaReaderParser` est étendue par **261** classes (`madara` : 551) → le besoin de JS est **transitif**, pas 17 sources |
| `interceptWebViewRequests` | **2** : `site.en.AllManga`, `site.en.Comix` | ALLMANGA, COMIX |
| `captureWebViewUrls` | **3** : `site.en.BatCave`, `site.en.YaoiMangaOnline`, `site.madara.pt.MangaLivre` | BATCAVE, YAOIMANGAONLINE, MANGALIVRE |
| `requestBrowserAction` | **13** | Kagane, Koharu, LunarAnime, Comicazen, BatCave, Comikuro, Comix, MangaCloud, YaoiMangaOnline, MadaraDex, RaijinScans, MangaLivre, YomuMangas |
| `requestCloudflareVerification` | **3** : Kagane, Comikuro, Comix | — |
| `redrawImageResponse` | **7** : ExHentai, MangaReaderTo, Comix, Mangago, PhiliaScans, MimiHentai, YuriGarden | — |
| `getConfig` | 260 parseurs | — |
| `getRequestHeaders` | ~80 parseurs | — |
| `AuthRequiredException` | 16 parseurs (lève sur **HTTP 401**, voir §5) | — |

Répartition de la famille `MangaReaderParser` (261 sous-classes) : id 71, en 58, tr 38, es 26, th 20, ar 18, **fr 14**, pt 10, ja 2, it 2, pl 1, cz 1.
Famille `MadaraParser` (551) : en 219, pt 93, tr 70, es 66, ar 27, **fr 21**, vi 12, id 12, th 11, all 8, ru 4, ja 3, de 1, zh 1, it 1, ko 1, pl 1.

## 5. Comportements amont qui décident du contrat

- **`CloudFlareHelper.checkResponseForProtection`** (`network/CloudFlareHelper.kt`) ne signale une protection que si le statut est **403 ou 503** *et* que le corps HTML contient `h2[data-translate="blocked_why_headline"]` (BLOCKED) ou `#challenge-error-title` / `#challenge-error-text` (CAPTCHA). Un 403 « nu » reste un 403 : `CHALLENGE_REQUIRED` ne part pas. Constantes : `PROTECTION_NOT_DETECTED=0`, `PROTECTION_CAPTCHA=1`, `PROTECTION_BLOCKED=2`.
- **`AuthRequiredException`** (`exception/AuthRequiredException.kt`) est levée par `OkHttpWebClient.ensureSuccess()` **uniquement sur HTTP 401** (`HTTP_UNAUTHORIZED`), et seulement si la requête porte le tag `MangaSource` (qu'`OkHttpWebClient` ajoute systématiquement). `404` → `NotFoundException`, `400..599` restants → `HttpStatusException`. **Il n'existe donc aucun chemin « authentification requise » dans `CHALLENGE_REQUIRED`** : les deux notions sont distinctes dans l'API.
- **`MangaParser extends okhttp3.Interceptor`.** Le parser *est* un intercepteur : `MangaReaderParser` s'en sert pour `isNetShieldProtected` / `getNetShieldCookie`, et `MangaReaderToParser`/`ExHentaiParser` appellent `MangaLoaderContext.redrawImageResponse(...)` **depuis leur `intercept()`** (`javap -c` : `invokevirtual redrawImageResponse` à l'intérieur de `MangaReaderToParser$intercept$1`).
- **`MangaParser.getRequestHeaders()`** (~80 parsers) et **`MangaParserAuthProvider`** (`authUrl`, `isAuthorized()`, `getUsername()`) existent dans l'API. Le contrat Tatami n'a **rien** pour l'authentification par source (seulement `setCookies`).

## 6. Ce que kotatsu-dl répond, et où Tatami s'en écarte

`MangaLoaderContextImpl` (kotatsu-dl) : `cookieJar` = `InMemoryCookieJar` ; un **seul** `OkHttpClient` partagé avec `CloudFlareInterceptor`, `GZipInterceptor`, `RateLimitInterceptor`, **`CommonHeadersInterceptor(this)`** ; timeouts 20/60/20 s ; `evaluateJs` par Nashorn ; `getConfig` → `DefaultMangaSourceConfig` ; `getDefaultUserAgent` → FIREFOX_DESKTOP ; `redrawImageResponse`/`createBitmap` sur `BufferedImage`. Les 4 méthodes WebView/navigateur **ne sont pas implémentées non plus** — Tatami a donc le même plafond que kotatsu-dl.

`CommonHeadersInterceptor` (kotatsu-dl) est la pièce centrale, et c'est **elle qui manque** à Tatami. Sur chaque requête elle :

1. lit le tag `MangaSource` de la requête → récupère le parser → fusionne `parser.getRequestHeaders()` ;
2. met `User-Agent` = `getDefaultUserAgent()` si absent, et **`Referer = https://<parser.domain>/`** (IDN) si absent ;
3. **si le parser est un `Interceptor`, délègue la chaîne à `parser.interceptSafe(...)`**.

`engine/src/main/kotlin/org/tatami/engine/EngineContext.kt` ne fait que le UA (ligne 36-42). Écarts mesurés :

| Capacité | kotatsu-dl | Tatami (`EngineContext.kt`) | Effet |
|---|---|---|---|
| En-têtes par source (`getRequestHeaders`) | fusionnés | **absents** | ~80 sources perdent leurs en-têtes obligatoires |
| `Referer` | `https://<domaine de la source>/` | `Referer = <url de l'image>` (dans `Server.kt:405`) | non conforme ; **cause du 403 de REMANGA non tranchée** (§8) |
| `parser.intercept()` | appelé (cookies netShield, désembrouillage) | **jamais appelé** (le parser n'est pas dans la chaîne d'intercepteurs) | 261 sources `MangaReaderParser` + 7 sources de désembrouillage perdent ce chemin ; `redrawImageResponse` est **du code mort** |
| GZip | `GZipInterceptor` | absent | non mesuré (OkHttp transparently décompresse gzip) |

## 7. Ce qui a été **exercé pour de vrai** (moteur seul, sans l'application)

Le moteur a été piloté en NDJSON (`hello` → `listSources` → `latest`/`search` → `details` → `chapters` → `pages` → `fetchImage`). Ce qui suit est **observé**, pas déduit :

| Capacité | Source d'essai | Résultat observé |
|---|---|---|
| `evaluateJs` (Nashorn) | BATCAVE | **atteint** : `ScriptException: <eval>:1:2 Expected an operand but found )` → le moteur JS est bien appelé, mais le script du parser est rejeté par Nashorn 15.6 |
| `requestBrowserAction` | MANGALIVRE | **atteint** : `INTERNAL: UnsupportedOperationException: Browser is not available` |
| `interceptWebViewRequests` | ALLMANGA | **atteint indirectement** : `ParseException: Mkissa reader initialization failed at https://mkissa.to/...` |
| `requestCloudflareVerification` / protection CF | COMIX | `ParseException: Comix could not get past the Cloudflare check` |
| `getConfig` | toutes | aucun échec observé, mais **aucune preuve directe** que la valeur lue ≠ défaut (non mesuré) |
| `redrawImageResponse` | — | **jamais atteint** : appelé seulement depuis `intercept()`, jamais invoqué (§6) |
| `createBitmap` | — | jamais atteint (même raison) |

## 8. Points durs desktop : état réel

1. **JavaScript** — Nashorn est bien présent et appelé (BATCAVE le prouve), mais **au moins un script de parser est incompatible** avec Nashorn 15.6. Nashorn ne comprend pas toute la syntaxe moderne : c'est un vrai bloqueur, pas une question de packaging.
2. **Images / `android.graphics`** — `Bitmap` ne demande que `width`, `height`, `drawBitmap(src, srcRect, dstRect)` ; `BitmapImpl` sur `BufferedImage` suffit, mais il n'est **atteignable que si le parser est branché comme intercepteur** (§6).
3. **WebView** — 2 sources (`ALLMANGA`, `COMIX`) et 3 autres (`BATCAVE`, `YAOIMANGAONLINE`, `MANGALIVRE`) échouent **mesurées** tant que `interceptWebViewRequests` / `captureWebViewUrls` lèvent. Ces méthodes exigent une WebView réelle : la seule voie desktop est la fenêtre Tauri, avec un contrat de plus (`InterceptionConfig` contient `filterScript` **et** `pageScript`, à injecter dans la page).
4. **Cloudflare** — l'intercepteur du moteur (`CloudFlareInterceptor.kt`, 21 lignes) est une transposition fidèle de `CloudFlareHelper`. Il marche : 2 sources renvoient aujourd'hui `CHALLENGE_REQUIRED` (voir `docs/rapport-session-recherche.md`).
5. **Cookies** — mémoire vive seule, `setCookies` alimente `InMemoryCookieJar`. **Piège mesuré** : `CloudFlareHelper.isCloudFlareCookie` considère `csrftoken` comme un cookie Cloudflare → la fenêtre de résolution doit transmettre large, pas seulement `cf_clearance`.
6. **Configuration par source** — `MangaSourceConfig` est une interface à une seule méthode `get(ConfigKey<T>)`, et `ConfigKey` a des sous-classes `Domain`, `Preference`, `Integer`, `Show`… 260 parseurs la consomment. `DefaultMangaSourceConfig` renvoie `key.defaultValue` : **correct mais figé** (pas de domaine alternatif par source, pas de préférences).
7. **Authentification par source** — absent du contrat. `MangaParserAuthProvider` existe dans l'API et `AuthRequiredException` est levée sur 401 par 16 parseurs ; le mapping actuel renvoie `INTERNAL`, ce qui est le pire choix (une erreur d'authentification n'ouvre pas la fenêtre de résolution et se lit comme une panne).

## 9. À VÉRIFIER (état au 2026-10-04)

- [x] JitPack résout et construit `com.github.Kotatsu-Redo:kotatsu-parsers-redo:b3c0000` → JAR présent dans `engine/dist/tatami-engine/lib/`, `listSources` = **1 370** sources.
- [x] Versions alignées sur le fork → okhttp **5.1.0**, okio 3.16.0, kotlin 2.2.10, coroutines 1.10.2 (contenu réel de `lib/`).
- [x] Nashorn est présent et **appelé** → mais BATCAVE échoue sur le script (§7). La case n'est donc **pas** résolue : elle devient « moteur JS actif, compatibilité des scripts à traiter source par source ».
- [x] Quelles sources appellent `interceptWebViewRequests` → **ALLMANGA, COMIX** (§4), plus `captureWebViewUrls` : BATCAVE, YAOIMANGAONLINE, MANGALIVRE.
- [ ] **Brancher le parser comme intercepteur** (équivalent de `CommonHeadersInterceptor`) : c'est la condition pour que `redrawImageResponse`, les cookies netShield et les en-têtes par source cessent d'être du code mort. Non fait, hors périmètre de cette session (session de mesures).
- [ ] **Pourquoi `img.reimg.org` (REMANGA) refuse** : le `Referer` envoyé par `Server.kt:405` vaut l'URL de l'image. 5 user-agents différents ont été essayés, **tous refusés** ; l'hypothèse « Referer » est donc plausible mais **non isolée** (le moteur n'expose aucun levier d'en-tête).
- [ ] Contrat d'authentification par source (`MangaParserAuthProvider`) : à concevoir ou à assumer absent.