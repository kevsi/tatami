// Types générés depuis `contract/translation/schema.json` v1.

export type TypeBloc = 'dialogue' | 'narration' | 'sfx' | 'free_text' | 'ignore';
export type StatutBloc = 'ok' | 'review' | 'fallback';

export interface Bbox {
  x: number;
  y: number;
  w: number;
  h: number;
}

export interface BlocTraduction {
  id: string;
  type: TypeBloc;
  bbox: Bbox;
  originalText?: string;
  translatedText?: string;
  readingOrder: number;
  status: StatutBloc;
  confidence?: number;
  fontSize?: number;
  fontFamily?: string;
  lineBreaks?: number[];
}

/** Longueur minimale du texte reconnu pour qu'un bloc soit superposé. */
export const LONGUEUR_MINIMALE = 4;

/**
 * Blocs dont la traduction est **superposée** à la page.
 *
 * ADR 0011 (acceptée le 2026-10-08) : le texte hors bulles (`sfx`,
 * `free_text`) n'est pas superposé, et `ignore` ne l'est pas non plus. Seuls
 * `dialogue` et `narration` le sont. Cette fonction est l'application, dans la
 * webview, de cette décision — sans elle l'overlay affichait tous les blocs
 * renvoyés, y compris un texte hors bulle.
 *
 * `LONGUEUR_MINIMALE` répond à une mesure, pas à une intuition : sur 5 pages
 * de « Yani Neko · 1mg » (2026-10-08), la vision a rendu 45 blocs **tous à
 * `confidence: 1`**, dont des `dialogue` d'**un seul caractère** (`"7"`,
 * `"一"`, `"d"`, `"s"`). Le champ `confidence` ne discrimine donc rien et ne
 * peut pas servir de garde ; seule la longueur du texte reconnu sépare un vrai
 * bloc d'un grain de décor. Rien n'est perdu à les ignorer : l'effacement
 * (`inpaint`) n'est pas branché — `tests/fixtures/README.md` le dit — donc
 * aucun dessin n'est modifié.
 */
export function blocsSuperposables(blocs: BlocTraduction[]): BlocTraduction[] {
  return blocs.filter((b) => {
    // La décision 0011 : seuls dialogue et narration sont superposés.
    if (b.type !== 'dialogue' && b.type !== 'narration') return false;
    // Un grain de décor n'est pas une bulle.
    const reconnu = (b.originalText ?? '').replace(/\s+/g, ' ').trim();
    if (reconnu.length < LONGUEUR_MINIMALE) return false;
    // Rien à afficher tant que la traduction n'est pas là et n'a pas réussi.
    if (b.status !== 'ok') return false;
    return Boolean(b.translatedText && b.translatedText.trim());
  });
}

export interface PageTranslation {
  pageId: string;
  mangaUrl?: string;
  chapterUrl: string;
  source: string;
  width: number;
  height: number;
  blocks: BlocTraduction[];
}

export interface ResumeTraduction {
  nbBlocs: number;
  nbBlocsTexte: number;
  nbCaracteresPrompt: number;
  envoiImages: boolean;
  fournisseur: string;
  modeHorsLignePossible: boolean;
}
