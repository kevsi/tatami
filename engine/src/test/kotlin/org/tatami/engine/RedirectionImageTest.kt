package org.tatami.engine

import kotlin.test.Test
import kotlin.test.assertFalse
import kotlin.test.assertTrue

/**
 * Une image ne doit jamais être cherchée sur un hôte que `validerUrlImage`
 * n'a pas validé — donc **pas de redirection**.
 *
 * Le défaut mesuré le 2026-10-09 : `validerUrlImage` (`Server.kt:619-653`)
 * contrôle le schéma et l'hôte de l'URL reçue, mais le client utilisé ensuite
 * gardait les défauts d'OkHttp, qui suivent une redirection **sans réexaminer
 * l'hôte**. Scénario : un script `.js` renvoie une URL publique qui répond
 * `302 Location: http://127.0.0.1:8080/…`, et les octets d'une interface
 * d'administration locale reviennent en base64 pour finir dans un `.cbz`.
 *
 * C'est exactement ce que `FetchImageReseauLocalTest` prétend protéger — mais
 * ses 4 tests utilisent une URL **directe**, jamais une redirection, donc le
 * trou passait entre les mailles.
 *
 * Ces tests visent le réglage du client, pas un serveur : un vrai serveur qui
 * redirige exigerait un socket, et la garantie qu'on veut poser est
 * précisément « le client ne suit rien », ce qui se lit sur le builder.
 */
class RedirectionImageTest {

    @Test
    fun `le client d'image ne suit aucune redirection`() {
        val contexte = EngineContext()
        // Les deux ensemble : `followRedirects` seul laisse passer https→http.
        assertFalse(contexte.clientImage.followRedirects, "une redirection changerait l'hôte validé")
        assertFalse(
            contexte.clientImage.followSslRedirects,
            "une redirection https→http contournerait la validation",
        )
    }

    @Test
    fun `le client principal, lui, suit les redirections`() {
        // Contre-épreuve, et la raison pour laquelle il y a deux clients :
        // `httpClient` sert aussi le HTML des parsers, où une redirection vers
        // une autre page du même site est normale. Si un jour ce réglage sautait
        // ici, ce test échouerait — et c'est ce qu'on veut, parce que le
        // correctif aurait alors cassé le HTML pour une garantie d'image.
        val contexte = EngineContext()
        assertTrue(contexte.httpClient.followRedirects)
    }

    @Test
    fun `le client d'image garde les cookies et l'UA du contexte`() {
        // Le client d'image est dérivé du principal, donc il hérite du magasin
        // de cookies posé par `setCookies` (`:965`) et de l'UA de challenge.
        // Sans ça, corriger la redirection aurait réintroduit le défaut du
        // cookie perdu, corrigé dans le même fichier plus tôt.
        val contexte = EngineContext()
        assertTrue(contexte.clientImage.cookieJar === contexte.cookieJar)
    }
}
