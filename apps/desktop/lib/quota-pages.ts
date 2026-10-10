/**
 * Quota de téléchargement des pages de la bande.
 *
 * ## Le défaut mesuré le 2026-10-10
 *
 * `admis` est l'ensemble des pages qui ont le droit de recevoir un `src` — donc
 * de demander une image. Il ne grandissait **que** sur un `onLoad` : les 6
 * premières pages à l'ouverture, puis une par décodage (le quota est une
 * **file**, pour ne pas saturer le `Mutex` du moteur, cf. `page.tsx`).
 *
 * Un **saut** vers une page lointaine cassait ce mécanisme. La fenêtre de
 * montage se déplace sur la cible, mais aucune de ses pages n'est admise :
 * pas de `src`, donc pas de chargement, donc pas de `onLoad`, donc plus rien
 * n'est admis. **La bande reste vierge.** Mesuré par l'utilisateur — saut de la
 * page 3 à la page 48, rien ne s'affiche, et il faut revenir à la page 3 puis
 * continuer la lecture pour que quelque chose arrive.
 *
 * C'est le même circularity que celui corrigé le même jour sur le changement de
 * chapitre : une garantie qui ne se déclenche que sur un événement qui ne peut
 * pas arriver n'en est pas une.
 */

/** Admet **une** page de plus quand un crédit se libère.
 *
 * `fenetre` est la fenêtre de pages montées : on n'admet que ce que
 * l'utilisateur est en train de regarder, jamais une page hors du champ.
 */
export function admettreUne(prec: readonly number[], fenetre: readonly number[]): number[] {
  const manque = fenetre.filter((i) => !prec.includes(i));
  return manque.length === 0 ? prec.slice() : [...prec, manque[0]];
}

/** Admet la page que l'utilisateur vient de **demander**.
 *
 * Un saut est une demande explicite : la page visée doit recevoir son `src`,
 * quoi que contienne la fenêtre à cet instant. Sans cela, un saut vers une
 * page lointaine ne déclenche aucun chargement — cf. le défaut ci-dessus.
 *
 * Une cible absurde (`-1`, `NaN`, un non-entier) est refusée : elle viendrait
 * d'un état incohérent, et une page qui n'existe pas n'a pas de jeton.
 */
export function admettreLaCible(prec: readonly number[], cible: number): number[] {
  if (!Number.isInteger(cible) || cible < 0) return prec.slice();
  if (prec.includes(cible)) return prec.slice();
  return [...prec, cible];
}
