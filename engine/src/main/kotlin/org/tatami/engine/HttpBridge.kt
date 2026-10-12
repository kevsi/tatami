package org.tatami.engine

import okhttp3.MediaType.Companion.toMediaTypeOrNull
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import java.io.IOException
import java.net.InetAddress
import java.net.Proxy
import java.net.SocketException
import java.net.UnknownHostException
import javax.net.ssl.SSLException

private object AdresseReseauPublique {
    private data class Prefixe(val adresse: ByteArray, val bits: Int)

    private val prefixesIpv4NonPublics = listOf(
        Prefixe(byteArrayOf(0, 0, 0, 0), 8),
        Prefixe(byteArrayOf(10, 0, 0, 0), 8),
        Prefixe(byteArrayOf(100, 64, 0, 0), 10),
        Prefixe(byteArrayOf(127, 0, 0, 0), 8),
        Prefixe(byteArrayOf(169.toByte(), 254.toByte(), 0, 0), 16),
        Prefixe(byteArrayOf(172.toByte(), 16, 0, 0), 12),
        Prefixe(byteArrayOf(192.toByte(), 0, 0, 0), 24),
        Prefixe(byteArrayOf(192.toByte(), 0, 2, 0), 24),
        Prefixe(byteArrayOf(192.toByte(), 88.toByte(), 99, 0), 24),
        Prefixe(byteArrayOf(192.toByte(), 168.toByte(), 0, 0), 16),
        Prefixe(byteArrayOf(198.toByte(), 18, 0, 0), 15),
        Prefixe(byteArrayOf(198.toByte(), 51, 100, 0), 24),
        Prefixe(byteArrayOf(203.toByte(), 0, 113, 0), 24),
        Prefixe(byteArrayOf(224.toByte(), 0, 0, 0), 4),
        Prefixe(byteArrayOf(240.toByte(), 0, 0, 0), 4),
    )

    private val prefixesIpv6NonPublics = listOf(
        Prefixe(byteArrayOf(0x20, 0x01, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0), 23),
        Prefixe(byteArrayOf(0x20, 0x02, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0), 16),
    )
    private val prefixeIpv6Global = Prefixe(
        byteArrayOf(0x20, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0),
        3,
    )

    fun estPublique(adresse: InetAddress): Boolean {
        if (
            adresse.isAnyLocalAddress ||
            adresse.isLoopbackAddress ||
            adresse.isLinkLocalAddress ||
            adresse.isSiteLocalAddress ||
            adresse.isMulticastAddress
        ) {
            return false
        }

        val octets = adresse.address
        return when (octets.size) {
            4 -> prefixesIpv4NonPublics.none { correspondAuPrefixe(octets, it) }
            16 -> {
                correspondAuPrefixe(octets, prefixeIpv6Global) &&
                    prefixesIpv6NonPublics.none { correspondAuPrefixe(octets, it) }
            }
            else -> false
        }
    }

    private fun correspondAuPrefixe(adresse: ByteArray, prefixe: Prefixe): Boolean {
        val octetsComplets = prefixe.bits / 8
        for (index in 0 until octetsComplets) {
            if (adresse[index] != prefixe.adresse[index]) return false
        }
        val bitsRestants = prefixe.bits % 8
        if (bitsRestants == 0) return true

        val masque = (0xFF shl (8 - bitsRestants)) and 0xFF
        return ((adresse[octetsComplets].toInt() xor prefixe.adresse[octetsComplets].toInt()) and masque) == 0
    }
}

/**
 * Expose `http.get(url)` et `http.post(url, body)` aux scripts Nashorn.
 *
 * Règles de sécurité (ADR 0010) :
 * - Seules les URLs HTTPS et les résolutions DNS vers des adresses publiques sont acceptées.
 * - Les requêtes de ce bridge contournent les proxys JVM et n'utilisent que le réseau direct.
 * - Le corps de réponse est limité à MAX_BODY_BYTES (5 Mo).
 * - Le timeout est celui de l'OkHttpClient injecté (30 s read).
 *
 * Nashorn conserve son interop Java : cette politique ne protège que les requêtes
 * qui passent par `HttpBridge` ; le sandbox des scripts reste contournable.
 *
 * Cette classe est instanciée une fois par script et injectée dans le contexte
 * Nashorn sous le nom `http`. Les méthodes sont appelées depuis JS :
 *   const html = http.get("https://example.com/");
 *   const json = http.post("https://example.com/api", "{\"q\":\"test\"}");
 */
class HttpBridge(private val client: OkHttpClient) {

    companion object {
        private const val MAX_BODY_BYTES = 5 * 1024 * 1024L // 5 Mo
        private const val HTTPS_PREFIX = "https://"
    }

    private val clientBridge = client.newBuilder()
        .proxy(Proxy.NO_PROXY)
        .dns { hote ->
            val adresses = client.dns.lookup(hote)
            if (adresses.isEmpty() || adresses.any { !AdresseReseauPublique.estPublique(it) }) {
                throw UnknownHostException("Adresse réseau non publique refusée")
            }
            adresses
        }
        .addNetworkInterceptor { chain ->
            if (!chain.request().url.isHttps) {
                throw IOException("HttpBridge refuse une redirection qui quitte HTTPS")
            }
            chain.proceed(chain.request())
        }
        .build()

    /**
     * Effectue un GET HTTPS et retourne le corps en texte.
     * @throws IllegalArgumentException si l'URL n'est pas HTTPS.
     * @throws IOException si la requête échoue ou si le corps dépasse MAX_BODY_BYTES.
     */
    fun get(url: String): String {
        validerUrl(url)
        val request = Request.Builder().url(url).get().build()
        return executer(request)
    }

    /**
     * Effectue un POST HTTPS avec un corps `application/json` et retourne le texte.
     * @throws IllegalArgumentException si l'URL n'est pas HTTPS.
     * @throws IOException si la requête échoue ou si le corps dépasse MAX_BODY_BYTES.
     */
    fun post(url: String, body: String): String {
        validerUrl(url)
        val reqBody = body.toRequestBody("application/json; charset=utf-8".toMediaTypeOrNull())
        val request = Request.Builder().url(url).post(reqBody).build()
        return executer(request)
    }

    private fun validerUrl(url: String) {
        require(url.startsWith(HTTPS_PREFIX)) {
            "HttpBridge n'accepte que les URLs https:// — reçu : ${url.take(80)}"
        }
    }

    private fun executer(request: Request): String {
        val response = try {
            clientBridge.newCall(request).execute()
        } catch (e: IOException) {
            val causeReseau = generateSequence(e as Throwable?) { it.cause }
                .firstOrNull {
                    it is UnknownHostException || it is SocketException || it is SSLException
                }
            if (causeReseau != null) {
                throw IOException(
                    "Réseau direct requis pour les sources scriptées (${causeReseau.javaClass.simpleName})",
                    e,
                )
            }
            throw e
        }
        response.use {
            if (!response.isSuccessful) {
                throw IOException("HTTP ${response.code} pour ${request.url}")
            }
            val body = response.body
                ?: throw IOException("Corps vide pour ${request.url}")
            // Lire avec un plafond pour éviter de charger un fichier massif en RAM.
            val bytes = body.byteStream().readNBytes(MAX_BODY_BYTES.toInt() + 1)
            if (bytes.size > MAX_BODY_BYTES) {
                throw IOException("Corps dépasse $MAX_BODY_BYTES octets pour ${request.url}")
            }
            return bytes.toString(Charsets.UTF_8)
        }
    }
}
