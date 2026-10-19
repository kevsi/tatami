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
