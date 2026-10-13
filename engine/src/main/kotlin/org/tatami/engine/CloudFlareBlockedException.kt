package org.tatami.engine

/**
 * Cloudflare **refuse** la requête sans proposer de vérification.
 *
 * ## Ce que cela change pour l'utilisateur
 *
 * `CloudFlareHelper` distingue trois issues, et notre intercepteur en
 * traitait deux comme une seule jusqu'au 2026-10-13 :
 *
 * | issue | constante | ce que c'est |
 * |---|---|---|
 * | rien | `PROTECTION_NOT_DETECTED` (0) | réponse normale |
 * | **captcha** | `PROTECTION_CAPTCHA` (1) | il y a un défi à résoudre |
 * | **bloqué** | `PROTECTION_BLOCKED` (2) | le site refuse, rien à résoudre |
 *
 * Les deux derniers se répondaient par [CloudFlareProtectionException], donc
 * l'interface ouvrait une fenêtre de vérification pour une source **bloquée** :
 * l'utilisateur résolvait un défi, le cookie partait, et la source restait
 * inaccessible. Le message annonçait une vérification à passer alors qu'il
 * n'y en avait pas.
 *
 * C'est la distinction que fait `Kotatsu-Redo`
 * (`core/network/CloudFlareInterceptor.kt`, `CloudFlareBlockedException` contre
 * `CloudFlareProtectedException`) : un blocage par règle de pare-feu n'est pas
 * un captcha, et les deux ne se traitent pas de la même façon.
 *
 * ## Pourquoi le message doit suffire seul
 *
 * OkHttp 5 perd parfois la cause et ne garde que le texte (mesuré le
 * 2026-10-05 sur MANGANATO et KAGANE), donc le motif est écrit pour être
 * retrouvé dans une chaîne de causes perdue — comme pour
 * [CloudFlareUnreplayableException].
 */
class CloudFlareBlockedException(
    val url: String,
) : IllegalStateException("Cloudflare blocked at $url")