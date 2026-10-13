package org.tatami.engine

import okhttp3.Request

private val challengeHeaderNames = listOf(
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
)

private val loggedBrowserHeaderNames = challengeHeaderNames
    .mapTo(mutableSetOf()) { it.lowercase() }

internal fun appliquerEntetesNavigateur(
    request: Request,
    identity: BrowserIdentity,
    userAgent: String,
): Request {
    val headers = construireEntetesNavigateur(identity, userAgent)
    val builder = request.newBuilder()
    challengeHeaderNames.forEach(builder::removeHeader)
    headers.forEach { (name, value) -> builder.header(name, value) }
    val result = builder.build()

    if (challengeDebug) {
        val headersLog = (0 until result.headers.size).joinToString(", ") { index ->
            val name = result.headers.name(index)
            val value = if (name.lowercase() in loggedBrowserHeaderNames) {
                result.headers.value(index)
            } else {
                "<masqué>"
            }
            "$name=$value"
        }
        System.err.println(
            "[challenge-debug] en-tetes envoyes hote=${request.url.host} [$headersLog]",
        )
    }

    return result
}

internal fun construireEntetesNavigateur(
    identity: BrowserIdentity,
    userAgent: String,
): List<Pair<String, String>> {
    val headers = mutableListOf<Pair<String, String>>()
    val brands = identity.brands
        .filter { it.brand.isNotBlank() && it.version.isNotBlank() }
        .joinToString(", ") { "${quoteClientHint(it.brand)};v=${quoteClientHint(it.version)}" }
    if (brands.isNotEmpty()) headers += "sec-ch-ua" to brands
    identity.mobile?.let { headers += "sec-ch-ua-mobile" to if (it) "?1" else "?0" }
    identity.platform
        ?.takeIf(String::isNotBlank)
        ?.let { headers += "sec-ch-ua-platform" to quoteClientHint(it) }

    headers += "Upgrade-Insecure-Requests" to "1"
    headers += "User-Agent" to userAgent
    headers += "Accept" to "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8"
    headers += "Sec-Fetch-Site" to "same-origin"
    headers += "Sec-Fetch-Mode" to "navigate"
    headers += "Sec-Fetch-User" to "?1"
    headers += "Sec-Fetch-Dest" to "document"
    headers += "Accept-Encoding" to "gzip"
    languageHeader(identity)?.let { headers += "Accept-Language" to it }
    return headers
}

private fun languageHeader(identity: BrowserIdentity): String? {
    val languages = (identity.languages + listOfNotNull(identity.language))
        .map(String::trim)
        .filter { it.matches(Regex("[A-Za-z0-9*]+(?:-[A-Za-z0-9]+)*")) }
        .distinct()
        .take(10)
    if (languages.isEmpty()) return null

    return languages.mapIndexed { index, language ->
        if (index == 0) {
            language
        } else {
            val quality = (10 - index).coerceAtLeast(1)
            "$language;q=0.$quality"
        }
    }.joinToString(",")
}

private fun quoteClientHint(value: String): String =
    "\"${value.replace("\\", "\\\\").replace("\"", "\\\"")}\""
