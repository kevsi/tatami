package org.tatami.engine

import okhttp3.Call
import okhttp3.Connection
import okhttp3.Cookie
import okhttp3.CookieJar
import okhttp3.HttpUrl
import okhttp3.Interceptor
import okhttp3.MediaType.Companion.toMediaTypeOrNull
import okhttp3.Protocol
import okhttp3.Request
import okhttp3.Response
import okhttp3.ResponseBody.Companion.toResponseBody
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertThrows
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Test
import java.util.concurrent.TimeUnit

/**
 * Le classement d'un **rejeu refusé**.
 *
 * ## Le fait mesuré
 *
 * Le 2026-10-13, sur TOONFR, dans `tatami.log` de l'application réelle :
 *
 * ```
 * challenge: 1 cookie(s) applique(s) automatiquement source=TOONFR
 * stdout erreur id=latest code=INTERNAL
 *   msg=HttpStatusException: . Status=403, URL=[https://toonfr.com/webtoon/]
 * ```
 *
 * Avant le cookie, la même source renvoyait `CHALLENGE_REQUIRED` et la fenêtre
 * de vérification s'ouvrait. Après application du cookie, le site répond 403,
 * `CloudFlareHelper.checkResponseForProtection` répond
 * `PROTECTION_NOT_DETECTED`, l'interception laisse passer, et le parseur
 * remonte une `HttpStatusException` qui devient `INTERNAL`.
 *
 * Ce que l'utilisateur lisait : « 1 cookie transmis », puis la source bloquée,
 * sans message exploitable. Et surtout : `CHALLENGE_UNREPLAYABLE` existe dans
 * le contrat, le moteur sait le produire, et l'interface a le message traduit
 * `challenge.non.rejouable` — **rien de tout cela ne se déclenchait jamais**.
 *
 * Le classement corrigé ne lit aucun marqueur Cloudflare. Il se base sur un fait
 * que nous avons sous la main : **la requête portait déjà une `cf_clearance`**,
 * donc la vérification a été faite, et le site refuse malgré tout.
 *
 * ## Méthode
 *
 * `CloudFlareHelper` inspecte une `Response` réelle, et OkHttp permet d'en
 * fabriquer une sans socket (`Response.Builder()`). `Interceptor.Chain` est une
 * interface de dix méthodes dont la signature a été relue dans le JAR livré
 * (`okhttp-jvm-5.1.0.jar`, `javap`) et non devinée.
 *
 * Chaque test utilise un **hôte distinct** : `RejeuRefuse` est une mémoire
 * globale, et deux tests sur le même hôte se parleraient.
 */
class CloudFlareRejeuRefuseTest {

    /** Magasin minimal : ne rend une `cf_clearance` que si `nom` n'est pas nul. */
    private class Bocal(private val nom: String?, private val domaine: String) : CookieJar {
        override fun saveFromResponse(url: HttpUrl, cookies: List<Cookie>) = Unit
        override fun loadForRequest(url: HttpUrl): List<Cookie> =
            if (nom == null) {
                emptyList()
            } else {
                listOf(
                    Cookie.Builder().name("cf_clearance").value("v1").domain(domaine).build(),
                )
            }
    }

    /** Chaîne qui rend une réponse fabriquée et n'ouvre aucun socket. */
    private class Chaine(private val req: Request, private val res: Response) : Interceptor.Chain {
        override fun request(): Request = req
        override fun proceed(request: Request): Response = res
        override fun connection(): Connection? = null
        override fun call(): Call = throw UnsupportedOperationException("aucun appel dans un test")
        override fun connectTimeoutMillis(): Int = 10_000
        override fun withConnectTimeout(timeout: Int, unit: TimeUnit): Interceptor.Chain = this
        override fun readTimeoutMillis(): Int = 10_000
        override fun withReadTimeout(timeout: Int, unit: TimeUnit): Interceptor.Chain = this
        override fun writeTimeoutMillis(): Int = 10_000
        override fun withWriteTimeout(timeout: Int, unit: TimeUnit): Interceptor.Chain = this
    }

    private fun reponse(url: String, code: Int, corps: String): Response =
        Response.Builder()
            .request(Request.Builder().url(url).build())
            .protocol(Protocol.HTTP_1_1)
            .code(code)
            .message("fabrique")
            .body(corps.toResponseBody("text/html".toMediaTypeOrNull()))
            .build()

    private fun intercepte(
        intercepteur: CloudFlareInterceptor,
        url: String,
        reponse: Response,
    ): Response {
        val req = Request.Builder().url(url).build()
        return intercepteur.intercept(Chaine(req, reponse))
    }

    @Test
    fun `un 403 portant une clearance devient un rejeu refuse`() {
        // Le cas mesuré sur TOONFR, hôte renommé pour rester isolé.
        val url = "https://rejeu-refuse.test/webtoon/"
        val i = CloudFlareInterceptor(Bocal("cf_clearance", "rejeu-refuse.test"))
        val e = assertThrows(CloudFlareUnreplayableException::class.java) {
            intercepte(i, url, reponse(url, 403, "<html>Forbidden</html>"))
        }
        assertEquals(url, e.url)
    }

    @Test
    fun `un 503 portant une clearance devient aussi un rejeu refuse`() {
        val url = "https://indisponible-refuse.test/webtoon/"
        val i = CloudFlareInterceptor(Bocal("cf_clearance", "indisponible-refuse.test"))
        assertThrows(CloudFlareUnreplayableException::class.java) {
            intercepte(i, url, reponse(url, 503, "<html>Service Unavailable</html>"))
        }
    }

    @Test
    fun `un 403 SANS clearance reste une reponse ordinaire`() {
        // Contrôle **positif du voisinage** : sans lui, une régression qui
        // refuserait tous les 403 passerait le test précédent. Un site peut
        // répondre 403 pour une raison banale — un filtre d'âge, un Referent.
        val url = "https://banal-refuse.test/manga/"
        val i = CloudFlareInterceptor(Bocal(null, "banal-refuse.test"))
        val rendue = intercepte(i, url, reponse(url, 403, "Forbidden"))
        assertEquals(403, rendue.code)
    }

    @Test
    fun `une reponse 200 portant une clearance traverse normalement`() {
        // Le cas nominal. Sans lui, le premier test pourrait passer pour une
        // régression qui refuse tout ce qui suit une clearance.
        val url = "https://nominal-refuse.test/webtoon/"
        val i = CloudFlareInterceptor(Bocal("cf_clearance", "nominal-refuse.test"))
        val rendue = intercepte(i, url, reponse(url, 200, "<html>ok</html>"))
        assertEquals(200, rendue.code)
        assertTrue(rendue.isSuccessful)
    }

    @Test
    fun `un 404 portant une clearance reste ordinaire`() {
        // Le refus ne doit viser que 403 et 503 : un 404 est une page absente,
        // pas une vérification refusée.
        val url = "https://absent-refuse.test/series/absent"
        val i = CloudFlareInterceptor(Bocal("cf_clearance", "absent-refuse.test"))
        assertEquals(404, intercepte(i, url, reponse(url, 404, "nope")).code)
    }

    @Test
    fun `un 429 portant une clearance reste ordinaire`() {
        // Un 429 est une limitation de débit : elle a déjà son code, et le
        // transformer en « rejeu refusé » ferait perdre le message juste.
        val url = "https://lent-refuse.test/webtoon/"
        val i = CloudFlareInterceptor(Bocal("cf_clearance", "lent-refuse.test"))
        assertEquals(429, intercepte(i, url, reponse(url, 429, "slow down")).code)
    }
}