package org.tatami.engine

import java.util.concurrent.ConcurrentHashMap

/**
 * Mémorise, par hôte, qu'un **rejeu** de challenge a échoué — c'est-à-dire que
 * l'utilisateur a résolu la vérification et que la requête rejouée a été
 * refusée quand même.
 *
 * ## Pourquoi c'est distinct de « challenge demandé »
 *
 * Un `403 cf-mitigated` reçu **sans** `cf_clearance` est un appel normal : il
 * faut ouvrir la fenêtre, l'utilisateur résout, et le rejeu passe. Un `403`
 * reçu **avec** `cf_clearance` dans le Jar est autre chose : la vérification a
 * déjà été faite, et le site refuse quand même. Refuser de distinguer les deux
 * produit le cycle mesuré : bandeau → puzzle → fenêtre qui se ferme → rien.
 *
 * ## La durée de vie, et pourquoi 30 minutes
 *
 * Ce n'est pas « le site nous refuse pour toujours » : une clearance peut
 * expirer, le site peut changer de configuration, le réseau peut changer. Mais
 * 30 minutes est assez long pour que le cycle ne se répète pas sous les yeux de
 * l'utilisateur, et assez court pour qu'une source revienne d'elle-même.
 *
 * Le garde-fou reste nonetheless **court par construction** : une entrée
 * expirée est retirée à la lecture, donc la mémoire ne croît pas avec les
 * rejets.
 *
 * ## Qui efface
 *
 * `setCookies` — c'est-à-dire **une résolution réussie**, donc une action
 * explicite de l'utilisateur. C'est le seul endroit qui efface : sans ça, un
 * site quioudrait ré-autoriser ne pourrait jamais le faire, et l'utilisateur
 * n'aurait aucun moyen de dire « j'ai changé d'avis, réessaie ».
 */
internal object RejeuRefuse {

    /** Durée de mémorisation d'un échec de rejeu. */
    const val TTL_MS: Long = 30L * 60L * 1000L

    private val echecs = ConcurrentHashMap<String, Long>()

    /** L'horloge, injectable pour que les tests n'aient pas à attendre 30 min. */
    @Volatile
    var maintenant: () -> Long = System::currentTimeMillis

    /** Vrai si un rejeu a déjà échoué pour cet hôte et que la fenêtre vit encore. */
    fun estRefuse(hote: String): Boolean {
        val expire = echecs[hote] ?: return false
        if (maintenant() >= expire) {
            echecs.remove(hote, expire)
            return false
        }
        return true
    }

    /** Mémorise l'échec. Un rejeu plus récent prolonge la fenêtre. */
    fun marquer(hote: String) {
        echecs[hote] = maintenant() + TTL_MS
    }

    /**
     * Oublie l'échec — appelé par `setCookies`, donc par une résolution réussie.
     *
     * Sans ce point, un site qui ré-autorise après une erreur de configuration
     * resterait bloqué 30 minutes, et l'utilisateur n'aurait aucun recours.
     */
    fun oublier(hote: String) {
        echecs.remove(hote)
    }

    /** Visible pour les tests : vide le registre sans passer par l'horloge. */
    fun vider() {
        echecs.clear()
    }

    /** Nombre d'entrées vivantes — borne la mémoire et se teste. */
    fun taille(): Int = echecs.keys.count { estRefuse(it) }
}