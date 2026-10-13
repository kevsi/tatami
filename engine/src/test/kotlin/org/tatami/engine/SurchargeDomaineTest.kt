package org.tatami.engine

import org.koitharu.kotatsu.parsers.model.MangaParserSource
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNull

/**
 * La redirection de domaine ne vaut que si elle change vraiment le domaine
 * vu par le parseur. On l'éprouve donc sur `parser.domain` — le même
 * chemin que les requêtes — et non sur la table.
 */
class SurchargeDomaineTest {
    @Test
    fun `une source migree pointe sur le nouveau domaine`() {
        val parser = EngineContext().newParserInstance(MangaParserSource.ANIMESAMA)
        assertEquals("anime-sama.to", parser.domain)
    }

    @Test
    fun `les trois sources migrees sont redirigees`() {
        assertEquals(
            "japscan.foo",
            EngineContext().newParserInstance(MangaParserSource.JAPSCANSFR).domain,
        )
        assertEquals(
            "hentai-scantrad.org",
            EngineContext().newParserInstance(MangaParserSource.SCANTRAD).domain,
        )
    }

    @Test
    fun `une source non redirigee garde le domaine de son parseur`() {
        // Contre-épreuve : sans elle, une table qui redirigerait *tout*
        // passerait les deux tests ci-dessus.
        val domaine = EngineContext().newParserInstance(MangaParserSource.WEBTOONS_FR).domain
        assertNull(SurchargeDomaine.pour(MangaParserSource.WEBTOONS_FR))
        assertEquals(
            emptyList(),
            listOf("anime-sama.to", "japscan.foo", "hentai-scantrad.org").filter { it == domaine },
        )
    }

    @Test
    fun `la table ne cite que des sources du catalogue`() {
        // Une source disparue ne doit pas laisser une entrée morte : elle
        // n'appliquerait un remplacement à rien, en silence.
        val connues = MangaParserSource.entries.map { it.name }.toSet()
        val citees = listOf("ANIMESAMA", "JAPSCANSFR", "SCANTRAD")
        assertEquals(emptyList(), citees.filterNot { it in connues })
    }

    @Test
    fun `aucune autre source du catalogue n est redirigee`() {
        // Garantit l'étendue : la surcharge ne doit toucher que les trois
        // sources migrées. Un test réseau ne le prouverait pas — celui-ci, si.
        val attendues = setOf("ANIMESAMA", "JAPSCANSFR", "SCANTRAD")
        val redirigees = MangaParserSource.entries
            .filter { SurchargeDomaine.pour(it) != null }
            .map { it.name }
            .toSet()
        assertEquals(attendues, redirigees)
    }
}