package org.tatami.engine

import okhttp3.MediaType.Companion.toMediaTypeOrNull
import okhttp3.Protocol
import okhttp3.Request
import okhttp3.Response
import okhttp3.ResponseBody.Companion.toResponseBody
import org.junit.jupiter.api.Assertions.assertThrows
import kotlin.test.assertEquals
import kotlin.test.Test
import kotlin.test.assertTrue

/**
 * Une réponse d'image trop grosse doit être **refusée**, pas chargée en RAM.
 *
 * Le défaut mesuré le 2026-10-09 : `fetch_octets` (`Server.kt:303`) et
 * `octetsImage` faisaient `body.bytes()` sans plafond, alors que `HttpBridge`
 * en impose un de 5 Mo (`HttpBridge.kt:66-69`). La même famille de code avait
 * donc deux disciplines.
 *
 * Conséquence chiffrée : `LimiteDeConcurrence` autorise 4 requêtes simultanées,
 * donc une source qui sert un « JPEG » de 500 Mo faisait monter **quatre
 * lectures de 500 Mo** dans le sidecar. Un JVM qui gonfle puis meurt ne laisse
 * aucune trace utile, et le moteur est un sidecar : sa mort est lue comme
 * « le moteur ne répond plus », jamais comme « une réponse trop grosse ».
 *
 * Le plafond est un paramètre de `lireOctetsBornes`, ce qui permet de l'éprouver
 * sans allouer 33 Mo dans un test.
 */
class BorneReponseTest {

    private fun reponse(corps: ByteArray, type: String = "image/jpeg"): Response {
        val req = Request.Builder().url("https://cdn.exemple.test/p/1.jpg").build()
        return Response.Builder()
            .request(req)
            .protocol(Protocol.HTTP_1_1)
            .code(200)
            .message("OK")
            .body(corps.toResponseBody(type.toMediaTypeOrNull()))
            .build()
    }

    @Test
    fun `un corps sous le plafond est rendu entier`() {
        val corps = ByteArray(64) { 0xFF.toByte() }
        assertEquals(64, lireOctetsBornes(reponse(corps), "cette page", 1024).size)
    }

    @Test
    fun `un corps au plafond exact passe`() {
        // La borne est inclusive : un octet de plus est refusé, la valeur exacte
        // non. Un test « sous la borne » ne prouve rien de ce cas limite.
        val corps = ByteArray(1024) { 0xFF.toByte() }
        assertEquals(1024, lireOctetsBornes(reponse(corps), "cette page", 1024).size)
    }

    @Test
    fun `un corps au-dessus du plafond est refuse`() {
        val corps = ByteArray(1025) { 0xFF.toByte() }
        val e = assertThrows(IllegalStateException::class.java) {
            lireOctetsBornes(reponse(corps), "cette page", 1024)
        }
        // Le message doit nommer la borne : sans elle, un refus se lit comme
        // « ça n'a pas marché » et personne ne sait quelle limite a mordu.
        assertTrue(e.message!!.contains("1024"), "obtenu : ${e.message}")
    }

    @Test
    fun `le refus nomme la chose, pas seulement un nombre`() {
        val e = assertThrows(IllegalStateException::class.java) {
            lireOctetsBornes(reponse(ByteArray(2048)), "cette page", 1024)
        }
        assertTrue(e.message!!.startsWith("cette page"), "obtenu : ${e.message}")
    }

    @Test
    fun `un corps vide rend un tableau vide, et c est a l appelant de refuser`() {
        // Prémisse corrigée après mesure : OkHttp **ne rend jamais `null`** pour
        // un corps absent. `Response.Builder()` sans `.body()` produit un corps
        // vide, donc le `?: throw` de `lireOctetsBornes` est inatteignable par
        // ce chemin. Prétendre le tester aurait vert un test qui ne vérifie
        // rien : le garde serait resté sans preuve.
        //
        // Le vrai contrat : `lireOctetsBornes` rend `ByteArray(0)`, et **c'est
        // `octetsImage` qui refuse** un tableau vide — parce qu'un corps vide
        // est un échec, pas une image de zéro octet. Mesuré le 2026-10-09 sur
        // custom:mangadex-api : sans ce refus côté appelant, le moteur répondait
        // `ok: true` avec `data: ""` (17 pages sur 30 en image cassée). Aussi
        // couvert par `ReponseImageVideTest`.
        val req = Request.Builder().url("https://cdn.exemple.test/x.jpg").build()
        val vide = Response.Builder()
            .request(req)
            .protocol(Protocol.HTTP_1_1)
            .code(200)
            .message("OK")
            .build()
        assertEquals(0, lireOctetsBornes(vide, "cette page", 1024).size)
        // Le contrat qui compte, côté appelant.
        assertThrows(IllegalStateException::class.java) { octetsImage(vide, "cette page") }
    }

    @Test
    fun `le plafond reel couvre une planche de manga`() {
        // Contrôle de valeur : 32 Mo, et NON les 5 Mo de HttpBridge. Une page
        // fait couramment 1 à 2 Mo et certaines planches dépassent 4 Mo —
        // reprendre la borne de HttpBridge ici rendrait illisibles les plus
        // grandes planches, un défaut pire que celui qu'on corrige.
        assertEquals(32L * 1024 * 1024, MAX_OCTETS_IMAGE)
    }
}
