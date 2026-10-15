package org.tatami.engine

import kotlinx.coroutines.runBlocking
import okhttp3.OkHttpClient
import kotlin.test.Test
import kotlin.test.assertContains
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith

/**
 * Un script de source personnalisée ne doit pas pouvoir toucher la JVM.
 *
 * ## Ce qui était mesuré avant la correction
 *
 * `ScriptedParser.creerMoteur()` faisait `getEngineByName("nashorn")` puis `eval`,
 * sans `--no-java` ni `ClassFilter` : `Java.type("java.lang.Runtime")
 * .getRuntime().exec(...)` s'exécutait avec les droits de l'utilisateur, **au
 * démarrage du moteur**, donc avant la première requête. Toute la politique SSRF
 * de `HttpBridge` était contournable depuis l'intérieur du processus.
 *
 * ## La forme des tests
 *
 * Chaque test a son **contrôle négatif** (une exception est bien levée) et le
 * fichier se termine par le **contrôle positif** : un script légitime doit
 * continuer de fonctionner. Sans lui, « tout est refusé » passerait verte.
 */
class ConfinementNashornTest {

    private val bridge = HttpBridge(OkHttpClient())

    /** Charge à tester, suivie d'un `SOURCE` valide pour passer la validation de contrat. */
    private fun scriptQuiTente(charge: String) = """
        $charge
        var SOURCE = {
            id: "evasion", name: "Evasion", lang: "fr", baseUrl: "https://example.com",
            getList: function(page, query) { return "[]"; },
            getDetails: function(u) { return "{}"; },
            getPages: function(u) { return "[]"; }
        };
    """.trimIndent()

    /** Script valide et sans évasion : la contre-épreuve du fichier. */
    private val scriptLegitime = """
        var SOURCE = {
            id: "source-saine",
            name: "Source Test",
            lang: "fr",
            baseUrl: "https://example.com",
            getList: function(page, query) {
                return JSON.stringify([{id: "m1", title: "Manga 1", coverUrl: "", url: "/manga/1"}]);
            },
            getDetails: function(mangaUrl) { return "{}"; },
            getPages: function(chapterUrl) { return "[]"; }
        };
    """.trimIndent()

    @Test
    fun `Java type est inaccessible`() {
        // La charge **est** du JavaScript valide : un échec ne peut donc pas être
        // une erreur de syntaxe confoundue avec le refus du confinement.
        val echec = assertFailsWith<ScriptedSourceException> {
            ScriptedParser(
                scriptQuiTente("""var R = Java.type("java.lang.Runtime");"""),
                bridge,
            ).meta
        }
        assertContains(echec.message.orEmpty(), "Erreur d'évaluation")
    }

    @Test
    fun `Packages et JavaImporter sont inaccessibles`() {
        for (charge in listOf(
            "var f = Packages.java.io.File;",
            "var i = new JavaImporter(java.io.File);",
        )) {
            assertFailsWith<ScriptedSourceException>(
                message = "le confinement doit refuser : $charge",
            ) {
                ScriptedParser(scriptQuiTente(charge), bridge).meta
            }
        }
    }

    @Test
    fun `les globaux qui ecivent sur la sortie standard sont retires`() = runBlocking {
        // `print` et `exit` sont des globaux Nashorn **indépendants** de
        // `--no-java`, et ils n'étaient pas retirés : un script pouvait écrire
        // dans le flux NDJSON — donc corrompre une réponse du protocole — sans
        // jamais passer par Java. `typeof` est le témoin, pas l'absence d'erreur.
        val script = """
            var marqueur = typeof print + '/' + typeof exit + '/' + typeof load;
            var SOURCE = {
                id: "sortie", name: "Sortie", lang: "fr", baseUrl: "https://example.com",
                getList: function(page, query) {
                    return JSON.stringify([{id: "m", title: marqueur, coverUrl: "", url: "/m"}]);
                },
                getDetails: function(u) { return "{}"; },
                getPages: function(u) { return "[]"; }
            };
        """.trimIndent()
        val rapport = ScriptedParser(script, bridge).getList(1, "")
        assertEquals(
            "undefined/undefined/undefined",
            rapport.getJSONObject(0).getString("title"),
            "print, exit et load doivent être absents du contexte global",
        )
    }

    @Test
    fun `la source legitime fonctionne toujours`() = runBlocking {
        // Le contrôle positif du fichier : sans lui, « tout est refusé » passe.
        val parser = ScriptedParser(scriptLegitime, bridge)
        assertEquals("source-saine", parser.meta.id)
        val resultat = parser.getList(1, "")
        assertEquals(1, resultat.length(), "un script confiné doit pouvoir travailler")
        assertEquals("Manga 1", resultat.getJSONObject(0).getString("title"))
    }
}
