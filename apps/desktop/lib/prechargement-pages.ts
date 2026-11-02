/**
 * Préchargement des pages du lecteur.
 *
 * ## Ce qui est mesuré (2026-11-02)
 *
 * Le lecteur ne demande **que** ce qu'il admet : `FENETRE_TELECHARGEMENT` (6)
 * pages à l'ouverture, puis la fenêtre montée. Vérifié dans le code : aucun
 * `new Image()`, aucun `.decode()`, aucun préchargement d'octets côté Rust.
 *
 * Conséquence directe : toute page encore hors de la fenêtre paie sa latence
 * **au moment du défilement**, alors que le moteur sait déjà servir la même
 * page depuis son cache disque en quelques millisecondes quand elle a été
 * demandée une fois. Sur un chapitre local, l'avance d'une page montée coûte
 * ~126 ms ; sur une source réseau, chaque image coûte la latence du site
 * (606 ms par image sur les sources personnalisées après ADR 0019, 2 810 ms
 * avant).
 *
 * ## La règle
 *
 * On demande d'avance les octets d'une fenêtre **devant** la page courante,
 * jamais ceux du chapitre entier : la rafale reste bornée, et couvre la
 * prochaine rangée d'un défilement rapide (le lecteur affiche ~1 page pour
 * 700 px, donc 6 pages couvrent plus d'un écran de marge).
 *
 * La lecture normale ne voit aucune différence de trafic : les pages admises
 * sont demandées de toute façon ; celles-ci le sont une seconde plus tôt.
 */

/** Nombre de pages demandées d'avance. */
export const PAGES_AVANCE = 6;

/**
 * Pages demandées **au-delà de la fenêtre montée**.
 *
 * Mesuré le 2026-11-02 sur `LELMANGA` : 5 415 / 5 403 / 5 361 ms par image,
 * avec trois requêtes en vol. WebView2 (comme Chromium) plafonne à **6
 * connexions par hôte**, et toutes les pages passent par `tatami.localhost` :
 * précharger large au démarrage prendrait les créneaux des pages visibles et
 * **retarderait** le premier écran. On ne demande donc que deux pages *après*
 * la dernière page montée — elles n'entrent en jeu que lorsque les visibles
 * ont déjà leur requête.
 */
export const PAGES_AVANCE_HORS_FENETRE = 2;

/** Délai avant de précharger le chapitre survolé.
 *
 *  Mesuré le 2026-11-02 : `pages` à froid coûte **11 299 ms** sur LELMANGA,
 *  contre 310 ms quand la réponse est déjà dans le cache disque. Un survol est
 *  un signal d'intention : ce délai ignore un simple passage de la souris sur
 *  la liste, sans retarder le clic (le clic, lui, demande le chapitre
 *  immédiatement, et retrouve la requête déjà en vol).
 */
export const DELAI_SURVOL_MS = 250;

/**
 * Faut-il précharger ce chapitre au survol ?
 *
 * Non quand on est déjà en train de lire (le lecteur occupe le moteur), ni
 * quand la réponse est déjà en mémoire — un survol répété ne doit rien
 * redemander.
 */
export function doitPrechargerAuSurvol(
  dejaEnMemoire: boolean,
  lectureEnCours: boolean,
): boolean {
  return !dejaEnMemoire && !lectureEnCours;
}

/**
 * Les index à demander d'avance, à partir de `index` (base 0).
 *
 * `dejaPrecharges` évite de redemander ce qui a déjà été servi — un
 * `fetch` répété n'est pas gratuit (il traverse le protocole `tatami://`).
 */
export function pagesAPrecharger(
  index: number,
  total: number,
  dejaPrecharges: ReadonlySet<number>,
  avance: number = PAGES_AVANCE,
): number[] {
  if (!Number.isInteger(index) || index < 0 || total <= 0 || avance <= 0) return [];
  const fin = Math.min(total - 1, index + avance);
  const pages: number[] = [];
  for (let i = index + 1; i <= fin; i += 1) {
    if (!dejaPrecharges.has(i)) pages.push(i);
  }
  return pages;
}