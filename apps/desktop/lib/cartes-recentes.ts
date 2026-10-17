/**
 * Quelles cartes du catalogue reçoivent leur **nombre de chapitres**.
 *
 * Ce compte est celui de la pastille posée sur la couverture. Il vivait à
 * côté d'une ligne « derniers chapitres » sous le titre, retirée le
 * 2026-10-17 : elle répétait ce que la couverture dit déjà.
 *
 * ## Pourquoi, mesuré le 2026-10-17
 *
 * La marche de fond couvrait **tout** le catalogue chargé — 223 cartes pour
 * `custom:rimuscan` — à `RECENT_CONCURRENCY = 4` requêtes concurrentes, alors
 * que le moteur n'a que `MAX_WORKERS = 4` travailleurs : les quatre restaient
 * occupés en permanence, et une requête demandée par l'utilisateur (ouvrir une
 * série) attendait derrière. Mesuré sur une session réelle :
 *
 *  - **423 appels `chapters` en 13 min**, 223 séries distinctes, sans fin ;
 *  - **25 couvertures lancées à la même seconde** passent de 0,5 s (la 1ʳᵉ) à
 *    15,3 s puis **34,3 s** (la 25ᵉ), alors que le travail réel par image vaut
 *    ~1,3 s : le reste est de l'attente.
 *
 * Le remède est la borne : on ne demande le compte que des cartes **dans la
 * fenêtre de lecture**. Une carte hors écran ne coûte rien tant que personne ne
 * la regarde — et si elle entre dans la vue, le composant relance la marche
 * (voir `recentTick` dans `page.tsx`).
 *
 * ## Ces fonctions ne touchent ni au DOM ni au réseau
 *
 * Le composant leur passe les **clés visibles** (lues dans le DOM, là où
 * `data-manga-key` expose la clé de la carte) et le cache local. C'est ce qui
 * rend la borne vérifiable par un test : une carte hors fenêtre n'est jamais
 * retournée, et une fenêtre vide ne demande rien.
 */

/** Entrée du cache local : le compte calculé et la date du calcul. */
export type EntreeRecent = { total: number; at: number };

/** Cache encore valide : à **reporter dans l'affichage**, sans requête.
 *
 *  Sans ce report, une carte servie par le cache resterait vide — plus aucune
 *  requête n'est à faire pour la remplir (défaut mesuré le 2026-10-06).
 *
 *  Volontairement **non borné à la fenêtre** : reporter une entrée déjà
 *  calculée ne coûte aucune requête, et une carte revenue à l'écran par un
 *  défilement rapide doit afficher son compte immédiatement.
 */
export function entreesFraiches<T extends { key: string }>(
  cartes: readonly T[],
  cache: Record<string, EntreeRecent>,
  maintenant: number,
  ttlMs: number,
): Record<string, EntreeRecent> {
  const fraiches: Record<string, EntreeRecent> = {};
  for (const { key } of cartes) {
    const entree = cache[key];
    if (entree && maintenant - (entree.at ?? 0) <= ttlMs) fraiches[key] = entree;
  }
  return fraiches;
}

/** Cartes à interroger : **dans la fenêtre**, sans cache frais, pas déjà en vol.
 *
 *  C'est ici que se joue la borne : `fenetre` vient de `clesVisibles()`, donc
 *  une carte hors écran n'est jamais demandée. Les deux autres filtres sont
 *  ceux du cache (ne pas refaire un travail frais) et du vol (ne pas doubler
 *  une requête en cours, ce qui arrive quand la marche est relancée).
 */
export function cartesAFaire<T extends { key: string }>(
  cartes: readonly T[],
  fenetre: ReadonlySet<string>,
  cache: Record<string, EntreeRecent>,
  enVol: ReadonlySet<string>,
  maintenant: number,
  ttlMs: number,
): T[] {
  return cartes.filter(({ key }) => {
    if (!fenetre.has(key)) return false;
    const entree = cache[key];
    if (entree && maintenant - (entree.at ?? 0) <= ttlMs) return false;
    return !enVol.has(key);
  });
}
