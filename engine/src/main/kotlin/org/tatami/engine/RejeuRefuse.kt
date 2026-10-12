package org.tatami.engine

import java.util.Locale
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
 * Le garde-fou reste **court par construction** : une entrée
 * expirée est retirée à la lecture, donc la mémoire ne croît pas avec les
 * rejets.
 *
 * ## Qui efface
 *
 * `setCookies` enregistre une résolution et efface les refus antérieurs du
 * domaine. Une réponse normale efface les refus et la fraîcheur mémorisée pour
 * les domaines apparentés : l'état reflète ainsi le dernier résultat observé.
 * Si une page publique répond normalement mais qu'une page protégée reste
 * refusée, ce 200 peut donc rendre le 403 suivant à nouveau résoluble et rouvrir
 * une fois la vérification ; la réponse normale est prioritaire sur l'ancien
 * état de challenge.
 */
internal object RejeuRefuse {

    /** Durée de mémorisation d'un échec de rejeu. */
    const val TTL_MS: Long = 30L * 60L * 1000L

    /**
     * Une clearance ne prouve un rejeu que juste après son injection :
     * au-delà, un nouveau challenge doit pouvoir être résolu normalement.
     * Cinq minutes laissent une marge généreuse au rejeu immédiat de
     * l'application, sans traiter un ancien cookie encore valide comme récent.
     */
    const val FRAICHEUR_MS: Long = 5L * 60L * 1000L

    private val echecs = ConcurrentHashMap<String, Long>()
    private val resolutions = ConcurrentHashMap<String, Long>()

    /** L'horloge, injectable pour que les tests n'aient pas à attendre 30 min. */
    @Volatile
    var maintenant: () -> Long = System::currentTimeMillis

    /** Canonicalise la clé partagée par les cookies, la fraîcheur et les refus. */
    fun normaliserHote(hote: String): String {
        val hoteNettoye = hote.trim().removePrefix(".")
        val sansPort = when {
            hoteNettoye.startsWith("[") ->
                hoteNettoye.substringAfter('[').substringBefore(']')
            hoteNettoye.count { it == ':' } == 1 -> hoteNettoye.substringBeforeLast(':')
            else -> hoteNettoye
        }
        return sansPort.trimEnd('.').lowercase(Locale.ROOT)
    }

    /** Enregistre une résolution réussie et efface les refus de son domaine. */
    fun resolutionReussie(hote: String) {
        val cle = normaliserHote(hote)
        if (cle.isEmpty()) return
        oublier(cle)
        resolutions[cle] = maintenant()
    }

    /** Vrai si une résolution de challenge récente existe pour ce domaine. */
    fun resolutionRecente(hote: String): Boolean {
        val cle = normaliserHote(hote)
        val maintenantMs = maintenant()
        for (domaine in domainesApplicables(cle)) {
            val resoluLe = resolutions[domaine] ?: continue
            if (maintenantMs < resoluLe || maintenantMs - resoluLe >= FRAICHEUR_MS) {
                resolutions.remove(domaine, resoluLe)
            } else {
                return true
            }
        }
        return false
    }

    /** Vrai si un rejeu a déjà échoué sur cet hôte ou son domaine parent. */
    fun estRefuse(hote: String): Boolean {
        val maintenantMs = maintenant()
        for (cle in domainesApplicables(normaliserHote(hote))) {
            val expire = echecs[cle] ?: continue
            if (maintenantMs >= expire) {
                echecs.remove(cle, expire)
            } else {
                return true
            }
        }
        return false
    }

    /** Mémorise l'échec. Un rejeu plus récent prolonge la fenêtre. */
    fun marquer(hote: String) {
        val cle = normaliserHote(hote)
        if (cle.isNotEmpty()) echecs[cle] = maintenant() + TTL_MS
    }

    /** Oublie les refus et résolutions récentes des domaines apparentés. */
    fun oublier(hote: String) {
        val cle = normaliserHote(hote)
        if (cle.isEmpty()) return
        echecs.keys.removeIf { sontDomainesApparentes(it, cle) }
        resolutions.keys.removeIf { sontDomainesApparentes(it, cle) }
    }

    /** Visible pour les tests : vide le registre sans passer par l'horloge. */
    fun vider() {
        echecs.clear()
        resolutions.clear()
    }

    /** Nombre d'entrées vivantes — borne la mémoire et se teste. */
    fun taille(): Int = echecs.keys.count { estRefuse(it) }

    private fun domainesApplicables(hote: String): Sequence<String> = sequence {
        yield(hote)
        if (hote.contains(':') || hote.matches(Regex("[0-9.]+"))) return@sequence
        var point = hote.indexOf('.')
        while (point >= 0) {
            val parent = hote.substring(point + 1)
            if (parent.contains('.')) yield(parent)
            point = hote.indexOf('.', point + 1)
        }
    }

    private fun sontDomainesApparentes(a: String, b: String): Boolean =
        a == b || a.endsWith(".$b") || b.endsWith(".$a")
}
