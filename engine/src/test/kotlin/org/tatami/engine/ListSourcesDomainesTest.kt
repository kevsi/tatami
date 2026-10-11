package org.tatami.engine

import java.io.BufferedReader
import java.io.File
import java.io.InputStreamReader
import java.util.concurrent.FutureTask
import java.util.concurrent.TimeUnit
import org.json.JSONObject
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertFalse
import org.junit.jupiter.api.Assertions.assertNotNull
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Test

class ListSourcesDomainesTest {

    private fun java(): String =
        File(System.getProperty("java.home"), "bin/java.exe").also {
            if (!it.isFile) throw AssertionError("java introuvable : $it")
        }.path

    private fun classpathDuMoteur(): String {
        val lib = File("build/install/tatami-engine/lib")
        if (!lib.isDirectory) throw AssertionError("moteur non construit : $lib absent")
        val jars = lib.listFiles { f: File -> f.name.endsWith(".jar") }.orEmpty()
        if (jars.isEmpty()) throw AssertionError("aucun JAR dans $lib")
        return jars.joinToString(";") { it.absolutePath }
    }

    private fun lireUneLigne(reader: BufferedReader): String {
        val future = FutureTask<String?> { reader.readLine() }
        Thread(future).apply { isDaemon = true }.start()
        return future.get(120, TimeUnit.SECONDS)
            ?: throw AssertionError("le moteur n'a pas répondu à listSources")
    }

    @Test
    fun `listSources expose les domaines utilisables sans chemins`() {
        val process = ProcessBuilder(
            java(),
            "-Xmx256m",
            "-Dfile.encoding=UTF-8",
            "-cp",
            classpathDuMoteur(),
            "org.tatami.engine.MainKt",
            "serve",
        ).redirectError(ProcessBuilder.Redirect.DISCARD).start()
        val writer = process.outputStream.bufferedWriter(Charsets.UTF_8)
        val reader = BufferedReader(InputStreamReader(process.inputStream, Charsets.UTF_8))

        try {
            writer.write("""{"id":"hello","method":"hello","params":{},"timeoutMs":30000}""")
            writer.newLine()
            writer.flush()
            val hello = JSONObject(lireUneLigne(reader))
            assertTrue(hello.getBoolean("ok"), hello.toString())
            assertEquals("0.2", hello.getJSONObject("result").getString("protocolVersion"))

            writer.write("""{"id":"sources","method":"listSources","params":{},"timeoutMs":120000}""")
            writer.newLine()
            writer.flush()

            val response = JSONObject(lireUneLigne(reader))
            assertTrue(response.getBoolean("ok"), response.toString())
            val sources = response.getJSONObject("result").getJSONArray("sources")
            val mangaLib = (0 until sources.length())
                .map(sources::getJSONObject)
                .firstOrNull { it.optString("name") == "MANGALIB" }
            assertNotNull(mangaLib, "MANGALIB doit être listée")
            val domaines = mangaLib!!.getJSONArray("domains")
            val hotes = (0 until domaines.length()).map(domaines::getString)
            assertTrue("mangalib.org" in hotes, hotes.toString())
            assertTrue("mangalib.me" in hotes, hotes.toString())

            for (index in 0 until sources.length()) {
                val domainesSource = sources.getJSONObject(index).optJSONArray("domains") ?: continue
                for (i in 0 until domainesSource.length()) {
                    val domaine = domainesSource.getString(i)
                    assertFalse('/' in domaine, "chemin exposé comme domaine : $domaine")
                    assertFalse("://" in domaine, "schéma exposé comme domaine : $domaine")
                    assertFalse(':' in domaine, "port exposé comme domaine : $domaine")
                }
            }
        } finally {
            runCatching { writer.close() }
            if (process.isAlive) {
                process.destroy()
                if (!process.waitFor(5, TimeUnit.SECONDS)) process.destroyForcibly()
            }
        }
    }
}
