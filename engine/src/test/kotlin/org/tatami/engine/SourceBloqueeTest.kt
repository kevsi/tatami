package org.tatami.engine

import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertNull
import org.junit.jupiter.api.Test

/**
 * Le classement d'un blocage Cloudflare, distinct d'un captcha.
 *
 * ## Pourquoi cette distinction
 *
 * `CloudFlareHelper` rend trois issues, et jusqu'au 2026-10-13 notre
 * intercepteur en traitait deux comme une seule : `PROTECTION_BLOCKED`
 * finissait en `CloudFlareProtectionException`, comme `PROTECTION_CAPTCHA`.
 * L'interface ouvrait donc une fenêtre de vérification pour une source que
 * Cloudflare **refuse sans défi** — l'utilisateur résolvait un puzzle qui ne
 * menait à rien.
 *
 * C'est la distinction que fait `Kotatsu-Redo`
 * (`core/network/CloudFlareInterceptor.kt`,
 * `CloudFlareBlockedException` contre `CloudFlareProtectedException`), et que
 * l'aide-ajoutée `sourceBloqueeDe` transmet côté moteur.
 */
class SourceBloqueeTest {

    @Test
    fun `un blocage direct est trouve`() {
        val e = CloudFlareBlockedException("https://toonfr.com/x")
        assertEquals("https://toonfr.com/x", sourceBloqueeDe(e))
    }

    @Test
    fun `un blocage emballe dans une IOException est retrouve`() {
        // OkHttp 5 (KMP) emballe parfois l'exception d'intercepteur et ne garde
        // que la cause : même forme que pour le challenge.
        val reel = CloudFlareBlockedException("https://kagane.org/y")
        val emballe = java.io.IOException("canceled", reel)
        assertEquals("https://kagane.org/y", sourceBloqueeDe(emballe))
    }

    @Test
    fun `un captcha n'est pas un blocage`() {
        // **Contrôle positif.** Sans ce test, une régression qui rendrait tous
        // les cas en `SOURCE_BLOCKED` ferait passer les deux precedents, et le
        // mot-clé `blocked` serait emporté par un motif un peu large.
        assertNull(sourceBloqueeDe(CloudFlareProtectionException("https://x.fr/y")))
        assertNull(challengeDe(CloudFlareBlockedException("https://x.fr/y")))
    }

    @Test
    fun `un rejeu refuse n'est pas un blocage`() {
        assertNull(sourceBloqueeDe(CloudFlareUnreplayableException("https://x.fr/y")))
    }

    @Test
    fun `le classement met le blocage avant le challenge dans une cause partagee`() {
        // Le cas qui decide de l'ordre dans `Server.kt` : une exception dont la
        // chaîne de causes contient les deux. Le blocage doit gagner, sinon la
        // fenêtre s'ouvre pour une source bloquee.
        val partage = CloudFlareBlockedException("https://bloque.fr/z")
        assertEquals("https://bloque.fr/z", sourceBloqueeDe(partage))
    }
}