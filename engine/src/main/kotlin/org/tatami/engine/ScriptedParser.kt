package org.tatami.engine

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.runInterruptible
import org.json.JSONArray
import org.json.JSONObject
import javax.script.Invocable
import javax.script.ScriptEngineManager
import javax.script.ScriptException

/**
 * Métadonnées extraites du bloc `SOURCE` déclaré dans un script.
 */
data class ScriptedSourceMeta(
    val id: String,
    val name: String,
    val lang: String,
    val baseUrl: String,
)

/**
 * Erreurs propres au cycle de vie d'un script.
 */
class ScriptedSourceException(message: String, cause: Throwable? = null) :
    RuntimeException(message, cause)

/**
 * Charge et exécute un script de source personnalisée via Nashorn.
 *
 * Contrat du script (ADR 0010) : le script doit définir un objet global `SOURCE`
 * avec les champs `id`, `name`, `lang`, `baseUrl` et les méthodes asynchrones
 * `getList(page, query)`, `getDetails(mangaUrl)`, `getPages(chapterUrl)`.
 *
 * L'objet `http` (HttpBridge) est injecté dans le contexte avant l'évaluation.
 *
 * Limite de taille : MAX_SCRIPT_BYTES (256 Ko) vérifiée par ScriptedSourceLoader.
 */
class ScriptedParser(
    private val scriptContent: String,
    private val httpBridge: HttpBridge,
) {
    companion object {
        const val MAX_SCRIPT_BYTES = 256 * 1024

        private val METHODES_REQUISES = listOf("getList", "getDetails", "getPages")
    }

    private val engine by lazy { creerMoteur() }

    /** Métadonnées lues depuis `SOURCE` après évaluation du script. */
    val meta: ScriptedSourceMeta by lazy { lireMeta() }

    // -------------------------------------------------------------------------
    // API publique appelée depuis ScriptedSourceLoader / Server
    // -------------------------------------------------------------------------

    /**
     * Retourne une liste de mangas (page 1-based, query peut être vide).
     * Le script doit retourner un Array JS de `{id, title, coverUrl, url}`.
     */
    suspend fun getList(page: Int, query: String): JSONArray =
        appellerAsync("getList", page, query)

    /**
     * Retourne les détails d'une série + liste de chapitres.
     * Le script doit retourner `{title, description, coverUrl, status, authors, tags, chapters}`.
     */
    suspend fun getDetails(mangaUrl: String): JSONObject =
        appellerAsync("getDetails", mangaUrl)

    /**
     * Retourne la liste des pages d'un chapitre `[{id, url}]`.
     */
    suspend fun getPages(chapterUrl: String): JSONArray =
        appellerAsync("getPages", chapterUrl)

    // -------------------------------------------------------------------------
    // Internals
    // -------------------------------------------------------------------------

    private fun creerMoteur(): Invocable {
        val manager = ScriptEngineManager()
        val nashorn = manager.getEngineByName("nashorn")
            ?: throw ScriptedSourceException("Nashorn non disponible sur cette JVM")

        // Injecter le bridge HTTP avant d'évaluer le script.
        nashorn.put("http", httpBridge)

        try {
            nashorn.eval(scriptContent)
        } catch (e: ScriptException) {
            throw ScriptedSourceException("Erreur d'évaluation du script : ${e.message}", e)
        }

        // Vérifier que SOURCE existe et possède les méthodes requises.
        val source = nashorn.get("SOURCE")
            ?: throw ScriptedSourceException("Le script ne définit pas d'objet global `SOURCE`")

        for (methode in METHODES_REQUISES) {
            // On vérifie via eval pour rester indépendant du type interne Nashorn.
            val existe = nashorn.eval("typeof SOURCE.$methode === 'function'") as? Boolean ?: false
            if (!existe) {
                throw ScriptedSourceException("SOURCE.$methode() est absent ou n'est pas une fonction")
            }
        }

        return nashorn as Invocable
    }

    private fun lireMeta(): ScriptedSourceMeta {
        // engine est lazy : ce getter déclenche creerMoteur() si nécessaire.
        fun lire(champ: String): String {
            val valeur = (engine as javax.script.ScriptEngine).eval("SOURCE.$champ")?.toString()
            return valeur?.takeIf { it.isNotBlank() && it != "undefined" && it != "null" }
                ?: throw ScriptedSourceException("SOURCE.$champ est absent ou vide")
        }
        return ScriptedSourceMeta(
            id = lire("id"),
            name = lire("name"),
            lang = lire("lang"),
            baseUrl = lire("baseUrl"),
        )
    }

    /**
     * Appelle une méthode async JS et attend la Promise résultante.
     *
     * Nashorn ne gère pas nativement les Promises ES6, mais les scripts peuvent
     * retourner un objet `then`able ou simplement une valeur synchrone.
     * Pour simplifier : on exige que les méthodes retournent des valeurs
     * synchrones (JSONObject/JSONArray sérialisés en String puis reparsés), ou
     * des `java.util.concurrent.Future` si le script utilise notre utilitaire.
     *
     * En pratique, la majorité des cas HTTP sont gérés par `http.get()` qui est
     * synchrone depuis le point de vue du script — pas de vraie asynchronie JS
     * requise.
     */
    private suspend inline fun <reified T> appellerAsync(methode: String, vararg args: Any?): T =
        runInterruptible(Dispatchers.Default) {
            val resultat = try {
                engine.invokeMethod(
                    (engine as javax.script.ScriptEngine).get("SOURCE"),
                    methode,
                    *args,
                )
            } catch (e: javax.script.ScriptException) {
                throw ScriptedSourceException("Erreur dans SOURCE.$methode() : ${e.message}", e)
            } catch (e: NoSuchMethodException) {
                throw ScriptedSourceException("SOURCE.$methode() introuvable", e)
            }

            // Le script retourne une String (JSON sérialisé) ou un objet JS.
            // On normalise en String puis on parse selon T.
            val json = resultat?.toString()
                ?: throw ScriptedSourceException("SOURCE.$methode() a retourné null")

            @Suppress("UNCHECKED_CAST")
            when (T::class) {
                JSONArray::class -> JSONArray(json) as T
                JSONObject::class -> JSONObject(json) as T
                else -> throw ScriptedSourceException("Type de retour non supporté : ${T::class}")
            }
        }
}
