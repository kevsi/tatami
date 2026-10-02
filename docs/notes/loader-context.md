# Note — `MangaLoaderContext` : ce que le moteur desktop doit implémenter

Date : 2026-10-02 (tâche 0.3). Sources lues sur GitHub, rien n'est deviné.

## Références lues

| Dépôt | Révision lue | Contenu exploité |
|---|---|---|
| `Kotatsu-Redo/kotatsu-parsers-redo` | commit `b3c0000` (2026-09-17, HEAD, pas de tags ni releases) | `README.md`, `MangaLoaderContext.kt`, `bitmap/Bitmap.kt` |
| `KotatsuApp/kotatsu-dl` | `master` (v0.2) | `build.gradle.kts`, `MangaLoaderContextImpl.kt` (implémentation non-Android de référence) |

Liens : [README parsers-redo](https://github.com/Kotatsu-Redo/kotatsu-parsers-redo/blob/master/README.md),
[`MangaLoaderContext.kt`](https://github.com/Kotatsu-Redo/kotatsu-parsers-redo/blob/master/src/main/kotlin/org/koitharu/kotatsu/parsers/MangaLoaderContext.kt),
[`MangaLoaderContextImpl.kt` (dl)](https://github.com/KotatsuApp/kotatsu-dl/blob/master/src/main/kotlin/org/koitharu/kotatsu/dl/parsers/MangaLoaderContextImpl.kt).

## Coordonnées de dépendance (exactes)

- Dépôt Maven : `https://jitpack.io` (JitPack construit à la première requête ; sans releases, on utilise le hash de commit ou `master-SNAPSHOT`, cf. [doc JitPack](https://docs.jitpack.io)).
- Artefact : `com.github.Kotatsu-Redo:kotatsu-parsers-redo:<commit>` — épingler un SHA, ex. `b3c0000` (7 chars OK) ou le SHA long `b3c000024563920d3eecde5816f2e9d3734fa467`.
- ⚠️ Le `README.md` du fork est périmé : il cite encore `com.github.KotatsuApp:kotatsu-parsers` et le JitPack de `KotatsuApp`. Ne pas copier ces coordonnées.
- Modèle de dépendances transposé de `kotatsu-dl/build.gradle.kts` (à ajuster en 0.4) :
  `kotlinx-coroutines-core-jvm:1.10.2`, `okhttp:4.12.0`, `okio:3.11.0`,
  `org.openjdk.nashorn:nashorn-core:15.6` (voir § JS), `org.json:json:20240303`.
  kotatsu-dl compile en `jvmToolchain(17)` ; nous visons JDK 21 (vérifié en 0.2 : Temurin 21.0.12.1).
- kotatsu-dl épingle l'amont d'origine `com.github.KotatsuApp:kotatsu-parsers:14cff0d651` — **pas** le fork redo. Ne pas confondre les deux artefacts.

## `MangaLoaderContext` (redo) — inventaire exact

Fournies par la classe de base : `newParserInstance(source)`, `newLinkResolver(url)`, `encodeBase64`/`decodeBase64`, `getPreferredLocales()`.

| Membre | Statut | Rôle |
|---|---|---|
| `httpClient: OkHttpClient` | abstract | Client HTTP des parsers. dl : timeouts 20/60/20 s + intercepteurs cookies, Cloudflare, gzip, rate-limit, headers communs |
| `cookieJar: CookieJar` | abstract | dl : `InMemoryCookieJar`. Chez nous : mémoire vive uniquement, Rust fait foi via `setCookies` (sidecar sans état persistant, cf. AGENTS.md §4) |
| `evaluateJs(script)` | abstract (deprecated, « Provide a base url ») | Garder pour compat, déléguer à la version 3 args |
| `evaluateJs(baseUrl, script, timeout)` | abstract | ⚠️ Signature à 3 args : kotatsu-dl (amont d'origine) n'implémente que la 2 args — le fork a évolué, implémenter la 3 args |
| `getConfig(source)` | abstract | Config par source (dl : `DefaultMangaSourceConfig()`) |
| `getDefaultUserAgent()` | abstract | dl : `UserAgents.FIREFOX_DESKTOP` |
| `redrawImageResponse(response, redraw)` | abstract | Recomposition d'images (désembrouillage) |
| `createBitmap(width, height)` | abstract | Fabrique de bitmap vide |
| `requestBrowserAction(parser, url)` | open (lève `UnsupportedOperationException`) | Captcha/auth externe → c'est notre fenêtre de résolution 0.11 |
| `requestCloudflareVerification(parser, url)` | open (délègue à `requestBrowserAction`) | Vérif Cloudflare → même fenêtre 0.11 |
| `interceptWebViewRequests(url, script[, timeout])`, `interceptWebViewRequests(url, config)`, `captureWebViewUrls(pageUrl, pattern)` | open (lèvent `UnsupportedOperationException`) | Nouveautés du fork : interception de requêtes WebView. Sans implémentation desktop, les sources qui en dépendent échouent — repli = fenêtre Tauri (0.11) ou échec typé |

## Points durs desktop (ce qui pose problème, et la réponse prévue)

1. **JavaScript — Nashorn absent du JDK 21.** Le JDK ne fournit plus de moteur JS depuis 15.
   kotatsu-dl répond par `org.openjdk.nashorn:nashorn-core:15.6` + `ScriptEngineManager().getEngineByName("nashorn")`
   (évalué sur `Dispatchers.Default` via `runInterruptible`, résultat `null` si vide/`"null"`).
   Reprendre tel quel. Le `ShadowJar.minimize` de dl exclut nashorn de la minimisation — retenir pour le packaging 0.7.
2. **Images — pas d'`android.graphics`.** `Bitmap` n'exige que `width`, `height`, `drawBitmap(src, srcRect, dstRect)`.
   dl l'implémente en ~30 lignes sur `BufferedImage` (`ImageIO.read`, recomposition, `compress("png")`).
   Transposable à l'identique en desktop (AWT disponible dans le runtime `jlink` — vérifier que les modules `java.desktop` sont inclus, cf. 0.7).
3. **WebView — pas de WebView embarquée côté moteur.** Les 3 méthodes d'interception lèvent par défaut.
   Tant que 0.11 n'est pas fait, toute source les appelant renvoie une erreur typée (`CHALLENGE_REQUIRED` si c'est un challenge, `INTERNAL` sinon) au lieu de planter.
4. **Cloudflare — intercepteur dédié.** dl ajoute un `CloudFlareInterceptor` (package `...dl.parsers`, à lire en 0.4) et laisse l'utilisateur résoudre via navigateur.
   Même stratégie : cookies/user-agent récupérés par la fenêtre Tauri → `setCookies` → rejouer.
5. **Cookies — le moteur ne persiste rien.** `InMemoryCookieJar` suffit ; la persistance vit dans SQLite côté Rust (contrat `setCookies`).

## À VÉRIFIER en 0.4 (pas avant d'écrire le `build.gradle.kts`)

- [ ] JitPack résout et construit `com.github.Kotatsu-Redo:kotatsu-parsers-redo:b3c0000` (déclencher le build, noter l'heure).
- [ ] `nashorn-core:15.6` fonctionne sur JDK 21 avec un parser qui évalue du JS.
- [ ] Version d'OkHttp attendue par le fork (lire son `build.gradle`) — aligner `okhttp`/`okio`, ne pas supposer 4.12.0.
- [ ] Lire `CloudFlareInterceptor.kt`, `InMemoryCookieJar.kt`, `BitmapImpl.kt` de kotatsu-dl (même package `...dl.parsers`) avant de les transposer.
- [ ] Quelles sources `MANGADEX` et candidates du spike appellent `interceptWebViewRequests` (si oui, 0.11 devient bloquant pour elles).
