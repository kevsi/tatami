package org.tatami.engine

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.runInterruptible
import kotlinx.coroutines.sync.withLock
import org.json.JSONArray
import org.json.JSONObject
import org.openjdk.nashorn.api.scripting.ClassFilter
import org.openjdk.nashorn.api.scripting.NashornScriptEngineFactory
import javax.script.Invocable
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

    /**
     * Un seul appel à la fois sur ce script.
     *
     * Le défaut mesuré le 2026-10-09 : une instance de moteur Nashorn **unique**
     * par script, invoquée depuis `Dispatchers.Default` — donc depuis n'importe
     * quel thread du pool non borné (`Server.kt:329`). Nashorn n'est pas
     * thread-safe. Le chemin **natif** règle ce problème : `ParserPool.avec`
     * prend un `Mutex` par source (`ParserPool.kt:111`), et un test le prouve
     * (`ParserPoolPaginationTest.kt:84`). Le chemin scripté n'avait rien.
     *
     * Scénario, **déjà présent en production** : ouvrir une série d'une source
     * scriptée lance deux appels sur le même `ScriptedParser`. `page.tsx:2397`
     * appelle `details` en `void …then(…)`, `page.tsx:2422` attend `chapters`, et
     * `chapters` appelle **aussi** `getDetails` (`Server.kt:560`). Deux
     * `invokeMethod("SOURCE", "getDetails", …)` simultanés sur le même moteur.
     *
     * Résultat indéfini : valeur fausse, `ConcurrentModificationException`, ou
     * moteur corrompu. Le symptôme se limite à une source scriptée, donc il est
     * lu comme « cette source marche par fois » — exactement le genre de panne
     * qu'on attribue au site et pas au client.
     *
     * Coût : les requêtes d'un même script se sérialisent. Accepté, parce que
     * c'est ce que fait déjà le chemin natif pour la même raison, et parce que le
     * défaut est silencieux. Un débit par source est préservé ailleurs
     * (`RECENT_INTERVALLE_MS`).
     */
    private val verrou = kotlinx.coroutines.sync.Mutex()

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
        // Une **factory**, pas `ScriptEngineManager` : c'est la factory qui porte
        // l'option `ClassFilter`, qui est un **paramètre** de `getScriptEngine`
        // (vérifié sur le JAR 15.6), donc inaccessible depuis un moteur déjà
        // construit.
        val factory = NashornScriptEngineFactory()
        val nashorn = factory.getScriptEngine(
            // `--no-java` passe ici, **dans les arguments de création** : mesuré le
            // 2026-10-15, `nashorn.put("--no-java", true)` après coup ne servait à
            // rien — Nashorn lit l'option une seule fois, à la construction, donc
            // `Java.type` restait joignable et le test rouge le disait.
            //
            // Ces arguments sont **ajoutés** aux valeurs par défaut de Nashorn
            // (`-strict`, `-scripting`, `--no-syntax-extensions`, `es6`), donc
            // `--strict` reste actif — c'est lui qui refuse `globalThis` plus bas.
            arrayOf("--no-java"),
            null,
            // Deuxième barrière : même si `--no-java` était relâché par une
            // version de Nashorn, un `ClassFilter` n'expose que le paquet du
            // moteur. Il est un **paramètre** de `getScriptEngine`, pas une
            // propriété de la factory (vérifié sur le JAR 15.6).
            ClassFilter { nom -> nom.startsWith("org.tatami.engine.") },
        )

        // `--no-java` : `Java.type`, `JavaImporter` et `Packages.*` deviennent
        // inaccessibles. C'est la barrière principale. Mesuré le 2026-10-15 :
        // sans elle, `Java.type("java.lang.Runtime").getRuntime().exec(...)`
        // s'exécutait avec les droits de l'utilisateur, au **démarrage** du
        // moteur (`serve()` évalue tous les `.js` du dossier de sources), donc
        // avant la première requête — et toute la politique SSRF de `HttpBridge`,
        // qui est le meilleur morceau de sécurité du dépôt, devenait contournable
        // depuis l'intérieur du processus.
        //
        // Portée volontairement étroite : seul le script **fourni par
        // l'utilisateur** est confiné. Le JS de `EngineContext.evaluateJs` vient
        // des parseurs épinglés, donc d'un artefact vérifié ; lui retirer
        // l'interop casserait de la recomposition d'images pour rien.
        // Nashorn ajoute `-scripting` **quelle que soit** la factory (vérifié sur
        // le JAR 15.6 : `getScriptEngine()` force `--strict`, `-scripting`,
        // `--no-syntax-extensions` et `es6`). Cinq globaux survivent donc à
        // `--no-java` : `load` et `loadWithNewGlobal` évaluent un fichier du
        // disque, et `print`/`exit`/`quit` écrivent sur la **sortie standard** —
        // c'est-à-dire dans le flux NDJSON du protocole, ce qui suffit à corrompre
        // une réponse sans jamais approcher Java. Ils sont retirés explicitement.
        for (nom in listOf("load", "loadWithNewGlobal", "exit", "quit", "print")) {
            // `delete <nom>` et non `delete globalThis.<nom>` : Nashorn force
            // `--strict`, qui refuse `globalThis` (mesuré 2026-10-15).
            nashorn.eval("delete $nom")
        }

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
        verrou.withLock {
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
}
