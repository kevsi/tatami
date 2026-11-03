/**
 * Constantes partagées du lecteur Tatami.
 *
 * Elles vivent dans un module séparé parce que deux écrans les consomment :
 * `page.tsx` (mémos du rail, zoom de la barre) et `bande-lecteur.tsx`
 * (rail + fenêtres virtualisées).
 */

/** Géométrie du rail d'aperçus (demandé le 2026-10-23).
 *
 *  Toute la géométrie vit ici, en style inline : le CSS du rail ne porte
 *  **aucune** taille. Une hauteur de tuile écrite deux fois (CSS et calcul de
 *  fenêtre) finirait par diverger — même famille que la leçon « une constante
 *  partagée entre deux langages a besoin d'un test qui lit l'autre fichier » :
 *  ici, il n'y a qu'un seul écrivain.
 *
 *  `RAIL_PAS` = hauteur de tuile + écart : c'est le pas qui positionne chaque
 *  tuile et qui dimensionne `.rail-inner` (hauteur totale = pages × pas), donc
 *  la barre de défilement du rail représente le chapitre entier.
 */
export const RAIL_TUILE_H = 80;
export const RAIL_ECART = 8;
export const RAIL_PAS = RAIL_TUILE_H + RAIL_ECART;

/** Bornes du zoom de lecture, en pourcentage.
 *
 *  Constantes nommées parce que le `200` était écrit en dur à **trois**
 *  endroits : la borne de `changeZoom`, le `max` du curseur, et le seuil du
 *  double-clic. Trois copies d'une même borne finissent toujours par diverger,
 *  et celle qui diverge en premier est celle que personne ne regarde.
 *
 *  Elles ne servent plus de seuil à un curseur : la loupe a été retirée le
 *  2026-10-10. Elles restent parce que la borne, elle, est utilisée trois fois.
 */
export const ZOOM_MIN = 50;
export const ZOOM_MAX = 200;
