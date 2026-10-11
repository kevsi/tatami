package org.tatami.engine

import java.awt.image.BufferedImage
import java.util.concurrent.TimeUnit
import javax.imageio.ImageIO
import javax.script.ScriptEngineManager
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.runInterruptible
import okhttp3.CookieJar
import okhttp3.Interceptor
import okhttp3.MediaType.Companion.toMediaTypeOrNull
import okhttp3.OkHttpClient
import okhttp3.Response
import okhttp3.ResponseBody.Companion.toResponseBody
import org.koitharu.kotatsu.parsers.MangaLoaderContext
import org.koitharu.kotatsu.parsers.bitmap.Bitmap
import org.koitharu.kotatsu.parsers.config.MangaSourceConfig
import org.koitharu.kotatsu.parsers.model.MangaSource
import org.koitharu.kotatsu.parsers.network.UserAgents

/**
 * Delegue la reponse au parser de la requete en cours, s'il y en a un.
 *
 * C'est ce qui rend ZENMANGA lisible : le parser dechiffre les images. Sans
 * delegation, les octets servis sont illisibles (mesure du 2026-10-05 : 832 513
 * octets d'entete `AA A1 AC 8B` au lieu de `FF D8 FF E0`).
 *
 * Hors d'un `ParserPool.avec`, il n'y a aucun parser : on laisse alors passer la
 * requete telle quelle. Un `ThreadLocal` non propage ne doit pas casser le
 * moteur, seulement priver la source de son traitement.
 */
internal val intercepteurParser = Interceptor { chain ->
    // `pourRequete`, pas `lire()` : sur un appel asynchrone d'OkHttp,
    // l'intercepteur tourne sur un autre fil que celui qui a activé le
    // parseur, et le `ThreadLocal` y est vide.
    val parser = ParserActif.pourRequete(chain.request())
    if (parser != null) parser.intercept(chain) else chain.proceed(chain.request())
}

/**
 * Instrumentation du flux challenge, activée par `TATAMI_CHALLENGE_DEBUG`
 * dans l'environnement du moteur.
 *
 * Journalisation sur **stderr** : stdout est le protocole (une ligne JSON
 * par réponse, cf. `Server.kt`).
 *
 * Ce qui est noté, et pourquoi — chaque ligne répond à une question posée
 * par la mesure du 2026-10-10 sur TOONFR :
 * - l'**UA d'OkHttp** réellement envoyé, à comparer à celui de la webview
 *   (une `cf_clearance` est liée à l'agent qui l'a obtenue) ;
 * - les **noms** des cookies que le magasin va joindre à la requête
 *   (`BridgeInterceptor` les pose après tous les intercepteurs
 *   d'application : lire l'en-tête `Cookie` ici ne verrait rien) — c'est la
 *   présence de `cf_clearance` dans la requête **suivante** ;
 * - le **statut** de la réponse et les en-têtes `cf-mitigated`, `server`,
 *   `cf-ray` — ce qui distingue « cookie refusé » d'« empreinte TLS
 *   soupçonnée ».
 *
 * Aucune **valeur** de cookie n'est écrite : les noms seulement.
 */
internal val challengeDebug: Boolean = System.getenv("TATAMI_CHALLENGE_DEBUG") != null

// Adapté de kotatsu-dl MangaLoaderContextImpl (amont d'origine) aux signatures
// du fork redo : evaluateJs à 3 args, mêmes timeouts, même Nashorn.
class EngineContext : MangaLoaderContext() {

    override val cookieJar: CookieJar = InMemoryCookieJar()

    // Remplacé par setCookies (contrat v0) après résolution d'un challenge.
    @Volatile
    var userAgentOverride: String? = null

    private val scriptEngineManager = ScriptEngineManager()

    

    override val httpClient: OkHttpClient = OkHttpClient.Builder()
        .cookieJar(cookieJar)
        .addInterceptor(RateLimitInterceptor())
        .addInterceptor(CloudFlareInterceptor())
        .addInterceptor(LazyLoadInterceptor())
        .addInterceptor { chain ->
            val requete = chain.request()
            // **En-tetes de navigateur, SANS les client hints.**
            //
            // Source : `Kotatsu-Redo/Kotatsu-Redo`,
            // `browser/cloudflare/CloudFlareInterceptClient.kt`. Kotatsu rejoue
            // les requetes de la WebView avec OkHttp en recopiant ses en-tetes
            // reelles, moins une liste de blocage explicite :
            // `sec-ch-ua`, `sec-ch-ua-full-version-list`, `x-requested-with`.
            // Autrement dit : le jeu d'en-tetes qui passe **retire** les
            // client hints. Notre hypothese d'hier (les ajouter) etait donc
            // a l'envers.
            //
            // Mesure : seul le User-Agent ne suffisait pas, et ajouter un
            // Referer isole non plus (4 variantes, toutes refusees).
            val entetes = requete.newBuilder()

            // **Les en-tetes que le parseur declare.** On ne les invente pas :
            // chaque source peut dire ce que son site attend, et
            // `MangaParser.getRequestHeaders()` est l'API prevue pour cela.
            //
            // Source : `Kotatsu-Redo/Kotatsu-Redo`,
            // `core/network/CommonHeadersInterceptor.kt`, qui fait exactement
            // `repository?.getRequestHeaders()?.let { mergeWith(it,
            // replaceExisting = false) }`. **Notre moteur ne le faisait
            // nulle part** (`grep getRequestHeaders` : 0 resultat), alors que
            // le depot expose l'API sur `MangaParser` et sur
            // `AbstractMangaParser`.
            //
            // `replaceExisting = false` : ce que le parseur a mis l'emporte
            // sur nos valeurs par defaut, mais jamais sur un en-tete pose
            // explicitement par le parseur lui-meme plus haut dans la chaine.
            val parser = ParserActif.pourRequete(requete) as? org.koitharu.kotatsu.parsers.MangaParser
            val declares = parser?.let { runCatching { it.getRequestHeaders() }.getOrNull() }
            if (challengeDebug) {
                // Noms seulement, jamais les valeurs : un en-tete peut porter un
                // secret. Le User-Agent est le seul qu'on nomme en clair, et il
                // n'en est pas un.
                val noms = declares?.let { h ->
                    (0 until h.size).joinToString(",") { h.name(it) }
                } ?: "-"
                System.err.println(
                    "[challenge-debug] intercepteur parseur=${parser?.javaClass?.simpleName} " +
                        "tag=${requete.tag(org.koitharu.kotatsu.parsers.model.MangaSource::class.java)} " +
                        "enTetesDeclares=${declares?.size ?: -1} [$noms] " +
                        "uaDeclare=${declares?.get("User-Agent") ?: "-"} " +
                        "uaEnvoye=${requete.header("User-Agent")}"
                )
            }
            if (declares != null) {
                for (i in 0 until declares.size) {
                    val nom = declares.name(i)
                    // `replaceExisting = false`, comme Kotatsu : ce que la
                    // requête porte **déjà** gagne. Sans cette garde,
                    // `header()` remplaçait aussi les en-têtes que le parseur
                    // avait posés lui-même sur cette requête précise.
                    if (requete.header(nom) == null) entetes.header(nom, declares.value(i))
                }
            }

            // **On interroge le constructeur, pas la requete d'origine.**
            // Interroger `requete` testait l'etat *avant* la fusion des
            // en-tetes du parseur : le parseur posait son User-Agent, et la
            // ligne suivante l'ecrasait avec le nôtre. Mesure du 2026-10-11 :
            // l'UA envoyee restait `FIREFOX_DESKTOP` alors que
            // `AbstractMangaParser.getRequestHeaders()` en declare un.
            val demande = entetes.build()

            // **L'UA impose par le challenge l'emporte sur celui du parseur.**
            // C'est l'ordre de `Kotatsu-Redo`, et le commentaire y dit pourquoi :
            // « Overrides the parser's agent, deliberately : a cf_clearance
            // cookie is only honoured for the exact User-Agent it was issued
            // to ». Sans cette ligne, un parseur qui declare un UA (HENTAIZONE
            // declare `Firefox/116`) annulait silencieusement
            // `userAgentOverride` — mesure du 2026-10-11 : on demandait
            // `Edg/154`, c'est `Firefox/116` qui partait.
            val impose = userAgentOverride
            if (impose != null) {
                entetes.header("User-Agent", impose)
            } else if (demande.header("User-Agent") == null) {
                entetes.header("User-Agent", getDefaultUserAgent())
            }
            if (demande.header("Accept") == null) {
                entetes.header("Accept", "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8")
            }
            if (demande.header("Accept-Language") == null) {
                entetes.header("Accept-Language", "en-US,en;q=0.9")
            }
            if (demande.header("Referer") == null) {
                entetes.header("Referer", "${requete.url.scheme}://${requete.url.host}/")
            }
            if (demande.header("Sec-Fetch-Dest") == null) {
                entetes.header("Sec-Fetch-Dest", "document")
                entetes.header("Sec-Fetch-Mode", "navigate")
                entetes.header("Sec-Fetch-Site", "same-origin")
                entetes.header("Sec-Fetch-User", "?1")
                entetes.header("Upgrade-Insecure-Requests", "1")
            }
            chain.proceed(entetes.build())
        }
        // Instrumentation `TATAMI_CHALLENGE_DEBUG`. Placé **après** l'UA (il
        // la lit) et **avant** l'intercepteur de parser : il voit donc
        // l'en-tête envoyé et toutes les réponses, y compris le 403/503 que
        // `CloudFlareInterceptor` transforme en exception — la réponse
        // remonte à travers lui avant que l'exception ne soit levée.
        .addInterceptor { chain ->
            val requete = chain.request()
            val reponse = chain.proceed(requete)
            if (challengeDebug) {
                val cookies = cookieJar.loadForRequest(requete.url).map { it.name }.sorted()
                System.err.println(
                    "[challenge-debug] requete hote=${requete.url.host} " +
                        "ua=${requete.header("User-Agent")} cookiesJar=$cookies " +
                        "statut=${reponse.code} " +
                        "cf-mitigated=${reponse.header("cf-mitigated")} " +
                        "server=${reponse.header("server")} " +
                        "cf-ray=${reponse.header("cf-ray")}",
                )
            }
            reponse
        }
        // **Dernier intercepteur ajouté = le plus proche du réseau.** Il doit l'être
        // parce que c'est lui qui débrouille la réponse : ZENMANGA chiffre ses
        // images et son `ImageCodec` (SEC / XOR) n'est décodé que là. Ajouté plus
        // haut, CloudFlare et le lazy-load verraient les octets déjà transformés.
        .addInterceptor(intercepteurParser)        .connectTimeout(20, TimeUnit.SECONDS)
        .readTimeout(60, TimeUnit.SECONDS)
        .writeTimeout(20, TimeUnit.SECONDS)
        .build()

    /**
     * Client dédié au téléchargement d'une image, **sans redirections**.
     *
     * Le défaut mesuré le 2026-10-09 (audit) : `validerUrlImage`
     * (`Server.kt:619-653`) contrôle le schéma et l'hôte de l'URL **reçue**, mais
     * le client utilisé ensuite ne fixait ni `followRedirects` ni
     * `followSslRedirects` — donc gardait les défauts d'OkHttp, qui suivent une
     * redirection sans réexaminer l'hôte.
     *
     * Scénario : un script `.js` — donc du code que le moteur exécute — renvoie
     * `https://cdn.exemple.test/p/1.jpg`, ce serveur répond
     * `302 Location: http://127.0.0.1:8080/…`, et les octets d'une interface
     * d'administration locale reviennent en base64 pour finir dans un `.cbz`.
     * C'est exactement ce que `FetchImageReseauLocalTest` prétend protéger, et ce
     * que la règle d'ADR 0010 annonce.
     *
     * Décision (validée le 2026-10-09) : **désactiver les redirections** plutôt
     * que les réexaminer. Un intercepteur qui rejouerait `validerUrlImage` à
     * chaque saut serait plus fidèle au comportement des sites, mais il ajoute du
     * code — donc de la surface — pour un bénéfice qui ne se voit que sur les
     * sources qui bougent beaucoup. Sans redirection, l'hôte validé **est** celui
     * qui est joint : la garantie devient simple à énoncer et à vérifier.
     *
     * Coût assumé : une source dont les images passent par une redirection ne
     * chargera plus ses pages. Ça se verra à l'écran, et ça se mesure.
     *
     * Pourquoi un second client et pas le même avec un réglage en plus : ce
     * client porte le magasin de cookies, les intercepteurs de déchiffrement et
     * l'UA. `httpClient` sert aussi le HTML des parsers, où une redirection vers
     * une autre page du même site est normale. Partager le réglage casserait ces
     * chemins pour une garantie qui ne les concerne pas.
     */
    val clientImage: OkHttpClient = httpClient.newBuilder()
        .followRedirects(false)
        .followSslRedirects(false)
        .build()

    @Suppress("OVERRIDE_DEPRECATION")
    override suspend fun evaluateJs(script: String): String? = evaluateJs("", script, 0L)

    // Nashorn (org.openjdk.nashorn:nashorn-core) : le JDK 21 ne fournit plus de moteur JS.
    // baseUrl/timeout sont ignorés, comme chez kotatsu-dl.
    override suspend fun evaluateJs(baseUrl: String, script: String, timeout: Long): String? =
        runInterruptible(Dispatchers.Default) {
            val nashorn = scriptEngineManager.getEngineByName("nashorn")
                ?: error("JavaScript engine is not available")
            nashorn.eval(script)?.toString()?.takeUnless { it.isEmpty() || it == "null" }
        }

    override fun getConfig(source: MangaSource): MangaSourceConfig = DefaultMangaSourceConfig()

    override fun getDefaultUserAgent(): String = userAgentOverride ?: UserAgents.FIREFOX_DESKTOP

    override fun redrawImageResponse(response: Response, redraw: (Bitmap) -> Bitmap): Response {
        val body = checkNotNull(response.body) { "Empty response body" }
        val srcImage = body.byteStream().use { ImageIO.read(it) }
        checkNotNull(srcImage) { "Cannot decode image" }
        val resImage = (redraw(BitmapImpl(srcImage)) as BitmapImpl)
        return response.newBuilder()
            .body(resImage.compress("png").toResponseBody("image/png".toMediaTypeOrNull()))
            .build()
    }

    override fun createBitmap(width: Int, height: Int): Bitmap {
        return BitmapImpl(BufferedImage(width, height, BufferedImage.TYPE_INT_RGB))
    }
}
