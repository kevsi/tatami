/**
 * Apparence (thème sombre / clair).
 *
 * ## Pourquoi un fichier à part
 *
 * Le thème est une **préférence d'affichage** : il vit dans `localStorage`, pas
 * dans SQLite (AGENTS.md §6). Il est appliqué par un attribut sur `<html>`, et
 * toute la feuille de style bascule par cet attribut — c'est pourquoi
 * `globals.css` ne contient qu'un bloc `[data-apparence='clair']` qui surcharge
 * des tokens, et aucune règle dupliquée.
 *
 * ## Abonnement plutôt que `useState`
 *
 * Même mécanisme que la langue (`useSyncExternalStore`) : l'état vit dans le
 * module, pas dans le composant. Une version précédente gardait l'état dans
 * `useState` et appelait `setApparence` (la fonction du module) — le
 * `setApparence` du composant était donc **écrasé par l'import**, React
 * n'était jamais prévenu, et le `<select>` affichait une valeur que rien ne
 * portait. Mesuré le 2026-10-05 : `localStorage` restait vide après un clic.
 *
 * ## Le piège du premier rendu — et pourquoi ce n'est plus ici
 *
 * Poser l'attribut dans un `useEffect` laisserait un éclair de thème sombre à
 * chaque démarrage : en export statique le HTML est produit au build, il ne peut
 * donc pas contenir la valeur stockée sur la machine.
 *
 * L'attribut est donc appliqué par un **script bloquant** dans `layout.tsx`, qui
 * s'exécute avant React. Il était auparavant posé ici, au chargement du module —
 * donc avant l'hydratation elle aussi, mais React ne l'attendait pas et
 * signalait « some attributes of the server rendered HTML didn't match »
 * (mesuré le 2026-10-05). Le même décalage existait sur `lang` dans `i18n.ts`.
 */

import { useSyncExternalStore } from 'react';

const CLE = 'tatami.apparence';

export type Apparence = 'sombre' | 'clair';

const abonnes = new Set<() => void>();

function lire(): Apparence {
  if (typeof window === 'undefined') return 'sombre';
  return window.localStorage.getItem(CLE) === 'clair' ? 'clair' : 'sombre';
}

/** Applique l'attribut qui pilote la feuille de style. */
function appliquer(a: Apparence): void {
  if (typeof document === 'undefined') return;
  document.documentElement.dataset.apparence = a;
}

export function setApparence(a: Apparence): void {
  if (typeof window !== 'undefined') window.localStorage.setItem(CLE, a);
  appliquer(a);
  abonnes.forEach((cb) => cb());
}

/**
 * Thème courant et setter. Le rerendu vient de l'abonnement, donc un changement
 * est visible immédiatement partout — comme pour la langue.
 */
export function useApparence(): [Apparence, (a: Apparence) => void] {
  const a = useSyncExternalStore(
    (cb) => {
      abonnes.add(cb);
      return () => abonnes.delete(cb);
    },
    lire,
    () => 'sombre' as Apparence,
  );
  return [a, setApparence];
}

// L'attribut est posé par le script de `layout.tsx`, avant l'hydratation de
// React — voir la note en tête. Le module garde la responsabilité de l'état, il
// ne touche plus au `<html>` au chargement.
