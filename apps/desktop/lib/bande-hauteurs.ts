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
