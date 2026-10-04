package org.tatami.engine

import okhttp3.Interceptor
import okhttp3.MediaType
import okhttp3.MediaType.Companion.toMediaTypeOrNull
import okhttp3.Response
import okhttp3.ResponseBody.Companion.toResponseBody

/**
 * Rend les images lazy-loadées lisibles par les parsers.
 *
 * **Pourquoi c'est nécessaire.** Mesuré le 2026-10-04 sur HENTAIORIGINES : la
 * page d'un chapitre contient 45 balises `<img>` et le parser annonce bien
 * 45 pages — mais **toutes avec une URL vide**. Le thème sert ses images en
 * lazy-load : l'URL est dans `data-src`, `src` est absent.
 *
 * ```html
 * <img id="image-0" data-src=" https://…/page_0001.webp"
 *      class="wp-manga-chapter-img img-responsive lazyload-ordered effect-fade">
 * ```
 *
 * Ce n'est **pas** un verrou d'âge. La même page, sans aucun cookie, donne
 * 53 images, et ces URL servent de vraies WebP (HTTP 200, `image/webp`,
 * 535 200 octets). Le HTML porte les URL ; c'est la lecture qui est en retard.
 *
 * **Ce que fait cet intercepteur.** Ce qu'un navigateur fait : avant de
 * parser, il place l'URL réelle dans `src`. On fait exactement cela.
 *
 * **Ce qu'il ne fait pas, volontairement.**
 * - Il ne touche qu'aux `<img>` dont `src` est absent, vide ou rempli par un placeholder.
 *   Une image normale n'est pas modifiée : une source qui fonctionne ne change
 *   donc pas de comportement.
 * - Il ne s'exécute que sur les réponses HTML.
 * - Il ne devine rien : sans attribut de lazy-load reconnu, la page est renvoyée
 *   octet pour octet, sans même avoir été consommée (`peekBody`).
 *
 * Les attributs couverts sont ceux des thèmes WordPress de lecture les plus
 * répandus (Madara, MangaThemesia…) : le but est qu'un correctif serve
 * plusieurs sources, pas qu'il devine juste.
 *
 * Ce correctif est **générique**, volontairement : corriger le parser
 * lui-même supposerait de forker `kotatsu-parsers-redo`, ce qu'interdit
 * AGENTS.md §13. Adapter le contenu HTTP reçu est exactement le rôle du moteur
 * (« seulement l'adaptation aux parsers »).
 */
internal class LazyLoadInterceptor : Interceptor {

    override fun intercept(chain: Interceptor.Chain): Response {
        val response = chain.proceed(chain.request())
        val type = response.body?.contentType()
        if (type == null || !type.subtype.equals("html", ignoreCase = true)) return response

        // `peekBody` ne consomme pas la réponse : si rien à faire, on la rend
        // telle quelle, octets et charset d'origine compris.
        val apercu = response.peekBody(APERCU_MAX)
        val corrige = corrigerHtml(apercu.string())
        if (corrige == null) return response

        val reecrit = corrige.toByteArray(Charsets.UTF_8).toResponseBody(type.toMediaTypeUtf8())
        return response.newBuilder()
            .body(reecrit)
            // Longueur et encodage d'origine ne valent plus rien : le corps
            // renvoyé est du texte clair, d'une longueur différente.
            .removeHeader("Content-Length")
            .removeHeader("Content-Encoding")
            .build()
    }

    /**
     * Heart of the correction, separated from HTTP so it can be tested.
     *
     * Returns `null` when the document has nothing to fix: **the caller must
     * then return the response untouched**. Returning `null` rather than the
     * original string is deliberate — it is what lets the interceptor leave
     * the bytes and the charset alone.
     */
    internal fun corrigerHtml(html: String): String? {
        if (!contientDesImagesLazy(html)) return null
        val corrige = promeouvoirDataSrc(html)
        return if (corrige == html) null else corrige
    }

    /**
     * Garde-fou bon marché : une balise `<img>` qui porte un attribut de
     * lazy-load **et** dont le `src` est inutilisable. Sans lui, on
     * réécrirait le HTML de toutes les pages du catalogue.
     */
    private fun contientDesImagesLazy(html: String): Boolean =
        IMG_TAG.findAll(html).map { it.value }.any { tag ->
            aUnAttributLazy(tag) && srcInutilisable(tag) && premierAttributLazy(tag) != null
        }

    private fun promeouvoirDataSrc(html: String): String =
        IMG_TAG.replace(html) { match ->
            val tag = match.value
            if (!aUnAttributLazy(tag) || !srcInutilisable(tag)) return@replace tag
            val src = premierAttributLazy(tag) ?: return@replace tag
            val debut = tag.indexOf('>')
            if (debut < 0) return@replace tag
            tag.substring(0, debut) + " src=\"" + echapper(src) + "\"" + tag.substring(debut)
        }

    private fun aUnAttributLazy(tag: String): Boolean =
        ATTRIBUTS_LAZY.any { tag.contains(it, ignoreCase = true) }

    private fun srcInutilisable(tag: String): Boolean {
        val v = attribut(tag, "src") ?: return true
        return v.isBlank() ||
            v.startsWith("data:") ||
            v.contains("placeholder", ignoreCase = true) ||
            v.contains("blank.gif", ignoreCase = true) ||
            v.contains("loader.gif", ignoreCase = true)
    }

    private fun premierAttributLazy(tag: String): String? {
        for (nom in ATTRIBUTS_LAZY) {
            val v = attribut(tag, nom)
            if (!v.isNullOrBlank()) return v.trim()
        }
        // `srcset` : on prend la plus grande variante, qui est la dernière
        // d'une liste « url 320w, url 720w, url 1280w ».
        for (nom in listOf("data-srcset", "srcset")) {
            val v = attribut(tag, nom)
            if (!v.isNullOrBlank()) {
                val meilleure = v.split(",").mapNotNull { candidat ->
                    val propre = candidat.trim()
                    val url = propre.substringBefore(' ').trim()
                    val largeur = propre.substringAfter(' ', "").trim().removeSuffix("w").toIntOrNull() ?: 0
                    if (url.isBlank() || url.startsWith("data:")) null else url to largeur
                }.maxByOrNull { it.second }
                if (meilleure != null) return meilleure.first
            }
        }
        return null
    }

    private fun attribut(tag: String, nom: String): String? {
        val re = Regex("(?i)\\s" + Regex.escape(nom) + "\\s*=\\s*(\"([^\"]*)\"|'([^']*)'|([^\\s>]+))")
        val m = re.find(tag) ?: return null
        return m.groupValues[2].ifEmpty {
            m.groupValues[3].ifEmpty { m.groupValues.getOrElse(4) { "" } }
        }
    }

    private fun echapper(v: String): String = v.replace("&", "&amp;").replace("\"", "&quot;")

    /** Le corps réécrit est en UTF-8 : on le dit dans le type MIME. */
    private fun MediaType.toMediaTypeUtf8(): MediaType =
        "$this; charset=utf-8".toMediaTypeOrNull() ?: this

    private val IMG_TAG = Regex("(?is)<img\\b[^>]*>")

    private val ATTRIBUTS_LAZY = listOf(
        "data-src", "data-lazy-src", "data-original", "data-original-src",
        "data-echo", "data-url", "data-full-src", "data-image",
    )

    private companion object {
        /** Assez pour une page de chapitre ; au-delà, on renonce. */
        const val APERCU_MAX = 4L * 1024 * 1024
    }
}