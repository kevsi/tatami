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

    /**
     * Le classpath de `installDist`, c'est-à-dire celui de la vraie application.
     *
     * Prendre `java.class.path` revient à passer les classes de test au moteur,
     * qui démarre alors plus lentement — et le test, qui ferme stdin
     * immédiatement, le coupe avant sa première réponse. Le défaut表现为 alors
     * comme un défaut du moteur : 0 réponse sur 5, sans rapport avec le code.
     */
    private fun classpathDuMoteur(): String {
        val lib = File("build/install/tatami-engine/lib")
        if (!lib.isDirectory) {
            throw AssertionError(
                "moteur non construit : $lib absent. Lancer `gradle installDist` " +
                    "(c'est ce que fait scripts/build-engine.ps1) avant ce test.",
            )
        }
        val jars = lib.listFiles { f: File -> f.name.endsWith(".jar") }.orEmpty()
        if (jars.isEmpty()) throw AssertionError("aucun JAR dans $lib")
        return jars.joinToString(";") { it.absolutePath }
    }

    /**
     * Lit une ligne de stdout, avec un délai.
     *
     * Un `Thread { readLine() }` par ligne est un piège : les cinq threads se
     * disputent le même flux, et celui qui n'a pas encore démarré perd sa ligne.
     * On lit donc **séquentiellement**, ce qui est de toute façon ce qu'un
     * lecteur de contrat fait.
     *
     * Le reader est passé en argument et non capturé : `stdout` est une variable
     * locale du test, donc la méthode ne peut pas la voir.
     */
    private fun lireUneLigne(reader: BufferedReader, délaiMs: Long = 30_000): String? {
        val future = java.util.concurrent.FutureTask<String?> { reader.readLine() }
        val thread = Thread(future).apply { isDaemon = true }
        thread.start()
        future.get(délaiMs, java.util.concurrent.TimeUnit.MILLISECONDS)
        return future.get()
    }

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
                // **Le classpath du run** (17 JAR de `installDist`), pas celui du
                // test. Mesuré le 2026-10-05 : avec `java.class.path`, ce
                // classpath contenait aussi les 25 classes de test, donc la JVM
                // démarrait plus lentement et la fermeture immédiate de stdin
                // survenait avant sa première réponse — 0 réponse sur 5, alors
                // que le même scénario hors Gradle rend 5/5. Le test échouait à
                // cause de son propre environnement, pas du code.
                "-cp",
                classpathDuMoteur(),
                "org.tatami.engine.MainKt",
                "serve",
            ),
        ).redirectError(ProcessBuilder.Redirect.DISCARD).start()

        val stdin = process.outputStream
        val stdout = BufferedReader(InputStreamReader(process.inputStream, Charsets.UTF_8))
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
            // **Une seule goroutine de lecture**, alimentée par une file : un
            // `Thread { readLine() }` par ligne perd la première réponse si le
            // reader n'a pas encore démarré — mesuré le 2026-10-05, 0 réponse sur
            // 5 alors que le moteur en rend 5/5 hors de ce test. Le blocage venait
            // du test, pas du moteur.
            val recues = mutableSetOf<String>()
            repeat(attendu.size) {
                val ligne = lireUneLigne(stdout)
                assertTrue(
                    ligne != null,
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