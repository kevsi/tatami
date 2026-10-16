package org.tatami.engine

import org.jsoup.HttpStatusException
import java.io.IOException
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertTrue

/**
 * Le detail renvoye avec `CHALLENGE_UNREPLAYABLE`.
 *
 * ## Pourquoi ce detail existe
 *
 * Un rejeu refuse n'a pas de remede : ni rejouer, ni rouvrir une fenetre
 * (`supervisor.rs:137` et `:160` — jamais transient, jamais de transport). Ce qui
 * manque alors est la **raison**, et elle etait perdue deux fois :
 *  - le message de l'engine ne portait que l'URL (`challenge at … (non rejouable)`) ;
 *  - le front ne recoit que `code` + `message`, donc **pas** la `cause` — c'est
 *    ce que prouve le type de la reponse, pas une convention interne.
 *
 * Le detail est donc rendu **en clair dans le message**, parce que c'est le seul
 * canal qui traverse les trois couches. Corollaire de la forme : ce qu'il
 * contient vient de la chaine de causes, donc il doit rester **borne** — un
 * message d'exception peut contenir un corps HTML entier.
 */
class DetailCauseTest {

    @Test
    fun `le type et la chaine de causes sont nommes`() {
        // Forme reelle : le parser emballe, OkHttp remappe. Chaque niveau porte
        // une information differente, et c'est le plus profond qui dit pourquoi.
        val reel = CloudFlareUnreplayableException("https://toonfr.com/webtoon/")
        val e = IOException("canceled", IOException("intermediaire", reel))
        val detail = detailCauseChallengeNonRejouable(e)
        assertTrue(detail.startsWith("type=IOException"), "le type racine doit etre nomme : $detail")
        assertTrue(detail.contains("CloudFlareUnreplayableException"), "la cause utile doit etre nommee : $detail")
    }

    @Test
    fun `le code HTTP est ajoute quand la chaine en porte un`() {
        // Le code est l'information la plus actionnable d'un refus : 403 avec
        // clearance, 429, 404. Il n'est dans le type que si la cause est gardee.
        val detail = detailCauseChallengeNonRejouable(
            IOException(
                "canceled",
                HttpStatusException("Status=403", 403, "https://hentaizone.xyz/manga/"),
            ),
        )
        assertTrue(detail.contains("http=403"), "le code HTTP doit etre rendu : $detail")
    }

    @Test
    fun `aucun code HTTP invente quand la chaine n en porte pas`() {
        // Un `http=` absent vaut mieux qu'un `http=0` : le message doit dire ce
        // qu'il sait, et rien de plus.
        val detail = detailCauseChallengeNonRejouable(IOException("canceled"))
        assertFalse(detail.contains("http="), "aucun code ne doit etre invente : $detail")
    }

    @Test
    fun `un message enorme est tronque`() {
        // Mesure : une page de verification Cloudflare tient dans un message
        // d'exception. Rendue en clair, elle partirait dans le message d'erreur
        // de l'interface — plusieurs kilo-octets de HTML illisible.
        val long = "x".repeat(5000) + "FIN-DU-MESSAGE"
        val detail = detailCauseChallengeNonRejouable(IOException(long))
        assertFalse(detail.contains("FIN-DU-MESSAGE"), "le message doit etre tronque : longueur=${detail.length}")
        assertTrue(detail.length <= 400, "le detail doit rester court : longueur=${detail.length}")
    }

    @Test
    fun `la chaine bornee cesse au bout de cinq niveaux`() {
        // Une chaine profonde est possible (intercepteurs imbriques). Le detail
        // est un **levier**, pas un dump. La borne marche du niveau **le plus
        // externe** vers le plus profond : sur six niveaux, les cinq premiers
        // restent nommes et le sixieme est coupe.
        val e = IOException(
            "n6",
            IOException("n5", IOException("n4", IOException("n3", IOException("n2", IOException("n1"))))),
        )
        val detail = detailCauseChallengeNonRejouable(e)
        assertTrue(detail.contains("n2"), "les cinq premiers niveaux doivent etre nommes : $detail")
        assertFalse(detail.contains("n1"), "le sixieme niveau doit etre coupe : $detail")
    }

    @Test
    fun `une cause vide ne rend pas un detail muet`() {
        // Le cas le plus pauvre : une exception sans message. On doit quand meme
        // dire de quoi il s'agit, sinon le message final ne vaut rien de plus
        // que l'ancien.
        assertEquals("type=IllegalStateException", detailCauseChallengeNonRejouable(IllegalStateException()))
    }
}