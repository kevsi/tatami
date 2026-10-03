'use client';

import { useCallback, useEffect, useMemo, useState } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

type Source = { name: string; lang: string };
type Manga = { url: string; title: string; source: string; coverUrl?: string | null };
type Chapter = { url: string; title?: string | null; number?: number };
type Tag = { key: string; title: string };
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
const TAGS_SHOWN = 24;
// Valeurs acceptées par les parsers (enum ContentRating). Une seule à la fois :
// en envoyer plusieurs lève « Multiple Content ratings are not supported ».
const RATINGS = [
  { value: 'SUGGESTIVE', label: 'Suggestive — défaut, exclut le contenu adulte' },
  { value: 'SAFE', label: 'Safe — plusieurs sources renvoient alors zéro résultat' },
  { value: 'ALL', label: 'Toutes — aucun filtre' },
] as const;

function loadPrefs(): { pageSize: number; enabled: string[] | null; rating: string } {
  if (typeof window === 'undefined') return { pageSize: PAGE, enabled: null, rating: 'SUGGESTIVE' };
  const raw = window.localStorage.getItem(PAGE_KEY);
  const rawSources = window.localStorage.getItem(SOURCES_KEY);
  const rawRating = window.localStorage.getItem(SAFE_KEY);
  return {
    pageSize: raw ? Number(raw) || PAGE : PAGE,
    enabled: rawSources ? (JSON.parse(rawSources) as string[]) : null,
    rating: rawRating || 'SUGGESTIVE',
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
  const [downloads, setDownloads] = useState<string[]>([]);
  const [notice, setNotice] = useState<string | null>(null);
  const [error, setError] = useState<string | null>(null);
  const [pageSize, setPageSize] = useState(PAGE);
  const [enabled, setEnabled] = useState<string[] | null>(null);
  const [tags, setTags] = useState<Tag[]>([]);
  const [tagsTotal, setTagsTotal] = useState(0);
  const [activeTags, setActiveTags] = useState<string[]>([]);
  const [mangaQuery, setMangaQuery] = useState('');
  const [rating, setRating] = useState('SUGGESTIVE');

  useEffect(() => {
    const prefs = loadPrefs();
    setPageSize(prefs.pageSize);
    setEnabled(prefs.enabled);
    setRating(prefs.rating);
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
    invoke<string[]>('list_downloads')
      .then(setDownloads)
      .catch(() => setDownloads([]));
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
    }
    try {
      const r = await invoke<{ list: Manga[] }>('latest', {
        source: s.name,
        offset,
        limit: pageSize,
        query: useQuery,
        tags: useTags,
        contentRating: rating === 'ALL' ? '' : rating,
      });
      const items = r.list ?? [];
      setLatest((prev) => (offset === 0 ? items : [...prev, ...items]));
      setLatestOffset(offset + items.length);
      setHasMore(items.length > 0);
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

  async function downloadChapter(c: Chapter) {
    if (!openManga || !selected) return;
    setDownloading(true);
    setError(null);
    setProgress({
      source: selected.name,
      chapter: c.title ?? 'chapitre',
      page: 0,
      total: 0,
      done: false,
      error: null,
    });
    try {
      const file = await invoke<string>('download_chapter', {
        source: selected.name,
        mangaUrl: openManga.url,
        mangaTitle: openManga.title,
        chapterUrl: c.url,
        chapterTitle: c.title ?? (c.number ? `Chapitre ${c.number}` : c.url),
      });
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
    const byManga = new Map<string, string[]>();
    for (const file of downloads) {
      const parts = file.split('\\');
      const manga = parts[parts.length - 2] ?? '?';
      byManga.set(manga, [...(byManga.get(manga) ?? []), parts[parts.length - 1] ?? '']);
    }
    return (
      <main>
        <AppBar tab={tab} onTab={goTab} />
        <button className="crumb" onClick={() => setTab('catalog')}>
          {'←'} Catalogues
        </button>
        <header className="page-head">
          <h1>Téléchargements</h1>
          <span className="sub">{downloads.length} fichier(s)</span>
        </header>

        {downloads.length === 0 && (
          <p className="muted">Aucun téléchargement pour l&apos;instant.</p>
        )}

        {[...byManga.entries()].map(([manga, files]) => (
          <section className="panel" key={manga}>
            <h2>{manga}</h2>
            <div className="dl-files">
              {files.map((f) => (
                <span className="file" key={f}>
                  {f}
                </span>
              ))}
            </div>
          </section>
        ))}
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
                  aria-label={`Sélectionner ${c.title ?? `chapitre ${c.number ?? ''}`}`}
                />
                <span className="chapter-label">
                  {c.title ?? (c.number ? `Chapitre ${c.number}` : 'Sans titre')}
                </span>
                {c.number != null && <span className="badge">#{c.number}</span>}
                <button
                  className="btn btn-ghost"
                  disabled={downloading}
                  onClick={() => downloadChapter(c)}
                  aria-label={`Télécharger ${c.title ?? 'le chapitre'}`}
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
            {latest.map((m) => (
              <button key={m.source + '|' + m.url} className="manga" onClick={() => loadChapters(m)}>
                {m.coverUrl ? (
                  <img className="thumb" src={m.coverUrl} alt="" loading="lazy" />
                ) : (
                  <span className="thumb" />
                )}
                <span className="label">{m.title}</span>
              </button>
            ))}
          </div>
        )}

        {latest.length === 0 && !loading && !error && (
          <p className="muted">Aucun résultat sur cette source.</p>
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