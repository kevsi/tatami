package org.tatami.engine

import kotlinx.coroutines.runBlocking
import okhttp3.Call
import okhttp3.Connection
import okhttp3.Interceptor
import okhttp3.Request
import okhttp3.Response
import org.json.JSONObject
import org.koitharu.kotatsu.parsers.model.MangaParserSource
import java.util.Base64
import java.util.concurrent.TimeUnit
import kotlin.test.Test
import kotlin.test.assertNotNull
import kotlin.test.assertTrue

/**
 * ZENMANGA chiffrait ses images : le lecteur n'affichait rien (tâche 0.17).
 *
 * Mesure de départ, le 2026-10-05 : 832 513 octets servis en
 * `application/octet-stream`, dont les **16 premiers octets étaient identiques
 * sur deux pages différentes** — `AA A1 AC 8B 70 20 08 3C 19 2E 77 6D 77 50 32
 * 57`. Ni JPEG, ni WEBP, ni PNG, ni HTML. Le parser porte un `ImageCodec { SEC,
 * XOR }` et `detectImageCodec(url)`, et le déchiffrement se fait dans
 * `MangaParser.intercept` — que personne n'appelait.
 *
 * Après correction : les **mêmes** octets commencent par `FF D8 FF E0`, un JPEG
 * valide.
 *
 * Le test passe par **`requete`**, le chemin réel de l'application. C'est
 * indispensable : sa première version appelait le parser directement, passait au
 * vert, et l'application montrait toujours des images vides — parce que
 * `fetchImage` faisait son téléchargement **en dehors** du `pool.avec` qui pose
 * le parser. Un test qui n'emprunte pas le chemin de production ne prouve rien
 * du chemin de production.
 */
class ZenMangaInterceptionTest {

    private fun entete(b: ByteArray, vararg attendus: Int): Boolean =
        b.size >= attendus.size && attendus.withIndex().all { (i, v) -> b[i].toInt() and 0xFF == v }

    private fun estImage(b: ByteArray): Boolean =
        entete(b, 0xFF, 0xD8) || entete(b, 0x89, 0x50, 0x4E, 0x47) || entete(b, 0x52, 0x49, 0x46, 0x46)

    private fun hexa(b: ByteArray) = b.take(8).joinToString("") { "%02X".format(it) }

    @Test
    fun `le parcours de l application sert des images lisibles`() = runBlocking {
        val ctx = EngineContext()
        val pool = ParserPool(ctx)
        // Un seul cache pour toute la séquence : `pages` exige `chapters` avant
        // lui, et `fetchImage` exige `pages`. Un cache neuf par étape mesurerait
        // un garde-fou, pas le déchiffrement.
        val cache = SessionCache()

        val liste = requete(
            ctx, pool, cache, "latest",
            JSONObject().put("source", "ZENMANGA").put("limit", 5),
        ).getJSONArray("list")
        assertTrue(liste.length() > 0, "ZENMANGA doit renvoyer des titres")

        // Mesuré le 2026-10-27 (run v0.2.0-rc.5 en échec) : la **première**
        // entrée de `latest` (« Клюква горькая », /content/bitterside-cranberries)
        // est servie **sans chapitres** — un manga ajouté au catalogue avant que
        // sa liste soit peuplée — et les quatre suivantes en ont (15, 5, 18, 4).
        // Ce test prouve le déchiffrement des images, pas la complétude du
        // catalogue : il cherche donc la première entrée *qui a des chapitres*
        // parmi les cinq, au lieu de supposer que la première en a.
        var mangaChoisi: JSONObject? = null
        var chapitresChoisis: org.json.JSONArray? = null
        for (i in 0 until minOf(5, liste.length())) {
            val candidat = liste.getJSONObject(i)
            val reponse = requete(
                ctx, pool, cache, "chapters",
                JSONObject()
                    .put("source", "ZENMANGA")
                    .put("mangaUrl", candidat.getString("url"))
                    .put("mangaTitle", candidat.getString("title")),
            ).getJSONArray("chapters")
            if (reponse.length() > 0) {
                mangaChoisi = candidat
                chapitresChoisis = reponse
                break
            }
        }
        val m = assertNotNull(mangaChoisi) {
            "aucune des 5 premières entrées de latest n'a de chapitres — le catalogue " +
                "a changé de côté, relire la mesure (sonde ZENMANGA) avant de corriger le code"
        }
        val ch = assertNotNull(chapitresChoisis)

        val pages = requete(
            ctx, pool, cache, "pages",
            JSONObject()
                .put("source", "ZENMANGA")
                .put("chapterUrl", ch.getJSONObject(0).getString("url")),
        ).getJSONArray("pages")
        assertTrue(pages.length() >= 2, "il faut au moins deux pages pour comparer")

        val relues = (0..1).map { i ->
            // La clé est `url`, pas `token` : le jeton est un concept de Rust
            // (cache disque), le contrat expose l'URL de la page.
            Base64.getDecoder().decode(
                requete(
                    ctx, pool, cache, "fetchImage",
                    JSONObject()
                        .put("source", "ZENMANGA")
                        .put("pageUrl", pages.getJSONObject(i).getString("url")),
                ).getString("data"),
            )
        }

        assertTrue(
            estImage(relues[0]),
            "la page 1 doit etre une image, entete=" + hexa(relues[0]) +
                " — le parser n'a pas dechiffre (intercepteur non branche ?)",
        )
        assertTrue(estImage(relues[1]), "la page 2 doit etre une image")
        assertTrue(
            !relues[0].contentEquals(relues[1]),
            "deux pages doivent produire des octets differents : des images identiques " +
                "signifieraient qu'on ecrit n fois la meme page",
        )
    }

    @Test
    fun `l intercepteur delegue au parser actif et laisse passer sinon`() {
        // Déterministe, sans réseau : une chaîne feinte suffit. Un test qui
        // télécharge une vraie image serait instable — mesuré le 2026-10-05,
        // l'URL CDN de MangaDex renvoie 404 selon le moment, sur le chemin de
        // production comme sur un client nu : ce n'est pas notre intercepteur.
        ParserActif.activer(null)
        val sansParser = FausseChain()
        try {
            intercepteurParser.intercept(sansParser)
        } catch (e: IllegalStateException) {
            // sentinelle
        }
        assertTrue(sansParser.proceedAppele, "sans parser actif, proceed doit etre appele")

        val espion = IntercepteurEspion()
        ParserActif.activer(espion)
        try {
            val avecParser = FausseChain()
            try {
                intercepteurParser.intercept(avecParser)
            } catch (e: IllegalStateException) {
                // sentinelle
            }
            assertTrue(espion.interceptAppele, "le parser actif doit recevoir la chaine")
            assertTrue(!avecParser.proceedAppele, "proceed ne doit pas etre appele si le parser repond")
        } finally {
            ParserActif.activer(null)
        }
    }

    @Test
    fun `le pool ne se fige pas et repete le meme resultat`() = runBlocking {
        // Garde-fou contre une régression de `Mutex` : deux requêtes
        // successives sur la même source doivent aboutir, pas se bloquer. Un
        // `Mutex` n'est pas réentrant : c'est un piège qui se voit par un
        // timeout, pas par un échec propre.
        val pool = ParserPool(EngineContext())
        val source = MangaParserSource.ZENMANGA
        val premier = pool.avec(source) { it.getFilterOptions().availableTags.size }
        val second = pool.avec(source) { it.getFilterOptions().availableTags.size }
        assertTrue(premier > 0, "la premiere requete doit repondre")
        assertTrue(premier == second, "deux appels doivent donner le meme resultat")
    }

    /** Une chaîne OkHttp qui note ce qu'on lui fait, sans toucher au réseau. */
    private class FausseChain : Interceptor.Chain {
        var proceedAppele = false
        private val requete = Request.Builder().url("https://exemple.test/a.jpg").build()

        override fun request(): Request = requete

        // On ne fabrique pas de réponse : le test vérifie *que* proceed est
        // appelé, donc on lève pour ne pas aller plus loin.
        override fun proceed(request: Request): Response {
            proceedAppele = true
            throw IllegalStateException("sentinelle: proceed appele")
        }

        override fun connection(): Connection? = null
        override fun call(): Call = throw UnsupportedOperationException("inutilise")
        override fun connectTimeoutMillis(): Int = 0
        override fun withConnectTimeout(timeout: Int, unit: TimeUnit): Interceptor.Chain = this
        override fun readTimeoutMillis(): Int = 0
        override fun withReadTimeout(timeout: Int, unit: TimeUnit): Interceptor.Chain = this
        override fun writeTimeoutMillis(): Int = 0
        override fun withWriteTimeout(timeout: Int, unit: TimeUnit): Interceptor.Chain = this
    }

    /** Un intercepteur-espion : il note l'appel et ne fait rien d'autre. */
    private class IntercepteurEspion : Interceptor {
        var interceptAppele = false
        override fun intercept(chain: Interceptor.Chain): Response {
            interceptAppele = true
            throw IllegalStateException("sentinelle: parser appele")
        }
    }
}
