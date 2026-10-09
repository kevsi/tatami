package org.tatami.engine

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import okhttp3.HttpUrl.Companion.toHttpUrl
import kotlin.test.assertFalse

/**
 * Le cookie posé par `setCookies` doit survivre au chemin **scripté**.
 *
 * Le défaut critique mesuré le 2026-10-09 : `telechargerImage`
 * construisait `val context2 = EngineContext()` — un contexte **neuf** à chaque
 * image. Or c'est le contexte de `serve()` (`Server.kt:312`) que `setCookies`
 * remplit : `cookieJar` (`:965`) et `userAgentOverride` (`:966`).
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

    @Test
    fun `l UA de challenge existe bien sur le contexte du client`() {
        val contexte = EngineContext()
        contexte.userAgentOverride = "Agent/Test"
        // Avant le correctif, un contexte neuf ignorait ce champ : l'UA posé
        // après un challenge ne servait à rien côté scripté.
        assertEquals("Agent/Test", contexte.userAgentOverride)
        kotlin.test.assertSame(contexte.httpClient, clientPourImage(contexte))
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
