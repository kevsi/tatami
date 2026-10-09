package org.tatami.engine

import java.io.File
import java.net.ServerSocket
import java.nio.file.Files
import java.util.concurrent.atomic.AtomicInteger
import kotlinx.coroutines.runBlocking
import okhttp3.OkHttpClient
import org.json.JSONObject
import org.junit.jupiter.api.Assertions.assertFalse
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Test

/**
 * `fetchImage` ne doit jamais atteindre le réseau local.
 *
 * Mesuré le 2026-10-08 : dans `dispatchCustom`, `fetchCover` exigeait
 * `https://` (`takeIf { it.startsWith("https://") }` … 12 lignes plus haut) et
 * `fetchImage` **rien du tout** : `p.reqString("pageUrl")` partait directement
 * dans `Request.Builder().url(pageUrl)`. Un script `.js` déposé par
 * l'utilisateur — donc du code que le moteur exécute — pouvait donc faire
 * sortir une requête vers `http://127.0.0.1:…`, `http://192.168.0.1/…` ou la
 * métadonnée d'instance d'un hébergeur, et les octets revenaient en base64
 * pour être écrits dans un `.cbz`.
 *
 * Le test ne se contente pas d'attendre une exception : « connexion refusée »
 * et « refusé par la validation » lèvent tous deux. Il vérifie le **témoin** —
 * un vrai serveur HTTP local attend sur `127.0.0.1`, et le test exige qu'il
 * n'ait reçu **aucune** requête. C'est ce qui distingue « on a refusé » de
 * « on a essayé et ça a échoué », et c'est ce qui rend le test rouge avec
 * l'ancien code au lieu de passer pour la mauvaise raison.
 */
class FetchImageReseauLocalTest {

    private val client = OkHttpClient()

    private fun scriptMinimal(id: String) = """
        var SOURCE = {
            id:      "$id",
            name:    "Source Test",
            lang:    "fr",
            baseUrl: "https://example.com",
            getList: function(page, query) { return JSON.stringify([]); },
            getDetails: function(mangaUrl) { return JSON.stringify({}); },
            getPages: function(chapterUrl) { return JSON.stringify([]); }
        };
    """.trimIndent()

    /**
     * Un serveur HTTP minimal sur `127.0.0.1`, qui compte les requêtes reçues
     * et répond `200` avec un corps JPEG minuscule.
     *
     * Le compteur est l'assertion : sans lui, une connexion refusée et un refus
     * de validation seraient indiscernables.
     */
    private fun serveurLocal(): Pair<Int, AtomicInteger> {
        val socket = ServerSocket(0, 1, java.net.InetAddress.getLoopbackAddress())
        val requetes = AtomicInteger(0)
        val thread = Thread {
            try {
                socket.accept().use { connexion ->
                    val entree = connexion.getInputStream().bufferedReader()
                    // Une ligne de requête suffit : on répond sans lire le corps.
                    entree.readLine()
                    requetes.incrementAndGet()
                    val corps = ByteArray(3) { 0xFF.toByte() }
                    connexion.outputStream.apply {
                        write(
                            (
                                "HTTP/1.1 200 OK\r\n" +
                                    "Content-Type: image/jpeg\r\n" +
                                    "Content-Length: ${corps.size}\r\n" +
                                    "Connection: close\r\n\r\n"
                                ).toByteArray(),
                        )
                        write(corps)
                        flush()
                    }
                }
            } catch (_: Exception) {
                // Le socket est fermé à la fin du test : rien à signaler.
            }
        }.apply { isDaemon = true }
        thread.start()
        Runtime.getRuntime().addShutdownHook(Thread { runCatching { socket.close() } })
        return socket.localPort to requetes
    }

    @Test
    fun `fetchImage ne fait aucune requete vers la boucle locale`() {
        val (port, requetes) = serveurLocal()
        val dossier = Files.createTempDirectory("tatami-test-ssrf").toFile()
        try {
            val loader = ScriptedSourceLoader(dossier, client)
            loader.charger(File(dossier, "ma-source.js").also { it.writeText(scriptMinimal("ma-source")) })

            var erreur: Throwable? = null
            try {
                runBlocking {
                    dispatchCustom(
                        loader, SessionCache(), "custom:ma-source", "fetchImage",
                        JSONObject().put("pageUrl", "http://127.0.0.1:$port/image.jpg"),
                        EngineContext(),
                    )
                }
            } catch (e: Throwable) {
                erreur = e
            }

            assertTrue(
                erreur != null,
                "fetchImage vers la boucle locale doit être refusé, or la requête a abouti",
            )
            assertFalse(
                erreur is java.io.IOException,
                "le refus doit venir de la validation d'URL, pas d'une connexion : $erreur",
            )
            assertTrue(
                requetes.get() == 0,
                "le serveur local a reçu ${requetes.get()} requête(s) : la validation " +
                    "d'URL n'a pas été appliquée avant le départ (avant le correctif : 1)",
            )
        } finally {
            dossier.deleteRecursively()
        }
    }

    @Test
    fun `une URL publique passe la validation et echoue plus tard, sur le reseau`() {
        val dossier = Files.createTempDirectory("tatami-test-ssrf-ok").toFile()
        try {
            val loader = ScriptedSourceLoader(dossier, client)
            loader.charger(File(dossier, "ma-source.js").also { it.writeText(scriptMinimal("ma-source")) })

            var erreur: Throwable? = null
            try {
                runBlocking {
                    dispatchCustom(
                        loader, SessionCache(), "custom:ma-source", "fetchImage",
                        // `.invalid` est réservé (RFC 2606) : jamais résolu, donc
                        // l'échec vient du DNS et non de la validation.
                        JSONObject().put("pageUrl", "https://exemple.invalid/image.jpg"),
                        EngineContext(),
                    )
                }
            } catch (e: Throwable) {
                erreur = e
            }

            // Le contrôle qui compte : l'erreur doit être **réseau**, pas
            // `ParamException`. Sans ce test, une validation trop severe
            // casserait toutes les images des sources perso sans qu'aucun test
            // ne le remarque — « une porte qui ne peut pas crier ».
            assertTrue(
                erreur is java.io.IOException,
                "une URL publique doit traverser la validation et échouer sur le " +
                    "réseau ; obtenu : ${erreur?.javaClass?.name} : $erreur",
            )
        } finally {
            dossier.deleteRecursively()
        }
    }

    @Test
    fun `les plages privees sont refusees meme en https`() {
        val refus = listOf(
            "https://192.168.0.1/admin",
            "https://10.0.0.5/image.jpg",
            "https://172.16.0.1/",
            "https://127.0.0.1/image.jpg",
            "https://169.254.169.254/latest/meta-data/",
            "http://example.com/image.jpg",
            "https://localhost/image.jpg",
        )
        for (url in refus) {
            val dossier = Files.createTempDirectory("tatami-test-ssrf-priv").toFile()
            try {
                val loader = ScriptedSourceLoader(dossier, client)
                loader.charger(
                    File(dossier, "ma-source.js").also { it.writeText(scriptMinimal("ma-source")) },
                )
                var erreur: Throwable? = null
                try {
                    runBlocking {
                        dispatchCustom(
                            loader, SessionCache(), "custom:ma-source", "fetchImage",
                            JSONObject().put("pageUrl", url),
                            EngineContext(),
                        )
                    }
                } catch (e: Throwable) {
                    erreur = e
                }
                // `ParamException` est `private` dans son fichier, donc on ne
                // peut pas le nommer ici. Sa superclasse publique suffit : un
                // refus de validation est un `IllegalArgumentException`, un
                // échec réseau est un `IOException`, et les deux sont disjoints.
                assertTrue(
                    erreur is IllegalArgumentException,
                    "$url doit être refusé par la validation ; obtenu : $erreur",
                )
            } finally {
                dossier.deleteRecursively()
            }
        }
    }

    @Test
    fun `fetchCover refuse aussi la boucle locale`() {
        val (port, requetes) = serveurLocal()
        val dossier = Files.createTempDirectory("tatami-test-ssrf-couv").toFile()
        try {
            val loader = ScriptedSourceLoader(dossier, client)
            loader.charger(File(dossier, "ma-source.js").also { it.writeText(scriptMinimal("ma-source")) })

            var erreur: Throwable? = null
            try {
                runBlocking {
                    dispatchCustom(
                        loader, SessionCache(), "custom:ma-source", "fetchCover",
                        JSONObject()
                            .put("mangaUrl", "/manga/1")
                            .put("coverUrl", "http://127.0.0.1:$port/couv.jpg"),
                        EngineContext(),
                    )
                }
            } catch (e: Throwable) {
                erreur = e
            }

            assertTrue(erreur != null, "fetchCover vers la boucle locale doit être refusé")
            assertTrue(
                requetes.get() == 0,
                "le serveur local a reçu ${requetes.get()} requête(s) sur fetchCover",
            )
        } finally {
            dossier.deleteRecursively()
        }
    }
}
