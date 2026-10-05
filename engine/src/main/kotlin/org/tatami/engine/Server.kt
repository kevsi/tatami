package org.tatami.engine

import java.net.SocketTimeoutException
import java.util.Base64
import java.util.concurrent.ConcurrentHashMap
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit
import kotlin.system.exitProcess
import kotlinx.coroutines.TimeoutCancellationException
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import okhttp3.Cookie
import okhttp3.Request
import org.json.JSONArray
import org.json.JSONObject
import org.koitharu.kotatsu.parsers.model.ContentRating
import org.koitharu.kotatsu.parsers.model.Manga
import org.koitharu.kotatsu.parsers.model.MangaChapter
import org.koitharu.kotatsu.parsers.model.MangaListFilter
import org.koitharu.kotatsu.parsers.model.MangaListFilterOptions
import org.koitharu.kotatsu.parsers.model.MangaPage
import org.koitharu.kotatsu.parsers.model.MangaParserSource

const val PROTOCOL_VERSION = "0.1"
const val ENGINE_VERSION = "0.1.0-SNAPSHOT"

// Version des parsers épinglée : même valeur que build.gradle.kts (pas de tags/releases amont).
const val PARSERS_VERSION = "b3c0000"

private const val DEFAULT_TIMEOUT_MS = 120_000L

// Combien de temps on laisse les requêtes déjà acceptées se terminer après la
// fermeture de stdin. `DEFAULT_TIMEOUT_MS` est le pire cas ; on ne veut pas
// bloquer l'arrêt jusque-là, juste ne pas tronquer une réponse en cours d'écriture.
private const val DELAI_VIDE_EOF_MS = 5_000L

private class ParamException(message: String) : IllegalArgumentException(message)

// Cache jetable de session (AGENTS.md §4 : pas d'état persistant côté moteur).
// Permet details/chapters/pages/fetchImage par URL sans reconstruire les objets :
// search/details remplissent le cache, les appels suivants le consomment.
private class SessionCache {
    private val manga = ConcurrentHashMap<String, Manga>()
    private val chapters = ConcurrentHashMap<String, MangaChapter>()
    private val pages = ConcurrentHashMap<String, MangaPage>()

    private fun key(source: String, url: String) = "$source|$url"

    fun putManga(source: String, m: Manga) {
        manga[key(source, m.url)] = m
    }

    fun getManga(source: String, url: String): Manga? = manga[key(source, url)]

    fun putChapters(source: String, list: List<MangaChapter>) {
        list.forEach { chapters[key(source, it.url)] = it }
    }

    fun getChapter(source: String, url: String): MangaChapter? = chapters[key(source, url)]

    fun putPages(source: String, list: List<MangaPage>) {
        list.forEach { pages[key(source, it.url)] = it }
    }

    fun getPage(source: String, url: String): MangaPage? = pages[key(source, url)]
}

private fun JSONObject.reqString(name: String): String =
    if (has(name)) getString(name) else throw ParamException("missing param: $name")

// Tâche 0.17 : la fiche série a besoin de l'état, des genres et du résumé.
// `getDetails` les remplit, `getList` les laisse souvent vides : on n'envoie
// que ce qui est réellement présent, l'interface affiche une ligne par champ.
private fun mangaToJson(m: Manga, source: String): JSONObject = JSONObject()
    .put("url", m.url)
    .put("title", m.title)
    .put("source", source)
    .put("publicUrl", m.publicUrl)
    .put("coverUrl", m.coverUrl ?: JSONObject.NULL)
    .put("authors", JSONArray(m.authors.toList()))
    // `altTitle` est déprécié côté parsers au profit du set `altTitles` : on prend
// le set, c'est la même information sans passer par une API sur le départ.
.put("altTitle", m.altTitles.firstOrNull() ?: JSONObject.NULL)
    .put("state", m.state?.name ?: "UNKNOWN")
    .put("rating", m.rating.toDouble())
    // L'interface s'en sert pour le mode incognito de l'accueil : un titre
    // sans classification reste visible, c'est le plus prudent.
    .put("contentRating", m.contentRating?.name ?: "UNKNOWN")
    .put("description", m.description ?: JSONObject.NULL)
    .put(
        "tags",
        JSONArray().apply {
            m.tags.forEach { tag ->
                put(
                    JSONObject()
                        .put("key", tag.key)
                        .put("title", tag.title)
                )
            }
        },
    )

private fun chapterToJson(c: MangaChapter): JSONObject = JSONObject()
    .put("url", c.url)
    .put("title", c.title ?: JSONObject.NULL)
    .put("number", c.number.toDouble())
    .put("uploadDate", c.uploadDate)

private fun pageToJson(index: Int, p: MangaPage): JSONObject = JSONObject()
    .put("index", index)
    .put("url", p.url)
    .put("previewUrl", p.preview ?: JSONObject.NULL)

private fun inferLang(name: String, className: String): String {
    SourceLocaleIndex.langOf(name)?.let { return it }
    val suffixes = linkedMapOf(
        "PTBR" to "pt", "PT_BR" to "pt", "ES419" to "es", "ESLA" to "es",
        "EN" to "en", "ES" to "es", "FR" to "fr", "DE" to "de", "IT" to "it",
        "JA" to "ja", "KO" to "ko", "ZH" to "zh", "RU" to "ru", "BR" to "pt",
        "AR" to "ar", "ID" to "id", "TH" to "th", "VI" to "vi", "PL" to "pl",
        "NL" to "nl", "TR" to "tr",
    )
    // Suffixe explicite uniquement (« _FR ») : « MANGASORIGINES » finirait sinon
    // par « ES » et serait classé espagnol à tort.
    for ((suffix, lang) in suffixes) {
        if (name.endsWith("_$suffix")) return lang
    }
    when (name) {
        "MANGAFR" -> return "fr"
        "MANGABR" -> return "pt"
        "MANGABALL_BR" -> return "pt"
        "READMANGA_RU", "KOHARU", "IKIRU" -> return "ru"
    }
    return when {
        name.contains("MANGAFR") || name.contains("SCANFRSCANS") -> "fr"
        name.contains("MANGABR") || name.endsWith("BR") -> "pt"
        className.contains(".en.") -> "en"
        className.contains(".fr.") -> "fr"
        className.contains(".es.") -> "es"
        className.contains(".ja.") -> "ja"
        className.contains(".ko.") -> "ko"
        className.contains(".ru.") -> "ru"
        className.contains(".pt.") -> "pt"
        className.contains(".de.") -> "de"
        else -> "int"
    }
}

private fun ok(id: String, result: JSONObject): String =
    JSONObject().put("id", id).put("ok", true).put("result", result).toString()

private fun err(id: String, code: String, message: String, url: String? = null): String {
    val e = JSONObject().put("code", code).put("message", message)
    if (url != null) e.put("url", url)
    return JSONObject().put("id", id).put("ok", false).put("error", e).toString()
}

private fun parseSource(name: String): MangaParserSource = try {
    MangaParserSource.valueOf(name)
} catch (_: IllegalArgumentException) {
    throw ParamException("unknown source: $name")
}

/**
 * Webtoon à utiliser pour `getDetails`.
 *
 * Le cache de session est vide après un redémarrage du moteur : or c'est
 * exactement le cas d'une reprise de téléchargement (l'app rouvre un webtoon
 * enregistré, elle ne repasse pas par une recherche). On reconstruit donc un
 * `Manga` minimal à partir de l'url et du titre — la plupart des parsers s'en
 * contentent puisque `getDetails` repart de `manga.url`.
 */
private fun mangaForDetails(
    cache: SessionCache,
    sourceName: String,
    url: String,
    title: String,
    source: MangaParserSource,
): Manga = cache.getManga(sourceName, url) ?: Manga(
    id = 0L,
    title = title.ifBlank { url },
    altTitles = emptySet(),
    url = url,
    publicUrl = url,
    rating = 0f,
    contentRating = null,
    coverUrl = null,
    tags = emptySet(),
    state = null,
    authors = emptySet(),
    source = source,
)

/**
 * Classification demandée : par défaut on exclut ADULT.
 * Beaucoup de parsers n'acceptent qu'UNE seule classification
 * (Â« Multiple Content ratings are not supported by this source Â», MangaDex) :
 * on ne renvoie donc jamais un set de plusieurs valeurs, et seulement si la
 * source annonce cette classification dans ses options.
 */
private fun requestedRatings(p: JSONObject, options: MangaListFilterOptions): Set<ContentRating> {
    val wanted = when {
        p.has("contentRating") -> p.optString("contentRating", "")
            .takeIf { it.isNotBlank() }
            ?.let { runCatching { ContentRating.valueOf(it.uppercase()) }.getOrNull() }
        p.optBoolean("safeOnly", true) -> ContentRating.SAFE
        else -> null
    } ?: return emptySet()
    return if (wanted in options.availableContentRating) setOf(wanted) else emptySet()
}

/**
 * Télécharge une image par le client du moteur (UA du moteur, cookie jar,
 * intercepteurs Cloudflare / lazy-load) et rend les **octets** + le type
 * réellement annoncé. Commun à `fetchImage` et `fetchCover` : c'est le même
 * chemin réseau, la seule différence est l'origine de l'URL.
 */
private fun fetch_octets(
    context: EngineContext,
    url: String,
    referer: String,
): Pair<ByteArray, String> {
    if (!url.startsWith("http://") && !url.startsWith("https://")) {
        throw ParamException("not an http(s) url: $url")
    }
    val request = Request.Builder()
        .url(url)
        .header("User-Agent", context.getDefaultUserAgent())
        .header("Referer", referer)
        .get()
        .build()
    // Note 0.6 : pas de désembrouillage ici (redrawImageResponse sert dans les
    // intercepteurs du parser). Les sources qui embrouillent auront un traitement
    // dédié quand un cas réel l'exigera — MangaDex sert des images directes.
    val response = context.httpClient.newCall(request).execute()
    response.use {
        if (!it.isSuccessful) throw IllegalStateException("HTTP ${it.code}")
        val body = checkNotNull(it.body) { "Empty image body" }
        val bytes = body.bytes()
        if (bytes.isEmpty()) throw IllegalStateException("empty image body for $url")
        return bytes to (body.contentType()?.toString() ?: "application/octet-stream")
    }
}

fun serve() {
    val context = EngineContext()
    val pool = ParserPool(context)
    val cache = SessionCache()
    val stdin = System.`in`.bufferedReader()
    System.err.println("tatami-engine serve protocol=$PROTOCOL_VERSION engine=$ENGINE_VERSION parsers=$PARSERS_VERSION")
    // Une requête = un thread : une source lente ou morte ne doit pas figer les
    // autres (le contrat route déjà les réponses par id). Écriture stdout
    // sérialisée : stdout reste le canal du protocole.
    val workers = Executors.newCachedThreadPool { r ->
        Thread(r, "tatami-worker").apply { isDaemon = true }
    }
    val stdout = System.out
    // Compteur des requêtes en vol : `shutdownNow()` interromp les threads, et
    // une réponse est parfois déjà partie (voir plus bas). Sans ce compteur, on
    // ne sait plus quand il est honnête de mourir.
    val enVol = java.util.concurrent.atomic.AtomicInteger(0)
    while (true) {
        val line = stdin.readLine() ?: break // EOF = arrêt demandé par Rust
        val req = try {
            JSONObject(line)
        } catch (e: Exception) {
            synchronized(stdout) {
                stdout.println(err("", "PARSE_ERROR", "invalid JSON: ${e.message}"))
                stdout.flush()
            }
            continue
        }
        val id = req.optString("id", "")
        val method = req.optString("method", "")
        val params = if (req.has("params")) req.getJSONObject("params") else JSONObject()
        val timeoutMs = req.optLong("timeoutMs", DEFAULT_TIMEOUT_MS)
        workers.execute {
            enVol.incrementAndGet()
            val response = try {
                val result = runBlocking {
                    withTimeout(timeoutMs) { dispatch(context, pool, cache, method, params) }
                }
                ok(id, result)
            } catch (e: TimeoutCancellationException) {
                err(id, "TIMEOUT", "exceeded ${timeoutMs}ms")
            } catch (e: SocketTimeoutException) {
                err(id, "TIMEOUT", e.message ?: "socket timeout")
            } catch (e: CloudFlareProtectionException) {
                err(id, "CHALLENGE_REQUIRED", "challenge at ${e.url}", e.url)
            } catch (e: RateLimitedException) {
                err(id, "RATE_LIMITED", e.message ?: "rate limited")
            } catch (e: ParamException) {
                err(id, "PARSE_ERROR", e.message ?: "bad params")
            } catch (e: Throwable) {
                err(id, "INTERNAL", "${e.javaClass.simpleName}: ${e.message}")
            }
            // stdout = protocole uniquement (jamais de logs ici).
            synchronized(stdout) {
                stdout.println(response)
                stdout.flush()
            }
            enVol.decrementAndGet()
            if (method == "shutdown") {
                // Un `shutdown` explicite : on répond, on coupe, on sort. Les
                // requêtes concurrentes sont abandonnées sans réponse, ce qui est
                // correct : Rust vient de demander l'arrêt et purge `pending`.
                workers.shutdownNow()
                exitProcess(0)
            }
        }
    }
    // EOF sur stdin : Rust a fermé le canal.
    //
    // On ne coupe pas immédiatement. `shutdownNow()` interromp les workers, et
    // une réponse peut être **déjà écrite** quand le thread est interrompu à la
    // ligne suivante : stdout se termine alors par une ligne tronquée, que le
    // lecteur JSON de Rust rejette — et le superviseur prend ce rejet pour le
    // signe d'un moteur cassé.
    //
    // On laisse donc les requêtes en vol se terminer. `workers.shutdown()` n'interrompt
    // rien : les requêtes déjà acceptées rendent leur réponse normalement, et
    // l'arrêt n'a lieu qu'une fois le compteur revenu à zéro.
    workers.shutdown()
    val fin = System.currentTimeMillis() + DELAI_VIDE_EOF_MS
    while (enVol.get() > 0 && System.currentTimeMillis() < fin) {
        Thread.sleep(20)
    }
    if (enVol.get() > 0) {
        System.err.println("tatami-engine: ${enVol.get()} requete(s) en vol a la fermeture de stdin, abandon")
        workers.shutdownNow()
    }
}

private suspend fun dispatch(
    context: EngineContext,
    pool: ParserPool,
    cache: SessionCache,
    method: String,
    p: JSONObject,
): JSONObject = when (method) {
    "hello" -> JSONObject()
        .put("protocolVersion", PROTOCOL_VERSION)
        .put("engineVersion", ENGINE_VERSION)
        .put("parsersVersion", PARSERS_VERSION)
        // Rust s'en sert pour donner à la fenêtre de résolution anti-bot le même
        // user-agent que le moteur : une clearance est émise pour un agent donné.
        .put("defaultUserAgent", context.getDefaultUserAgent())

    "listSources" -> {
        val arr = JSONArray()
        MangaParserSource.entries.forEach { src ->
            val lang = inferLang(src.name, "")
            arr.put(JSONObject().put("name", src.name).put("lang", lang))
        }
        JSONObject().put("sources", arr)
    }

    "filterOptions" -> pool.avec(parseSource(p.reqString("source"))) { parser ->
        val options = parser.getFilterOptions()
        val tags = JSONArray()
        options.availableTags.forEach { tags.put(JSONObject().put("title", it.title).put("key", it.key)) }
        val orders = JSONArray()
        parser.availableSortOrders.forEach { orders.put(it.name) }
        JSONObject().put("tags", tags).put("sortOrders", orders)
    }

    "latest" -> {
        val sourceName = p.reqString("source")
        val offset = p.optInt("offset", 0)
        val limit = p.optInt("limit", 20)
        val query = p.optString("query", "")
        val tagKeys = p.optJSONArray("tags")
        pool.avec(parseSource(sourceName)) { parser ->
            // availableSortOrders est un Set : on le convertit pour pouvoir choisir
            // un tri par son rang.
            val orders = parser.availableSortOrders.toList()
            // Tri explicite si demandé, sinon le premier disponible (comportement
            // initial). L'app mobile permet de choisir le tri ; sans cela on reste
            // bloqué sur un seul sous-ensemble du catalogue.
            val orderIndex = p.optInt("sortOrder", 0)
            val order = if (orderIndex in orders.indices) orders[orderIndex] else orders[0]
            val options = parser.getFilterOptions()
            val tags = if (tagKeys == null || tagKeys.length() == 0) {
                emptySet()
            } else {
                val keys = (0 until tagKeys.length()).map { tagKeys.getString(it) }.toSet()
                runCatching { options.availableTags.filter { it.key in keys }.toSet() }
                    .getOrDefault(emptySet())
            }
            val filter = MangaListFilter(
                query = query.ifBlank { null },
                tags = tags,
                contentRating = requestedRatings(p, options),
            )
            val list = parser.getList(offset, order, filter).take(limit)
            list.forEach { cache.putManga(sourceName, it) }
            val arr = JSONArray()
            list.forEach { arr.put(mangaToJson(it, sourceName)) }
            JSONObject().put("list", arr)
        }
    }

    "search" -> {
        val sourceName = p.reqString("source")
        val query = p.reqString("query")
        val offset = p.optInt("offset", 0)
        val limit = p.optInt("limit", 20)
        pool.avec(parseSource(sourceName)) { parser ->
            val order = parser.availableSortOrders.first()
            val filter = MangaListFilter(
                query = query,
                contentRating = requestedRatings(p, parser.getFilterOptions()),
            )
            val list = parser.getList(offset, order, filter).take(limit)
            list.forEach { cache.putManga(sourceName, it) }
            val arr = JSONArray()
            list.forEach { arr.put(mangaToJson(it, sourceName)) }
            JSONObject().put("list", arr)
        }
    }

    "details" -> {
        val sourceName = p.reqString("source")
        val url = p.reqString("url")
        val title = p.optString("mangaTitle", "")
        val parsed = parseSource(sourceName)
        val seed = mangaForDetails(cache, sourceName, url, title, parsed)
        val full = pool.avec(parsed) { it.getDetails(seed) }
        cache.putManga(sourceName, full)
        JSONObject().put("manga", mangaToJson(full, sourceName))
    }

    "chapters" -> {
        val sourceName = p.reqString("source")
        val mangaUrl = p.reqString("mangaUrl")
        val title = p.optString("mangaTitle", "")
        val parsed = parseSource(sourceName)
        val seed = mangaForDetails(cache, sourceName, mangaUrl, title, parsed)
        val full = pool.avec(parsed) { it.getDetails(seed) }
        val list = full.chapters ?: emptyList()
        cache.putManga(sourceName, full)
        cache.putChapters(sourceName, list)
        val arr = JSONArray()
        list.forEach { arr.put(chapterToJson(it)) }
        JSONObject().put("chapters", arr)
    }

    "pages" -> {
        val sourceName = p.reqString("source")
        val chapterUrl = p.reqString("chapterUrl")
        val cached = cache.getChapter(sourceName, chapterUrl)
            ?: throw ParamException("unknown chapter url, call chapters first: $chapterUrl")
        val list = pool.avec(parseSource(sourceName)) { it.getPages(cached) }
        cache.putPages(sourceName, list)
        val arr = JSONArray()
        list.forEachIndexed { i, page -> arr.put(pageToJson(i, page)) }
        JSONObject().put("pages", arr)
    }

    // Couverture : elle aussi passe par le moteur (UA, cookies, intercepteurs),
    // jamais par un appel direct de la webview. Sans cela une source derrière
    // Cloudflare renvoyait une couverture vide, en silence.
    "fetchCover" -> {
        val sourceName = p.reqString("source")
        val mangaUrl = p.reqString("mangaUrl")
        val cached = cache.getManga(sourceName, mangaUrl)
        // Le cache de session est vide après un redémarrage : on retombe alors
        // sur l'URL mémorisée par Rust (favoris, signets, historique), qu'on
        // refuse de traiter si elle n'est pas une adresse web.
        val coverUrl = when {
            !cached?.coverUrl.isNullOrBlank() -> cached!!.coverUrl!!
            else -> p.optString("coverUrl", "")
                .takeIf { it.startsWith("http://") || it.startsWith("https://") }
                ?: throw ParamException("no cover known for $sourceName / $mangaUrl")
        }
        val referer = cached?.publicUrl?.takeIf { it.startsWith("http") }
            ?: p.optString("referer", "")
                .takeIf { it.startsWith("http") }
            ?: coverUrl.substringBeforeLast('/', "https://" + coverUrl.substringAfter("//").substringBefore('/'))
        val (bytes, contentType) = fetch_octets(context, coverUrl, referer)
        JSONObject()
            .put("data", Base64.getEncoder().encodeToString(bytes))
            .put("contentType", contentType)
    }

    "fetchImage" -> {
        val sourceName = p.reqString("source")
        val pageUrl = p.reqString("pageUrl")
        val cached = cache.getPage(sourceName, pageUrl)
            ?: throw ParamException("unknown page url, call pages first: $pageUrl")
        val imageUrl = pool.avec(parseSource(sourceName)) { it.getPageUrl(cached) }
        // Garde-fou : une URL d'image vide n'est pas une adresse à aller
        // chercher. Sans ce test, le client HTTP partait sur l'origine du site
        // et renvoyait sa page d'accueil — mesuré : 45 entrées HTML identiques
        // dans un `.cbz` prétendument valide. Le cache était en prime indexé
        // sur cette chaîne vide, donc les 45 pages partageaient la meme cle.
        if (imageUrl.isBlank()) {
            throw ParamException(
                "the parser returned an empty image URL for $sourceName " +
                    "(page key '${p.optString("pageUrl", "")}')",
            )
        }
        // Même chemin réseau que `fetchCover` : UA du moteur, cookie jar,
        // intercepteurs Cloudflare / lazy-load.
        val (bytes, contentType) = fetch_octets(context, imageUrl, imageUrl)
        // Note 0.6 : pas de désembrouillage ici (redrawImageResponse sert dans les
        // intercepteurs du parser). Les sources qui embrouillent auront un traitement
        // délégué quand un cas réel l'exigera — MangaDex sert des images directes.
        JSONObject()
            .put("data", Base64.getEncoder().encodeToString(bytes))
            .put("contentType", contentType)
    }

    "setCookies" -> {
        val sourceName = p.reqString("source")
        parseSource(sourceName) // valide le nom
        val cookies = p.optJSONArray("cookies") ?: JSONArray()
        val built = (0 until cookies.length()).map { i ->
            val c = cookies.getJSONObject(i)
            Cookie.Builder()
                .name(c.reqString("name"))
                .value(c.reqString("value"))
                .domain(c.reqString("domain"))
                .path(c.optString("path", "/"))
                .expiresAt(System.currentTimeMillis() + TimeUnit.DAYS.toMillis(365))
                .build()
        }
        val host = if (built.isNotEmpty()) built.first().domain.trimStart('.') else ""
        (context.cookieJar as InMemoryCookieJar).putCookies(host, built)
        if (p.has("userAgent")) context.userAgentOverride = p.getString("userAgent")
        JSONObject().put("ok", true)
    }

    "health" -> {
        if (!p.has("source")) {
            JSONObject().put("ok", true).put("engineVersion", ENGINE_VERSION)
        } else {
            val sourceName = p.getString("source")
            parseSource(sourceName) // lève si inconnue
            JSONObject().put("source", sourceName).put("reachable", true)
        }
    }

    "shutdown" -> JSONObject().put("ok", true)

    else -> throw ParamException("unknown method: $method")
}
