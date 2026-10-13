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
 * Kotatsu-Redo (`core/network/CloudFlareInterceptor.kt`) distingue **trois**
 * issues de `CloudFlareHelper`, et rend trois exceptions différentes :
 *
 * | issue | constante | rendu |
 * |---|---|---|
 * | réponse normale | `PROTECTION_NOT_DETECTED` | la réponse |
 * | **captcha** | `PROTECTION_CAPTCHA` | `CloudFlareProtectedException` |
 * | **bloqué** | `PROTECTION_BLOCKED` | `CloudFlareBlockedException` |
 *
 * Jusqu'au 2026-10-13 nous n'en distinguions que **deux** : bloqué et captcha
 * finissaient tous deux en `CloudFlareProtectionException`. L'interface ouvrait
 * donc une fenêtre de vérification pour une source **bloquée** — l'utilisateur
 * résolvait un défi, le cookie partait, et la source restait inaccessible, avec
 * un message qui annonçait une vérification là où il n'y en avait aucune. Un
 * blocage par regle de pare-feu n'est pas un captcha.
 *
 * ## Ce que dit une résolution réussie suivie d'un refus
 *
 * Mesuré le 2026-10-13 sur TOONFR (application reelle) : le rejeu reçoit 403 et
 * `CloudFlareHelper` répond `PROTECTION_NOT_DETECTED`, donc la page ne porte
 * pas de marqueur de protection. Le parseur explose en `HttpStatusException` et
 * le moteur répondait `INTERNAL` — d'où le message qui suit.
 *
 * ## Où lit-on la présence du cookie
 *
 * **Pas** dans l'en-tête `Cookie` de la requête : `BridgeInterceptor` d'OkHttp
 * l'ajoute **après** tous les intercepteurs d'application, donc il est encore
 * vide ici. On interroge donc le magasin.
 */
internal class CloudFlareInterceptor(
    private val cookieJar: CookieJar,
) : Interceptor {

    override fun intercept(chain: Interceptor.Chain): Response {
        val request = chain.request()
        val response = chain.proceed(request)
        val protection = CloudFlareHelper.checkResponseForProtection(response)

        if (protection == CloudFlareHelper.PROTECTION_NOT_DETECTED) {
            val clearance = porteUneClearance(request.url)
            // **Le 403 du rejeu, quand Cloudflare ne le reconnaît plus comme
            // une protection.** Mesuré le 2026-10-13 sur TOONFR : la première
            // requete est bien detectee (`CHALLENGE_REQUIRED`), la fenêtre
            // aboutit, `cf_clearance` est lu et appliqué… puis le rejeu reçoit
            // **403** sans le moindre marqueur de protection.
            //
            // Ce que l'utilisateur voyait : « 1 cookie transmis », puis TOONFR
            // bloqué, sans aucun message exploitable. Et comme `INTERNAL` n'est
            // ni `CHALLENGE_REQUIRED` ni `CHALLENGE_UNREPLAYABLE`, la chaîne
            // existante pour dire « le rejeu est refusé, ne redemande pas » ne
            // se déclenchait jamais.
            //
            // Le classement ne dépend d'aucun marqueur Cloudflare : **la
            // requête portait déjà une `cf_clearance`** (la vérification a donc
            // été faite) et le site répond 403/503 malgré tout. C'est la
            // définition d'un rejeu refusé. Sans ce cookie, le 403 reste un 403
            // ordinaire — un filtre d'age, un Referent manquant — et le
            // comportement ne change pas.
            if (clearance != null && (response.code == 403 || response.code == 503)) {
                val urlRefusee = response.request.url.toString()
                // **Que dit Cloudflare ?** `peekBody` lit sans consommer le
                // corps, et la reponse est fermee juste apres de toute facon.
                //
                // Mesure du 2026-10-13 : apres resolution, le journal disait
                // `403` et rien d'autre. Or Cloudflare **nomme** ses refus dans
                // le corps de la page — `1010` (signature de navigateur
                // banniee), `1020` (regle de pare-feu), `1022` (methode),
                // `1006`… Chacune a une cause et un remede differents, et
                // choisir parmi elles sans les lire serait exactement l'erreur
                // que ce projet s'interdit.
                //
                // Aucune valeur de cookie n'est journalisee : des codes et du
                // texte, bornes a 4 Ko.
                val corps = runCatching { response.peekBody(4096).string() }.getOrDefault("")
                val codeCloudflare = Regex("(?i)cf-error-code[^>]*>\\s*(\\d{3,4})")
                    .find(corps)?.groupValues?.get(1)
                    ?: Regex("(?i)error\\s*(?:code)?\\s*[:#]?\\s*(\\d{3,4})").find(corps)
                        ?.groupValues?.get(1)
                val titre = Regex("(?i)<title>([^<]{0,90})").find(corps)
                    ?.groupValues?.get(1)?.trim()
                System.err.println(
                    "[challenge-debug] rejeu refuse hote=${request.url.host} code=${response.code} " +
                        "cf_error=<$codeCloudflare> titre=${titre ?: "-"} " +
                        "ray=${response.header("cf-ray") ?: "-"} octets=${corps.length}",
                )
                response.close()
                RejeuRefuse.marquer(clearance.domain)
                throw CloudFlareUnreplayableException(url = urlRefusee)
            }
            // **Une réponse normale annule l'échec précédent.** Sans cette
            // ligne, un site qui ré-autorise resterait bloqué jusqu'à
            // l'expiration, alors qu'il vient de nous répondre.
            RejeuRefuse.oublier(request.url.host)
            return response
        }

        // ── Protection détectée : la réponse ne doit jamais sortir d'ici ──
        val url = response.request.url.toString()
        response.close()

        if (protection == CloudFlareHelper.PROTECTION_BLOCKED) {
            // Pas de fenêtre, pas de mémoire de rejeu : il n'y a rien à
            // résoudre.
            throw CloudFlareBlockedException(url = url)
        }

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

    /** Le magasin va-t-il joindre une `cf_clearance` à une requête vers cette URL ? */
    private fun porteUneClearance(url: HttpUrl): Cookie? =
        cookieJar.loadForRequest(url).firstOrNull { it.name == "cf_clearance" }
}
