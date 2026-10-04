package org.tatami.engine

import java.io.BufferedReader
import java.io.File
import java.io.InputStreamReader
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicReference
import org.json.JSONObject
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Test

/**
 * Un flux de réponses sur stdout ne doit jamais être tronqué.
 *
 * Mesuré le 2026-10-04 : cinq requêtes envoyées d'un bloc, puis fermeture de
 * stdin, donnaient **stdout vide**. Le worker écrivait sa réponse PUIS se
 * faisait interrompre par `shutdownNow()` : entre les deux, stdout pouvait se
 * terminer par une ligne tronquée, que le lecteur JSON de Rust rejette — et le
 * superviseur prend ce rejet pour un moteur cassé.
 *
 * Ce test reproduit le scénario réel : un processus `serve` piloté par un
 * pipe, qui doit rendre ses cinq réponses avant de s'arrêter. Il vérifie les
 * deux propriétés qui comptent :
 *
 *  1. chaque réponse reçue est un JSON **complet** (donc jamais tronquée) ;
 *  2. le moteur s'arrête proprement sur `shutdown`.
 *
 * Il vérifie aussi que la fermeture de stdin NE FAIT PAS perdre les requêtes
 * déjà acceptées : c'est la régression qu'on protège.
 */
class FermetureStdinTest {

    private fun java(): String =
        File(System.getProperty("java.home"), "bin/java.exe").also {
            if (!it.isFile) throw AssertionError("java introuvable : $it")
        }.path

    /** Une réponse est lisible si et seulement si c'est un JSON complet. */
    private fun reponse(ligne: String): JSONObject {
        assertTrue(ligne.isNotBlank(), "ligne vide reçue")
        assertTrue(
            ligne.trim().endsWith("}"),
            "réponse tronquée (ne se termine pas par `}`) : $ligne",
        )
        return JSONObject(ligne)
    }

    @Test
    fun `cinq requetes d'un bloc rendent cinq reponses completes`() {
        val process = ProcessBuilder(
            listOf(
                java(),
                "-Xmx256m",
                "-Dfile.encoding=UTF-8",
                "-Dsun.stdout.encoding=UTF-8",
                "-cp",
                System.getProperty("java.class.path"),
                "org.tatami.engine.MainKt",
                "serve",
            ),
        ).redirectError(ProcessBuilder.Redirect.DISCARD).start()

        val stdin = process.outputStream
        val stdout = BufferedReader(InputStreamReader(process.inputStream, Charsets.UTF_8))
        val lecture = AtomicReference<String?>(null)
        val lecteurErreur = Thread {
            try {
                BufferedReader(InputStreamReader(process.errorStream, Charsets.UTF_8)).use { r ->
                    r.readText()
                }
            } catch (_: Exception) {
                // Le pipe d'erreur se ferme avec le process : rien à signaler.
            }
        }.apply { isDaemon = true }
        lecteurErreur.start()

        try {
            // Cinq requêtes d'un bloc, puis **fermeture immédiate de stdin** : c'est
            // exactement le scénario mesuré à 0 ligne. Le mot important est
            // « immédiate » : lire d'abord les réponses, puis fermer, ne
            // reproduirait pas le défaut et le test passerait avec l'ancien
            // code — c'est-à-dire ne prouverait rien.
            val attendu = (0 until 5).map { "id$it" }
            val requetes = attendu.joinToString("\n") { id ->
                """{"id":"$id","method":"hello","params":{},"timeoutMs":30000}"""
            }
            stdin.write(requetes.toByteArray())
            stdin.write('\n'.code)
            stdin.flush()
            stdin.close()

            // L'ordre n'est PAS garanti : chaque requête a son propre thread, et
            // le contrat route les réponses par `id` précisément parce qu'elles
            // peuvent arriver dans le désordre. Ce qui doit être vrai, c'est que
            // les CINQ arrivent, toutes entières.
            val recues = mutableSetOf<String>()
            repeat(attendu.size) {
                val future = Thread { lecture.set(stdout.readLine()) }
                future.isDaemon = true
                future.start()
                future.join(30_000)
                val ligne = lecture.getAndSet(null)
                assertTrue(
                    future.isAlive.not() && ligne != null,
                    "stdin fermé : ${recues.size} réponse(s) sur ${attendu.size} " +
                        "(avant le correctif : 0 réponse)",
                )
                val json = reponse(ligne!!)
                assertTrue(json.optBoolean("ok"), "hello doit réussir : $ligne")
                recues += json.optString("id")
            }
            assertEquals(attendu.toSet(), recues, "chaque requête doit recevoir sa réponse")

            // Après la fermeture de stdin, le moteur doit s'arrêter de lui-même.
            assertTrue(
                process.waitFor(15, TimeUnit.SECONDS),
                "le moteur ne s'est pas arrêté après la fermeture de stdin",
            )
        } finally {
            runCatching { stdin.close() }
            if (process.isAlive) process.destroyForcibly()
        }
    }
}