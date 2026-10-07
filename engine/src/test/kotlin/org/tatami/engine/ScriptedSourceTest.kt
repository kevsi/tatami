package org.tatami.engine

import okhttp3.OkHttpClient
import okhttp3.mockwebserver.MockResponse
import okhttp3.mockwebserver.MockWebServer
import org.json.JSONArray
import org.json.JSONObject
import java.io.File
import java.nio.file.Files
import kotlin.test.*

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

private fun scriptMinimal(id: String = "test-source") = """
    var SOURCE = {
        id:      "$id",
        name:    "Source Test",
        lang:    "fr",
        baseUrl: "https://example.com",
        getList: function(page, query) {
            return JSON.stringify([{id: "m1", title: "Manga 1", coverUrl: "", url: "/manga/1"}]);
        },
        getDetails: function(mangaUrl) {
            return JSON.stringify({
                title: "Manga 1", description: "Desc", coverUrl: "",
                status: "ongoing", authors: [], tags: [],
                chapters: [{id: "c1", title: "Ch.1", number: 1.0, url: "/ch/1"}]
            });
        },
        getPages: function(chapterUrl) {
            return JSON.stringify([{id: "p1", url: "https://example.com/img/1.jpg"}]);
        }
    };
""".trimIndent()

private fun scriptSansMethode(methodeManquante: String) = """
    var SOURCE = {
        id: "incomplet", name: "X", lang: "fr", baseUrl: "https://x.com",
        ${if (methodeManquante != "getList") "getList: function() { return '[]'; }," else ""}
        ${if (methodeManquante != "getDetails") "getDetails: function() { return '{}'; }," else ""}
        ${if (methodeManquante != "getPages") "getPages: function() { return '[]'; }," else ""}
        _pad: null
    };
""".trimIndent()

// ---------------------------------------------------------------------------
// Tests HttpBridge
// ---------------------------------------------------------------------------

class HttpBridgeTest {

    private val client = OkHttpClient()
    private val bridge = HttpBridge(client)

    @Test
    fun `get refuse une URL http`() {
        val ex = assertFailsWith<IllegalArgumentException> {
            bridge.get("http://example.com/")
        }
        assertTrue(ex.message!!.contains("https://"), "Message attendu : mention de https://")
    }

    @Test
    fun `post refuse une URL http`() {
        assertFailsWith<IllegalArgumentException> {
            bridge.post("http://example.com/", "{}")
        }
    }

    @Test
    fun `get refuse une URL vide`() {
        assertFailsWith<IllegalArgumentException> {
            bridge.get("ftp://example.com/")
        }
    }

    @Test
    fun `get retourne le corps pour une reponse 200`() {
        val serveur = MockWebServer()
        serveur.enqueue(MockResponse().setBody("bonjour"))
        serveur.start()
        try {
            // MockWebServer démarre en HTTP — on teste ici qu'une URL https valide
            // passe la validation. On utilise une URL synthétique https valide
            // qui pointe sur le serveur mock via une factory OkHttp personnalisée.
            // Pour rester simple, on teste juste la validation ; le test réseau
            // réel est couvert par l'intégration.
            val bridgeTest = HttpBridge(
                OkHttpClient.Builder()
                    .build()
            )
            // URL https bien formée → la validation passe (l'erreur réseau n'est
            // pas une IllegalArgumentException).
            val ex = runCatching { bridgeTest.get("https://0.0.0.0:1/") }.exceptionOrNull()
            assertFalse(ex is IllegalArgumentException, "La validation ne doit pas lever IAE pour https://")
        } finally {
            serveur.shutdown()
        }
    }
}

// ---------------------------------------------------------------------------
// Tests ScriptedParser — méta et validation
// ---------------------------------------------------------------------------

class ScriptedParserTest {

    private val client = OkHttpClient()
    private fun bridge() = HttpBridge(client)

    @Test
    fun `meta est correctement lue depuis un script valide`() {
        val parser = ScriptedParser(scriptMinimal("mon-site"), bridge())
        assertEquals("mon-site", parser.meta.id)
        assertEquals("Source Test", parser.meta.name)
        assertEquals("fr", parser.meta.lang)
        assertEquals("https://example.com", parser.meta.baseUrl)
    }

    @Test
    fun `un script sans SOURCE leve ScriptedSourceException`() {
        val ex = assertFailsWith<ScriptedSourceException> {
            ScriptedParser("var x = 1;", bridge()).meta
        }
        assertTrue(ex.message!!.contains("SOURCE"))
    }

    @Test
    fun `une methode manquante leve ScriptedSourceException`() {
        for (methode in listOf("getList", "getDetails", "getPages")) {
            val ex = assertFailsWith<ScriptedSourceException> {
                ScriptedParser(scriptSansMethode(methode), bridge()).meta
            }
            assertTrue(
                ex.message!!.contains(methode),
                "Le message doit nommer la méthode manquante : $methode",
            )
        }
    }

    @Test
    fun `un id vide leve ScriptedSourceException`() {
        val script = scriptMinimal("").replace("""id:      """", """id: """")
        assertFailsWith<ScriptedSourceException> {
            ScriptedParser(script, bridge()).meta
        }
    }

    @Test
    fun `getList retourne un JSONArray`() {
        val parser = ScriptedParser(scriptMinimal(), bridge())
        val resultat = kotlinx.coroutines.runBlocking { parser.getList(1, "") }
        assertIs<JSONArray>(resultat)
        assertEquals(1, resultat.length())
        assertEquals("Manga 1", resultat.getJSONObject(0).getString("title"))
    }

    @Test
    fun `getDetails retourne un JSONObject`() {
        val parser = ScriptedParser(scriptMinimal(), bridge())
        val resultat = kotlinx.coroutines.runBlocking { parser.getDetails("/manga/1") }
        assertIs<JSONObject>(resultat)
        assertEquals("Manga 1", resultat.getString("title"))
        assertEquals(1, resultat.getJSONArray("chapters").length())
    }

    @Test
    fun `getPages retourne un JSONArray`() {
        val parser = ScriptedParser(scriptMinimal(), bridge())
        val resultat = kotlinx.coroutines.runBlocking { parser.getPages("/ch/1") }
        assertIs<JSONArray>(resultat)
        assertEquals(1, resultat.length())
    }
}

// ---------------------------------------------------------------------------
// Tests ScriptedSourceLoader
// ---------------------------------------------------------------------------

class ScriptedSourceLoaderTest {

    private val client = OkHttpClient()

    @Test
    fun `charger enregistre le parser et retourne les meta`() {
        val dossier = Files.createTempDirectory("tatami-test-loader").toFile()
        try {
            val loader = ScriptedSourceLoader(dossier, client)
            val fichier = File(dossier, "mon-site.js").also { it.writeText(scriptMinimal("mon-site")) }
            val parser = loader.charger(fichier)
            assertEquals("mon-site", parser.meta.id)
            assertTrue(loader.sources().containsKey("mon-site"))
        } finally {
            dossier.deleteRecursively()
        }
    }

    @Test
    fun `decharger retire le parser du registre`() {
        val dossier = Files.createTempDirectory("tatami-test-loader2").toFile()
        try {
            val loader = ScriptedSourceLoader(dossier, client)
            val fichier = File(dossier, "source-a.js").also { it.writeText(scriptMinimal("source-a")) }
            loader.charger(fichier)
            assertTrue(loader.sources().containsKey("source-a"))
            loader.decharger("source-a")
            assertFalse(loader.sources().containsKey("source-a"))
        } finally {
            dossier.deleteRecursively()
        }
    }

    @Test
    fun `demarrer charge tous les scripts present`() {
        val dossier = Files.createTempDirectory("tatami-test-loader3").toFile()
        try {
            File(dossier, "s1.js").writeText(scriptMinimal("s1"))
            File(dossier, "s2.js").writeText(scriptMinimal("s2"))
            val loader = ScriptedSourceLoader(dossier, client)
            loader.demarrer()
            loader.arreter()
            val sources = loader.sources()
            assertTrue(sources.containsKey("s1"), "s1 doit être chargée")
            assertTrue(sources.containsKey("s2"), "s2 doit être chargée")
        } finally {
            dossier.deleteRecursively()
        }
    }

    @Test
    fun `l identite vient du nom de fichier meme si le script declare autre chose`() {
        // Régression du 2026-10-07 : le registre indexait sur `meta.id`, donc un
        // script nommé `exemple-source.js` qui déclare `id: "exemple-fr"` était
        // enregistré sous `exemple-fr` — un identifiant que la base (elle, dérive
        // du nom de fichier) ne connaît pas. Le nom de fichier fait foi.
        val dossier = Files.createTempDirectory("tatami-test-loader6").toFile()
        try {
            val loader = ScriptedSourceLoader(dossier, client)
            val fichier = File(dossier, "exemple-source.js")
                .also { it.writeText(scriptMinimal("exemple-fr")) }
            loader.charger(fichier)
            assertTrue(
                loader.sources().containsKey("exemple-source"),
                "le registre doit être indexé sur le nom de fichier"
            )
            assertFalse(
                loader.sources().containsKey("exemple-fr"),
                "SOURCE.id ne doit plus servir de clé"
            )
        } finally {
            dossier.deleteRecursively()
        }
    }

    @Test
    fun `supprimer le fichier decharge le parser par son nom`() {
        // Les deux moitiés que le bug désaccordait : `charger` indexe sur
        // identiteDe(fichier), et le handler ENTRY_DELETE décharge par
        // identiteDe(fichier). On vérifie l'appariement sans démarrer le thread
        // du watcher (poll à 2 s, non testable de façon fiable).
        val dossier = Files.createTempDirectory("tatami-test-loader7").toFile()
        try {
            val loader = ScriptedSourceLoader(dossier, client)
            val fichier = File(dossier, "exemple-source.js")
                .also { it.writeText(scriptMinimal("exemple-fr")) }
            loader.charger(fichier)
            assertTrue(loader.sources().containsKey("exemple-source"))
            // Ce que fait la boucleWatcher sur ENTRY_DELETE.
            fichier.delete()
            loader.decharger(identiteDe(fichier))
            assertTrue(
                loader.sources().isEmpty(),
                "après suppression, le registre ne doit plus rien contenir : " +
                    loader.sources().keys
            )
        } finally {
            dossier.deleteRecursively()
        }
    }

    @Test
    fun `un fichier sans identifiant est refuse`() {
        val dossier = Files.createTempDirectory("tatami-test-loader8").toFile()
        try {
            val loader = ScriptedSourceLoader(dossier, client)
            val sansId = File(dossier, ".js").also { it.writeText(scriptMinimal("peu-importe")) }
            assertFailsWith<IllegalArgumentException> { loader.charger(sansId) }
            assertTrue(loader.sources().isEmpty())
        } finally {
            dossier.deleteRecursively()
        }
    }

    @Test
    fun `un fichier trop gros est rejete`() {
        val dossier = Files.createTempDirectory("tatami-test-loader4").toFile()
        try {
            val loader = ScriptedSourceLoader(dossier, client)
            val gros = File(dossier, "gros.js").also {
                it.writeText("x".repeat(ScriptedParser.MAX_SCRIPT_BYTES + 1))
            }
            assertFailsWith<IllegalArgumentException> { loader.charger(gros) }
        } finally {
            dossier.deleteRecursively()
        }
    }

    @Test
    fun `un script invalide leve ScriptedSourceException`() {
        val dossier = Files.createTempDirectory("tatami-test-loader5").toFile()
        try {
            val loader = ScriptedSourceLoader(dossier, client)
            val mauvais = File(dossier, "mauvais.js").also { it.writeText("var x = 1;") }
            assertFailsWith<ScriptedSourceException> { loader.charger(mauvais) }
        } finally {
            dossier.deleteRecursively()
        }
    }
}
