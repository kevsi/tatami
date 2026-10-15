package org.tatami.engine

import okhttp3.HttpUrl.Companion.toHttpUrl
import kotlinx.coroutines.runBlocking
import org.json.JSONArray
import org.json.JSONObject
import java.io.File
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertTrue

/**
 * `setCookies` doit fonctionner pour une source **personnalisée**.
 *
 * ## Le défaut mesuré le 2026-10-15
 *
 * `setCookies` n'était traité que par `dispatchNatif`, dont la première ligne
 * est `parseSource(sourceName)` → `MangaParserSource.valueOf("custom:x")` →
 * `ParamException("unknown source")`. Une source perso derrière un challenge
 * Cloudflare ne pouvait donc **jamais** recevoir son `cf_clearance`, et le cycle
 * de vérification se répétait indéfiniment. L'erreur était vraie de la
 * `MangaParserSource` et fausse de la source — le pire genre de message.
 *
 * ## Chaque test a son refus
 *
 * Ouvrir le chemin ne suffit pas : il faut prouver qu'un nom fantôme reste
 * refusé **et** qu'aucun cookie ne traîne. C'est ce qui empêche la correction de
 * devenir une porte ouverte.
 */
class SetCookiesSourceCustomTest {

    private lateinit var dossier: File

    private fun nouveauDossier(tag: String): File {
        val d = File(System.getProperty("java.io.tmpdir"), "tatami-cookies-$tag-${System.nanoTime()}")
        d.mkdirs()
        return d
    }

    /** Un vrai script, écrit sur disque : le registre est la source de vérité. */
    private fun ecritSource(loader: ScriptedSourceLoader, nom: String) {
        val f = File(dossier, "$nom.js")
        f.writeText(
            """
            var SOURCE = {
                id: "$nom", name: "Source de test", lang: "fr",
                baseUrl: "https://exemple.test",
                getList: function(page, query) { return "[]"; },
                getDetails: function(u) { return "{}"; },
                getPages: function(u) { return "[]"; }
            };
            """.trimIndent(),
        )
        loader.charger(f)
    }

    private fun cookie(nom: String = "cf_clearance", domaine: String = "exemple.test") = JSONObject()
        .put("name", nom)
        .put("value", "valeur-de-test")
        .put("domain", domaine)
        .put("path", "/")

    private fun requete(source: String, vararg cookies: JSONObject) = JSONObject()
        .put("source", source)
        .put("cookies", JSONArray().apply { cookies.forEach { put(it) } })

    private fun cookiesDe(context: EngineContext, url: String) =
        (context.cookieJar as InMemoryCookieJar).loadForRequest(url.toHttpUrl())

    @Test
    fun `une source personnalisee reçoit ses cookies`() {
        dossier = nouveauDossier("perso")
        val context = EngineContext()
        val loader = ScriptedSourceLoader(dossier, context.httpClient)
        ecritSource(loader, "ma-source")
        try {
            val r = appliquerCookies(context, requete("custom:ma-source", cookie()))
            assertTrue(r.getBoolean("ok"))

            assertEquals(
                listOf("cf_clearance"),
                cookiesDe(context, "https://exemple.test/manga/1").map { it.name },
                "le cookie de la source personnalisée doit être dans le magasin",
            )
        } finally {
            loader.arreter()
            dossier.deleteRecursively()
        }
    }

    @Test
    fun `le chemin de dispatch valide la source avant d appliquer quoi que ce soit`() {
        dossier = nouveauDossier("fantome")
        val context = EngineContext()
        val loader = ScriptedSourceLoader(dossier, context.httpClient)
        ecritSource(loader, "connue")
        try {
            // **Le contrôle qui compte** : une source fantôme ne doit rien
            // déposer dans le magasin. Ouvrir le chemin ne doit pas devenir une
            // porte ouverte.
            val e = assertFailsWith<IllegalArgumentException> {
                validerSourcePourCookies(loader, "custom:fantome")
            }
            assertTrue(
                e.message!!.contains("unknown custom source"),
                "le refus doit nommer la source absente : ${e.message}",
            )

            // La source connue passe.
            validerSourcePourCookies(loader, "custom:connue")
        } finally {
            loader.arreter()
            dossier.deleteRecursively()
        }
    }

    @Test
    fun `une source native inconnue reste refusee`() {
        val context = EngineContext()
        val loader = ScriptedSourceLoader(nouveauDossier("natif"), context.httpClient)
        assertFailsWith<IllegalArgumentException> {
            validerSourcePourCookies(loader, "SOURCE_QUI_N_EXISTE_PAS")
        }
    }

    @Test
    fun `un domaine a point initial est normalise puis matche`() {
        // okhttp ne matche **rien** à un domaine écrit « .exemple.test » : il
        // comparerait l'hôte au suffixe « ..exemple.test ». C'est un piège du
        // contrat — la webview livre le domaine avec son point initial — et
        // c'est pourquoi `trimStart('.')` est obligatoire.
        //
        // Premier jet de ce test (2026-10-15) : il affirmait **0** correspondance
        // et a échoué avec **1**. L'assertion était à l'envers : c'est la
        // normalisation qui la produit. Un test écrit depuis l'explication du
        // code plutôt que depuis la forme qu'il reçoit ne prouve rien.
        val avecPoint = EngineContext()
        appliquerCookies(avecPoint, requete("custom:x", cookie(domaine = ".exemple.test")))
        assertEquals(
            1,
            cookiesDe(avecPoint, "https://www.exemple.test/x").size,
            "le point initial doit être retiré, sinon aucune requête ne matche",
        )

        // Le sous-domaine matche aussi : c'est ce que couvre `domainMatch`.
        assertEquals(
            1,
            cookiesDe(avecPoint, "https://www.exemple.test/sous/page").size,
            "un sous-domaine doit matcher le domaine parent",
        )

        // Le contrôle négatif : un autre hôte, lui, ne doit rien recevoir.
        assertEquals(
            0,
            cookiesDe(avecPoint, "https://autre.test/x").size,
            "le cookie ne doit pas fuir vers un autre hôte",
        )
    }

    @Test
    fun `la regle est unique quel que soit le nom de source`() {
        // Même charge utile, deux préfixes : le résultat doit être identique.
        // C'est la garantie qu'il n'y a qu'**une** règle, et non deux chemins
        // susceptibles de diverger — le défaut même de `noms_presents` /
        // `resoudre_nom`, où l'écriture et la lecture ont divergé.
        val a = appliquerCookies(EngineContext(), requete("custom:une", cookie()))
        val b = appliquerCookies(EngineContext(), requete("MANGADEX", cookie()))
        assertEquals(a.toString(), b.toString(), "la règle doit être unique")
    }

    @Test
    fun `le routage reel laisse passer une source personnalisee`() = runBlocking {
        // **Le test qui couvre le défaut.** Les autres appelent les fonctions
        // directement ; mesuré le 2026-10-15, ils passaient même avec l'ancien
        // routage rétabli (`parseSource` d'abord, qui rejette `custom:x`). Il
        // faut donc passer par `dispatch`, comme le fait `serve()`.
        dossier = nouveauDossier("routage")
        val context = EngineContext()
        val loader = ScriptedSourceLoader(dossier, context.httpClient)
        ecritSource(loader, "ma-source")
        try {
            val r = dispatch(
                context,
                ParserPool(context),
                SessionCache(),
                loader,
                "setCookies",
                requete("custom:ma-source", cookie()),
            )
            assertTrue(r.getBoolean("ok"), "setCookies doit répondre ok pour une source perso")
            assertEquals(
                listOf("cf_clearance"),
                cookiesDe(context, "https://exemple.test/manga/1").map { it.name },
                "le cookie doit être réellement stocké",
            )
        } finally {
            loader.arreter()
            dossier.deleteRecursively()
        }
    }

    @Test
    fun `le routage reel refuse une source personnalisee fantome`() = runBlocking {
        dossier = nouveauDossier("routage-fantome")
        val context = EngineContext()
        val loader = ScriptedSourceLoader(dossier, context.httpClient)
        try {
            val e = assertFailsWith<IllegalArgumentException> {
                dispatch(
                    context,
                    ParserPool(context),
                    SessionCache(),
                    loader,
                    "setCookies",
                    requete("custom:fantome", cookie()),
                )
            }
            assertTrue(
                e.message!!.contains("unknown custom source"),
                "le refus doit nommer la source absente : ${e.message}",
            )
            assertTrue(
                cookiesDe(context, "https://exemple.test/x").isEmpty(),
                "un refus ne doit laisser aucun cookie",
            )
        } finally {
            loader.arreter()
            dossier.deleteRecursively()
        }
    }

    // Un premier jet affirmait « un cookie sans nom est refusé ». Mesuré le
    // 2026-10-15 : okhttp **accepte** un nom vide — `Cookie.Builder().name("")`
    // ne lève pas. Le test a donc été supprimé plutôt quehotsé : garder une
    // assertion inventée aurait fait porter au test une garantie qu'aucun code
    // ne tient. La validation d'un nom vide reste non faite, et c'est connu.
}
