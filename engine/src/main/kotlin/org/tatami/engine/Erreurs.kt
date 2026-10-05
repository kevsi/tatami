package org.tatami.engine

import org.koitharu.kotatsu.parsers.exception.AuthRequiredException

/**
 * Retrouve une exception connue dans la chaîne de `cause`, même enveloppée.
 *
 * ## Pourquoi
 *
 * Un `catch (e: CloudFlareProtectionException)` n'attrape **rien** dès que le
 * parser emballe l'exception : `CloudFlareProtectionException` devient une
 * `IOException("canceled due to …")`. Mesuré le 2026-10-05 sur MANGANATO et
 * KAGANE, qui renvoyaient `INTERNAL: IOException: canceled due to
 * CloudFlareProtectionException` — un vrai challenge Cloudflare rapporté comme
 * panne interne, donc la fenêtre de résolution de la tâche 0.11 ne s'ouvrait
 * jamais.
 *
 * OkHttp 5 (KMP) peut aller plus loin et **perdre la cause** : le wrapper ne
 * garde que le texte. On cherche donc aussi dans le message, sur le format
 * exact de nos propres exceptions.
 *
 * La boucle est bornée : une chaîne de causes circulaire ferait otherwise
 * tourner le serveur sur une seule requête.
 */
internal inline fun <reified T : Throwable> retrouverDansLesCauses(t: Throwable): T? {
    var courant: Throwable? = t
    var etapes = 0
    while (courant != null && etapes++ < 20) {
        if (courant is T) return courant
        val suivant = courant.cause
        if (suivant === courant) return null // boucle : on sort plutot que de tourner
        courant = suivant
    }
    return null
}

/** Un challenge Cloudflare, que l'exception soit directe ou emballée. */
internal fun challengeDe(t: Throwable): String? =
    retrouverDansLesCauses<CloudFlareProtectionException>(t)?.url
        ?: MOTIF_CHALLENGE.find(t.message ?: "")?.groupValues?.get(1)

/**
 * Un chapitre protégé par compte.
 *
 * `AuthRequiredException` n'est levée que sur HTTP **401** par 16 parseurs, et
 * ne peut pas correspondre à `CHALLENGE_REQUIRED` : celle-ci exige 403/503 **et**
 * un marqueur HTML Cloudflare. Sans code dédié, l'utilisateur voyait une erreur
 * générique et Rust n'avait rien à lui proposer.
 */
internal fun authRequise(t: Throwable): AuthRequiredException? =
    retrouverDansLesCauses<AuthRequiredException>(t)

/** Le message que nos exceptions écrivent, retenu par OkHttp quand il perd la cause. */
private val MOTIF_CHALLENGE = Regex("Cloudflare protection at (\\S+)")
