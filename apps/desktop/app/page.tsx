'use client';

import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { open } from '@tauri-apps/plugin-dialog';

import { useApparence } from './apparence';
import { useI18n, type MessageKey } from './i18n';

type Source = { name: string; lang: string };
type Manga = {
  url: string;
  title: string;
  source: string;
  /** URL distante de la couverture. **Ne jamais l'utiliser dans un `<img src>`** :
   *  la webview n'a ni user-agent, ni cookies, ni déblocage Cloudflare, et
   *  l'échec est silencieux (vignette vide, console propre). */
  coverUrl?: string | null;
  /** Jeton `tatami://cover/...` — la seule source à afficher. */
  coverToken?: string | null;
  authors?: string[];
  altTitle?: string | null;
  description?: string | null;
  state?: string | null;
  rating?: number;
  contentRating?: string | null;
  tags?: Tag[];
};
// `uploadDate` est déjà émis par le moteur (epoch ms, 0 si la source ne le
// fournit pas) : Rust fait du pass-through, seul le type le ignorait.
type Chapter = { url: string; title?: string | null; number?: number; uploadDate?: number | null };
type Page = {
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
/** Page d'un `.cbz` déjà téléchargé : servie par Rust, sans moteur ni réseau. */
type LocalPage = { index: number; token: string; name: string; width: number; height: number };
/** Référence stable : `reader?.pages ?? []` créerait un nouveau tableau à chaque
 *  rendu, donc les `useMemo` de la bande recalculeraient sans cesse. */
const EMPTY_PAGES: Page[] = [];
/** Lecteur paginé (tâche 0.10) : les images viennent du protocole `tatami://`,
 *  jamais d'un appel direct aux hôtes distants. */
/** Un favori enregistré par l'utilisateur (table `favorites`). */
type Bookmark = {
  source: string;
  manga_url: string;
  chapter_url: string;
  title: string;
  cover_url: string | null;
  /** Jeton `tatami://cover/...` — seul champ à afficher (cf. `Manga.coverUrl`). */
  cover_token: string | null;
  chapter_title: string | null;
  page_index: number;
  page_count: number;
  created_at: number;
};

type Category = {
  id: number;
  name: string;
  color: number;
};

type Favorite = {
  source: string;
  manga_url: string;
  title: string;
  cover_url: string | null;
  cover_token: string | null;
  content_rating: string | null;
  added_at: number;
  category_id: number | null;
  category_color: number | null;
};

/** Une entrée de l'historique de lecture (table `history`). */
type HistoryItem = {
  source: string;
  manga_url: string;
  title: string;
  cover_url: string | null;
  cover_token: string | null;
  content_rating: string | null;
  chapter_url: string | null;
  chapter_title: string | null;
  chapter_index: number;
  page_index: number;
  page_count: number;
  read_at: number;
};

type Reader = {
  chapter: Chapter;
  pages: Page[];
  index: number;
  source: string;
  /** Le chapitre vient du disque : aucune requête ne part vers le site. */
  horsLigne?: boolean;
  mangaTitre?: string;
  /** URL du webtoon : le lecteur doit se suffire à lui-même pour enregistrer
   *  la progression, sans dépendre d'une fiche ouverte. */
  mangaUrl?: string;
  coverUrl?: string | null;
};
type Tag = { key: string; title: string };
type LibraryItem = {
  title: string;
  source: string;
  url: string;
  coverUrl?: string | null;
  /** Jeton `tatami://cover/...` — seul champ à afficher (cf. `Manga.coverUrl`). */
  coverToken?: string | null;
  dir: string;
  downloaded: number;
  totalChapters: number;
  complete: boolean;
  sizeBytes: number;
  updatedAt: number;
  chapters: { url: string; title: string; file: string }[];
};
/** Un webtoon dans la file de téléchargement multi-sélection. */
type QueueItem = {
  key: string;
  title: string;
  source: string;
  total: number;
  done: number;
  skipped: number;
  failed: number;
  state: 'attente' | 'téléchargement' | 'terminé' | 'erreur' | 'pause';
  current?: string;
};
type Progress = {
  source: string;
  chapter: string;
  page: number;
  total: number;
  done: boolean;
  error: string | null;
};

const PAGE = 24;
// Une source peut renvoyer plusieurs centaines de chapitres : on n'en rend
// qu'une tranche a la fois (le reste est derriere « Afficher plus »).
const CHAPTER_PAGE = 100;
const CHAPTER_STEP = 200;
const PAGE_KEY = 'tatami.pageSize';
const SOURCES_KEY = 'tatami.enabledSources';
const SAFE_KEY = 'tatami.contentRating';
const TIMEOUT_KEY = 'tatami.listTimeout';
const TAGS_SHOWN = 24;
// Valeurs acceptées par les parsers (enum ContentRating). Une seule à la fois :
// en envoyer plusieurs lève « Multiple Content ratings are not supported ».
/** Palette des catégories. La base ne stocke qu'un INDEX, jamais une couleur :
 *  ces variables sont les seules à fournir une teinte, cf. AGENTS.md §9.2. */
const CATEGORY_COLORS = [
  'var(--cat-1)',
  'var(--cat-2)',
  'var(--cat-3)',
  'var(--cat-4)',
  'var(--cat-5)',
  'var(--cat-6)',
  'var(--cat-7)',
  'var(--cat-8)',
] as const;

function couleurCategorie(index: number | null | undefined): string {
  if (index == null) return 'var(--text-3)';
  return CATEGORY_COLORS[(index - 1) % CATEGORY_COLORS.length];
}

type Tri = 'source' | 'az' | 'za';

const TRIS: { value: Tri; cle: MessageKey }[] = [
  { value: 'source', cle: 'tri.source' },
  { value: 'az', cle: 'tri.az' },
  { value: 'za', cle: 'tri.za' },
];

// Les libellés sont des CLÉS i18n, pas du texte : la liste est constante mais
// sa traduction dépend de la langue courante.
const RATINGS = [
  { value: 'SUGGESTIVE', cle: 'reglages.rating.suggestive' },
  { value: 'SAFE', cle: 'reglages.rating.safe' },
  { value: 'ALL', cle: 'reglages.rating.all' },
] as const satisfies readonly { value: string; cle: MessageKey }[];

const TIMEOUTS = [30, 60, 120] as const;

const INCOGNITO_KEY = 'tatami.incognito';
const RECENT_LIMIT = 24;

function loadPrefs(): {
  pageSize: number;
  enabled: string[] | null;
  rating: string;
  listTimeout: number;
} {
  if (typeof window === 'undefined') {
    return { pageSize: PAGE, enabled: null, rating: 'SUGGESTIVE', listTimeout: 60 };
  }
  const raw = window.localStorage.getItem(PAGE_KEY);
  const rawSources = window.localStorage.getItem(SOURCES_KEY);
  const rawRating = window.localStorage.getItem(SAFE_KEY);
  const rawTimeout = window.localStorage.getItem(TIMEOUT_KEY);
  return {
    pageSize: raw ? Number(raw) || PAGE : PAGE,
    enabled: rawSources ? (JSON.parse(rawSources) as string[]) : null,
    rating: rawRating || 'SUGGESTIVE',
    listTimeout: rawTimeout ? Number(rawTimeout) || 60 : 60,
  };
}

const TABS = [
  { id: 'home', cle: 'nav.accueil' },
  { id: 'catalog', cle: 'nav.catalogues' },
  { id: 'downloads', cle: 'nav.telechargements' },
  { id: 'settings', cle: 'nav.parametres' },
] as const;

// ---------- Notifications (toasts) ----------

/** Durée d'affichage d'une notification, demande explicite de l'utilisateur. */
const TOAST_DUREE_MS = 3000;

/**
 * Compteur d'identifiants pour les toasts.
 *
 * `Date.now()` ne suffit pas : deux notifications dans la même milliseconde
 * partageraient le même identifiant, et la première ferait disparaître la
 * seconde avec elle. Même famille que le dossier temporaire des tests Rust.
 */
let compteurToast = 0;

type Toast = { id: number; texte: string };

/**
 * Pile de notifications, rendue hors du flux : elle survit au changement de vue
 * et n'en décale aucune.
 *
 * La croix est un vrai `<button>` avec un nom accessible — sans lui, la seule
 * façon de fermer une notification au clavier serait d'attendre les 3 secondes.
 */
function Toaster({
  toasts,
  onClose,
}: {
  toasts: Toast[];
  onClose: (id: number) => void;
}) {
  const { t } = useI18n();
  if (toasts.length === 0) return null;
  return (
    <div className="toaster" role="status" aria-live="polite">
      {toasts.map((x) => (
        <div className="toast" key={x.id}>
          <span className="toast-texte">{x.texte}</span>
          <button
            className="toast-fermer"
            aria-label={t('notification.fermer')}
            title={t('notification.fermer')}
            onClick={() => onClose(x.id)}
          >
            {'×'}
          </button>
        </div>
      ))}
    </div>
  );
}

// Barre d'application persistante : brand + onglets, visible dans toutes les vues.
function AppBar({
  tab,
  onTab,
  children,
}: {
  tab: string;
  onTab: (id: string) => void;
  children?: React.ReactNode;
}) {
  const { t } = useI18n();
  return (
    <div className="appbar">
      <div className="brand">
        <span className="logo" />
        {t('app.nom')}
      </div>
      <nav className="tabs" role="tablist" aria-label={t('nav.catalogues')}>
        {TABS.map((tabule) => (
          <button
            key={tabule.id}
            className="tab"
            role="tab"
            aria-selected={tab === tabule.id}
            onClick={() => onTab(tabule.id)}
          >
            {t(tabule.cle)}
          </button>
        ))}
      </nav>
      {children}
    </div>
  );
}

// MangaDex et quelques Madara renvoient un UUID comme titre de chapitre :
// on retombe alors sur le numéro, sinon l'interface n'affiche qu'un identifiant.
const UUID_LIKE = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

function chapterLabel(c: Chapter): string {
  if (c.title && !UUID_LIKE.test(c.title)) return c.title;
  if (c.number != null) return `Chapitre ${c.number}`;
  return 'Sans titre';
}

/** Numéro brut quand la source en donne un : « Ch. 392 » plutôt que le titre.
 *  Les flottants se rounds : la capture montrait « Ch. 38.09999847… », une
 *  division flottante de la source ne doit pas fuiter dans l'interface. */
function chapterShort(c: Chapter): string {
  if (c.number == null) return chapterLabel(c);
  const n = c.number;
  if (!Number.isFinite(n)) return chapterLabel(c);
  if (Number.isInteger(n)) return `Ch. ${n}`;
  const arrondi = Math.round(n * 100) / 100;
  return `Ch. ${Number.isInteger(arrondi) ? arrondi.toFixed(0) : arrondi}`;
}

const DATE_FR = new Intl.DateTimeFormat('fr-FR', { day: 'numeric', month: 'short', year: 'numeric' });

/** Date de publication d'un chapitre. Le moteur émet l'epoch en ms, 0 quand la
 *  source ne la fournit pas : dans ce cas on n'affiche rien plutôt qu'un 1970. */
function chapterDate(c: Chapter): string | null {
  const ms = c.uploadDate;
  if (ms == null || !Number.isFinite(ms) || ms <= 0) return null;
  const d = new Date(ms);
  if (Number.isNaN(d.getTime())) return null;
  return DATE_FR.format(d);
}

/** Les résumés de sources arrivent en HTML (mesuré sur REMANGA : `<p>…</p>`) ;
 *  on retire les balises et on retombe sur des espaces, sinon le balisage
 *  s'affiche tel quel dans le texte. */
function plainText(raw?: string | null): string {
  if (!raw) return '';
  return raw
    .replace(/<br\s*\/?>/gi, ' ')
    .replace(/<\/(p|div|li|h[1-6])>/gi, ' ')
    .replace(/<[^>]*>/g, '')
    .replace(/&nbsp;/gi, ' ')
    .replace(/&amp;/gi, '&')
    .replace(/&lt;/gi, '<')
    .replace(/&gt;/gi, '>')
    .replace(/&quot;/gi, '"')
    .replace(/&#39;|&apos;/gi, "'")
    .replace(/\s+/g, ' ')
    .trim();
}

/** Libellé lisible de l'état d'une série, tel que le moteur l'énumère. */
const STATE_FR: Record<string, string> = {
  ONGOING: 'En cours',
  FINISHED: 'Terminé',
  ABANDONED: 'Abandonné',
  LICENSED: 'Sous licence',
  PUBLISHING_FINISHED: 'Publication terminée',
  ON_HIATUS: 'En pause',
  NOT_YET: 'Pas encore publié',
  DISCONTINUED: 'Interrompu',
  UNKNOWN: '',
};

/** Les lignes de la table d'informations : on n'affiche que ce que la source a
 *  réellement donné, une ligne vide est pire qu'une ligne absente. */
function infoRows(m: Manga | null): { label: string; value: string }[] {
  if (!m) return [];
  const rows: { label: string; value: string }[] = [];
  const state = m.state ? (STATE_FR[m.state] ?? '') : '';
  if (state) rows.push({ label: 'Statut', value: state });
  if (m.authors && m.authors.length > 0) {
    rows.push({ label: 'Auteur', value: m.authors.join(', ') });
  }
  const genres = m.tags ?? [];
  if (genres.length > 0) {
    rows.push({ label: 'Genres', value: genres.map((t) => t.title).join(', ') });
  }
  const alt = plainText(m.altTitle);
  if (alt.length > 0) {
    rows.push({ label: 'Titre alternatif', value: alt });
  }
  if (m.rating != null && m.rating > 0) {
    rows.push({ label: 'Note', value: `${m.rating.toFixed(1)} / 5` });
  }
  rows.push({ label: 'Source', value: m.source });
  return rows;
}

/** Ancienneté courte et traduite : « 5 mois », « 3 j », « il y a 2 h ».
 *
 *  Cette fonction était française en dur — contraire à `AGENTS.md` §7, qui
 *  interdit toute chaîne non traduite dans un composant. Mesuré le 2026-10-05
 *  en ajoutant l'écran 5.4 : alors que l'interface était en anglais, la date
 *  d'une panne s'affichait en français. Les unités passent donc par `t`, et il
 *  n'y a plus qu'une seule fonction de calcul.
 *
 *  Le type du premier paramètre reprend celui de `useI18n` (`Cle`), pas
 *  `string` : c'est lui qui fait qu'une clé d'i18n **inexistante ne compile
 *  pas**. Annoté en `string`, TypeScript aurait accepté n'importe quelle
 *  faute de frappe.
 */
function depuis(t: ReturnType<typeof useI18n>['t'], ms: number): string {
  const diff = Date.now() - ms;
  if (diff < 0) return t('temps.aujourdhui');
  const min = Math.round(diff / 60_000);
  if (min < 60) return min <= 1 ? t('temps.instance') : t('temps.min', { n: min });
  const h = Math.round(min / 60);
  if (h < 24) return t('temps.heure', { n: h });
  const d = Math.round(h / 24);
  if (d < 31) return t('temps.jour', { n: d });
  const mo = Math.round(d / 30.44);
  // Le singulier a sa propre cle : mesuré le 2026-10-05 sur l'écran 5.4,
  // l'anglais affichait « 1 months ago ». Une seule clé `{n} mois` ne peut pas
  // faire les deux, l'anglais et le français n'ayant pas la même règle.
  if (mo < 12) return mo <= 1 ? t('temps.mois1') : t('temps.mois', { n: mo });
  const y = Math.floor(d / 365.25);
  return y <= 1 ? t('temps.an') : t('temps.ans', { n: y });
}

/* Tâche 0.16 : derniers chapitres dans la carte du catalogue.
   Coût assumé : UNE requête `chapters` par carte affichée. Le cache local
   évite de répéter ces requêtes à chaque visite (30 min), et la concurrence
   est bornée pour ne pas marteler la source. */
const RECENT_KEY = 'tatami.recent';
const RECENT_TTL = 30 * 60 * 1000;
const RECENT_CONCURRENCY = 4;

type RecentEntry = { labels: string[]; total: number; at: number };

function loadRecentCache(): Record<string, RecentEntry> {
  try {
    const raw = window.localStorage.getItem(RECENT_KEY);
    return raw ? (JSON.parse(raw) as Record<string, RecentEntry>) : {};
  } catch {
    return {};
  }
}

function saveRecentCache(cache: Record<string, RecentEntry>) {
  try {
    window.localStorage.setItem(RECENT_KEY, JSON.stringify(cache));
  } catch {
    /* quota atteint : le cache est un confort, pas une donnée */
  }
}

/** File de téléchargement persistée : ce qui reste à faire si l'app s'arrête. */
type SavedQueue = { items: Manga[]; startedAt: number };
const QUEUE_KEY = 'tatami.queue';
const READER_MODE_KEY = 'tatami.readerMode';
const READER_ZOOM_KEY = 'tatami.readerZoom';
const READER_FIT_KEY = 'tatami.readerFit';
const READER_SPREAD_KEY = 'tatami.readerSpread';
const READER_DIR_KEY = 'tatami.readerDir';

/** Tâche 1.8 : combien de pages sont préchargées **en avance** dans le cache Rust.
 *
 *  L'interface posait une seule balise `<img class="preload">` sur la page
 *  suivante, donc un cran de navigation sur deux repartait sur le réseau. Le
 *  cache disque s'alimente par le protocole `tatami://` : la balise provoque le
 *  téléchargement, et la page s'affiche ensuite en centièmes de seconde.
 *
 *  Plafond volontaire : au-delà, on télécharge des pages que l'utilisateur peut
 *  ne jamais atteindre (il ferme le chapitre), sur une machine déjà chargée par
 *  le moteur. Quatre pages couvrent largement un défilement rapide, y compris en
 *  double page où un cran en consomme deux.
 */
const PRECHARGE_PAGES = 4;

/** Tâche 1.8 : distance de molette minimale pour tourner une page.
 *
 *  En dessous, on laisse le défilement normal de la page se faire : c'est ce qui
 *  évite qu'un micro-scroll dans la barre d'outils fasse changer de page.
 */
const DELTA_MOLETTE_MIN = 40;
/** Durée pendant laquelle un même sens de molette est ignoré.
 *
 *  C'est ce qui transforme un geste en UNE page au lieu de dix.
 */
const DELTA_MOLETTE_FIN_MS = 300;

type ReaderMode = 'page' | 'webtoon';
/** Tâche 0.13 : comment la page est mise à l'échelle dans le cadre. */
type ReaderFit = 'width' | 'height';
/** Ordre de lecture d'une double page. Le manga se lit de droite à gauche. */
type ReaderDir = 'rtl' | 'ltr';

function loadSavedQueue(): SavedQueue | null {
  if (typeof window === 'undefined') return null;
  try {
    const raw = window.localStorage.getItem(QUEUE_KEY);
    if (!raw) return null;
    const parsed = JSON.parse(raw) as SavedQueue;
    return parsed.items?.length ? parsed : null;
  } catch {
    return null;
  }
}

function saveQueue(items: Manga[]) {
  window.localStorage.setItem(QUEUE_KEY, JSON.stringify({ items, startedAt: Date.now() }));
}

const HEALTH_KEY = 'tatami.sourceHealth';
/** Source en échec réseau, mémorisé pour ne pas la retester à chaque visite. */
type SourceHealth = Record<string, { code: string; message: string; at: number }>;

function loadHealth(): SourceHealth {
  if (typeof window === 'undefined') return {};
  try {
    return JSON.parse(window.localStorage.getItem(HEALTH_KEY) || '{}') as SourceHealth;
  } catch {
    return {};
  }
}

function noteHealth(sources: string[], code: string, message: string) {
  const all = loadHealth();
  for (const s of sources) all[s] = { code, message, at: Date.now() };
  window.localStorage.setItem(HEALTH_KEY, JSON.stringify(all));
}

function clearHealthFor(sources: string[]) {
  const all = loadHealth();
  for (const s of sources) delete all[s];
  window.localStorage.setItem(HEALTH_KEY, JSON.stringify(all));
}

function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes < 1024) return `${Math.round(bytes || 0)} o`;
  const mb = bytes / (1024 * 1024);
  if (mb < 1) return `${Math.round(bytes / 1024)} Ko`;
  return `${mb.toFixed(1).replace('.', ',')} Mo`;
}

/** Extrait l'URL de vérification d'une erreur `CHALLENGE_REQUIRED`. */
function challengeUrl(raw: string): string | null {
  const m = raw.match(/challenge at (\S+)/);
  return m ? m[1] : null;
}

/** Signature du traducteur, pour les fonctions de module qui doivent parler.
 *
 *  La clé est typée comme dans `i18n.ts` : c'est ce qui fait échouer la
 *  compilation si on passe une clé inexistante. Une `string` large accepterait
 *  n'importe quelle faute de frappe en silence. */
type Traduire = (key: MessageKey, vars?: Record<string, string | number>) => string;

/**
 * Traduit une erreur brute du moteur en un message court, sans l'URL technique.
 *
 * Les codes viennent de `contract/protocol.json` ; le reste est heuristique sur
 * les messages d'exception observés (sources mortes : 404, hôte inconnu, 403).
 *
 * `t` est passé en paramètre parce que cette fonction vit hors du composant :
 * elle n'a pas accès au contexte i18n. C'est aussi ce qui lui permet de
 * supprimer 20 chaines en dur au profit de l'i18n.
 */
function describeError(raw: string, sourceName: string, t: Traduire): string {
  const s = raw.replace(/\s+/g, ' ').trim();
  if (/TIMEOUT|timeout/i.test(s)) {
    return t('erreur.delai', { source: sourceName });
  }
  if (/CHALLENGE_REQUIRED/i.test(s)) {
    return t('erreur.verification', { source: sourceName });
  }
  // 401 : le site exige un compte. Ni un challenge ni une panne réseau — demander
  // une vérification anti-robot ici serait futile, c'est un compte qu'il faut.
  if (/AUTH_REQUIRED/i.test(s)) {
    return t('erreur.compte.requis', { source: sourceName });
  }
  if (/RATE_LIMITED/i.test(s)) {
    return t('erreur.surcharge', { source: sourceName });
  }
  if (/UnknownHostException/i.test(s)) {
    return t('erreur.domaine', { source: sourceName });
  }
  if (/404|NotFoundException/i.test(s)) {
    return t('erreur.404', { source: sourceName });
  }
  if (/403/.test(s)) {
    return t('erreur.403', { source: sourceName });
  }
  if (/500|502|503/.test(s)) {
    return t('erreur.serveur', { source: sourceName });
  }
  const short = s.length > 140 ? `${s.slice(0, 140)}…` : s;
  return `${sourceName} : ${short}`;
}

export default function Home() {
  // `lang` et `setLang` sont déjà pris par le FILTRE de langue des
  // catalogues : ceux de l'i18n sont donc aliassés.
  const { t, lang: langue, setLang: setLangue } = useI18n();
  const [apparence, setApparence] = useApparence();
  const [sources, setSources] = useState<Source[]>([]);
  const [lang, setLang] = useState('all');
  const [nameFilter, setNameFilter] = useState('');
  const [shownSources, setShownSources] = useState(48);
  const [selected, setSelected] = useState<Source | null>(null);
  const [latest, setLatest] = useState<Manga[]>([]);
  const [latestOffset, setLatestOffset] = useState(0);
  const [hasMore, setHasMore] = useState(false);
  const [openManga, setOpenManga] = useState<Manga | null>(null);
  // Tâche 0.17 : fiche détaillée (résumé, état, genres). Les chapters arrivent
  // à part : les deux appels sont indépendants et l'un ne doit pas masquer l'autre.
  const [info, setInfo] = useState<Manga | null>(null);
  const [chapters, setChapters] = useState<Chapter[]>([]);
  const [chapterLimit, setChapterLimit] = useState(CHAPTER_PAGE);
  const [chapterQuery, setChapterQuery] = useState('');
  // Nombre de chapitres à cocher d'un clic dans la barre de sélection.
  const [pickCount, setPickCount] = useState(10);
  // Incrémenté pour demander un défilement vers la sélection (voir
  // `pickChapters`) : un `scrollIntoView` direct serait exécuté avant le rendu.
  const [reveal, setReveal] = useState(0);
  const [checked, setChecked] = useState<Set<string>>(new Set());
  const [downloading, setDownloading] = useState(false);
  const [progress, setProgress] = useState<Progress | null>(null);
  const [tab, setTab] = useState<string>('home');
  // Bibliothèque personnelle : favoris, historique, reprise (table SQLite).
  const [favorites, setFavorites] = useState<Favorite[]>([]);
  const [recent, setRecent] = useState<HistoryItem[]>([]);
  // Mode incognito : les titres classés adultes disparaissent de l'accueil.
  // Préférence d'affichage, donc `localStorage` est le bon endroit (cf. §6).
  const [incognito, setIncognito] = useState(true);
  const [library, setLibrary] = useState<LibraryItem[]>([]);
  // Notifications : une pile, pas un message unique. Un toast remplace le
  // précédent, donc deux confirmations rapprochées (« dossier changé » puis
  // « 13 webtoons ») s'effaçaient l'une l'autre — la seconde n'était jamais vue.
  const [toasts, setToasts] = useState<Toast[]>([]);
  // `setNotice` garde son nom et sa signature : les ~8 appelants existants
  // n'ont aucune raison de savoir qu'un toast est désormais affiché.
  const setNotice = useCallback((texte: string) => {
    const id = ++compteurToast;
    setToasts((precedent) => [...precedent, { id, texte }]);
    // Le minuteur est posé ici et non dans le composant `Toaster` : un toast
    // déjà retiré ne doit pas pouvoir se retirer lui-même plus tard.
    window.setTimeout(() => {
      setToasts((precedent) => precedent.filter((x) => x.id !== id));
    }, TOAST_DUREE_MS);
  }, []);
  const [error, setError] = useState<string | null>(null);
  const fermerToast = useCallback((id: number) => {
    setToasts((precedent) => precedent.filter((x) => x.id !== id));
  }, []);
  const [pageSize, setPageSize] = useState(PAGE);
  const [enabled, setEnabled] = useState<string[] | null>(null);
  // Tâche 1.2 : sources écartées par l'utilisateur (donnée métier, pas une
  // préférence d'affichage : elle vit en base, cf. AGENTS.md §6).
  const [sourcesOff, setSourcesOff] = useState<string[]>([]);
  const [voirDesactives, setVoirDesactives] = useState(false);
  // Tâche 1.7 : signets. Un signet = une page précise d'un chapitre.
  const [signets, setSignets] = useState<Bookmark[]>([]);
  // Tâche 1.5 : catégories de favoris. Une seule par favori (choix humain,
  // parité Kotatsu-Redo) ; `null` = sans catégorie.
  const [categories, setCategories] = useState<Category[]>([]);
  const [filtreCategorie, setFiltreCategorie] = useState<number | 'all' | 'aucune'>('all');
  const [nouvelleCategorie, setNouvelleCategorie] = useState('');
  const [tags, setTags] = useState<Tag[]>([]);
  const [tagsTotal, setTagsTotal] = useState(0);
  const [activeTags, setActiveTags] = useState<string[]>([]);
  const [mangaQuery, setMangaQuery] = useState('');
  // Tâche 1.3 : tri. Décidé avec l'utilisateur : alphabétique seul, appliqué
  // à tout ce qui est chargé (le moteur ne propose pas de tri, et un tri par
  // « dernière mise à jour » coûterait une requête par source).
  const [tri, setTri] = useState<Tri>('source');
  const [rating, setRating] = useState('SUGGESTIVE');
  const [onePageOnly, setOnePageOnly] = useState(false);
  const [listTimeout, setListTimeout] = useState(60);
  const [deleteTarget, setDeleteTarget] = useState<LibraryItem | null>(null);
  // Sélecteur de chapitre d'un webtoon téléchargé : avant, « Lire » ouvrait
  // toujours le premier chapitre, ce qui est inutilisable sur une série de 137.
  const [chapitresVisibles, setChapitresVisibles] = useState<LibraryItem | null>(null);
  const [reader, setReader] = useState<Reader | null>(null);
  const [readerLoading, setReaderLoading] = useState(false);
  const [challenge, setChallenge] = useState<string | null>(null);
  const [challengeBusy, setChallengeBusy] = useState(false);
  // Vérification demandée par un TÉLÉCHARGEMENT (et non par une lecture).
  // État distinct de `challenge` : celui-ci est une simple URL rattachée à la
  // source ouverte, alors qu'ici l'évènement Rust porte source + URL + motif,
  // et que l'utilisateur peut être sur la vue Téléchargements sans source
  // sélectionnée. Mesuré : HENTAIORIGINES répond 200 OK avec sa page de
  // vérification d'âge, donc le moteur ne signale aucun challenge.
  const [verif, setVerif] = useState<{ source: string; url: string; reason: string } | null>(
    null,
  );
  // Confort de lecture : mode paginé ou webtoon (bande verticale), et largeur.
  const [readerMode, setReaderMode] = useState<ReaderMode>('page');
  const [zoom, setZoom] = useState(100);
  // Tâche 0.13 : ajustement largeur/hauteur, double page, sens de lecture.
  const [fit, setFit] = useState<ReaderFit>('width');
  const [spread, setSpread] = useState(false);
  const [readerDir, setReaderDir] = useState<ReaderDir>('rtl');
  // Tâche 0.14 : aller directement à un chapitre sans repasser par la fiche.
  const [pageJump, setPageJump] = useState('');
  /** Dossier de téléchargement choisi par l'utilisateur (choix humain du 2026-10-05).
   *
   *  Il est lu et écrit par Rust, pas par `localStorage` : c'est une donnée que
   *  Rust applique à chaque écriture de fichier, et une webview qui l'écrirait
   *  pourrait diverger de l'état réel. `parDefaut` = l'utilisateur n'a rien
   *  choisi, l'app utilise `%APPDATA%\Tatami\downloads`. */
  const [dossierTelechargements, setDossierTelechargements] = useState({
    chemin: '',
    parDefaut: true,
  });
  const stripRef = useRef<HTMLDivElement>(null);
  const stripImages = useRef<(HTMLImageElement | null)[]>([]);
  /** Page sur laquelle la bande doit se placer à la prochaine ouverture.
   *
   *  Déclarée ici, et non près de l'effet qui la consomme : `openReaderAt` la
   *  remplit, et cette fonction est **définie plus haut** dans le composant. Un
   *  `useRef` déclaré après son usage resterait `undefined` au moment de la
   *  lecture — sans erreur, sans avertissement, et la reprise affichait la
   *  première page (mesuré le 2026-10-05). */
  const aPositionner = useRef<number | null>(null);
  // Virtualisation de la bande webtoon (tâche 1.9) : hauteur réelle de chaque
  // page, position de la bande, et largeur du conteneur (la hauteur affichée
  // est proportionnelle à cette largeur).
  const dernierScroll = useRef(0);
  const enAttente = useRef(false);
  // Tâche 1.8 : sens du dernier geste de molette, 0 = aucun en cours. Vaut 1 pour
  // « on descend », -1 pour « on remonte ». Permet à `onMoletteStage` de
  // n'avancer que d'une page par geste.
  const moletteEnCours = useRef(0);
  const [scrollY, setScrollY] = useState(0);
  const [largeurBande, setLargeurBande] = useState(0);
  const [health, setHealth] = useState<SourceHealth>({});
  // Sélection multiple de webtoons + file de téléchargement
  const [selectMode, setSelectMode] = useState(false);
  const [picked, setPicked] = useState<Set<string>>(new Set());
  const [queue, setQueue] = useState<QueueItem[]>([]);
  // Tâche 0.16 : derniers chapitres par carte du catalogue
  const [cardRecent, setCardRecent] = useState<Record<string, string[]>>({});
  const [cardTotal, setCardTotal] = useState<Record<string, number>>({});
  const [recentBusy, setRecentBusy] = useState(0);
  const [showRecent, setShowRecent] = useState(true);
  const recentInFlight = useRef<Set<string>>(new Set());
  const [queueRunning, setQueueRunning] = useState(false);
  const cancelQueue = useRef(false);

  /*
   * Préférences et santé des sources lues APRÈS l'hydratation : en export
   * statique le HTML est produit au build, il ne peut pas contenir ce qui est
   * stocké sur la machine de l'utilisateur. Les poser dans un initialiseur
   * paresseux créerait un écart serveur/client. `set-state-in-effect` est donc
   * désactivé pour ce seul effet, et uniquement lui.
   */
/* eslint-disable react-hooks/set-state-in-effect */
  useEffect(() => {
    const prefs = loadPrefs();
    setPageSize(prefs.pageSize);
    setEnabled(prefs.enabled);
    setRating(prefs.rating);
    setListTimeout(prefs.listTimeout);
    setHealth(loadHealth());
    // Dossier de téléchargement : lu au démarrage, car Rust l'a déjà appliqué
    // avant même que la fenêtre s'affiche.
    void invoke<{ chemin: string; parDefaut: boolean }>('get_downloads_dir')
      .then(setDossierTelechargements)
      .catch(() => {
        /* l'affichage retombe sur le dossier par défaut */
      });
    const modeIncognito = window.localStorage.getItem(INCOGNITO_KEY);
    if (modeIncognito != null) setIncognito(modeIncognito === '1');
    // Lecture des préférences après l'hydratation : en export statique le HTML
    // est produit au build, il ne peut donc pas contenir la valeur stockée sur
    // la machine de l'utilisateur. Les poser dans un initialiseur paresseux
    // créerait un écart serveur/client ; l'effet est ici le bon moment.
    if (typeof window !== 'undefined') {
      setReaderMode((window.localStorage.getItem(READER_MODE_KEY) as ReaderMode) || 'page');
      setZoom(Number(window.localStorage.getItem(READER_ZOOM_KEY)) || 100);
      setFit((window.localStorage.getItem(READER_FIT_KEY) as ReaderFit) || 'width');
      setSpread(window.localStorage.getItem(READER_SPREAD_KEY) === '1');
      setReaderDir((window.localStorage.getItem(READER_DIR_KEY) as ReaderDir) || 'rtl');
    }
    invoke<{ sources: Source[] }>('list_sources')
      .then((r) => setSources(r.sources ?? []))
      .catch((e) => setError(String(e)));
    refreshDownloads();
    const unlisten = listen<Progress>('download://progress', (event) => {
      setProgress(event.payload);
    });
    // Une source peut répondre 200 OK avec sa page de vérification d'âge : le
    // moteur ne signale alors aucun challenge (il ne voit que 403/503). C'est
    // Rust qui constate l'absence d'image ou l'URL vide et demande l'ouverture
    // de la fenêtre ; l'utilisateur y confirme lui-même.
    const unlistenVerif = listen<{ source: string; url: string; reason: string }>(
      'download://challenge',
      (event) => {
        const { source, url, reason } = event.payload;
        setVerif({ source, url, reason });
        invoke('open_challenge', { source, url })
          .then(() => setNotice(t('telechargements.verification.ouverte', { source })))
          .catch((e) =>
            setError(t('telechargements.verification.erreur', { detail: String(e) })),
          );
      },
    );
    return () => {
      unlisten.then((off) => off()).catch(() => {});
      unlistenVerif.then((off) => off()).catch(() => {});
    };
    // Montage uniquement : rejouer cet effet à chaque changement de
    // `refreshDownloads` réabonnerait l'écouteur d'événements.
    }, []);
  /* eslint-enable react-hooks/set-state-in-effect */

  const refreshDownloads = useCallback(() => {
    invoke<LibraryItem[]>('list_library', { root: '' })
      .then((items) => setLibrary(items ?? []))
      .catch(() => setLibrary([]));
  }, []);

  /** Recharge favoris et historique. `hideAdult` explicite parce que l'appel
   *  déclenché par l'interrupteur arrive AVANT que l'état change : sans
   *  paramètre, la closure renverrait l'ancienne valeur et l'historique
   *  resterait filtré (mesuré : la liste restait vide après désactivation).
   */
  const refreshShelf = useCallback(
    async (hideAdult?: boolean) => {
      const hide = hideAdult ?? incognito;
      // Pas d'indicateur « chargement » : la lecture est un SQLite local,
      // elle dure moins d'une image. Un état pour ça ne servirait qu'à
      // scintiller et à forcer un rendu en cascade.
      try {
        const [fav, hist, cats] = await Promise.all([
          invoke<Favorite[]>('list_favorites'),
          invoke<HistoryItem[]>('list_recent', { limit: RECENT_LIMIT, hideAdult: hide }),
          invoke<Category[]>('list_categories'),
        ]);
        setFavorites(fav ?? []);
        setRecent(hist ?? []);
        setCategories(cats ?? []);
      } catch (e) {
        // Ne rien afficher ici : l'accueil affiche déjà l'erreur si l'action
        // l'a déclenchée. On trace pour pouvoir diagnostiquer.
        console.error('lecture de la bibliothèque impossible', e);
        setFavorites([]);
        setRecent([]);
      }
    },
    [incognito],
  );

  const refreshSignets = useCallback(async (mangaUrl?: string) => {
    try {
      const r = await invoke<Bookmark[]>('list_bookmarks', {
        limit: 200,
        mangaUrl: mangaUrl ?? null,
      });
      setSignets(r ?? []);
    } catch (e) {
      console.error('lecture des signets impossible', e);
      setSignets([]);
    }
  }, []);

  const goTab = useCallback(
    (id: string) => {
      if (id === 'downloads') refreshDownloads();
      if (id === 'home') {
        void refreshShelf();
        void refreshSignets();
      }
      // Changer d'onglet quitte la vue en cours (source, fiche) : sinon
      // l'ancien écran, rendu avant l'accueil, resterait affiché par-dessus.
      setOpenManga(null);
      if (id !== 'catalog') setSelected(null);
      setTab(id);
    },
    [refreshDownloads, refreshShelf, refreshSignets],
  );

  function isFavorite(m: Manga): boolean {
    return favorites.some((f) => f.source === m.source && f.manga_url === m.url);
  }

  /** Le favori correspondant à un manga, pour lire sa catégorie. */
  function favoriDe(m: Manga): Favorite | undefined {
    return favorites.find((f) => f.source === m.source && f.manga_url === m.url);
  }

  /** Favoris après application du filtre de catégorie. */
  const favorisVisibles = useMemo(() => {
    if (filtreCategorie === 'all') return favorites;
    if (filtreCategorie === 'aucune') return favorites.filter((f) => f.category_id == null);
    return favorites.filter((f) => f.category_id === filtreCategorie);
  }, [favorites, filtreCategorie]);

  // Charge la bibliothèque au démarrage. Indispensable : `tab` vaut « home »
  // dès le premier rendu SANS passer par `goTab`, donc rien ne déclencherait
  // le rechargement sinon (mesuré : l'accueil affichait « 0 favori » après un
  // redémarrage alors que la base contenait bien la ligne).
  // `refreshShelf` ne touche l'état qu'APRÈS son `await` : aucun setState
  // synchrone ici. La règle n'analyse pas la suspension et le signale à tort.
  useEffect(() => {
    // eslint-disable-next-line react-hooks/set-state-in-effect
    void refreshShelf();
    void refreshSignets();
    invoke<string[]>('list_disabled_sources')
      .then((r) => setSourcesOff(r ?? []))
      .catch(() => setSourcesOff([]));
    }, []);

  function estDesactivee(nom: string): boolean {
    return sourcesOff.includes(nom);
  }

  async function validerNouvelleCategorie() {
    const nom = nouvelleCategorie.trim();
    if (nom.length === 0) return;
    // La couleur tourne : deux catégories créées d'affilée ne se ressemblent pas.
    const couleur = (categories.length % CATEGORY_COLORS.length) + 1;
    const c = await creerCategorie(nom, couleur);
    if (c) setNouvelleCategorie('');
  }

  /** Affecte une catégorie à un favori (ou la retire avec `null`). */
  async function choisirCategorie(f: Manga, categoryId: number | null) {
    const avant = favorites;
    // Mise à jour optimiste : le sélecteur ne doit pas attendre la base.
    setFavorites((prev) =>
      prev.map((x) =>
        x.source === f.source && x.manga_url === f.url
          ? {
              ...x,
              category_id: categoryId,
              category_color:
                categories.find((c) => c.id === categoryId)?.color ?? null,
            }
          : x,
      ),
    );
    try {
      await invoke('set_favorite_category', {
        source: f.source,
        mangaUrl: f.url,
        categoryId,
      });
      await refreshShelf();
    } catch (e) {
      setFavorites(avant);
      setError(t('erreur.categorie', { raison: String(e) }));
    }
  }

  async function creerCategorie(nom: string, couleur: number) {
    try {
      const c = await invoke<Category>('add_category', { name: nom, color: couleur });
      await refreshShelf();
      return c;
    } catch (e) {
      setError(t('erreur.categorie', { raison: String(e) }));
      return null;
    }
  }

  async function supprimerCategorie(id: number) {
    const avant = categories;
    setCategories((prev) => prev.filter((c) => c.id !== id));
    try {
      await invoke('delete_category', { id });
      // Supprimer une catégorie ne supprime pas ses favoris : ils reviennent
      // en « sans catégorie », et `list_favorites` est la source de vérité.
      await refreshShelf();
    } catch (e) {
      setCategories(avant);
      setError(t('erreur.categorie', { raison: String(e) }));
    }
  }

  /** Déplace une catégorie d'un cran. On renvoie TOUS les identifiants dans
   *  le nouvel ordre : la base réécrit les positions plutôt que d'échanger
   *  deux lignes, l'ordre final est donc exactement celui demandé. */
  async function deplacerCategorie(id: number, pas: -1 | 1) {
    const ids = categories.map((c) => c.id);
    const i = ids.indexOf(id);
    const j = i + pas;
    if (i < 0 || j < 0 || j >= ids.length) return;
    [ids[i], ids[j]] = [ids[j], ids[i]];
    const avant = categories;
    setCategories(categories.slice().sort((a, b) => ids.indexOf(a.id) - ids.indexOf(b.id)));
    try {
      await invoke('reorder_categories', { ids });
      await refreshShelf();
    } catch (e) {
      setCategories(avant);
      setError(t('erreur.categories.ordre', { raison: String(e) }));
    }
  }

  async function renommerCategorie(id: number, nom: string, couleur: number) {
    try {
      await invoke('rename_category', { id, name: nom, color: couleur });
      await refreshShelf();
    } catch (e) {
      setError(t('erreur.categorie', { raison: String(e) }));
    }
  }

  /** La page affichée porte-t-elle déjà un signet ? */
  function pageMarquee(): boolean {
    if (!reader) return false;
    return signets.some(
      (b) =>
        b.chapter_url === reader.chapter.url && b.page_index === reader.index,
    );
  }

  async function basculerSignet() {
    if (!reader) return;
    const { chapter, index, pages } = reader;
    const manga = info ?? openManga;
    if (!manga) return;
    const cle = { chapter_url: chapter.url, page_index: index };
    const dejaLa = signets.some(
      (b) => b.chapter_url === cle.chapter_url && b.page_index === cle.page_index,
    );
    // Mise à jour optimiste avec retour arrière, comme pour les favoris et les
    // sources : l'interface ne montre jamais un état qu'elle ne tient pas.
    const avant = signets;
    const apercu: Bookmark = {
      source: manga.source,
      manga_url: manga.url,
      chapter_url: chapter.url,
      title: manga.title,
      cover_url: manga.coverUrl ?? null,
      cover_token: manga.coverToken ?? null,
      chapter_title: chapterLabel(chapter),
      page_index: index,
      page_count: pages.length,
      created_at: Date.now(),
    };
    setSignets(dejaLa ? avant.filter((b) => !sameSignet(b, cle)) : [apercu, ...avant]);
    try {
      await invoke(dejaLa ? 'remove_bookmark' : 'add_bookmark', {
        source: manga.source,
        mangaUrl: manga.url,
        chapterUrl: chapter.url,
        title: manga.title,
        coverUrl: manga.coverUrl ?? null,
        chapterTitle: chapterLabel(chapter),
        pageIndex: index,
        pageCount: pages.length,
      });
      // On relit depuis la base : la source de vérité reste SQLite, pas l'état
      // optimiste qui vient de disparaître.
      await refreshSignets();
    } catch (e) {
      setSignets(avant);
      setError(t('erreur.signet', { raison: String(e) }));
    }
  }

  function sameSignet(b: Bookmark, cle: { chapter_url: string; page_index: number }): boolean {
    return b.chapter_url === cle.chapter_url && b.page_index === cle.page_index;
  }

  /** Rejouer un signet : rouvre son chapitre exactement à sa page. */
  async function ouvrirSignet(b: Bookmark) {
    setError(null);
    const manga: Manga = {
      url: b.manga_url,
      title: b.title,
      source: b.source,
      coverUrl: b.cover_url,
      coverToken: b.cover_token,
    };
    setOpenManga(manga);
    // Le moteur ne connaît un chapitre que si la fiche a été listée dans la
    // session : sans cet appel, `pages` échoue et le lecteur ne s'ouvre pas.
    const [ficheRes, chapitresRes] = await Promise.allSettled([
      invoke<{ manga: Manga }>('details', {
        source: b.source,
        mangaUrl: b.manga_url,
        mangaTitle: b.title,
      }),
      invoke<{ chapters: Chapter[] }>('chapters', {
        source: b.source,
        mangaUrl: b.manga_url,
        mangaTitle: b.title,
      }),
    ]);
    if (ficheRes.status === 'fulfilled' && ficheRes.value.manga) {
      setInfo(ficheRes.value.manga);
    }
    if (chapitresRes.status === 'rejected') {
      setError(t('erreur.chapitres', { raison: String(chapitresRes.reason) }));
      return;
    }
    const list: Chapter[] = chapitresRes.value.chapters ?? [];
    setChapters(list);
    const chapter =
      list.find((c) => c.url === b.chapter_url) ?? list[0];
    if (!chapter) {
      setError('Ce chapitre n’est plus disponible dans cette source.');
      return;
    }
    // Le nom de source doit être passé explicitement : ouvert depuis l'accueil,
    // aucune source n'est sélectionnée et `openReaderAt` sortirait en silence.
    await openReaderAt(
      chapter,
      Math.min(b.page_index, Math.max(0, b.page_count - 1)),
      b.source,
    );
  }

  async function basculerSource(nom: string, actif: boolean) {
    // Mise à jour optimiste comme pour les favoris, avec retour arrière si la
    // base refuse : l'interface ne doit jamais montrer un état qu'elle ne tient
    // pas (mesuré sur les favoris, même principe).
    const avant = sourcesOff;
    setSourcesOff((prev) => (actif ? prev.filter((n) => n !== nom) : [...prev, nom]));
    try {
      await invoke('set_source_enabled', { source: nom, enabled: actif });
    } catch (e) {
      setSourcesOff(avant);
      setError(t('erreur.source', { raison: String(e) }));
    }
  }

  async function reactiverToutes() {
    const avant = sourcesOff;
    setSourcesOff([]);
    try {
      await invoke('enable_all_sources');
    } catch (e) {
      setSourcesOff(avant);
      setError(t('erreur.reactivation', { raison: String(e) }));
    }
  }

  async function toggleFavorite(m: Manga) {
    const target = !isFavorite(m);
    // Mise à jour optimiste : l'interface ne doit pas attendre la base.
    setFavorites((prev) =>
      target
        ? [
            {
              source: m.source,
              manga_url: m.url,
              title: m.title,
              cover_url: m.coverUrl ?? null,
              cover_token: m.coverToken ?? null,
              content_rating: m.contentRating ?? null,
              added_at: Date.now(),
              // Un favori tout juste ajouté n'a pas encore de catégorie ; il
              // reste visible sous « Sans catégorie » jusqu'au choix.
              category_id: null,
              category_color: null,
            },
            ...prev,
          ]
        : prev.filter((f) => !(f.source === m.source && f.manga_url === m.url)),
    );
    try {
      await invoke('set_favorite', {
        source: m.source,
        mangaUrl: m.url,
        title: m.title,
        coverUrl: m.coverUrl ?? null,
        contentRating: m.contentRating ?? null,
        favorite: target,
      });
      if (target) setNotice(`${m.title} ajouté à vos favoris.`);
    } catch (e) {
      // On annule l'optimisme si la base refuse : l'interface ne ment pas.
      await refreshShelf();
      setError(t('erreur.favori', { raison: String(e) }));
    }
  }

  const langs = useMemo(
    () => ['all', ...Array.from(new Set(sources.map((s) => s.lang))).sort()],
    [sources],
  );
  const visibleSources = useMemo(() => {
    const base = enabled === null ? sources : sources.filter((s) => enabled.includes(s.name));
    // Les sources désactivées sont retirées du parcours par défaut : elles
    // restent enregistrées et réapparaissent d'un clic (« Afficher les n
    // sources désactivées »).
    return voirDesactives ? base : base.filter((s) => !sourcesOff.includes(s.name));
  }, [sources, enabled, sourcesOff, voirDesactives]);
  const byLang = lang === 'all' ? visibleSources : visibleSources.filter((s) => s.lang === lang);
  const filtered = byLang.filter((s) => s.name.toLowerCase().includes(nameFilter.toLowerCase()));

  // Tri appliqué à TOUT ce qui est chargé, pas seulement à la page courante :
  // sinon l'ordre changerait à chaque « Voir plus ». `localeCompare` sans
  // locale explicite utilise celle du navigateur, quirange correctement les
  // accents et les chiffres ; `numeric` place « Ch. 2 » avant « Ch. 10 ».
  const latestTrie = useMemo(() => {
    if (tri === 'source') return latest;
    const copie = [...latest];
    copie.sort((a, b) => {
      const r = a.title.localeCompare(b.title, undefined, {
        numeric: true,
        sensitivity: 'base',
      });
      return tri === 'az' ? r : -r;
    });
    return copie;
  }, [latest, tri]);

  /* 1 370 noms à plat ne se parcourent pas : deux états.
     - Sans recherche et sans langue choisie, on montre les LANGUES (19 tuiles
       avec leur volume) : c'est le point d'entrée, comme les genres du site.
     - Dès qu'une langue est choisie ou qu'on cherche un nom, on montre la liste
       des sources de cette langue sous un bandeau. */
  const NOMS_LANGUE: Record<string, string> = {
    en: 'Anglais', ja: 'Japonais', fr: 'Français', pt: 'Portugais', es: 'Espagnol',
    it: 'Italien', de: 'Allemand', ru: 'Russe', ko: 'Coréen', zh: 'Chinois',
    ar: 'Arabe', pl: 'Polonais', tr: 'Turc', id: 'Indonésien', th: 'Thaï',
    vi: 'Vietnamien', nl: 'Néerlandais', uk: 'Ukrainien', hi: 'Hindi', int: 'International',
  };
  const langCounts = useMemo(() => {
    const map = new Map<string, number>();
    visibleSources.forEach((s) => map.set(s.lang, (map.get(s.lang) ?? 0) + 1));
    return [...map.entries()].sort((a, b) => b[1] - a[1]);
  }, [visibleSources]);

  const enTiles = lang === 'all' && nameFilter.trim().length === 0;

  // Tâche 0.15 : filtre de la grille de chapitres. Un terme numérique compare
  // aussi les numéros, « 1180 » trouve donc le chapitre 1180 même quand son
  // titre ne contient que des lettres.
  const filteredChapters = useMemo(() => {
    const q = chapterQuery.trim().toLowerCase();
    if (!q) return chapters;
    const n = Number(q);
    return chapters.filter(
      (c) =>
        chapterLabel(c).toLowerCase().includes(q) ||
        (c.number != null && Number.isFinite(n) && Math.round(n) === Math.round(c.number)),
    );
  }, [chapters, chapterQuery]);

  // Défile vers la première case cochée quand la sélection en masse le demande.
  useEffect(() => {
    if (reveal === 0) return;
    const el = document.querySelector('.chapter-cell.on');
    if (el) el.scrollIntoView({ block: 'center', behavior: 'smooth' });
  }, [reveal, checked]);

  async function loadSource(s: Source, offset = 0, opts?: { tags?: string[]; query?: string }) {
    const useTags = opts?.tags ?? activeTags;
    const useQuery = opts?.query ?? mangaQuery;
    setSelected(s);
    setLoadingState(true);
    if (offset === 0) {
      setLatest([]);
      setLatestOffset(0);
      setOnePageOnly(false);
    }
    try {
      const r = await invoke<{ list: Manga[] }>('latest', {
        source: s.name,
        offset,
        limit: pageSize,
        query: useQuery,
        tags: useTags,
        contentRating: rating === 'ALL' ? '' : rating,
        timeoutSecs: listTimeout,
      });
      const items = r.list ?? [];
      if (items.length > 0 && selected) clearHealthFor([selected.name]);
      setLatest((prev) => (offset === 0 ? items : [...prev, ...items]));
      setLatestOffset(offset + items.length);
      // Une source qui renvoie 0 au-delà de la première page ne pagine pas
      // (mesuré sur EXHENTAI) : on masque le bouton au lieu de le laisser
      // promising des résultats qui n'existent pas.
      setHasMore(items.length > 0);
      if (offset > 0 && items.length === 0) setOnePageOnly(true);
    } catch (e) {
      const raw = String(e);
      setError(raw);
      const url = challengeUrl(raw);
      setChallenge(url && selected ? url : null);
      // Mémorise la source en échec : elle sera signalée dans le catalogue.
      // Une demande de vérification n'est PAS un échec : à résoudre par l'humain.
      if (selected && !url) {
        const code = raw.slice(0, raw.indexOf(':') > 0 ? raw.indexOf(':') : 12);
        noteHealth([selected.name], code, describeError(raw, selected.name, t));
        setHealth(loadHealth());
      }
    } finally {
      setLoadingState(false);
    }
  }

  async function loadTags(s: Source) {
    try {
      const r = await invoke<{ tags: Tag[] }>('filter_options', { source: s.name });
      setTags((r.tags ?? []).slice(0, TAGS_SHOWN));
      setTagsTotal(r.tags?.length ?? 0);
    } catch {
      setTags([]);
      setTagsTotal(0);
    }
  }

  async function openSource(s: Source) {
    setSelected(s);
    setActiveTags([]);
    setTags([]);
    setMangaQuery('');
    setOnePageOnly(false);
    void loadTags(s);
    await loadSource(s, 0, { tags: [], query: '' });
  }

  function toggleTag(key: string) {
    if (!selected) return;
    const next = activeTags.includes(key) ? activeTags.filter((k) => k !== key) : [...activeTags, key];
    setActiveTags(next);
    void loadSource(selected, 0, { tags: next });
  }

  function clearTags() {
    if (!selected) return;
    setActiveTags([]);
    void loadSource(selected, 0, { tags: [] });
  }

  async function searchInSource() {
    if (!selected) return;
    await loadSource(selected, 0, { query: mangaQuery, tags: activeTags });
  }

  const [busy, setBusy] = useState(false);
  const setLoadingState = (v: boolean) => setBusy(v);
  const loading = busy;

  // Charge en arrière-plan les derniers chapitres des cartes affichées.
  // `recentInFlight` empêche les doublons quand l'effet se redéclenche avant
  // la fin des requêtes ; le cache local évite de refaire le travail au retour.
  useEffect(() => {
    if (!showRecent || latest.length === 0) return;
    const cache = loadRecentCache();
    const now = Date.now();
    const sourceName = selected?.name;
    const fresh: Record<string, string[]> = {};
    const freshTotal: Record<string, number> = {};
    const todo = latest
      .map((m) => ({ m, key: mangaKey(m) }))
      .filter(({ key }) => {
        const hit = cache[key];
        // Cache valide : on l'affiche sans requête. Sans ce report dans l'état,
        // une carte servie par le cache resterait vide (plus aucune requête à
        // faire pour la remplir) — c'est le bug mesuré au 3e run.
        if (hit && now - hit.at <= RECENT_TTL) {
          fresh[key] = hit.labels;
          freshTotal[key] = hit.total;
          return false;
        }
        if (recentInFlight.current.has(key)) return false;
        return true;
      });
    if (Object.keys(fresh).length > 0) {
      setCardRecent((prev) => ({ ...prev, ...fresh }));
      setCardTotal((prev) => ({ ...prev, ...freshTotal }));
    }
    if (todo.length === 0) return;

    let cursor = 0;
    const run = async () => {
      while (cursor < todo.length) {
        const { m, key } = todo[cursor++];
        recentInFlight.current.add(key);
        setRecentBusy((n) => n + 1);
        try {
          const r = await invoke<{ chapters: Chapter[] }>('chapters', {
            source: m.source,
            mangaUrl: m.url,
            mangaTitle: m.title,
          });
          const list = r.chapters ?? [];
          // Les DERNIERS chapitres, pas les premiers : sur la fiche du site,
          // ces lignes servent à voir si une série est vivante. Les parsers
          // rendent la liste dans l'ordre croissant, mais pas toujours (ni
          // toujours avec une date), donc on trie explicitement.
          const labels = [...list]
            .sort((a, b) => {
              const da = a.uploadDate ?? 0;
              const db = b.uploadDate ?? 0;
              if (da !== db) return db - da;
              return (b.number ?? 0) - (a.number ?? 0);
            })
            .slice(0, 3)
            .map(
              (c) =>
                `${chapterShort(c)}${
                  c.uploadDate != null && c.uploadDate > 0 ? ` · ${depuis(t, c.uploadDate)}` : ''
                }`,
            );
          cache[key] = { labels, total: list.length, at: Date.now() };
          saveRecentCache(cache);
          // Si la source a changé entre-temps, le résultat ne appartient plus à
          // l'écran courant : on l'écrit quand même au cache, on ne l'affiche pas.
          if (m.source === sourceName) {
            setCardRecent((prev) => ({ ...prev, [key]: labels }));
            setCardTotal((prev) => ({ ...prev, [key]: list.length }));
          }
        } catch {
          // Source morte ou chapitre vide : la carte reste simplement sans ligne.
          // Rien n'est mémorisé pour ne pas figer un échec définitif.
        } finally {
          recentInFlight.current.delete(key);
          setRecentBusy((n) => Math.max(0, n - 1));
        }
      }
    };
    void Promise.all(
      Array.from({ length: Math.min(RECENT_CONCURRENCY, todo.length) }, () => run()),
    );
    // `t` est une dépendance RÉELLE depuis que les étiquettes de chapitre sont
    // passées par `depuis(t, …)` : sans elle, changer de langue laisserait les
    // lignes « Ch. 12 · il y a 3 mois » dans l'ancienne langue. L'avertissement
    // `exhaustive-deps` est donc exact, pas un bruit à supprimer.
  }, [latest, showRecent, selected?.name, t, langue]);

  async function loadChapters(m: Manga) {
    setOpenManga(m);
    setLoadingState(true);
    setError(null);
    setChapters([]);
    setInfo(null);
    setChapterLimit(CHAPTER_PAGE);
    setChapterQuery('');
    setChecked(new Set());
    // La fiche et les chapitres partent en parallèle : une source lente pour
    // l'une ne doit pas retarder l'autre.
    void invoke<{ manga: Manga }>('details', {
      source: m.source,
      mangaUrl: m.url,
      mangaTitle: m.title,
    })
      .then((r) => {
        setInfo(r.manga ?? null);
        // Les deux sources de classification se contredisent : mesuré,
        // WAMANGA classe `ADULT` dans la liste mais `UNKNOWN` sur la fiche,
        // MANGABUFF fait l'inverse. On prend donc la PREMIÈRE valeur connue,
        // sans jamais écraser celle déjà enregistrée.
        const depuisFiche = r.manga?.contentRating;
        const depuisListe = m.contentRating;
        const connu = [depuisListe, depuisFiche].find((c) => c && c !== 'UNKNOWN');
        if (connu) {
          void invoke('note_content_rating', {
            source: m.source,
            mangaUrl: m.url,
            contentRating: connu,
          }).catch(() => {});
        }
        return r;
      })
      .catch(() => setInfo(null));
    try {
      const r = await invoke<{ chapters: Chapter[] }>('chapters', {
        source: m.source,
        mangaUrl: m.url,
        mangaTitle: m.title,
      });
      setChapters(r.chapters ?? []);
    } catch (e) {
      setError(String(e));
    } finally {
      setLoadingState(false);
    }
  }

  function toggleChapter(url: string) {
    setChecked((prev) => {
      const next = new Set(prev);
      if (next.has(url)) next.delete(url);
      else next.add(url);
      return next;
    });
  }

  /** Sélection en masse. Cocher 286 chapitres un par un n'est pas tenable :
   *  on propose « les N derniers », « les N premiers », « tout » et « aucun ».
   *  `pool` = les chapitres correspondant au filtre, donc « Tout » ne prend que
   *  ce que la recherche a gardé.
   *  « N derniers » sur une série de 286 alors que 100 cellules sont affichées
   *  cocherait des chapitres invisibles (constaté) : on déplie alors la grille
   *  jusqu'à la fin pour que la sélection soit visible. */
  function pickChapters(howMany: 'last' | 'first' | 'all' | 'none') {
    const pool = filteredChapters;
    const n = Math.min(pool.length, Math.max(1, Math.round(Number(pickCount) || 1)));
    if (howMany === 'last') {
      if (pool.length > chapterLimit) setChapterLimit(pool.length);
      // Amène la sélection dans le champ de vision : sans ça, on clique
      // « N derniers » et la grille affichée reste le début de la série,
      // les cases cochées sont 250 lignes plus bas.
      setReveal((v) => v + 1);
    }
    setChecked((prev) => {
      const next = new Set(prev);
      if (howMany === 'none') {
        pool.forEach((c) => next.delete(c.url));
        return next;
      }
      if (howMany === 'all') {
        pool.forEach((c) => next.add(c.url));
        return next;
      }
      const slice = howMany === 'last' ? pool.slice(-n) : pool.slice(0, n);
      slice.forEach((c) => next.add(c.url));
      return next;
    });
  }

  async function fetchOneChapter(
    source: string,
    manga: Manga,
    chapter: Chapter,
    totalChapters: number,
  ): Promise<string> {
    const title = chapterLabel(chapter);
    setProgress({ source, chapter: title, page: 0, total: 0, done: false, error: null });
    return invoke<string>('download_chapter', {
      source,
      mangaUrl: manga.url,
      mangaTitle: manga.title,
      coverUrl: manga.coverUrl ?? null,
      totalChapters,
      chapterUrl: chapter.url,
      chapterTitle: title,
    });
  }

  async function downloadChapter(c: Chapter) {
    if (!openManga || !selected) return;
    setDownloading(true);
    setError(null);
    try {
      const file = await fetchOneChapter(selected.name, openManga, c, chapters.length);
      setNotice(`Téléchargé : ${file}`);
      refreshDownloads();
    } catch (e) {
      setError(t('erreur.telechargement', { raison: String(e) }));
    } finally {
      setDownloading(false);
      setProgress(null);
    }
  }

  async function downloadSelected() {
    const targets = chapters.filter((c) => checked.has(c.url));
    for (const c of targets) {
      await downloadChapter(c);
    }
  }

  // ---------- Sélection multiple + file de téléchargement ----------

  function mangaKey(m: Manga) {
    return `${m.source}|${m.url}`;
  }

  function togglePick(m: Manga) {
    if (queueRunning) return;
    setPicked((prev) => {
      const next = new Set(prev);
      const key = mangaKey(m);
      if (next.has(key)) next.delete(key);
      else next.add(key);
      return next;
    });
  }

  function leaveSelectMode() {
    setSelectMode(false);
    setPicked(new Set());
  }

  /** Télécharge tous les chapitres de chaque webtoon de `targets`, un par un,
   *  en ignorant ce qui est déjà sur le disque. Annulable, et la file est
   *  persistée : si l'app s'arrête, il reste à la reprise. */
  async function runQueue(targets: Manga[]) {
    if (queueRunning || targets.length === 0) return;

    cancelQueue.current = false;
    setQueueRunning(true);
    setError(null);
    const pending = [...targets];
    saveQueue(pending);
    setQueue(
      targets.map((m) => ({
        key: mangaKey(m),
        title: m.title,
        source: m.source,
        total: 0,
        done: 0,
        skipped: 0,
        failed: 0,
        state: 'attente' as const,
      })),
    );

    for (const m of targets) {
      const key = mangaKey(m);
      const patch = (changes: Partial<QueueItem>) =>
        setQueue((prev) => prev.map((q) => (q.key === key ? { ...q, ...changes } : q)));

      if (cancelQueue.current) {
        patch({ state: 'pause' });
        continue;
      }
      patch({ state: 'téléchargement' });

      let list: Chapter[] = [];
      try {
        const r = await invoke<{ chapters: Chapter[] }>('chapters', {
          source: m.source,
          mangaUrl: m.url,
          mangaTitle: m.title,
        });
        list = r.chapters ?? [];
      } catch (e) {
        patch({ state: 'erreur', current: describeError(String(e), m.source, t) });
        continue;
      }
      patch({ total: list.length });

      // Reprise : on ne télécharge que ce qui manque vraiment. Le nom de fichier
      // attendu est calculé par Rust (même règle que le téléchargement), donc
      // aucune approximation côté interface.
      let todo: Chapter[] = list;
      try {
        await invoke('clean_partials', { source: m.source, title: m.title });
        const missingUrls = await invoke<string[]>('missing_chapters', {
          source: m.source,
          title: m.title,
          urls: list.map((c) => c.url),
          titles: list.map((c) => chapterLabel(c)),
        });
        const missing = new Set(missingUrls);
        todo = list.filter((c) => missing.has(c.url));
      } catch {
        todo = list;
      }
      const skipped = list.length - todo.length;
      patch({ skipped });

      let done = 0;
      let failed = 0;
      for (const c of todo) {
        if (cancelQueue.current) {
          patch({ done, failed, state: 'pause' });
          break;
        }
        try {
          await fetchOneChapter(m.source, m, c, list.length);
          done += 1;
        } catch {
          failed += 1;
        }
        patch({ done, failed, current: chapterLabel(c) });
      }
      patch({
        done,
        skipped,
        failed,
        state: cancelQueue.current ? 'pause' : failed > 0 ? 'erreur' : 'terminé',
      });

      // Ce webtoon est réglé : il sort de la file persistée (sauf annulation,
      // où il reste pour reprendre plus tard).
      if (!cancelQueue.current) {
        const at = pending.findIndex((x) => mangaKey(x) === key);
        if (at >= 0) pending.splice(at, 1);
        saveQueue(pending);
      }
    }

    if (cancelQueue.current) saveQueue(pending);
    setQueueRunning(false);
    setProgress(null);
    refreshDownloads();
    setTab('downloads');
  }

  function startQueue() {
    if (!selected) return;
    void runQueue(latest.filter((m) => picked.has(mangaKey(m))));
  }

  function resumeQueue() {
    const saved = loadSavedQueue();
    if (saved) void runQueue(saved.items);
  }

  /** Reprend un webtoon précis déjà entamé : on ne retélécharge que les
   *  chapitres absents (la commande `missing_chapters` fait la différence). */
  async function resumeManga(item: LibraryItem) {
    if (queueRunning) return;
    await runQueue([
      { url: item.url, title: item.title, source: item.source, coverUrl: item.coverUrl ?? null },
    ]);
  }

  function forgetQueue() {
    window.localStorage.removeItem(QUEUE_KEY);
    setQueue([]);
  }

  // ---------- Dossier de téléchargement ----------

  async function refreshDossier() {
    try {
      setDossierTelechargements(await invoke('get_downloads_dir'));
    } catch (e) {
      setError(t('telechargements.dossier.erreur', { raison: String(e) }));
    }
  }

  /** Ouvre le sélecteur natif, puis fait valider le dossier par Rust.
   *
   *  Le dialogue ne fait que choisir un chemin : c'est Rust qui décide s'il est
   *  utilisable (existe, accessible en écriture) et qui l'enregistre. Un
   *  dialogue qui accepte n'importe quoi serait une invitation à casser tous
   *  les téléchargements. */
  async function parcourirDossier() {
    try {
      const choisi = await open({ directory: true, multiple: false, title: t('telechargements.dossier.parcourir') });
      if (typeof choisi !== 'string') return; // annulé
      setDossierTelechargements(await invoke('set_downloads_dir', { chemin: choisi }));
      setNotice(t('telechargements.dossier.change'));
      await refreshDownloads();
    } catch (e) {
      setError(t('telechargements.dossier.erreur', { raison: String(e) }));
    }
  }

  async function restaurerDossier() {
    try {
      const chemin = await invoke<string>('reset_downloads_dir');
      await refreshDossier();
      setNotice(t('telechargements.dossier.retour', { chemin }));
      await refreshDownloads();
    } catch (e) {
      setError(t('telechargements.dossier.erreur', { raison: String(e) }));
    }
  }

  async function removeDownload(item: LibraryItem) {
    try {
      const n = await invoke<number>('remove_download', {
        source: item.source,
        title: item.title,
      });
      setNotice(`${item.title} : ${n} chapitre(s) supprimé(s).`);
      refreshDownloads();
    } catch (e) {
      setError(t('erreur.suppression', { raison: String(e) }));
    }
  }

  function cancelQueueNow() {
    cancelQueue.current = true;
  }

  /** Tâche 0.11 : ouvre la fenêtre de résolution anti-bot. */
  async function startChallenge() {
    if (!challenge || !selected) return;
    setChallengeBusy(true);
    setError(null);
    try {
      await invoke('open_challenge', { source: selected.name, url: challenge });
    } catch (e) {
      setError(`Fenêtre de vérification impossible : ${e}`);
    } finally {
      setChallengeBusy(false);
    }
  }

  /** Après résolution : pousse les cookies au moteur et rejoue la requête. */
  async function applyChallenge() {
    if (!challenge || !selected) return;
    setChallengeBusy(true);
    setError(null);
    try {
      const n = await invoke<number>('solve_challenge', {
        source: selected.name,
        url: challenge,
      });
      setNotice(`${n} cookie(s) transmis à ${selected.name}.`);
      setChallenge(null);
      await loadSource(selected, 0);
    } catch (e) {
      setError(`Vérification non appliquée : ${e}`);
    } finally {
      setChallengeBusy(false);
    }
  }

  async function cancelChallenge() {
    if (!selected) return;
    setChallenge(null);
    await invoke('cancel_challenge', { source: selected.name }).catch(() => {});
  }

  /** L'utilisateur a confirmé dans la fenêtre du site : on transmet les
   *  cookies au moteur. Le chapitre reste absent, donc il sera retéléchargé. */
  async function applyVerif() {
    if (!verif) return;
    setChallengeBusy(true);
    setError(null);
    try {
      const n = await invoke<number>('solve_challenge', {
        source: verif.source,
        url: verif.url,
      });
      if (n === 0) {
        setError(t('telechargements.verification.aucun.cookie'));
      } else {
        setNotice(t('telechargements.verification.cookies', { n, source: verif.source }));
        setVerif(null);
      }
    } catch (e) {
      setError(t('telechargements.verification.erreur', { detail: String(e) }));
    } finally {
      setChallengeBusy(false);
    }
  }

  async function cancelVerif() {
    if (!verif) return;
    setVerif(null);
    await invoke('cancel_challenge', { source: verif.source }).catch(() => {});
  }

  function backToSource() {
    setOpenManga(null);
    setChapters([]);
    setChapterLimit(CHAPTER_PAGE);
    setChapterQuery('');
  }

  function backToCatalog() {
    setSelected(null);
    setLatest([]);
    setOpenManga(null);
    setChapters([]);
    setChapterLimit(CHAPTER_PAGE);
    setChapterQuery('');
  }

  function savePageSize(value: number) {
    setPageSize(value);
    window.localStorage.setItem(PAGE_KEY, String(value));
  }

  function toggleSourceEnabled(name: string) {
    const current = enabled ?? sources.map((s) => s.name);
    const next = current.includes(name) ? current.filter((n) => n !== name) : [...current, name];
    setEnabled(next);
    window.localStorage.setItem(SOURCES_KEY, JSON.stringify(next));
  }

  // ---------- Lecteur (0.10) ----------

function pageSrc(token: string) {
  // Sous Windows, le protocole enregistré s'atteint par ce nom d'hôte.
  return `http://tatami.localhost/page/${token}`;
}

/** Couverture via le moteur : même protocole, préfixe `cover/`.
 *
 *  C'est cette fonction, et non `coverUrl`, qui doit alimenter un `<img src>`.
 *  Rust répond `undefined` si la série n'a pas de couverture, ce qui laisse la
 *  vignette de remplacement s'afficher au lieu d'une image cassée. */
function coverSrc(token?: string | null): string | undefined {
  return token ? `http://tatami.localhost/cover/${token}` : undefined;
}

/**
 * Transforme une entrée d'historique en `Manga` exploitable par `loadChapters`.
 *
 * Partagé par la reprise de lecture et par le clic sur la carte : les deux ont
 * besoin des mêmes champs, et une reconstruction dans chaque sens divergenceait
 * déjà par le passé.
 */
function mangaDepuisHistorique(h: HistoryItem): Manga {
  return {
    url: h.manga_url,
    title: h.title,
    source: h.source,
    coverUrl: h.cover_url,
    coverToken: h.cover_token,
    contentRating: h.content_rating,
  };
}

async function openReader(c: Chapter) {
  return openReaderAt(c, 0);
}

/** Ouvre un chapitre à une page donnée. `sourceName` permet la reprise depuis
 *  l'accueil, où aucune source n'est sélectionnée.
 *
 *  **Un chapitre téléchargé se lit DANS son fichier, sans le moteur et sans
 *  réseau** (tâche 2.3). Avant, le lecteur passait toujours par le moteur : un
 *  `.cbz` écrit par Tatami était donc illisible par Tatami, et le seul moyen de
 *  le lire était d'ouvrir un autre logiciel. On demande donc le local d'abord,
 *  et on ne retombe sur la source que si le fichier est absent.
 */
/**
 * Ouvre un chapitre à une page donnée.
 *
 * `mangaTitre` est un **paramètre**, pas une lecture de `openManga`. Mesuré le
 * 2026-10-05 : à la reprise, `openManga?.title` valait `""` — `setOpenManga()`
 * venait d'être appelé dans la même continuation `await`, et React n hadn't
 * pas encore appliqué l'état. Conséquence : `local_chapter_pages` cherchait un
 * dossier `downloads\<source>\` vide, renvoyait 0 page, et le lecteur partait
 * sur le moteur — donc **hors ligne ne l'était plus**, la bande perdait toutes
 * ses dimensions, et la reprise s'affichait à la page 1.
 *
 * Le lecteur porte déjà `mangaTitre` dans son état ; il suffit de s'y fier.
 */
async function openReaderAt(c: Chapter, startIndex: number, sourceName?: string, titreExplicite?: string) {
  const src = sourceName ?? selected?.name;
  if (!src) return;
  const mangaTitre = titreExplicite ?? openManga?.title ?? '';
  if (!mangaTitre) {
    // Sans titre, le chapitre local est introuvable : mieux vaut le dire que
    // ouvrir silencieusement une version en ligne d'un chapitre téléchargé.
    console.warn('openReaderAt: titre du webtoon inconnu, lecture locale impossible');
  }
  setReaderLoading(true);
  setError(null);
  try {
    const locales = await invoke<LocalPage[]>('local_chapter_pages', {
      source: src,
      mangaTitle: mangaTitre,
      chapterTitle: chapterLabel(c),
      chapterUrl: c.url,
    }).catch(() => [] as LocalPage[]);

    let pages: Page[];
    let horsLigne = false;
    if (locales.length > 0) {
      pages = locales.map((p) => ({ index: p.index, url: '', token: p.token, largeur: p.width, hauteur: p.height }));
      horsLigne = true;
    } else {
      const r = await invoke<{ pages: Page[] }>('pages', {
        source: src,
        chapterUrl: c.url,
      });
      pages = r.pages ?? [];
    }
    if (pages.length === 0) {
      setError('Ce chapitre n’a aucune page lisible.');
      return;
    }
    setPageJump('');
    // La position mémorisée peut dépasser le chapitre si celui-ci a été
    // raccourci depuis : on retombe sur sa dernière page.
    const index = Math.max(0, Math.min(pages.length - 1, startIndex));
    // La bande devra se placer sur cette page : le mode paginé n'en a pas
    // besoin, le webtoon si (c'est un conteneur de défilement).
    aPositionner.current = index > 0 ? index : null;
    setReader({
      chapter: c,
      pages,
      index,
      source: src,
      horsLigne,
      mangaTitre,
      mangaUrl: openManga?.url,
      coverUrl: openManga?.coverUrl ?? null,
    });
  } catch (e) {
    setError(t('erreur.lecture', { raison: String(e) }));
  } finally {
    setReaderLoading(false);
  }
}

/**
 * Les surcouches de l'application : panneau de vérification et notifications.
 *
 * Rendues **après la barre de navigation dans toutes les vues**, par un seul
 * appel. Le panneau de vérification avait déjà ce point unique, et il l'avait
 * mérité : rendu seulement dans une vue, il était invisible depuis les autres.
 * Les toasts suivent la même règle — une vue qui oublierait le conteneur
 * afficherait des notifications qui n'apparaissent jamais.
 */
function renderSuperpositions() {
  return (
    <>
      {renderVerif()}
      <Toaster toasts={toasts} onClose={fermerToast} />
    </>
  );
}

/**
 * Panneau de vérification d'une source, rendu **après la barre de navigation
 * dans toutes les vues**.
 *
 * Il était d'abord rendu seulement dans la vue Téléchargements. Mesuré le
 * 2026-10-04 : l'utilisateur était dans l'onglet Catalogues, donc
 * `panneauxH2: []` et `boutonsVisibles: []` — le bouton « C'est fait,
 * appliquer » n'existait nulle part, et le clic ne pouvait rien déclencher.
 * Un panneau d'action doit être visible là où l'action a été demandée, pas
 * dans la vue où l'on pense qu'elle restera.
 */
function renderVerif() {
  if (!verif) return null;
  return (
    <section className="panel resume">
      <h2>{t('telechargements.verification.titre', { source: verif.source })}</h2>
      <p className="muted">{t('telechargements.verification.note')}</p>
      <p className="muted">{verif.reason}</p>
      <div className="alert-actions">
        <button className="btn" disabled={challengeBusy} onClick={() => void applyVerif()}>
          {t('telechargements.verification.appliquer')}
        </button>
        <button className="btn btn-ghost" onClick={() => void cancelVerif()}>
          {t('telechargements.verification.fermer')}
        </button>
      </div>
    </section>
  );
}

/** Ouvre un chapitre téléchargé directement depuis la vue Téléchargements. */
async function openLocalChapter(item: LibraryItem, chapter: LibraryItem['chapters'][number]) {
  setReaderLoading(true);
  setError(null);
  try {
    const pages = await invoke<LocalPage[]>('local_file_pages', {
      dir: item.dir,
      name: chapter.file,
    });
    if (pages.length === 0) {
      setError(t('telechargements.lecture.VIDE', { chapitre: chapter.title }));
      return;
    }
    setPageJump('');
    setReader({
      chapter: { url: chapter.url, title: chapter.title, number: 0, uploadDate: 0 },
      pages: pages.map((p) => ({ index: p.index, url: '', token: p.token, largeur: p.width, hauteur: p.height })),
      index: 0,
      source: item.source,
      horsLigne: true,
      mangaTitre: item.title,
      mangaUrl: item.url,
      coverUrl: item.coverUrl ?? null,
    });
  } catch (e) {
    setError(t('erreur.lecture', { raison: String(e) }));
  } finally {
    setReaderLoading(false);
  }
}

/** Reprise de lecture : rouvre le dernier chapitre à la dernière page.
 *
 *  Point non évident : le moteur ne reconnaît un chapitre que si les chapitres
 *  de la fiche ont été listés PENDANT CETTE SESSION (son cache est en mémoire).
 *  Sans cet appel préalable, `pages` répond « unknown chapter url » et le
 *  lecteur ne s'ouvre pas — c'est exactement ce que renvoie l'enveloppe de
 *  539 octets observée dans les journaux.
 */
async function resumeReading(h: HistoryItem) {
  const manga = mangaDepuisHistorique(h);
  if (!h.chapter_url) {
    await loadChapters(manga);
    return;
  }
  setOpenManga(manga);
  setError(null);
  // Les deux appels partent ensemble : `details` sert au classement (incognito)
  // et à la couverture, `chapters` ré-enregistre les chapitres pour le moteur.
  const [ficheRes, chapitresRes] = await Promise.allSettled([
    invoke<{ manga: Manga }>('details', {
      source: h.source,
      mangaUrl: h.manga_url,
      mangaTitle: h.title,
    }),
    invoke<{ chapters: Chapter[] }>('chapters', {
      source: h.source,
      mangaUrl: h.manga_url,
      mangaTitle: h.title,
    }),
  ]);
  if (ficheRes.status === 'fulfilled' && ficheRes.value.manga) {
    setInfo(ficheRes.value.manga);
    // Même règle qu'à l'ouverture d'une fiche : première valeur connue.
    const connu = [ficheRes.value.manga.contentRating, h.content_rating].find(
      (c) => c && c !== 'UNKNOWN',
    );
    if (connu) {
      void invoke('note_content_rating', {
        source: h.source,
        mangaUrl: h.manga_url,
        contentRating: connu,
      }).catch(() => {});
    }
  }
  if (chapitresRes.status === 'rejected') {
    setError(t('erreur.chapitres', { raison: String(chapitresRes.reason) }));
    return;
  }
  const list: Chapter[] = chapitresRes.value.chapters ?? [];
  setChapters(list);
  // Le chapitre mémorisé a pu disparaître (source modifiée) : on retombe sur
  // le même rang, sinon sur le premier disponible.
  const chapter =
    list.find((c) => c.url === h.chapter_url) ??
    list[h.chapter_index] ??
    list[0];
  if (!chapter) {
    setError('Ce webtoon n’a plus aucun chapitre lisible.');
    return;
  }
  await openReaderAt(chapter, h.page_index, h.source, h.title);
}

// Mémorise la position de lecture à chaque changement de chapitre ou de page.
// Un `INSERT ... ON CONFLICT` par page tournée, sans conséquence visible. Le
// classement vient de la FICHE (`details`) quand elle est connue : mesuré, la
// liste renvoie souvent `UNKNOWN` là où la fiche donne `ADULT`, et c'est
// cette valeur que l'incognito filtre.
useEffect(() => {
  // Mesuré : la garde `!openManga` faisait que **rien n'était enregistré**
  // quand on lisait depuis la vue Téléchargements — pas de fiche ouverte, donc
  // pas d'`openManga`. Le webtoon ne réapparaissait jamais dans « Continuer »
  // ni dans l'historique. Le lecteur porte désormais lui-même l'URL et le
  // titre du webtoon, donc il n'a plus besoin de la fiche.
  if (!reader) return;
  const mangaUrl = reader.mangaUrl ?? openManga?.url;
  if (!mangaUrl) return;
  const manga = info ?? openManga;
  const chapterIndex = chapters.findIndex((c) => c.url === reader.chapter.url);
  void invoke('save_progress', {
    source: reader.source,
    mangaUrl,
    title: reader.mangaTitre ?? openManga?.title ?? 'sans-titre',
    coverUrl: reader.coverUrl ?? openManga?.coverUrl ?? manga?.coverUrl ?? null,
    contentRating: manga?.contentRating ?? null,
    chapterUrl: reader.chapter.url,
    chapterTitle: chapterLabel(reader.chapter),
    chapterIndex,
    pageIndex: reader.index,
    pageCount: reader.pages.length,
  }).catch(() => {
    /* la reprise est un confort : une écriture ratée ne doit pas bruiter */
  });
}, [reader, openManga, info, chapters]);

function closeReader() {
  // Sortir du plein écran AVANT de retirer le lecteur. Mesuré : le plein écran
  // porte sur `.reader`, et supprimer l'élément qui est en plein écran laisse
  // la fenêtre dans un état dégradé — signalé par l'utilisateur : « le plein
  // écran a un impact sur toute la fenêtre quand je sors de la lecture ». On
  // demande donc explicitement la sortie, et le retrait du lecteur a lieu au
  // tour d'après, le temps que le navigateur applique le changement.
  if (document.fullscreenElement) {
    void document.exitFullscreen().catch(() => {});
    setPleinEcran(false);
    setTimeout(() => {
      setReader(null);
      apresFermetureLecteur();
    }, 0);
    return;
  }
  setReader(null);
  apresFermetureLecteur();
}

/** Reprise depuis l'accueil : aucune source n'est sélectionnée, on rend donc
 *  l'accueil plutôt qu'une fiche de chapitre orpheline. */
function apresFermetureLecteur() {
  if (!selected) {
    setTab('home');
    setOpenManga(null);
  }
}

function switchMode(mode: ReaderMode) {
  setReaderMode(mode);
  window.localStorage.setItem(READER_MODE_KEY, mode);
}

function changeZoom(next: number) {
  const value = Math.min(200, Math.max(50, next));
  setZoom(value);
  window.localStorage.setItem(READER_ZOOM_KEY, String(value));
}

/** En double page, un cran de navigation avance de deux pages. */
function step(): number {
  return readerMode === 'page' && spread ? 2 : 1;
}

function switchFit(next: ReaderFit) {
  setFit(next);
  window.localStorage.setItem(READER_FIT_KEY, next);
}

function toggleSpread() {
  setSpread((prev) => {
    const next = !prev;
    window.localStorage.setItem(READER_SPREAD_KEY, next ? '1' : '0');
    // Referme sur une page paire pour que la double page démarre sur un
    // couple cohérent (0-1, 2-3, ...) au lieu d'un décalage d'une page.
    setReader((r) => (r && next && r.index % 2 === 1 ? { ...r, index: r.index - 1 } : r));
    return next;
  });
}

function switchDir() {
  setReaderDir((prev) => {
    const next: ReaderDir = prev === 'rtl' ? 'ltr' : 'rtl';
    window.localStorage.setItem(READER_DIR_KEY, next);
    return next;
  });
}

/** Tâche 0.14 : saute à la page demandée (1 → index 0).
 *
 *  En mode webtoon il faut aussi **défiler** : le mode pagé montre la page
 *  courante par construction, pas la bande.
 */
function jumpToPage(raw: string) {
  const n = Number(raw);
  setPageJump('');
  if (!reader || !Number.isFinite(n) || n < 1) return;
  const target = Math.min(reader.pages.length, Math.round(n)) - 1;
  if (readerMode === 'webtoon') scrollToPage(target);
  else setReader((r) => (r ? { ...r, index: target } : r));
}

/** Fait défiler la bande jusqu'à la page demandée (mode webtoon).
 *
 *  En mode virtualisé, l'image n'existe peut-être pas encore dans le DOM : on
 *  positionne alors le conteneur sur l'offset **calculé**, ce qui fonctionne
 *  dans les deux cas et évite de dépendre du rendu.
 */
function scrollToPage(i: number) {
  const root = stripRef.current;
  const cible = i >= 0 && i < pagesBande.length ? i : 0;
  const haut = offsets[cible];
  if (root && haut !== undefined) {
    root.scrollTo({ top: Math.max(0, haut - 8), behavior: 'smooth' });
    setReader((r) => (r ? { ...r, index: cible } : r));
    return;
  }
  const el = stripImages.current[cible];
  if (el) {
    el.scrollIntoView({ behavior: 'smooth', block: 'start' });
    setReader((r) => (r ? { ...r, index: cible } : r));
  } else {
    turn(cible - (reader?.index ?? 0));
  }
}

// ---------- Plein écran du lecteur ----------
// Le plein écran porte sur l'élément `.reader`, pas sur `document` : la fenêtre
// de l'application reste visible si l'utilisateur en sort, et surtout les
// raccourcis et la barre de navigation de Tatami ne disparaissent pas
// définitivement. `:fullscreen` est une pseudo-classe CSS, donc l'état est
// lu depuis le DOM — c'est la seule source fiable, since `Échap` et le bouton
// du système peuvent sortir du plein écran sans passer par notre handler.
const [pleinEcran, setPleinEcran] = useState(false);
useEffect(() => {
  const surChangement = () => {
    setPleinEcran(!!document.querySelector('.reader:fullscreen'));
  };
  document.addEventListener('fullscreenchange', surChangement);
  return () => document.removeEventListener('fullscreenchange', surChangement);
}, []);

async function basculerPleinEcran() {
  try {
    const cible = document.querySelector('.reader') as HTMLElement | null;
    if (!cible) return;
    if (document.fullscreenElement) {
      await document.exitFullscreen();
    } else {
      await cible.requestFullscreen();
    }
  } catch (e) {
    // Refusé par la webview : on le dit plutôt que de ne rien faire, sinon le
    // bouton semble cassé (leçon AGENTS.md : une garde silencieuse est un piège).
    setError(t('lecteur.plein.ecran.erreur', { raison: String(e) }));
  }
}

// ---------- Bande webtoon virtualisée (tâche 1.9) ----------
// Rust renvoie la **taille réelle** de chaque page (lue dans l'en-tête de
// l'image). On en déduit la hauteur affichée, puis la position de chaque page.
// La hauteur de la bande est donc exacte dès le premier rendu : la barre de
// défilement est juste et une fenêtre glissante peut ne monter qu'une dizaine
// de nœuds au lieu de 303 (mesuré : Killer Peter, 303 pages dans un `.cbz`).
//
// Ces `useMemo` sont ici, **avant** les retours conditionnels de vue : les mettre
// dans le bloc du lecteur violerait l'ordre des hooks, puisque le composant
// retourne avant d'y arriver selon la vue affichée.
const pagesBande = reader?.pages ?? EMPTY_PAGES;
const formatBande = (readerMode === 'webtoon' ? zoom : 100) / 100;
const hauteurs = useMemo(() => {
  const l = largeurBande || 1;
  return pagesBande.map((p) => {
    const w = p.largeur ?? 0;
    const h = p.hauteur ?? 0;
    if (w <= 0 || h <= 0) return 0;
    return Math.round((l * formatBande * h) / w);
  });
}, [pagesBande, largeurBande, formatBande]);

const offsets = useMemo(() => {
  const out: number[] = [];
  let y = 0;
  for (const h of hauteurs) {
    out.push(y);
    y += h;
  }
  return out;
}, [hauteurs]);

/** `true` quand chaque page a une hauteur calculée : la bande est virtualisable.
 *
 *  `false` = au moins une page sans dimension. On rend alors tout, en flux
 *  normal — **jamais une estimation** : une hauteur de réserve fausse faisait
 *  reculer la position de défilement de 3 964 à 1 788 px (mesuré).
 */
const bandeVirtualisable = useMemo(
  () => hauteurs.length > 0 && !hauteurs.some((h) => h <= 0),
  [hauteurs],
);

const bandeHauteur = useMemo(() => {
  if (!bandeVirtualisable) return 0;
  return hauteurs.reduce((n, h) => n + h, 0);
}, [hauteurs, bandeVirtualisable]);

const fenetre = useMemo(() => {
  const tous = pagesBande.map((_, i) => i);
  if (readerMode !== 'webtoon' || !bandeVirtualisable) return tous;
  let premier = 0;
  while (
    premier < pagesBande.length &&
    (offsets[premier] ?? 0) + (hauteurs[premier] ?? 0) < scrollY
  ) {
    premier += 1;
  }
  const marge = 2;
  const debut = Math.max(0, premier - marge);
  // On monte jusqu'à couvrir la fenêtre visible plus une page, puis deux de
  // marge : de quoi éviter le « trou blanc » pendant un défilement rapide.
  let y = offsets[premier] ?? 0;
  let fin = premier;
  while (fin < pagesBande.length && y < scrollY + 2000) {
    y += hauteurs[fin] ?? 0;
    fin += 1;
  }
  const limite = Math.min(pagesBande.length, fin + marge);
  const out: number[] = [];
  for (let i = debut; i < limite; i += 1) out.push(i);
  return out;
}, [pagesBande, readerMode, scrollY, offsets, hauteurs, bandeVirtualisable]);

// La largeur du conteneur décide de la hauteur affichée : il faut la mesurer,
// sinon la première bande est dimensionnée sur une largeur nulle.
const nbPagesBande = pagesBande.length;
useEffect(() => {
  const root = stripRef.current;
  if (!root) return;
  const mesurer = () => setLargeurBande(root.clientWidth);
  mesurer();
  const observateur = new ResizeObserver(mesurer);
  observateur.observe(root);
  return () => observateur.disconnect();
}, [nbPagesBande, readerMode]);

/** Défilement de la bande : recalcule la fenêtre de pages montées. */
function onScrollBande() {
  const root = stripRef.current;
  if (!root) return;
  const y = root.scrollTop;
  if (Math.abs(y - dernierScroll.current) < 40) return; // seuil : évite un rendu par pixel
  dernierScroll.current = y;
  if (!enAttente.current) {
    enAttente.current = true;
    requestAnimationFrame(() => {
      enAttente.current = false;
      setScrollY(y);
    });
  }
  // Le compteur de pages suit la page réellement visible.
  let i = 0;
  for (let k = 0; k < offsets.length; k += 1) {
    if ((offsets[k] ?? 0) <= y + 8) i = k;
    else break;
  }
  if (reader && i !== reader.index) setReader((r) => (r ? { ...r, index: i } : r));
}

/** Place la bande sur la page courante à l'ouverture d'un chapitre.
 *
 *  Mesuré le 2026-10-05 : la reprise affichait la **page 1** alors que
 *  `reader.index` valait 50. Rien ne positionnait la bande — ni à
 *  l'ouverture, ni au changement de mode. Le mode paginé n'en a pas besoin
 *  (la page courante est rendue), mais la bande est un conteneur de défilement :
 *  sans `scrollTop`, elle reste en haut quoi qu'on dise dans l'état.
 *
 *  On attend que les dimensions soient connues : `offsets` est calculé à partir
 *  de `largeurBande`, mesurée par un `ResizeObserver` qui n'a pas encore tourné
 *  au premier rendu. Défiler trop tôt reviendrait à 0 — c'est-à-dire exactement
 *  le bug. D'où `bandeVirtualisable` comme garde : c'est lui, et non « offset
 *  non nul » (offsets[0] vaut toujours 0), qui dit que les dimensions sont
 *  prêtes.
 *
 *  `aPositionner` est déclaré **avec les autres refs**, avant tout retour
 *  conditionnel : un `useRef` lu par une fonction (ligne de `openReaderAt`)
 *  doit exister avant elle, et les hooks doivent être tous au même niveau.
 */
useEffect(() => {
  if (readerMode !== 'webtoon' || !reader) return;
  const i = aPositionner.current;
  if (i === null) return;
  // On ne positionne QUE dans le bandeau virtualisé : c'est le seul cas où
  // l'offset calculé est juste. En flux normal, `offsetTop` d'une image dépend
  // de la hauteur réelle de celles qui la précèdent, connue seulement une fois
  // les images chargées — défiler avant donnerait 0 (mesuré).
  if (!bandeVirtualisable) return;
  const haut = offsets[i];
  if (haut === undefined) return;
  const root = stripRef.current;
  if (!root) return;
  aPositionner.current = null;
  const y = Math.max(0, haut - 8);
  root.scrollTo({ top: y });
  dernierScroll.current = y;
  // `fenetre` se calcule à partir de `scrollY`, que seul `onScrollBande` met à
  // jour. Sans cette ligne, la bande était positionnée sur la bonne page mais
  // gardait les **images du début** montées : la page 100 était hors du DOM,
  // donc l'écran paraissait vide (mesuré : scrollTop 199576, images 0, 1, 2).
  setScrollY(y);
}, [reader, readerMode, offsets, largeurBande, bandeVirtualisable]);

// En mode webtoon, le compteur suit la page réellement visible.
useEffect(() => {
  if (!reader || readerMode !== 'webtoon') return;
  const root = stripRef.current;
  if (!root) return;
  const observer = new IntersectionObserver(
    (entries) => {
      for (const entry of entries) {
        if (!entry.isIntersecting) continue;
        const i = stripImages.current.indexOf(entry.target as HTMLImageElement);
        if (i >= 0) setReader((r) => (r && r.index !== i ? { ...r, index: i } : r));
      }
    },
    { root, threshold: 0.6 },
  );
  for (const el of stripImages.current) if (el) observer.observe(el);
  return () => observer.disconnect();
  // L'observateur dépend du chapitre, du mode et du nombre de pages — pas de
  // l'objet `reader` complet, qui est recréé à chaque changement de page et
  // rejouerait l'observation sans raison.
}, [reader?.chapter.url, readerMode, reader?.pages.length]);

function turn(delta: number) {
  setReader((r) => {
    if (!r) return r;
    const next = r.index + delta;
    if (next < 0 || next >= r.pages.length) return r;
    return { ...r, index: next };
  });
}

/** Tâche 1.8 : la molette tourne la page en mode paginé.
 *
 *  Mesuré avant correction : la molette ne faisait **rien** dans le lecteur paginé
 *  (trois positions testées, « rien ne bouge ») alors que la tâche 1.8 demande
 *  des « raccourcis clavier et souris ». Le clavier était couvert, la souris pas.
 *
 *  Deux garde-fous, tous deux nécessaires :
 *
 *  - **Un seuil de distance.** La molette émet des dizaines d'événements pour un
 *    seul geste de la main. Sans seuil, un geste de lecture traverserait dix
 *    pages d'un coup — mesuré : 43 événements pour 2 crans de molette.
 *  - **`Ctrl` est réservé au zoom**, sinon on ne pourrait plus zoomer à la
 *    molette alors que c'est le geste attendu sur une image.
 */
function onMoletteStage(e: React.WheelEvent<HTMLDivElement>) {
  // En mode webtoon la bande gère son propre défilement : on ne s'en mêle pas.
  if (readerMode !== 'page') return;
  if (e.ctrlKey) return;
  const delta = e.deltaY;
  if (Math.abs(delta) < DELTA_MOLETTE_MIN) return;
  e.preventDefault();
  // Une seule page par direction et par gesture : on mémorise le signe courant et
  // on ignore les événements qui suivent tant qu'il n'a pas changé.
  const sens = delta > 0 ? 1 : -1;
  if (moletteEnCours.current === sens) return;
  moletteEnCours.current = sens;
  window.setTimeout(() => {
    moletteEnCours.current = 0;
  }, DELTA_MOLETTE_FIN_MS);
  turn(sens * step());
}

useEffect(() => {
  if (!reader) return;
  const onKey = (e: KeyboardEvent) => {
    const webtoon = readerMode === 'webtoon';
    const last = reader ? reader.pages.length - 1 : 0;
    // Un cran = 1 page, ou 2 en double page : le lecteur ne s'arrête jamais
    // au milieu d'un couple de pages.
    const by = step();
    if (e.key === 'Escape') {
      // En plein écran, `Échap` doit d'abord sortir du plein écran : fermer le
      // lecteur d'un coup est exactement ce que l'utilisateur ne cherche pas,
      // et il perdrait sa page. Mesuré avant correction : un seul `Échap`
      // fermait les deux.
      if (document.fullscreenElement) {
        void document.exitFullscreen().catch(() => {});
        return;
      }
      closeReader();
      return;
    }
    const forward = ['ArrowRight', 'ArrowDown', 'PageDown', ' '];
    const back = ['ArrowLeft', 'ArrowUp', 'PageUp'];
    if (forward.includes(e.key)) {
      e.preventDefault();
      webtoon ? scrollToPage(Math.min(last, reader!.index + 1)) : turn(by);
    } else if (back.includes(e.key)) {
      e.preventDefault();
      webtoon ? scrollToPage(Math.max(0, reader!.index - 1)) : turn(-by);
    } else if (e.key === 'Home') {
      e.preventDefault();
      webtoon ? scrollToPage(0) : setReader((r) => (r ? { ...r, index: 0 } : r));
    } else if (e.key === 'End') {
      e.preventDefault();
      webtoon
        ? scrollToPage(last)
        : setReader((r) => (r ? { ...r, index: r.pages.length - 1 } : r));
    }
  };
  window.addEventListener('keydown', onKey);
  return () => window.removeEventListener('keydown', onKey);
  // `scrollToPage` et `step` sont redéfinies à chaque rendu ; les inclure
  // rebouclerait l'écouteur sur chaque changement d'état du lecteur.
}, [reader, readerMode, spread]);

  // ---------- Vue : Réglages ----------
  // Chaque vue s'affiche par un `return` conditionnel. Le lecteur doit donc
  // **gagner contre toutes** : sans ce `!reader`, `if (reader)` plus bas
  // n'est jamais atteint depuis une vue qui a déjà rendu son `return`, et le
  // clic « Lire » sur un webtoon téléchargé ne fait strictement rien
  // (mesuré : aucun message, aucune erreur, lecteur absent).
  if (tab === 'settings' && !reader) {
    return (
      <main>
        <AppBar tab={tab} onTab={goTab} />{renderSuperpositions()}
        <button className="crumb" onClick={() => setTab('catalog')}>
          {'←'} Catalogues
        </button>
        <header className="page-head">
          <h1>{t('nav.parametres')}</h1>
        </header>

        {/* Mesuré : un échec déclenché depuis cet écran (catégorie en double,
            par exemple) était totalement invisible, l'utilisateur cliquait et
            rien ne se passait. Les autres vues affichent déjà cette erreur. */}
        {error && <p className="error">{error}</p>}

        <section className="panel">
          <h2>{t('reglages.contenu')}</h2>
          <label className="field">
            {t('reglages.contenu.affiche')}
            <select
              value={rating}
              onChange={(e) => {
                setRating(e.target.value);
                window.localStorage.setItem(SAFE_KEY, e.target.value);
              }}
              aria-label={t('reglages.contenu.affiche')}
            >
              {RATINGS.map((r) => (
                <option key={r.value} value={r.value}>
                  {t(r.cle)}
                </option>
              ))}
            </select>
          </label>
          <p className="muted">{t('reglages.contenu.note')}</p>
        </section>

        <section className="panel">
          <h2>{t('reglages.categories')}</h2>
          <p className="muted">{t('reglages.categories.note')}</p>

          {categories.length === 0 ? (
            <p className="muted">{t('reglages.categories.vide')}</p>
          ) : (
            <ul className="cat-list">
              {categories.map((c, i) => (
                <li className="cat-row" key={c.id}>
                  <span
                    className="cat-dot"
                    style={{ background: couleurCategorie(c.color) }}
                    aria-hidden="true"
                  />
                  <input
                    className="cat-name"
                    defaultValue={c.name}
                    aria-label={t('reglages.categories.nom', { nom: c.name })}
                    onBlur={(e) => {
                      const v = e.target.value.trim();
                      if (v && v !== c.name) void renommerCategorie(c.id, v, c.color);
                      else e.target.value = c.name;
                    }}
                    onKeyDown={(e) => {
                      if (e.key === 'Enter') e.currentTarget.blur();
                    }}
                  />
                  <div className="chips" role="group" aria-label={`Couleur de ${c.name}`}>
                    {CATEGORY_COLORS.map((col, k) => (
                      <button
                        key={col}
                        className={`cat-swatch${c.color === k + 1 ? ' on' : ''}`}
                        style={{ background: col }}
                        aria-pressed={c.color === k + 1}
                        aria-label={t('reglages.categories.couleur', { nom: c.name })}
                        onClick={() => void renommerCategorie(c.id, c.name, k + 1)}
                      />
                    ))}
                  </div>
                  <span className="cat-count muted">
                    {favorites.filter((f) => f.category_id === c.id).length}
                  </span>
                  <button
                    className="btn btn-ghost"
                    aria-label={t('reglages.categories.monter', { nom: c.name })}
                    title={t('reglages.categories.monter', { nom: c.name })}
                    disabled={i === 0}
                    onClick={() => void deplacerCategorie(c.id, -1)}
                  >
                    ↑
                  </button>
                  <button
                    className="btn btn-ghost"
                    aria-label={t('reglages.categories.descendre', { nom: c.name })}
                    title={t('reglages.categories.descendre', { nom: c.name })}
                    disabled={i === categories.length - 1}
                    onClick={() => void deplacerCategorie(c.id, 1)}
                  >
                    ↓
                  </button>
                  <button
                    className="btn btn-ghost"
                    aria-label={t('reglages.categories.supprimer')}
                    onClick={() => void supprimerCategorie(c.id)}
                  >
                    {t('reglages.categories.supprimer')}
                  </button>
                </li>
              ))}
            </ul>
          )}

          <div className="cat-add">
            <input
              className="cat-name"
              placeholder={t('reglages.categories.nouvelle')}
              aria-label={t('reglages.categories.nouvelle')}
              value={nouvelleCategorie}
              onChange={(e) => setNouvelleCategorie(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === 'Enter') void validerNouvelleCategorie();
              }}
            />
            <button
              className="btn btn-primary"
              disabled={nouvelleCategorie.trim().length === 0}
              onClick={() => void validerNouvelleCategorie()}
            >
              {t('reglages.categories.ajouter')}
            </button>
          </div>
        </section>

        <section className="panel">
          <h2>{t('reglages.apparence')}</h2>
          <label className="field">
            {t('reglages.apparence')}
            <select
              value={apparence}
              onChange={(e) => setApparence(e.target.value === 'clair' ? 'clair' : 'sombre')}
              aria-label={t('reglages.apparence')}
            >
              <option value="sombre">{t('reglages.apparence.sombre')}</option>
              <option value="clair">{t('reglages.apparence.clair')}</option>
            </select>
          </label>
          <p className="muted">{t('reglages.apparence.note')}</p>
        </section>

        <section className="panel">
          <h2>{t('reglages.langue')}</h2>
          <label className="field">
            {t('reglages.langue')}
            <select
              value={langue}
              onChange={(e) => setLangue(e.target.value === 'en' ? 'en' : 'fr')}
              aria-label={t('reglages.langue')}
            >
              <option value="fr">{t('reglages.langue.fr')}</option>
              <option value="en">{t('reglages.langue.en')}</option>
            </select>
          </label>
          <p className="muted">{t('reglages.langue.note')}</p>
        </section>

        <section className="panel">
          <h2>{t('telechargements.dossier')}</h2>
          <p className="muted">{t('telechargements.dossier.note')}</p>
          <div className="row-actions">
            <button className="btn btn-primary" onClick={parcourirDossier}>
              {t('telechargements.dossier.parcourir')}
            </button>
            {!dossierTelechargements.parDefaut && (
              <button className="btn btn-ghost" onClick={restaurerDossier}>
                {t('telechargements.dossier.defaut')}
              </button>
            )}
          </div>
        </section>

        <section className="panel">
          <h2>{t('reglages.reseau')}</h2>
          <label className="field">
            {t('reglages.reseau.delai')}
            <select
              value={listTimeout}
              onChange={(e) => {
                const v = Number(e.target.value);
                setListTimeout(v);
                window.localStorage.setItem(TIMEOUT_KEY, String(v));
              }}
              aria-label={t('reglages.reseau.delai')}
            >
              {TIMEOUTS.map((secondes) => (
                <option key={secondes} value={secondes}>
                  {t('reglages.reseau.secondes', { n: secondes })}
                </option>
              ))}
            </select>
          </label>
          <p className="muted">{t('reglages.reseau.note')}</p>
        </section>

        <section className="panel">
          <h2>{t('reglages.telechargement')}</h2>
          <label className="field">
            Nombre de webtoons par page
            <select
              value={pageSize}
              onChange={(e) => savePageSize(Number(e.target.value))}
              aria-label="Nombre de webtoons par page"
            >
              {[12, 24, 48, 100].map((n) => (
                <option key={n} value={n}>
                  {n}
                </option>
              ))}
            </select>
          </label>
          <p className="muted">
            {t('telechargements.dossier.ligne')}
            <code className="path">{dossierTelechargements.chemin}</code>
          </p>
        </section>

        {/* Tâche 5.4 — écran « état des sources ».
         *
         *  Ce qui existait : un compteur « N sources signalées hors service » et
         *  un bouton « Tout réessayer ». Aucune liste, aucune date, aucun message
         *  d'erreur — donc impossible de savoir *quelle* source casse ni
         *  *depuis quand*, alors que c'est précisément ce que la tâche demande
         *  (« pour repérer ce qui est cassé »).
         *
         *  La donnée est déjà là : `noteHealth` enregistre `{ code, message, at }`
         *  par source et le persiste. Il n'y avait qu'à l'afficher.
         *
         *  Les chiffres sont regroupés par code d'erreur : 1 200 sources en
         *  `CHALLENGE_REQUIRED` sont un problème de Cloudflare, pas 1 200
         *  pannes distinctes à pursue une par une.
         */}
        <section className="panel">
          <h2>{t('etat.titre')}</h2>
          {(() => {
            const noms = Object.keys(health);
            if (noms.length === 0) {
              return <p className="muted">{t('etat.tout.va')}</p>;
            }
            // Tri : plus récent d'abord — c'est la panne à regarder en premier.
            const lignes = noms
              .map((nom) => ({ nom, ...health[nom] }))
              .sort((a, b) => b.at - a.at);
            const parCode = new Map<string, number>();
            for (const l of lignes) parCode.set(l.code, (parCode.get(l.code) ?? 0) + 1);
            return (
              <>
                <p className="muted">
                  {lignes.length === 1 ? t('etat.resume1') : t('etat.resume', { n: lignes.length })}
                </p>
                <ul className="code-list">
                  {[...parCode.entries()]
                    .sort((a, b) => b[1] - a[1])
                    .map(([code, n]) => (
                      <li key={code}>
                        <code>{code}</code>
                        <span className="muted">
                          {n === 1 ? t('etat.sources1') : t('etat.sources', { n })}
                        </span>
                      </li>
                    ))}
                </ul>
                <table className="etat-table">
                  <thead>
                    <tr>
                      <th scope="col">{t('etat.source')}</th>
                      <th scope="col">{t('etat.code')}</th>
                      <th scope="col">{t('etat.dernier')}</th>
                    </tr>
                  </thead>
                  <tbody>
                    {lignes.map((l) => (
                      <tr key={l.nom}>
                        <td>
                          <b>{l.nom}</b>
                          {l.message ? <span className="muted">{l.message}</span> : null}
                        </td>
                        <td>
                          <code>{l.code}</code>
                        </td>
                        <td>
                          <time dateTime={new Date(l.at).toISOString()}>
                            {depuis(t, l.at)}
                          </time>
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </table>
                <div className="row-actions">
                  <button
                    className="btn btn-ghost"
                    onClick={() => {
                      window.localStorage.removeItem(HEALTH_KEY);
                      setHealth({});
                    }}
                  >
                    {t('etat.reessayer')}
                  </button>
                </div>
              </>
            );
          })()}
        </section>

        <section className="panel">
          <h2>{t('reglages.catalogues')}</h2>
          <div className="setting-bar">
            <button
              className="btn"
              onClick={() => {
                const all = sources.map((s) => s.name);
                const next = enabled === null || enabled.length === all.length ? [] : all;
                setEnabled(next);
                window.localStorage.setItem(SOURCES_KEY, JSON.stringify(next));
              }}
            >
              {enabled === null || enabled.length === sources.length ? 'Tout désactiver' : 'Tout activer'}
            </button>
            <span className="muted">
              {visibleSources.length} / {sources.length} catalogues
            </span>
          </div>
          <div className="chip-wrap">
            {sources.slice(0, 200).map((s) => {
              // Tant que l'utilisateur n'a rien touché (enabled === null), tout est
              // affiché : on ne peint les chips en « actif » que sur une sélection explicite.
              const on = enabled !== null && enabled.includes(s.name);
              return (
                <button
                  key={s.name}
                  className="chip"
                  aria-pressed={on}
                  onClick={() => toggleSourceEnabled(s.name)}
                >
                  {s.name}
                </button>
              );
            })}
          </div>
          {sources.length > 200 && (
            <p className="muted">200 premiers catalogues sur {sources.length}. Utilise la recherche pour les autres.</p>
          )}
        </section>
      </main>
    );
  }

  // ---------- Vue : Téléchargements ----------
  if (tab === 'downloads' && !reader) {
    const running = queue.find((q) => q.state === 'téléchargement');
    const savedQueue = loadSavedQueue();
    const totalChapters = library.reduce((n, i) => n + i.downloaded, 0);
    const totalBytes = library.reduce((n, i) => n + i.sizeBytes, 0);
    const partiels = library.filter((i) => !i.complete).length;
    return (
      <main>
        <AppBar tab={tab} onTab={goTab} />{renderSuperpositions()}
        <header className="page-head">
          <h1>{t('nav.telechargements')}</h1>
        </header>

        {error && <p className="error">{error}</p>}

        {chapitresVisibles && (
          <dialog
            className="confirm"
            open
            aria-labelledby="chapitres-titre"
            onCancel={() => setChapitresVisibles(null)}
          >
            <h2 id="chapitres-titre">{chapitresVisibles.title}</h2>
            {chapitresVisibles.chapters.length === 0 ? (
              <p className="muted">{t('telechargements.chapitres.aucun')}</p>
            ) : (
              <div className="chap-list">
                {chapitresVisibles.chapters.map((c) => (
                  <button
                    key={c.file}
                    className="btn"
                    onClick={() => {
                      const cible = chapitresVisibles;
                      setChapitresVisibles(null);
                      void openLocalChapter(cible, c);
                    }}
                  >
                    {c.title}
                  </button>
                ))}
              </div>
            )}
            <div className="alert-actions">
              <button className="btn btn-ghost" onClick={() => setChapitresVisibles(null)}>
                {t('telechargements.verification.fermer')}
              </button>
            </div>
          </dialog>
        )}

        {savedQueue && savedQueue.items.length > 0 && !queueRunning && (
          <section className="panel resume">
            <h2>{t('telechargements.file.interrompue')}</h2>
            <p className="muted">
              {t('telechargements.file.interrompue.note', {
                n: savedQueue.items.length,
                s: savedQueue.items.length > 1 ? 's' : '',
              })}
            </p>
            <div className="alert-actions">
              <button className="btn btn-primary" onClick={resumeQueue}>
                {t('telechargements.file.reprendre')}
              </button>
              <button className="btn btn-ghost" onClick={forgetQueue}>
                {t('telechargements.file.abandonner')}
              </button>
            </div>
          </section>
        )}

        {queue.length > 0 && (
          <section className="panel queue">
            <h2>{t('telechargements.file')}</h2>
            {queueRunning ? (
              <button className="btn" onClick={cancelQueueNow} title={t('telechargements.pause.note')}>
                {t('telechargements.pause')}
              </button>
            ) : (
              <button className="btn btn-ghost" onClick={() => setQueue([])}>
                {t('telechargements.effacer.file')}
              </button>
            )}
            <div className="queue-list">
              {queue.map((q) => {
                const pct = q.total > 0 ? Math.round(((q.done + q.skipped) / q.total) * 100) : 0;
                return (
                  <div className="queue-row" key={q.key}>
                    <div className="queue-head">
                      <span className="queue-title">{q.title}</span>
                      <span className={`badge state-${q.state.replace('é', 'e')}`}>
                        {q.state === 'pause'
                          ? t('telechargements.pause.etat')
                          : q.state}
                      </span>
                    </div>
                    <div className="progress">
                      <div className="progress-bar" style={{ width: `${pct}%` }} />
                    </div>
                    <span className="queue-detail">
                      {q.total > 0
                        ? `${q.done + q.skipped}/${q.total} chapitres${q.skipped > 0 ? ` (${q.skipped} déjà là)` : ''}${q.failed > 0 ? ` · ${q.failed} en échec` : ''}`
                        : 'préparation…'}
                      {q.current ? ` · ${q.current}` : ''}
                    </span>
                  </div>
                );
              })}
            </div>
            {running && progress && progress.total > 0 && (
              <p className="muted">
                {running.title} — {progress.chapter} : page {progress.page}/{progress.total}
              </p>
            )}
          </section>
        )}

        {library.length === 0 ? (
          <p className="muted">{t('telechargements.aucun')}</p>
        ) : (
          <>
            <section className="dl-stats" aria-label={t('telechargements.recap')}>
              <div className="dl-stat">
                <b>{library.length}</b>
                <span>
                  {t('telechargements.stat.webtoons', {
                    s: library.length > 1 ? 's' : '',
                  })}
                </span>
              </div>
              <div className="dl-stat">
                <b>{totalChapters}</b>
                <span>{t('telechargements.stat.chapitres')}</span>
              </div>
              <div className="dl-stat">
                <b>{formatBytes(totalBytes)}</b>
                <span>{t('telechargements.stat.disque')}</span>
              </div>
              <div className="dl-stat">
                <b>{partiels}</b>
                <span>{t('telechargements.stat.a.completer')}</span>
              </div>
            </section>

            <div className="band">{t('telechargements.vos.webtoons')}</div>

            <div className="dl-grid">
              {library.map((item) => {
                const pct =
                  item.totalChapters > 0
                    ? Math.round((item.downloaded / item.totalChapters) * 100)
                    : 100;
                return (
                  <article className="dl-card" key={item.dir}>
                    {item.coverToken ? (
                      <img className="thumb" src={coverSrc(item.coverToken)} alt="" loading="lazy" />
                    ) : (
                      // Pas de couverture enregistrée (téléchargements
                      // antérieurs à la colonne `coverUrl` de meta.json) : une
                      // pastille d'initiale vaut mieux qu'un rectangle vide, qui
                      // se lit comme une image cassée.
                      <span className="thumb thumb-empty" aria-hidden="true">
                        {(item.title.trim()[0] ?? '?').toUpperCase()}
                      </span>
                    )}
                    <div className="dl-body">
                      <h3 className="dl-title">{item.title}</h3>
                      <span className="muted dl-meta">
                        {item.source} ·{' '}
                        {item.totalChapters > 0
                          ? t('telechargements.chapitres.compte', { n: item.downloaded, total: item.totalChapters })
                          : `${item.downloaded} chapitre${item.downloaded > 1 ? 's' : ''}`}{' '}
                        · {formatBytes(item.sizeBytes)}
                      </span>
                      <div className="progress">
                        <div className="progress-bar" style={{ width: `${pct}%` }} />
                      </div>
                      <span className={`badge ${item.complete ? 'state-termine' : 'state-attente'}`}>
                        {item.complete ? 'Terminé' : 'Partiel'}
                      </span>
                      <div className="dl-actions">
                        {/* Tâche 2.3 : un `.cbz` téléchargé doit se lire DANS
                            l'application. Avant, il fallait l'ouvrir dans un autre
                            logiciel — le lecteur passait toujours par le moteur. */}
                        {item.chapters.length > 0 && (
                          <>
                            <button
                              className="btn btn-primary"
                              onClick={() => void openLocalChapter(item, item.chapters[0])}
                            >
                              {t('telechargements.lecture.lancer')}
                            </button>
                            {item.chapters.length > 1 && (
                              <button
                                className="btn"
                                onClick={() => setChapitresVisibles(item)}
                                aria-label={t('telechargements.chapitres.ouvrir')}
                              >
                                {t('telechargements.chapitres.ouvrir')} ({item.chapters.length})
                              </button>
                            )}
                          </>
                        )}
                        {!item.complete && item.downloaded > 0 && (
                          <button
                            className="btn"
                            disabled={queueRunning}
                            onClick={() => void resumeManga(item)}
                          >
                            {t('telechargements.file.reprendre')}
                          </button>
                        )}
                        <button className="btn btn-ghost" onClick={() => setDeleteTarget(item)}>
                          Supprimer
                        </button>
                      </div>
                    </div>
                  </article>
                );
              })}
            </div>
          </>
        )}

        {deleteTarget && (
          <dialog
            className="confirm"
            open
            aria-labelledby="confirm-titre"
            onCancel={() => setDeleteTarget(null)}
          >
            <h3 id="confirm-titre">
              {t('telechargements.supprimer.titre', { nom: deleteTarget.title })}
            </h3>
            <p className="muted">
              {t('telechargements.supprimer.note', {
                n: deleteTarget.downloaded,
                poids: formatBytes(deleteTarget.sizeBytes),
              })}
            </p>
            <div className="alert-actions">
              <button
                className="btn btn-danger"
                onClick={() => {
                  const target = deleteTarget;
                  setDeleteTarget(null);
                  void removeDownload(target);
                }}
              >
                {t('telechargements.supprimer.definitif')}
              </button>
              <button className="btn btn-ghost" onClick={() => setDeleteTarget(null)}>
                {t('telechargements.annuler')}
              </button>
            </div>
          </dialog>
        )}
      </main>
    );
  }

  // ---------- Vue : lecteur ----------
if (reader) {
    const { pages, index, chapter } = reader;
  const webtoon = readerMode === 'webtoon';
    const page = pages[index];
    // Signet présent sur la page affichée : l'état du bouton du lecteur.
    const marque = pageMarquee();
    const by = readerMode === 'page' && spread ? 2 : 1;
    // Précharge les pages suivantes, pas seulement la première.
    //
    // Mesuré le 2026-10-05 : la balise unique d'avant rendait la page suivante
    // bien plus vite (elle arrivait depuis le cache disque Rust alimenté par le
    // protocole), mais elle ne couvrait qu'**une** page — donc un cran de
    // navigation sur deux repartait sur le réseau. On couvre désormais
    // `PRECHARGE_PAGES` pages d'avance.
    const aPrecharger = pages.slice(index + by, index + by + PRECHARGE_PAGES);
    // En double page, la page courante et la suivante partagent le cadre. En RTL
    // (défaut manga) la page courante est à droite : l'ordre DOM est donc
    // inversé, et il ne reste qu'une page à la dernière.
    const visible: Page[] = [];
    if (readerMode === 'page' && spread) {
      const second = pages[index + 1];
      if (page && second) visible.push(...(readerDir === 'rtl' ? [second, page] : [page, second]));
      else if (page) visible.push(page);
    } else if (page) {
      visible.push(page);
    }
    return (
      <main className={`reader${readerMode === 'webtoon' ? ' pleine-largeur' : ''}`}>
        <header className="reader-bar">
          <button className="btn btn-ghost" onClick={closeReader}>
            {'←'} Chapitres
          </button>
          <span className="reader-title">
            {reader.mangaTitre || openManga?.title} · {chapterLabel(chapter)}
          </span>

          {/* Sans cette mention, impossible de savoir si l'affichage vient du
              disque ou du site — donc si une coupure réseau cassera la lecture.
              Cette information change tout pour l'utilisateur. */}
          {reader.horsLigne && (
            <span className="reader-offline" title={t('telechargements.lecture.horsligne')}>
              {t('lecteur.horsligne')}
            </span>
          )}

          <div className="reader-tools">
            <button
              className={`seg-btn${marque ? ' on' : ''}`}
              aria-pressed={marque}
              onClick={() => void basculerSignet()}
              title="Marquer cette page pour la retrouver plus tard"
            >
              {marque ? t('lecteur.signet.actif') : t('lecteur.signet')}
            </button>

            {/* Navigation du bandeau, dans la barre du lecteur (demande
                explicite). En mode webtoon, la navigation vivait sous la
                barre : changer de page imposait de remonter en haut de
                l'écran, soit parcourir 650 000 px sur un chapitre de 303
                pages. */}
            <div className="seg" role="group" aria-label={t('lecteur.navigation')}>
              <button
                className="seg-btn"
                disabled={index === 0}
                onClick={() => (webtoon ? scrollToPage(index - 1) : turn(-1))}
              >
                {'←'}
              </button>
              <button
                className="seg-btn"
                disabled={index >= pages.length - 1}
                onClick={() => (webtoon ? scrollToPage(index + 1) : turn(1))}
              >
                {'→'}
              </button>
            </div>

            <button
              className={`seg-btn solo${pleinEcran ? ' on' : ''}`}
              aria-pressed={pleinEcran}
              onClick={() => void basculerPleinEcran()}
              title={t('lecteur.plein.ecran')}
            >
              {pleinEcran ? '⤡' : '⤢'}
            </button>

            <div className="seg" role="group" aria-label="Mode de lecture">
              <button
                className="seg-btn"
                aria-pressed={readerMode === 'page'}
                onClick={() => switchMode('page')}
              >
                Pagé
              </button>
              <button
                className="seg-btn"
                aria-pressed={readerMode === 'webtoon'}
                onClick={() => switchMode('webtoon')}
              >
                Webtoon
              </button>
            </div>

            {readerMode === 'page' && (
              <>
                <div className="seg" role="group" aria-label="Pages par écran">
                  <button
                    className="seg-btn"
                    aria-pressed={!spread}
                    onClick={() => {
                      if (spread) toggleSpread();
                    }}
                  >
                    Simple
                  </button>
                  <button
                    className="seg-btn"
                    aria-pressed={spread}
                    onClick={() => {
                      if (!spread) toggleSpread();
                    }}
                  >
                    Double
                  </button>
                </div>

                <div className="seg" role="group" aria-label="Ajustement de la page">
                  <button
                    className="seg-btn"
                    aria-pressed={fit === 'width'}
                    onClick={() => switchFit('width')}
                  >
                    Largeur
                  </button>
                  <button
                    className="seg-btn"
                    aria-pressed={fit === 'height'}
                    onClick={() => switchFit('height')}
                  >
                    Hauteur
                  </button>
                </div>

                {spread && (
                  <button
                    className="seg-btn solo"
                    onClick={switchDir}
                    title="Inverser l'ordre des deux pages"
                    aria-label={
                      readerDir === 'rtl'
                        ? 'Sens de lecture droite à gauche, inverser'
                        : 'Sens de lecture gauche à droite, inverser'
                    }
                  >
                    {readerDir === 'rtl' ? 'RTL' : 'LTR'}
                  </button>
                )}
              </>
            )}

            <label className="zoom">
              <span className="muted">{zoom}%</span>
              <input
                type="range"
                min={50}
                max={200}
                step={10}
                value={zoom}
                onChange={(e) => changeZoom(Number(e.target.value))}
                onWheel={(e) => {
                  if (!e.ctrlKey) return;
                  e.preventDefault();
                  changeZoom(zoom + (e.deltaY < 0 ? 10 : -10));
                }}
                aria-label="Largeur de la page en pourcentage"
              />
            </label>
            {chapters.length > 0 && (
              <select
                className="chapter-select"
                aria-label="Changer de chapitre"
                value={chapter.url}
                disabled={readerLoading}
                onChange={(e) => {
                  const target = chapters.find((c) => c.url === e.target.value);
                  if (target) void openReader(target);
                }}
              >
                {chapters.map((c) => (
                  <option key={c.url} value={c.url}>
                    {chapterLabel(c)}
                  </option>
                ))}
              </select>
            )}
            <form
              className="page-jump"
              onSubmit={(e) => {
                e.preventDefault();
                jumpToPage(pageJump);
              }}
            >
              <label className="page-jump-label" htmlFor="reader-page">
                Page
              </label>
              <input
                id="reader-page"
                type="number"
                inputMode="numeric"
                min={1}
                max={pages.length}
                placeholder={String(index + 1)}
                value={pageJump}
                onChange={(e) => setPageJump(e.target.value)}
                // La page affichée est le `placeholder` ; le champ ne sert qu'à
                // saisir une destination. Sans cette remise à zéro, une valeur
                // saisie puis validée restait affichée pendant que le lecteur
                // continuait à tourner : on lisait « 50 » en étant page 4
                // (mesuré), donc la reprise paraissait ne pas reprendre.
                onBlur={() => setPageJump('')}
                aria-label="Aller à la page"
              />
              <span className="muted">/ {pages.length}</span>
            </form>
          </div>
        </header>

        {readerMode === 'webtoon' ? (
          <div className="webtoon" ref={stripRef} onScroll={onScrollBande}>
            {/* Tâche 1.9 — virtualisation. Mesuré : Killer Peter = 303 pages
                dans UN `.cbz` ; rendues toutes, le navigateur chargeait les
                303 images à l'ouverture (211 requêtes `tatami://`).

                Rust renvoie la **hauteur réelle** de chaque page (lue dans
                l'en-tête), donc la bande a une hauteur exacte dès le premier
                rendu et une fenêtre glissante peut ne monter qu'une dizaine de
                nœuds. Une hauteur estimée avait été essayée et retirée : la
                position de défilement reculait de 3 964 à 1 788 px. */}
            {/* Sans dimension connue, on rend toutes les pages en flux normal :
                `height: 0` + `position: absolute` + `top: 0` empilait les
                77 pages l'une sur l'autre (mesuré). Le conteneur doit alors
                laisser sa hauteur au contenu, jamais la forcer à zéro. */}
            <div
              className="webtoon-inner"
              style={bandeVirtualisable ? { height: bandeHauteur } : undefined}
            >
              {fenetre.map((i) => {
                const p = pages[i];
                if (!p) return null;
                const hauteur = hauteurs[i] || 0;
                return (
                  <img
                    key={p.token}
                    ref={(el) => {
                      stripImages.current[i] = el;
                    }}
                    src={pageSrc(p.token)}
                    alt={`Page ${i + 1}`}
                    loading="lazy"
                    decoding="async"
                    data-index={i}
                    style={
                      bandeVirtualisable
                        ? {
                            width: `${zoom}%`,
                            position: 'absolute',
                            top: offsets[i] ?? 0,
                            height: hauteur ? `${hauteur}px` : undefined,
                          }
                        : { width: `${zoom}%` }
                    }
                    onDoubleClick={() => changeZoom(zoom >= 200 ? 100 : 200)}
                  />
                );
              })}
            </div>
          </div>
        ) : (
          <>
            <div
              className={`reader-stage fit-${fit}${spread ? ' spread' : ''}`}
              onWheel={onMoletteStage}
            >
              {visible.length > 0 ? (
                visible.map((p) => {
                  const at = pages.indexOf(p);
                  return (
                    <img
                      key={p.token}
                      src={pageSrc(p.token)}
                      alt={`Page ${at + 1} sur ${pages.length}`}
                    />
                  );
                })
              ) : (
                <p className="muted">{t('erreur.page')}</p>
              )}
            </div>
            <div className="reader-nav">
              <button className="btn" disabled={index === 0} onClick={() => turn(-by)}>
                Page précédente
              </button>
              <button
                className="btn btn-primary"
                disabled={index >= pages.length - 1}
                onClick={() => turn(by)}
              >
                Page suivante
              </button>
            </div>
          </>
        )}

        {readerMode === 'page' &&
          aPrecharger.map((p) => (
            <img key={p.token} className="preload" src={pageSrc(p.token)} alt="" aria-hidden="true" />
          ))}
      </main>
    );
  }

  // ---------- Vue : chapitres ----------
  if (openManga) {
    const pct = progress && progress.total > 0 ? Math.round((progress.page / progress.total) * 100) : 0;
    // `info` peut être en retard sur les chapitres : on affiche ce qu'on a.
    const fiche = info ?? openManga;
    const rows = infoRows(info);
    const summary = plainText(info?.description);
    return (
      <main>
        <AppBar tab={tab} onTab={goTab} />{renderSuperpositions()}
        <button className="crumb" onClick={backToSource}>
          {'←'} {selected?.name}
        </button>
        <header className="page-head">
          <h1>{openManga.title}</h1>
          <span className="sub">{selected?.name}</span>
          <button
            className={`btn fav-toggle${isFavorite(fiche) ? ' on' : ''}`}
            aria-pressed={isFavorite(fiche)}
            onClick={() => void toggleFavorite(fiche)}
          >
            {isFavorite(fiche) ? t('fiche.dans.favoris') : t('fiche.ajouter.favori')}
          </button>
        </header>

        {/* La catégorie n'a de sens que si le titre est déjà favori : on ne
            propose pas de classer ce qu'on n'a pas choisi de garder. */}
        {isFavorite(fiche) && (
          <div className="cat-pick">
            <label className="field" htmlFor="fiche-categorie">
              Catégorie
              <select
                id="fiche-categorie"
                value={favoriDe(fiche)?.category_id ?? ''}
                onChange={(e) => {
                  const v = e.target.value;
                  void choisirCategorie(fiche, v === '' ? null : Number(v));
                }}
              >
                <option value="">{t('fiche.sans.categorie')}</option>
                {categories.map((c) => (
                  <option key={c.id} value={c.id}>
                    {c.name}
                  </option>
                ))}
              </select>
            </label>
            {categories.length === 0 && (
              <p className="muted">
                Créez vos catégories dans Réglages pour ranger vos favoris.
              </p>
            )}
          </div>
        )}

        {(summary || rows.length > 0 || fiche.coverUrl) && (
          <section className="sheet">
            {fiche.coverToken && (
              <img className="sheet-cover" src={coverSrc(fiche.coverToken)} alt="" loading="lazy" />
            )}
            <div className="sheet-body">
              {summary && <p className="sheet-desc">{summary}</p>}
              {rows.length > 0 && (
                <dl className="info-table">
                  {rows.map((r) => (
                    <div className="info-row" key={r.label}>
                      <dt>{r.label}</dt>
                      <dd>{r.value}</dd>
                    </div>
                  ))}
                </dl>
              )}
            </div>
          </section>
        )}

        {error && <p className="error">{error}</p>}

        {chapters.length > 0 && (
          <p className="count">
            {chapters.length} chapitre{chapters.length > 1 ? 's' : ''} · {checked.size} sélectionné
            {checked.size > 1 ? 's' : ''}
          </p>
        )}

        <div className="dl-bar">
          <button
            className="btn btn-primary"
            disabled={downloading || checked.size === 0}
            onClick={downloadSelected}
          >
            {downloading ? 'Téléchargement…' : `Télécharger la sélection (${checked.size})`}
          </button>
          <div className="pick-bar" role="group" aria-label="Sélection rapide des chapitres">
            <span className="pick-label">
              {chapterQuery ? 'Sur les chapitres filtrés' : 'Choisir'}
            </span>
            <input
              type="number"
              min={1}
              max={Math.max(1, filteredChapters.length)}
              value={pickCount}
              onChange={(e) => setPickCount(Number(e.target.value))}
              aria-label={`Nombre de chapitres à sélectionner (${pickCount})`}
            />
            <button className="btn btn-ghost" onClick={() => pickChapters('last')}>
              N derniers
            </button>
            <button className="btn btn-ghost" onClick={() => pickChapters('first')}>
              N premiers
            </button>
            <button className="btn btn-ghost" onClick={() => pickChapters('all')}>
              Tout
            </button>
            <button className="btn btn-ghost" onClick={() => pickChapters('none')}>
              Aucun
            </button>
          </div>
          <input
            className="search chapter-search"
            type="search"
            aria-label="Filtrer les chapitres par numéro ou titre"
            placeholder="Filtrer un chapitre…"
            value={chapterQuery}
            onChange={(e) => setChapterQuery(e.target.value)}
          />
        </div>

        {progress && (
          <div className="progress" role="status">
            <div className="progress-bar" style={{ width: `${progress.done ? 100 : pct}%` }} />
            <span>
              {progress.done
                ? 'Terminé'
                : progress.total > 0
                  ? `${progress.chapter} — page ${progress.page}/${progress.total} (${pct}%)`
                  : `${progress.chapter} — préparation…`}
            </span>
          </div>
        )}

        {loading && chapters.length === 0 ? (
          <div className="chapter-grid" aria-busy="true" aria-label="Chargement des chapitres">
            {Array.from({ length: 15 }).map((_, i) => (
              <div className="chapter-cell skeleton-row" key={i} />
            ))}
          </div>
        ) : (
          <div className="chapter-grid">
            {filteredChapters.slice(0, chapterLimit).map((c) => {
              const date = chapterDate(c);
              const on = checked.has(c.url);
              return (
                <div className={`chapter-cell${on ? ' on' : ''}`} key={c.url}>
                  <label className="chapter-check">
                    <input
                      type="checkbox"
                      checked={on}
                      onChange={() => toggleChapter(c.url)}
                      aria-label={`Sélectionner ${chapterLabel(c)}`}
                    />
                  </label>
                  <button
                    className="chapter-open"
                    disabled={readerLoading}
                    onClick={() => void openReader(c)}
                    aria-label={`Lire ${chapterLabel(c)}`}
                  >
                    <span className="chapter-num">{chapterShort(c)}</span>
                    <span className="chapter-sub">{date ?? chapterLabel(c)}</span>
                  </button>
                  <button
                    className="chapter-dl"
                    disabled={downloading}
                    onClick={() => void downloadChapter(c)}
                    aria-label={`Télécharger ${chapterLabel(c)}`}
                    title="Télécharger"
                  >
                    ↓
                  </button>
                </div>
              );
            })}
          </div>
        )}

        {!loading && chapters.length > 0 && filteredChapters.length === 0 && (
          <p className="muted">Aucun chapitre ne correspond à « {chapterQuery} ».</p>
        )}

        {!loading && chapters.length === 0 && !error && (
          <p className="muted">{t('fiche.chapitres.vide')}</p>
        )}

        {filteredChapters.length > chapterLimit && (
          <button
            className="btn btn-more"
            onClick={() => setChapterLimit((n) => n + CHAPTER_STEP)}
          >
            Afficher plus ({filteredChapters.length - chapterLimit} restants)
          </button>
        )}
      </main>
    );
  }

  // ---------- Vue : webtoons d'une source ----------
  if (selected) {
    return (
      <main>
        <AppBar tab={tab} onTab={goTab} />{renderSuperpositions()}
        <button className="crumb" onClick={backToCatalog}>
          {'←'} Catalogues
        </button>
        <header className="page-head">
          <h1>{selected.name}</h1>
          <span className="sub">{selected.lang.toUpperCase()}</span>
        </header>

        <div className="source-search">
          <input
            className="search"
            type="search"
            aria-label="Rechercher un webtoon dans cette source"
            placeholder="Rechercher un webtoon…"
            value={mangaQuery}
            onChange={(e) => setMangaQuery(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') void searchInSource();
            }}
          />
          <button className="btn" onClick={searchInSource} disabled={loading}>
            Rechercher
          </button>
        </div>

        {tags.length > 0 && (
          <div className="chips" role="group" aria-label="Genres">
            {tags.map((t) => (
              <button
                key={t.key}
                className="chip"
                aria-pressed={activeTags.includes(t.key)}
                onClick={() => toggleTag(t.key)}
              >
                {t.title}
              </button>
            ))}
            {activeTags.length > 0 && (
              <button className="chip chip-clear" onClick={clearTags}>
                Effacer ({activeTags.length})
              </button>
            )}
            {tagsTotal > tags.length && (
              <span className="muted chips-note">{tagsTotal} genres au total</span>
            )}
          </div>
        )}

        <div className="select-bar">
          {selectMode ? (
            <>
              <span className="muted">
                {picked.size} webtoon{picked.size > 1 ? 's' : ''} sélectionné
                {picked.size > 1 ? 's' : ''}
              </span>
              <div className="select-actions">
                <button
                  className="btn btn-ghost"
                  onClick={() => setPicked(new Set(latest.map(mangaKey)))}
                >
                  Tout
                </button>
                <button className="btn btn-ghost" onClick={leaveSelectMode}>
                  Annuler
                </button>
                <button
                  className="btn btn-primary"
                  disabled={picked.size === 0 || queueRunning}
                  onClick={startQueue}
                >
                  {queueRunning ? 'Téléchargement…' : `Télécharger (${picked.size})`}
                </button>
              </div>
            </>
          ) : (
            <>
              <span className="muted">{t('source.chapitres.vide')}</span>
              <div className="select-actions">
                <label className="recent-toggle">
                  <input
                    type="checkbox"
                    checked={showRecent}
                    onChange={(e) => setShowRecent(e.target.checked)}
                  />
                  <span>{t('source.chapitres.recents')}</span>
                  <span className="muted">
                    {recentBusy > 0
                      ? `${recentBusy} en cours…`
                      : `${Object.keys(cardRecent).length}/${latest.length}`}
                  </span>
                </label>
                <button className="btn" onClick={() => setSelectMode(true)} disabled={latest.length === 0}>
                  Sélectionner
                </button>
              </div>
            </>
          )}
        </div>

        {queueRunning && (
          <div className="progress" role="status">
            <div
              className="progress-bar"
              style={{
                width: `${
                  progress && progress.total > 0
                    ? Math.round((progress.page / progress.total) * 100)
                    : 15
                }%`,
              }}
            />
            <span>
              {queue.find((q) => q.state === 'téléchargement')?.title ?? 'File en cours'}
              {progress ? ` — ${progress.chapter} ${progress.page}/${progress.total || '?'}` : ''}
            </span>
          </div>
        )}

        {error && selected && (
          <div className="alert">
            <p>{describeError(error, selected.name, t)}</p>
            {challenge && (
              <p className="muted">
                Ce site demande une vérification anti-robot. Résous-la dans la fenêtre
                qui va s&apos;ouvrir, puis reviens ici : les cookies seront transmis au
                moteur et la requête rejouée.
              </p>
            )}
            <div className="alert-actions">
              {challenge ? (
                <>
                  <button
                    className="btn btn-primary"
                    disabled={challengeBusy}
                    onClick={() => void startChallenge()}
                  >
                    Résoudre la vérification
                  </button>
                  <button
                    className="btn"
                    disabled={challengeBusy}
                    onClick={() => void applyChallenge()}
                  >
                    C&apos;est fait, réessayer
                  </button>
                  <button className="btn btn-ghost" onClick={() => void cancelChallenge()}>
                    Annuler
                  </button>
                </>
              ) : (
                <>
                  <button
                    className="btn"
                    disabled={loading}
                    onClick={() => loadSource(selected, 0)}
                  >
                    Réessayer
                  </button>
                  <button className="btn btn-ghost" onClick={backToCatalog}>
                    Changer de catalogue
                  </button>
                </>
              )}
            </div>
          </div>
        )}
          {latest.length > 0 && (
          <div className="list-bar">
            <p className="count">
              {latest.length} webtoon{latest.length > 1 ? 's' : ''}
              {loading ? ' — chargement…' : ''}
            </p>
            <div className="chips" role="group" aria-label="Tri des titres">
              {TRIS.map((option) => (
                <button
                  key={option.value}
                  className="chip"
                  aria-pressed={tri === option.value}
                  onClick={() => setTri(option.value)}
                >
                  {t(option.cle)}
                </button>
              ))}
            </div>
          </div>
        )}

        {loading && latest.length === 0 ? (
          <div className="grid">
            {Array.from({ length: 8 }).map((_, i) => (
              <div className="skeleton" key={i} />
            ))}
          </div>
        ) : (
          <div className="manga-grid">
            {latestTrie.map((m) => {
              const key = mangaKey(m);
              const isPicked = picked.has(key);
              return (
                <div
                  key={key}
                  className={`manga${selectMode ? ' pickable' : ''}${isPicked ? ' picked' : ''}`}
                >
                  {/* La carte est un `div` et non un `button` : le bouton favori
                      est un frère, pas un enfant (un bouton dans un bouton est
                      du HTML invalide et casse l'hydratation). */}
                  <button
                    className="manga-open"
                    aria-pressed={selectMode ? isPicked : undefined}
                    onClick={() => (selectMode ? togglePick(m) : loadChapters(m))}
                  >
                    <span className="cover">
                      {m.coverToken ? (
                        <img className="thumb" src={coverSrc(m.coverToken)} alt="" loading="lazy" />
                      ) : (
                        <span className="thumb thumb-empty" aria-hidden="true">
                          {(m.title.trim()[0] ?? '?').toUpperCase()}
                        </span>
                      )}
                      {cardTotal[key] > 0 && (
                        <span className="card-count" title={t('telechargements.chapitres.compte', { n: cardTotal[key], total: cardTotal[key] })}>
                          {cardTotal[key]} ch.
                        </span>
                      )}
                    </span>
                    <span className="label">{m.title}</span>
                    {showRecent && cardRecent[key] && cardRecent[key].length > 0 && (
                      <span className="recent">
                        {cardRecent[key].map((line) => (
                          <span className="recent-line" key={line}>
                            {line}
                          </span>
                        ))}
                      </span>
                    )}
                  </button>
                  <button
                    className={`fav-btn${isFavorite(m) ? ' on' : ''}`}
                    aria-pressed={isFavorite(m)}
                    aria-label={
                      isFavorite(m)
                        ? `Retirer ${m.title} des favoris`
                        : `Ajouter ${m.title} aux favoris`
                    }
                    title={isFavorite(m) ? 'Retirer des favoris' : 'Ajouter aux favoris'}
                    onClick={() => void toggleFavorite(m)}
                  >
                    ♥
                  </button>
                  {selectMode && (
                    <span className="pick" aria-hidden="true">
                      {isPicked ? '✓' : ''}
                    </span>
                  )}
                </div>
              );
            })}
          </div>
        )}

        {latest.length === 0 && !loading && !error && (
          <p className="muted">{t('source.vide')}</p>
        )}

        {onePageOnly && (
          <p className="muted note">
            {selected?.name} ne renvoie qu&apos;une seule page : son catalogue est plus
            court qu&apos;il n&apos;y paraît (limite du site, pas de l&apos;application).
          </p>
        )}

        {hasMore && !loading && (
          <button
            className="btn btn-more"
            onClick={() => loadSource(selected, latestOffset, { tags: activeTags, query: mangaQuery })}
          >
            Voir plus ({latest.length} chargés)
          </button>
        )}
      </main>
    );
  }

// ---------- Vue : accueil ----------
  // Reprise de lecture, favoris, mode incognito.
  if (tab === 'home' && !reader) {
    return (
      <main>
        <AppBar tab={tab} onTab={goTab} />{renderSuperpositions()}
        <header className="page-head">
          <h1>{t('nav.accueil')}</h1>
          <span className="sub">
            {t('accueil.lu.recemment', { n: recent.length })} ·{' '}
            {t('accueil.favori', {
              n: favorites.length,
              s: favorites.length > 1 ? 's' : '',
            })}
          </span>
        </header>

        {error && <p className="error">{error}</p>}

        <div className="band">
          {t('accueil.continuer')}
          <label
            className="recent-toggle"
            title={t('accueil.incognito.note')}
          >
            <input
              type="checkbox"
              checked={incognito}
              onChange={(e) => {
                const hide = e.target.checked;
                setIncognito(hide);
                window.localStorage.setItem(INCOGNITO_KEY, hide ? '1' : '0');
                void refreshShelf(hide);
              }}
            />
            <span>{t('accueil.incognito')}</span>
          </label>
        </div>

        {recent.length === 0 ? (
          <p className="muted">{t('accueil.rien.a.reprendre')}</p>
        ) : (
          <div className="dl-grid">
            {recent.map((h) => (
              <article className="dl-card ouvrable" key={`${h.source}|${h.manga_url}`}>
                {h.cover_token ? (
                  <img className="thumb" src={coverSrc(h.cover_token)} alt="" loading="lazy" />
                ) : (
                  <span className="thumb thumb-empty" aria-hidden="true">
                    {(h.title.trim()[0] ?? '?').toUpperCase()}
                  </span>
                )}
                <div className="dl-body">
                  <h3 className="dl-title">{h.title}</h3>
                  <span className="muted dl-meta">
                    {h.source}
                    {h.chapter_title ? ` · ${h.chapter_title}` : ''}
                  </span>
                  <span className="muted dl-meta">
                    {t('accueil.page.sur', {
                      p: h.page_index + 1,
                      n: h.page_count || '?',
                      quand: depuis(t, h.read_at),
                    })}
                  </span>
                  <div className="dl-actions">
                    <button
                      className="btn btn-primary"
                      disabled={readerLoading}
                      onClick={() => void resumeReading(h)}
                    >
                      {readerLoading ? t('accueil.ouverture') : t('accueil.reprendre')}
                    </button>
                    <button
                      className="btn btn-ghost"
                      aria-label={t('accueil.oublier.titre', { titre: h.title })}
                      title={t('accueil.oublier.titre', { titre: h.title })}
                      onClick={() => {
                        void invoke('forget_history', {
                          source: h.source,
                          mangaUrl: h.manga_url,
                        })
                          .then(() => refreshShelf())
                          .catch((e) => setError(t('erreur.purge', { raison: String(e) })));
                      }}
                    >
                      {t('accueil.oublier')}
                    </button>
                  </div>
                </div>
                {/* La carte ouvre la fiche. Ce bouton est un FRÈRE du contenu, en
                    surimpression : un `<button>` dans un `<button>` serait du HTML
                    invalide, et un lecteur d'écran n'y verrait qu'un seul nom. Les
                    actions restent au-dessus (`.dl-actions`) et cliquables. */}
                <button
                  className="dl-open"
                  aria-label={t('accueil.fiche.titre', { titre: h.title })}
                  title={t('accueil.fiche.titre', { titre: h.title })}
                  onClick={() => void loadChapters(mangaDepuisHistorique(h))}
                />
              </article>
            ))}
          </div>
        )}

        {signets.length > 0 && (
          <>
            <div className="band">
              {t('accueil.vos.signets')}
              <span className="muted">
                {t('accueil.pages.marquees', {
                  n: signets.length,
                  s: signets.length > 1 ? 's' : '',
                })}
              </span>
            </div>
            <div className="bm-list">
              {signets.map((b) => (
                <div
                  className="bm"
                  key={`${b.source}|${b.manga_url}|${b.chapter_url}|${b.page_index}`}
                >
                  <span className="bm-cover">
                    {b.cover_token ? (
                      <img className="thumb" src={coverSrc(b.cover_token)} alt="" loading="lazy" />
                    ) : (
                      <span className="thumb thumb-empty" aria-hidden="true">
                        {(b.title.trim()[0] ?? '?').toUpperCase()}
                      </span>
                    )}
                  </span>
                  <span className="bm-text">
                    <b>{b.title}</b>
                    <span className="muted">
                      {b.source}
                      {b.chapter_title ? ` · ${b.chapter_title}` : ''} ·{' '}
                      {t('accueil.page.sur', {
                        p: b.page_index + 1,
                        n: b.page_count,
                        quand: depuis(t, b.created_at),
                      })}
                    </span>
                  </span>
                  <button className="btn" onClick={() => void ouvrirSignet(b)}>
                    {t('accueil.rouvrir')}
                  </button>
                  <button
                    className="btn btn-ghost"
                    onClick={() => {
                      const avant = signets;
                      setSignets(avant.filter((x) => !sameSignet(x, b) || x.chapter_url !== b.chapter_url || x.page_index !== b.page_index || x.source !== b.source));
                      void invoke('remove_bookmark', {
                        source: b.source,
                        mangaUrl: b.manga_url,
                        chapterUrl: b.chapter_url,
                        pageIndex: b.page_index,
                      })
                        .then(() => refreshSignets())
                        .catch((e) => {
                          setSignets(avant);
                          setError(t('erreur.signet', { raison: String(e) }));
                        });
                    }}
                  >
                    {t('accueil.retirer')}
                  </button>
                </div>
              ))}
            </div>
          </>
        )}

        <div className="band">
          {t('accueil.vos.favoris')}
          {categories.length > 0 && favorites.length > 0 && (
            <span className="chips" role="group" aria-label={t('reglages.categories')}>
              <button
                className="chip"
                aria-pressed={filtreCategorie === 'all'}
                onClick={() => setFiltreCategorie('all')}
              >
                {t('accueil.filtre.toutes')}
              </button>
              {categories.map((c) => (
                <button
                  key={c.id}
                  className="chip"
                  aria-pressed={filtreCategorie === c.id}
                  onClick={() => setFiltreCategorie(c.id)}
                >
                  <span
                    className="cat-dot"
                    style={{ background: couleurCategorie(c.color) }}
                    aria-hidden="true"
                  />
                  {c.name}
                  {' ('}
                  {favorites.filter((f) => f.category_id === c.id).length})
                </button>
              ))}
              <button
                className="chip"
                aria-pressed={filtreCategorie === 'aucune'}
                onClick={() => setFiltreCategorie('aucune')}
              >
                {t('accueil.filtre.sans', {
                  n: favorites.filter((f) => f.category_id == null).length,
                })}
              </button>
            </span>
          )}
          {favorites.length > 0 && (
            <button
              className="btn btn-ghost"
              onClick={() => {
                void invoke('clear_history')
                  .then(() => refreshShelf())
                  .catch((e) => setError(t('erreur.purge', { raison: String(e) })));
              }}
            >
              {t('accueil.vider.historique')}
            </button>
          )}
        </div>

        {favorites.length === 0 ? (
          <p className="muted">{t('accueil.favoris.vide')}</p>
        ) : favorisVisibles.length === 0 ? (
          <p className="muted">{t('accueil.favoris.filtre.vide')}</p>
        ) : (
          <div className="manga-grid">
            {favorisVisibles.map((f) => {
              const m: Manga = {
                url: f.manga_url,
                title: f.title,
                source: f.source,
                coverUrl: f.cover_url,
                coverToken: f.cover_token,
                // Le classement voyage avec le favori : sans lui, ouvrir depuis
                // l'accueil n'aurait aucun contexte et l'incognito ne pourrait
                // rien masquer.
                contentRating: f.content_rating,
              };
              return (
                <div className="manga" key={`${f.source}|${f.manga_url}`}>
                  <button
                    className="manga-open"
                    onClick={() => void loadChapters(m)}
                    aria-label={`Ouvrir ${f.title}`}
                  >
                    <span className="cover">
                      {f.cover_token ? (
                        <img className="thumb" src={coverSrc(f.cover_token)} alt="" loading="lazy" />
                      ) : (
                        <span className="thumb thumb-empty" aria-hidden="true">
                          {(f.title.trim()[0] ?? '?').toUpperCase()}
                        </span>
                      )}
                      {f.category_id != null && (
                        <span
                          className="cat-dot on-cover"
                          style={{ background: couleurCategorie(f.category_color) }}
                          title={
                            categories.find((c) => c.id === f.category_id)?.name ?? 'Catégorie'
                          }
                          aria-hidden="true"
                        />
                      )}
                    </span>
                    <span className="label">{f.title}</span>
                    <span className="recent-line fav-src">{f.source}</span>
                  </button>
                  <button
                    className="fav-remove"
                    aria-label={`Retirer ${f.title} des favoris`}
                    onClick={() => void toggleFavorite(m)}
                  >
                    ♥
                  </button>
                </div>
              );
            })}
          </div>
        )}
      </main>
    );
  }

  // ---------- Vue : catalogue ----------
  return (
    <main>
      <AppBar tab={tab} onTab={goTab} />{renderSuperpositions()}

      <header className="page-head">
        <h1>{t('nav.catalogues')}</h1>
        <span className="sub">
          {t('catalogue.sources.disponibles', { n: visibleSources.length })}
          {sourcesOff.length > 0
            ? ` · ${t('catalogue.sources.desactivees', { n: sourcesOff.length })}`
            : ''}
        </span>
      </header>

      <input
        className="search"
        type="search"
        aria-label={t('catalogue.rechercher')}
        placeholder={t('catalogue.rechercher')}
        value={nameFilter}
        onChange={(e) => {
          setNameFilter(e.target.value);
          setShownSources(48);
        }}
      />

      <div className="chips" role="group" aria-label={t('reglages.langue')}>
        {langs.map((code) => (
          <button
            key={code}
            className="chip"
            onClick={() => {
              setLang(code);
              setShownSources(48);
            }}
            aria-pressed={code === lang}
          >
            {code === 'all' ? t('catalogue.langue.toutes') : code.toUpperCase()}
          </button>
        ))}
      </div>

      {error && <p className="error">{error}</p>}

      {sourcesOff.length > 0 && (
        <div className="row-actions">
          <label
            className="recent-toggle"
            title={t('catalogue.desactivees.note')}
          >
            <input
              type="checkbox"
              checked={voirDesactives}
              onChange={(e) => {
                setVoirDesactives(e.target.checked);
                setShownSources(48);
              }}
            />
            <span>
              {t('catalogue.desactivees.afficher', {
                n: sourcesOff.length,
                s: sourcesOff.length > 1 ? 's' : '',
              })}
            </span>
          </label>
          <button className="btn btn-ghost" onClick={() => void reactiverToutes()}>
            {t('catalogue.reactiver.tout')}
          </button>
        </div>
      )}

      {enTiles ? (
        <>
          <p className="count">
            {t('catalogue.compte.langues', {
              n: visibleSources.length,
              langs: langCounts.length,
            })}
          </p>
          <div className="lang-grid">
            {langCounts.map(([code, n]) => (
              <button
                key={code}
                className="lang-tile"
                onClick={() => {
                  setLang(code);
                  setShownSources(48);
                }}
              >
                <b>{code.toUpperCase()}</b>
                <span className="lang-nom">{NOMS_LANGUE[code] ?? code.toUpperCase()}</span>
                <span className="muted">
                  {n} source{n > 1 ? 's' : ''}
                </span>
              </button>
            ))}
          </div>
        </>
      ) : (
        <>
          <p className="count">
            {filtered.length} source{filtered.length > 1 ? 's' : ''}
            {nameFilter ? ` pour « ${nameFilter} »` : ''}
            {lang !== 'all' ? ` · langue ${lang.toUpperCase()}` : ''}
          </p>

          {filtered.length === 0 ? (
            <div className="empty">
              <p className="empty-title">{t('catalogue.vide.titre')}</p>
              <p className="muted">
                {nameFilter
                  ? t('catalogue.vide.texte', { nom: nameFilter })
                  : t('catalogue.vide.langue')}{' '}
                {sourcesOff.length > 0 && (
                  <button
                    className="btn btn-ghost"
                    onClick={() => {
                      setVoirDesactives(true);
                      setShownSources(48);
                    }}
                  >
                    {t('catalogue.desactivees.afficher', {
                      n: sourcesOff.length,
                      s: sourcesOff.length > 1 ? 's' : '',
                    })}
                  </button>
                )}
              </p>
            </div>
          ) : (
            <>
              <div className="band">
                {lang === 'all'
                  ? t('catalogue.bandeau.langues')
                  : `${lang.toUpperCase()} — ${NOMS_LANGUE[lang] ?? ''}`}
                <span className="muted">{filtered.length}</span>
              </div>

              <div className="grid">
            {filtered.slice(0, shownSources).map((s) => {
              const dead = health[s.name];
              const off = estDesactivee(s.name);
              // Deux boutons ne peuvent pas s'imbriquer (HTML invalide, cf.
              // AGENTS.md §14) : la tuile est un `<div>` qui contient le bouton
              // d'ouverture et, à côté, l'interrupteur d'activation.
              return (
                <div key={s.name + '-' + s.lang} className={`slot${off ? ' off' : ''}`}>
                  <button
                    className={`card${dead ? ' dead' : ''}`}
                    onClick={() => openSource(s)}
                    title={dead ? `${t('catalogue.hors.service')} : ${dead.message}` : undefined}
                    aria-label={t('catalogue.ouvrir', { nom: s.name })}
                  >
                    <span className="name">{s.name}</span>
                    {dead ? (
                      <span className="badge dead-badge">{t('catalogue.hors.service')}</span>
                    ) : null}
                    {lang === 'all' && <span className="badge">{s.lang}</span>}
                  </button>
                  <button
                    className="slot-switch"
                    role="switch"
                    aria-checked={!off}
                    aria-label={t(off ? 'catalogue.reactiver' : 'catalogue.desactiver', {
                      nom: s.name,
                    })}
                    title={t(off ? 'catalogue.reactiver' : 'catalogue.desactiver', {
                      nom: s.name,
                    })}
                    onClick={() => void basculerSource(s.name, off)}
                  >
                    <span className="slot-knob" />
                  </button>
                </div>
              );
            })}
              </div>

              {filtered.length > shownSources && (
                <button className="btn btn-more" onClick={() => setShownSources((n) => n + 48)}>
                  Voir plus ({filtered.length - shownSources} restantes)
                </button>
              )}
            </>
          )}
        </>
      )}
    </main>
  );
}
