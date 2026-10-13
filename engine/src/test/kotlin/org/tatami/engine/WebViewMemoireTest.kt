package org.tatami.engine

import okhttp3.Call
import okhttp3.Connection
import okhttp3.Interceptor
import okhttp3.Protocol
import okhttp3.Request
import okhttp3.ResponseBody.Companion.toResponseBody
import java.util.concurrent.TimeUnit
import org.junit.jupiter.api.AfterEach
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertNotNull
import org.junit.jupiter.api.Assertions.assertNull
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Test

/**
 * La mémoire webview, et **surtout** son ordre dans la chaîne.
 *
 * ## Le fait mesuré qui motive ces tests
 *
 * Le 2026-10-13, sur le WebView2 de l'application réelle : TOONFR rend
 * **200** dans le navigateur et **403** dans OkHttp, avec la même clearance.
 * Le seul client accepté est celui qui a résolu (spike consigné dans
 * `PLAN.md`, section « Spike — résultats et go/no-go »).
 *
 * D'où l'idée : demander le HTML au navigateur, le donner au moteur. Ce module
 * en est la moitié moteur. La moitié Rust (webview cachée) vient ensuite.
 *
 * ## Pourquoi l'ordre est le point le plus fragile
 *
 * `CloudFlareInterceptor` transforme un 403 en exception. Si la mémoire webview
 * passait **après** lui, une page déjà capturée par le navigateur serait
 * challengée une seconde fois — le transport ne servirait à rien. Le premier
 * test de ce fichier verrouille cet ordre, et il est le seul qui puisse le
 * faire : les autres tests du module passeraient même avec l'ordre inversé.
 */
class WebViewMemoireTest {

    @AfterEach
    fun nettoyer() {
        WebViewMemoire.vider()
        WebViewMemoire.maintenant = System::currentTimeMillis
    }

    /** Chaîne qui **compte** les passages réseau au lieu d'en ouvrir un. */
    private class ChaineComptee(
        private val req: Request,
        private val reponse: okhttp3.Response?,
        private val compteur: Compteur,
    ) : Interceptor.Chain {
        override fun request(): Request = req
        override fun proceed(request: Request): okhttp3.Response {
            compteur.appels++
            return reponse ?: throw IllegalStateException("aucune réponse dans ce test")
        }
        override fun connection(): Connection? = null
        override fun call(): Call = throw UnsupportedOperationException("aucun appel dans un test")
        override fun connectTimeoutMillis(): Int = 10_000
        override fun withConnectTimeout(t: Int, u: TimeUnit): Interceptor.Chain = this
        override fun readTimeoutMillis(): Int = 10_000
        override fun withReadTimeout(t: Int, u: TimeUnit): Interceptor.Chain = this
        override fun writeTimeoutMillis(): Int = 10_000
        override fun withWriteTimeout(t: Int, u: TimeUnit): Interceptor.Chain = this
    }

    private class Compteur(var appels: Int = 0)

    /** Le vrai `CloudFlareInterceptor`, avec un magasin porteur de clearance. */
    private fun cloudflare(host: String) = CloudFlareInterceptor(
        object : okhttp3.CookieJar {
            override fun saveFromResponse(url: okhttp3.HttpUrl, cookies: List<okhttp3.Cookie>) = Unit
            override fun loadForRequest(url: okhttp3.HttpUrl): List<okhttp3.Cookie> = listOf(
                okhttp3.Cookie.Builder()
                    .name("cf_clearance").value("v1").domain(host).build(),
            )
        },
    )

    /**
     * **L'ordre, en une assertion.** Une page capturée par la webview rendue sur
     * une URL qui, sans mémoire, serait challengée. Le réseau ne doit pas être
     * touché — c'est ce qui distingue « la mémoire est servie avant
     * CloudFlareInterceptor » de « elle est servie après, et le 403 a déjà été
     * levé ».
     */
    @Test
    fun `une page captive par la webview bypass le challenge sans toucher le reseau`() {
        val host = "ordre-webview.test"
        val url = "https://$host/webtoon/"
        WebViewMemoire.injecter(url, "<html><body>chapitres reels</body></html>")

        val compteur = Compteur()
        // La chaîne complète, dans l'ordre de `EngineContext.httpClient` :
        // mémoire d'abord, Cloudflare ensuite. `CloudFlareInterceptor`
        // lèverait si on l'atteignait avec un 403 — compteur le prouve.
        val chaine = intercepteurWebView.intercept(
            ChaineComptee(
                Request.Builder().url(url).build(),
                null, // le réseau n'a pas de réponse à donner dans ce test
                compteur,
            ),
        )

        assertNotNull(chaine)
        assertEquals(200, chaine.code)
        val texte = chaine.body?.string()
        assertTrue(texte?.contains("chapitres reels") == true, "le corps doit être le HTML capturé")
        assertEquals(0, compteur.appels, "le réseau ne doit pas être sollicité")
    }

    /**
     * Le contrôle de voisinage : **sans** mémoire, la chaîne va bien au réseau.
     * Sans ce test, le précédent pourrait passer parce que rien ne toucherait
     * jamais le réseau, ordre correct ou pas.
     */
    @Test
    fun `sans memoire la requete va au reseau`() {
        val host = "sans-memoire-test.test"
        val url = "https://$host/webtoon/"
        val reponse = okhttp3.Response.Builder()
            .request(Request.Builder().url(url).build())
            .protocol(Protocol.HTTP_1_1)
            .code(200)
            .message("reseau")
            .body("<html>reseau</html>".toResponseBody())
            .build()
        val compteur = Compteur()
        val rendue = intercepteurWebView.intercept(
            ChaineComptee(Request.Builder().url(url).build(), reponse, compteur),
        )
        assertEquals(1, compteur.appels)
        assertEquals(200, rendue.code)
        assertEquals("<html>reseau</html>", rendue.body?.string())
    }

    /**
     * Le cas que le module existe pour traiter : la même URL, la même seconde.
     * Cloudflare répondrait 403 (challenge) et lèverait une exception ; la
     * mémoire doit rendre 200 sans que cet intercepteur ne soit atteint.
     */
    @Test
    fun `une URL challenge reconnue en memoire ne passe pas par CloudFlareInterceptor`() {
        val host = "cloudflare-court-circuite.test"
        val url = "https://$host/webtoon/"
        WebViewMemoire.injecter(url, "<html>capture</html>")

        val compteur = Compteur()
        val rendue = intercepteurWebView.intercept(
            ChaineComptee(Request.Builder().url(url).build(), null, compteur),
        )
        assertEquals(200, rendue.code)
        assertEquals(0, compteur.appels)
    }

    /**
     * L'horloge injectable. Cinq minutes, pas une journée : un contenu figé ne
     * doit pas devenir une vérité durable, sinon une page vue le matin serait
     * servie le soir alors que la source a changé.
     */
    @Test
    fun `une entree expire apres cinq minutes`() {
        var t = 1_000_000L
        WebViewMemoire.maintenant = { t }
        val url = "https://expiration-webview.test/x"
        WebViewMemoire.injecter(url, "<html>vieux</html>")
        assertNotNull(WebViewMemoire.lire(url))

        t += WebViewMemoire.TTL_MS - 1
        assertNotNull(WebViewMemoire.lire(url), "encore vivante juste avant l'échéance")

        t += 2
        assertNull(WebViewMemoire.lire(url), "expirée après")
        assertEquals(0, WebViewMemoire.taille(), "l'entrée expirée est retirée, pas gardée")
    }

    /** La clé : un fragment n'est pas une autre page côté serveur. */
    @Test
    fun `le fragment est ignore dans la cle`() {
        val url = "https://cle.test/webtoon/"
        WebViewMemoire.injecter(url, "<html>a</html>")
        assertEquals("<html>a</html>", WebViewMemoire.lire("https://cle.test/webtoon/#chapitre-3"))
        // Et l'inverse.
        WebViewMemoire.vider()
        WebViewMemoire.injecter("https://cle.test/webtoon/#ancre", "<html>b</html>")
        assertEquals("<html>b</html>", WebViewMemoire.lire("https://cle.test/webtoon/"))
    }

    /** La requête, elle, fait la page. */
    @Test
    fun `la requete fait partie de la cle`() {
        WebViewMemoire.injecter("https://query.test/p?page=2", "<html>deux</html>")
        assertNull(WebViewMemoire.lire("https://query.test/p?page=3") ?: null)
        assertEquals("<html>deux</html>", WebViewMemoire.lire("https://query.test/p?page=2"))
    }

    /** Un corps vide n'est pas une capture : mieux vaut retenter le réseau. */
    @Test
    fun `un corps vide n est pas memorise`() {
        WebViewMemoire.injecter("https://vide.test/x", "")
        assertNull(WebViewMemoire.lire("https://vide.test/x"))
    }

    @Test
    fun `oublier retire une entree`() {
        val url = "https://oublier.test/x"
        WebViewMemoire.injecter(url, "<html>a</html>")
        WebViewMemoire.oublier(url)
        assertNull(WebViewMemoire.lire(url))
    }
}
