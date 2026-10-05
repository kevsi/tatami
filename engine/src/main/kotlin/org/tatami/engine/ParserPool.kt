package org.tatami.engine

import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import okhttp3.Interceptor
import org.koitharu.kotatsu.parsers.MangaLoaderContext
import org.koitharu.kotatsu.parsers.MangaParser
import org.koitharu.kotatsu.parsers.model.MangaParserSource
import java.util.LinkedHashMap

/**
 * Le parser de la requête en cours, pour le fil qui l'exécute.
 *
 * ## Pourquoi un `ThreadLocal`
 *
 * Un parser est un `okhttp3.Interceptor` : il porte ses propres en-têtes de
 * requête **et** il débrouille les réponses (cf. `redrawImageResponse`). Sans
 * lui dans la chaîne, ZENMANGA sert du chiffrement : mesuré le 2026-10-05,
 * 832 513 octets dont les 16 premiers sont
 * **identiques sur deux pages différentes**. En passant le parser dans la
 * chaîne, les mêmes octets commencent par `FF D8 FF E0` — un JPEG valide.
 *
 * `httpClient` est construit une seule fois, avant qu'aucun parser n'existe :
 * on ne peut donc pas y inscrire le parser d'une source. Le `ThreadLocal` fait
 * le lien, et `ParserPool.avec` le pose autour de l'appel.
 *
 * **Si le fil ne correspond pas** (un `ThreadLocal` non propagé à travers une
 * suspension), l'intercepteur laisse simplement passer la réponse : on retombe
 * sur le comportement d'aujourd'hui, pas sur une panne. Mieux vaut une source
 * qui manque d'images qu'un moteur qui ne répond plus.
 */
internal object ParserActif {
    // Un `MangaParser` **est** un `okhttp3.Interceptor` (`MangaParser extends
    // okhttp3.Interceptor`). On stocke donc l'intercepteur, pas le parser : c'est
    // tout ce dont la chaîne a besoin, et un test peut fournir un simple
    // intercepteur-espion sans fabriquer un faux parser complet.
    private val local = ThreadLocal<Interceptor?>()

    fun activer(intercepteur: Interceptor?) {
        local.set(intercepteur)
    }

    fun lire(): Interceptor? = local.get()
}

/**
 * Instances de parser **conservées** d'une requête à l'autre.
 *
 * ## Pourquoi
 *
 * Un parser n'est pas une fonction pure : certains parsers gardent l'état de
 * leur pagination **dans l'instance**. EXHENTAI en est l'exemple mesuré le
 * 2026-10-05 : pour atteindre la page 2 il lui faut le paramètre `next=<ts>` que
 * le site n'inscrit **que dans le lien de la page précédente**. Le parser le
 * mémorise dans une table par filtre ; sans cette table il lève
 * `AssertionError("Page timestamp not found")` **sans faire aucune requête**
 * (mesuré : 2 ms). Le moteur créait une instance neuve par requête, donc la
 * page 2 était inaccessible quel que soit le nombre d'éléments par page.
 *
 * C'est aussi le comportement de l'app mobile, qui conserve ses parsers vivants.
 *
 * Effet secondaire bienvenu, mesuré sur la même source : la page 1 passe de
 * 2 004 ms à 570 ms, la configuration et la carte des tags n'étant plus
 * reparsées à chaque appel.
 *
 * ## Choix de conception
 *
 * - **Une instance par source, sérialisée par un `Mutex`.** Les parsers ont un
 *   état mutable (`PagedMangaParser` porte un `Paginator`) : deux requêtes
 *   concurrentes sur la même source se marcheraient dessus. `Mutex` est
 *   `suspend`, donc — contrairement à `synchronized` — il ne bloque pas le fil
 *   autour d'un appel suspend.
 * - **LRU borné.** Une instance par source sur les 1 370 consommerait la
 *   mémoire sans fin (les parsers zonent des cartes de tags énormes). Le cache
 *   est plafonné, et l'éviction **saute les entrées en cours d'usage** :
 *   éjecter une entrée utilisée par une requête en vol lui retirerait son état
 *   sous les pieds.
 */
internal class ParserPool(
    private val context: MangaLoaderContext,
    private val maxSources: Int = 32,
) {

    private class Entry(val parser: MangaParser) {
        val mutex = Mutex()

        /** Vrai tant qu'une requête tient l'instance : l'éviction l'ignore. */
        @Volatile
        var enCours: Boolean = false
    }

    // `accessOrder = true` : un `get` marque l'entrée comme la plus récemment
    // utilisée, et l'itération suit donc l'ordre LRU. C'est ce qui permet
    // d'éjecter la première entrée libre sans aucun tri explicite.
    private val entries = object : LinkedHashMap<String, Entry>(16, 0.75f, true) {}
    private val garde = Any()

    /**
     * Exécute [bloc] avec le parser de [source], en gardant l'instance pour les
     * requêtes suivantes.
     *
     * **Pas réentrant.** Un `Mutex` ne se reprend pas lui-même : appeler `avec`
     * sur la même source depuis l'intérieur de [bloc] figerait le moteur sur un
     * simple `getPageUrl` puis `lire`. C'est attrapé le jour où un test a
     * TimeoutError au lieu d'échouer. Sortir d'abord, puis rentrer.
     */
    suspend fun <T> avec(source: MangaParserSource, bloc: suspend (MangaParser) -> T): T {
        val entry = entryPour(source)
        entry.enCours = true
        try {
            return entry.mutex.withLock {
                // Le parser doit etre visible depuis la chaine HTTP : sans cela
                // ses en-tetes et son debrouillage d'image sont perdus.
                // On restaure le précédent, sinon la requête suivante hériterait
                // du parser de celle-la.
                val precedent = ParserActif.lire()
                ParserActif.activer(entry.parser)
                try {
                    bloc(entry.parser)
                } finally {
                    ParserActif.activer(precedent)
                }
            }
        } finally {
            entry.enCours = false
        }
    }

    private fun entryPour(source: MangaParserSource): Entry = synchronized(garde) {
        val cle = source.name
        entries[cle]?.let { return@synchronized it }

        val neuf = Entry(context.newParserInstance(source))
        entries[cle] = neuf
        if (entries.size > maxSources) {
            val aVancer = entries.entries.iterator()
            while (aVancer.hasNext()) {
                if (!aVancer.next().value.enCours) {
                    aVancer.remove()
                    break
                }
            }
        }
        neuf
    }

    /** Nombre d'instances conservées. Pour les tests et le diagnostic. */
    val taille: Int
        get() = synchronized(garde) { entries.size }
}
