package org.tatami.engine

import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicBoolean
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeoutOrNull
import org.junit.jupiter.api.Assertions.assertNotNull
import org.junit.jupiter.api.Assertions.assertNull
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Test
import org.koitharu.kotatsu.parsers.model.MangaParserSource

/**
 * Le verrou de source est pris par `avec`, **pas** par `horsVerrou`.
 *
 * C'est la propriété qui rend le téléchargement des pages parallèle : mesuré le
 * 2026-10-18, quatre `fetchImage` de la même page prenaient 762 ms en parallèle
 * contre 669 ms en série tant que la lecture des octets se faisait sous verrou.
 *
 * Les deux tests sont **hors réseau** : les blocs sont des fermetures qui
 * attendent un loquet, donc ils mesurent le verrou lui-même et pas la vitesse
 * d'un CDN — une porte qui dépendrait d'un hôte n'en serait pas une.
 *
 * Le second test existe pour empêcher la correction d'aller trop loin : si
 * `horsVerrou` ne prenait plus de réservation ou si `avec` cessait de sérialiser,
 * deux requêtes de la même source se marcheraient dessus (les parsers portent un
 * `Paginator` mutable — EXHENTAI le prouve).
 */
class ParserPoolHorsVerrouTest {

    private val source = MangaParserSource.MANGADEX
    private val pool = ParserPool(EngineContext())

    @Test
    fun `horsVerrou ne prend pas le verrou de la source`() = runBlocking {
        val dedans = CountDownLatch(1)
        val libere = CountDownLatch(1)
        val fil = Thread {
            pool.horsVerrou(source) {
                dedans.countDown()
                libere.await(10, TimeUnit.SECONDS)
            }
        }
        fil.start()
        try {
            assertTrue(dedans.await(10, TimeUnit.SECONDS), "le bloc horsVerrou doit demarrer")
            // Le verrou doit être libre : un `avec` concurrent aboutit pendant
            // que le bloc horsVerrou est encore à l'intérieur.
            val acquis = withTimeoutOrNull(5_000) { pool.avec(source) { true } }
            assertNotNull(
                acquis,
                "un `avec` doit aboutir pendant un `horsVerrou` : le verrou ne doit pas être pris",
            )
        } finally {
            libere.countDown()
            fil.join(10_000)
        }
    }

    @Test
    fun `avec serialise toujours deux blocs concurrents`() = runBlocking {
        val premierDedans = CountDownLatch(1)
        val libere = CountDownLatch(1)
        val secondEntre = AtomicBoolean(false)
        val fil = Thread {
            runBlocking {
                pool.avec(source) {
                    premierDedans.countDown()
                    libere.await(10, TimeUnit.SECONDS)
                }
            }
        }
        fil.start()
        try {
            assertTrue(premierDedans.await(10, TimeUnit.SECONDS), "le premier `avec` doit entrer")
            val second = withTimeoutOrNull(400) {
                pool.avec(source) { secondEntre.set(true) }
            }
            assertNull(second, "un second `avec` ne doit pas entrer tant que le premier tient le verrou")
            assertTrue(!secondEntre.get(), "le second bloc n'a pas dû s'exécuter")
        } finally {
            libere.countDown()
            fil.join(10_000)
        }
    }
}
