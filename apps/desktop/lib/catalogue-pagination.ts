/**
 * Pagination du catalogue : deux règles pures, testées hors React.
 *
 * Mesuré le 2026-11-02 sur le moteur réel : plusieurs sources personnalisées
 * ignorent `offset` et renvoient la **même liste** à chaque page
 * (`custom:lire-manga` : 43 fiches identiques quel que soit l'offset). Sans
 * dédoublonnage, la grille empilait des cartes jumelles et « Voir plus »
 * repromettait du vide.
 *
 * Et le rafraîchissement d'arrière-plan ne porte que la **première page** : il
 * remplaçait toute la grille par celle-ci, ramenant un catalogue paginé à une
 * seule page — le symptôme « ce catalogue n'a qu'une page ».
 */

/** Les entrées de `items` absentes de `prev`, dans l'ordre reçu. */
export function nouvellesEntrees<T>(
  prev: T[],
  items: T[],
  cle: (valeur: T) => string,
): T[] {
  const deja = new Set(prev.map(cle));
  return items.filter((item) => {
    const k = cle(item);
    if (deja.has(k)) return false;
    deja.add(k);
    return true;
  });
}

/**
 * Remplace le début du tableau par `pageFraiche` (la page 1 revalidée) et
 * recolle les pages suivantes, sans doublon.
 *
 * Si la page fraîche est au moins aussi longue que la grille, elle la remplace :
 * il n'y a plus de « pages suivantes » à conserver.
 */
export function fusionnerPremierePage<T>(
  prev: T[],
  pageFraiche: T[],
  cle: (valeur: T) => string,
): T[] {
  if (prev.length <= pageFraiche.length) return pageFraiche;
  const vus = new Set(pageFraiche.map(cle));
  const suite = prev.slice(pageFraiche.length).filter((item) => {
    const k = cle(item);
    if (vus.has(k)) return false;
    vus.add(k);
    return true;
  });
  return [...pageFraiche, ...suite];
}