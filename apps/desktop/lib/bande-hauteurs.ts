/**
 * Hauteurs de la bande webtoon.
 *
 * Extraite de `page.tsx` le 2026-10-09 pour être **testable** : la logique
 * était dans le composant, donc aucune porte ne pouvait la vérifier — c'est
 * la fragilité que `pages:check` (textuel) ne comble pas.
 *
 * ## Le défaut mesuré
 *
 * `bandeVirtualisable` exige que **toutes** les hauteurs soient `> 0` :
 *
 * ```ts
 * hauteurs.length > 0 && !hauteurs.some((h) => h <= 0)
 * ```
 *
 * Or une page que la source ne sert pas n'a pas de dimensions — elle vaut `0`.
 * **Une seule page en échec suffisait donc à désactiver toute la
 * virtualisation**, et le lecteur rendait alors les 50 pages d'un coup.
 *
 * Mesuré dans l'application réelle le 2026-10-09 (`custom:mangadex-api`,
 * Dragon Ball FR), sur des chapitres qui servent pourtant des images :
 *
 * | chapitre | pages | images affichées | hauteur mesurée | bande |
 * |---|---|---|---|---|
 * | ch. 6 | 50 | **12, toutes chargées** | 967 px | **non virtualisée** |
 *
 * Les dimensions étaient donc connues (967 px, pas 0), et la virtualisation
 * était quand même inactive : les 28 pages en échec, elles, n'avaient aucune
 * hauteur. Le seuil « toutes positives » est donc trop strict pour une source
 * qui perd des pages — ce qui est le cas normal, pas l'exception.
 */

/** Hauteur de repli d'une page dont les dimensions sont inconnues.
 *
 * Un manga/webtoon fait typiquement de 1200 à 2000 px de haut. On prend une
 * valeur **médiane** plutôt que la première : une bande dont l'espacement est
 * très légèrement faux se voit au défilement, alors qu'un décalage nul — la
 * valeur 1 — ferait chevaucher les images et masquer le défaut.
 *
 * La valeur est en pixels, à l'échelle 100 % du zoom.
 */
export const HAUTEUR_REPLI = 1600;

/** Format de la bande, tel que le calcule le lecteur (webtoon = 1). */
export const FORMAT_BANDE = 1;

export type PageMesures = { largeur?: number | null; hauteur?: number | null };

/**
 * Hauteurs rendues d'une page, en pixels CSS.
 *
 * `largeurBande` est la largeur de la fenêtre : les dimensions de la page
 * sont en pixels d'image, la hauteur affichée doit être mise à l'échelle.
 * À `largeurBande = 0` (avant le `ResizeObserver`), on rend `1` : une valeur
 * dégénérée mais **positive**, pour que la virtualisation ne s'effondre pas
 * sur une mesure prise trop tôt.
 */
export function hauteurPage(
  page: PageMesures,
  largeurBande: number,
): number {
  const l = largeurBande || 1;
  const w = page.largeur ?? 0;
  const h = page.hauteur ?? 0;
  if (w <= 0 || h <= 0) return Math.round((l * FORMAT_BANDE * HAUTEUR_REPLI) / 900);
  return Math.round((l * FORMAT_BANDE * h) / w);
}

/** Hauteurs de toutes les pages de la bande, dans l'ordre.
 *
 * `mesurees` (index → dimensions réelles) **prime** sur les dimensions de la
 * page : Rust ne connaît que les pages déjà en cache, alors que le navigateur
 * connaît le ratio de tout ce qu'il a décodé — donc la mesure est à la fois plus
 * complète et plus fraîche.
 */
export function hauteursBande(
  pages: readonly PageMesures[],
  largeurBande: number,
  mesurees: Readonly<Record<number, { w: number; h: number }>> = {},
): number[] {
  return pages.map((p, i) => {
    const m = mesurees[i];
    return hauteurPage(m && m.w > 1 && m.h > 1 ? { largeur: m.w, hauteur: m.h } : p, largeurBande);
  });
}

/** La bande est-elle virtualisable ?
 *
 *  **Une page dont la hauteur rendue est inconnue rend la bande NON
 *  virtualisable.** C'est ce qui manquait, et le défaut mesuré le 2026-10-09 :
 *
 *  `hauteurPage` donne une hauteur de **repli** à une page sans dimensions, afin
 *  que le défilement reste cohérent. Mais cette valeur est une *estimation* :
 *  elle ne correspond pas au ratio réel de l'image. Or `bandeVirtualisable`
 *  s'appuyait sur le seul fait que les hauteurs soient positives — donc
 *  **toujours vrai**, même quand aucune page n'était mesurée.
 *
 *  Mesuré dans l'application réelle (chapitre de 15 pages ouvert pour la
 *  première fois, donc aucune dimension en cache — Rust ne connaît que les pages
 *  déjà mises en cache) :
 *
 *  | | mesuré |
 *  |---|---|
 *  | bande réservée | 13 461 px |
 *  | contenu réellement rendu | 11 661 px |
 *  | saut demandé page 10 | page **9** affichée (écart -1) |
 *
 *  La bande était positionnée sur des hauteurs **inventées** pendant que les
 *  images se rendaient à leur ratio réel. D'où la règle : ne virtualiser que ce
 *  dont on connaît la géométrie.
 *
 *  `null` de `hauteurRendue` est exactement « je ne sais pas » : la page rend à
 *  son ratio naturel, donc une hauteur ici serait une invention de plus.
 *
 *  La contrepartie est assumée : sur un chapitre jamais ouvert, la bande n'est
 *  pas virtualisée et rend toutes les pages en flux normal — donc elle est plus
 *  lente à la première ouverture, mais **juste**. C'est le sens du
 *  commentaire d'origine : « jamais une estimation », qui n'était plus appliqué.
 */
export function bandeVirtualisableEst(hauteursRendues: readonly (number | null)[]): boolean {
  if (hauteursRendues.length === 0) return false;
  // Il suffit qu'**une** page soit connue pour virtualiser : les autres pages
  // apprendront leur ratio au `onLoad`, et la bande se recalera. Un chapitre
  // entièrement inconnu, lui, resterait faux du début à la fin.
  return hauteursRendues.some((h) => h !== null);
}

/** Position de défilement qui garde la même page visible après un changement de hauteurs.
 *
 *  Le défaut mesuré le 2026-10-09 : les pages se mesurent au fil de l'eau
 *  (`onLoad` → `noterDims`), donc les hauteurs — et donc les offsets —
 *  **changent après coup**. Mesuré, la bande passe de 15 908 à 14 570 px pendant
 *  la lecture. Or `scrollTop` restait ce qu'il était : tout ce qui était
 *  au-dessus de la fenêtre remontait, et la page que l'utilisateur lisait
 *  sortait du cadre.
 *
 *  Le remède est un **décalage**, pas une position : on translate `scrollTop` de
 *  exactement ce que le recalcul a déplacé la page qui est sous le haut du
 *  cadre. On ne cherche donc pas la page visible — on demande seulement « de
 *  combien a bougé la page qui était à cette position », ce qui rend le calcul
 *  indépendant de toute mesure de géométrie.
 *
 *  Pourquoi pas `reader.index` : essayé, et **cassé**. Après un saut de page,
 *  l'index ancré était celui d'avant le saut, donc la correction ramenait la
 *  bande en haut — mesuré, un saut vers la page 10 affichait la page 2.
 *  L'index change APRÈS le défilement ; l'ancrage doit venir de la position,
 *  pas de l'état.
 *
 *  `null` = rien à corriger (moins de 2 px de déplacement : un `scrollTo` par
 *  rendu ferait bouger la page en permanence, ou nombre de pages différent).
 */
export function decalageAncrage(
  offsetsAvant: readonly number[],
  offsetsApres: readonly number[],
  scrollTop: number,
): number | null {
  // Un nombre de pages différent (changement de chapitre, ou bande qui vient de
  // se virtualiser) n'a pas de correspondance fiable : on ne touche à rien.
  if (offsetsAvant.length !== offsetsApres.length) return null;
  // Page sous le haut du cadre, d'après les ANCIENS offsets — ceux qui étaient
  // en vigueur quand `scrollTop` a été posé.
  let i = 0;
  for (let k = 0; k < offsetsAvant.length; k += 1) {
    if ((offsetsAvant[k] ?? 0) <= scrollTop) i = k;
    else break;
  }
  const dep = (offsetsApres[i] ?? 0) - (offsetsAvant[i] ?? 0);
  if (Math.abs(dep) < 2) return null;
  return Math.max(0, scrollTop + dep);
}

/** Positions cumulées : l'offset de la page `i` est `offsets[i]`. */
export function offsetsBande(hauteurs: readonly number[]): number[] {
  const out: number[] = [];
  let y = 0;
  for (const h of hauteurs) {
    out.push(y);
    y += h;
  }
  return out;
}

/**
 * Hauteur à poser **sur l'image**, ou `null` pour n'en poser aucune.
 *
 * ## Le défaut mesuré le 2026-10-09
 *
 * `hauteurPage` sert à **positionner** les pages. Poser le même nombre en
 * `height` inline sur l'image, elle, **déforme** l'image : un style inline
 * gagne toujours, donc ni `height: auto` ni `box-sizing: border-box` ne
 * peuvent Plus rien.
 *
 * Rust ne connaît les dimensions que des pages **déjà en cache**
 * (`commands.rs:304`), donc un chapitre ouvert pour la première fois n'en a
 * aucune et toutes les pages retombaient sur `HAUTEUR_REPLI` — un ratio de
 * 1600/900 inventé. Mesuré sur TOOMICSFR, 6 images sur 6 étirées :
 *
 * |        | dim. réelles | dim. affichées | `height` inline | écart |
 * |--------|--------------|----------------|-----------------|-------|
 * | page 0 | 720x500      | 673x1196       | 1196px          | -60,9%|
 *
 * Le repli reste donc juste pour le **défilement** (un décalage de quelques
 * pixels se voit à peine) mais faux pour le **rendu** (60 % d'écart se voit
 * immédiatement). D'où deux fonctions au lieu d'une :
 *
 * - `hauteurPage` — pour les **offsets**, toujours un nombre positif ;
 * - `hauteurRendue` — pour le style, `null` quand la mesure manque, afin que
 *   le navigateur garde le ratio naturel de l'image.
 *
 * L'image occupe alors plus ou moins de place que prevu, la bande se
 * recale : c'est le compromis assume, et il est **discret**, la ou
 * l'etirement etait immediatement visible.
 */
export function hauteurRendue(
  page: PageMesures,
  largeurBande: number,
): number | null {
  const w = page.largeur ?? 0;
  const h = page.hauteur ?? 0;
  // 1x1 est le rapport d'une image qui n'a pas encore décodé, pas une mesure.
  if (w <= 1 || h <= 1) return null;
  const l = largeurBande || 1;
  return Math.round((l * FORMAT_BANDE * h) / w);
}

/** Hauteur rendue d'une page, mesure du navigateur prioritaire.
 *
 *  Même rôle que {@link hauteurRendue}, avec la remontée `onLoad` en plus. Dans
 *  le lecteur, c'est cette fonction-ci qu'il faut appeler : une page jamais mise
 *  en cache n'a de dimensions que par ce chemin. */
export function hauteurRendueMesuree(
  page: PageMesures,
  largeurBande: number,
  mesure: { w: number; h: number } | undefined,
): number | null {
  if (mesure && mesure.w > 1 && mesure.h > 1) {
    return hauteurRendue({ largeur: mesure.w, hauteur: mesure.h }, largeurBande);
  }
  return hauteurRendue(page, largeurBande);
}

/**
 * Largeur **en pixels entiers** d'une page de la bande.
 *
 * ## Le défaut mesuré (2026-10-09)
 *
 * La largeur était posée en **pourcentage** : `width: ${zoom}%`. Or un
 * pourcentage d'une largeur impaire donne un nombre fractionnaire — mesuré en
 * réduisant la fenêtre :
 *
 * | largeur du conteneur | `width: 50%` rendu |
 * |---|---|
 * | 1420 px | 710 px — entier, rien ne se voit |
 * | 1081 px | **540,5 px** |
 * | 977 px | **488,5 px** |
 * | 843 px | **421,5 px** |
 *
 * Une image dessinée sur un demi-pixel est interpolée sur une demi-colonne :
 * le navigateur y laisse une bande **translucide** au bord — c'est la ligne
 * verticale que l'utilisateur a signalée. Et elle n'apparaît qu'en réduisant la
 * fenêtre, parce qu'à la largeur par défaut le pourcentage tombe juste.
 *
 * Un pixel entier se dessine net : on arrondit donc **une fois**, ici, et la
 * même valeur sert à calculer la hauteur — sinon la boîte serait arrondie dans
 * un sens et le ratio dans l'autre.
 */
export function largeurImagePx(largeurBande: number, zoom: number): number {
  return Math.max(1, Math.round((largeurBande || 0) * (zoom / 100)));
}

/**
 * Pas d'un appui sur flèche haut/bas dans la bande.
 *
 * ## Pourquoi une fonction, et pas un nombre dans le gestionnaire de clés
 *
 * Mesuré le 2026-10-09 : les flèches haut et bas étaient traitées comme
 * « page suivante / page précédente », donc chaque appui valait **une page
 * entière** (18 629 px sur le chapitre mesuré). Dans une bande qui se défile,
 * c'est un saut, pas un défilement. Pire : le premier appui ne déplaçait
 * **rien** (`scrollTop` resté à 0 alors que le compteur passait de 1 à 2),
 * parce que l'ancrage de page ne peut pas se satisfaire quand la page courante
 * est plus haute que le cadre.
 *
 * On garde 80 % du cadre plutôt qu'une page entière : assez pour avancer avec
 * le défilement, assez peu pour ne pas sauter du texte. `PAS_DEFILEMENT_MIN`
 * est un plancher pour un cadre minuscule : sans lui, un pas de 8 px rendrait
 * la touche décorative.
 */
export const RATIO_PAS_DEFILEMENT = 0.8;
export const PAS_DEFILEMENT_MIN = 80;

export function pasDeDefilement(clientHeight: number): number {
  return Math.max(PAS_DEFILEMENT_MIN, Math.round((clientHeight || 0) * RATIO_PAS_DEFILEMENT));
}

/**
 * Indices de pages montés dans la bande.
 *
 * Une fenêtre glissante, plus une garantie : **la dernière page est toujours
 * montée**.
 *
 * ## Le défaut mesuré (2026-10-09)
 *
 * En fin de bande, les pages de queue n'étaient montées qu'**à l'arrivée** — et
 * la bande leur avait déjà réservé une hauteur de repli (`HAUTEUR_REPLI`,
 * 1 115 px à cette largeur). Tant que l'image n'était pas chargée, sa hauteur
 * réelle n'était pas connue, donc la bande réservait 1 115 px pour une image
 * qui en fait 260 : **1 062 px de vide en bas du lecteur**, soit un « footer »
 * inutile que l'utilisateur a signalé.
 *
 * Mesuré sur le chapitre de 53 pages : bande de 57 314 px au fond, contenu
 * rendu finissant 1 062 px plus haut ; 6 s plus tard, la bande était retombée à
 * 55 440 px et le vide avait disparu — la preuve que c'est bien un **artefact de
 * chargement de la queue**, pas un calcul faux.
 *
 * Monter la dernière page dès l'ouverture la fait décoder et `onLoad` la
 * mesure : la bande est donc déjà juste quand on y arrive. Coût : **une image**
 * de plus, pas la queue entière — la virtualisation reste la sienne.
 *
 * ## Portée et marge : pourquoi elles ne valent pas 2 / 2 000
 *
 * Mesuré le 2026-10-09, sur un défilement par paliers de 500 px : **29 images
 * montées et 28 retirées en 40 pas**, et surtout **2 corrections d'ancrage qui
 * reculaient `scrollTop` de 507 px** (13 000 → 12 493). Un recul de 507 px au
 * milieu d'une lecture est un à-coup visible — le « flash » signalé.
 *
 * Pourquoi 507 : une page encore non mesurée reçoit `HAUTEUR_REPLI` (1 120 px
 * à cette largeur) ; dès que son `onLoad` la mesure (613 px), tout ce qui est
 * au-dessus remonte de 507 px, et la correction d'ancrage suit pour garder la
 * page lue en place. **Le calcul est juste, le moment est mauvais** : la mesure
 * arrivait quand la page était déjà sous le haut du cadre.
 *
 * D'où une portée plus large : on monte ~6 pages devant au lieu de 3, donc une
 * page est mesurée bien avant d'arriver en haut, et la correction qui reste ne
 * porte plus que sur une page hors du cadre.
 */
export const MARGE_FENETRE = 3;
export const PORTEE_FENETRE = 4200;

export function fenetreBande(
  nbPages: number,
  offsets: readonly number[],
  hauteurs: readonly number[],
  scrollY: number,
  marge = MARGE_FENETRE,
  portee = PORTEE_FENETRE,
): number[] {
  const tous = Array.from({ length: nbPages }, (_, i) => i);
  if (nbPages === 0) return tous;
  // Page contenant le haut du cadre : la première dont le bas dépasse `scrollY`.
  // `premier` peut valoir `nbPages` si l'ancre est au-delà de la bande : on ne
  // le borne pas ici, parce que la garantie du bas (`la dernière page est
  // toujours montée`) rend ce cas correct de toute façon. Un clamp ici serait
  // du code redondant — et, sans test qui le morde, du code que rien ne protège.
  let premier = 0;
  while (
    premier < nbPages &&
    (offsets[premier] ?? 0) + (hauteurs[premier] ?? 0) < scrollY
  ) {
    premier += 1;
  }

  const debut = Math.max(0, premier - marge);
  let y = offsets[premier] ?? 0;
  let fin = premier;
  while (fin < nbPages && y < scrollY + portee) {
    y += hauteurs[fin] ?? 0;
    fin += 1;
  }
  const limite = Math.min(nbPages, fin + marge);
  const out: number[] = [];
  for (let i = debut; i < limite; i += 1) out.push(i);

  // La dernière page, toujours montée : sans elle, sa hauteur de repli reste
  // réservée jusqu'à ce qu'on arrive dessus.
  const derniere = nbPages - 1;
  if (derniere >= 0 && out[out.length - 1] !== derniere) out.push(derniere);
  return out;
}
