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

private fun mangaToJson(m: Manga, source: String): JSONObject = JSONObject()
    .put("url", m.url)
    .put("title", m.title)
    .put("source", source)
    .put("publicUrl", m.publicUrl)
    .put("coverUrl", m.coverUrl ?: JSONObject.NULL)
    .put("authors", JSONArray(m.authors.toList()))

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
 * Classification demand�e : par d�faut on exclut ADULT.
 * Beaucoup de parsers n'acceptent qu'UNE seule classification
 * (� Multiple Content ratings are not supported by this source �, MangaDex) :
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

fun serve() {
    val context = EngineContext()
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
            val response = try {
                val result = runBlocking {
                    withTimeout(timeoutMs) { dispatch(context, cache, method, params) }
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
            if (method == "shutdown") {
                workers.shutdownNow()
                exitProcess(0)
            }
        }
    }
    workers.shutdownNow()
}

private suspend fun dispatch(
    context: EngineContext,
    cache: SessionCache,
    method: String,
    p: JSONObject,
): JSONObject = when (method) {
    "hello" -> JSONObject()
        .put("protocolVersion", PROTOCOL_VERSION)
        .put("engineVersion", ENGINE_VERSION)
        .put("parsersVersion", PARSERS_VERSION)

    "listSources" -> {
        val arr = JSONArray()
        MangaParserSource.entries.forEach { src ->
            val lang = inferLang(src.name, "")
            arr.put(JSONObject().put("name", src.name).put("lang", lang))
        }
        JSONObject().put("sources", arr)
    }

    "filterOptions" -> {
        val sourceName = p.reqString("source")
        val parser = context.newParserInstance(parseSource(sourceName))
        val options = parser.getFilterOptions()
        val tags = JSONArray()
        options.availableTags.forEach { tags.put(JSONObject().put("title", it.title).put("key", it.key)) }
        JSONObject().put("tags", tags)
    }

    "latest" -> {
        val sourceName = p.reqString("source")
        val offset = p.optInt("offset", 0)
        val limit = p.optInt("limit", 20)
        val query = p.optString("query", "")
        val tagKeys = p.optJSONArray("tags")
        val parser = context.newParserInstance(parseSource(sourceName))
        val order = parser.availableSortOrders.first()
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

    "search" -> {
        val sourceName = p.reqString("source")
        val query = p.reqString("query")
        val offset = p.optInt("offset", 0)
        val limit = p.optInt("limit", 20)
        val parser = context.newParserInstance(parseSource(sourceName))
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

    "details" -> {
        val sourceName = p.reqString("source")
        val url = p.reqString("url")
        val title = p.optString("mangaTitle", "")
        val parsed = parseSource(sourceName)
        val seed = mangaForDetails(cache, sourceName, url, title, parsed)
        val parser = context.newParserInstance(parsed)
        val full = parser.getDetails(seed)
        cache.putManga(sourceName, full)
        JSONObject().put("manga", mangaToJson(full, sourceName))
    }

    "chapters" -> {
        val sourceName = p.reqString("source")
        val mangaUrl = p.reqString("mangaUrl")
        val title = p.optString("mangaTitle", "")
        val parsed = parseSource(sourceName)
        val seed = mangaForDetails(cache, sourceName, mangaUrl, title, parsed)
        val parser = context.newParserInstance(parsed)
        val full = parser.getDetails(seed)
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
        val parser = context.newParserInstance(parseSource(sourceName))
        val list = parser.getPages(cached)
        cache.putPages(sourceName, list)
        val arr = JSONArray()
        list.forEachIndexed { i, page -> arr.put(pageToJson(i, page)) }
        JSONObject().put("pages", arr)
    }

    "fetchImage" -> {
        val sourceName = p.reqString("source")
        val pageUrl = p.reqString("pageUrl")
        val cached = cache.getPage(sourceName, pageUrl)
            ?: throw ParamException("unknown page url, call pages first: $pageUrl")
        val parser = context.newParserInstance(parseSource(sourceName))
        val imageUrl = parser.getPageUrl(cached)
        val request = Request.Builder()
            .url(imageUrl)
            .header("User-Agent", context.getDefaultUserAgent())
            .header("Referer", imageUrl)
            .get()
            .build()
        // Note 0.6 : pas de désembrouillage ici (redrawImageResponse sert dans les
        // intercepteurs du parser). Les sources qui embrouillent auront un traitement
        // dédié quand un cas réel l'exigera � MangaDex sert des images directes.
        val response = context.httpClient.newCall(request).execute()
        response.use {
            if (!it.isSuccessful) throw IllegalStateException("HTTP ${it.code}")
            val body = checkNotNull(it.body) { "Empty image body" }
            val bytes = body.bytes()
            JSONObject()
                .put("data", Base64.getEncoder().encodeToString(bytes))
                .put("contentType", body.contentType()?.toString() ?: "application/octet-stream")
        }
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
            context.newParserInstance(parseSource(sourceName)) // lève si inconnue
            JSONObject().put("source", sourceName).put("reachable", true)
        }
    }

    "shutdown" -> JSONObject().put("ok", true)

    else -> throw ParamException("unknown method: $method")
}
