package org.tatami.engine

import kotlinx.coroutines.async
import kotlinx.coroutines.awaitAll
import kotlinx.coroutines.runBlocking
import org.json.JSONObject
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertNotEquals
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Test
import java.io.File
import java.nio.file.Files
import okhttp3.OkHttpClient
import kotlin.concurrent.thread

/**
 * Deux appels simultanés sur le même script doivent donner le même résultat.
 *
 * Le défaut mesuré le 2026-10-09 : une instance de moteur Nashorn **unique** par
 * script, invoquée depuis `Dispatchers.Default` donc depuis n'importe quel thread
 * du pool non borné. Le chemin **natif** a un `Mutex` par source
 * (`ParserPool.kt:111`) ; le chemin scripté n'avait rien.
 *
 * Scénario **déjà présent en production** : ouvrir une série d'une source
 * scriptée. `page.tsx:2397` lance `details` en `void …then(…)`,
 * `page.tsx:2422` attend `chapters`, et `chapters` appelle **aussi**
 * `getDetails` (`Server.kt:560`). Deux `invokeMethod("SOURCE", "getDetails", …)`
 * simultanés sur le même moteur.
 *
 * Le script de ce test renvoie une valeur **comptée** : chaque appel incrémente
 * un compteur et renvoie sa valeur. Si les appels se chevauchent, les deux
 * résultats diffèrent — et c'est tout ce qu'on demande au test.
 */
class ScriptedParserConcurrenceTest {

    /** Un script dont `getDetails` compte ses appels. */
    private fun scriptCompteur(id: String) = """
        var SOURCE = {
            id: "$id",
            name: "Source Test",
            lang: "fr",
            baseUrl: "https://example.com",
            __compteur: 0,
            getList: function(page, query) { return JSON.stringify([]); },
            getDetails: function(mangaUrl) {
                this.__compteur += 1;
                return JSON.stringify({
                    title: "Manga " + this.__compteur,
                    description: "Desc",
                    coverUrl: "",
                    status: "ongoing",
                    authors: [],
                    tags: [],
                    chapters: []
                });
            },
            getPages: function(chapterUrl) { return JSON.stringify([]); }
        };
    """.trimIndent()

    private fun parser(id: String, dossier: File): ScriptedParser {
        val fichier = File(dossier, "$id.js")
        fichier.writeText(scriptCompteur(id))
        val bridge = HttpBridge(OkHttpClient())
        return ScriptedParser(fichier.readText(), bridge)
    }

    @Test
    fun `deux appels simultanes sur le meme script donnent le meme resultat`() = runBlocking {
        val dossier = Files.createTempDirectory("tatami-test-conc").toFile()
        try {
            val p = parser("conc-script", dossier)
            // Deux appels EN MÊME TEMPS, comme le fait l'application. Le type
            // est explicite : `async` ne peut pas inférer `JSONObject` à travers
            // `awaitAll`, et l'inférence échouerait avec un message qui ne dit
            // pas où est le problème.
            val deux: List<JSONObject> = listOf(
                async(kotlinx.coroutines.Dispatchers.Default) { p.getDetails("/m/1") },
                async(kotlinx.coroutines.Dispatchers.Default) { p.getDetails("/m/1") },
            ).awaitAll()
            val a = deux[0]
            val b = deux[1]

            // Le test compte ses appels : chaque appel incrémente `__compteur`.
            //
            // **Sans le verrou**, deux lectures simultanées du compteur
            // produisent la MÊME valeur (« Manga 1 » et « Manga 1 ») : les deux
            // threads lisent 0, incrémentent, réécrivent — le compteur gagne une
            // seule fois. C'est la signature du défaut.
            //
            // **Avec le verrou**, les appels se sérialisent : 1 puis 2. Ils
            // doivent donc être **différents** — et pas seulement différents :
            // exactement ces deux valeurs, sans doublon ni saut.
            //
            // C'est cette assertion qui fait le test : vérifier l'égalité aurait
            // été l'inverse du bon critère, et le test aurait échoué sur le
            // correctif lui-même — ce qui s'est produit à la première exécution.
            assertNotEquals(
                a.getString("title"), b.getString("title"),
                "deux appels sérialisés doivent donner deux valeurs différentes : " +
                    "s'ils donnent la même, le compteur a été écrasé, donc les " +
                    "appels se sont chevauchés sur le même moteur Nashorn",
            )
            assertEquals(
                setOf("Manga 1", "Manga 2"),
                setOf(a.getString("title"), b.getString("title")),
                "les deux appels doivent produire exactement les valeurs 1 et 2",
            )
        } finally {
            dossier.deleteRecursively()
        }
    }

    @Test
    fun `un seul appel a la fois, meme sous charge`() = runBlocking {
        val dossier = Files.createTempDirectory("tatami-test-charge").toFile()
        try {
            val p = parser("charge-script", dossier)
            // Dix appels d'un coup. Le verrou doit les sérialiser, donc le
            // compteur doit monter de 1 à 10 sans saut ni doublon.
            val titres = (1..10)
                .map { async(kotlinx.coroutines.Dispatchers.Default) { p.getDetails("/m/1") } }
                .awaitAll()
                .map { it.getString("title") }
            val distincts = titres.toSet()
            assertTrue(
                distincts.size == 10,
                "10 appels doivent donner 10 valeurs distinctes, obtenu ${distincts.size} : " +
                    titres.joinToString(),
            )
        } finally {
            dossier.deleteRecursively()
        }
    }

    @Test
    fun `le verrou n empeche pas un appel suivant`() {
        // Le garde-fou : un `Mutex` mal relâché figerait la source, et le
        // symptôme serait un timeout — pas une erreur propre. Même famille que
        // la leçon du `Mutex` non réentrant.
        runBlocking {
            val dossier = Files.createTempDirectory("tatami-test-suite").toFile()
            try {
                val p = parser("suite-script", dossier)
                repeat(5) { p.getDetails("/m/1") }
                val dernier = p.getDetails("/m/1")
                assertTrue(dernier.getString("title").isNotBlank())
            } finally {
                dossier.deleteRecursively()
            }
        }
    }
}
