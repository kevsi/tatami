package org.tatami.engine

import java.io.ByteArrayOutputStream
import java.util.zip.GZIPOutputStream
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertNull
import kotlin.test.assertTrue
import okhttp3.Cookie
import okhttp3.Request
import okhttp3.mockwebserver.MockResponse
import okhttp3.mockwebserver.MockWebServer
import okio.Buffer
import org.json.JSONObject

class ChallengeHeadersTest {

    @Test
    fun `les en-tetes utilisent les client hints capturés dans l ordre demande`() {
        val identity = profile()

        val headers = construireEntetesNavigateur(identity, "WebView UA/154")

        assertEquals(
            listOf(
                "sec-ch-ua",
                "sec-ch-ua-mobile",
                "sec-ch-ua-platform",
                "Upgrade-Insecure-Requests",
                "User-Agent",
                "Accept",
                "Sec-Fetch-Site",
                "Sec-Fetch-Mode",
                "Sec-Fetch-User",
                "Sec-Fetch-Dest",
                "Accept-Encoding",
                "Accept-Language",
            ),
            headers.map { it.first },
        )
        assertEquals(
            "\"Microsoft Edge\";v=\"154\", \"Not.A/Brand\";v=\"99\"",
            headers.first { it.first == "sec-ch-ua" }.second,
        )
        assertEquals("?0", headers.first { it.first == "sec-ch-ua-mobile" }.second)
        assertEquals("\"Windows\"", headers.first { it.first == "sec-ch-ua-platform" }.second)
        assertEquals("WebView UA/154", headers.first { it.first == "User-Agent" }.second)
        assertEquals("gzip", headers.first { it.first == "Accept-Encoding" }.second)
        assertFalse(headers.first { it.first == "Accept-Encoding" }.second.contains("br"))
        assertEquals("fr-FR,fr;q=0.9,en-US;q=0.8", headers.last().second)
    }

    @Test
    fun `la clearance active les entetes captures et OkHttp decompresse gzip`() {
        val server = MockWebServer()
        server.start()
        try {
            val plainBody = "réponse gzip décodée"
            val compressedBody = ByteArrayOutputStream().use { output ->
                GZIPOutputStream(output).use { it.write(plainBody.toByteArray(Charsets.UTF_8)) }
                output.toByteArray()
            }
            server.enqueue(
                MockResponse()
                    .setBody(Buffer().write(compressedBody))
                    .addHeader("Content-Type", "text/plain; charset=utf-8")
                    .addHeader("Content-Encoding", "gzip"),
            )

            val context = EngineContext()
            val host = server.hostName
            val clearance = Cookie.Builder()
                .name("cf_clearance")
                .value("test-only")
                .domain(host)
                .path("/")
                .expiresAt(System.currentTimeMillis() + 60_000)
                .build()
            (context.cookieJar as InMemoryCookieJar).putCookies(host, listOf(clearance))
            context.definirUserAgentChallenge(host, "WebView UA/154")
            context.definirIdentiteNavigateurChallenge(host, profile())

            val response = context.httpClient.newCall(
                Request.Builder().url(server.url("/webtoon/")).build(),
            ).execute()
            val decodedBody = response.use { it.body.string() }
            assertEquals(plainBody, decodedBody)

            val request = server.takeRequest()
            assertEquals("WebView UA/154", request.getHeader("User-Agent"))
            assertEquals(
                "\"Microsoft Edge\";v=\"154\", \"Not.A/Brand\";v=\"99\"",
                request.getHeader("sec-ch-ua"),
            )
            assertEquals("gzip", request.getHeader("Accept-Encoding"))
            val expectedOrder = listOf(
                "sec-ch-ua",
                "sec-ch-ua-mobile",
                "sec-ch-ua-platform",
                "upgrade-insecure-requests",
                "user-agent",
                "accept",
                "sec-fetch-site",
                "sec-fetch-mode",
                "sec-fetch-user",
                "sec-fetch-dest",
                "accept-encoding",
                "accept-language",
            )
            val observedOrder = (0 until request.headers.size)
                .map { request.headers.name(it).lowercase() }
                .filter { it in expectedOrder }
            assertEquals(expectedOrder, observedOrder)
        } finally {
            server.shutdown()
        }
    }

    @Test
    fun `sans clearance le profil challenge ne modifie pas les en-tetes`() {
        val server = MockWebServer()
        server.enqueue(MockResponse().setBody("ok"))
        server.start()
        try {
            val context = EngineContext()
            context.definirUserAgentChallenge(server.hostName, "WebView UA/154")
            context.definirIdentiteNavigateurChallenge(server.hostName, profile())

            val response = context.httpClient.newCall(
                Request.Builder().url(server.url("/manga/")).build(),
            ).execute()
            response.use { assertEquals("ok", it.body.string()) }

            val request = server.takeRequest()
            assertNull(request.getHeader("sec-ch-ua"))
            assertFalse(request.getHeader("User-Agent") == "WebView UA/154")
            assertTrue(request.getHeader("User-Agent")?.isNotBlank() == true)
        } finally {
            server.shutdown()
        }
    }

    private fun profile(): BrowserIdentity =
        BrowserIdentity.fromJson(
            JSONObject(
                """
                {
                  "brands":[
                    {"brand":"Microsoft Edge","version":"154"},
                    {"brand":"Not.A/Brand","version":"99"}
                  ],
                  "mobile":false,
                  "platform":"Windows",
                  "fullVersionList":[{"brand":"Microsoft Edge","version":"154.0.0.0"}],
                  "platformVersion":"19.0.0",
                  "architecture":"x86",
                  "bitness":"64",
                  "model":"",
                  "language":"fr-FR",
                  "languages":["fr-FR","fr","en-US"]
                }
                """.trimIndent(),
            ),
        )
}
