/**
 * Quel chapitre téléchargé peut être supprimé sans perte ?
 *
 * La règle est **étroite volontairement**. L'application ne connaît qu'une
 * position de lecture par webtoon (`derniere_lecture` : chapitre courant, page
 * courante, nombre de pages). Elle ne sait pas quels chapitres *précédents* ont
 * été lus, et l'ordre de la liste dépend de la source (souvent du plus récent
 * au plus ancien). Supprimer « tout ce qui précède » risquerait donc d'effacer
 * des chapitres jamais ouverts.
 *
 * On ne supprime donc que ce qui est **prouvé** : le chapitre courant, et
 * seulement s'il a été lu jusqu'à la dernière page. C'est peu, mais c'est
 * irréversible — un `.cbz` supprimé ne revient pas.
 */
export type PositionLecture = {
  chapter_url?: string | null;
  page_index?: number | null;
  page_count?: number | null;
};

export function chapitreLu<T extends { url: string }>(
  chapitres: readonly T[],
  position: PositionLecture | null | undefined,
): T | null {
  const url = position?.chapter_url;
  if (!url) return null;
  const chapitre = chapitres.find((c) => c.url === url);
  if (!chapitre) return null;
  const index = position?.page_index ?? 0;
  const total = position?.page_count ?? 0;
  if (total <= 0) return null;
  // `page_index` est base 0 : la dernière page est `total - 1`.
  return index >= total - 1 ? chapitre : null;
}