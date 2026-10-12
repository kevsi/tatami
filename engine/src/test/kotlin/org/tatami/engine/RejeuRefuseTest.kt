package org.tatami.engine

import okhttp3.Cookie
import okhttp3.Dns
import okhttp3.HttpUrl
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.mockwebserver.MockResponse
import okhttp3.mockwebserver.MockWebServer
import java.net.InetAddress
import java.util.concurrent.TimeUnit
import kotlin.test.AfterTest
import kotlin.test.BeforeTest
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertNotNull
import kotlin.test.assertTrue

/**
 * Le cycle « bandeau → puzzle → fenêtre qui se ferme → rien » doit s'arrêter.
 *
 * ## Ce qui est mesuré, et comment
 *
 * Le serveur simulé répond `403` **avec** le marqueur Cloudflare que
 * `CloudFlareHelper` reconnaît. Le marqueur n'est pas deviné : il est lu dans le
 * bytecode de la dépendance (`javap`), qui teste
 * `h2[data-translate="blocked_why_headline"]`, puis
 * `#challenge-error-title` / `#challenge-error-text`. Un 403 nu ne déclencherait
 * rien, et le test passerait pour une raison absente du code.
 *
 * Le test couvre les **deux** moments :
 *  1. le premier 403, sans clearance : c'est une demande de vérification
 *     ordinaire — la fenêtre doit pouvoir s'ouvrir (`CloudFlareProtectionException`) ;
 *  2. le 403 qui suit `setCookies`, donc **avec** `cf_clearance` : c'est un
 *     rejeu refusé, et c'est là que le code change.
 *
 * Point 2 est le cœur : sans lui, « la fenêtre s'ouvre et se ferme » reste le
 * seul comportement mesuré, et le défaut est invisible.
 */
class RejeuRefuseTest {

    private lateinit var serveur: MockWebServer
    private lateinit var jar: InMemoryCookieJar

    /**
     * Horloge du test — mais **ancrée sur le temps réel**, pas à zéro.
     *
     * Mesuré le 2026-10-11 : partie de 1 000 000 (1970), la `cf_clearance` était
     * déjà expirée, `InMemoryCookieJar.loadForRequest` la filtrait (`expiresAt >=
     * System.currentTimeMillis()`), donc `porteUneClearance` rendait `false` et 4
     * tests échouaient — pour une raison **dans le test**, pas dans le code.
     * C'est le piège « une mesure qui renvoie 0 élément ne prouve rien ».
     */
    private var instant = System.currentTimeMillis()

    @BeforeTest
    fun avant() {
        serveur = MockWebServer().also { it.start() }
        jar = InMemoryCookieJar()
        RejeuRefuse.vider()
        RejeuRefuse.maintenant = { instant }
    }

    @AfterTest
    fun apres() {
        RejeuRefuse.vider()
        RejeuRefuse.maintenant = System::currentTimeMillis
        serveur.shutdown()
    }

    private fun client() = OkHttpClient.Builder()
        .cookieJar(jar)
        .addInterceptor(CloudFlareInterceptor(jar))
        .callTimeout(5, TimeUnit.SECONDS)
        .build()

    /** Un 403 que `CloudFlareHelper` reconnaît comme protection. */
    private fun reponseProtegee() = MockResponse()
        .setResponseCode(403)
        .setHeader("cf-mitigated", "challenge")
        .setHeader("server", "cloudflare")
        .setBody(
            "<html><body>" +
                "<h2 data-translate=\"blocked_why_headline\">Attention Required!</h2>" +
                "<div id=\"challenge-error-title\">Verifying you are human</div>" +
                "<div id=\"challenge-error-text\">This process is automatic.</div>" +
                "</body></html>",
        )

    private fun appeler(
        client: OkHttpClient,
        url: HttpUrl = serveur.url("/manga/"),
    ): Throwable? = try {
        client.newCall(Request.Builder().url(url).build()).execute().close()
        null
    } catch (e: Throwable) {
        e
    }

    private fun clearance(host: String) = Cookie.Builder()
        .name("cf_clearance")
        .value("une-valeur-de-test")
        .domain(host)
        .path("/")
        .expiresAt(instant + 3_600_000)
        .httpOnly()
        .build()

    /** Même méthode commune que le chemin de production `setCookies`. */
    private fun poserClearance(host: String) {
        jar.putCookies(host, listOf(clearance(host)))
        RejeuRefuse.resolutionReussie(host)
    }

    /** Simule un cookie encore valide dont l'injection n'est pas récente. */
    private fun poserClearanceSansResolutionRecente(host: String) {
        jar.putCookies(host, listOf(clearance(host)))
    }

    @Test
    fun `sans clearance le 403 reste une demande de verification`() {
        serveur.enqueue(reponseProtegee())
        val e = assertNotNull(
            appeler(client()),
            "le 403 protege doit lever une exception",
        )
        assertTrue(
            e is CloudFlareProtectionException,
            "sans clearance c'est une verification a faire, pas un rejeu refuse : ${e.javaClass.simpleName}",
        )
    }

    @Test
    fun `apres setCookies le 403 devient un rejeu refuse`() {
        serveur.enqueue(reponseProtegee())
        poserClearance(serveur.hostName)

        val e = assertNotNull(
            appeler(client()),
            "le 403 protege doit lever une exception",
        )
        assertTrue(
            e is CloudFlareUnreplayableException,
            "avec cf_clearance le site refuse le rejeu : ${e.javaClass.simpleName}",
        )
    }

    @Test
    fun `une clearance sans resolution recente reste un challenge ordinaire`() {
        serveur.enqueue(reponseProtegee())
        poserClearanceSansResolutionRecente(serveur.hostName)

        val e = assertNotNull(appeler(client()), "le 403 protege doit lever une exception")

        assertTrue(
            e is CloudFlareProtectionException,
            "un cookie encore present sans resolution recente ne prouve pas un rejeu",
        )
        assertFalse(RejeuRefuse.estRefuse(serveur.hostName))
    }

    @Test
    fun `une clearance apres expiration de la fraicheur redevient un challenge ordinaire`() {
        serveur.enqueue(reponseProtegee())
        poserClearance(serveur.hostName)
        instant += RejeuRefuse.FRAICHEUR_MS + 1

        val e = assertNotNull(appeler(client()), "le 403 protege doit lever une exception")

        assertTrue(
            e is CloudFlareProtectionException,
            "une resolution trop ancienne doit permettre une nouvelle verification",
        )
        assertFalse(RejeuRefuse.estRefuse(serveur.hostName))
    }

    @Test
    fun `la clearance du domaine parent couvre le sous domaine et partage sa cle`() {
        val domaine = "example.com"
        serveur.enqueue(reponseProtegee())
        jar.putCookies(domaine, listOf(clearance(domaine)))
        RejeuRefuse.resolutionReussie("EXAMPLE.COM:443")
        val urlSousDomaine = serveur.url("/manga/")
            .newBuilder()
            .host("www.example.com")
            .build()
        val clientSousDomaine = OkHttpClient.Builder()
            .dns(Dns { hote ->
                assertEquals("www.example.com", hote)
                listOf(InetAddress.getByName(serveur.hostName))
            })
            .cookieJar(jar)
            .addInterceptor(CloudFlareInterceptor(jar))
            .build()

        val e = assertNotNull(appeler(clientSousDomaine, urlSousDomaine))

        assertTrue(
            e is CloudFlareUnreplayableException,
            "la clearance example.com doit s'appliquer à www.example.com",
        )
        assertTrue(RejeuRefuse.estRefuse("WWW.EXAMPLE.COM:80"))
        assertTrue(RejeuRefuse.resolutionRecente("EXAMPLE.COM:443"))
    }

    @Test
    fun `fraicheur et registre normalisent la casse et le port de la meme facon`() {
        RejeuRefuse.resolutionReussie("EXAMPLE.com:443")
        assertTrue(RejeuRefuse.resolutionRecente("example.COM"))

        RejeuRefuse.marquer("Example.com")
        assertTrue(RejeuRefuse.estRefuse("EXAMPLE.COM:443"))
    }

    @Test
    fun `une resolution du domaine parent est recente pour son sous domaine`() {
        RejeuRefuse.resolutionReussie("site.com")

        assertTrue(RejeuRefuse.resolutionRecente("www.site.com"))
    }

    @Test
    fun `une resolution du sous domaine ne rend pas le domaine parent recent`() {
        RejeuRefuse.resolutionReussie("www.site.com")

        assertFalse(RejeuRefuse.resolutionRecente("site.com"))
    }

    @Test
    fun `le rejeu refuse est reconnu meme quand la cause est perdue`() {
        // Forme que produit OkHttp 5 : plus de cause, seulement le texte.
        val e = java.io.IOException(
            "canceled due to Cloudflare protection not replayable at https://x.fr/z",
        )
        assertEquals("https://x.fr/z", challengeNonRejouableDe(e))
        // Et l'ordre des deux tests : un rejeu refuse ne doit surtout pas être
        // rendu comme une demande de verification par `challengeDe`.
        assertEquals(null, challengeDe(e), "le motif du rejeu ne vaut pas challenge ordinaire")
    }

    @Test
    fun `un deuxieme essai dans la duree de vie ne redemande pas de verification`() {
        poserClearance(serveur.hostName)
        serveur.enqueue(reponseProtegee())
        serveur.enqueue(reponseProtegee())

        val premier = assertNotNull(appeler(client()), "premier appel")
        assertTrue(premier is CloudFlareUnreplayableException, "le premier doit etre un rejeu refuse")

        // Deuxieme essai, toujours dans la duree de vie. **Le site est
        // reconsulte** — c'est voulu : un site peut autoriser entre-temps, et
        // court-circuiter le reseau empacherait de le voir. Ce qui ne doit plus
        // changer, c'est le CODE : plus de demande de verification, donc plus de
        // bandeau ni de fenetre a rouvrir.
        val second = assertNotNull(appeler(client()), "deuxieme appel")

        assertTrue(
            second is CloudFlareUnreplayableException,
            "dans la duree de vie le code doit rester 'rejeu refuse', jamais une verification : ${second.javaClass.simpleName}",
        )
        assertEquals(
            2,
            serveur.requestCount,
            "le site doit rester consulte : court-circuiter le reseau cacherait un redressement",
        )
    }

    @Test
    fun `un nouvel essai SANS clearance apres un echec reste un rejeu refuse`() {
        // Le cas de l'utilisateur qui reclique sans avoir resolu : la memoire
        // porte sur l'HOTE, pas sur la presence du cookie, donc c'est elle qui
        // tranche. Sans cela, effacer le cookie suffirait a rouvrir la fenetre
        // et le cycle repartirait de zero — exactement le defaut corrige.
        serveur.enqueue(reponseProtegee())
        serveur.enqueue(reponseProtegee())
        poserClearance(serveur.hostName)
        assertNotNull(appeler(client()), "premier appel")

        jar.putCookies(serveur.hostName, emptyList()) // l'utilisateur repart de zero
        val e = assertNotNull(appeler(client()), "deuxieme appel")

        assertTrue(
            e is CloudFlareUnreplayableException,
            "la memoire est par hote : elle doit survivre a un cookie efface : ${e.javaClass.simpleName}",
        )
    }

    @Test
    fun `apres la duree de vie le site est reconsulte`() {
        poserClearance(serveur.hostName)
        serveur.enqueue(reponseProtegee())
        appeler(client())

        // On avance de plus que la duree de vie.
        instant += RejeuRefuse.TTL_MS + 1
        serveur.enqueue(MockResponse().setResponseCode(200).setBody("<html>ok</html>"))

        val e = appeler(client())

        assertEquals(null, e, "le site repond 200, donc plus rien ne doit lever")
        assertEquals(2, serveur.requestCount, "un nouvel essai doit repartir sur le reseau")
    }

    @Test
    fun `une resolution reussie oublie l echec anterieur`() {
        poserClearance(serveur.hostName)
        serveur.enqueue(reponseProtegee())
        appeler(client())
        assertTrue(RejeuRefuse.estRefuse(serveur.hostName), "l'echec doit etre memorise")

        // `setCookies` : une resolution reussie, donc une action explicite.
        RejeuRefuse.resolutionReussie(serveur.hostName)

        assertFalse(
            RejeuRefuse.estRefuse(serveur.hostName),
            "apres une resolution reussie l'utilisateur ne doit pas rester bloque",
        )
        assertTrue(RejeuRefuse.resolutionRecente(serveur.hostName))
    }

    @Test
    fun `une reponse normale oublie aussi l echec anterieur`() {
        poserClearance(serveur.hostName)
        serveur.enqueue(reponseProtegee())
        appeler(client())
        assertTrue(RejeuRefuse.estRefuse(serveur.hostName))

        serveur.enqueue(MockResponse().setResponseCode(200).setBody("<html>ok</html>"))
        appeler(client())

        assertTrue(
            !RejeuRefuse.estRefuse(serveur.hostName),
            "le site a repondu normalement : la memoire doit refleter le dernier etat",
        )
    }

    @Test
    fun `une reponse normale oublie aussi la fraicheur avant un nouveau challenge`() {
        poserClearance(serveur.hostName)
        serveur.enqueue(MockResponse().setResponseCode(200).setBody("<html>ok</html>"))
        assertEquals(null, appeler(client()), "la premiere reponse doit etre normale")
        assertFalse(RejeuRefuse.resolutionRecente(serveur.hostName))

        serveur.enqueue(reponseProtegee())
        val e = assertNotNull(appeler(client()), "le 403 protege doit lever une exception")

        assertTrue(
            e is CloudFlareProtectionException,
            "apres une reponse normale, une nouvelle verification doit etre possible",
        )
    }
}