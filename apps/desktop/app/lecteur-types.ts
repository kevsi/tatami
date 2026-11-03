/**
 * Types partagés du lecteur Tatami.
 *
 * Extraite de `page.tsx` le 2026-11-04 avec la bande et la barre : une fois
 * l'écran découpé en plusieurs modules, les types ne peuvent plus vivre dans
 * l'un ou l'autre d'entre eux.
 *
 * Rien ici ne décrit du persistant : ce sont les projections du lecteur,
 * jamais sérialisées. `invoke<Type>` reste une affirmation — `pnpm ipc:check`
 * est la porte pour les charges utiles envoyées par le cœur.
 */

// `uploadDate` est déjà émis par le moteur (epoch ms, 0 si la source ne le
// fournit pas) : Rust fait du pass-through, seul le type le ignorait.
// `premium` suit la même règle — mesuré le 2026-10-13, une source scriptée
// l'émet et le cœur le relaie sans le toucher. `invoke<Type>` reste une
// affirmation que rien ne vérifie : c'est ce que `pnpm ipc:check` surveille.
/** Une ligne de sommaire d'un chapitre. */
export type Chapter = {
  url: string;
  title?: string | null;
  number?: number;
  uploadDate?: number | null;
  premium?: boolean;
};

/** Une page de la bande — la charge utile de `reader.pages`. */
export type Page = {
  index: number;
  url: string;
  previewUrl?: string | null;
  token: string;
  /** Taille réelle en pixels, lue par Rust dans l'en-tête. 0 = inconnue.
   *  Indispensable au mode webtoon : sans elle, la hauteur de la bande ne peut
   *  qu'être estimée, et une estimation fausse perd la position de lecture. */
  largeur?: number;
  hauteur?: number;
};
