package org.tatami.engine

import kotlinx.coroutines.runBlocking
import org.json.JSONObject
import java.io.File
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertTrue

/**
 * Le réglage de classification doit être validé sur le chemin **scripté**.
 *
 * ## Le défaut mesuré le 2026-10-15
 *
 * `requestedRatings` n'était appelé que dans `dispatchNatif`. Le chemin
 * personnalisé l'ignorait complètement, donc :
 *
 *   - une classification **inconnue** (`"teen"`, `"NSFW"`) était acceptée en
 *     silence au lieu d'être refusée comme partout ailleurs ;
 *   - une classification demandée mais non annoncée n'était **écrite nulle part**,
 *     alors que la règle native écrit sur stderr pour que ce soit mesurable.
 *
 * ## Ce que le test ne prétend pas prouver
 *
 * Il ne prouve **pas** qu'un contenu adulte est filtré : il ne peut pas l'être.
 * `ScriptedParser.getList(page, query)` n'accepte pas de filtre, et un script
 * renvoie des mangas sans `contentRating`. La politique appliquée est donc
 * « on ne filtre pas, mais on rend visible », et c'est une décision à trancher
 * avec l'utilisateur, pas un oubli de code. Voir le commentaire dans
 * `dispatchCustom`.
 */
class ClassificationSourceCustomTest {

    private fun nouveauDossier(tag: String) =
        File(System.getProperty("java.io.tmpdir"), "tatami-rating-$tag-${System.nanoTime()}")
            .also { it.mkdirs() }

    private fun loaderAvec(dossier: File, context: EngineContext): ScriptedSourceLoader {
        val loader = ScriptedSourceLoader(dossier, context.httpClient)
        val f = File(dossier, "classifiee.js")
        f.writeText(
            """
            var SOURCE = {
                id: "classifiee", name: "Source de test", lang: "fr",
                baseUrl: "https://exemple.test",
                getList: function(page, query) {
                    return JSON.stringify([{id: "m1", title: "Un manga", coverUrl: "", url: "/manga/1"}]);
                },
                getDetails: function(u) { return "{}"; },
                getPages: function(u) { return "[]"; }
            };
            """.trimIndent(),
        )
        loader.charger(f)
        return loader
    }

    private fun requete(demandee: String? = null, safeOnly: Boolean? = null) =
        JSONObject()
            .put("source", "custom:classifiee")
            .put("query", "")
            .put("limit", 20)
            .apply {
                if (demandee != null) put("contentRating", demandee)
                if (safeOnly != null) put("safeOnly", safeOnly)
            }

    private suspend fun latest(context: EngineContext, loader: ScriptedSourceLoader, p: JSONObject) =
        dispatch(context, ParserPool(context), SessionCache(), loader, "latest", p)

    @Test
    fun `une classification inconnue est refusee, comme sur le chemin natif`() = runBlocking {
        // **Le comportement réellement nouveau.** Avant la correction, ce cas
        // passait : le chemin scripté n'appelait pas `requestedRatings`.
        val dossier = nouveauDossier("inconnue")
        val context = EngineContext()
        val loader = loaderAvec(dossier, context)
        try {
            for (valeur in listOf("teen", "NSFW", "safee")) {
                val e = assertFailsWith<IllegalArgumentException>("« $valeur » doit être refusée") {
                    latest(context, loader, requete(demandee = valeur))
                }
                assertTrue(
                    e.message!!.contains("classification inconnue"),
                    "le refus doit nommer la cause : ${e.message}",
                )
            }
        } finally {
            loader.arreter()
            dossier.deleteRecursively()
        }
    }

    @Test
    fun `une classification valide ne bloque pas la source`() = runBlocking {
        // Le contrôle positif : refuser tout ne passerait pas ce test.
        val dossier = nouveauDossier("valide")
        val context = EngineContext()
        val loader = loaderAvec(dossier, context)
        try {
            val r = latest(context, loader, requete(demandee = "SAFE"))
            assertEquals(1, r.getJSONArray("list").length(), "la source doit répondre")
        } finally {
            loader.arreter()
            dossier.deleteRecursively()
        }
    }

    @Test
    fun `une chaine vide signifie aucun filtre et reste acceptee`() = runBlocking {
        // Le réglage « Toutes » de l'interface voyage en chaîne vide. Le traiter
        // comme une valeur inconnue rendrait la source inaccessible — c'est
        // exactement le piège de la dégradation, documenté dans `RequestedRatingsTest`.
        val dossier = nouveauDossier("vide")
        val context = EngineContext()
        val loader = loaderAvec(dossier, context)
        try {
            val r = latest(context, loader, requete(demandee = ""))
            assertEquals(1, r.getJSONArray("list").length())
        } finally {
            loader.arreter()
            dossier.deleteRecursively()
        }
    }

    @Test
    fun `safeOnly reste accepte sans rien demander`() = runBlocking {
        val dossier = nouveauDossier("safeonly")
        val context = EngineContext()
        val loader = loaderAvec(dossier, context)
        try {
            val r = latest(context, loader, requete(safeOnly = true))
            assertEquals(1, r.getJSONArray("list").length())
        } finally {
            loader.arreter()
            dossier.deleteRecursively()
        }
    }

    @Test
    fun `la casse de la valeur n est pas decisive`() = runBlocking {
        val dossier = nouveauDossier("casse")
        val context = EngineContext()
        val loader = loaderAvec(dossier, context)
        try {
            val r = latest(context, loader, requete(demandee = "safe"))
            assertEquals(1, r.getJSONArray("list").length())
        } finally {
            loader.arreter()
            dossier.deleteRecursively()
        }
    }
}
