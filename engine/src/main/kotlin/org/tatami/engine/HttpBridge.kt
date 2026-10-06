package org.tatami.engine

import okhttp3.MediaType.Companion.toMediaTypeOrNull
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import java.io.IOException

/**
 * Expose `http.get(url)` et `http.post(url, body)` aux scripts Nashorn.
 *
 * Règles de sécurité (ADR 0010) :
 * - Seules les URLs `https://` sont acceptées (pas de réseau local).
 * - Le corps de réponse est limité à MAX_BODY_BYTES (5 Mo).
 * - Le timeout est celui de l'OkHttpClient injecté (30 s read).
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
        client.newCall(request).execute().use { response ->
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
