package org.tatami.engine

import java.io.BufferedReader
import java.io.File
import java.io.InputStreamReader
import java.util.concurrent.FutureTask
import java.util.concurrent.TimeUnit
import org.json.JSONObject
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Test

/**
 * Une ligne mal formée doit être refusée, pas fatale.
 *
 * Mesuré le 2026-10-08 : la boucle de lecture n'enveloppait que
 * `JSONObject(line)` dans son `try/catch`. L'extraction des champs — `id`,
 * `method`, puis `params` — était **hors** de ce `try`, donc
 * `req.getJSONObject("params")` sur un `params` qui n'est pas un objet levait
 * une `JSONException`, c'est-à-dire une `RuntimeException`. Elle sortait de
 * `while (true)`, puis de `serve()`, puis de `main()` : la JVM mourait, stdout
 * se coupait au milieu d'une ligne selon le flush, et le superviseur Rust voyait
 * le moteur disparaître pour une requête mal formée.
 *
 * Le test reproduit le pire cas : une requête mal formée **puis** une requête
 * valide. Si le moteur meurt sur la première, la seconde n'a aucune réponse et
 * le test le voit — un test qui n'enverrait que la ligne hostile ne prouverait
 * rien, puisque « le moteur s'est arrêté » est aussi une sortie correcte après
 * `shutdown`.
 *
 * Le contrôle est donc la **réponse valide suivante**, pas le code de sortie.
 */
class EntreeMalformeeTest {

    private fun java(): String =
        File(System.getProperty("java.home"), "bin/java.exe").also {
            if (!it.isFile) throw AssertionError("java introuvable : $it")
        }.path

    /** Le classpath de `installDist`, celui de la vraie application (cf. `FermetureStdinTest`). */
    private fun classpathDuMoteur(): String {
        val lib = File("build/install/tatami-engine/lib")
        if (!lib.isDirectory) {
            throw AssertionError(
                "moteur non construit : $lib absent. Lancer `gradle installDist` " +
                    "avant ce test.",
            )
        }
        val jars = lib.listFiles { f: File -> f.name.endsWith(".jar") }.orEmpty()
        if (jars.isEmpty()) throw AssertionError("aucun JAR dans $lib")
        return jars.joinToString(";") { it.absolutePath }
    }

    /** Lecture séquentielle : un thread par ligne se disputerait le flux. */
    private fun lireUneLigne(reader: BufferedReader, delaiMs: Long = 30_000): String? {
        val future = FutureTask<String?> { reader.readLine() }
        val thread = Thread(future).apply { isDaemon = true }
        thread.start()
        future.get(delaiMs, TimeUnit.MILLISECONDS)
        return future.get()
    }

    private fun ecrire(process: Process, ligne: String) {
        process.outputStream.write(ligne.toByteArray())
        process.outputStream.write('\n'.code)
        process.outputStream.flush()
    }

    @Test
    fun `un params qui n'est pas un objet ne tue pas le moteur`() {
        val process = ProcessBuilder(
            listOf(
                java(),
                "-Xmx256m",
                "-Dfile.encoding=UTF-8",
                "-Dsun.stdout.encoding=UTF-8",
                "-cp",
                classpathDuMoteur(),
                "org.tatami.engine.MainKt",
                "serve",
            ),
        ).redirectError(ProcessBuilder.Redirect.DISCARD).start()

        val stdout = BufferedReader(InputStreamReader(process.inputStream, Charsets.UTF_8))
        Thread {
            try {
                BufferedReader(InputStreamReader(process.errorStream, Charsets.UTF_8)).use { it.readText() }
            } catch (_: Exception) {
                // Le pipe d'erreur se ferme avec le process.
            }
        }.apply { isDaemon = true }.start()

        try {
            // 1. La requête hostile : `params` est un tableau, pas un objet.
            ecrire(process, """{"id":"hostile","method":"hello","params":[],"timeoutMs":30000}""")

            // 2. Une requête parfaitement valide, qui doit obtenir sa réponse.
            ecrire(process, """{"id":"valide","method":"hello","params":{},"timeoutMs":30000}""")

            // 3. On lit **deux** réponses : celle de la requête hostile peut être
            //    une erreur (c'est correct), mais il doit y en avoir une, et la
            //    suivante doit être un `hello` qui réussit.
            val recues = mutableListOf<JSONObject>()
            repeat(2) {
                val ligne = lireUneLigne(stdout)
                assertTrue(
                    ligne != null,
                    "le moteur n'a rendu que ${recues.size} réponse(s) sur 2 : " +
                        "il est probablement mort sur la requête mal formée " +
                        "(avant le correctif, il ne rendait rien du tout)",
                )
                recues += JSONObject(ligne!!)
            }

            val valide = recues.first { it.optString("id") == "valide" }
            assertTrue(
                valide.optBoolean("ok"),
                "la requête valide doit réussir après une requête mal formée : $valide",
            )
            assertEquals("valide", valide.optString("id"))

            // L'id est recopié tel quel : c'est lui qui route la réponse.
            assertEquals(
                setOf("hostile", "valide"),
                recues.map { it.optString("id") }.toSet(),
                "chaque requête, même mal formée, doit recevoir sa réponse : $recues",
            )

            // 4. Et le moteur répond toujours.
            ecrire(process, """{"id":"troisieme","method":"hello","params":{},"timeoutMs":30000}""")
            val troisieme = JSONObject(lireUneLigne(stdout)!!)
            assertTrue(troisieme.optBoolean("ok"), "le moteur doit toujours répondre : $troisieme")
        } finally {
            runCatching { process.outputStream.close() }
            if (process.isAlive) process.destroyForcibly()
        }
    }
}
