package org.tatami.engine

/**
 * L'origine à annoncer dans le `Referer` d'une **image** : celle de la
 * **source**, jamais celle de l'image.
 *
 * Mesuré le 2026-10-16 sur HENTAIZONE, dont les chapitres sont servis par un
 * autre domaine (`scanscloud.xyz`) :
 *
 * ```
 * Referer absent                        -> 403, 4 571 octets, text/html
 * Referer = https://hentaizone.xyz/     -> 200, 1 701 369 octets, image/jpeg
 * Referer = https://scanscloud.xyz/     -> 403
 * Referer = https://example.org/        -> 403
 * ```
 *
 * L'agent utilisateur n'y change rien : la même image répond 403 sous Chrome et
 * 200 sous Firefox **si** le referer est sur le domaine de la source. Ce qui
 * manque, c'est donc l'en-tête — et le chemin natif envoyait jusqu'ici
 * **`l'URL de l'image` elle-même**, c'est-à-dire l'image qui se présente comme
 * son propre site. Le chemin scripté documente depuis le 2026-10-14 que c'est
 * exactement ce qu'un serveur cherche à écarter.
 *
 * Symptôme côté utilisateur, mesuré : la liste des chapitres s'affiche et
 * **aucune page n'est lisible** — un `SOURCE_BLOCKED` sur chaque `fetchImage`.
 *
 * N'importe quelle page du domaine de la source convient, y compris
 * inexistante (mesuré) : seule l'**origine** compte. Le repli — l'origine de
 * l'image — reste le bon cas quand la source n'a pas de domaine lisible, et le
 * seul qui marche quand tout est servi depuis un même domaine (mesuré le
 * 2026-10-13 sur rimuscan.fr).
 *
 * Fonction pure et séparée : elle est éprouvée sur les formes réelles
 * relevées, pas sur ce qu'on suppose.
 */
internal fun refererImage(domaineSource: String?, urlImage: String): String {
    val depuisSource = domaineSource
        ?.trim()
        ?.takeIf { it.isNotEmpty() }
        ?.let { site_a_referer("https://" + it) }
    return depuisSource
        ?: site_a_referer(urlImage)
        ?: ""
}