/**
 * Ordre de téléchargement d'un webtoon.
 *
 * Mesuré le 2026-11-02 : un webtoon coché depuis sa fiche se téléchargeait dans
 * l'ordre de la source — souvent le plus récent d'abord, donc **pas** le
 * chapitre que l'utilisateur est en train de lire. Sur un webtoon de 121
 * chapitres, celui qu'on voulait lire arrivait après des dizaines d'autres.
 *
 * La règle est pure : elle réordonne, ne filtre rien et ne prive personne d'un
 * chapitre. Le chapitre en cours passe devant ; le reste garde son ordre.
 */
export function prioriserChapitreEnCours<T extends { url: string }>(
  chapitres: readonly T[],
  enCours: string | null | undefined,
): T[] {
  const copie = chapitres.slice();
  if (!enCours) return copie;
  const index = copie.findIndex((c) => c.url === enCours);
  if (index <= 0) return copie;
  const [courant] = copie.splice(index, 1);
  return [courant, ...copie];
}