/**
 * Source d'une action de la fiche (téléchargement, vérification).
 *
 * **La fiche prime sur la sélection du catalogue, toujours.** Mesuré le
 * 2026-10-18 : une fiche ouverte depuis l'historique n'a aucune source
 * sélectionnée, et `downloadChapter` sortait sur
 * `if (!openManga || !selected) return` — le bouton ne faisait rien, sans
 * message. La sélection peut en plus désigner une AUTRE source (on ouvre une
 * fiche d'historique alors qu'une source est encore choisie) : la faire primer
 * enverrait le téléchargement au mauvais endroit.
 *
 * Une source vide n'est pas une source : elle est traitée comme absente.
 */
export function sourceDeLaFiche(fiche: { source: string } | null | undefined): string | null {
  return fiche?.source || null;
}
