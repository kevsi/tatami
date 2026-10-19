package org.tatami.engine

import java.util.concurrent.Semaphore
import java.util.concurrent.atomic.AtomicInteger

/**
 * Borne le nombre d'appels HTTP simultanés.
 *
 * ## Pourquoi
 *
 * Mesuré le 2026-10-09 : le lecteur d'un chapitre de 30 pages a déclenché 30
 * requêtes `fetchImage` **en même temps**, et le CDN a répondu 403 à corps vide
 * aux 17 premières. La rafale est possible parce que la boucle de service
 * utilise `Executors.newCachedThreadPool` (`Server.kt:329`) : chaque requête
 * reçue obtient son propre thread, donc rien ne borne les départs vers le
 * réseau.
 *
 * Le même constat, déjà mesuré sur le planificateur (AGENTS.md §14) : « un hôte
 * sent les rafales simultanées, pas le débit moyen ». Un débit moyen correct ne
 * protège pas un hôte ; ce qui le protège, c'est l'absence de départs
 * simultanés.
 *
 * ## Pourquoi ne pas dépendre des dimensions
 *
 * La virtualisation du lecteur (tâche 1.9) limiterait elle aussi la rafale,
 * mais elle n'est active que si **toutes** les pages ont une hauteur connue, ce
 * qui n'est pas le cas d'un chapitre ouvert pour la première fois (les
 * dimensions ne sont connues qu'après avoir servi la page). Cette borne ne
 * dépend d'aucun état : elle tient même quand la virtualisation est inactive.
 *
 * @param places nombre maximal d'appels simultanés.
 */
internal class LimiteDeConcurrence(val places: Int) {

    private val semaphore = Semaphore(places)

    // Compteur **d'appels réellement en cours**, incrémenté après l'acquisition
    // d'un permis. Il ne peut pas être déduit de `availablePermits` : le pic
    // observé pendant un appel est ce qu'on veut mesurer, et un compteur lu
    // après coup ne prouve rien (AGENTS.md §14 : « un compteur qui bouge n'est
    // pas la preuve que l'écran a bougé »).
    private val compteur = AtomicInteger(0)

    /** Nombre d'appels en cours. */
    val enVol: Int get() = compteur.get()

    /**
     * Exécute [bloc] en respectant la borne, et rend sa valeur.
     *
     * Le permis est rendu dans un `finally` : un échec réseau ne doit pas
     * coûter une place définitivement, sinon la dixième erreur successive
     * bloquerait le moteur jusqu'au redémarrage.
     */
    fun <T> autourDe(bloc: () -> T): T {
        // `acquire` **bloque** : c'est la contre-pression voulue. Le pool de
        // threads est non borné (`Server.kt:329`), donc la file se forme ici et
        // pas sur le réseau — c'est exactement le but.
        semaphore.acquire()
        compteur.incrementAndGet()
        try {
            return bloc()
        } finally {
            compteur.decrementAndGet()
            semaphore.release()
        }
    }
}

/**
 * Borne partagée par **toutes** les récupérations d'octets d'image du moteur :
 * `fetchImage` et `fetchCover`, chemin natif (`fetch_octets`) comme source
 * scriptée (`telechargerImage`).
 *
 * Une seule instance, volontairement : une borne par chemin laisserait passer
 * deux rafales simultanées de 4, qui est précisément ce qu'on cherche à
 * éviter — un hôte voit le total, pas la répartition interne. Une couverture
 * prise dans la file face à 30 pages est le bon arbitrage : elle fait 40 Ko,
 * une page 800 Ko.
 *
 * La valeur 4 initiale reprenait le `RECENT_CONCURRENCY` déjà mesuré sur le
 * planificateur (AGENTS.md §14) — le seul nombre déjà chez nous assorti d'une
 * mesure. Relevée à 6 le 2026-10-20 : elle suit les bornes du cœur
 * (`download.rs::CONCURRENCE`) et le nombre de travailleurs du moteur
 * (`Server.kt::MAX_WORKERS`), qui doivent bouger ensemble. Mesure et décision :
 * ADR 0019.
 */
internal val LIMITES_IMAGES = LimiteDeConcurrence(6)
