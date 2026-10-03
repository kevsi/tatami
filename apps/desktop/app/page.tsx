'use client';

import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

type Source = { name: string; lang: string };
type Manga = { url: string; title: string; source: string; coverUrl?: string | null };
type Chapter = { url: string; title?: string | null; number?: number };
type Page = { index: number; url: string; previewUrl?: string | null; token: string };
/** Lecteur paginé (tâche 0.10) : les images viennent du protocole `tatami://`,
 *  jamais d'un appel direct aux hôtes distants. */
type Reader = { chapter: Chapter; pages: Page[]; index: number };
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
const RATINGS = [
  { value: 'SUGGESTIVE', label: 'Suggestive — défaut, exclut le contenu adulte' },
  { value: 'SAFE', label: 'Safe — plusieurs sources renvoient alors zéro résultat' },
  { value: 'ALL', label: 'Toutes — aucun filtre' },
] as const;

const TIMEOUTS = [30, 60, 120] as const;

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
            aria-pressed={tab === t.id}
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

/** File de téléchargement persistée : ce qui reste à faire si l'app s'arrête. */
type SavedQueue = { items: Manga[]; startedAt: number };
const QUEUE_KEY = 'tatami.queue';
const READER_MODE_KEY = 'tatami.readerMode';
const READER_ZOOM_KEY = 'tatami.readerZoom';

type ReaderMode = 'page' | 'webtoon';

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
  const [chapters, setChapters] = useState<Chapter[]>([]);
  const [chapterLimit, setChapterLimit] = useState(CHAPTER_PAGE);
  const [checked, setChecked] = useState<Set<string>>(new Set());
  const [downloading, setDownloading] = useState(false);
  const [progress, setProgress] = useState<Progress | null>(null);
  const [tab, setTab] = useState<string>('catalog');
  const [library, setLibrary] = useState<LibraryItem[]>([]);
  const [notice, setNotice] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [pageSize, setPageSize] = useState(PAGE);
  const [enabled, setEnabled] = useState<string[] | null>(null);
  const [tags, setTags] = useState<Tag[]>([]);
  const [tagsTotal, setTagsTotal] = useState(0);
  const [activeTags, setActiveTags] = useState<string[]>([]);
  const [mangaQuery, setMangaQuery] = useState('');
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
  const stripRef = useRef<HTMLDivElement>(null);
  const stripImages = useRef<(HTMLImageElement | null)[]>([]);
  const [health, setHealth] = useState<SourceHealth>({});
  // Sélection multiple de webtoons + file de téléchargement
  const [selectMode, setSelectMode] = useState(false);
  const [picked, setPicked] = useState<Set<string>>(new Set());
  const [queue, setQueue] = useState<QueueItem[]>([]);
  const [queueRunning, setQueueRunning] = useState(false);
  const cancelQueue = useRef(false);

  useEffect(() => {
    const prefs = loadPrefs();
    setPageSize(prefs.pageSize);
    setEnabled(prefs.enabled);
    setRating(prefs.rating);
    setListTimeout(prefs.listTimeout);
    setHealth(loadHealth());
    if (typeof window !== 'undefined') {
      setReaderMode((window.localStorage.getItem(READER_MODE_KEY) as ReaderMode) || 'page');
      setZoom(Number(window.localStorage.getItem(READER_ZOOM_KEY)) || 100);
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
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  const refreshDownloads = useCallback(() => {
    invoke<LibraryItem[]>('list_library', { root: '' })
      .then((items) => setLibrary(items ?? []))
      .catch(() => setLibrary([]));
  }, []);

  const goTab = useCallback(
    (id: string) => {
      if (id === 'downloads') refreshDownloads();
      setTab(id);
    },
    [refreshDownloads],
  );

  const langs = useMemo(
    () => ['all', ...Array.from(new Set(sources.map((s) => s.lang))).sort()],
    [sources],
  );
  const visibleSources = useMemo(
    () => (enabled === null ? sources : sources.filter((s) => enabled.includes(s.name))),
    [sources, enabled],
  );
  const byLang = lang === 'all' ? visibleSources : visibleSources.filter((s) => s.lang === lang);
  const filtered = byLang.filter((s) => s.name.toLowerCase().includes(nameFilter.toLowerCase()));

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

  async function loadChapters(m: Manga) {
    setOpenManga(m);
    setLoadingState(true);
    setError(null);
    setChapters([]);
    setChapterLimit(CHAPTER_PAGE);
    setChecked(new Set());
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
      // eslint-disable-next-line no-await-in-loop
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
          // eslint-disable-next-line no-await-in-loop
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
  }

  function backToCatalog() {
    setSelected(null);
    setLatest([]);
    setOpenManga(null);
    setChapters([]);
    setChapterLimit(CHAPTER_PAGE);
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
  if (!selected) return;
  setReaderLoading(true);
  setError(null);
  try {
    const r = await invoke<{ pages: Page[] }>('pages', {
      source: selected.name,
      chapterUrl: c.url,
    });
    const pages = r.pages ?? [];
    if (pages.length === 0) {
      setError('Ce chapitre n’a aucune page lisible.');
      return;
    }
    setReader({ chapter: c, pages, index: 0 });
  } catch (e) {
    setError(`Lecture impossible : ${e}`);
  } finally {
    setReaderLoading(false);
  }
}

function closeReader() {
  setReader(null);
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
    if (e.key === 'Escape') {
      closeReader();
      return;
    }
    const forward = ['ArrowRight', 'ArrowDown', 'PageDown', ' '];
    const back = ['ArrowLeft', 'ArrowUp', 'PageUp'];
    if (forward.includes(e.key)) {
      e.preventDefault();
      webtoon ? scrollToPage(Math.min(last, reader!.index + 1)) : turn(1);
    } else if (back.includes(e.key)) {
      e.preventDefault();
      webtoon ? scrollToPage(Math.max(0, reader!.index - 1)) : turn(-1);
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
  // eslint-disable-next-line react-hooks/exhaustive-deps
}, [reader, readerMode]);

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
            {Object.keys(health).length} source(s) signalée(s) hors service lors d'une
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
    return (
      <main>
        <AppBar tab={tab} onTab={goTab} />
        <header className="page-head">
          <h1>Téléchargements</h1>
          <span className="sub">
            {library.length} webtoon · {library.reduce((n, i) => n + i.downloaded, 0)} chapitres
          </span>
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
                    <span className="thumb" />
                  )}
                  <div className="dl-body">
                    <h3 className="dl-title">{item.title}</h3>
                    <span className="muted dl-meta">
                      {item.source} · {item.downloaded}/
                      {item.totalChapters > 0 ? item.totalChapters : '?'} chapitres ·{' '}
                      {formatBytes(item.sizeBytes)}
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
    const next = pages[index + 1];
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
            <span className="reader-count">
              {index + 1} / {pages.length}
            </span>
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
            <div className="reader-stage">
              {page ? (
                <img
                  key={page.token}
                  src={pageSrc(page.token)}
                  alt={`Page ${index + 1} sur ${pages.length}`}
                />
              ) : (
                <p className="muted">Page introuvable.</p>
              )}
            </div>
            <div className="reader-nav">
              <button className="btn" disabled={index === 0} onClick={() => turn(-1)}>
                Page précédente
              </button>
              <button
                className="btn btn-primary"
                disabled={index >= pages.length - 1}
                onClick={() => turn(1)}
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
    return (
      <main>
        <AppBar tab={tab} onTab={goTab} />
        <button className="crumb" onClick={backToSource}>
          {'←'} {selected?.name}
        </button>
        <header className="page-head">
          <h1>{openManga.title}</h1>
          <span className="sub">{selected?.name}</span>
        </header>

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
          <div className="chapter-list" aria-busy="true" aria-label="Chargement des chapitres">
            {Array.from({ length: 8 }).map((_, i) => (
              <div className="chapter skeleton-row" key={i} />
            ))}
          </div>
        ) : (
          <div className="chapter-list">
            {chapters.slice(0, chapterLimit).map((c) => (
              <div key={c.url} className="chapter">
                <input
                  type="checkbox"
                  checked={checked.has(c.url)}
                  onChange={() => toggleChapter(c.url)}
                  aria-label={`Sélectionner ${chapterLabel(c)}`}
                />
                <span className="chapter-label">{chapterLabel(c)}</span>
                {c.number != null && <span className="badge">#{c.number}</span>}
                <button
                  className="btn btn-ghost"
                  disabled={readerLoading}
                  onClick={() => void openReader(c)}
                  aria-label={`Lire ${chapterLabel(c)}`}
                >
                  {readerLoading ? 'Chargement…' : 'Lire'}
                </button>
                <button
                  className="btn btn-ghost"
                  disabled={downloading}
                  onClick={() => downloadChapter(c)}
                  aria-label={`Télécharger ${chapterLabel(c)}`}
                >
                  Télécharger
                </button>
              </div>
            ))}
          </div>
        )}

        {!loading && chapters.length === 0 && !error && (
          <p className="muted">Aucun chapitre disponible sur cette source.</p>
        )}

        {chapters.length > chapterLimit && (
          <button
            className="btn btn-more"
            onClick={() => setChapterLimit((n) => n + CHAPTER_STEP)}
          >
            Afficher plus ({chapters.length - chapterLimit} restants)
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
              <button className="btn" onClick={() => setSelectMode(true)} disabled={latest.length === 0}>
                Sélectionner
              </button>
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
          <p className="count">
            {latest.length} webtoon{latest.length > 1 ? 's' : ''}
            {loading ? ' — chargement…' : ''}
          </p>
        )}

        {loading && latest.length === 0 ? (
          <div className="grid">
            {Array.from({ length: 8 }).map((_, i) => (
              <div className="skeleton" key={i} />
            ))}
          </div>
        ) : (
          <div className="manga-grid">
            {latest.map((m) => {
              const key = mangaKey(m);
              const isPicked = picked.has(key);
              return (
                <button
                  key={key}
                  className={`manga${selectMode ? ' pickable' : ''}${isPicked ? ' picked' : ''}`}
                  aria-pressed={selectMode ? isPicked : undefined}
                  onClick={() => (selectMode ? togglePick(m) : loadChapters(m))}
                >
                  {m.coverUrl ? (
                    <img className="thumb" src={m.coverUrl} alt="" loading="lazy" />
                  ) : (
                    <span className="thumb" />
                  )}
                  <span className="label">{m.title}</span>
                  {selectMode && (
                    <span className="pick" aria-hidden="true">
                      {isPicked ? '✓' : ''}
                    </span>
                  )}
                </button>
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

  // ---------- Vue : catalogue ----------
  return (
    <main>
      <AppBar tab={tab} onTab={goTab} />

      <header className="page-head">
        <h1>Catalogues</h1>
        <span className="sub">{visibleSources.length} sources disponibles</span>
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

      <p className="count">
        {filtered.length} source{filtered.length > 1 ? 's' : ''}
        {nameFilter ? ` pour « ${nameFilter} »` : ''}
      </p>

      <div className="grid">
        {filtered.slice(0, shownSources).map((s) => {
          const dead = health[s.name];
          return (
            <button
              key={s.name + '-' + s.lang}
              className={`card${dead ? ' dead' : ''}`}
              onClick={() => openSource(s)}
              title={dead ? `Dernier échec : ${dead.message}` : undefined}
            >
              <span className="name">{s.name}</span>
              {dead ? <span className="badge dead-badge">hors service</span> : null}
              <span className="badge">{s.lang}</span>
            </button>
          );
        })}
      </div>

      {filtered.length > shownSources && (
          <button className="btn btn-more" onClick={() => setShownSources((n) => n + 48)}>
            Voir plus ({filtered.length - shownSources} restantes)
          </button>
        )}
    </main>
  );
}