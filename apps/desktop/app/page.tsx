'use client';

import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

type Source = { name: string; lang: string };
type Manga = { url: string; title: string; source: string; coverUrl?: string | null };
type Chapter = { url: string; title?: string | null; number?: number };
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

function formatBytes(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes < 1024) return `${Math.round(bytes || 0)} o`;
  const mb = bytes / (1024 * 1024);
  if (mb < 1) return `${Math.round(bytes / 1024)} Ko`;
  return `${mb.toFixed(1).replace('.', ',')} Mo`;
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
      setLatest((prev) => (offset === 0 ? items : [...prev, ...items]));
      setLatestOffset(offset + items.length);
      // Une source qui renvoie 0 au-delà de la première page ne pagine pas
      // (mesuré sur EXHENTAI) : on masque le bouton au lieu de le laisser
      // promising des résultats qui n'existent pas.
      setHasMore(items.length > 0);
      if (offset > 0 && items.length === 0) setOnePageOnly(true);
    } catch (e) {
      setError(String(e));
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

  /** Télécharge tous les chapitres de chaque webtoon sélectionné, un par un,
   *  en ignorant ce qui est déjà sur le disque. Annulable. */
  async function startQueue() {
    if (!selected || queueRunning) return;
    const targets = latest.filter((m) => picked.has(mangaKey(m)));
    if (targets.length === 0) return;

    cancelQueue.current = false;
    setQueueRunning(true);
    setError(null);
    setQueue(
      targets.map((m) => ({
        key: mangaKey(m),
        title: m.title,
        source: selected.name,
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
        });
        list = r.chapters ?? [];
      } catch (e) {
        patch({ state: 'erreur', current: String(e) });
        continue;
      }
      patch({ total: list.length });

      let existing: string[] = [];
      try {
        existing = await invoke<string[]>('existing_chapters', {
          source: m.source,
          title: m.title,
        });
      } catch {
        existing = [];
      }

      let done = 0;
      let skipped = 0;
      let failed = 0;
      for (const c of list) {
        if (cancelQueue.current) {
          patch({ done, skipped, failed, state: 'annulé' });
          break;
        }
        try {
          // eslint-disable-next-line no-await-in-loop
          const file = await fetchOneChapter(m.source, m, c, list.length);
          const name = file.split(/[\\/]/).pop() ?? '';
          if (existing.includes(name)) skipped += 1;
          else done += 1;
        } catch {
          failed += 1;
        }
        patch({ done, skipped, failed, current: chapterLabel(c) });
      }
      patch({
        done,
        skipped,
        failed,
        state: cancelQueue.current ? 'annulé' : failed > 0 ? 'erreur' : 'terminé',
      });
    }

    setQueueRunning(false);
    setProgress(null);
    refreshDownloads();
    setTab('downloads');
  }

  function cancelQueueNow() {
    cancelQueue.current = true;
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
    return (
      <main>
        <AppBar tab={tab} onTab={goTab} />
        <header className="page-head">
          <h1>Téléchargements</h1>
          <span className="sub">
            {library.length} webtoon · {library.reduce((n, i) => n + i.downloaded, 0)} chapitres
          </span>
        </header>

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
                  </div>
                </article>
              );
            })}
          </div>
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
            <div className="alert-actions">
              <button className="btn" disabled={loading} onClick={() => loadSource(selected, 0)}>
                Réessayer
              </button>
              <button className="btn btn-ghost" onClick={backToCatalog}>
                Changer de catalogue
              </button>
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
        {filtered.slice(0, shownSources).map((s) => (
          <button key={s.name + '-' + s.lang} className="card" onClick={() => openSource(s)}>
            <span className="name">{s.name}</span>
            <span className="badge">{s.lang}</span>
          </button>
        ))}
      </div>

      {filtered.length > shownSources && (
          <button className="btn btn-more" onClick={() => setShownSources((n) => n + 48)}>
            Voir plus ({filtered.length - shownSources} restantes)
          </button>
        )}
    </main>
  );
}