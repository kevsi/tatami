package org.tatami.engine

import org.koitharu.kotatsu.parsers.model.MangaSource

/**
 * Domaines de remplacement, pour les sources dont le site a changé d'adresse.
 *
 * Mesuré le 2026-10-12 : ces trois domaines codés en dur dans
 * `kotatsu-parsers-redo` ne résolvent plus (HTTP 000, pas de réponse TCP) :
 *
 * | Source        | Domaine du parseur (mort) | Nouveau domaine (vivant) |
 * |---------------|---------------------------|--------------------------|
 * | ANIMESAMA     | `anime-sama.eu`           | `anime-sama.to`         |
 * | JAPSCANSFR    | `japscans.fr`             | `japscan.foo`            |
 * | SCANTRAD      | `scan-trad.com`          | `hentai-scantrad.org`    |
 *
 * On ne touche pas au JAR : `AbstractMangaParser.getDomain()` lit
 * `config.get(configKeyDomain)`, donc c'est la **config** qui décide du
 * domaine. `getConfig(source)` recevait la source et l'ignorait — c'est
 * ici que la redirection se fait.
 *
 * Le nouveau domaine est vérifié à la main (HTTP 200) mais **les
 * sélecteurs du parseur** peuvent ne pas correspondre à la nouvelle
 * version du site : une source surchargée peut rester en
 * `PARSER_OUTDATED`. C'est le probe (`scripts/sonde-sources.mjs`) qui le
 * dira, pas cette table.
 */
internal object SurchargeDomaine {
    private val REMPLACEMENTS: Map<String, String> = mapOf(
        "ANIMESAMA" to "anime-sama.to",
        "JAPSCANSFR" to "japscan.foo",
        "SCANTRAD" to "hentai-scantrad.org",
    )

    fun pour(source: MangaSource): String? = REMPLACEMENTS[source.name]
}