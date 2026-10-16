/**
 * Combien de temps l'écran peut-il afficher « chargement » avant de devoir
 * expliquer quelque chose.
 *
 * ## Pourquoi, mesuré le 2026-10-16
 *
 * En ouvrant la source `READCOMICSONLINE`, l'écran est resté **60 s** avec les
 * 6 squelettes de la grille, 0 carte, **aucun bandeau** et **aucune
 * notification** — pendant qu'une webview de transport attendait une
 * vérification Cloudflare que rien ne signalait (`audit-ux-2026-10-15.md`, F3 et
 * F13 ; relevé `02-readcomicsonline.json` / `05-etat-vivant-f3.cjs`).
 *
 * Deux choses en découlent :
 *
 *  1. **Un chargement qui dure n'est pas une information.** Tant qu'il dure
 *     quelques secondes, les squelettes suffisent. Au-delà, l'utilisateur
 *     regarde une grille vide et une fenêtre qu'il n'a pas demandée : il ne peut
 *     ni deviner qu'il doit agir, ni savoir si le programme est cassé.
 *  2. **Le seuil est une décision, pas une évidence.** 8 s sortent des mesures :
 *     le catalogue met 1,3 s à se remplir à froid et 2,3 s à chaud
 *     (`PLAN.md`, 2026-10-06), donc 8 s c'est déjà trois fois le pire cas connu.
 *     Au-delà de cette limite, le mot « lent » est vrai.
 *
 * La fonction ne rend qu'un booléen : elle est testable sans composant, sans
 * minuteur et sans horloge réelle — le composant lui passe l'écart mesuré.
 */

/** Trois fois le pire temps de chargement mesuré (2,3 s à chaud). */
export const SEUIL_CHARGEMENT_MS = 8000;

/**
 * `true` quand l'attente dépasse le seuil.
 *
 * `attenteMs` vient d'une horloge, pas d'un compteur de rendus : compter les
 * rendus mesurerait la fréquence de rendu de React, pas le temps vécu.
 */
export function attenteDepasseLeSeuil(
  attenteMs: number,
  seuilMs: number = SEUIL_CHARGEMENT_MS,
): boolean {
  return attenteMs >= seuilMs;
}