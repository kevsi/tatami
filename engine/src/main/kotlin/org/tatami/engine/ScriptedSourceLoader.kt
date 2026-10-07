package org.tatami.engine

import okhttp3.OkHttpClient
import java.io.File
import java.nio.file.FileSystems
import java.nio.file.Path
import java.nio.file.StandardWatchEventKinds.*
import java.util.concurrent.ConcurrentHashMap
import java.util.logging.Logger

/**
 * Identité d'une source scriptée : le **nom du fichier** sans `.js`.
 *
 * C'est le nom de fichier qui fait foi, pas `SOURCE.id`. Rust enregistre la
 * ligne en base sous ce même nom (`custom_source.rs`, `id = nom sans .js`) :
 * les deux doivent concorder **par construction**, pas par chance.
 *
 * Mesuré le 2026-10-07 : `docs/exemple-source.js` déclare `id: "exemple-fr"`, et
 * le registre indexé sur `meta.id` faisait échouer la suppression — le handler
 * cherchait `exemple-fr.js`, un fichier qui n'a jamais existé. Résultat : la
 * source disparaissait de Réglages (base + fichier) et restait **fantôme** dans
 * la grille du catalogue jusqu'au redémarrage, et les favoris/historique
 * référençaient `custom:exemple-fr` — un identifiant que la base ignore.
 *
 * `SOURCE.id` reste lu et validé (un script qui ne le déclare pas est refusé) :
 * il documente le script, il n'identifie plus la source.
 */
fun identiteDe(fichier: File): String = fichier.nameWithoutExtension

/**
 * Scanne un dossier pour les scripts de sources personnalisées (*.js),
 * les charge via ScriptedParser, et maintient un registre rechargeable à chaud.
 *
 * Utilisé par Server.kt pour enrichir listSources avec les sources perso.
 *
 * Cycle de vie :
 *   val loader = ScriptedSourceLoader(File("..."), httpClient)
 *   loader.demarrer()          // charge les scripts + démarre le watcher
 *   loader.sources()           // snapshot thread-safe
 *   loader.arreter()           // arrête le watcher
 */
class ScriptedSourceLoader(
    private val dossier: File,
    private val httpClient: OkHttpClient,
) {
    private val log = Logger.getLogger(ScriptedSourceLoader::class.java.name)

    // sourceId → ScriptedParser (thread-safe)
    private val parsers = ConcurrentHashMap<String, ScriptedParser>()

    @Volatile
    private var watcherThread: Thread? = null

    @Volatile
    private var actif = false

    // -------------------------------------------------------------------------
    // API publique
    // -------------------------------------------------------------------------

    /** Charge tous les scripts présents et démarre la surveillance du dossier. */
    fun demarrer() {
        dossier.mkdirs()
        chargerTous()
        actif = true
        watcherThread = Thread(::boucleWatcher, "scripted-source-watcher").apply {
            isDaemon = true
            start()
        }
    }

    /** Arrête la surveillance. */
    fun arreter() {
        actif = false
        watcherThread?.interrupt()
        watcherThread = null
    }

    /**
     * Retourne un snapshot des parsers chargés avec succès.
     * Chaque entrée est (sourceId → ScriptedParser).
     */
    fun sources(): Map<String, ScriptedParser> = HashMap(parsers)

    /**
     * Charge ou recharge un script par son fichier.
     * Appelé depuis l'extérieur quand Rust importe un nouveau `.js`.
     */
    fun charger(fichier: File): ScriptedParser {
        val contenu = lireContenu(fichier)
        val bridge = HttpBridge(httpClient)
        val parser = ScriptedParser(contenu, bridge)
        // meta est lazy — on force l'évaluation ici pour valider le script
        // avant d'annoncer le succès.
        val meta = parser.meta
        val cle = identiteDe(fichier)
        require(cle.isNotBlank()) { "nom de fichier sans identifiant : ${fichier.name}" }
        parsers[cle] = parser
        log.info(
            "Source personnalisée chargée : $cle " +
                "(déclarée « ${meta.id} », ${meta.name}, ${meta.lang})"
        )
        return parser
    }

    /**
     * Supprime un script du registre (le fichier est supprimé par Rust).
     */
    fun decharger(sourceId: String) {
        parsers.remove(sourceId)
        log.info("Source personnalisée retirée : $sourceId")
    }

    // -------------------------------------------------------------------------
    // Internals
    // -------------------------------------------------------------------------

    private fun chargerTous() {
        val fichiers = dossier.listFiles { f -> f.extension == "js" } ?: return
        for (fichier in fichiers) {
            try {
                charger(fichier)
            } catch (e: ScriptedSourceException) {
                log.warning("Script ignoré (${fichier.name}) : ${e.message}")
            } catch (e: Exception) {
                log.warning("Erreur inattendue pour ${fichier.name} : ${e.message}")
            }
        }
    }

    private fun lireContenu(fichier: File): String {
        val taille = fichier.length()
        require(taille <= ScriptedParser.MAX_SCRIPT_BYTES) {
            "Le script ${fichier.name} dépasse ${ScriptedParser.MAX_SCRIPT_BYTES} octets (taille : $taille)"
        }
        return fichier.readText(Charsets.UTF_8)
    }

    private fun boucleWatcher() {
        val watchService = FileSystems.getDefault().newWatchService()
        val cle = dossier.toPath().register(watchService, ENTRY_CREATE, ENTRY_MODIFY, ENTRY_DELETE)
        try {
            while (actif) {
                val key = watchService.poll(2, java.util.concurrent.TimeUnit.SECONDS) ?: continue
                for (event in key.pollEvents()) {
                    val chemin = event.context() as? Path ?: continue
                    val fichier = dossier.resolve(chemin.toString())
                    if (!fichier.name.endsWith(".js")) continue
                    when (event.kind()) {
                        ENTRY_CREATE, ENTRY_MODIFY -> {
                            try {
                                charger(fichier)
                            } catch (e: Exception) {
                                log.warning("Rechargement échoué (${fichier.name}) : ${e.message}")
                            }
                        }
                        ENTRY_DELETE -> {
                            // Le registre est indexé sur le nom de fichier : la
                            // clé est directe. L'ancien code reconstruisait
                            // `${meta.id}.js` et comparait des `canonicalPath`,
                            // donc une suppression ne déchargeait rien si le
                            // script déclarait un `id` ≠ son nom de fichier.
                            decharger(identiteDe(fichier))
                        }
                    }
                }
                key.reset()
            }
        } catch (_: InterruptedException) {
            // Arrêt normal.
        } finally {
            cle.cancel()
            watchService.close()
        }
    }
}
