package org.tatami.engine

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse

/**
 * Le `Referer` d'une image doit porter le domaine de la **source**, jamais
 * celui de l'image.
 *
 * Les formes employées sont **relevées**, pas inventées : l'URL d'image est
 * celle du journal du 2026-10-16 sur HENTAIZONE, dont les chapitres sont
 * servis par `scanscloud.xyz`.
 */
class RefererImageTest {

    private val imageHentaizone =
        "https://scanscloud.xyz/chapters/manga_6824866728de7/" +
            "a67e078d6368bc6d2dd0a71019a0d04d/01.jpg"

    /**
     * La mesure qui motive la règle : le CDN répond `403` sans referer et
     * `200, 1 701 369 octets, image/jpeg` avec. Un referer tiré de l'image
     * elle-même donne `403`.
     */
    @Test
    fun `le referer porte le domaine de la source, pas celui de l'image`() {
        assertEquals("https://hentaizone.xyz/", refererImage("hentaizone.xyz", imageHentaizone))
    }

    /** Le même refus que le chemin natif produisait, vérifié explicitement. */
    @Test
    fun `le referer n'est jamais l'image elle-meme`() {
        val referer = refererImage("hentaizone.xyz", imageHentaizone)
        assertFalse(referer.contains("scanscloud.xyz"), "referer = $referer")
        assertFalse(referer.contains("/chapters/"), "le chemin de l'image fuit dans le referer")
    }

    /**
     * Un chemin de source peut porter un jeton de session : le referer ne doit
     * garder que l'origine (même règle que `site_a_referer`).
     */
    @Test
    fun `aucun jeton du chemin de la source ne fuit dans le referer`() {
        val referer = refererImage("hentaizone.xyz", imageHentaizone)
        assertFalse(referer.contains("?"), "referer = $referer")
        assertFalse(referer.contains("#"), "referer = $referer")
    }

    /**
     * Repli mesuré le 2026-10-13 sur rimuscan.fr : quand la source n'a pas de
     * domaine lisible, l'origine de l'image est le bon choix — et le seul qui
     * marche quand tout est servi depuis un même domaine.
     */
    @Test
    fun `sans domaine de source, le repli est l'origine de l'image`() {
        assertEquals("https://scanscloud.xyz/", refererImage(null, imageHentaizone))
        assertEquals("https://scanscloud.xyz/", refererImage("", imageHentaizone))
        assertEquals("https://scanscloud.xyz/", refererImage("   ", imageHentaizone))
    }

    /** Le domaine de la source est normalisé comme le reste des en-têtes. */
    @Test
    fun `le domaine de la source est normalise`() {
        assertEquals("https://hentaizone.xyz/", refererImage("  HENTAIZONE.XYZ ", imageHentaizone))
        assertEquals("https://mangas-origines.fr/", refererImage("mangas-origines.fr", imageHentaizone))
    }

    /**
     * Un nom de domaine qui n'en est pas un ne doit pas produire un referer
     * absurde : on retombe alors sur l'image, donc sur le comportement connu.
     */
    @Test
    fun `un domaine illisible retombe sur l'origine de l'image`() {
        assertEquals("https://scanscloud.xyz/", refererImage("pas un domaine", imageHentaizone))
    }
}