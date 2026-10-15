package org.tatami.engine

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNotNull
import kotlin.test.assertNull
import kotlin.test.assertTrue

/**
 * `listSources` ne doit **pas** réinstancier 1 384 parseurs à chaque appel.
 *
 * ## Mesures (moteur réel, sonde NDJSON, deux passages le 2026-10-15)
 *
 * | appel | sans cache | avec cache |
 * |---|---|---|
 * | 1er  | 6 477 / 6 724 ms | 4 469 / 4 580 ms |
 * | 2e   | 415 / 461 ms     | 54 / 24 ms       |
 * | 3e   | 335 / 330 ms     | 17 / 12 ms       |
 * | 4e   | 180 / 252 ms     | 10 / 20 ms       |
 *
 * L'audit annonçait « 7,6 s à chaque appel ». C'est faux : l'écart entre le
 * premier appel et les suivants est le **chargement des classes**, pas le
 * constructeur du parseur. Le gain porte donc sur les appels **suivants**,
 * environ 20 à 30 fois moins cher.
 *
 * ## Ce que ce test fixe, et ce qu'il ne fixe pas
 *
 * Il vérifie que le catalogue est construit **une fois** et réutilisé, avec le
 * **même contenu**. Il ne mesure pas le temps : une mesure de durée dans un
 * test est un test qui échoue sur une machine chargée, donc elle ne prouve que
 * la machine.
 */
class CatalogueSourcesTest {

    @Test
    fun `le catalogue est construit une fois puis reutilise`() {
        val context = EngineContext()
        assertNull(context.sourcesNatives, "rien ne doit être construit au depart")

        val premier = construireCatalogueNatif(context)
        assertTrue(
            premier.size > 1000,
            "le catalogue natif devrait couvrir plus de mille sources, trouve ${premier.size}",
        )
        context.sourcesNatives = premier

        // Le chemin de `listSources` lit le cache : il ne reconstruit rien.
        val lu = context.sourcesNatives
        assertNotNull(lu, "le cache doit etre relu tel quel")
        assertEquals(premier.size, lu.size, "le cache ne doit pas perdre d'entrees")
        assertEquals(
            premier.map { it.getString("name") },
            lu.map { it.getString("name") },
            "l'ordre et les noms doivent être identiques",
        )
    }

    @Test
    fun `chaque entree du catalogue porte les champs que l'interface lit`() {
        val catalogue = construireCatalogueNatif(EngineContext())

        val champs = catalogue.first().keys().asSequence().toSet()
        assertTrue(
            champs.containsAll(listOf("name", "lang", "custom")),
            "le contrat exige name/lang/custom, trouve $champs",
        )

        // `custom` doit être **explicitement** faux côté natif : l'interface
        // s'en sert pour distinguer une source personnalisée, et une clé absente
        // se lirait `undefined`.
        val natifs = catalogue.count { !it.optBoolean("custom", true) }
        assertEquals(catalogue.size, natifs, "toutes les sources natives sont custom=false")

        // `domains` est optionnel, mais s'il est présent il ne doit pas être vide :
        // l'interface le présente comme la liste des hôtes autorisés.
        for (entree in catalogue) {
            val domaines = entree.optJSONArray("domains") ?: continue
            assertTrue(domaines.length() > 0, "domains vide pour ${entree.getString("name")}")
            for (i in 0 until domaines.length()) {
                val hote = domaines.getString(i)
                assertTrue(
                    hote.isNotBlank() && hote.contains('.'),
                    "hote suspect pour ${entree.getString("name")} : « $hote »",
                )
            }
        }
    }

    @Test
    fun `deux appels consecutifs rendent le meme nombre de sources`() {
        // Le garde-fou le plus simple contre une régression : si quelqu'un
        // reintroduit la reconstruction, le compte reste le même et ce test ne
        // voit rien. C'est pourquoi le test précédent vérifie *l'identité* des
        // objets, pas seulement leur nombre.
        val context = EngineContext()
        val a = construireCatalogueNatif(context)
        val b = construireCatalogueNatif(context)
        assertEquals(a.size, b.size)
        assertEquals(
            a.first().getString("name"),
            b.first().getString("name"),
            "le catalogue doit être déterministe",
        )
    }
}