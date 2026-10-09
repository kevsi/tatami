package org.tatami.engine

import okhttp3.MediaType.Companion.toMediaTypeOrNull
import okhttp3.Protocol
import okhttp3.Request
import okhttp3.Response
import okhttp3.ResponseBody.Companion.toResponseBody
import org.junit.jupiter.api.Assertions.assertArrayEquals
import org.junit.jupiter.api.Assertions.assertThrows
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Test

/**
 * Une réponse d'image vide ou en erreur ne doit jamais devenir un succès.
 *
 * Défaut mesuré le 2026-10-09 sur `custom:mangadex-api`, chapitre « Okaeri,
 * Papa » vol. 1 : 17 pages sur 30 affichées comme des images cassées, et le
 * journal de Rust répétait 145 fois
 *
 *     tatami serve error: cette page : corps vide
 *
 * Chaîne complète, mesurée bout en bout :
 *
 *  1. MangaDex répond 403/404 **à corps vide** (CDN bloqué après une rafale) ;
 *  2. `telechargerImage` lisait `resp.body?.bytes()` — pour un corps vide,
 *     OkHttp rend un tableau **vide**, pas `null`, donc le `?: throw` ne partait
 *     jamais ;
 *  3. le moteur répondait `ok: true` avec `data: ""` — un succès qui ne
 *     contient rien ;
 *  4. `decoder()` côté Rust voyait les octets vides et renvoyait
 *     « corps vide », donc une **erreur**… mais la webview, elle, n'affichait
 *     qu'une `<img>` cassée sans aucun message.
 *
 * C'est la famille décrite dans AGENTS.md §14 : « quand une requête réussit
 * avec un résultat aberrant, le défaut est rarement dans le parseur — c'est
 * qu'un HTTP en erreur est traité comme un succès ». `HttpBridge` fait déjà le
 * contrôle (`HttpBridge.kt:60`) ; c'est ce chemin-ci qui ne le faisait pas.
 *
 * Le test est **hors réseau** : `OkHttpClient` étant un `val` non injectable,
 * on fabrique la `Response`. C'est ce qui rend la preuve déterministe — un
 * test qui dépendrait de MangaDex passerait ou échouerait selon l'humeur du
 * CDN, et une porte qui depends du monde extérieur n'est pas une porte.
 */
class ReponseImageVideTest {

    private fun reponse(code: Int, corps: ByteArray): Response =
        Response.Builder()
            .request(Request.Builder().url("https://uploads.mangadex.org/data/x/1.jpg").build())
            .protocol(Protocol.HTTP_1_1)
            .code(code)
            .message(if (code == 200) "OK" else "Forbidden")
            .body(corps.toResponseBody("image/jpeg".toMediaTypeOrNull()))
            .build()

    /** Le cas nominal : sans lui, une porte qui refuse tout ne prouve rien. */
    @Test
    fun `une image valide est rendue telle quelle`() {
        val pixels = byteArrayOf(0xFF.toByte(), 0xD8.toByte(), 0xFF.toByte(), 0xE0.toByte(), 1, 2, 3)
        val octets = octetsImage(reponse(200, pixels), "cette page")
        assertArrayEquals(pixels, octets, "une réponse 200 avec des octets doit passer telle quelle")
    }

    @Test
    fun `un corps vide est refuse meme sur un 200`() {
        // Le 200 à corps vide est le piège exact : il passe le contrôle de
        // statut, donc seul le contrôle de contenu peut l'attraper.
        val e = assertThrows(IllegalStateException::class.java) {
            octetsImage(reponse(200, ByteArray(0)), "cette page")
        }
        assertTrue(
            e.message!!.contains("cette page"),
            "le refus doit nommer ce qui a échoué ; obtenu : ${e.message}",
        )
    }

    @Test
    fun `un statut d erreur est refuse meme avec des octets`() {
        // Un CDN bloqué peut renvoyer 403 **avec** un corps (page de challenge
        // HTML). Servir ça comme une image produit une page illisible : le
        // statut doit être consulté avant le contenu.
        val html = "<html><body>Forbidden</body></html>".toByteArray()
        val e = assertThrows(IllegalStateException::class.java) {
            octetsImage(reponse(403, html), "cette page")
        }
        assertTrue(
            e.message!!.contains("403"),
            "le refus doit nommer le statut ; obtenu : ${e.message}",
        )
    }

    // Pas de cas « corps absent » : en OkHttp 4+ `Response.body` est non-null,
    // donc cette situation n'est pas constructible. Un test qui la simule
    // prouverait un cas impossible.
}
