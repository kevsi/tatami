/**
 * Réconciliation de la liste blanche des sources.
 *
 * ## Le défaut, mesuré le 2026-10-10
 *
 * L'interface active une source par son **nom**, à travers une **liste fermée**
 * stockée dans `localStorage['tatami.enabledSources']` : un nom absent de la
 * liste = source non affichée. Cette liste est écrite en un seul coup par
 * « Tout activer », qui y met *tous* les noms connus à cet instant.
 *
 * Toute source apparue **ensuite** porte donc un nom absent, et disparaît sans
 * un mot. Deux cas réels, un seul mécanisme :
 *
 * - un script `.js` importé (`invoke('list_sources')` rend 1373, l'en-tête
 *   affiche 1372, et l'écart est stable sur 8 relits espacés de 10 s — ce
 *   n'est donc pas React en retard) ;
 * - une source neuve apportée par une mise à jour du catalogue, exactement de
 *   la même façon.
 *
 * ## Pourquoi la réconciliation naïve serait un bug
 *
 * « Ajouter les noms manquants » **reactiverait** tout ce que l'utilisateur a
 * désactivé : dans ce modèle, un nom absent de la liste *est* la définition
 * d'une source désactivée. Les deux intentions sont indiscernables tant qu'on
 * n'a pas la clé `seen` — l'ensemble des noms déjà présentés à l'utilisateur.
 *
 * Avec elle, la question devient répondable : un nom **dans `seen`** a été
 * vu, donc son absence est un choix ; un nom **hors de `seen`** est arrivé
 * depuis, donc il est activé.
 *
 * ## Ce que cette fonction ne décide pas
 *
 * Rien sur le fond : une source exclue par une liste blanche côté moteur, ou
 * désactivée côté Rust (`sourcesOff`), reste exclue. Ici on ne s'occupe que du
 * filtre `enabled` de l'affichage.
 */

export type Reconciliation = {
  /** Liste blanche à écrire. `null` = « tout est activé », on n'y touche pas. */
  enabled: string[] | null;
  /** Univers connu : tous les noms déjà présentés, y compris disparus. */
  seen: string[];
  /** Noms activés par cette réconciliation — pour journaliser, pas pour l'UI. */
  nouveaux: string[];
  /** Vrai si c'est la première exécution (pas encore de clé `seen`). */
  migration: boolean;
  /** Vrai s'il faut écrire quelque chose. Une écriture à chaque chargement
   *  ferait vibrer `localStorage` pour rien. */
  modifie: boolean;
};

export function reconcilierListeBlanche(params: {
  /** Noms renvoyés par `list_sources`, dans l'ordre reçu. */
  nomsConnus: readonly string[];
  /** Liste blanche courante. `null` = aucune liste = tout activé. */
  enabled: string[] | null;
  /** Univers connu. `null` = pas encore écrit, donc première exécution. */
  seen: string[] | null;
}): Reconciliation {
  const { nomsConnus, enabled, seen } = params;

  // Sans liste blanche, le filtre n'est pas appliqué : il n'y a rien à
  // réconcilier. On note quand même l'univers, pour qu'un futur « tout
  // activer » parte d'une base juste.
  if (enabled === null) {
    const vu = seen ?? [];
    const connu = new Set(vu);
    let change = false;
    for (const n of nomsConnus) {
      if (!connu.has(n)) {
        connu.add(n);
        change = true;
      }
    }
    const suivant = [...connu];
    return {
      enabled: null,
      seen: suivant,
      nouveaux: [],
      migration: false,
      modifie: change,
    };
  }

  const dejaVu = new Set(seen ?? []);

  // Première exécution : on ne sait pas si un nom absent est un choix ou une
  // absence. On ne rajoute donc RIEN à la liste active — c'est la seule
  // hypothèse qui ne réactive pas une source désactivée.
  if (seen === null) {
    return {
      enabled,
      seen: [...new Set([...enabled, ...nomsConnus])],
      nouveaux: [],
      migration: true,
      modifie: true,
    };
  }
  const nouveaux: string[] = [];
  for (const n of nomsConnus) {
    if (!dejaVu.has(n)) {
      dejaVu.add(n);
      // Jamais vue : elle arrive d'ailleurs (script importé, mise à jour du
      // catalogue). Elle est donc active, sauf si l'utilisateur l'a déjà
      // désactivée — impossible ici, il ne l'a jamais vue.
      if (!enabled.includes(n)) nouveaux.push(n);
    }
  }

  const suivants = nouveaux.length
    ? [...enabled, ...nouveaux]
    : enabled;

  return {
    enabled: suivants,
    seen: [...dejaVu],
    nouveaux,
    migration: false,
    modifie: nouveaux.length > 0 || suivants.length !== enabled.length,
  };
}
