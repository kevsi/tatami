package org.tatami.engine

import java.io.FileDescriptor
import java.io.FileOutputStream
import java.io.PrintStream
import kotlinx.coroutines.runBlocking
import org.koitharu.kotatsu.parsers.model.MangaListFilter
import org.koitharu.kotatsu.parsers.model.MangaParserSource

// CLI de test (phase 0.4) : lister les sources, chercher dans une source.
// Mode serveur (phase 0.6) : boucle NDJSON sur stdio, voir contract/.
// Usage :
//   gradle run --args="list-sources"
//   gradle run --args="search MANGADEX naruto"
//   gradle run --args="serve"
fun main(args: Array<String>) {
    // Windows encode la console en cp1252/GBK : sans cela, stdout emet des octets
    // invalides en UTF-8 et le superviseur Rust tue sa boucle de lecture
    // (incident 2026-10-02 : "stream did not contain valid UTF-8").
    System.setOut(PrintStream(FileOutputStream(FileDescriptor.out), true, Charsets.UTF_8))
    System.setErr(PrintStream(FileOutputStream(FileDescriptor.err), true, Charsets.UTF_8))
    if (args.isEmpty()) {
        println("usage: list-sources | search <SOURCE> <query> | serve")
        return
    }
    val context = EngineContext()
    when (args[0]) {
        "serve" -> serve()
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
            println("usage: list-sources | search <SOURCE> <query> | serve")
        }
    }
}
