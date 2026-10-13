package org.tatami.engine

import java.util.concurrent.ConcurrentHashMap
import okhttp3.Interceptor
import okhttp3.MediaType.Companion.toMediaTypeOrNull
import okhttp3.Protocol
import okhttp3.Response
import okhttp3.ResponseBody.Companion.toResponseBody

/**
 * Mémoire **URL → HTML**, alimentée par le transport webview.
 *
 * ## Pourquoi ce module existe
 *
 * Mesuré le 2026-10-13 : le WebView2 résout seul le challenge Cloudflare de
 * TOONFR en ~10 s et obtient 200 là où OkHttp, avec la même clearance, obtient
 * 403. Le seul client accepté est donc celui qui a résolu. Dès lors, plutôt que
 * de contrefaire l'empreinte d'OkHttp, on **demande au navigateur** — et c'est
 * ce module qui rend le résultat utilisable par le moteur sans qu'il change une
 * ligne de son raisonnement : il continue de faire une requête HTTP, et reçoit
 * une réponse 200 dont le corps est le HTML que la webview a obtenu.
 *
 * ## Ce que fait l'intercepteur, et pourquoi il est premier
 *
 * Il sert la mémoire **avant** tout le reste. L'ordre compte :
 * `CloudFlareInterceptor` transforme un 403 en exception, donc si on le laissait
 * passer devant, une page en mémoire serait de nouveau challengée. Le premier
 * `addInterceptor` d'OkHttp est le plus externe, celui qui est appelé avant
 * tous les autres (cf. `EngineContext.httpClient`).
 *
 * ## Portée assumée — HTML seulement
 *
 * Servir la mémoire avant la chaîne fait l'impasse sur
 * `intercepteurParser`, qui déchiffre les images de certaines sources
 * (ZENMANGA, mesure du 2026-10-05). Pour du HTML, aucun déchiffrement n'est
 * nécessaire ; pour des images, si. Ce module ne sert donc **que** les pages
 * HTML, et les images restent prises en charge par le réseau — jusqu'à ce que
 * la mesure du transport d'images dise le contraire.
 *
 * ## Durée de vie
 *
 * Cinq minutes, alignées sur `RejeuRefuse.FRAICHEUR_MS` : un contenu figé ne
 * doit pas devenir une vérité durable. Une page vue il y a une heure vaut un
 * nouveau passage par la webview, qui refera un challenge si le site l'exige.
 * Une entrée expirée est retirée à la lecture, donc la mémoire ne grandit pas
 * avec les consultations.
 */
internal object WebViewMemoire {

    /** Durée de vie d'une entrée. */
    const val TTL_MS: Long = 5L * 60L * 1000L

    private class Entree(val corps: String, val expireA: Long)

    private val entrees = ConcurrentHashMap<String, Entree>()

    /** Horloge injectable : les tests n'ont pas à attendre cinq minutes. */
    @Volatile
    var maintenant: () -> Long = System::currentTimeMillis

    /**
     * Normalise la clé. Le fragment est retiré (`#chapitre-3` n'est pas une
     * page différente côté serveur) ; la requête est conservée, car deux pages
     * peuvent partager un chemin et différer par leurs paramètres.
     */
    fun cle(url: String): String {
        val sansFragment = url.substringBefore('#')
        return sansFragment.trim()
    }

    /** Enregistre le HTML obtenu par la webview pour cette URL. */
    fun injecter(url: String, html: String) {
        if (html.isEmpty()) return
        entrees[cle(url)] = Entree(html, maintenant() + TTL_MS)
    }

    /**
     * Lit le HTML mémorisé, ou `null`.
     *
     * `null` couvre trois cas volontairement confondus : jamais injecté, expiré,
     * ou vidé. L'appelant n'a pas à distinguer « le transport n'a pas encore
     * tourné » de « la page est trop vieille » — dans les deux cas il doit
     * refaire une requête réseau.
     */
    fun lire(url: String): String? {
        val entree = entrees[cle(url)] ?: return null
        if (maintenant() >= entree.expireA) {
            entrees.remove(cle(url), entree)
            return null
        }
        return entree.corps
    }

    /** Oublie une URL précise. */
    fun oublier(url: String) {
        entrees.remove(cle(url))
    }

    /** Nombre d'entrées vivantes — borne la mémoire et se teste. */
    fun taille(): Int = entrees.keys.count { lire(it) != null }

    /** Vide tout. Visible pour les tests et pour la remise à zéro d'une source. */
    fun vider() {
        entrees.clear()
    }
}

/**
 * Sert la mémoire webview avant toute autre chose.
 *
 * La réponse est fabriquée à la main : `200`, `text/html; charset=utf-8`, corps
 * = le HTML capturé. Aucun appel réseau n'a lieu, donc aucun challenge, aucun
 * cookie, aucun `RejeuRefuse`. C'est exactement le but : la question « le site
 * accepte-t-il ce client ? » a déjà été tranchée par la webview.
 *
 * La requête est reconstruite depuis l'originale (`newBuilder()` copie les
 * tags), pour que `requete.tag(MangaSource::class.java)` continue de répondre
 * dans ce qui suit de la chaîne — même si, court-circuité ici, il n'y a rien
 * après.
 */
internal val intercepteurWebView = Interceptor { chain ->
    val requete = chain.request()
    val memorise = WebViewMemoire.lire(requete.url.toString())
    if (memorise != null) {
        // **Réponse fabriquée de toutes pièces, pas `chain.proceed`.**
        // Appeler `proceed` ici déclencherait la vraie requête réseau — donc le
        // challenge qu'on veut justement éviter — puis on en jetterait le
        // résultat. Le test `une page captive par la webview bypass le challenge
        // sans toucher le reseau` verrouille ce point : son compteur tombe à
        // l'instant où on remet un `proceed`.
        val corps = memorise.toResponseBody("text/html; charset=utf-8".toMediaTypeOrNull())
        return@Interceptor Response.Builder()
            .request(requete)
            .protocol(Protocol.HTTP_1_1)
            .code(200)
            .message("OK")
            .body(corps)
            .build()
    }
    chain.proceed(requete)
}
