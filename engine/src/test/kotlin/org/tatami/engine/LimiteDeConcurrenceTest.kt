package org.tatami.engine

import java.util.concurrent.Callable
import java.util.concurrent.CountDownLatch
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicInteger
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertThrows
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Test

/**
 * La limite de concurrence doit réellement borner, et ne rien coûter quand
 * l'appel échoue.
 *
 * Ces deux propriétés sont testées **sans réseau** : le bloc passé à
 * [LimiteDeConcurrence.autourDe] est une closure qui compte et dort, donc le
 * test mesure la borne elle-même et non le comportement d'un hôte — qui, lui,
 * dépend d'un CDN et d'une nuit.
 *
 * Le test du pic est écrit pour échouer tant que la borne n'est pas appliquée :
 * sans sémaphore, 24 appels simultanés donnent un pic de 24, pas 4.
 */
class LimiteDeConcurrenceTest {

    /** 24 appels, borne à 4 : le pic doit être 4, ni plus (défaut) ni moins (sérialisation). */
    @Test
    fun `le pic de concurrence ne depasse pas la borne`() {
        val limite = LimiteDeConcurrence(4)
        val appels = 24
        val pool = Executors.newFixedThreadPool(appels)
        val depart = CountDownLatch(1)
        val pic = AtomicInteger(0)
        try {
            val futures = (1..appels).map {
                pool.submit {
                    depart.await()
                    limite.autourDe {
                        // On lit le pic **pendant** l'appel : le lire après ne
                        // prouverait rien, les appels seraient déjà terminés.
                        pic.updateAndGet { p -> maxOf(p, limite.enVol) }
                        Thread.sleep(40)
                        pic.updateAndGet { p -> maxOf(p, limite.enVol) }
                    }
                }
            }
            depart.countDown()
            futures.forEach { it.get(20, TimeUnit.SECONDS) }
        } finally {
            pool.shutdownNow()
        }

        assertTrue(
            pic.get() <= 4,
            "la borne est dépassée : pic de ${pic.get()} pour une limite de 4",
        )
        assertEquals(
            4, pic.get(),
            "la borne doit être atteinte (et non pas contournée par une file de 1) : pic de ${pic.get()}",
        )
        assertEquals(0, limite.enVol, "tous les appels doivent être rendus")
    }

    /**
     * Un appel qui échoue ne doit pas coûter une place définitivement.
     *
     * Sans `finally`, dix échecs réseau successifs bloqueraient le moteur
     * jusqu'au redémarrage — un défaut qui n'apparaît que sur une source en
     * panne, donc jamais en test nominal.
     */
    @Test
    fun `un echec rend sa place`() {
        val limite = LimiteDeConcurrence(2)
        repeat(10) {
            assertThrows(IllegalStateException::class.java) {
                limite.autourDe { throw IllegalStateException("échec réseau simulé") }
            }
        }
        assertEquals(0, limite.enVol, "aucune place ne doit fuir après dix échecs")

        // Et le moteur reste utilisable : dix appels réussis après dix échecs.
        // `Callable` explicite : sans lui, Kotlin choisit la surcharge
        // `submit(Runnable)` et `get()` rend `null` — un échec de mesure qui
        // ferait accuser la limite alors que le test est fautif.
        val pool = Executors.newFixedThreadPool(10)
        try {
            val depart = CountDownLatch(1)
            val futures = (1..10).map {
                pool.submit(Callable { depart.await(); limite.autourDe { "ok" } })
            }
            depart.countDown()
            futures.forEach { assertEquals("ok", it.get(20, TimeUnit.SECONDS)) }
        } finally {
            pool.shutdownNow()
        }
        assertEquals(0, limite.enVol, "la limite doit revenir à zéro")
    }

    /** Le cas nominal : la valeur du bloc est rendue telle quelle. */
    @Test
    fun `la valeur du bloc est rendue`() {
        val limite = LimiteDeConcurrence(4)
        assertEquals(42, limite.autourDe { 42 })
        assertEquals("suite", limite.autourDe { "suite" })
        assertEquals(0, limite.enVol)
    }
}
