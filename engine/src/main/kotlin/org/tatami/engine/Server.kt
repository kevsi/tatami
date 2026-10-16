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
import okhttp3.HttpUrl.Companion.toHttpUrl
import okhttp3.OkHttpClient
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

const val PROTOCOL_VERSION = "0.2"
const val ENGINE_VERSION = "0.1.0-SNAPSHOT"

// Version des parsers épinglée : même valeur que build.gradle.kts (pas de tags/releases amont).
const val PARSERS_VERSION = "b3c0000"

private const val DEFAULT_TIMEOUT_MS = 120_000L

// Combien de temps on laisse les requêtes déjà acceptées se terminer après la
// fermeture de stdin. `DEFAULT_TIMEOUT_MS` est le pire cas ; on ne veut pas
// bloquer l'arrêt jusque-là, juste ne pas tronquer une réponse en cours d'écriture.
private const val DELAI_VIDE_EOF_MS = 5_000L

// `internal` et non `private` : le chemin réseau du fetcha d'image doit etre
// eprouve par un test, et `private` le rendait invisible depuis `src/test`.
// C'est une visibilite de **module**, donc l'API publique du moteur reste
// identique — rien n'est expose a l'appelant. Le defaut que cela couvre
// (mesure le 2026-10-15) : le chemin natif n'avait aucun filtre SSRF et son URL
// vient du HTML du site.
internal class ParamException(message: String) : IllegalArgumentException(message)

// Cache jetable de session (AGENTS.md §4 : pas d'état persistant côté moteur).
// Permet details/chapters/pages/fetchImage par URL sans reconstruire les objets :
// search/details remplissent le cache, les appels suivants le consomment.
// `internal` et non `private` : `requete`, exposee aux tests, prend ce cache en
// parametre. Le cache est ce qui relie `chapters` -> `pages` -> `fetchImage`
// (chaque etape refuse une entree inconnue), donc un test doit pouvoir le
// partager entre plusieurs appels.
internal class SessionCache {
    // Instrumentation (item 4, 2026-10-15) : la cache n'est **jamais
    // purgée**. Les compteurs ci-dessous donnent le débit effectif au fur et
    // à mesure, et une ligne part sur stderr tous les 1000 insertions, pour
    // que le magnifique puisse mesurer la croissance **avant** de choisir la
    // stratégie d'éviction. La politique elle-même (LRU, expiration par
    // source, taille max) est la suite de ce travail et n'est pas tranchée
    // ici - la troncature bon marché qui choisirait mal masquerait le
    // symptôme sans en éliminer la cause.
    private val insertions = java.util.concurrent.atomic.AtomicInteger(0)
    private val SEUIL_LOG = 1000
    internal val tailleTotale: Int
        get() = manga.size + chapters.size + pages.size +
            customManga.size + customDetails.size + customChapters.size + customPages.size

    private fun tracerSiSeuil() {
        val n = insertions.get()
        if (n.mod(SEUIL_LOG) != 0) return
        System.err.println(
            "[session-cache] insertion=$n vue totale=" + tailleTotale +
                " manga=${manga.size} chapters=${chapters.size} pages=${pages.size} " +
                "customManga=${customManga.size} customDetails=${customDetails.size} " +
                "customChapters=${customChapters.size} customPages=${customPages.size}",
        )
    }

    private val manga = ConcurrentHashMap<String, Manga>()
    private val chapters = ConcurrentHashMap<String, MangaChapter>()
    private val pages = ConcurrentHashMap<String, MangaPage>()

    // Cache pour les sources custom (valeurs JSON brutes).
    private val customManga   = ConcurrentHashMap<String, JSONObject>()
    private val customDetails = ConcurrentHashMap<String, JSONObject>()
    private val customChapters = ConcurrentHashMap<String, JSONObject>()
    private val customPages   = ConcurrentHashMap<String, JSONObject>()

    private fun key(source: String, url: String) = "$source|$url"

    fun putManga(source: String, m: Manga) {
        manga[key(source, m.url)] = m
        insertions.incrementAndGet()
        tracerSiSeuil()
    }

    fun getManga(source: String, url: String): Manga? = manga[key(source, url)]

    fun putChapters(source: String, list: List<MangaChapter>) {
        list.forEach { chapters[key(source, it.url)] = it }
        insertions.addAndGet(list.size)
        tracerSiSeuil()
    }

    fun getChapter(source: String, url: String): MangaChapter? = chapters[key(source, url)]

    fun putPages(source: String, list: List<MangaPage>) {
        list.forEach { pages[key(source, it.url)] = it }
        insertions.addAndGet(list.size)
        tracerSiSeuil()
    }

    fun getPage(source: String, url: String): MangaPage? = pages[key(source, url)]

    // --- Custom sources ---
    fun putCustomManga(source: String, url: String, m: JSONObject) {
        customManga[key(source, url)] = m
        insertions.incrementAndGet()
        tracerSiSeuil()
    }
    fun putCustomDetails(source: String, url: String, d: JSONObject) {
        customDetails[key(source, url)] = d
        insertions.incrementAndGet()
        tracerSiSeuil()
    }
    fun getCustomDetails(source: String, url: String): JSONObject? = customDetails[key(source, url)]
    fun putCustomChapter(source: String, url: String, c: JSONObject) {
        customChapters[key(source, url)] = c
        insertions.incrementAndGet()
        tracerSiSeuil()
    }
    fun putCustomPage(source: String, url: String, pg: JSONObject) {
        customPages[key(source, url)] = pg
        insertions.incrementAndGet()
        tracerSiSeuil()
    }
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

/**
 * Une requête du contrat, exécutée **dans le même chemin que la vraie
 * `serve()`** : mêmes commandes, même `ParserPool`, même gestionnaire d'erreur.
 *
 * `internal` et non `private` pour que les tests puissent emprunter ce chemin.
 * Le test de la tâche 0.17 en a besoin : sa première version appelait le parser
 * directement, passait au vert, et l'application servait toujours des images
 * illisibles — parce que le téléchargement se faisait **en dehors** du bloc qui
 * pose le parser. Un test qui n'emprunte pas le chemin de production ne prouve
 * rien du chemin de production.
 */
internal suspend fun requete(
    context: EngineContext,
    pool: ParserPool,
    method: String,
    params: JSONObject,
): JSONObject = requete(context, pool, SessionCache(), method, params)

/**
 * Idem, avec un cache fourni.
 *
 * Le cache est **indispensable** : `pages` refuse une URL de chapitre inconnue
 * (« call chapters first »), et `fetchImage` refuse une page inconnue. Chaque
 * étape alimente donc la suivante — un test qui recréerait le cache entre deux
 * appels mesurerait un garde-fou, pas le déchiffrement.
 */
internal suspend fun requete(
    context: EngineContext,
    pool: ParserPool,
    cache: SessionCache,
    method: String,
    params: JSONObject,
): JSONObject = dispatch(
    context, pool, cache,
    ScriptedSourceLoader(java.io.File("."), context.httpClient),
    method, params,
)

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
 *
 * ## Échoue ouvert, et pourquoi ça ne se corrige pas d'un bloc (2026-10-10)
 *
 * L'audit du 2026-10-10 signalait que cette fonction « échoue ouvert » : quand
 * la classification demandée n'est pas applicable, elle rendait `emptySet()` —
 * donc **aucun filtre**. Avec le réglage par défaut de l'interface
 * (`contentRating: "SUGGESTIVE"`, qui exclut l'adulte), une source qui ne
 * l'expose pas livre son catalogue adulte sans le dire. Ce n'est pas
 * théorique : c'est le réglage par défaut.
 *
 * Refuser dans tous les cas aurait été pire. **Mesuré** sur 82 sources
 * (échantillon déterministe : les 80 premières de l'enum + les 4 témoins des
 * fixtures) :
 *
 * | `availableContentRating` | sources |
 * |---|---|
 * | aucun (ensemble vide)   | 39 |
 * | erreur de construction  | 37 |
 * | ADULT, SAFE, SUGGESTIVE | 4 |
 * | SAFE, SUGGESTIVE        | 2 |
 *
 * **6 sources sur 82 seulement annoncent SAFE ou SUGGESTIVE.** Refuser dès que
 * la classification n'est pas exposée aurait donc cassé le catalogue de ~93 %
 * des sources — le réglage par défaut de l'interface n'aurait plus rien
 * affiché nulle part. La règle distingue donc les cas, par ce que
 * « applicable » veut dire :
 *
 * - **valeur d'enum invalide** (`"safee"`, `"adulte"`) : on refuse. Aucun
 *   appelant légitime n'en envoie, et une faute de frappe ne doit pas devenir
 *   une fuite de contenu.
 * - **la source n'annonce AUCUNE classification** : le filtre n'est pas
 *   applicable, elle n'a rien à filtrer. On ne filtre pas — refuser casserait
 *   39 sources sur 82 — mais on l'écrit sur stderr, pour que ce soit
 *   **mesurable** au lieu d'être un silence.
 * - **la source annonce des classifications, pas celle-là** : le filtre est
 *   applicable et la source a choisi de ne pas l'appliquer. On **dégrade vers
 *   la valeur exposée la plus stricte qui reste dans l'ensemble demandé** —
 *   jamais vers une valeur plus permissive (2026-10-12 : MANGASORIGINES
 *   n'expose que `SAFE, ADULT`, le réglage par défaut `SUGGESTIVE` était donc
 *   refusé et la source devenait entièrement inaccessible alors qu'elle répond
 *   5 résultats). Le refus ne survit que si **aucune** valeur exposée n'est dans
 *   l'ensemble demandé : c'est le cas où relaxing serait le seul moyen de
 *   satisfaire la demande, donc où il faut refuser.
 *
 *   `SUGGESTIVE` exclut `ADULT` : la dégradation vers `SAFE` sert donc le
 *   réglage, elle ne le contourne pas.
 *
 * Fonction `internal`, pas `private` : c'est la décision la plus risquée du
 * moteur (elle choisit si du contenu adulte est livré malgré le réglage), et
 * elle était inatteignable depuis un test. Même raison que les `pub` du cœur
 * Rust (cf. `lib.rs`) : une garantie de sécurité doit être **vérifiable**.
 * `RequestedRatingsTest` la couvre sur ses trois branches.
 */
/**
 * Des options de filtre **sans aucune classification annoncée**.
 *
 * C'est la forme que prend un script de source personnalisée : il n'a pas de
 * `getFilterOptions`, donc il ne déclare rien. C'est exactement le cas que la
 * règle native traite déjà — « la source n'annonce AUCUNE classification » —
 * et le nom le dit pour qu'on ne croie pas à un oubli.
 */
@OptIn(org.koitharu.kotatsu.parsers.InternalParsersApi::class)
private fun optionsSansClassification(): MangaListFilterOptions = MangaListFilterOptions(
    availableTags = emptySet(),
    availableStates = emptySet(),
    availableContentRating = emptySet(),
    availableContentTypes = emptySet(),
    availableDemographics = emptySet(),
    availableLocales = emptySet(),
)

internal fun requestedRatings(
    p: JSONObject,
    options: MangaListFilterOptions,
    sourceName: String,
): Set<ContentRating> {
    val demandee = if (p.has("contentRating")) p.optString("contentRating", "") else ""
    // La classification **demandée**, distinguée de « aucune » : une chaîne vide
    // est l'absence de filtre de l'interface (son réglage « Toutes »), pas une
    // faute. La comparaison ignore la casse, donc `"safe"` vaut `"SAFE"`.
    val exploree: ContentRating? = when {
        p.has("contentRating") && demandee.isNotBlank() ->
            ContentRating.entries.firstOrNull { it.name.equals(demandee, ignoreCase = true) }
        p.has("contentRating") -> null
        p.optBoolean("safeOnly", true) -> ContentRating.SAFE
        else -> null
    }

    // Une valeur que l'enum ne connaît pas est une faute de l'appelant, pas un
    // réglage à deviner. Le refus vient AVANT tout `return emptySet()` : dans la
    // première version, le `?: return emptySet()` de l'exploration avalait la
    // valeur inconnue et rendait « aucun filtre » — donc le contenu que
    // l'appelant demandait d'exclure était livré quand même. Le test
    // `une classification inconnue est refusee` a attrapé cette inversion.
    if (p.has("contentRating") && demandee.isNotBlank() && exploree == null) {
        throw ParamException("classification inconnue : « $demandee »")
    }

    val wanted = exploree ?: return emptySet()

    val annoncees = options.availableContentRating
    if (annoncees.isEmpty()) {
        System.err.println(
            "latest/search: classification $wanted demandee mais $sourceName " +
                "n'en annonce aucune — catalogue NON filtre",
        )
        return emptySet()
    }
    if (wanted !in annoncees) {
        // Dégradation vers la classification exposée la PLUS STRICTE qui reste
        // dans l'ensemble demandé. Mesuré le 2026-10-12 : MANGASORIGINES
        // annonce `SAFE, ADULT` et rien d'autre, donc le réglage par défaut de
        // l'interface (`SUGGESTIVE`, qui exclut l'adulte) était refusé — alors
        // que la source répond 5 résultats sans filtre. Le refus rendait
        // 100 % du catalogue de cette source inaccessible.
        //
        // La sécurité tient dans le sens de la dégradation : on ne **relaxe**
        // jamais le filtre. `SUGGESTIVE` doit exclure ADULT, donc tout ce que
        // la source expose et qui est plus strict que SUGGESTIVE convient ;
        // `ADULT` n'y est pas et ne sera jamais livré. Le refus reste le
        // verdict quand aucune valeur exposée n'est dans l'ensemble demandé :
        // c'est le cas verrouillé par `une classification non exposee est
        // refusee` (demander SAFE à une source qui n'expose qu'ADULT).
        val stricte = annoncees
            .filter { it <= wanted }
            .maxByOrNull { it.ordinal }
        if (stricte == null) {
            throw ParamException(
                "classification $wanted non exposee par $sourceName " +
                    "(expose : ${annoncees.joinToString()})",
            )
        }
        System.err.println(
            "latest/search: $sourceName n'expose pas $wanted, " +
                "filtre degrade vers $stricte (expose : ${annoncees.joinToString()})",
        )
        return setOf(stricte)
    }
    return setOf(wanted)
}

/**
 * Télécharge une image par le client du moteur (UA du moteur, cookie jar,
 * intercepteurs Cloudflare / lazy-load) et rend les **octets** + le type
 * réellement annoncé. Commun à `fetchImage` et `fetchCover` : c'est le même
 * chemin réseau, la seule différence est l'origine de l'URL.
 */
internal fun fetch_octets(
    context: EngineContext,
    url: String,
    referer: String,
): Pair<ByteArray, String> = LIMITES_IMAGES.autourDe {
    if (!url.startsWith("http://") && !url.startsWith("https://")) {
        throw ParamException("not an http(s) url: $url")
    }
    // Deux gardes que le chemin **natif** n'avait pas, et le chemin scripté si.
    //
    // 1. HTTPS obligatoire. Mesuré le 2026-10-15 : `http://` était accepté, donc
    //    une source pouvait faire transiter des octets en clair.
    // 2. Adresse littérale bornée. Ici l'URL **vient du HTML du site**
    //    (`coverUrl`, `getPageUrl`) : une source hostile ou compromise
    //    contrôle donc cette chaîne, et le moteur la téléchargeait sans la
    //    relire. `fetchCover` reprenait `coverUrl` sans aucun filtre — la
    //    garantie que le test `FetchImageReseauLocalTest` pretend couvrir ne
    //    s'appliquait qu'au chemin scripté.
    if (!url.startsWith("https://")) {
        throw ParamException("seul https:// est accepte")
    }
    val uri = runCatching { url.toHttpUrl() }.getOrElse {
        throw ParamException("URL illisible (${it.javaClass.simpleName})")
    }
    val hote = uri.host.lowercase()
    if (hote == "localhost" || hote.endsWith(".localhost") || hote.endsWith(".local")) {
        throw ParamException("hote local")
    }
    if (adresseLitteraleEstPublique(hote) == false) {
        throw ParamException("reseau prive, boucle locale ou lien local")
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
        // Plafonné : voir `MAX_OCTETS_IMAGE`. Sans borne, quatre lectures
        // simultanées d'une réponse de 500 Mo suffisaient à gonfler le sidecar.
        val bytes = lireOctetsBornes(it, "image", MAX_OCTETS_IMAGE)
        if (bytes.isEmpty()) throw IllegalStateException("empty image body for $url")
        bytes to (it.body?.contentType()?.toString() ?: "application/octet-stream")
    }
}

/** Préfixe des identifiants de sources personnalisées dans le contrat. */
const val CUSTOM_PREFIX = "custom:"

/**
 * Le catalogue des sources **natives**, dans l'ordre de l'enum.
 *
 * Fonction **pure** du JAR de parseurs épinglé : `domain`,
 * `configKeyDomain.presetValues` et le nom ne changent pas pendant la vie du
 * moteur. C'est ce qui autorise [EngineContext.sourcesNatives] à être mis en
 * cache sans invalidation.
 */
internal fun construireCatalogueNatif(context: EngineContext): List<JSONObject> =
    MangaParserSource.entries.map { src ->
        val parser = context.newParserInstance(src)
        val sourceInfo = JSONObject()
            .put("name", src.name)
            .put("lang", inferLang(src.name, ""))
            .put("custom", false)
        val domaines = domainesNatifs(
            parser.domain,
            parser.configKeyDomain.presetValues.asList(),
        )
        if (domaines.isNotEmpty()) sourceInfo.put("domains", JSONArray(domaines))
        sourceInfo
    }

fun serve() {
    val context = EngineContext()
    val pool = ParserPool(context)
    val cache = SessionCache()

    // **Aucun préchauffage ici**, et c'est un résultat mesuré, pas une absence.
    //
    // Trois variantes essayées le 2026-10-15, sur le moteur réel, en NDJSON :
    //
    //   | variante                          | `hello` | 1er `listSources` |
    //   |------------------------------------|---------|------------------|
    //   | sans rien (référence)              |    77 ms|        2 170 ms  |
    //   | catalogue construit dans `serve()` | 9 524 ms|           50 ms  |
    //   | catalogue sur un fil daemon        | 5 455 ms|           50 ms  |
    //
    // Le coût dominant est le **chargement des classes**, pas le constructeur du
    // parseur — donc il est d'autant plus cher qu'il est déclenché tôt, quand la
    // JVM est la plus froide. Le déplacer ne fait que changer *quand* on paie :
    // sur un fil daemon, il vole encore assez de CPU pour que `hello` passe de
    // 77 ms à 5,5 s.
    //
    // Reste donc la seule variante qui n'aggrave rien : construire **au premier
    // appel**, puis servir le cache. La première ouverture coûte ce qu'elle
    // coûtait, les suivantes ne reconstruisent plus 1 384 parseurs.
    //
    // L'autre levier — ne pas instancier de parser du tout pour lire
    // `domain` et `configKeyDomain` — demanderait une API de la librairie de
    // parseurs qui n'a pas été vérifiée. Conformément à la règle du projet, on ne
    // l'invente pas : `À VÉRIFIER`.

    // Sources personnalisées (ADR 0010) : dossier $APPDATA/Tatami/sources/ ou
    // dossier courant en dev. Le loader est démarré en daemon thread et surveille
    // les changements de fichiers sans bloquer serve().
    val appData = System.getenv("APPDATA") ?: System.getProperty("user.home")
    val dossierId = java.io.File(appData, "Tatami${java.io.File.separator}sources")
    val scriptLoader = ScriptedSourceLoader(dossierId, context.httpClient)
    scriptLoader.demarrer()

    val stdin = System.`in`.bufferedReader()
    System.err.println("tatami-engine serve protocol=$PROTOCOL_VERSION engine=$ENGINE_VERSION parsers=$PARSERS_VERSION")
    // Une requête = un thread : une source lente ou morte ne doit pas figer les
    // autres (le contrat route déjà les réponses par id). Écriture stdout
    // sérialisée : stdout reste le canal du protocole.
    // **Un nombre fini de workers, et non plus une usine d'un thread par
    // requête.**
    //
    // Mesuré le 2026-10-15 : `Executors.newCachedThreadPool` crée un thread à
    // **chaque** requête tant qu'aucun thread n'est disponible ; sans borne
    // supérieure, un flux de requêtes (surtout celles-mêmes lentes)
    // saturait la mémoire avant même que `withTimeout` ait le temps de
    // rendre la main. 6 ou 8 workers suffisent ici : les requêtes sont
    // légères et à durée courte (µs à s), et ce moteur est single-client
    // (un seul superviseur Rust).
    //
    // Le `withTimeout` (ligne 589) reste **non débordable**: une requête
    // qui dépasse son délai fait rentrer `TIMEOUT` au client, mais l'`execute()`
    // OkHttp qui tourne en dessous continue jusqu'à son terme sur son thread.
    // `runBlocking` voit la cancellation à la suspension point, et le thread
    // revient au pool une fois le thread OkHttp terminé. Avec une usine
    // bornee, le pire cas est **bounded** (MAX_WORKERS threads bloqués, pas
    // un nombre sans borne), ce qui rend le défaut observable et limitable
    // au lieu de fatal. Cette partie est un vrai travail restant : raccorder
    // la cancellation à `Call.cancel()`, qui exige de passer le `Job` jusqu'à
    // `fetch_octets`. Bonne tâche isolée, et marquée ici pour ne pas être
    // tentée de nouveau sans y repenser.
    // @todo(3.5) : raccorder cancellation -> OkHttp Call.cancel().
    val MAX_WORKERS = (Runtime.getRuntime().availableProcessors()).coerceIn(2, 8)
    val workers = Executors.newFixedThreadPool (MAX_WORKERS) { r ->
        Thread(r, "tatami-worker").apply { isDaemon = true }
    }
    System.err.println("tatami-engine workers=$MAX_WORKERS")
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
        // `optJSONObject`, **jamais** `getJSONObject`. Mesuré le 2026-10-08 :
        // cette extraction était hors du `try` de la ligne 339, donc
        // `{"id":"a","method":"hello","params":[]}` levait une JSONException —
        // une RuntimeException — qui sortait de `while (true)`, de `serve()` et
        // de `main()`. La JVM mourait, stdout se coupait au milieu d'une ligne
        // selon le flush, et le superviseur Rust croyait le moteur cassé.
        // `opt*` rend une valeur par défaut au lieu de lever : ici, `params`
        // manquant ou mal typé vaut `{}`.
        val params = req.optJSONObject("params") ?: JSONObject()
        val timeoutMs = req.optLong("timeoutMs", DEFAULT_TIMEOUT_MS)
        // `enVol` compte l'**acceptation**, pas l'exécution en cours : il est
        // incrémenté **avant** la soumission, pas dans la tâche.
        //
        // Mesuré le 2026-10-09 : incrémenté dans la tâche, il restait à 0
        // pendant toute la fenêtre entre `execute` et le démarrage du thread.
        // La boucle de vidage qui suit la fermeture de stdin teste
        // `enVol > 0` : elle pouvait donc lire 0 alors que des requêtes
        // étaient en file, conclure qu'il n'y avait rien à attendre et laisser
        // `main` rendre. Les workers étant des threads **daemon**, la JVM les
        // tuait alors en plein vol — 0 réponse sur 5, une fois sur deux
        // environ, ce qui rendait `FermetureStdinTest` flaker alors que le
        // défaut était dans le moteur et pas dans le test.
        //
        // Le décrément reste dans la tâche : il doit durer jusqu'à ce que la
        // réponse soit **écrite et vidée** sur stdout, sinon la boucle peut
        // repartir trop tôt.
        enVol.incrementAndGet()
        workers.execute {
            val response = try {
                val result = runBlocking {
                    withTimeout(timeoutMs) { dispatch(context, pool, cache, scriptLoader, method, params) }
                }
                ok(id, result)
            } catch (e: TimeoutCancellationException) {
                err(id, "TIMEOUT", "exceeded ${timeoutMs}ms")
            } catch (e: SocketTimeoutException) {
                err(id, "TIMEOUT", e.message ?: "socket timeout")
            } catch (e: RateLimitedException) {
                err(id, "RATE_LIMITED", e.message ?: "rate limited")
            } catch (e: ParamException) {
                err(id, "PARSE_ERROR", e.message ?: "bad params")
            } catch (e: Throwable) {
                // Un seul chemin pour les deux exceptions que le parser **emballe** :
                // `CloudFlareProtectionException` et `AuthRequiredException` finissent
                // dans une `IOException` (« canceled due to … »), donc un `catch` sur
                // leur type ne les voit jamais. Mesuré le 2026-10-05 : MANGANATO et
                // KAGANE renvoyaient `INTERNAL` pour un vrai challenge Cloudflare, et
                // la fenêtre de résolution ne s'ouvrait pas.
                //
                // On déballe ici plutôt que dans chaque `catch` : un seul endroit à
                // maintenir, et le cas direct passe par la même voie (`t is X`).
                //
                // **L'ordre des deux tests est une garantie, pas un détail.** Un
                // rejeu refusé doit être reconnu **avant** un challenge ordinaire,
                // sinon il est rendu comme une demande de vérification et la
                // fenêtre se rouvre — le cycle « bandeau → puzzle → rien »
                // mesuré le 2026-10-11 sur HENTAIZONE.
                // **L'ordre des tests est une garantie, pas un détail** (cf. le
                // commentaire d'en-tête de `challengeNonRejouableDe`). Un
                // **blocage** passe devant tout le reste : ouvrir une fenêtre
                // de vérification pour une source que Cloudflare refuse sans
                // défi, c'est faire résoudre l'utilisateur un puzzle qui ne
                // mène à rien — et c'est exactement ce que `PROTECTION_BLOCKED`
                // désigne.
                val blocage = sourceBloqueeDe(e)
                val nonRejouable = challengeNonRejouableDe(e)
                val challenge = if (nonRejouable != null) null else challengeDe(e)
                if (blocage != null) {
                    err(
                        id,
                        "SOURCE_BLOCKED",
                        "source blocked by Cloudflare at $blocage",
                        blocage,
                    )
                } else if (nonRejouable != null) {
                    val detail = detailCauseChallengeNonRejouable(e)
                    err(
                        id,
                        "CHALLENGE_UNREPLAYABLE",
                        "challenge at $nonRejouable (non rejouable; $detail)",
                        nonRejouable,
                    )
                } else if (challenge != null) {
                    err(id, "CHALLENGE_REQUIRED", "challenge at $challenge", challenge)
                } else if (authRequise(e) != null) {
                    err(id, "AUTH_REQUIRED", "this source asks for an account")
                } else {
                    // Le typage ajoute un **code** sans réécrire le détail : le
                    // message garde `Exception: texte`, que `is_transient` côté
                    // Rust reconnaît pour rejouer un échec réseau. Mesuré le
                    // 2026-10-12 : 83 des 98 sources FR remontaient ici un
                    // `INTERNAL` que l'utilisateur ne pouvait pas interpréter.
                    val classe = classerEchecSource(e)
                    if (classe != null) {
                        val (code, cause) = classe
                        err(id, code, "${cause.javaClass.simpleName}: ${cause.message}")
                    } else {
                        err(id, "INTERNAL", "${e.javaClass.simpleName}: ${e.message}")
                    }
                }
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
    scriptLoader.arreter()
}

/**
 * `internal` et non `private` : le **routage** est la partie à risque, pas
 * seulement la construction des cookies.
 *
 * Mesuré le 2026-10-15 : les tests de `SetCookiesSourceCustomTest` appelaient
 * `appliquerCookies` et `validerSourcePourCookies` **directement**, et ils
 * passaient avec l'ancien routage rétabli — donc ils ne couvraient pas le
 * défaut. Un test qui n'epreuve pas le chemin réel prouve la fonction, pas le
 * produit. La visibilité reste bornée au module.
 */
internal suspend fun dispatch(
    context: EngineContext,
    pool: ParserPool,
    cache: SessionCache,
    scriptLoader: ScriptedSourceLoader,
    method: String,
    p: JSONObject,
): JSONObject = when (method) {
    // ---- Routage vers les sources personnalisées (ADR 0010) ----
    "listSources" -> {
        val arr = JSONArray()
        // Natives : catalogue figé, construit une fois (cf. `EngineContext.sourcesNatives`).
        val natives = context.sourcesNatives ?: construireCatalogueNatif(context).also {
            context.sourcesNatives = it
        }
        natives.forEach { arr.put(it) }
        scriptLoader.sources().forEach { (cle, parser) ->
            // La clé du registre EST le nom de fichier sans `.js`
            // (identiteDe) : c'est elle que la base connaît, donc celle
            // que l'interface renvoie et supprime.
            val sourceInfo = JSONObject()
                .put("name", "$CUSTOM_PREFIX$cle")
                .put("lang", parser.meta.lang)
                .put("custom", true)
                .put("displayName", parser.meta.name)
            domaineScript(parser.meta.baseUrl)?.let {
                sourceInfo.put("domains", JSONArray(listOf(it)))
            }
            arr.put(sourceInfo)
        }
        JSONObject().put("sources", arr)
    }
    "search", "latest" -> {
        val sourceName = p.optString("source", "")
        if (sourceName.startsWith(CUSTOM_PREFIX)) {
            dispatchCustom(scriptLoader, cache, sourceName, method, p, context)
        } else {
            dispatchNatif(context, pool, cache, method, p)
        }
    }
    "details", "chapters", "pages", "fetchCover", "fetchImage" -> {
        val sourceName = p.optString("source", "")
        if (sourceName.startsWith(CUSTOM_PREFIX)) {
            dispatchCustom(scriptLoader, cache, sourceName, method, p, context)
        } else {
            dispatchNatif(context, pool, cache, method, p)
        }
    }

    // `setCookies` n'a pas de branche « custom » : les cookies sont stockés par
    // **domaine**, pas par source, donc le parser n'intervient pas. Il faut
    // seulement valider que la source existe — sinon un nom fantôme
    // `custom:inexistant` deposerait des cookies sans que rien ne le relie à une
    // source réelle. La validation est faite ici, une seule fois.
    //
    // Mesuré le 2026-10-15 : sans cette branche, une source perso derrière
    // Cloudflare ne pouvait jamais recevoir son `cf_clearance`, et l'erreur
    // `unknown source` — vraie de la `MangaParserSource`, fausse de la source.
    "setCookies" -> {
        validerSourcePourCookies(scriptLoader, p.optString("source", ""))
        appliquerCookies(context, p)
    }

    else -> dispatchNatif(context, pool, cache, method, p)
}

/**
 * Vérifie qu'une source existe, par son nom, avant d'y rattacher quoi que ce soit.
 *
 * `internal` et non `private` : c'est la partie à risque de `setCookies` — un nom
 * fantôme ne doit rien déposer dans le magasin de cookies — et elle doit être
 * éprouvable sans passer par le protocole NDJSON. La visibilité reste bornée au
 * module : rien n'est exposé à l'appelant.
 */
internal fun validerSourcePourCookies(loader: ScriptedSourceLoader, sourceName: String) {
    if (sourceName.startsWith(CUSTOM_PREFIX)) {
        val id = sourceName.removePrefix(CUSTOM_PREFIX)
        if (!loader.sources().containsKey(id)) {
            throw ParamException("unknown custom source: $id")
        }
    } else {
        parseSource(sourceName) // valide le nom contre la librairie
    }
}

/**
 * Applique les cookies d'une fenêtre de vérification, **quelle que soit la source**.
 *
 * ## Pourquoi cette fonction existe
 *
 * `setCookies` était traité dans `dispatchNatif` seulement, dont la première
 * ligne est `parseSource(sourceName)` → `MangaParserSource.valueOf("custom:x")` →
 * `ParamException("unknown source")`. Mesuré le 2026-10-15 : une source
 * personnalisée derrière un challenge Cloudflare ne pouvait donc **jamais**
 * recevoir son `cf_clearance`, et le cycle de vérification se répétait
 * indéfiniment. L'erreur disait « unknown source », ce qui est vrai de la
 * **`MangaParserSource`** mais trompeur : la source existe, elle n'est juste pas
 * une source de la librairie.
 *
 * ## Pourquoi elle ne valide plus le nom
 *
 * Elle ne connaît pas le nom de la source : les cookies sont stockés par
 * **domaine**, qui est ce que la fenêtre de vérification a réellement résolu.
 * La validation du nom est faite en amont, dans `dispatch`, qui refuse une
 * source personnalisée inconnue — donc rien n'entre dans le magasin pour une
 * source fantôme. Cette fonction ne fait que la partie réseau.
 */
internal fun appliquerCookies(context: EngineContext, p: JSONObject): JSONObject {
    val sourceName = p.optString("source", "")
    val cookies = p.optJSONArray("cookies") ?: JSONArray()
    val maintenant = System.currentTimeMillis()
    val built = (0 until cookies.length()).map { i ->
        val c = cookies.getJSONObject(i)
        val b = Cookie.Builder()
            .name(c.reqString("name"))
            .value(c.reqString("value"))
            // Le domaine arrive sans point initial : okhttp matche alors
            // l'hôte exact **et** ses sous-domaines (verifie dans le
            // bytecode d'okhttp 5.1.0 : `domainMatch` compare l'hôte au
            // suffixe `".$domaine"`). Un domaine **avec** son point
            // initial ne matche RIEN — il faudrait « ..exemple.com » en
            // suffixe. `trimStart` est donc une garantie, pas un
            // embellissement.
            .domain(c.reqString("domain").trimStart('.'))
            .path(c.optString("path", "/"))
            // Expiration réelle lue dans la webview. Absente = cookie de
            // session : une journée par défaut, faute de pouvoir
            // représenter une session côté moteur (il vit après la
            // fermeture de la fenêtre). Jusqu'au 2026-10-10 on forçait
            // maintenant + 365 j, ce qui gardait en circulation un
            // cookie que le site avait déjà fait périmer.
            .expiresAt(
                if (c.has("expiresAt")) {
                    c.getLong("expiresAt")
                } else {
                    maintenant + TimeUnit.DAYS.toMillis(1)
                },
            )
        if (c.optBoolean("secure")) b.secure()
        if (c.optBoolean("httpOnly")) b.httpOnly()
        b.build()
    }
    val host = built.firstOrNull { it.name == "cf_clearance" }?.domain
        ?: built.firstOrNull()?.domain.orEmpty()
    (context.cookieJar as InMemoryCookieJar).putCookies(host, built)
    // Même domaine de cookie pour l'horodatage et le registre de rejeu :
    // un `cf_clearance` de example.com couvre aussi www.example.com.
    if (host.isNotEmpty()) RejeuRefuse.resolutionReussie(host)
    if (p.has("userAgent")) context.definirUserAgentChallenge(host, p.getString("userAgent"))
    if (
        built.any { it.name == "cf_clearance" } &&
        p.has("browserProfile") &&
        !p.isNull("browserProfile")
    ) {
        val browserProfile = p.optJSONObject("browserProfile")
            ?: throw ParamException("browserProfile must be an object")
        context.definirIdentiteNavigateurChallenge(host, BrowserIdentity.fromJson(browserProfile))
    }
    if (challengeDebug) {
        // Ce qui entre vraiment dans le magasin, une ligne par cookie :
        // nom, domaine, chemin, expiration, drapeaux, **longueur** de la
        // valeur — jamais la valeur (règle projet).
        built.forEach { c ->
            System.err.println(
                "[challenge-debug] cookie injecte source=$sourceName nom=${c.name} " +
                    "domaine=${c.domain} chemin=${c.path} expiresAt=${c.expiresAt} " +
                    "secure=${c.secure} httpOnly=${c.httpOnly} " +
                    "longueurValeur=${c.value.length}",
            )
        }
        System.err.println(
            "[challenge-debug] cf_clearance present=${
                built.any { it.name == "cf_clearance" }
            } ua=${if (p.has("userAgent")) p.getString("userAgent") else "-"}",
        )
    }
    return JSONObject().put("ok", true)
}

/**
 * Traite une commande pour une source personnalisée scriptée.
 */
internal suspend fun dispatchCustom(
    scriptLoader: ScriptedSourceLoader,
    cache: SessionCache,
    sourceName: String,
    method: String,
    p: JSONObject,
    context: EngineContext,
): JSONObject {
    val id = sourceName.removePrefix(CUSTOM_PREFIX)
    val parser = scriptLoader.sources()[id]
        ?: throw ParamException("unknown custom source: $id")

    return when (method) {
        "search", "latest" -> {
            val query = p.optString("query", "")
            val limit = maxOf(1, p.optInt("limit", 20))
            val offset = p.optInt("offset", 0)
            // Un `offset` qui n'est pas un multiple de `limit` ne peut venir
            // que d'une page **incomplète** : la précédente a rendu moins
            // d'éléments que demandé, donc la suivante n'existe pas.
            //
            // Mesuré le 2026-10-10 sur `custom:aniverse` : le site n'expose que
            // 14 séries et `limit` vaut 24, donc l'interface rappelait
            // `offset=14`. Or `14 / 24 = 0` en division entière → page 1 → les
            // **14 mêmes séries** ajoutées à la grille, indéfiniment. La liste
            // ne revenant jamais vide, l'avertissement « ne renvoie qu'une
            // seule page » ne s'allumait pas non plus.
            //
            // On rend donc une liste vide : l'interface cesse d'ajouter et
            // `onePageOnly` s'allume — le comportement prévu depuis le début.
            //
            // `dispatchNatif` n'est pas concerné : les sources natives
            // reçoivent `offset` et fenêtrent elles-mêmes, il n'y a pas de
            // conversion offset→page. La règle est donc propre au scripté.
            if (offset > 0 && offset % limit != 0) {
                return JSONObject().put("list", JSONArray())
            }
            val page = offset / limit + 1
            // **Le réglage de classification passe ici aussi**, et le résultat
            // est ce qu'il peut être — c'est-à-dire pas un filtre.
            //
            // Mesuré le 2026-10-15 : ce chemin n'appelait pas
            // `requestedRatings`, donc une source personnalisée servait son
            // contenu en incognito sans que rien ne le dise, pendant que la règle
            // était instrumentée sur 82 sources natives.
            //
            // Deux effets réels, malgré l'absence de filtrage possible :
            //   1. une classification **inconnue** est désormais refusée, comme
            //      partout ailleurs (avant : acceptée en silence) ;
            //   2. une classification demandée mais non annoncée est **écrite sur
            //      stderr**, donc mesurable au lieu d'être un silence.
            //
            // Pourquoi aucun filtrage : `ScriptedParser.getList(page, query)`
            // n'accepte pas de filtre — le contrat d'un script (ADR 0010) ne le
            // prévoit pas — et le script renvoie des mangas **sans**
            // `contentRating`, que le moteur force ensuite à `UNKNOWN`. Il n'y a
            // donc rien à filtrer, et prétendre le contraire serait une fiction.
            //
            // Ce qui reste ouvert est une **décision**, pas un oubli : un script
            // peut servir de l'adulte alors que l'incognito est actif. Bloquer
            // rendrait les sources perso inutilisables en navigation prudente ;
            // ne pas bloquer laisse passer. La politique appliquée est la
            // seconde, rendue visible.
            requestedRatings(p, optionsSansClassification(), sourceName)
            val arr = parser.getList(page, query)
            // Le script ne connaît pas l'identifiant de contrat qu'il reçoit à la
            // recherche. Or c'est l'interface qui le renvoie ensuite à `details`,
            // `chapters` et aux téléchargements : sans ce champ, la commande part
            // sans `source` et Tauri la refuse. Mesuré le 2026-07 : « invalid
            // args `source` for command `chapters`: command chapters missing
            // required key source ». Les sources natives, elles, portent `source`
            // (mangaToJson) — d'où l'écart.
            for (i in 0 until arr.length()) {
                val m = arr.getJSONObject(i)
                m.put("source", sourceName)
                // Un script ne déclare pas de classification : l'interface lit
                // ce champ pour le mode incognito, autant lui donner la valeur
                // explicite des natives sans classification.
                if (!m.has("contentRating")) m.put("contentRating", "UNKNOWN")
                cache.putCustomManga(sourceName, m.optString("url", ""), m)
            }
            JSONObject().put("list", arr)
        }
        "details" -> {
            val mangaUrl = p.reqString("url")
            val result = parser.getDetails(mangaUrl)
            // Le contrat nomme le statut `state` (mangaToJson) ; les scripts, eux,
            // écrivent `status` — c'est le nom que portent les API JSON. Sans
            // cette normalisation, la ligne « Statut » de la fiche disparaît pour
            // toute source scriptée (mesuré le 2026-07 : Auteur, Genres et Source
            // présents, Statut absent).
            if (!result.has("state") && result.has("status")) {
                result.put("state", result.get("status"))
            }
            // Idem pour `url` : c'est la clé du cache de couverture côté Rust
            // (`porter_jeton` sort sans elle), et les natives l'ont toujours
            // (mangaToJson). Le gabarit `docs/exemple-source.js` ne demande pas
            // d'`url` dans `getDetails` — donc tout script conforme s'en
            // passait. Mesuré le 2026-07 : la fiche d'une source scriptée
            // n'affichait aucune couverture, alors que la grille, elle, les
            // montrait (là, la réponse de `search` porte bien `url`).
            if (!result.has("url") || result.isNull("url")) {
                result.put("url", mangaUrl)
            }
            cache.putCustomDetails(sourceName, mangaUrl, result)
            JSONObject().put("manga", result)
        }
        "chapters" -> {
            val mangaUrl = p.reqString("mangaUrl")
            val details = cache.getCustomDetails(sourceName, mangaUrl)
                ?: parser.getDetails(mangaUrl).also { cache.putCustomDetails(sourceName, mangaUrl, it) }
            val chapters = details.optJSONArray("chapters") ?: JSONArray()
            for (i in 0 until chapters.length()) {
                val c = chapters.getJSONObject(i)
                cache.putCustomChapter(sourceName, c.optString("url", ""), c)
            }
            JSONObject().put("chapters", chapters)
        }
        "pages" -> {
            val chapterUrl = p.reqString("chapterUrl")
            val pages = parser.getPages(chapterUrl)
            // **On jette une page sans URL d'image.** Mesuré le 2026-10-09
            // (audit) : sans ce contrôle, `pages` renvoyait 45 entrées dont
            // certaines à URL vide. L'interface les rendait, `onError` les
            // marquait, et l'écran affichait « page indisponible chez la
            // source » — un message **vrai** mais qui accuse la source, alors
            // que le moteur avait reçu une page de vérification d'âge déguisée
            // en succès (HTTP 200 + corps HTML).
            //
            // Le garde de `fetchImage` (URL vide) ne rattrapait qu'au moment du
            // téléchargement, jamais la liste.
            val gardees = JSONArray()
            var jetees = 0
            for (i in 0 until pages.length()) {
                val pg = pages.getJSONObject(i)
                val url = pg.optString("url", "")
                if (url.isBlank() || (!url.startsWith("http://") && !url.startsWith("https://"))) {
                    jetees += 1
                    continue
                }
                cache.putCustomPage(sourceName, url, pg)
                gardees.put(pg)
            }
            if (jetees > 0) {
                System.err.println(
                    "pages: $jetees entree(s) sans URL d'image ignoree(s) pour " +
                        "$sourceName — la source a probablement renvoye une page " +
                        "de verification au lieu de la liste",
                )
            }
            JSONObject().put("pages", gardees)
        }
        "fetchCover" -> {
            // Les sources custom servent les images directement par URL.
            val mangaUrl = p.reqString("mangaUrl")
            val coverUrl = validerUrlImage(
                p.optString("coverUrl", ""),
                "cover URL for custom source $id / $mangaUrl",
            )
            telechargerImage(coverUrl, "cover body", context, parser.meta.baseUrl)
        }
        "fetchImage" -> {
            // Même validation que `fetchCover`, et pour la même raison : sans
            // elle, `pageUrl` partait tel quel dans OkHttp. Mesuré le
            // 2026-10-08 : un script `.js` — donc du code que le moteur exécute
            // — pouvait faire sortir une requête vers `http://127.0.0.1:…`,
            // `http://192.168.0.1/…` ou la métadonnée d'instance d'un
            // hébergeur, et les octets revenaient en base64 pour finir dans un
            // `.cbz`. Le test `FetchImageReseauLocalTest` le prouve en
            // vérifiant qu'un serveur local n'a reçu **aucune** requête.
            val pageUrl = validerUrlImage(p.reqString("pageUrl"), "page URL for custom source $id")
            telechargerImage(pageUrl, "image body", context, parser.meta.baseUrl)
        }
        else -> throw ParamException("method $method not supported for custom sources")
    }
}

/**
 * Contrôle une URL d'image servie par une source scriptée.
 *
 * Deux règles, dans cet ordre :
 *
 *  1. **HTTPS obligatoire.** Un `http://` est refusé : le trafic d'images
 *     n'a aucune raison d'être en clair, et c'est aussi la règle qu'applique
 *     déjà `HttpBridge` pour les requêtes des scripts.
 *  2. **Pas d'hôte interne.** Le préfixe `https://` seul laisse passer
 *     `https://192.168.0.1/` et `https://127.0.0.1/` — c'est-à-dire
 *     l'interface d'administration du routeur, ou un service local. On refuse
 *     donc les plages privées, la boucle locale et le lien local.
 *
 * Le refus est une `ParamException`, donc le client reçoit un code d'erreur du
 * contrat au lieu d'une image.
 */
private fun validerUrlImage(url: String, quoi: String): String {
    fun refuser(raison: String): Nothing =
        throw ParamException("no $quoi : $raison (refuse : ${url.take(80)})")

    if (url.isBlank()) refuser("URL vide")
    val uri = try {
        java.net.URI(url)
    } catch (e: Exception) {
        refuser("URL illisible (${e.javaClass.simpleName})")
    }
    if (!uri.scheme.equals("https", ignoreCase = true)) {
        refuser("seul https:// est accepte")
    }
    val hote = uri.host?.lowercase() ?: refuser("hote absent de l'URL")
    if (hote == "localhost" || hote.endsWith(".localhost") || hote.endsWith(".local")) {
        refuser("hote local")
    }
    // Une adresse litterale est bornee — y compris sous ses formes exotiques
    // (decimale `2852039166`, hexadecimale `0x7f000001`, octale, abregee) et en
    // IPv6 litterale. Le validateur est celui de `HttpBridge`, donc les deux
    // disent la meme chose : c'etait le defaut mesure le 2026-10-15, le
    // validateur faible etant celui du chemin des images.
    //
    // Un nom public n'est pas borne ici, et c'est assume : resoudre le nom
    // ouvrirait la porte a un DNS rebinding. La resolution appartient au `Dns` de
    // `HttpBridge`, qui refuse des qu'une seule adresse retournee n'est pas
    // publique. Le prefixe `https://` reste obligatoire dans tous les cas.
    if (adresseLitteraleEstPublique(hote) == false) {
        refuser("reseau prive, boucle locale ou lien local")
    }
    return url
}

/**
 * Le client HTTP à utiliser pour télécharger une image.
 *
 * Extrait en fonction pour une raison précise : **c'est exactement là que se
 * trouvait le défaut**. Le code était `val context2 = EngineContext()`, et
 * `context2.httpClient` — un contexte neuf par image, donc un magasin de cookies
 * vide et l'UA par défaut, alors que `setCookies` avait posé le cookie de
 * challenge sur le contexte de `serve()`. Toutes les images repartaient en 403.
 * Effet de bord du même défaut : un `OkHttpClient` par image, donc aucune
 * réutilisation de connexion, donc un handshake TLS par page.
 *
 * Un test d'intégration avec serveur local ne peut pas couvrir ce point :
 * `validerUrlImage` refuse `http://`, et un serveur en `https://` exigerait un
 * certificat de confiance. La couture est ici, et `CookieRejoueTest` vérifie que
 * le client rendu est bien celui du contexte — donc que le cookie et l'UA
 * survivent au chemin scripté.
 */
internal fun clientPourImage(context: EngineContext): OkHttpClient = context.clientImage

/**
 * Construit la requête de téléchargement d'une image **servie par une source
 * scriptée**, avec l'en-tête `Referer` que le chemin natif envoie déjà.
 *
 * Isolé dans une fonction pour être éprouvé : `telechargerImage` est `private`
 * et son client vient d'un `EngineContext` sans couture d'injection, donc un
 * test ne peut pas observer la requête sur le fil. Ce que ce test garantit,
 * c'est le **construire** — donc le supprimer d'ici casse le test, et le
 * réintroduire en dur dans `telechargerImage` le casserait aussi.
 *
 * Seule l'**origine** est envoyée : elle est calculée à partir de l'URL déjà
 * validée par `validerUrlImage` (https obligatoire, pas d'adresse privée), donc
 * aucun lien n'est divulgué hors du site qui sert l'image, et l'URL de l'image
 * elle-même — qui peut porter un jeton de session — ne part pas dans l'en-tête.
 *
 * Pourquoi : mesuré le 2026-10-13 sur rimuscan.fr, la même image répond 403
 * sans `Referer` et 200 (786 715 octets) avec. Beaucoup de sites refusent le vol
 * d'image à chaud ; le chemin **natif** (`fetch_octets`) en envoie déjà un,
 * le chemin scripté non — c'était une incohérence, pas une fonctionnalité.
 */
internal fun requeteImageScriptee(url: String, origineSite: String? = null): okhttp3.Request {
    // Une seule analyse, réutilisée : `Request.Builder().url()` refuse déjà
    // toute URL illisible, donc une branche « URL non analysable » ici serait
    // **inatteignable**. `toHttpUrl()` lève la même exception que `.url(url)`,
    // ce qui évite d'entretenir du code mort et son test.
    val cible = url.toHttpUrl()
    return okhttp3.Request.Builder()
        .url(cible)
        // L'**origine déclarée par la source** prime sur celle de l'image.
        // Mesuré le 2026-10-14 sur sushiscan.net : les pages sont servies par
        // `c1.sushiscan.net`, **un autre domaine**, et le CDN répond 403 +
        // page de vérification (4 572 octets) à un `Referer` qui vient de
        // lui-même. Avec `Referer: https://sushiscan.net/` : **200, 605 363
        // octets, image/jpeg**. Un `Referer` déduit de l'image revient donc à
        // se présenter comme un autre site qui la vole — ce qui est
        // précisément ce que le serveur cherche à écarter.
        //
        // L'origine de l'image reste le repli : c'est le bon cas quand le
        // script ne déclare rien, et le seul qui marche quand tout est servi
        // depuis un même domaine (mesuré le 2026-10-13 sur rimuscan.fr).
        //
        // `host` ne garde que l'hôte : le port est écarté, et surtout **jamais
        // le chemin ni la chaîne de requête**, qui peut porter un jeton.
        .header("Referer", site_a_referer(origineSite) ?: "${cible.scheme}://${cible.host}/")
        .get()
        .build()
}

/**
 * L'origine à annoncer dans le `Referer` : celle que le **script** déclare.
 *
 * `null` quand le script ne déclare rien d'exploitable, et l'appelant retombe
 * alors sur l'origine de l'image. Une `baseUrl` illisible est traitée comme
 * absente plutôt que comme une erreur : une source ne doit pas pouvoir
 * empêcher ses propres images de se télécharger.
 */
internal fun site_a_referer(origineSite: String?): String? {
    val brut = origineSite?.trim().orEmpty()
    if (brut.isEmpty()) return null
    val u = runCatching { brut.toHttpUrl() }.getOrNull() ?: return null
    // Ni chemin ni requête : une URL de source peut porter un jeton.
    return "${u.scheme}://${u.host}/"
}

/** Télécharge une image déjà validée et la rend en base64, comme le contrat l'attend. */
private fun telechargerImage(
    url: String,
    quoi: String,
    context: EngineContext,
    origineSite: String? = null,
): JSONObject =
    LIMITES_IMAGES.autourDe {
        clientPourImage(context).newCall(requeteImageScriptee(url, origineSite)).execute().use { resp ->
            val bytes = octetsImage(resp, quoi)
            JSONObject()
                .put("data", java.util.Base64.getEncoder().encodeToString(bytes))
                .put("contentType", resp.body?.contentType()?.toString() ?: "image/jpeg")
        }
    }

/**
 * Extrait les octets d'une réponse d'image, ou refuse.
 *
 * Extrait en fonction pour être épruvable **sans réseau** : `OkHttpClient` est
 * construit sur place dans `EngineContext` (un `val`, pas un injectable), donc
 * la seule façon de prouver le comportement sur un 403 est de fabriquer la
 * `Response` — ce qu'OkHttp permet sans socket.
 */
internal fun octetsImage(resp: okhttp3.Response, quoi: String): ByteArray {
    // Le statut d'abord. Un CDN bloqué renvoie 403/404/429, parfois avec un
    // corps (page de challenge HTML) : le servir comme une image produirait une
    // page illisible dans le `.cbz`. `HttpBridge` consultait déjà le statut
    // (HttpBridge.kt:60) ; ce chemin-ci ne le faisait pas.
    if (!resp.isSuccessful) {
        // **Le refus nomme sa cause.** `HTTP 404 pour cette page` est un
        // constat, pas une cause : il ne distingue pas un CDN qui refuse (et
        // dit pourquoi dans son corps — mesuré le 2026-10-09 sur
        // `*.mangadex.network` : « You must set an appropriate User-Agent
        // header ») d'une URL périmée. Ces deux cas appellent des remèdes
        // opposés, et confondus ils ont fait dévier deux diagnostics.
        throw IllegalStateException(messageDeRefus(resp, quoi))
    }
    val bytes = lireOctetsBornes(resp, quoi, MAX_OCTETS_IMAGE)
    // Un corps vide est un échec, pas une image de zéro octet. OkHttp rend un
    // tableau **vide** — pas `null` — pour un corps absent, donc le `?: throw`
    // ci-dessus ne le voyait jamais : le moteur répondait `ok: true` avec
    // `data: ""`, et Rust concluait « corps vide » sans que l'interface ne
    // puisse dire pourquoi (mesuré le 2026-10-09 sur custom:mangadex-api :
    // 17 pages sur 30 en image cassée, 145 « corps vide » au journal).
    if (bytes.isEmpty()) {
        throw IllegalStateException("empty $quoi")
    }
    return bytes
}

/**
 * Compose le message d'un refus d'image : statut, type de contenu, et le début
 * du corps rendu par le serveur.
 *
 * **Le corps est assaini.** Un CDN de refus répète l'URL demandée, et la
 * rédaction du journal (`journal.rs`) ne remplace que les jetons **commençant**
 * par `http` : une URL au milieu d'un texte passerait telle quelle. On retire
 * donc nous-mêmes toute URL et tout chemin absolu avant d'écrire.
 *
 * La longueur est bornée : une page d'erreur de plusieurs kilo-octets noierait
 * le journal, et c'est précisément le journal qu'on veut rester lisible.
 *
 * **Deux bornes, et elles ne font pas la même chose.** `peekBody` borne ce que
 * le réseau nous fait charger ; `take` borne ce que `nettoyerDiagnostic`
 * écrit. La seconde n'est donc pas redondante : neutraliser `peekBody` seul
 * laissait le test vert, ce qui est le comportement attendu puisque `take`
 * rattrape la longueur.
 */
private fun messageDeRefus(resp: okhttp3.Response, quoi: String): String {
    val ct = resp.body?.contentType()?.toString() ?: "inconnu"
    // `peekBody` ne consomme pas la réponse, et borne ce que le réseau nous
    // fait charger : une page d'erreur de plusieurs kilo-octets n'est jamais
    // lue en entier. La longueur **écrite** est, elle, bornée par `take` dans
    // `nettoyerDiagnostic` — les deux bornes protègent deux choses distinctes.
    val extrait = runCatching {
        resp.peekBody(DIAGNOSTIC_MAX).string()
    }.getOrDefault("")
    val corps = nettoyerDiagnostic(extrait)
    val detail = if (corps.isBlank()) "sans corps" else corps
    return "HTTP ${resp.code} pour $quoi [type=$ct] $detail"
}

/** Longueur max du corps repris dans un refus : assez pour une cause, pas pour noyer le journal. */
private const val DIAGNOSTIC_MAX = 200L

/**
 * Rend un corps d'erreur lisible et sans fuite : une seule ligne, bornée, et
 * sans URL ni chemin de fichier.
 *
 * Testé par `DiagnosticImageTest` : c'est cette fonction qui empêche le corps
 * d'un CDN de faire entrer l'URL d'une image dans le journal.
 */
internal fun nettoyerDiagnostic(corps: String): String =
    corps
        // Tout ce qui ressemble à une URL absolute, avant tout le reste : sans
        // cela, les substitutions suivantes casseraient la détection.
        .replace(URL_DANS_UNE_CHAINE, "<url>")
        // Un chemin de fichier (`/data/abc/1.png`, `C:\…`) trahit la structure
        // du site et n'apporte rien au diagnostic.
        .replace(CHEMIN_ABSOLU, "<chemin>")
        .replace(ASCII_AUX_IMPRIMABLES, " ")
        .replace(ESPACES_REPETES, " ")
        .trim()
        .take(DIAGNOSTIC_MAX.toInt())
        .trimEnd()

private val URL_DANS_UNE_CHAINE = Regex("""\b[a-zA-Z][a-zA-Z0-9+.-]*://\S+""")
private val CHEMIN_ABSOLU = Regex("""(^|\s)/[^\s]*""")
private val ASCII_AUX_IMPRIMABLES = Regex("""[^\p{Print}\p{Space}]""")
private val ESPACES_REPETES = Regex("""\s+""")

/**
 * Dispatch original pour les sources natives kotatsu-parsers-redo.
 * Correspond au `when (method)` d'origine, sans les branches "search"/"latest"/etc.
 * qui sont maintenant routées en amont.
 */
private suspend fun dispatchNatif(
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
                contentRating = requestedRatings(p, options, sourceName),
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
                contentRating = requestedRatings(p, parser.getFilterOptions(), sourceName),
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
        // Dans `avec` pour la même raison que `fetchImage` : le parser fournit ses
        // en-têtes (certains sites refusent une couverture sans Referer) et
        // débrouille la réponse.
        val (bytes, contentType) = pool.avec(parseSource(sourceName)) {
            fetch_octets(context, coverUrl, referer)
        }
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
        // **Le téléchargement doit rester dans `avec`.** C'est là que le parser
        // est posé, et donc là que l'intercepteur peut débrouille la réponse.
        // Mesuré le 2026-10-05 : en sortant de `avec`, ZENMANGA renvoyait 832 513
        // octets d'entête `AA A1 AC 8B` — illisibles, `naturalWidth` = 0 à
        // l'écran — alors que le même appel dans `avec` donnait `FF D8 FF E0`.
        // Le test du moteur passait parce qu'il enveloppait tout le parcours ;
        // l'application, elle, ne l'enveloppait qu'autour de `getPageUrl`.
        val (bytes, contentType) = pool.avec(parseSource(sourceName)) {
            fetch_octets(context, imageUrl, imageUrl)
        }
        JSONObject()
            .put("data", Base64.getEncoder().encodeToString(bytes))
            .put("contentType", contentType)
    }

    "injecter" -> {
        // **Transport webview, moitié moteur.** Le HTML capturé par la fenêtre
        // de vérification entre ici ; `WebViewMemoire` le sert à la requête
        // suivante sur la même URL, avant tout réseau (spike du 2026-10-13 :
        // le WebView2 obtient 200 où OkHttp obtient 403, même clearance).
        //
        // Aucune validation de source : ce qui est injecté est du HTML, et le
        // valider supposerait de connaître la source de chaque page. La
        // liste blanche d'hôtes est faite côté Rust, avant l'appel — c'est là
        // qu'est la confiance, une seule fois.
        val url = p.reqString("url")
        val html = p.reqString("html")
        WebViewMemoire.injecter(url, html)
        if (challengeDebug) {
            // La longueur, jamais le contenu : une page de source n'est pas un
            // secret, mais rien ne justifie de recopier 125 Ko dans le journal.
            System.err.println(
                "[challenge-debug] webview html injecte url=${WebViewMemoire.cle(url)} " +
                    "octets=${html.length} totalMemoire=${WebViewMemoire.taille()}",
            )
        }
        JSONObject()
            .put("ok", true)
            .put("octets", html.length)
            .put("memoire", WebViewMemoire.taille())
    }

    "setCookies" -> appliquerCookies(context, p)

    // `health` ne rend que la forme globale. La forme par source
    // (`source` + `reachable`) a été retirée le 2026-10-10 : elle répondait
    // `reachable: true` après une simple validation d'enum — aucun appel
    // réseau, donc un champ qui affirmait une disponibilité sans la mesurer.
    // La méthode n'a par ailleurs aucun appelant dans l'application (aucune
    // commande du cœur ne l'expose, l'écran « état des sources » lit les
    // erreurs réelles). Un `source` reçu en paramètre est désormais ignoré au
    // lieu d'être validé pour une réponse qui ne mesurait rien.
    "health" -> JSONObject().put("ok", true).put("engineVersion", ENGINE_VERSION)

    "shutdown" -> JSONObject().put("ok", true)

    else -> throw ParamException("unknown method: $method")
}
