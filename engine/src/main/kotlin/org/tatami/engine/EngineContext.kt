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
    val parser = ParserActif.lire()
    if (parser != null) parser.intercept(chain) else chain.proceed(chain.request())
}

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
            chain.proceed(
                chain.request().newBuilder()
                    .header("User-Agent", getDefaultUserAgent())
                    .build(),
            )
        }
        // **Dernier intercepteur ajouté = le plus proche du réseau.** Il doit l'être
        // parce que c'est lui qui débrouille la réponse : ZENMANGA chiffre ses
        // images et son `ImageCodec` (SEC / XOR) n'est décodé que là. Ajouté plus
        // haut, CloudFlare et le lazy-load verraient les octets déjà transformés.
        .addInterceptor(intercepteurParser)
        .connectTimeout(20, TimeUnit.SECONDS)
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
