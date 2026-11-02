// Choix du chapitre à précharger à l'ouverture d'une fiche (chantier perf
// 2026-10-19, B).
//
// La règle vit ici, dans `lib/`, parce qu'elle **déclenche une requête
// réseau** : dans le composant, aucune porte ne pouvait la vérifier (même
// raison que `lib/signet-cle.ts`). Elle est pure : une liste de chapitres et
// l'URL de reprise entrent, le chapitre à demander sort.

/** Le chapitre à précharger : celui de la reprise s'il est bien dans la liste
 *  — une source peut avoir réordonné ou retiré des chapitres, et précharger
 *  une URL absente ferait une requête pour rien — le **premier** sinon. `null`
 *  quand la liste est vide : il n'y a alors rien à préparer. */
export function ciblePrechargement<T extends { url: string }>(
  chapitres: readonly T[],
  chapitreReprise: string | null | undefined,
): T | null {
  if (chapitres.length === 0) return null;
  if (chapitreReprise) {
    const reprise = chapitres.find((c) => c.url === chapitreReprise);
    if (reprise) return reprise;
  }
  return chapitres[0];
}

/**
 * Les chapitres **voisins** d'une cible, pour précharger leur liste de pages.
 *
 * Mesuré le 2026-11-02 : ouvrir un chapitre jamais lu coûte le parsing de sa
 * page (`pages`) — **11 299 ms** sur LELMANGA — contre 310 ms quand la réponse
 * est déjà sur le disque. La fiche ne préchargeait que le chapitre de reprise :
 * enchaîner sur le suivant repayait l'attente entière.
 *
 * `rayon = 1` donne le précédent et le suivant (deux requêtes) : assez pour
 * l'enchaînement courant, sans transformer l'ouverture d'une fiche en rafale.
 */
export function chapitresVoisins<T>(
  chapitres: readonly T[],
  index: number,
  rayon = 1,
): T[] {
  if (!Number.isInteger(index) || index < 0 || index >= chapitres.length) return [];
  const voisins: T[] = [];
  for (let d = 1; d <= rayon; d += 1) {
    if (index - d >= 0) voisins.push(chapitres[index - d]);
    if (index + d < chapitres.length) voisins.push(chapitres[index + d]);
  }
  return voisins;
}
