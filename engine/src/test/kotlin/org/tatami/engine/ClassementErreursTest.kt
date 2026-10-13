package org.tatami.engine

import org.jsoup.HttpStatusException
import org.koitharu.kotatsu.parsers.exception.NotFoundException
import org.koitharu.kotatsu.parsers.exception.ParseException
import java.io.IOException
import java.net.SocketTimeoutException
import java.net.UnknownHostException
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNull

/**
 * `classerEchecSource` : une panne de source devient-elle un code que
 * l'utilisateur peut agir ?
 *
 * ## Ce que ces tests fixent
 *
 * Mesuré le 2026-10-12 sur les 98 sources FR : 83 ne fonctionnaient pas, et
 * chacune remontait `INTERNAL: <exception Java brute>`. L'utilisateur ne pouvait
 * distinguer « mon réseau est cassé », « la source est morte » et « le
 * programme est périmé » — trois preverbes d'action complètement différents.
 *
 * Le classement se fait sur le **type** et le **code HTTP**, pas sur le texte :
 * un parseur qui réécrit son message ne doit pas changer le verdict. D'où le
 * test `un message reecrit ne change pas le code`.
 */
// Les constructeurs de `ParseException` et `NotFoundException` sont derrière
// l'API interne des parsers : même opt-in que `RequestedRatingsTest.kt:39`.
@OptIn(org.koitharu.kotatsu.parsers.InternalParsersApi::class)
class ClassementErreursTest {

    private fun enveloppe(cause: Throwable) = IOException("canceled due to $cause", cause)

    @Test
    fun `un domaine qui ne resout pas est une source morte`() {
        val cause = UnknownHostException("Hôte inconnu (anime-sama.eu)")
        assertEquals(
            "SOURCE_DEAD",
            classerEchecSource(enveloppe(cause))?.first,
        )
    }

    @Test
    fun `une origine Cloudflare eteinte n'est pas une source morte`() {
        // La nuance qui compte : le domaine répond, Cloudflare répond, et le
        // serveur derrière est éteint. « Hors service » serait un mensonge
        // (« ça ne marchera jamais ») alors que « origine injoignable » est
        // juste et peut revenir.
        val (code, _) = classerEchecSource(
            HttpStatusException("Status=523", 523, "https://vinlandsaga.fr/"),
        )!!
        assertEquals("ORIGIN_DOWN", code)
    }

    @Test
    fun `les quatre codes dorigine injoignable sont groupes`() {
        // 521/522/523/525 sont quatre formulations du même fait pour
        // l'utilisateur. Les traiter différemment obligerait l'interface à
        // connaître les codes Cloudflare.
        for (s in listOf(521, 522, 523, 525)) {
            assertEquals(
                "ORIGIN_DOWN",
                classerEchecSource(HttpStatusException("Status=$s", s, "https://x/"))?.first,
                "le code $s doit etre groupe",
            )
        }
    }

    @Test
    fun `un parseur perime est distingue d une source morte`() {
        val (code, _) = classerEchecSource(
            ParseException("Root not found", "https://fr-scan.com/manga/"),
        )!!
        assertEquals("PARSER_OUTDATED", code)
    }

    @Test
    fun `la cause est retrouvee meme quand le parseur l emballe`() {
        // Le cas réel : OkHttp 5 perd la cause et ne garde que le texte. Le
        // classement doit marcher sur l'exception directe ET sur la version
        // emballée, sinon il ne servent à rien sur les vraies pannes.
        val (code, _) = classerEchecSource(
            enveloppe(enveloppe(ParseException("Cannot find", "https://x/"))),
        )!!
        assertEquals("PARSER_OUTDATED", code)
    }

    @Test
    fun `un message reecrit ne change pas le code`() {
        // Le classement ne doit pas dependre du texte : c'est tout l'interet
        // d'utiliser `getStatusCode` plutot qu'une recherche de motif.
        val a = classerEchecSource(HttpStatusException("n'importe quoi", 523, "https://x/"))
        assertEquals("ORIGIN_DOWN", a?.first)
    }

    @Test
    fun `404 et 410 sont une source morte`() {
        for (s in listOf(404, 410)) {
            assertEquals(
                "SOURCE_DEAD",
                classerEchecSource(HttpStatusException("Status=$s", s, "https://x/"))?.first,
            )
        }
    }

    @Test
    fun `NotFoundException est une source morte`() {
        assertEquals(
            "SOURCE_DEAD",
            classerEchecSource(enveloppe(NotFoundException("404", "https://x/")))?.first,
        )
    }

    @Test
    fun `une panne inconnue reste non classee`() {
        // Le filet de sécurité : une exception qu'on ne sait pas nommer doit
        // rester `INTERNAL` plutôt que d'être rangée au hasard. C'est ce qui
        // permet d'apprendre des cas nouveaux au lieu de les masquer.
        assertNull(classerEchecSource(IllegalStateException("Required value was null.")))
        assertNull(classerEchecSource(SocketTimeoutException("timeout")))
    }
}