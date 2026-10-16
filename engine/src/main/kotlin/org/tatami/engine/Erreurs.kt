package org.tatami.engine

import org.jsoup.HttpStatusException
import org.koitharu.kotatsu.parsers.exception.AuthRequiredException
import org.koitharu.kotatsu.parsers.exception.NotFoundException
import org.koitharu.kotatsu.parsers.exception.ParseException
import java.net.UnknownHostException

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
 * Un challenge résolu dont le **rejeu** est refusé.
 *
 * Distinct de [challengeDe] par le message : `not replayable`. OkHttp 5 perd
 * parfois la cause et ne garde que le texte (mesuré le 2026-10-05 sur
 * MANGANATO et KAGANE), donc le motif doit suffire seul.
 *
 * **L'ordre des deux tests compte** : les deux motifs partagent leur début
 * (`Cloudflare protection at …`). `challengeDe` est donc appelé sur les deux,
 * mais on teste `non rejouable` d'abord — sinon un rejeu refusé serait rendu
 * comme une demande de vérification, et la fenêtre se rouvrirait en boucle.
 * C'est exactement le défaut que ce code corrige.
 */
internal fun challengeNonRejouableDe(t: Throwable): String? =
    retrouverDansLesCauses<CloudFlareUnreplayableException>(t)?.url
        ?: MOTIF_CHALLENGE_NON_REJOUABLE.find(t.message ?: "")?.groupValues?.get(1)

/**
 * Détail supprimé de `challengeNonRejouableDe` : la chaîne de causes, le code
 * HTTP éventuel et le type. Sert au levier de diagnostic qui cerne **pourquoi**
 * Cloudflare refuse le rejeu, sans exposer de secret. Renvoyé en clair dans le
 * message parce que le front ne reçoit que `code`+`message` (pas `cause`).
 */
internal fun detailCauseChallengeNonRejouable(t: Throwable): String = buildString {
    append("type=${t::class.simpleName}")
    var cur: Throwable? = t
    var n = 0
    while (cur != null && n++ < 5) {
        val msg = cur.message?.take(160)
        if (!msg.isNullOrBlank()) append(" > ${cur::class.simpleName}:$msg")
        if (cur === cur.cause) break
        cur = cur.cause
    }
    val http = retrouverDansLesCauses<org.jsoup.HttpStatusException>(t)
    if (http != null) append(" | http=${http.statusCode}")
}

/**
 * Cloudflare **refuse** la requête sans proposer de vérification
 * (`CloudFlareHelper.PROTECTION_BLOCKED`).
 *
 * Distinct de [challengeDe] par le type, et surtout par ce qu'il faut en
 * faire : ici il n'y a **rien à résoudre**, donc l'interface ne doit ouvrir
 * aucune fenêtre. C'est la distinction que `Kotatsu-Redo` fait entre
 * `CloudFlareBlockedException` et `CloudFlareProtectedException` ; jusqu'au
 * 2026-10-13 nous rendions les deux en `CHALLENGE_REQUIRED`.
 */
internal fun sourceBloqueeDe(t: Throwable): String? =
    retrouverDansLesCauses<CloudFlareBlockedException>(t)?.url

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

/**
 * Cloudflare répond, mais son **origine** ne répond plus. Le domaine existe, le
 * TLS passe, Cloudflare affiche une page d'erreur 52x — la source n'est donc ni
 * morte, ni un problème de parseur : le serveur derrière est éteint.
 *
 * Ces quatre codes viennent tous de Cloudflare et disent la même chose.
 */
private val ORIGINES_INJOIGNABLES = setOf(521, 522, 523, 525)

/**
 * Classe une panne de source en code actionnable, et rend la cause qui l'a
 * décidée.
 *
 * ## Pourquoi, mesuré le 2026-10-12
 *
 * Sur les **98 sources FR**, 83 ne fonctionnaient pas et chacune remontait
 * `INTERNAL: <exception Java brute>`. Un utilisateur ne peut rien faire de
 * `HttpStatusException: . Status=523` : il ne sait ni si son réseau est en
 * cause, ni si la source est morte, ni si le programme est cassé.
 *
 * Le classement se fait sur le **type** de l'exception et son **code de
 * statut**, jamais sur une recherche de texte : `HttpStatusException` expose
 * `getStatusCode`, donc le classement tient même si un parseur réécrit son
 * message d'erreur.
 *
 * La cause est rendue **avec** le code pour que l'appelant compose un message
 * qui conserve le détail technique : c'est ce détail que
 * `Supervisor::is_transient` reconnaît côté Rust pour rejouer un échec réseau.
 * Un message réécrit casserait ce retry — le typage est donc additif.
 */
internal fun classerEchecSource(t: Throwable): Pair<String, Throwable>? {
    val morte = retrouverDansLesCauses<UnknownHostException>(t)
        ?: retrouverDansLesCauses<NotFoundException>(t)
        ?: retrouverDansLesCauses<HttpStatusException>(t)
            ?.takeIf { it.statusCode == 404 || it.statusCode == 410 }
    if (morte != null) return "SOURCE_DEAD" to morte

    val origine = retrouverDansLesCauses<HttpStatusException>(t)
        ?.takeIf { it.statusCode in ORIGINES_INJOIGNABLES }
    if (origine != null) return "ORIGIN_DOWN" to origine

    val perime = retrouverDansLesCauses<ParseException>(t)
    if (perime != null) return "PARSER_OUTDATED" to perime

    return null
}

/** Le message que nos exceptions écrivent, retenu par OkHttp quand il perd la cause. */
private val MOTIF_CHALLENGE = Regex("Cloudflare protection at (\\S+)")

/** Idem pour le rejeu refusé — voir [challengeNonRejouableDe] sur l'ordre des tests. */
private val MOTIF_CHALLENGE_NON_REJOUABLE = Regex("Cloudflare protection not replayable at (\\S+)")
