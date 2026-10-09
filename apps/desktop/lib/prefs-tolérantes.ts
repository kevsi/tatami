/**
 * Lecture tolérante d'une liste de sources dans `localStorage`.
 *
 * Le défaut mesuré le 2026-10-09 : `loadPrefs()` faisait
 * `JSON.parse(rawSources)` **sans garde**. Or cette fonction est appelée par
 * l'effet de montage, et elle est suivie de `chargerSources()`,
 * `refreshDownloads()` et des trois écouteurs `listen` (`maj://`,
 * `download://`). Une valeur corrompue faisait donc échouer l'effet **avant**
 * eux : l'application démarrait sans bibliothèque, sans téléchargements et sans
 * écouteur d'événements, sans le moindre message.
 *
 * Une préférence illisible doit se comporter comme une préférence absente
 * (`null`), jamais comme une panne de démarrage.
 *
 * Extrait de `page.tsx` parce que le défaut est **inatteignable par une porte** :
 * aucune porte ne lit une chaîne JSON, et l'effet de montage ne s'exécute que
 * dans le navigateur.
 */

/** Lit une liste de sources. `null` = pas de préférence, ou préférence inutilisable. */
export function lireListeSources(raw: string | null): string[] | null {
  if (raw === null || raw === '') return null;
  let valeur: unknown;
  try {
    valeur = JSON.parse(raw);
  } catch {
    return null;
  }
  if (!Array.isArray(valeur)) return null;
  // On ne garde la valeur que si elle était **déjà** propre. Réécrire une liste
  // partiellement valide ferait disparaître des sources en silence — un tableau
  // `["MANGADEX", 42]` doit être traité comme illisible, pas comme « MANGADEX ».
  if (!valeur.every((x) => typeof x === 'string')) return null;
  return valeur as string[];
}

/** Une liste lisible donne ses éléments ; une liste illisible donne `null`.
 *
 *  `null` n'est pas `[]` : `[]` voudrait dire « aucune source activée », ce que
 *  le code interprète comme un réglage volontaire de l'utilisateur et persiste.
 */
export function estLisible(raw: string | null): boolean {
  return lireListeSources(raw) !== null;
}
