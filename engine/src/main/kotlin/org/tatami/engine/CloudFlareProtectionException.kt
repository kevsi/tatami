package org.tatami.engine

// Transposée de kotatsu-dl CloudFlareProtectedException : signale qu'une source
// exige une résolution (captcha / challenge) que le moteur ne fait pas lui-même.
// Le serveur la convertit en erreur CHALLENGE_REQUIRED (contrat v0).
class CloudFlareProtectionException(
    val url: String,
) : java.io.IOException("Cloudflare protection at $url")
