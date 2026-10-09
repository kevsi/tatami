package org.tatami.engine

import okhttp3.MediaType.Companion.toMediaTypeOrNull
import okhttp3.Protocol
import okhttp3.Request
import okhttp3.Response
import okhttp3.ResponseBody.Companion.toResponseBody
import org.junit.jupiter.api.Assertions.assertFalse
import org.junit.jupiter.api.Assertions.assertThrows
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Test

/**
 * Un refus d'image doit dire **pourquoi**, pas seulement qu'il y a refus.
 *
 * Le défaut mesuré le 2026-10-09 : le journal ne portait que
 * `IllegalStateException: HTTP 404 pour image body`. C'est un constat, pas une
 * cause — et il ne distinguait pas deux situations très différentes :
 *
 *  - le CDN **refuse** (et son corps texte dit pourquoi : « You must set an
 *    appropriate User-Agent header », relevé sur `*.mangadex.network`) ;
 *  - l'URL est **périmée** (`at-home` de MangaDex est un jeton temporaire, et
 *    le CDN répond alors 404 sur un nœud qui a changé).
 *
 * Ces deux cas appellent des remèdes opposés, et le message actuel permet de
 * les confondre. C'est aussi ce qui a fait dévier deux diagnostics pendant la
 * session : faute de corps de réponse, la cause réelle est restée invisible.
 *
 * Deux contraintes, non négociables :
 *
 *  1. **Hors réseau** : on fabrique la `Response`, comme dans
 *     `ReponseImageVideTest`. Une porte qui dépend de l'humeur du CDN n'est
 *     pas une porte.
 *  2. **Aucune fuite.** Le message atterrit au journal, dont la rédaction
 *     (`journal.rs`) remplace une valeur `clé=valeur` par `<url:hachage>`.
 *     Mais cette rédaction ne voit que les jetons **commençant** par `http` :
 *     une URL au milieu d'un corps de texte passerait. On assainit donc
 *     nous-mêmes, et un test le prouve.
 */
class DiagnosticImageTest {

    private fun reponse(code: Int, corps: String, ct: String = "text/plain"): Response =
        Response.Builder()
            .request(Request.Builder().url("https://uploads.mangadex.org/data/x/1.jpg").build())
            .protocol(Protocol.HTTP_1_1)
            .code(code)
            .message("Error")
            .body(corps.toByteArray().toResponseBody(ct.toMediaTypeOrNull()))
            .build()

    @Test
    fun `le refus rapporte le texte du CDN`() {
        val e = assertThrows(IllegalStateException::class.java) {
            octetsImage(
                reponse(400, "You must set an appropriate User-Agent header"),
                "cette page",
            )
        }
        assertTrue(
            e.message!!.contains("User-Agent"),
            "le message doit porter la cause rendue par le CDN ; obtenu : ${e.message}",
        )
    }

    @Test
    fun `le refus rapporte le type de contenu`() {
        // Un 404 `image/png` et un 404 `text/html` ne se soignent pas de la
        // même façon : le premier est une image manquante, le second une page
        // de refus servie à la place.
        val e = assertThrows(IllegalStateException::class.java) {
            octetsImage(reponse(404, "<html>not found</html>", ct = "text/html"), "cette page")
        }
        assertTrue(
            e.message!!.contains("text/html"),
            "le message doit nommer le type de contenu ; obtenu : ${e.message}",
        )
    }

    @Test
    fun `le refus n invite aucune URL`() {
        // Le corps d'erreur d'un CDN **répète** l'URL demandée. Or la rédaction
        // du journal ne remplace que les jetons commençant par `http` : une URL
        // au milieu d'un texte passerait telle quelle. On l'écarte donc nous-mêmes.
        val corps = "Not found: https://cmdxd98sb0x3yprd.mangadex.network/data/secret/1.png"
        val e = assertThrows(IllegalStateException::class.java) {
            octetsImage(reponse(404, corps), "cette page")
        }
        assertFalse(
            e.message!!.contains("mangadex.network"),
            "l'URL du CDN ne doit pas fuiter dans le message ; obtenu : ${e.message}",
        )
        assertFalse(
            e.message!!.contains("secret"),
            "le chemin de l'URL ne doit pas fuiter non plus ; obtenu : ${e.message}",
        )
    }

    @Test
    fun `le refus borne la longueur du corps`() {
        // Un CDN peut renvoyer une page d'erreur de plusieurs kilo-octets :
        // la laisser entera noierait le journal (et le message d'erreur).
        val corps = "E".repeat(5000)
        val e = assertThrows(IllegalStateException::class.java) {
            octetsImage(reponse(500, corps), "cette page")
        }
        assertTrue(
            e.message!!.length < 400,
            "le message doit rester court ; longueur obtenue : ${e.message!!.length}",
        )
    }

    @Test
    fun `le refus reste complet quand le corps est vide`() {
        // Contre-test : une porte qui n'ajoute un diagnostic que s'il y en a un
        // ne doit pas ceases de refuser. Le refus reste la garantie.
        val e = assertThrows(IllegalStateException::class.java) {
            octetsImage(reponse(404, ""), "cette page")
        }
        assertTrue(
            e.message!!.contains("404"),
            "le statut doit rester present même sans corps ; obtenu : ${e.message}",
        )
        assertTrue(
            e.message!!.contains("cette page"),
            "le nom de ce qui a échoué doit rester présent ; obtenu : ${e.message}",
        )
    }
}