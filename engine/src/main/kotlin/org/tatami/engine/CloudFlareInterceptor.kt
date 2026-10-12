package org.tatami.engine

import okhttp3.Cookie
import okhttp3.CookieJar
import okhttp3.HttpUrl
import okhttp3.Interceptor
import okhttp3.Response
import org.koitharu.kotatsu.parsers.network.CloudFlareHelper

/**
 * Transposé de kotatsu-dl `CloudFlareInterceptor`, adapté à OkHttp 5
 * (close() public au lieu de okhttp3.internal.closeQuietly).
 *
 * ## Ce que cet intercepteur décide de plus que la référence
 *
 * Kotatsu-Redo (`core/network/CloudFlareInterceptor.kt`) lève toujours
 * `CloudFlareProtectedException`, ce qui ouvre la fenêtre à chaque 403 Cloudflare.
 * C'est correct chez eux : chez eux, la fenêtre **marche**. Mesuré chez nous le
 * 2026-10-11 (HENTAIZONE) : la fenêtre aboutit, la clearance est lue, le rejeu
 * est refusé (`403 cf-mitigated`). Rouvrir la fenêtre ne peut donc pas aider,
 * et le cycle ne se termine pas.
 *
 * La distinction tient à **un seul fait observable** : la requête portait-elle
 * déjà `cf_clearance` ? Si oui, le challenge a été résolu et le rejeu échoue —
 * on ne propose plus de réessayer. Sinon, c'est une demande de vérification
 * ordinaire et on ouvre la fenêtre comme avant.
 *
 * ## Où lit-on la présence du cookie
 *
 * **Pas** dans l'en-tête `Cookie` de la requête : `BridgeInterceptor` d'OkHttp
 * l'ajoute **après** tous les intercepteurs d'application, donc il est encore
 * vide ici. C'est ce que fait déjà l'instrumentation `challenge-debug`
 * (`EngineContext`), et pour la même raison. On interroge donc le magasin.
 *
 * L'injection du magasin se fait ici plutôt qu'en lecture dans `Server` : un
 * intercepteur **sans** magasin ne peut pas distinguer les deux cas, et
 * reverrait à lever le même challenge pour les deux.
 */
internal class CloudFlareInterceptor(
    private val cookieJar: CookieJar,
) : Interceptor {

    override fun intercept(chain: Interceptor.Chain): Response {
        val request = chain.request()
        val response = chain.proceed(request)
        if (CloudFlareHelper.checkResponseForProtection(response) != CloudFlareHelper.PROTECTION_NOT_DETECTED) {
            val url = response.request.url.toString()
            response.close()
            val hote = request.url.host
            val clearance = porteUneClearance(request.url)
            val cleHote = clearance?.domain ?: hote
            // Mémoire d'un échec **antérieur** sur cet hôte : inutile de
            // renvoyer le réseau si l'on sait déjà que le rejeu échoue.
            if (RejeuRefuse.estRefuse(hote)) {
                RejeuRefuse.marquer(cleHote)
                throw CloudFlareUnreplayableException(url = url)
            }
            if (clearance != null && RejeuRefuse.resolutionRecente(clearance.domain)) {
                RejeuRefuse.marquer(clearance.domain)
                throw CloudFlareUnreplayableException(url = url)
            }
            throw CloudFlareProtectionException(url = url)
        }
        // **Une réponse normale annule l'échec précédent.** Sans cette ligne,
        // un site qui ré-autorise resterait bloqué jusqu'à l'expiration, alors
        // qu'il vient de nous répondre : la mémoire ne serait plus le reflet du
        // dernier état observé.
        RejeuRefuse.oublier(request.url.host)
        return response
    }

    /** Le magasin va-t-il joindre une `cf_clearance` à une requête vers cette URL ? */
    private fun porteUneClearance(url: HttpUrl): Cookie? =
        cookieJar.loadForRequest(url).firstOrNull { it.name == "cf_clearance" }
}