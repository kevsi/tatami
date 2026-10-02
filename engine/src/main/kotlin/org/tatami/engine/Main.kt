package org.tatami.engine

import kotlinx.coroutines.runBlocking
import org.koitharu.kotatsu.parsers.model.MangaListFilter
import org.koitharu.kotatsu.parsers.model.MangaParserSource

// CLI de test (phase 0.4) : lister les sources, chercher dans une source.
// Usage :
//   gradle run --args="list-sources"
//   gradle run --args="search MANGADEX naruto"
fun main(args: Array<String>) {
    if (args.isEmpty()) {
        println("usage: list-sources | search <SOURCE> <query>")
        return
    }
    val context = EngineContext()
    when (args[0]) {
        "list-sources" -> {
            val entries = MangaParserSource.entries
            println("sources: ${entries.size}")
            entries.forEach { println(it.name) }
        }
        "search" -> {
            require(args.size >= 3) { "usage: search <SOURCE> <query>" }
            val source = MangaParserSource.valueOf(args[1])
            val query = args.drop(2).joinToString(" ")
            runBlocking {
                val parser = context.newParserInstance(source)
                val order = parser.availableSortOrders.first()
                val results = parser.getList(0, order, MangaListFilter(query = query))
                println("results: ${results.size}")
                results.take(5).forEach { println("${it.title} | ${it.url}") }
            }
        }
        else -> {
            println("unknown command: ${args[0]}")
            println("usage: list-sources | search <SOURCE> <query>")
        }
    }
}
