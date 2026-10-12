package org.tatami.engine

import java.net.IDN
import java.net.URI
import java.util.Locale

internal fun domainesNatifs(domaineActuel: String, presets: Iterable<String>): List<String> =
    (sequenceOf(domaineActuel) + presets.asSequence())
        .mapNotNull(::normaliserDomaineNatif)
        .distinct()
        .toList()

internal fun domaineScript(baseUrl: String): String? {
    val uri = runCatching { URI(baseUrl) }.getOrNull() ?: return null
    val schema = uri.scheme?.lowercase(Locale.ROOT)
    if (
        schema !in listOf("http", "https") ||
        uri.rawUserInfo != null ||
        uri.rawQuery != null ||
        uri.rawFragment != null
    ) {
        return null
    }
    return uri.host?.let(::normaliserHote)
}

private fun normaliserDomaineNatif(valeur: String): String? {
    if (valeur.isBlank() || valeur != valeur.trim() || '\\' in valeur) {
        return null
    }
    val avecSchema = if (SCHEMA_URL.containsMatchIn(valeur)) valeur else "https://$valeur"
    val uri = runCatching { URI(avecSchema) }.getOrNull() ?: return null
    if (uri.rawUserInfo != null) return null
    val hote = uri.host ?: runCatching { uri.toURL().host }.getOrNull() ?: return null
    return normaliserHote(hote)
}

private val SCHEMA_URL = Regex("^[A-Za-z][A-Za-z0-9+.-]*://")

private fun normaliserHote(hote: String): String? {
    val ascii = runCatching { IDN.toASCII(hote, IDN.USE_STD3_ASCII_RULES) }.getOrNull() ?: return null
    val normalise = ascii.trimEnd('.').lowercase(Locale.ROOT)
    if (
        normalise.isEmpty() ||
        normalise.length > 253 ||
        normalise.split('.').any { it.isEmpty() || it.length > 63 }
    ) {
        return null
    }
    return normalise
}
