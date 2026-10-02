package org.tatami.engine

import org.koitharu.kotatsu.parsers.config.ConfigKey
import org.koitharu.kotatsu.parsers.config.MangaSourceConfig

// Transposé de kotatsu-dl : chaque clé vaut son défaut tant que Rust
// ne pousse pas de configuration par source.
class DefaultMangaSourceConfig : MangaSourceConfig {
    override fun <T> get(key: ConfigKey<T>): T = key.defaultValue
}
