/**
 * Un chapitre, du point de vue de l'interface : ce qui décide de son identité.
 *
 * `url` est le seul champ que le reste de l'écran utilise comme identité :
 * la case à cocher (`checked: Set<string>`), le chapitre en cours
 * (`progression.chapter_url`), le téléchargement et le signet. Deux lignes de
 * même `url` sont donc **le même chapitre** pour toutes les actions possibles —
 * les afficher deux fois n'ajoute rien et fait agir l'utilisateur deux fois.
 */

/** Les champs dont l'interface a besoin ; le moteur en rend plus. */
export type ChapitreAffiche = {
  url: string;
  name?: string;
  number?: number | null;
  [cle: string]: unknown;
};

/**
 * Retire les chapitres de même `url`, en gardant le premier.
 *
 * ## Pourquoi, mesuré le 2026-10-15
 *
 * Sur la fiche « One Piece » (SCANVF), la source rend des **URL de chapitre en
 * double** : 40 à 100 avertissements `Encountered two children with the same
 * key` selon les rendus (audit `docs/audit-ux-2026-10-15.md`, F6). Deux
 * conséquences, pas une : React peut attacher l'état d'une ligne à une autre,
 * et l'utilisateur voit des doublons qu'il ne peut pas expliquer.
 *
 * **Pourquoi dédoublonner plutôt que corriger la clé.** Une clé robuste
 * (`url` + indice) supprimerait l'avertissement React en laissant les doublons
 * à l'écran — l'avertissement est le **symptôme**, pas le défaut. Et l'identité
 * du reste de l'écran étant l'URL, deux lignes de même URL sont indiscernables
 * pour l'utilisateur : cocher l'une coche l'autre, télécharger l'une télécharge
 * l'autre. Les afficher deux fois est donc une inexactitude, pas une option.
 *
 * L'ordre est conservé : le premierpassage reste visible, donc la liste ne
 * bouge pas sous les doigts de l'utilisateur.
 */
export function chapitresSansDoublons<T extends ChapitreAffiche>(chapitres: T[]): T[] {
  const vues = new Set<string>();
  const uniques: T[] = [];
  for (const c of chapitres) {
    if (vues.has(c.url)) continue;
    vues.add(c.url);
    uniques.push(c);
  }
  return uniques;
}