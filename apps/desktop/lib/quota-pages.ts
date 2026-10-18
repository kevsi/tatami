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


/** Admet la page demandee **et une fenetre de voisins**, pour qu'un saut
 *  lointain n'aboutisse pas a une bande vierge.
 *
 *  Le defaut mesure (2026-10-17) : un saut vers la fin n'admettait que la
 *  page cible. La fenetre de montage se deplacait sur elle, mais ses voisins
 *  (les pages visibles autour) n'avaient pas de src : pas de chargement,
 *  donc pas de onLoad, donc plus aucune admission - la bande restait vide
 *  jusqu'a ce que l'utilisateur remonte manuellement.
 *
 *  En admettant la cible et ses voisins immediats, toutes les pages visibles
 *  recoivent leur src d'emblée et se chargent en parallele.
 */
export function admettreCibleEtVoisins(
  prec: readonly number[],
  cible: number,
  nbPages: number,
  rayon: number = 2,
): number[] {
  if (!Number.isInteger(cible) || cible < 0 || nbPages <= 0) return prec.slice();
  const resultat = new Set(prec);
  const debut = Math.max(0, cible - rayon);
  const fin = Math.min(nbPages - 1, cible + rayon);
  for (let i = debut; i <= fin; i += 1) resultat.add(i);
  return [...resultat];
}

/** L'ensemble des pages qui ont le droit d'avoir un `src` **maintenant**.
 *
 *  ## Le défaut mesuré le 2026-10-18
 *
 *  L'admission ne se faisait que sur `onLoad` (`admettreUne`) : une page par
 *  décodage. Tant que les chargements suivaient le défilement, la file
 *  avançait ; dès qu'ils prenaient du retard, la fenêtre montée ne contenait
 *  plus **aucune** page admise — pas de `src`, donc pas de chargement, donc pas
 *  de `onLoad`, donc plus rien n'était admis. Mesuré sur un chapitre local de
 *  23 pages : **6 requêtes au total**, puis 7 pages montées, 0 décodée, bande
 *  vide. C'est le même cercle vicieux que le saut de page, sous une autre
 *  forme : une garantie qui ne se déclenche que sur un événement qui ne peut
 *  plus arriver.
 *
 *  ## Pourquoi la fenêtre, et seulement en virtualisation
 *
 *  En bande virtualisée, la fenêtre montée **est** ce que l'utilisateur
 *  regarde, et elle est bornée (marge 3 + portée de 4 200 px, ~8 pages) :
 *  l'admettre en entier ne fait pas plus de requêtes que l'ouverture (6 pages).
 *  Hors virtualisation, `fenetreBande` rend **toutes** les pages — l'union
 *  lancerait donc 303 requêtes d'un coup, la rafale que le quota existe pour
 *  éviter. On y garde la file « une par décodage ».
 */
export function pagesAffichees(
  admis: readonly number[],
  fenetre: readonly number[],
  virtualisee: boolean,
): ReadonlySet<number> {
  const ensemble = new Set(admis);
  if (virtualisee) {
    for (const i of fenetre) ensemble.add(i);
  }
  return ensemble;
}
