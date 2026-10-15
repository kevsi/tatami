package org.tatami.engine

import java.io.BufferedReader
import java.io.File
import java.io.InputStreamReader
import java.util.concurrent.TimeUnit
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Test

/**
 * Une requête **acceptée** doit obtenir sa réponse, même si stdin se ferme
 * aussitôt après.
 *
 * ## Le défaut
 *
 * Le compteur `enVol` était incrémenté **dans** la tâche remise à
 * `workers.execute { … }`, donc **après** la soumission. Or la boucle de vidage
 * qui suit la fermeture de stdin teste `enVol > 0` : elle peut lire `0` alors
 * que des requêtes sont acceptées mais pas encore **démarrées**, conclure qu'il
 * n'y a rien à attendre, et laisser `main` rendre. Les workers étant des
 * threads **daemon** (`Server.kt:330`), la JVM les tue alors en plein vol, et
 * les réponses promises disparaissent.
 *
 * Le message d'erreur disait « stdin fermé : N réponse(s) sur 5 » — ce qui
 * désigne le test, alors que le défaut est dans le moteur. C'est ce qui rendait
 * `FermetureStdinTest` flaker : il échouait une fois sur trois **sans que rien
 * dans le moteur n'ait changé**.
 *
 * ## Pourquoi répéter, et pourquoi 10 fois
 *
 * La course se joue entre la soumission et le démarrage du thread. Elle est
 * réelle mais courte, donc aucune exécution isolée ne la prouve : c'est
 * exactement le défaut de `FermetureStdinTest`, qui pouvait passer au vert
 * alors que le moteur était cassé.
 *
 * Le nombre de répétitions n'est pas un caprice : l'échec mesuré est de
 * **3 fois sur 6**, donc 1 chance sur 2 par tour. Dix tours consécutifs
 * passent avec une probabilité de 2⁻¹⁰ ≈ 0,1 % — soit, en pratique, un
 * test rouge avant le correctif et vert après. En dessous de 10, la porte
 * resterait flakera, donc inutile.
 *
 * Le nombre de requêtes par tour est **5**, et c'est mesuré : la fenêtre de
 * course est proportionnelle au temps de création des threads, donc **large
 * avec peu de requêtes** et étroite avec beaucoup. Un essai à 60 requêtes
 * passait systématiquement — il prouvait donc le contraire de ce qu'il
 * cherchait à prouver.
 */
class RequetesAccepteesAvantEofTest {

    private fun java(): String =
        File(System.getProperty("java.home"), "bin/java.exe").also {
            if (!it.isFile) throw AssertionError("java introuvable : $it")
        }.path

    /** Le classpath de `installDist`, comme `FermetureStdinTest`. */
    private fun classpathDuMoteur(): String {
        val lib = File("build/install/tatami-engine/lib")
        if (!lib.isDirectory) {
            throw AssertionError("moteur non construit : $lib absent. Lancer `gradle installDist`.")
        }
        val jars = lib.listFiles { f: File -> f.name.endsWith(".jar") }.orEmpty()
        if (jars.isEmpty()) throw AssertionError("aucun JAR dans $lib")
        return jars.joinToString(";") { it.absolutePath }
    }

    /** Un tour : 5 requêtes d'un bloc, fermeture immédiate, puis lecture. */
    private fun unTour(nombre: Int): Int {
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

        val stdin = process.outputStream
        val stdout = BufferedReader(InputStreamReader(process.inputStream, Charsets.UTF_8))
        Thread {
            try {
                BufferedReader(InputStreamReader(process.errorStream, Charsets.UTF_8)).use { it.readText() }
            } catch (_: Exception) {
                // Le pipe d'erreur se ferme avec le process.
            }
        }.apply { isDaemon = true }.start()

        try {
            val attendu = (0 until nombre).map { "id$it" }
            stdin.write(
                attendu.joinToString("\n") { id ->
                    """{"id":"$id","method":"hello","params":{},"timeoutMs":30000}"""
                }.toByteArray()
            )
            stdin.write('\n'.code)
            stdin.flush()
            // Fermeture immédiate : c'est l'instant qui déclenche la course.
            stdin.close()

            val recues = mutableSetOf<String>()
            // Budget global, et non un délai par ligne : sinon un tour en échec
            // attend 5 × 30 s et personne ne le rejoue pour vérifier.
            //
            // 60 s, et non 20 s. Mesuré le 2026-10-15 : chaque tour démarre un
            // **JVM neuf**, donc 4 s par réponse n'est une marge que sur une
            // machine au repos. Lancée en même temps qu'une construction Rust,
            // la suite a échoué sur un tour — et le message disait « le moteur a
            // perdu des réponses », ce qui accuse le code alors que la cause était
            // la saturation. Un délai et une perte sont deux défauts différents :
            // le test les distingue maintenant.
            val limite = System.currentTimeMillis() + 60_000
            var eof = false
            while (recues.size < nombre && System.currentTimeMillis() < limite) {
                val ligne = stdout.readLine() ?: run { eof = true; break }
                assertTrue(ligne.isNotBlank(), "ligne vide reçue")
                assertTrue(
                    ligne.trim().endsWith("}"),
                    "réponse tronquée (${recues.size} reçue(s)) : $ligne",
                )
                recues += org.json.JSONObject(ligne).optString("id")
            }
            if (recues.size < nombre && !eof) {
                // Délai, et non perte : le moteur tourne peut-être encore, la
                // machine est simplement saturée. Le dire, sinon le prochain
                // lecteur part chercher un bug de protocole qui n'existe pas.
                throw AssertionError(
                    "délai dépassé : ${recues.size}/$nombre réponses en 60 s. " +
                        "C'est une saturation de la machine, pas une perte — " +
                        "relancer le test sur une machine au repos avant d'en tirer " +
                        "une conclusion. Manquantes : " +
                        (0 until nombre).map { "id$it" }.filter { it !in recues },
                )
            }
            assertTrue(
                process.waitFor(15, TimeUnit.SECONDS),
                "le moteur ne s'est pas arrêté après la fermeture de stdin",
            )
            return recues.size
        } finally {
            runCatching { stdin.close() }
            if (process.isAlive) process.destroyForcibly()
        }
    }

    @Test
    fun `dix tours de cinq requetes rendent tous leurs reponses`() {
        val tours = 10
        val parTour = 5
        val perdus = mutableListOf<String>()
        repeat(tours) { tour ->
            val rendus = unTour(parTour)
            if (rendus != parTour) perdus.add("tour ${tour + 1} : $rendus/$parTour")
        }
        assertTrue(
            perdus.isEmpty(),
            "le moteur a perdu ${perdus.size} tour(s) sur $tours : ${perdus.joinToString(", ")}. " +
                "Une requête acceptée puis la fermeture de stdin ne doit jamais perdre " +
                "de réponse : `enVol` doit compter l'acceptation, pas l'exécution en cours.",
        )
    }
}
