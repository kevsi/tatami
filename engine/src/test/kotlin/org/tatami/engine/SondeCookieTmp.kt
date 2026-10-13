package org.tatami.engine

import okhttp3.Cookie
import okhttp3.mockwebserver.MockResponse
import okhttp3.mockwebserver.MockWebServer
import kotlin.test.Test

/**
 * Sonde **jetable** : elle ne teste rien, elle écrit un rapport. Sert à
 * comprendre pourquoi `loadForRequest` ne rend pas la `cf_clearance` sur le
 * serveur de test — c'est-à-dire mesurer avant de conclure que le code est
 * faux.
 */
class SonteCookieTmp {
    @Test
    fun imprime() {
        val s = MockWebServer()
        s.enqueue(MockResponse().setResponseCode(200).setBody("ok"))
        s.start()
        val url = s.url("/manga/")
        val c = Cookie.Builder()
            .name("cf_clearance").value("v")
            .domain(url.host).path("/")
            .expiresAt(System.currentTimeMillis() + 3_600_000)
            .httpOnly()
            .build()
        val rapport = buildString {
            appendLine("url        = $url")
            appendLine("url.host   = ${url.host}")
            appendLine("url.scheme = ${url.scheme}")
            appendLine("hostName() = ${s.hostName}")
            appendLine("egaux      = ${url.host == s.hostName}")
            appendLine("matches    = ${c.matches(url)}")
        }
        java.io.File("C:/Users/alexanders/AppData/Local/Temp/opencode/sonte-cookie.txt")
            .writeText(rapport)
        s.shutdown()
    }
}