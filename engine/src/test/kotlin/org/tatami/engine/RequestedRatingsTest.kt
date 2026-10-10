package org.tatami.engine

import org.json.JSONObject
import org.koitharu.kotatsu.parsers.model.ContentRating
import org.koitharu.kotatsu.parsers.model.MangaListFilterOptions
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertTrue

/**
 * `requestedRatings` : la classification demandée par l'appelant est-elle
 *
 * ## Le défaut d'origine
 *
 * La fonction rendait `emptySet()` — **aucun filtre** — dans deux cas très
 * différents : une valeur d'enum invalide, et une classification que la source
 * n'expose pas. Avec le réglage par défaut de l'interface (`contentRating:
 * "SUGGESTIVE"`, qui exclut l'adulte), une source qui ne l'expose pas livre son
 * catalogue adulte sans le dire.
 *
 * ## Ce que la mesure a empêché de faire
 *
 * Refuser dans les deux cas aurait semblé la correction évidente. Elle aurait
 * cassé le catalogue de ~93 % des sources (mesuré sur 82 sources : seules 6
 * annoncent SAFE ou SUGGESTIVE, 39 n'annoncent rien). Ces tests figent donc
 * **les trois comportements distincts**, pour que personne ne les confonde de
 * nouveau :
 *
 * 1. valeur inconnue → refus (fuite de contenu par faute de frappe) ;
 * 2. source qui annonce des classifications, pas celle-là → refus (la source
 *    peut filtrer, elle choisit de ne pas le faire) ;
 * 3. source qui n'annonce rien → pas de filtre, **et c'est assumé** : le
 *    troisième test verrouille ce comportement pour qu'une « correction »
 *    ultérieure ait à le lire.
 */
// Construire `MangaListFilterOptions` à la main demande l'opt-in : l'API des
// options est interne aux parsers. Même annotation que `ErreursTest.kt:23`.
@OptIn(org.koitharu.kotatsu.parsers.InternalParsersApi::class)
class RequestedRatingsTest {

    private fun params(vararg paires: Pair<String, Any>): JSONObject = JSONObject().apply {
        paires.forEach { (k, v) -> put(k, v) }
    }

    private fun options(vararg annoncees: ContentRating) = MangaListFilterOptions(
        availableTags = emptySet(),
        availableStates = emptySet(),
        availableContentRating = annoncees.toSet(),
        availableContentTypes = emptySet(),
        availableDemographics = emptySet(),
        availableLocales = emptySet(),
    )

    // ---------- 1. Une valeur que l'enum ne connaît pas est refusée ----------

    @Test
    fun `une classification inconnue est refusee, pas ignoree`() {
        // Le défaut : `"safee"` retombait sur `null` → aucun filtre, donc le
        // contenu que l'appelant voulait exclure était livré quand même.
        val e = assertFailsWith<IllegalArgumentException>(
            "une faute de frappe ne doit pas devenir une fuite de contenu",
        ) {
            requestedRatings(params("contentRating" to "safee"), options(ContentRating.SAFE), "SRC")
        }
        assertTrue(
            (e.message ?: "").contains("safee"),
            "le message doit nommer la valeur refusée, reçu : ${e.message}",
        )
    }

    @Test
    fun `une classification vide veut dire aucune, et reste acceptee`() {
        // L'interface envoie `""` pour son réglage « Toutes ». Une chaîne vide
        // n'est pas une faute : c'est l'absence de filtre, demandée.
        assertEquals(
            emptySet(),
            requestedRatings(params("contentRating" to ""), options(ContentRating.SAFE), "SRC"),
        )
    }

    @Test
    fun `la casse de la valeur nest pas decisive`() {
        assertEquals(
            setOf(ContentRating.SAFE),
            requestedRatings(params("contentRating" to "safe"), options(ContentRating.SAFE), "SRC"),
        )
    }

    // ---------- 2. La source expose des classifications, pas celle-là ----------

    @Test
    fun `une classification exposee est appliquee`() {
        assertEquals(
            setOf(ContentRating.SUGGESTIVE),
            requestedRatings(
                params("contentRating" to "SUGGESTIVE"),
                options(ContentRating.SAFE, ContentRating.SUGGESTIVE, ContentRating.ADULT),
                "SRC",
            ),
        )
    }

    @Test
    fun `une classification non exposee est refusee, avec ce que la source expose`() {
        // Le seul cas où refuser est possible sans rien casser : la source sait
        // filtrer, elle annonce d'autres classifications. Livrer quand même
        // remettrait de l'adulte à quelqu'un qui a demandé de l'exclure.
        val e = assertFailsWith<IllegalArgumentException>(
            "la source expose ADULT seulement ; demander SAFE doit échouer",
        ) {
            requestedRatings(params("contentRating" to "SAFE"), options(ContentRating.ADULT), "SRC")
        }
        assertTrue(
            (e.message ?: "").contains("ADULT"),
            "le message doit dire ce que la source expose, reçu : ${e.message}",
        )
    }

    @Test
    fun `safeOnly reste le defaut quand rien nest demande`() {
        assertEquals(
            setOf(ContentRating.SAFE),
            requestedRatings(JSONObject(), options(ContentRating.SAFE), "SRC"),
        )
    }

    // ---------- 3. La source n'annonce rien : pas de filtre, et on le dit ----

    @Test
    fun `une source qui nannonce rien ne filtre pas et ne refuse pas`() {
        // Comportement ASSUMÉ, et verrouillé par ce test. La correction naïve —
        // refuser — casserait 39 sources sur 82 (mesuré 2026-10-10).
        assertEquals(
            emptySet(),
            requestedRatings(
                params("contentRating" to "SUGGESTIVE"),
                options(),
                "MANGABUFF",
            ),
        )
    }

    @Test
    fun `contre-epreuve le refus de la valeur inconnue est bien ce qui protège`() {
        // Si les deux assertions passent, c'est que la branche de refus fait
        // réellement quelque chose : la même source, la même demande, et un
        // verdict opposé selon que la valeur est dans l'enum.
        val inconnue = runCatching {
            requestedRatings(params("contentRating" to "SAFEE"), options(ContentRating.SAFE), "SRC")
        }
        val connue = runCatching {
            requestedRatings(params("contentRating" to "SAFE"), options(ContentRating.SAFE), "SRC")
        }
        assertTrue(inconnue.isFailure, "« SAFEE » doit être refusé")
        assertTrue(connue.isSuccess, "« SAFE » doit passer : le test distingue les deux cas")
    }
}
