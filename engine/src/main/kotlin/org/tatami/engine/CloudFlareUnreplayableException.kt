package org.tatami.engine

/**
 * Un challenge a été résolu, et le rejeu a été **refusé quand même**.
 *
 * Différent de [CloudFlareProtectionException] par la seule chose qui compte
 * pour l'utilisateur : ici, réessayer ne sert à rien. Ouvrir la fenêtre une
 * nouvelle fois produirait le même résultat — c'est le cycle « bandeau →
 * puzzle → fenêtre qui se ferme → rien », mesuré sur HENTAIZONE le 2026-10-11.
 *
 * Le message est donc écrit pour que le serveur puisse le retrouver dans une
 * chaîne de causes **perdue** (OkHttp 5 ne garde que le texte, cf.
 * [retrouverDansLesCauses]) : le motif est le même que pour le challenge
 * classique, complété d'un mot-clé.
 */
class CloudFlareUnreplayableException(
    val url: String,
) : java.io.IOException("Cloudflare protection not replayable at $url")