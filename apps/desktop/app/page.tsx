'use client';

import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

type Source = { name: string; lang: string };
type Manga = {
  url: string;
  title: string;
  source: string;
  coverUrl?: string | null;
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
type Page = { index: number; url: string; previewUrl?: string | null; token: string };
/** Lecteur paginé (tâche 0.10) : les images viennent du protocole `tatami://`,
 *  jamais d'un appel direct aux hôtes distants. */
/** Un favori enregistré par l'utilisateur (table `favorites`). */
type Favorite = {
  source: string;
  manga_url: string;
  title: string;
  cover_url: string | null;
  content_rating: string | null;
  added_at: number;
};

/** Une entrée de l'historique de lecture (table `history`). */
type HistoryItem = {
  source: string;
  manga_url: string;
  title: string;
  cover_url: string | null;
  content_rating: string | null;
  chapter_url: string | null;
  chapter_title: string | null;
  chapter_index: number;
  page_index: number;
  page_count: number;
  read_at: number;
};

type Reader = { chapter: Chapter; pages: Page[]; index: number; source: string };
type Tag = { key: string; title: string };
type LibraryItem = {
  title: string;
  source: string;
  url: string;
  coverUrl?: string | null;
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
  state: 'attente' | 'téléchargement' | 'terminé' | 'erreur' | 'annulé';
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
type Tri = 'source' | 'az' | 'za';

const TRIS: { value: Tri; label: string }[] = [
  { value: 'source', label: 'Ordre du site' },
  { value: 'az', label: 'A → Z' },
  { value: 'za', label: 'Z → A' },
];

const RATINGS = [
  { value: 'SUGGESTIVE', label: 'Suggestive — défaut, exclut le contenu adulte' },
  { value: 'SAFE', label: 'Safe — plusieurs sources renvoient alors zéro résultat' },
  { value: 'ALL', label: 'Toutes — aucun filtre' },
] as const;

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
  { id: 'home', label: 'Accueil' },
  { id: 'catalog', label: 'Catalogues' },
  { id: 'downloads', label: 'Téléchargements' },
  { id: 'settings', label: 'Paramètres' },
] as const;

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
  return (
    <div className="appbar">
      <div className="brand">
        <span className="logo" />
        Tatami
      </div>
      <nav className="tabs" role="tablist" aria-label="Sections">
        {TABS.map((t) => (
          <button
            key={t.id}
            className="tab"
            role="tab"
            aria-selected={tab === t.id}
            onClick={() => onTab(t.id)}
          >
            {t.label}
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

/** Ancienneté en français court : « 5 mois », « 3 j » — le format des cartes
 *  de catalogue, où la date exacte Takes trop de place. */
function sinceFr(ms: number): string {
  const diff = Date.now() - ms;
  if (diff < 0) return "aujourd'hui";
  const min = Math.round(diff / 60_000);
  if (min < 60) return min <= 1 ? "à l'instant" : `${min} min`;
  const h = Math.round(min / 60);
  if (h < 24) return `${h} h`;
  const d = Math.round(h / 24);
  if (d < 31) return `${d} j`;
  const mo = Math.round(d / 30.44);
  if (mo < 12) return `${mo} mois`;
  const y = Math.floor(d / 365.25);
  return y <= 1 ? '1 an' : `${y} ans`;
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

/** Traduit une erreur brute du moteur en un message court, sans l'URL technique.
 *  Les codes viennent de contract/protocol.json ; le reste est heuristique sur
 *  les messages d'exception observés (sources mortes : 404, hôte inconnu, 403). */
function describeError(raw: string, sourceName: string): string {
  const s = raw.replace(/\s+/g, ' ').trim();
  if (/TIMEOUT|timeout/i.test(s)) return `${sourceName} n'a pas répondu à temps. Site mort ou trop lent.`;
  if (/CHALLENGE_REQUIRED/i.test(s)) return `${sourceName} demande une vérification anti-robot, pas encore gérée.`;
  if (/RATE_LIMITED/i.test(s)) return `${sourceName} est surchargé. Réessaie dans quelques minutes.`;
  if (/UnknownHostException/i.test(s)) return `Le domaine de ${sourceName} n'existe plus.`;
  if (/404|NotFoundException/i.test(s)) return `${sourceName} renvoie 404 : la source est hors service.`;
  if (/403/.test(s)) return `${sourceName} bloque les requêtes (403).`;
  if (/500|502|503/.test(s)) return `${sourceName} a renvoyé une erreur serveur.`;
  const short = s.length > 140 ? `${s.slice(0, 140)}…` : s;
  return `${sourceName} : ${short}`;
}

export default function Home() {
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
  const [notice, setNotice] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [pageSize, setPageSize] = useState(PAGE);
  const [enabled, setEnabled] = useState<string[] | null>(null);
  // Tâche 1.2 : sources écartées par l'utilisateur (donnée métier, pas une
  // préférence d'affichage : elle vit en base, cf. AGENTS.md §6).
  const [sourcesOff, setSourcesOff] = useState<string[]>([]);
  const [voirDesactives, setVoirDesactives] = useState(false);
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
  const [reader, setReader] = useState<Reader | null>(null);
  const [readerLoading, setReaderLoading] = useState(false);
  const [challenge, setChallenge] = useState<string | null>(null);
  const [challengeBusy, setChallengeBusy] = useState(false);
  // Confort de lecture : mode paginé ou webtoon (bande verticale), et largeur.
  const [readerMode, setReaderMode] = useState<ReaderMode>('page');
  const [zoom, setZoom] = useState(100);
  // Tâche 0.13 : ajustement largeur/hauteur, double page, sens de lecture.
  const [fit, setFit] = useState<ReaderFit>('width');
  const [spread, setSpread] = useState(false);
  const [readerDir, setReaderDir] = useState<ReaderDir>('rtl');
  // Tâche 0.14 : aller directement à un chapitre sans repasser par la fiche.
  const [pageJump, setPageJump] = useState('');
  const stripRef = useRef<HTMLDivElement>(null);
  const stripImages = useRef<(HTMLImageElement | null)[]>([]);
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
    return () => {
      unlisten.then((off) => off()).catch(() => {});
    };
    // Montage uniquement : rejouer cet effet à chaque changement de
    // `refreshDownloads` réabonnerait l'écouteur d'événements.
    // eslint-disable-next-line react-hooks/exhaustive-deps
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
        const [fav, hist] = await Promise.all([
          invoke<Favorite[]>('list_favorites'),
          invoke<HistoryItem[]>('list_recent', { limit: RECENT_LIMIT, hideAdult: hide }),
        ]);
        setFavorites(fav ?? []);
        setRecent(hist ?? []);
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

  const goTab = useCallback(
    (id: string) => {
      if (id === 'downloads') refreshDownloads();
      if (id === 'home') void refreshShelf();
      // Changer d'onglet quitte la vue en cours (source, fiche) : sinon
      // l'ancien écran, rendu avant l'accueil, resterait affiché par-dessus.
      setOpenManga(null);
      if (id !== 'catalog') setSelected(null);
      setTab(id);
    },
    [refreshDownloads, refreshShelf],
  );

  function isFavorite(m: Manga): boolean {
    return favorites.some((f) => f.source === m.source && f.manga_url === m.url);
  }

  // Charge la bibliothèque au démarrage. Indispensable : `tab` vaut « home »
  // dès le premier rendu SANS passer par `goTab`, donc rien ne déclencherait
  // le rechargement sinon (mesuré : l'accueil affichait « 0 favori » après un
  // redémarrage alors que la base contenait bien la ligne).
  // `refreshShelf` ne touche l'état qu'APRÈS son `await` : aucun setState
  // synchrone ici. La règle n'analyse pas la suspension et le signale à tort.
  useEffect(() => {
    // eslint-disable-next-line react-hooks/set-state-in-effect
    void refreshShelf();
    invoke<string[]>('list_disabled_sources')
      .then((r) => setSourcesOff(r ?? []))
      .catch(() => setSourcesOff([]));
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  function estDesactivee(nom: string): boolean {
    return sourcesOff.includes(nom);
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
      setError(`Activation de la source impossible : ${e}`);
    }
  }

  async function reactiverToutes() {
    const avant = sourcesOff;
    setSourcesOff([]);
    try {
      await invoke('enable_all_sources');
    } catch (e) {
      setSourcesOff(avant);
      setError(`Réactivation impossible : ${e}`);
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
              content_rating: m.contentRating ?? null,
              added_at: Date.now(),
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
      setError(`Favori non enregistré : ${e}`);
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
        noteHealth([selected.name], code, describeError(raw, selected.name));
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
                  c.uploadDate != null && c.uploadDate > 0 ? ` · ${sinceFr(c.uploadDate)}` : ''
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
  }, [latest, showRecent, selected?.name]);

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
      setError(`Téléchargement échoué : ${e}`);
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
        patch({ state: 'annulé' });
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
        patch({ state: 'erreur', current: describeError(String(e), m.source) });
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
          patch({ done, failed, state: 'annulé' });
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
        state: cancelQueue.current ? 'annulé' : failed > 0 ? 'erreur' : 'terminé',
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

  async function removeDownload(item: LibraryItem) {    try {
      const n = await invoke<number>('remove_download', {
        source: item.source,
        title: item.title,
      });
      setNotice(`${item.title} : ${n} chapitre(s) supprimé(s).`);
      refreshDownloads();
    } catch (e) {
      setError(`Suppression impossible : ${e}`);
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

async function openReader(c: Chapter) {
  return openReaderAt(c, 0);
}

/** Ouvre un chapitre à une page donnée. `sourceName` permet la reprise depuis
 *  l'accueil, où aucune source n'est sélectionnée. */
async function openReaderAt(c: Chapter, startIndex: number, sourceName?: string) {
  const src = sourceName ?? selected?.name;
  if (!src) return;
  setReaderLoading(true);
  setError(null);
  try {
    const r = await invoke<{ pages: Page[] }>('pages', {
      source: src,
      chapterUrl: c.url,
    });
    const pages = r.pages ?? [];
    if (pages.length === 0) {
      setError('Ce chapitre n’a aucune page lisible.');
      return;
    }
    setPageJump('');
    // La position mémorisée peut dépasser le chapitre si celui-ci a été
    // raccourci depuis : on retombe sur sa dernière page.
    const index = Math.max(0, Math.min(pages.length - 1, startIndex));
    setReader({ chapter: c, pages, index, source: src });
  } catch (e) {
    setError(`Lecture impossible : ${e}`);
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
  const manga: Manga = {
    url: h.manga_url,
    title: h.title,
    source: h.source,
    coverUrl: h.cover_url,
    contentRating: h.content_rating,
  };
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
    setError(`Chapitres indisponibles : ${chapitresRes.reason}`);
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
  await openReaderAt(chapter, h.page_index, h.source);
}

// Mémorise la position de lecture à chaque changement de chapitre ou de page.
// Un `INSERT ... ON CONFLICT` par page tournée, sans conséquence visible. Le
// classement vient de la FICHE (`details`) quand elle est connue : mesuré, la
// liste renvoie souvent `UNKNOWN` là où la fiche donne `ADULT`, et c'est
// cette valeur que l'incognito filtre.
useEffect(() => {
  if (!reader || !openManga) return;
  const manga = info ?? openManga;
  const chapterIndex = chapters.findIndex((c) => c.url === reader.chapter.url);
  void invoke('save_progress', {
    source: reader.source,
    mangaUrl: openManga.url,
    title: openManga.title,
    coverUrl: openManga.coverUrl ?? manga.coverUrl ?? null,
    contentRating: manga.contentRating ?? null,
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
  setReader(null);
  // Reprise depuis l'accueil : aucune source n'est sélectionnée, on rend donc
  // l'accueil plutôt qu'une fiche de chapitre orpheline.
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

/** Tâche 0.14 : saute à la page demandée (1 → index 0). */
function jumpToPage(raw: string) {
  const n = Number(raw);
  setPageJump('');
  if (!reader || !Number.isFinite(n) || n < 1) return;
  const target = Math.min(reader.pages.length, Math.round(n)) - 1;
  setReader((r) => (r ? { ...r, index: target } : r));
}

/** Fait défiler la bande jusqu'à la page demandée (mode webtoon). */
function scrollToPage(i: number) {
  const el = stripImages.current[i];
  if (el) {
    el.scrollIntoView({ behavior: 'smooth', block: 'start' });
    setReader((r) => (r ? { ...r, index: i } : r));
  } else {
    turn(i - (reader?.index ?? 0));
  }
}

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
  // eslint-disable-next-line react-hooks/exhaustive-deps
}, [reader?.chapter.url, readerMode, reader?.pages.length]);

function turn(delta: number) {
  setReader((r) => {
    if (!r) return r;
    const next = r.index + delta;
    if (next < 0 || next >= r.pages.length) return r;
    return { ...r, index: next };
  });
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
  // eslint-disable-next-line react-hooks/exhaustive-deps
}, [reader, readerMode, spread]);

  // ---------- Vue : Réglages ----------
  if (tab === 'settings') {
    return (
      <main>
        <AppBar tab={tab} onTab={goTab} />
        <button className="crumb" onClick={() => setTab('catalog')}>
          {'←'} Catalogues
        </button>
        <header className="page-head">
          <h1>Paramètres</h1>
        </header>

        <section className="panel">
          <h2>Contenu</h2>
          <label className="field">
            Contenu affiché
            <select
              value={rating}
              onChange={(e) => {
                setRating(e.target.value);
                window.localStorage.setItem(SAFE_KEY, e.target.value);
              }}
              aria-label="Classification de contenu maximale affichée"
            >
              {RATINGS.map((r) => (
                <option key={r.value} value={r.value}>
                  {r.label}
                </option>
              ))}
            </select>
          </label>
          <p className="muted">
            Une seule classification à la fois : les sources n’en acceptent pas
            plusieurs. Les sources qui n’exposent pas de classification ne sont pas filtrées.
          </p>
        </section>

        <section className="panel">
          <h2>Réseau</h2>
          <label className="field">
            Délai avant abandon
            <select
              value={listTimeout}
              onChange={(e) => {
                const v = Number(e.target.value);
                setListTimeout(v);
                window.localStorage.setItem(TIMEOUT_KEY, String(v));
              }}
              aria-label="Délai avant abandon d'une source, en secondes"
            >
              {TIMEOUTS.map((t) => (
                <option key={t} value={t}>
                  {t} secondes
                </option>
              ))}
            </select>
          </label>
          <p className="muted">
            Certaines sources scannent tout leur catalogue avant de répondre
            (EXHENTAI : 36 s). Au-delà du délai, la source est signalée hors
            service plutôt que de faire attendre.
          </p>
        </section>

        <section className="panel">
          <h2>Téléchargement</h2>
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
            Dossier de téléchargement : <code>%APPDATA%\Tatami\downloads</code>
          </p>
        </section>

        {Object.keys(health).length > 0 && (
          <p className="muted">
            {Object.keys(health).length} source(s) signalée(s) hors service lors d&apos;une
            visite.{' '}
            <button
              className="btn btn-ghost"
              onClick={() => {
                window.localStorage.removeItem(HEALTH_KEY);
                setHealth({});
              }}
            >
              Tout réessayer
            </button>
          </p>
        )}

        <section className="panel">
          <h2>Catalogues affichés</h2>
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
  if (tab === 'downloads') {
    const running = queue.find((q) => q.state === 'téléchargement');
    const savedQueue = loadSavedQueue();
    const totalChapters = library.reduce((n, i) => n + i.downloaded, 0);
    const totalBytes = library.reduce((n, i) => n + i.sizeBytes, 0);
    const partiels = library.filter((i) => !i.complete).length;
    return (
      <main>
        <AppBar tab={tab} onTab={goTab} />
        <header className="page-head">
          <h1>Téléchargements</h1>
        </header>

        {error && <p className="error">{error}</p>}
        {notice && <p className="notice">{notice}</p>}

        {savedQueue && savedQueue.items.length > 0 && !queueRunning && (
          <section className="panel resume">
            <h2>File interrompue</h2>
            <p className="muted">
              {savedQueue.items.length} webtoon{savedQueue.items.length > 1 ? 's' : ''} attendaient
              encore des chapitres. Les chapitres déjà téléchargés seront ignorés.
            </p>
            <div className="alert-actions">
              <button className="btn btn-primary" onClick={resumeQueue}>
                Reprendre
              </button>
              <button className="btn btn-ghost" onClick={forgetQueue}>
                Abandonner la file
              </button>
            </div>
          </section>
        )}

        {queue.length > 0 && (
          <section className="panel queue">
            <h2>File de téléchargement</h2>
            {queueRunning ? (
              <button className="btn" onClick={cancelQueueNow}>
                Arrêter après le chapitre en cours
              </button>
            ) : (
              <button className="btn btn-ghost" onClick={() => setQueue([])}>
                Effacer la file
              </button>
            )}
            <div className="queue-list">
              {queue.map((q) => {
                const pct = q.total > 0 ? Math.round(((q.done + q.skipped) / q.total) * 100) : 0;
                return (
                  <div className="queue-row" key={q.key}>
                    <div className="queue-head">
                      <span className="queue-title">{q.title}</span>
                      <span className={`badge state-${q.state.replace('é', 'e')}`}>{q.state}</span>
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
          <p className="muted">Aucun webtoon téléchargé pour l&apos;instant.</p>
        ) : (
          <>
            <section className="dl-stats" aria-label="Récapitulatif des téléchargements">
              <div className="dl-stat">
                <b>{library.length}</b>
                <span>webtoon{library.length > 1 ? 's' : ''}</span>
              </div>
              <div className="dl-stat">
                <b>{totalChapters}</b>
                <span>chapitres</span>
              </div>
              <div className="dl-stat">
                <b>{formatBytes(totalBytes)}</b>
                <span>sur le disque</span>
              </div>
              <div className="dl-stat">
                <b>{partiels}</b>
                <span>à compléter</span>
              </div>
            </section>

            <div className="band">Vos webtoons</div>

            <div className="dl-grid">
              {library.map((item) => {
                const pct =
                  item.totalChapters > 0
                    ? Math.round((item.downloaded / item.totalChapters) * 100)
                    : 100;
                return (
                  <article className="dl-card" key={item.dir}>
                    {item.coverUrl ? (
                      <img className="thumb" src={item.coverUrl} alt="" loading="lazy" />
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
                          ? `${item.downloaded}/${item.totalChapters} chapitres`
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
                        {!item.complete && item.downloaded > 0 && (
                          <button
                            className="btn"
                            disabled={queueRunning}
                            onClick={() => void resumeManga(item)}
                          >
                            Reprendre
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
            <h3 id="confirm-titre">Supprimer {deleteTarget.title} ?</h3>
            <p className="muted">
              {deleteTarget.downloaded} chapitre(s), {formatBytes(deleteTarget.sizeBytes)} seront
              supprimés du dossier de téléchargements. Le webtoon reste téléchargeable ensuite.
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
                Supprimer définitivement
              </button>
              <button className="btn btn-ghost" onClick={() => setDeleteTarget(null)}>
                Annuler
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
    const page = pages[index];
    const by = readerMode === 'page' && spread ? 2 : 1;
    const next = pages[index + by];
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
      <main className="reader">
        <header className="reader-bar">
          <button className="btn btn-ghost" onClick={closeReader}>
            {'←'} Chapitres
          </button>
          <span className="reader-title">
            {openManga?.title} · {chapterLabel(chapter)}
          </span>

          <div className="reader-tools">
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
                aria-label="Aller à la page"
              />
              <span className="muted">/ {pages.length}</span>
            </form>
          </div>
        </header>

        {readerMode === 'webtoon' ? (
          <div className="webtoon" ref={stripRef}>
            {pages.map((p, i) => (
              <img
                key={p.token}
                ref={(el) => {
                  stripImages.current[i] = el;
                }}
                src={pageSrc(p.token)}
                alt={`Page ${i + 1}`}
                loading="lazy"
                decoding="async"
                style={{ width: `${zoom}%` }}
                onDoubleClick={() => changeZoom(zoom >= 200 ? 100 : 200)}
              />
            ))}
          </div>
        ) : (
          <>
            <div className={`reader-stage fit-${fit}${spread ? ' spread' : ''}`}>
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
                <p className="muted">Page introuvable.</p>
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

        {readerMode === 'webtoon' && (
          <div className="reader-nav">
            <button className="btn" disabled={index === 0} onClick={() => scrollToPage(index - 1)}>
              {'↑'} Page précédente
            </button>
            <button
              className="btn btn-primary"
              disabled={index >= pages.length - 1}
              onClick={() => scrollToPage(index + 1)}
            >
              Page suivante {'↓'}
            </button>
          </div>
        )}

        {readerMode === 'page' && next && (
          <img className="preload" src={pageSrc(next.token)} alt="" aria-hidden="true" />
        )}
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
        <AppBar tab={tab} onTab={goTab} />
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
            {isFavorite(fiche) ? '♥ Dans vos favoris' : '♡ Ajouter aux favoris'}
          </button>
        </header>

        {(summary || rows.length > 0 || fiche.coverUrl) && (
          <section className="sheet">
            {fiche.coverUrl && (
              <img className="sheet-cover" src={fiche.coverUrl} alt="" loading="lazy" />
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
        {notice && <p className="notice">{notice}</p>}

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
          <p className="muted">Aucun chapitre disponible sur cette source.</p>
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
        <AppBar tab={tab} onTab={goTab} />
        <button className="crumb" onClick={backToCatalog}>
          {'←'} Catalogues
        </button>
        <header className="page-head">
          <h1>{selected.name}</h1>
          <span className="sub">{selected.lang.toUpperCase()}</span>
        </header>

        {notice && <p className="notice">{notice}</p>}

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
              <span className="muted">Clique un webtoon pour voir ses chapitres.</span>
              <div className="select-actions">
                <label className="recent-toggle">
                  <input
                    type="checkbox"
                    checked={showRecent}
                    onChange={(e) => setShowRecent(e.target.checked)}
                  />
                  <span>Chapitres récents</span>
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
            <p>{describeError(error, selected.name)}</p>
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
              {TRIS.map((t) => (
                <button
                  key={t.value}
                  className="chip"
                  aria-pressed={tri === t.value}
                  onClick={() => setTri(t.value)}
                >
                  {t.label}
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
                      {m.coverUrl ? (
                        <img className="thumb" src={m.coverUrl} alt="" loading="lazy" />
                      ) : (
                        <span className="thumb thumb-empty" aria-hidden="true">
                          {(m.title.trim()[0] ?? '?').toUpperCase()}
                        </span>
                      )}
                      {cardTotal[key] > 0 && (
                        <span className="card-count" title={`${cardTotal[key]} chapitres`}>
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
          <p className="muted">Aucun résultat sur cette source.</p>
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
  if (tab === 'home') {
    return (
      <main>
        <AppBar tab={tab} onTab={goTab} />
        <header className="page-head">
          <h1>Accueil</h1>
          <span className="sub">
            {recent.length} lu{recent.length > 1 ? 's' : ''} récemment ·{' '}
            {favorites.length} favori{favorites.length > 1 ? 's' : ''}
          </span>
        </header>

        {error && <p className="error">{error}</p>}
        {notice && <p className="notice">{notice}</p>}

        <div className="band">
          Continuer la lecture
          <label className="recent-toggle" title="Masque de l'accueil les titres classés adultes par leur source. Ils restent accessibles en recherche.">
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
            <span>Incognito</span>
          </label>
        </div>

        {recent.length === 0 ? (
          <p className="muted">
            Rien à reprendre. Lancez une lecture depuis un catalogue, elle apparaîtra ici
            {incognito
              ? ' — les titres classés adultes restent masqués tant que l’incognito est actif.'
              : '.'}
          </p>
        ) : (
          <div className="dl-grid">
            {recent.map((h) => (
              <article className="dl-card" key={`${h.source}|${h.manga_url}`}>
                {h.cover_url ? (
                  <img className="thumb" src={h.cover_url} alt="" loading="lazy" />
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
                    page {h.page_index + 1}/{h.page_count || '?'} ·{' '}
                    {sinceFr(h.read_at)}
                  </span>
                  <div className="dl-actions">
                    <button
                      className="btn btn-primary"
                      disabled={readerLoading}
                      onClick={() => void resumeReading(h)}
                    >
                      {readerLoading ? 'Ouverture…' : 'Reprendre'}
                    </button>
                    <button
                      className="btn btn-ghost"
                      aria-label={`Oublier ${h.title} dans l'historique`}
                      onClick={() => {
                        void invoke('forget_history', {
                          source: h.source,
                          mangaUrl: h.manga_url,
                        })
                          .then(() => refreshShelf())
                          .catch((e) => setError(`Suppression impossible : ${e}`));
                      }}
                    >
                      Oublier
                    </button>
                  </div>
                </div>
              </article>
            ))}
          </div>
        )}

        <div className="band">
          Vos favoris
          {favorites.length > 0 && (
            <button
              className="btn btn-ghost"
              onClick={() => {
                void invoke('clear_history')
                  .then(() => refreshShelf())
                  .catch((e) => setError(`Purge impossible : ${e}`));
              }}
            >
              Vider l&apos;historique
            </button>
          )}
        </div>

        {favorites.length === 0 ? (
          <p className="muted">
            Aucun favori. Ouvrez un webtoon puis cliquez sur l&apos;icône ♥.
          </p>
        ) : (
          <div className="manga-grid">
            {favorites.map((f) => {
              const m: Manga = {
                url: f.manga_url,
                title: f.title,
                source: f.source,
                coverUrl: f.cover_url,
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
                      {f.cover_url ? (
                        <img className="thumb" src={f.cover_url} alt="" loading="lazy" />
                      ) : (
                        <span className="thumb thumb-empty" aria-hidden="true">
                          {(f.title.trim()[0] ?? '?').toUpperCase()}
                        </span>
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
      <AppBar tab={tab} onTab={goTab} />

      <header className="page-head">
        <h1>Catalogues</h1>
        <span className="sub">
          {visibleSources.length} sources disponibles
          {sourcesOff.length > 0 ? ` · ${sourcesOff.length} désactivée${sourcesOff.length > 1 ? 's' : ''}` : ''}
        </span>
      </header>

      <input
        className="search"
        type="search"
        aria-label="Rechercher un catalogue"
        placeholder="Rechercher un catalogue…"
        value={nameFilter}
        onChange={(e) => {
          setNameFilter(e.target.value);
          setShownSources(48);
        }}
      />

      <div className="chips" role="group" aria-label="Langue">
        {langs.map((l) => (
          <button
            key={l}
            className="chip"
            onClick={() => {
              setLang(l);
              setShownSources(48);
            }}
            aria-pressed={l === lang}
          >
            {l === 'all' ? 'Toutes' : l.toUpperCase()}
          </button>
        ))}
      </div>

      {error && <p className="error">{error}</p>}

      {sourcesOff.length > 0 && (
        <div className="row-actions">
          <label className="recent-toggle" title="Les sources désactivées restent enregistrées, elles sont juste masquées.">
            <input
              type="checkbox"
              checked={voirDesactives}
              onChange={(e) => {
                setVoirDesactives(e.target.checked);
                setShownSources(48);
              }}
            />
            <span>Afficher les {sourcesOff.length} source{sourcesOff.length > 1 ? 's' : ''} désactivée{sourcesOff.length > 1 ? 's' : ''}</span>
          </label>
          <button className="btn btn-ghost" onClick={() => void reactiverToutes()}>
            Tout réactiver
          </button>
        </div>
      )}

      {enTiles ? (
        <>
          <p className="count">
            {visibleSources.length} sources réparties en {langCounts.length} langues. Choisissez
            une langue pour parcourir ses sources.
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
              <p className="empty-title">Aucune source à afficher</p>
              <p className="muted">
                {nameFilter
                  ? `Aucune source ne correspond à « ${nameFilter} ».`
                  : 'Toutes les sources de cette langue sont désactivées.'}{' '}
                {sourcesOff.length > 0 && (
                  <button
                    className="btn btn-ghost"
                    onClick={() => {
                      setVoirDesactives(true);
                      setShownSources(48);
                    }}
                  >
                    Afficher les sources désactivées
                  </button>
                )}
              </p>
            </div>
          ) : (
            <>
              <div className="band">
                {lang === 'all' ? 'Toutes les langues' : `${lang.toUpperCase()} — ${NOMS_LANGUE[lang] ?? ''}`}
                <span className="muted">{filtered.length} source{filtered.length > 1 ? 's' : ''}</span>
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
                    title={dead ? `Dernier échec : ${dead.message}` : undefined}
                    aria-label={`Ouvrir la source ${s.name}`}
                  >
                    <span className="name">{s.name}</span>
                    {dead ? <span className="badge dead-badge">hors service</span> : null}
                    {lang === 'all' && <span className="badge">{s.lang}</span>}
                  </button>
                  <button
                    className="slot-switch"
                    role="switch"
                    aria-checked={!off}
                    aria-label={`${off ? 'Réactiver' : 'Désactiver'} la source ${s.name}`}
                    title={
                      off
                        ? 'Source désactivée : elle n’apparaît plus dans le catalogue'
                        : 'Désactiver cette source'
                    }
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
