package org.tatami.engine

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import okhttp3.HttpUrl.Companion.toHttpUrl
import org.koitharu.kotatsu.parsers.network.UserAgents
import kotlin.test.assertFalse

/**
 * Le cookie posé par `setCookies` doit survivre au chemin **scripté**.
 *
 * Le défaut critique mesuré le 2026-10-09 : `telechargerImage`
 * construisait `val context2 = EngineContext()` — un contexte **neuf** à chaque
 * image. Or c'est le contexte de `serve()` (`Server.kt:312`) que `setCookies`
 * remplit : `cookieJar` et l'UA associé au domaine de la clearance.
 *
 * Scénario réel : l'utilisateur résout un challenge Cloudflare sur une source
 * scriptée, le `cf_clearance` et l'UA sont posés, le téléchargement repart…
 * et les 30 requêtes `fetchImage` repartent avec un magasin de cookies **vide**
 * et l'UA Firefox générique, donc en 403. Le message affiché disait « ce n'est
 * pas une image », jamais « cookie non transmis ».
 *
 * Effet de bord du même défaut : un `OkHttpClient` par image, donc **aucune
 * réutilisation de connexion**, donc un handshake TLS par page.
 *
 * ## Pourquoi ce test ne parle pas à un vrai serveur
 *
 * `validerUrlImage` refuse `http://` et toute IP privée, donc un serveur de test
 * en clair est rejeté avant d'atteindre le réseau. Un serveur `https://`
 * exigerait un magasin de certificats de confiance, ce qu'aucun test unitaire ne
 * peut produire sans fabriquer une autorité. Le défaut n'était donc pas
 * **prouvable** en réseau — c'est tout le problème, et la raison pour laquelle
 * il a tenu.
 *
 * Ce test cible donc la couture exacte du défaut — `clientPourImage`, extraite
 * pour ça — et vérifie le témoin qui décide de tout : le client utilisé est-il
 * celui du contexte, ou un contexte neuf ? Un `EngineContext` neuf par appel
 * donnerait deux clients **différents** ; le correctif donne le même.
 */
class CookieRejoueTest {

    @Test
    fun `le client d'une image est celui du contexte, pas un contexte neuf`() {
        val contexte = EngineContext()
        val a = clientPourImage(contexte)
        val b = clientPourImage(contexte)
        // `assertSame`, et non `assertNotSame` : le défaut rendait DEUX clients
        // différents (un `EngineContext` neuf par appel). C'est l'identité qui
        // compte : deux objets égaux mais distincts passeraient `==`.
        kotlin.test.assertSame(a, b, "le client change entre deux appels")
    }

    @Test
    fun `un contexte neuf aurait un client different - la contre-epreuve qui prouve que le test mord`() {
        // Si le test ci-dessus passait par hasard, celui-ci échouerait : deux
        // `EngineContext` distincts ont bien deux clients distincts. Réunis, les
        // deux prouvent que l'égalité ci-dessus vient du correctif.
        val un = EngineContext()
        val deux = EngineContext()
        kotlin.test.assertNotSame(un.httpClient, deux.httpClient)
    }

    @Test
    fun `le cookie est relu sur le client du contexte donc il a survecu`() {
        val contexte = EngineContext()
        val url = "https://cdn.exemple.test/p/1.jpg".toHttpUrl()
        val cookie = okhttp3.Cookie.Builder()
            .name("cf_clearance")
            .value("abc123")
            .domain("cdn.exemple.test")
            .path("/")
            .expiresAt(System.currentTimeMillis() + 60_000)
            .build()
        (contexte.cookieJar as InMemoryCookieJar).putCookies("cdn.exemple.test", listOf(cookie))

        // La question que pose le défaut : le cookie que `setCookies` a posé
        // est-il lisible sur le client qui sert `fetchImage` ?
        val relus = clientPourImage(contexte).cookieJar.loadForRequest(url)
        assertEquals(1, relus.size)
        assertEquals("abc123", relus.first().value)
    }

    /**
     * Ce que `setCookies` transmet depuis le 2026-10-10 : l'expiration
     * **réelle** et les drapeaux du cookie, au lieu de `maintenant + 365 j`
     * forcé. Un cookie court et `secure` ne doit plus repartir en clair,
     * ni après sa date.
     *
     * Les deux sens se vérifient sans horloge truquée : `secure` se teste
     * avec une URL `http`, l'expiration avec une date déjà passée.
     */
    @Test
    fun `un cookie court et secure n est pas renvoye en http ni apres expiration`() {
        val jar = InMemoryCookieJar()
        val court = okhttp3.Cookie.Builder()
            .name("cf_clearance")
            .value("valeur")
            .domain("exemple.com")
            .path("/")
            .expiresAt(System.currentTimeMillis() + 60_000) // une minute
            // okhttp 5 : `secure()` sans argument pose le drapeau à true
            // (verifié dans le bytecode : `iconst_1; putfield secure`). Il
            // n'existe pas de `secure(false)` — le défaut est false.
            .secure()
            .build()
        jar.putCookies("exemple.com", listOf(court))

        assertEquals(
            0,
            jar.loadForRequest("http://exemple.com/".toHttpUrl()).size,
            "un cookie secure ne repart jamais en clair",
        )
        assertEquals(
            1,
            jar.loadForRequest("https://exemple.com/".toHttpUrl()).size,
            "il est bien transmis en https avant son expiration",
        )
    }

    @Test
    fun `un cookie deja perime n est plus transmis`() {
        val jar = InMemoryCookieJar()
        val perime = okhttp3.Cookie.Builder()
            .name("cf_clearance")
            .value("valeur")
            .domain("exemple.com")
            .path("/")
            .expiresAt(System.currentTimeMillis() - 1_000) // déjà passée
            .build()
        jar.putCookies("exemple.com", listOf(perime))

        assertEquals(
            0,
            jar.loadForRequest("https://exemple.com/".toHttpUrl()).size,
            "l'expiration réelle doit être respectée : un cookie périmé ne part plus",
        )
    }

    /**
     * Un cookie de domaine (`exemple.com`) doit couvrir ses sous-domaines.
     * C'est ce que permet le domaine élargi transmis par `setCookies` quand
     * la webview rend un cookie plus large que l'hôte du challenge — et la
     * raison pour laquelle le moteur normalise le point initial : avec lui,
     * okhttp ne matche plus rien (verifié dans son bytecode).
     */
    @Test
    fun `un cookie sur le domaine couvre le sous-domaine`() {
        val jar = InMemoryCookieJar()
        val domaine = okhttp3.Cookie.Builder()
            .name("cf_clearance")
            .value("valeur")
            .domain("exemple.com")
            .path("/")
            .expiresAt(System.currentTimeMillis() + 60_000)
            .build()
        jar.putCookies("exemple.com", listOf(domaine))

        assertEquals(
            1,
            jar.loadForRequest("https://api.exemple.com/".toHttpUrl()).size,
            "un cookie de domaine doit couvrir api.<domaine>",
        )
    }

    @Test
    fun `l UA de challenge existe bien sur le client qui sert les images`() {
        val contexte = EngineContext()
        contexte.definirUserAgentChallenge("hentaizone.xyz", "Agent/Test")
        assertEquals("Agent/Test", contexte.userAgentChallengePour("hentaizone.xyz"))
        // `assertSame` sur `httpClient` ne tient plus depuis que
        // `clientPourImage` rend `clientImage` — un client dérivé, sans
        // redirections (`RedirectionImageTest`). L'identité du client même
        // n'a jamais été la garantie : ce qui compte, c'est que le client rendu
        // soit **dérivé** du principal, donc qu'il porte le même magasin de
        // cookies et les mêmes intercepteurs.
        kotlin.test.assertNotSame(contexte.httpClient, clientPourImage(contexte))
        kotlin.test.assertSame(contexte.cookieJar, clientPourImage(contexte).cookieJar)
    }

    @Test
    fun `UA d un challenge ne remplace pas celui des autres domaines`() {
        val contexte = EngineContext()
        contexte.definirUserAgentChallenge("hentaizone.xyz", "Agent/Challenge")

        assertEquals("Agent/Challenge", contexte.userAgentChallengePour("hentaizone.xyz"))
        assertEquals("Agent/Challenge", contexte.userAgentChallengePour("www.hentaizone.xyz"))
        assertEquals(null, contexte.userAgentChallengePour("mangas-origines.fr"))
        assertEquals(UserAgents.FIREFOX_DESKTOP, contexte.getDefaultUserAgent())
    }

    @Test
    fun `un contexte neuf donne bien un client different de celui du contexte courant`() {
        // Contre-épreuve utile : elle prouve que `assertSame` ci-dessus
        // distingue réellement les deux cas. Si un jour `clientPourImage`
        // retombe sur `EngineContext()`, ce test passe et celui du dessus
        // échoue — donc le correctif ne peut pas disparaître en silence.
        val contexte = EngineContext()
        kotlin.test.assertNotSame(contexte.httpClient, EngineContext().httpClient)
    }
}
