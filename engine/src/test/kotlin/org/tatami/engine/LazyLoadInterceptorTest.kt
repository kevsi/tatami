package org.tatami.engine

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNotNull
import kotlin.test.assertNull
import kotlin.test.assertTrue

/**
 * Teste l'intercepteur sur du HTML **réel**, pas sur un cas idéalisé.
 *
 * Le fragment `HTML_REEL` ci-dessous est extrait tel quel de la page d'un
 * chapitre HENTAIORIGINES le 2026-10-04 (voir le journal de PLAN.md). Il
 * contient les deux pièges mesurés : l'URL est dans `data-src` et elle est
 * précédée d'une **espace**.
 */
class LazyLoadInterceptorTest {

    private val inter = LazyLoadInterceptor()

    @Test
    fun `le src manquant est rempli depuis data-src`() {
        val sortie = inter.corrigerHtml(HTML_REEL)
        assertNotNull(sortie, "la page doit etre corrigee : elle contient des images lazy")
        assertTrue(sortie.contains("src=\"https://"), "l'URL doit etre placee dans src")
        assertTrue(sortie.contains("page_0001.webp"), "l'URL reelle doit etre conservee")
    }

    @Test
    fun `l espace en debut d URL est retire`() {
        // Mesure : les 45 `data-src` commençaient par une espace. Conservée,
        // l'URL ne serait pas résoluble et OkHttp renverrait le site lui-même.
        //
        // L'assertion doit viser l'attribut *inséré* : `data-src=" https://`
        // contient déjà la sous-chaîne `src=" https://`, donc un test qui la
        // cherche détecte l'attribut d'origine, intact, et échoue à tort.
        val sortie = inter.corrigerHtml(HTML_REEL)!!
        val insere = " src=\"https://hentai-origines.com/wp-content/uploads/WP-manga/data/" +
            "manga_6a9d551e13e20/2388e313428baaa6cda9a9edff6b91c9/page_0001.webp\""
        assertTrue(sortie.contains(insere), "l'attribut src inséré doit être rogné : ${sortie.take(200)}")
        assertTrue(
            Regex("""\ssrc=" """).containsMatchIn(sortie) == false,
            "aucun src ne doit commencer par une espace",
        )
    }

    @Test
    fun `les 45 images sont traitees`() {
        val sortie = inter.corrigerHtml(HTML_45_IMAGES)!!
        val trouvees = Regex("(?i)\\bsrc=\"https://[^\"]*page_\\d{4}\\.webp\"").findAll(sortie).count()
        assertEquals(45, trouvees, "les 45 images de la page doivent avoir une URL dans src")
    }

    @Test
    fun `une image qui a deja un src n est pas touchee`() {
        val html = "<img src=\"https://exemple/logo.png\" data-src=\"https://exemple/autre.png\">"
        assertNull(
            inter.corrigerHtml(html),
            "une image avec un src valide ne doit jamais etre reecrite",
        )
    }

    @Test
    fun `un src placeholder est remplace`() {
        val html = "<img src=\"data:image/gif;base64,R0lGOD\" data-src=\"https://exemple/vraie.jpg\">"
        val sortie = inter.corrigerHtml(html)
        assertNotNull(sortie)
        assertTrue(sortie.contains("https://exemple/vraie.jpg"))
    }

    @Test
    fun `une page sans image lazy n est pas reecrite`() {
        val html = "<html><body><p>rien ici</p><img src=\"a.png\"></body></html>"
        assertNull(inter.corrigerHtml(html), "rien a faire : le retour doit rester intact")
    }

    @Test
    fun `le srcset prend la plus grande variante`() {
        val html = "<img data-srcset=\"https://exemple/p-320.jpg 320w, https://exemple/p-1280.jpg 1280w\">"
        val sortie = inter.corrigerHtml(html)
        assertNotNull(sortie)
        assertTrue(
            sortie.contains("https://exemple/p-1280.jpg"),
            "il faut la plus grande variante, pas la premiere : ${sortie.take(120)}",
        )
    }

    @Test
    fun `les autres attributs de lazy-load sont reconnus`() {
        for (attribut in listOf("data-lazy-src", "data-original", "data-original-src", "data-echo", "data-full-src")) {
            val html = "<img $attribut=\" https://exemple/page.jpg\">"
            val sortie = inter.corrigerHtml(html)
            assertNotNull(sortie, "$attribut doit etre reconnu")
            assertTrue(sortie.contains("https://exemple/page.jpg"), "$attribut : URL absente")
        }
    }

    @Test
    fun `le HTML est conserve hors des balises img`() {
        val sortie = inter.corrigerHtml(HTML_REEL)!!
        assertTrue(sortie.contains("<title>Titre du chapitre</title>"), "le reste du HTML doit rester intact")
        assertTrue(sortie.contains("data-src="), "l'attribut d'origine doit rester present")
    }

    private companion object {
        /**
         * Fragment réel : 3 des 45 balises `<img>` de la page, plus le
         * squelette. L'espace après `data-src="` est faithfully reproduced.
         */
        val HTML_REEL = """
            <html><head><title>Titre du chapitre</title></head><body>
            <div class="reading-content">
            <img id="image-0" data-src=" https://hentai-origines.com/wp-content/uploads/WP-manga/data/manga_6a9d551e13e20/2388e313428baaa6cda9a9edff6b91c9/page_0001.webp" class="wp-manga-chapter-img img-responsive lazyload-ordered effect-fade">
            <img id="image-1" data-src=" https://hentai-origines.com/wp-content/uploads/WP-manga/data/manga_6a9d551e13e20/2388e313428baaa6cda9a9edff6b91c9/page_0002.webp" class="wp-manga-chapter-img img-responsive lazyload-ordered effect-fade">
            <img decoding="async" id="image-2" data-src=" https://hentai-origines.com/wp-content/uploads/WP-manga/data/manga_6a9d551e13e20/2388e313428baaa6cda9a9edff6b91c9/page_0003.webp" class="wp-manga-chapter-img img-responsive lazyload-ordered effect-fade">
            </div></body></html>
        """.trimIndent()

        /**
         * La page réelle en compte 45 (`page_0001` … `page_0045`). On la
         * reconstruit pour vérifier qu'aucune n'est oubliée : c'est
         * exactement le compte que le parser annonce.
         */
        val HTML_45_IMAGES: String = buildString {
            append("<html><head><title>Chapitre</title></head><body><div class=\"reading-content\">")
            for (i in 1..45) {
                val n = i.toString().padStart(4, '0')
                append(
                    "<img id=\"image-${i - 1}\" data-src=\" https://hentai-origines.com/wp-content/uploads/" +
                        "WP-manga/data/manga_6a9d551e13e20/2388e313428baaa6cda9a9edff6b91c9/page_$n.webp\" " +
                        "class=\"wp-manga-chapter-img img-responsive lazyload-ordered effect-fade\">",
                )
            }
            append("</div></body></html>")
        }
    }
}