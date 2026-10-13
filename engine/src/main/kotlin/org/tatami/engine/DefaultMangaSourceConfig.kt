package org.tatami.engine

import org.koitharu.kotatsu.parsers.config.ConfigKey
import org.koitharu.kotatsu.parsers.config.MangaSourceConfig

// Transposé de kotatsu-dl : chaque clé vaut son défaut tant que Rust
// ne pousse pas de configuration par source.
//
// La seule exception est le **domaine** : une source dont le site a changé
// d'adresse est redirigée par `SurchargeDomaine`. On le sait parce que
// `AbstractMangaParser.getDomain()` lit `config.get(configKeyDomain)` —
// la config décide du domaine, pas le parseur.
class DefaultMangaSourceConfig(
    private val domaine: String? = null,
) : MangaSourceConfig {
    @Suppress("UNCHECKED_CAST")
    override fun <T> get(key: ConfigKey<T>): T =
        if (key is ConfigKey.Domain && domaine != null) domaine as T else key.defaultValue
}