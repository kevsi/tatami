package org.tatami.engine

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.async
import kotlinx.coroutines.launch
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import kotlinx.coroutines.yield
import org.koitharu.kotatsu.parsers.model.MangaListFilter
import org.koitharu.kotatsu.parsers.model.MangaParserSource
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import kotlin.test.Test
import kotlin.test.assertSame
import kotlin.test.assertTrue

/**
 * EXHENTAI n'exposait que sa première page.
 *
 * Mesuré le 2026-10-05, source de référence pour ce test :
 *
 * ```
 * instance neuve par appel   offset 0 -> 25   offset 20 -> AssertionError (38 ms)
 * instance réutilisée        offset 0 -> 25   offset 20 -> 25            (492 ms)
 * ```
 *
 * Cause : EXHENTAI n'inscrit le paramètre `next=<ts>` — qui ouvre la page
 * suivante — que dans le lien de la page **précédente**. Le parser le mémorise
 * dans son instance ; sans elle il lève `AssertionError("Page timestamp not
 * found")` sans faire de requête (2 ms mesurés).
 *
 * Ce test échouerait avec une instance neuve par appel. Il vérifie aussi que la
 * page 2 n'est pas la page 1 répétée : deux pages identiques passeraient sinon
 * le compte sans prouver que la pagination avance.
 */
class ParserPoolPaginationTest {

    private val source = MangaParserSource.EXHENTAI

    @Test
    fun `la page 2 d'EXHENTAI est atteignable et differente de la page 1`() {
        val ctx = EngineContext()
        val pool = ParserPool(ctx)
        val filter = MangaListFilter(
            query = null,
            tags = emptySet(),
            contentRating = emptySet(),
        )

        val premiere = runBlocking {
            val order = pool.avec(source) { it.availableSortOrders.first() }
            pool.avec(source) { it.getList(0, order, filter) }
        }
        val seconde = runBlocking {
            val order = pool.avec(source) { it.availableSortOrders.first() }
            pool.avec(source) { it.getList(20, order, filter) }
        }

        assertTrue(premiere.isNotEmpty(), "la page 1 doit renvoyer des galleries")
        assertTrue(
            seconde.isNotEmpty(),
            "la page 2 doit renvoyer des galleries : c'est le bug corrige",
        )
        assertTrue(
            premiere.map { it.url }.toSet().intersect(seconde.map { it.url }.toSet()).isEmpty(),
            "la page 2 ne doit pas répéter la page 1 : " +
                "sinon le compteur passe sans que la pagination avance",
        )
    }

    @Test
    fun `le pool conserve une instance par source et n'explose pas en memoire`() {
        val pool = ParserPool(EngineContext(), maxSources = 4)
        val sources = listOf(
            MangaParserSource.MANGADEX,
            MangaParserSource.SEIMANGA,
            MangaParserSource.MANGASORIGINES,
            MangaParserSource.REMANGA,
            MangaParserSource.MANGALIB,
        )
        runBlocking {
            sources.forEach { pool.avec(it) { p -> p.availableSortOrders.size } }
        }
        assertTrue(
            pool.taille <= 4,
            "le cache doit rester borne, il en Holds ${pool.taille}",
        )
    }

    @Test
    fun `deux requetes concurrentes sur la meme source ne se marchent pas dessus`() {
        val pool = ParserPool(EngineContext())
        val source = MangaParserSource.MANGADEX
        val resultats = java.util.concurrent.ConcurrentLinkedQueue<Int>()
        runBlocking {
            // Sans verrou, ces deux appels se disputeraient le meme
            // `Paginator` du parser et l'un des deux verrait l'etat de l'autre.
            val a = launch { resultats += pool.avec(source) { it.availableSortOrders.size } }
            val b = launch { resultats += pool.avec(source) { it.availableSortOrders.size } }
            a.join()
            b.join()
        }
        assertTrue(resultats.size == 2, "les deux appels doivent aboutir, mes: $resultats")
        assertTrue(resultats.all { it > 0 }, "les deux doivent lire les tris, mes: $resultats")
        assertTrue(pool.taille == 1, "une seule instance pour une seule source")
    }

    @Test
    fun `une entree reservee par deux appels ne peut pas etre evincee`() = runBlocking {
        val source = MangaParserSource.MANGADEX
        val extras = listOf(
            MangaParserSource.SEIMANGA,
            MangaParserSource.MANGASORIGINES,
            MangaParserSource.REMANGA,
            MangaParserSource.MANGALIB,
        )
        val pool = ParserPool(EngineContext(), maxSources = 2)
        val aPret = CountDownLatch(1)
        val libererA = CountDownLatch(1)
        val bPret = CountDownLatch(1)
        val libererB = CountDownLatch(1)

        val a = async(Dispatchers.IO) {
            pool.avec(source) { parser ->
                aPret.countDown()
                assertTrue(libererA.await(5, TimeUnit.SECONDS), "A doit etre libere")
                parser
            }
        }
        assertTrue(aPret.await(5, TimeUnit.SECONDS), "A doit tenir le mutex")

        val b = async(Dispatchers.IO) {
            pool.avec(source) { parser ->
                bPret.countDown()
                assertTrue(libererB.await(5, TimeUnit.SECONDS), "B doit etre libere")
                parser
            }
        }
        withTimeout(5_000) {
            while (pool.utilisateurs(source) != 2) yield()
        }

        extras.take(3).forEach { other ->
            pool.avec(other) { it.availableSortOrders.size }
        }

        libererA.countDown()
        val parserA = a.await()
        assertTrue(bPret.await(5, TimeUnit.SECONDS), "B doit prendre le mutex après A")

        pool.avec(extras[3]) { it.availableSortOrders.size }
        libererB.countDown()
        b.await()

        val parserApres = pool.avec(source) { it }
        assertSame(parserA, parserApres, "le parser de S doit garder son etat")
    }
}
