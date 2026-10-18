package org.tatami.engine

import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import okhttp3.Interceptor
import okhttp3.Request
import org.koitharu.kotatsu.parsers.MangaLoaderContext
import org.koitharu.kotatsu.parsers.MangaParser
import org.koitharu.kotatsu.parsers.model.MangaParserSource
import org.koitharu.kotatsu.parsers.model.MangaSource
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

    /**
     * Posé par [ParserPool] : retrouve l'instance conservée d'une source, sans
     * prendre le verrou de la requête.
     *
     * @Volatile et non un verrou : le résolveur ne fait qu'une lecture dans une
     * table, et le worst case est de ne pas trouver une entrée évincée entre-temps.
     */
    @Volatile
    var resolveur: ((MangaSource) -> Interceptor?)? = null

    fun activer(intercepteur: Interceptor?) {
        local.set(intercepteur)
    }

    fun lire(): Interceptor? = local.get()

    /**
     * Le parseur qui concerne **cette** requête.
     *
     * D'abord la source portée par la requête — `OkHttpWebClient` (dans la
     * librairie de parseurs) pose `tag(MangaSource::class.java, source)` sur
     * chaque requête, et c'est ce que lit l'intercepteur de Kotatsu. Puis le
     * `ThreadLocal` en repli, pour les chemins synchrones.
     *
     * **Pourquoi le tag d'abord.** Mesuré le 2026-10-11 : les parseurs
     * appellent OkHttp en asynchrone (`enqueue`), donc l'intercepteur
     * s'exécute sur un fil du dispatcher d'OkHttp, où le `ThreadLocal` posé par
     * `ParserPool.avec` sur le fil appelant **n'existe pas**. La trace le disait
     * sans ambiguïté : `intercepteur parseur=null intercepteurLu=false
     * enTetesDeclares=-1`. Les en-têtes déclarés par les sources n'étaient donc
     * jamais appliqués — alors que Kotatsu les applique.
     */
    fun pourRequete(requete: Request): Interceptor? {
        val source = requete.tag(MangaSource::class.java)
        if (source != null) {
            resolveur?.invoke(source)?.let { return it }
        }
        return local.get()
    }
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
 * - **Le verrou n'est pas pris pour lire des octets** (`horsVerrou`) : ce que
 *   la chaîne HTTP doit retrouver, c'est le parser *posé sur le fil*, pas
 *   l'exclusion mutuelle. Mesuré le 2026-10-18 : le tenir pendant un
 *   téléchargement sérialisait les pages d'un chapitre (4 en parallèle :
 *   762 ms contre 669 ms en série).
 */
internal class ParserPool(
    private val context: MangaLoaderContext,
    private val maxSources: Int = 32,
) {

    private class Entry(val parser: MangaParser) {
        val mutex = Mutex()
        var utilisateurs: Int = 0
    }

    // `accessOrder = true` : un `get` marque l'entrée comme la plus récemment
    // utilisée, et l'itération suit donc l'ordre LRU. C'est ce qui permet
    // d'éjecter la première entrée libre sans aucun tri explicite.
    private val entries = object : LinkedHashMap<String, Entry>(16, 0.75f, true) {}

    /**
     * Rend le parseur conservé d'une source, pour que l'intercepteur HTTP
     * retrouve le sien **sans dépendre du fil** (cf. `ParserActif.pourRequete`).
     *
     * L'entrée existe déjà quand la requête part : `avec` l'a créée avant
     * d'appeler le parseur, et elle ne peut pas être évincée pendant ce temps
     * (`utilisateurs` est positif). Le seul cas où elle manque est une source que le
     * pool n'a jamais instanciée — on rend alors `null` et l'intercepteur
     * laisse passer la requête, comme il le fait pour tout le monde.
     */
    private fun parserDeSource(source: MangaSource): MangaParser? =
        synchronized(garde) { entries[source.name]?.parser }

    init {
        ParserActif.resolveur = { source -> parserDeSource(source) }
    }
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
            relacher(entry)
        }
    }

    /**
     * Exécute [bloc] avec le parser de [source], **sans prendre son verrou**.
     *
     * Réservé aux appels qui n'utilisent pas l'**état** du parser — aujourd'hui
     * le seul : le téléchargement des octets d'une URL déjà résolue
     * (`fetchImage`, après `getPageUrl`).
     *
     * ## Pourquoi c'est correct
     *
     * Ce que la chaîne HTTP a besoin de retrouver, c'est le parser **posé sur le
     * fil** (ses en-têtes et son débrouillage d'image, cf. `ParserActif`), et
     * c'est [ParserActif.activer] qui le pose — pas le `Mutex`. Le verrou, lui,
     * protège l'état mutable (`PagedMangaParser` porte un `Paginator`), que la
     * lecture d'octets ne touche pas.
     *
     * ## Mesure qui a motivé son existence
     *
     * Le 2026-10-18 : quatre `fetchImage` de la **même page** en parallèle
     * prenaient 762 ms contre 669 ms en série — donc sérialisées, parce que le
     * verrou était tenu pendant l'`execute()` bloquant. Le client demande 4
     * images à la fois (`download.rs`, `CONCURRENCE = 4`).
     *
     * ## Ce qui n'est pas perdu
     *
     * La politesse : chaque téléchargement passe par `LIMITES_IMAGES` (4 images
     * simultanées, toutes sources confondues), donc l'hôte voit toujours au plus
     * quatre requêtes — c'est la borne voulue, et elle ne dépend pas de ce
     * verrou. La réservation d'entrée (`utilisateurs`) est prise puis rendue,
     * donc l'éviction LRU ne peut pas retirer ce parser sous les pieds de
     * l'appel en cours.
     */
    fun <T> horsVerrou(source: MangaParserSource, bloc: (MangaParser) -> T): T {
        val entry = entryPour(source)
        try {
            val precedent = ParserActif.lire()
            ParserActif.activer(entry.parser)
            try {
                return bloc(entry.parser)
            } finally {
                ParserActif.activer(precedent)
            }
        } finally {
            relacher(entry)
        }
    }

    private fun relacher(entry: Entry) {
        synchronized(garde) {
            check(entry.utilisateurs > 0) { "reservation du parser deja liberee" }
            entry.utilisateurs--
        }
    }

    private fun entryPour(source: MangaParserSource): Entry = synchronized(garde) {
        val cle = source.name
        entries[cle]?.let {
            it.utilisateurs++
            return@synchronized it
        }

        val neuf = Entry(context.newParserInstance(source))
        neuf.utilisateurs = 1
        entries[cle] = neuf
        if (entries.size > maxSources) {
            val aVancer = entries.entries.iterator()
            while (aVancer.hasNext()) {
                if (aVancer.next().value.utilisateurs == 0) {
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

    /** Réservations simultanées d'une source, sans modifier l'ordre LRU. */
    internal fun utilisateurs(source: MangaParserSource): Int =
        synchronized(garde) {
            entries.entries.firstOrNull { it.key == source.name }?.value?.utilisateurs ?: 0
        }
}
