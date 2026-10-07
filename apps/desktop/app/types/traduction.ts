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
