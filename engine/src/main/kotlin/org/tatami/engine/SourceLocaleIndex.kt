package org.tatami.engine

import java.io.File
import java.util.zip.ZipFile

/**
 * Langue d'une source, lue dans le JAR des parsers (package `site/**/<xx>/`).
 *
 * Les noms ne suffisent pas : « MANGASORIGINES » finit par « ES » mais vit dans
 * `site/madara/fr/`. On indexe donc une fois au demarrage, sans instancier
 * les 1370 parsers (instantiation complete Measured tres lentement).
 */
object SourceLocaleIndex {
    private val byName: Map<String, String> by lazy { scan() }

    fun langOf(sourceName: String): String? = byName[sourceName.uppercase()]

    private fun scan(): Map<String, String> {
        val out = HashMap<String, String>()
        val uri = runCatching {
            org.koitharu.kotatsu.parsers.model.MangaParserSource::class.java
                .protectionDomain.codeSource.location.toURI()
        }.getOrNull() ?: return out
        val jar = File(uri)
        if (!jar.isFile || !jar.name.endsWith(".jar")) return out
        runCatching {
            ZipFile(jar).use { zip ->
                val entries = zip.entries()
                while (entries.hasMoreElements()) {
                    val path = entries.nextElement().name
                    if (!path.startsWith("org/koitharu/kotatsu/parsers/site/")) continue
                    if (!path.endsWith(".class") || path.contains('$')) continue
                    val segments = path.removeSuffix(".class").split('/')
                    if (segments.size < 7) continue
                    val className = segments.last()
                    val lang = segments.drop(4).dropLast(1).firstOrNull {
                        it.length == 2 && it[0].isLowerCase() && it[1].isLowerCase()
                    } ?: continue
                    out.putIfAbsent(className.uppercase(), lang)
                }
            }
        }
        return out
    }
}