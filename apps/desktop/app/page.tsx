'use client';

import { useCallback, useEffect, useMemo, useRef, useState, type CSSProperties } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { bandeVirtualisableEst, decalageAncrage, hauteurRendueMesuree, hauteursBande, offsetsBande } from '../lib/bande-hauteurs';
import { open } from '@tauri-apps/plugin-dialog';
import {
  isPermissionGranted,
  requestPermission,
  sendNotification,
} from '@tauri-apps/plugin-notification';

import { useApparence } from './apparence';
import { useI18n, type MessageKey } from './i18n';
import { notifActive, notifie, setNotifActive, texteNotification } from './notification';
import {
  lireDemarrage,
  reglerDemarrage,
  type EtatDemarrage,
} from './demarrage';
import { exporterRapport, purgerJournal } from './diagnostic';

type Source = {
  name: string;
  lang: string;
  /** Vrai pour les sources importées par l'utilisateur (`.js` local). */
  custom?: boolean;
  /** Nom d'affichage libre, distinct de l'identifiant `name`. */
  displayName?: string;
};

/** Reflète `CustomSource` de `store.rs`. Retourné par `lister_sources_custom`
 *  et `importer_source`. */
type CustomSource = {
  id: string;
  name: string;
  lang: string;
  path: string;
  installed_at: number;
};
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

/** Tâche 3.2 — un chapitre tel que le planificateur le rapporte.
 *
 *  Les champs sont en `snake_case` comme le contrat Rust : le moteur sérialise
 *  `Nouveaute`/`Chapitre` directement dans le payload de l'évènement, sans couche
 *  de conversion. Le nom du champ EST donc le nom Rust, sans quoi `titre`
 *  arriverait `undefined`.
 */
type MajChapitre = {
  url: string;
  titre: string;
  numero: number | null;
  depose_le: number | null;
};

/** Un favori qui a reçu des chapitres nouveaux.
 *
 *  `total` et `chapitres` diffèrent quand `tronque` est vrai : `chapitres` est
 *  plafonné côté moteur, `total` est le compte réel. C'est `total` qu'il faut
 *  afficher — `chapitres.length` annoncerait un nombre faux dès qu'un série
 *  dépasse le plafond. Le détail l'affirme explicitement.
 */
type MajNouveaute = {
  source: string;
  manga_url: string;
  titre: string;
  chapitres: MajChapitre[];
  total: number;
  tronque: boolean;
};

type MajRate = {
  source: string;
  manga_url: string;
  motif: string;
};

/** Le payload de `maj://nouveaux-chapitres` : un bilan de passe complet.
 *
 *  `verifies` / `sources` / `ignores` sont pour le résumé de la ligne « dernière
 *  vérification ». `desactive` sert à distinguer « coupée par le réglage » de
 *  « n'a rien trouvé » — sans quoi l'écran afficherait « 0 nouveau » alors que
 *  la veille ne tournait pas du tout.
 */
type MajPasse = {
  nouveaux: MajNouveaute[];
  rates: MajRate[];
  verifies: number;
  sources: number;
  ignores: number;
  desactive: boolean;
};

/** Une entrée du flux, conservée côté interface après la réception.
 *
 *  `vue` est false tant que l'utilisateur n'a pas ouvert la ligne : le flux est
 *  persistant (on peut le rouvrir au redémarrage), mais ouvrir une ligne doit
 *  rester un geste volontaire.
 */
type MajItem = MajNouveaute & { at: number; vue: boolean };

/** Tâche 3.5 — les réglages du planificateur, tels que renvoyés par Rust.
 *
 *  Les bornes (`*_min_s`, `*_max`) voyagent **depuis** le cœur : c'est
 *  `Reglages::bornes()` qui les connaît, donc l'écran ne peut pas proposer une
 *  valeur que le moteur corrigerait en silence. Les recopier ici ferait
 *  diverger les deux à la première modification.
 */
type PlanReglages = {
  periode_s: number;
  delai_source_s: number;
  max_par_passe: number;
  max_sources_par_passe: number;
  actif: boolean;
  periode_min_s: number;
  delai_source_min_s: number;
  max_par_passe_max: number;
  max_sources_max: number;
  dernier_pass: number;
};

/** Écriture **partielle** : seuls les champs fournis voyagent.
 *
 *  Chaque paramètre de la commande est un `Option`, donc un champ omis garde sa
 *  valeur — c'est ce qui permet d'envoyer le seul réglage qu'on change.
 *
 *  **Les clés sont en `camelCase`, volontairement.** Tauri convertit
 *  `camelCase` en `snake_case` côté Rust, et **ignore silencieusement** une clé
 *  déjà en `snake_case`. Mesuré le 2026-10-05 : `periode_s` passait sans erreur
 *  et n'était pas pris — le réglage ne bougeait pas et la commande répondait
 *  quand même avec les anciennes valeurs. Le reste du projet suit déjà cette
 *  convention (`mangaUrl`, `timeoutSecs`, `chapterUrl`).
 */
type PlanPatch = Partial<{
  periodeS: number;
  delaiSourceS: number;
  maxParPasse: number;
  maxSourcesParPasse: number;
  actif: boolean;
}>;

/** Périodes proposées, en secondes. Le plancher est **filtré** à l'affichage
 *  par `plan.periode_min_s` : proposer une valeur que le moteur refuse serait
 *  une interface qui ment.
 */
const PERIODES = [900, 1800, 3600, 3 * 3600, 6 * 3600, 12 * 3600, 86400];
/** Délais par source proposés, en secondes. */
const DELAIS = [5, 10, 20, 30, 60, 120, 300];

/** Durée lisible et **traduite** : « 15 min », « 6 h », « 1 jour ».
 *
 *  La première version renvoyait « 1 jour » en dur, donc l'écran english
 *  affichait « every 1 jour ». Les unités passent par `t`.
 */
function dureeFr(t: ReturnType<typeof useI18n>['t'], s: number): string {
  if (s < 3600) return t('duree.min', { n: Math.round(s / 60) });
  if (s < 86400) {
    const h = Math.round(s / 3600);
    return h === 1 ? t('duree.heure1') : t('duree.heure', { n: h });
  }
  const j = Math.round(s / 86400);
  return j === 1 ? t('duree.jour1') : t('duree.jour', { n: j });
}

/** Tâche 2.4 — ce qu'un import de dossier a produit, tel que renvoyé par Rust.
 *
 *  `refuses` est un tableau de paires `[nom, refus]` : Rust sérialise un
 *  `Vec<(String, Refus)>` en tableau de tableaux, pas en objet.
 *
 *  Le motif est un **code**, pas une phrase : Rust n'a pas à connaître les
 *  langues de l'interface. La version initiale renvoyait du français, et
 *  l'écran anglais affichait « pas un fichier .cbz » au milieu de ses libellés
 *  anglais — mesuré, puis corrigé en code + `detail` technique.
 */
type RefusCode =
  | 'pas-un-cbz'
  | 'archive-illisible'
  | 'aucune-image'
  | 'nom-chapitre'
  | 'collision'
  | 'copie-impossible';

type ImportRapport = {
  dir: string;
  importes: number;
  refuses: [string, { code: RefusCode; detail?: string | null }][];
  titre: string;
};

/** Rend un chemin lisible par un humain.
 *
 *  Rust renvoie le chemin **canonicalisé**, donc préfixé de `\\?\` sur Windows
 *  (forme longue). C'est indispensable en interne — c'est ce qui permet les
 *  chemins longs au-delà de 260 caractères — mais illisible à l'écran, et
 *  l'utilisateur ne doit pas avoir à le savoir pour retrouver son dossier dans
 *  l'explorateur.
 */
function cheminLisible(p: string): string {
  return p.startsWith('\\\\?\\') ? p.slice(4) : p;
}

/** Table **explicite** des motifs d'import, code → clé de traduction.
 *
 *  Une clé construite à la volée (`import.refus.${code}`) aurait deux défauts
 *  mesurés : `i18n:check` ne voit que les appels `t()` littéraux, donc il
 *  signalait les six motifs comme morts ; et un code ajouté côté Rust sans
 *  traduction afficherait la clé brute, sans le moindre avertissement.
 *
 *  Ici le type est `Record<RefusCode, …>` : **un code nouveau ne compile pas**
 *  tant que sa traduction n'est pas branchée. C'est le compilateur qui tranche,
 *  pas une recherche de texte.
 */
const MOTIF_REFUS: Record<RefusCode, Parameters<ReturnType<typeof useI18n>['t']>[0]> = {
  'pas-un-cbz': 'import.refus.pas-un-cbz',
  'archive-illisible': 'import.refus.archive-illisible',
  'aucune-image': 'import.refus.aucune-image',
  'nom-chapitre': 'import.refus.nom-chapitre',
  collision: 'import.refus.collision',
  'copie-impossible': 'import.refus.copie-impossible',
};

/** Traduit un motif d'import. `{detail}` n'existe que pour les refus qui en ont
 *  un — les clés sans paramètre le laissent tel quel.
 */
function motifRefus(
  t: ReturnType<typeof useI18n>['t'],
  r: { code: RefusCode; detail?: string | null },
): string {
  const detail = r.detail ?? '';
  // Un refus sans détail (par exemple « pas un .cbz ») ne doit pas afficher un
  // `{detail}` vide ni un `{nom}` en trop.
  return detail ? t(MOTIF_REFUS[r.code], { detail }) : t(MOTIF_REFUS[r.code], {});
}

/** Une entrée de l'historique de lecture (table `history`). */
type HistoryItem = {  source: string;
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
 
// Bandeau d'avertissement : base en mémoire (repli de secours).
// Affiche un message fixe en haut de la fenêtre tant que la base est en mémoire.
function BaseMemoireBanner({ visible }: { visible: boolean }) {
  const { t } = useI18n();
  if (!visible) return null;
  return (
    <div className="bandeau-memoire" role="alert" aria-live="assertive">
      <span>{t('bandeau.base_en_memoire')}</span>
      <button
        className="bandeau-fermer"
        aria-label={t('notification.fermer')}
        title={t('notification.fermer')}
        onClick={() => window.localStorage.setItem('bandeau_memoire_masque', '1')}
      >
        {'×'}
      </button>
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
            // Attribut stable : un onglet se cible par son id, jamais par son
            // libellé — qui change avec la langue et casse toute sonde (leçon
            // AGENTS.md §14).
            data-tab={tabule.id}
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

/** Les états de série que le moteur énumère, dans l'ordre.
 *
 *  La traduction est dans le dictionnaire (`fiche.etat.*`), pas ici : cette
 *  table ne sert plus qu'à savoir si une clé est traduisible. Mesuré le
 *  2026-10-09 : elle était `Record<string, string>` avec les libellés français
 *  en dur, et `infoRows` était une fonction de module — donc sans `t`.
 *  En anglais, la fiche affichait « Statut / Auteur / Genres » au milieu d'un
 *  reste traduit.
 */
const ETATS_SERIE = [
  'ONGOING',
  'FINISHED',
  'ABANDONED',
  'LICENSED',
  'PUBLISHING_FINISHED',
  'ON_HIATUS',
  'NOT_YET',
  'DISCONTINUED',
  'UNKNOWN',
] as const;

/** Les lignes de la table d'informations : on n'affiche que ce que la source a
 *  réellement donné, une ligne vide est pire qu'une ligne absente. */
function infoRows(
  m: Manga | null,
  t: (k: MessageKey, v?: Record<string, string | number>) => string,
): { label: string; value: string }[] {
  if (!m) return [];
  const rows: { label: string; value: string }[] = [];
  const state = m.state ? ETATS_SERIE.find((e) => e === m.state) : undefined;
  if (state && state !== 'UNKNOWN') {
    rows.push({ label: t('fiche.statut'), value: t(`fiche.etat.${state}`) });
  }
  if (m.authors && m.authors.length > 0) {
    rows.push({ label: t('fiche.auteur'), value: m.authors.join(', ') });
  }
  const genres = m.tags ?? [];
  if (genres.length > 0) {
    rows.push({ label: t('fiche.genres'), value: genres.map((g) => g.title).join(', ') });
  }
  const alt = plainText(m.altTitle);
  if (alt.length > 0) {
    rows.push({ label: t('fiche.titre.alternatif'), value: alt });
  }
  if (m.rating != null && m.rating > 0) {
    rows.push({ label: t('fiche.note'), value: t('fiche.note.valeur', { note: m.rating.toFixed(1) }) });
  }
  rows.push({ label: t('fiche.source'), value: m.source });
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
/** Intervalle **minimal entre deux requêtes** vers la même source.
 *
 *  `RECENT_CONCURRENCY` borne le nombre de requêtes **en vol**, ce qui
 *  n'est pas la même chose que le débit : quatre appels de 0,5 s se
 *  relancent aussitôt. Mesuré le 2026-10-06 avant correction, le journal
 *  du serveur donnait un écart **minimum de 0 ms** sur trois rafales : des
 *  départs simultanés. Or `AGENTS.md` §10 demande de limiter le débit par
 *  source, et un hôte sent les rafales, pas le débit moyen.
 *
 *  Donc on sépare les deux notions : la concurrence reste à 4, et ce délai
 *  borne le débit par hôte.
 *
 *  **Abaissé de 800 à 300 ms le 2026-10-06**, sur plainte de l'utilisateur : à
 *  800 ms, les cartes visibles mettaient 9,4 s à se remplir. Mesuré ensuite :
 *  écart **minimum 248 ms**, médiane 400 ms — donc toujours **aucun départ
 *  simultané**, qui était le défaut réel. Le goulot n'était d'ailleurs pas
 *  l'intervalle : passer de 800 à 300 ms n'avait gagné que 0,7 s, alors que le
 *  tri « visible d'abord » en gagnait 3,6. Ce qui coûte, c'est la latence des
 *  réponses, pas l'intervalle.
 *
 *  Effet secondaire désirable : les cartes se remplissent
 *  **progressivement**, ce que la rafale masquait. Même état que le
 *  squelette : l'utilisateur voit le progrès au lieu d'un gel.
 */
const RECENT_INTERVALLE_MS = 300;

/** Prochain départ autorisé, **par source**. Module-level, donc partagé.
 *
 *  Première version, mesurée fausse : le compteur était local à l'effet des
 *  « derniers chapitres ». Mesuré le 2026-10-06 sur le journal du serveur,
 *  MANGADEX était bien à 800 ms (806, 793, 796, 799)… mais une autre source
 *  finissait sur **200 ms**. Cause : dès que l'effet se relance (changement de
 *  source, `latest` qui change), le compteur repart de zéro et un second flux
 *  frappe en parallèle. Deux horloges, aucune garantie.
 *
 *  D'où le niveau module : la politesse est une propriété de **l'hôte**, pas
 *  d'un effet React. Une seule horloge par source, donc un seul flux.
 *
 *  `Map` et non objet : une source s'appelle `__proto__` sur une source
 *  malveillante, pas sur un `Object.create(null)`.
 */
const prochainDepartParSource = new Map<string, number>();

/**
 * Attend son tour pour une requête **automatique** vers `source`.
 *
 *  Réservé au trafic de fond. Un clic de l'utilisateur — ouvrir une fiche, un
 *  signet, un chapitre — **n'attend pas** : faire attendre quelqu'un qui a
 *  cliqué pour rester poli envers le site serait l'inverse du but. Seules les
 *  requêtes que personne n'a demandées passent par ici.
 *
 *  La lecture puis l'écriture se font sans `await` entre les deux : le code
 *  JavaScript est mono-thread, donc deux.work Workers ne peuvent pas lire la
 *  même valeur et partir ensemble.
 */
async function attendreTourAutomatique(source: string): Promise<void> {
  const maintenant = Date.now();
  const depart = Math.max(maintenant, prochainDepartParSource.get(source) ?? 0);
  prochainDepartParSource.set(source, depart + RECENT_INTERVALLE_MS);
  const attente = depart - maintenant;
  if (attente > 0) await new Promise((r) => setTimeout(r, attente));
}
/** Plafond d'entrées du cache des « derniers chapitres ».
 *
 *  Le TTL purge l'essentiel ; ce plafond est le filet de sécurité pour qu'un
 *  cache ne puisse pas croître sans limite (voir `purgerCacheRecent`).
 */
const MAX_ENTREES_RECENT = 500;

type RecentEntry = { labels: string[]; total: number; at: number };

/** Cache des « derniers chapitres » des cartes, avec purge des entrées mortes.
 *
 *  Le TTL existait **à la lecture** depuis le début, mais rien ne purgeait à
 *  l'écriture. Mesuré le 2026-10-06 : `tatami.recent` contenait **443 entrées
 *  dont 422 expirées**, 57 Ko — donc 95 % du cache ne servait plus à rien et
 *  continuait d'être réécrit à chaque série consultée.
 *
 *  La purge est ici, dans `saveRecentCache`, et nulle part ailleurs : c'est le
 *  seul endroit qui décide de la taille du cache. Le faire à la lecture aurait
 *  été prématuré — un cache n'a pas à être purgé pour être lu, et cela coûterait
 *  un `localStorage.setItem` par simple affichage.
 */
function loadRecentCache(): Record<string, RecentEntry> {
  try {
    const raw = window.localStorage.getItem(RECENT_KEY);
    return raw ? (JSON.parse(raw) as Record<string, RecentEntry>) : {};
  } catch {
    return {};
  }
}

/** Retire les entrées trop vieilles, et borne le reste.
 *
 *  La borne est le filet de sécurité : même si le TTL change un jour, le cache
 *  ne peut pas croître sans limite. Mesuré : ~0,13 Ko par entrée, donc
 *  500 entrées ≈ 64 Ko, très en dessous du quota de ~5 Mo — mais un cache non
 *  borné finit toujours par le toucher, sur une machine qui browse beaucoup.
 */
function purgerCacheRecent(cache: Record<string, RecentEntry>): Record<string, RecentEntry> {
  const maintenant = Date.now();
  const vivants: [string, RecentEntry][] = [];
  for (const [cle, entree] of Object.entries(cache)) {
    if (maintenant - (entree.at ?? 0) <= RECENT_TTL) vivants.push([cle, entree]);
  }
  // Les plus récentes d'abord, puis on garde les `MAX_ENTREES_RECENT` premières.
  vivants.sort((a, b) => (b[1].at ?? 0) - (a[1].at ?? 0));
  return Object.fromEntries(vivants.slice(0, MAX_ENTREES_RECENT));
}

function saveRecentCache(cache: Record<string, RecentEntry>) {
  try {
    window.localStorage.setItem(RECENT_KEY, JSON.stringify(purgerCacheRecent(cache)));
  } catch {
    /* quota atteint : le cache est un confort, pas une donnée */
  }
}

/** File de téléchargement persistée : ce qui reste à faire si l'app s'arrête. */
type SavedQueue = { items: Manga[]; startedAt: number };
const QUEUE_KEY = 'tatami.queue';
const READER_ZOOM_KEY = 'tatami.readerZoom';

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

/** Tâche 3.2 — le flux est une préférence d'affichage *et* un journal court : il
 *  vit dans `localStorage`, pas en base. Même règle que `HEALTH_KEY`.
 *
 *  On ne garde que les entrées non vidées et on plafonne : le flux s'accumule à
 *  chaque passe, et une liste de plusieurs centaines dans `localStorage` est une
 *  base qui déborde. 200 entrées de quelques centaines d'octets chacune restent
 *  sous le quota, et au-delà le tri par date écarte les plus anciennes.
 */
const MAJ_KEY = 'tatami.maj';
const MAJ_MAX = 200;

function loadMaj(): MajItem[] {
  if (typeof window === 'undefined') return [];
  try {
    const brut = JSON.parse(window.localStorage.getItem(MAJ_KEY) || '[]');
    if (!Array.isArray(brut)) return [];
    return brut
      .filter((x) => x && typeof x.source === 'string' && typeof x.titre === 'string')
      .filter((x) => (x.total ?? 0) > 0)
      .slice(0, MAJ_MAX)
      .map((x) => ({
        ...x,
        chapitres: Array.isArray(x.chapitres) ? x.chapitres : [],
        total: typeof x.total === 'number' ? x.total : 0,
        tronque: x.tronque === true,
        at: typeof x.at === 'number' ? x.at : Date.now(),
        vue: x.vue === true,
      }));
  } catch {
    return [];
  }
}

function saveMaj(items: MajItem[]) {
  try {
    window.localStorage.setItem(MAJ_KEY, JSON.stringify(items.slice(0, MAJ_MAX)));
  } catch {
    // Quota dépassé ou `localStorage` indisponible : le flux reste en mémoire,
    // il ne fait pas planter l'application pour une sauvegarde.
  }
}
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
  const [progressParSource, setProgressParSource] = useState<Map<string, Progress>>(() => new Map());
  const [tab, setTab] = useState<string>('home');
  // Tâche 3.2 — le flux « mises à jour ». État propre : il est relu au
  // démarrage depuis `localStorage`, donc il survit à un redémarrage. C'est
  // ce que la passe émet ; l'interface ne la déclenche pas.
  //
  // **Pas `loadMaj()` dans l'initialiseur.** Mesuré le 2026-10-06 : l'export
  // statique produit un HTML serveur qui ne peut pas contenir la valeur du
  // `localStorage` de la machine. `loadMaj()` renvoyait donc `[]` côté serveur
  // (son garde `typeof window`) et la vraie liste côté client — donc
  // `<section class="maj">` existait d'un seul côté, et React abandonnait
  // l'hydratation de **tout l'arbre** :
  //
  //   Hydration failed …  + <section className="maj">  /  - <div className="band">
  //
  // Ce n'est pas une alerte cosmétique : abandonner l'hydratation fait
  // reconstruire la page entière sur le client, ce qui est précisément le
  // chargement qu'on cherche à=alléger. La lecture se fait donc dans l'effet
  // de montage, comme les préférences du lecteur (même raison, même endroit).
  const [maj, setMaj] = useState<MajItem[]>([]);
  // Tâche 3.5 — réglages du planificateur. `null` = pas encore chargé, ce qui
  // n'est pas la même chose que « absent » : l'écran affiche alors « chargement »
  // et non des valeurs par défaut inventées.
  const [plan, setPlan] = useState<PlanReglages | null>(null);
  const [planErreur, setPlanErreur] = useState<string | null>(null);
  // Tâche 3.3 — l'état du réglage est lu **une fois** au montage et recopié ici.
  // Le recopier dans un état le ferait diverger de `localStorage` dès qu'une
  // autre fenêtre (ou un walk de mesure) écrit la clé : l'interrupteur
  // afficherait alors « actif » pendant que plus rien n'est envoyé.
  //
  // Même raison que `maj` ci-dessus : `notifActive()` lit le `localStorage`, donc
  // le serveur ne peut pas connaître sa valeur. On part de la **valeur par
  // défaut** (`true`) des deux côtés — sans scintillement, parce que ce
  // interrupteur n'existe que dans l'écran des réglages, que l'utilisateur doit
  // ouvrir : l'effet de montage a donc déjà fait son travail avant le premier
  // rendu de cet écran.
  const [notif, setNotif] = useState(true);
  // Tâche 3.4 — démarrage automatique. `inconnu` au premier rendu : afficher
  // `inactif` avant la lecture du système ferait clignoter l'interrupteur à
  // chaque ouverture de l'écran des réglages.
  const [demarrage, setDemarrage] = useState<EtatDemarrage>('inconnu');
  const [baseEnMemoire, setBaseEnMemoire] = useState(false);
  // Ce qui n'a pas encore été regardé. Une valeur dérivée, pas un second état :
  // deux compteurs qui peuvent divergir sont deux bugs en attente.
  const majNonVus = maj.filter((m) => !m.vue).length;
  // Bibliothèque personnelle : favoris, historique, reprise (table SQLite).
  const [favorites, setFavorites] = useState<Favorite[]>([]);
  const [recent, setRecent] = useState<HistoryItem[]>([]);
  /* Chargement de l'accueil et de la bibliothèque.
   *
   * Ces deux états ont été ajoutés le 2026-10-06 après mesure, contre un
   * commentaire qui affirmait que la lecture locale « dure moins d'une image ».
   * Mesuré sur l'application réelle, chronologie échantillonnée à 120 ms :
   * squelette de 0 à **295 ms**, contenu réel à **436 ms** — et
   * **jamais** de « aucun favori » affiché. En incluant le rechargement
   * complet de la page, l'accueil est prêt en 1,4 s. */
  const [shelfCharge, setShelfCharge] = useState(true);
  const [bibliothequeCharge, setBibliothequeCharge] = useState(true);
  // Mode incognito : les titres classés adultes disparaissent de l'accueil.
  // Préférence d'affichage, donc `localStorage` est le bon endroit (cf. §6).
  const [incognito, setIncognito] = useState(true);
  const [library, setLibrary] = useState<LibraryItem[]>([]);
  // Tâche 2.4 — dernier rapport d'import, affiché tel quel. `refuses` porte
  // un motif par fichier : sans lui, l'utilisateur ne sait pas *pourquoi* un
  // chapitre de son dossier manque après l'import.
  const [importRapport, setImportRapport] = useState<ImportRapport | null>(null);
  const [importEnCours, setImportEnCours] = useState(false);
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
  // Phase S, S2.1 : sources personnalisées (scripts `.js` importés par
  // l'utilisateur). Liste vide au premier rendu — le panneau des réglages la
  // charge à son ouverture (effet sur `tab`).
  const [sourcesPerso, setSourcesPerso] = useState<CustomSource[]>([]);
  const [importPersoEnCours, setImportPersoEnCours] = useState(false);
  // Identifiant de la source en attente de confirmation. `null` = aucune.
  const [aSupprimerPerso, setASupprimerPerso] = useState<string | null>(null);
  // S2.3 — les sources importées **telles que le moteur les voit** (donc
  // filtrables par langue, avec leur nom lisible), à distinguer de
  // `sourcesPerso` qui est la liste de la base, celle des réglages.
  const sourcesImportees = useMemo(() => sources.filter((s) => s.custom), [sources]);
  // La tuile « sources personnalisées » est une entrée du catalogue parmi les
  // autres, pas un encart à part (demande du 2026-10-09). Le clic ouvre donc une
  // vue dédiée, sur le modèle exact de `lang` : tuile → vue.
  //
  // L'état vit ici, **pas** dans le rendu du catalogue : le composant retourne
  // avant d'atteindre le bloc du lecteur selon la vue affichée, donc un
  // `useState` déclaré plus bas serait `undefined` à la lecture.
  const [vueSourcesPerso, setVueSourcesPerso] = useState(false);
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
  // Confort de lecture : largeur de la bande.
  const [zoom, setZoom] = useState(100);
  // Le lecteur est en mode webtoon, c'est le seul qu'on offre. Le mode
  // « pagé », le double écran et l'ajustement hauteur ont été jugés inutiles
  // sur le retour de l'usage : on garde uniquement la bande verticale.
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
  /** Index des pages que la source ne sert pas, dans le chapitre ouvert.
   *
   *  Ce n'est pas rare, et ce n'est pas notre défaut : mesuré le 2026-10-09 sur
   *  `custom:mangadex-api` (Dragon Ball, chapitres FR), **par le même code et
   *  dans la même seconde** :
   *
   *  | chapitre | pages annoncées | servies |
   *  |---|---|---|
   *  | ch. 1    | 32               | **7/8** sondées, ~200 Ko de JPEG |
   *  | ch. 3    | 15               | **0/8** sondées, toutes en 404 |
   *
   *  MangaDex sert certains chapitres et en perd d'autres. Sans cet état, la
   *  page manquante s'affichait en `<img>` cassée **sans aucun message**, et
   *  l'utilisateur lisait « MangaDex ne sert pas cette page » comme « l'app
   *  est cassée » — ce qu'aucun pixel ne permet de distinguer. */
  const [pagesEnErreur, setPagesEnErreur] = useState<Set<number>>(new Set());
  /** Dimensions réelles des pages, mesurées par le navigateur à leur chargement.
   *
   *  Rust ne connaît les dimensions que des pages **déjà en cache**
   *  (`commands.rs:304`) : un chapitre ouvert pour la première fois n'en a donc
   *  aucune. Mesuré le 2026-10-09 sur TOOMICSFR, c'était la cause de l'étirement
   *  — 6 images sur 6 déformées de 60,9 %, parce que toutes retombaient sur le
   *  repli `1600/900` alors que la vraie page mesurait 720x500.
   *
   *  Le navigateur, lui, a toujours le ratio : `naturalWidth`/`naturalHeight`.
   *  On le remonte donc au fil de l'eau, et la bande se recale toute seule.
   *
   *  Clé = index de page, valeur = dimensions **en pixels d'image** (pas CSS).
   *  `null` = pas encore mesurée. */
  const [dimsPages, setDimsPages] = useState<Record<number, { w: number; h: number }>>({});
  /** Note les dimensions d'une page et **ne bouge que si elles changent**.
   *
   *  Sans cette garde, chaque `onLoad` ferait un `setState` donc un rendu, et le
   *  rendu recharge l'image qui redéclenche `onLoad` : une boucle de rendus dont
   *  rien ne prouve qu'elle s'arrêterait. La comparaison rend l'opération
   *  idempotente, donc elle se stabilise au bout d'un rendu par page. */
  const noterDims = useCallback((i: number, w: number, h: number) => {
    if (w <= 1 || h <= 1) return;
    setDimsPages((prec) => {
      const avant = prec[i];
      if (avant && avant.w === w && avant.h === h) return prec;
      return { ...prec, [i]: { w, h } };
    });
  }, []);
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

  /** Recharge la liste des sources du moteur.
   *
   *  Mémorisée **sans dépendance** : l'action d'import s'en sert, et une
   *  identité recréée à chaque rendu la relancerait en boucle (leçon AGENTS.md
   *  §14 sur les dépendances d'effet recréées).
   */
  const chargerSources = useCallback(() => {
    invoke<{ sources: Source[] }>('list_sources')
      .then((r) => setSources(r.sources ?? []))
      .catch((e) => setError(String(e)));
  }, []);

  /*
   * Miroir de `t` pour les écouteurs d'évènements.
   *
   * `useI18n()` rend `t` sous forme de fonction **recréée à chaque rendu**. Un
   * écouteur abonné une seule fois (`[]`) garde donc la closure du rendu de
   * montage : tout texte qu'il produit reste dans la langue de ce moment-là.
   * Mesuré le 2026-10-08 sur les notifications « N nouveaux chapitres ».
   *
   * On ne peut pas mettre `langue` dans le tableau de dépendances de l'effet
   * qui l'abonne : cet effet recharge aussi les téléchargements et les sources,
   * donc le rejouer à chaque changement de langue réabonnerait trois écouteurs
   * et relancerait deux requêtes. D'où le miroir, mis à jour par un effet
   * séparé qui ne dépend que du **primitif** `langue`.
   */
  const tCourant = useRef(t);
  useEffect(() => {
    tCourant.current = t;
    // `t` est volontairement absent : il est recréé à chaque rendu, donc le
    // l'ajouter relancerait ce miroir à chaque rendu — l'inverse de l'effet
    // recherché. La dépendance utile est le primitif `langue`, qui change
    // exactement quand la traduction change.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [langue]);

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
    // Vérification base en mémoire : si le dossier de données est verrouillé,
    // Rust a basculé sur une base en mémoire — rien ne persistera.
    void invoke<boolean>('est_base_en_memoire')
      .then(setBaseEnMemoire)
      .catch(() => {
        /* silencieux : si la commande échoue, on assume base persistante */
      });
    const modeIncognito = window.localStorage.getItem(INCOGNITO_KEY);
    if (modeIncognito != null) setIncognito(modeIncognito === '1');
    // Lecture des préférences après l'hydratation : en export statique le HTML
    // est produit au build, il ne peut donc pas contenir la valeur stockée sur
    // la machine de l'utilisateur. Les poser dans un initialiseur paresseux
    // créerait un écart serveur/client ; l'effet est ici le bon moment.
    if (typeof window !== 'undefined') {
      setZoom(Number(window.localStorage.getItem(READER_ZOOM_KEY)) || 100);
      // Même raison pour ces deux-là : leur valeur vit dans le navigateur, donc
      // le HTML statique du serveur ne peut pas la contenir.
      setMaj(loadMaj());
      setNotif(notifActive());
    }
    chargerSources();
    refreshDownloads();
    const unlisten = listen<Progress>('download://progress', (event) => {
      // Deux téléchargements actifs = deux sources distinctes (cf. verrou
      // Rust), donc la source est une clé sans collision.
      setProgressParSource((prev) => {
        const next = new Map(prev);
        next.set(event.payload.source, event.payload);
        return next;
      });
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
    // Tâche 3.2 — le flux « mises à jour ». Le planificateur émet
    // `maj://nouveaux-chapitres` avec un bilan de passe, et **rien** quand il
    // n'y a rien de neuf : on ne se réabonne donc pas pour rien.
    //
    // Une passe remplace le flux précédent plutôt que de s'y ajouter : le même
    // favori peut être signalé à chaque passe tant que ses chapitres sont dans
    // la fenêtre « non lus » de la source, et empiler créerait des doublons.
    // On fusionne donc par `(source, manga_url)`, en gardant la date la plus
    // récente et en cumulant les totaux — deux '_total_' distincts pour le même
    // favori sont des chapitres différents, pas les mêmes comptés deux fois.
    const unlistenMaj = listen<MajPasse>('maj://nouveaux-chapitres', (event) => {
      const passe = event.payload;
      if (!passe || !Array.isArray(passe.nouveaux)) return;
      const maintenant = Date.now();

      // Tâche 3.3 — une notification **par passe**, pas par chapitre. Une passe
      // peut en trouver 12 dans 5 séries ; douze alertes d'un coup sont pires
      // qu'aucune. Le rejet est sans conséquence ici : le flux « mises à »
      // n'en dépend pas, et notifier est un bonus.
      // `tCourant`, pas `t` : cet effet est abonné **une fois** (tableau `[]`,
      // pour ne pas réabonner à chaque `refreshDownloads`), donc sa closure
      // garde la fonction de traduction du rendu de montage. Mesuré le
      // 2026-10-08 : après un basculement FR → EN, toutes les notifications
      // « 3 nouveaux chapitres » restaient en français. La porte
      // `hooks:check` ne le voit pas, car elle cherche `t` **dans** le tableau
      // de dépendances — qui est vide.
      void notifie(texteNotification(tCourant.current, passe.nouveaux));

      setMaj((precedent) => {
        const cumul = new Map<string, MajItem>();
        for (const item of precedent) {
          cumul.set(item.source + '\u001f' + item.manga_url, item);
        }
        for (const n of passe.nouveaux) {
          if (!n || typeof n.source !== 'string' || (n.total ?? 0) <= 0) continue;
          const cle = n.source + '\u001f' + n.manga_url;
          const deja = cumul.get(cle);
          cumul.set(cle, {
            ...n,
            source: n.source,
            manga_url: n.manga_url,
            titre: n.titre || deja?.titre || n.source,
            chapitres: Array.isArray(n.chapitres) ? n.chapitres : [],
            total: (deja?.total ?? 0) + (n.total ?? 0),
            tronque: n.tronque === true || deja?.tronque === true,
            at: maintenant,
            vue: false,
          });
        }
        const suivant = [...cumul.values()].sort((a, b) => b.at - a.at).slice(0, MAJ_MAX);
        saveMaj(suivant);
        return suivant;
      });
    });
    return () => {
      unlisten.then((off) => off()).catch(() => {});
      unlistenVerif.then((off) => off()).catch(() => {});
      unlistenMaj.then((off) => off()).catch(() => {});
    };
    // Montage uniquement : rejouer cet effet à chaque changement de
    // `refreshDownloads` réabonnerait l'écouteur d'événements.
    }, []);
  /* eslint-enable react-hooks/set-state-in-effect */

  /** Tâche 3.5 — lit les réglages du planificateur.
   *
   *  Volontairement **sans** rejeu automatique dans l'effet de montage : les
   *  régler ne concerne que l'écran Réglages, et une requête inutile au
   *  démarrage pour un écran rarement ouvert n'est pas un détail (le moteur est
   *  séquentiel, il sert d'abord l'utilisateur).
   */
  /** Tâche 3.4 — écrit le démarrage automatique, puis **relit**.
   *
   *  `reglerDemarrage` renvoie déjà l'état obtenu ; on ne devine donc jamais le
   *  résultat localement. Si le système refuse, l'interrupteur revient à sa
   *  position réelle — coché pour rien serait le pire résultat.
   */
  const basculerDemarrage = useCallback(async (actif: boolean) => {
    setDemarrage('inconnu');
    setDemarrage(await reglerDemarrage(actif));
  }, []);

  /** Tâche 5.5 — ce que l'écran affiche **après** l'action.
   *
   *  Un seul champ `null` = « rien à afficher ». Les deux autres sont
   *  volontairement distincts : un succès et une erreur qui afficheraient le même
   *  gris seraient indiscernables, et c'est le cas typique quand on n'affiche que
   *  « terminé ».
   */
  const [journalOccupe, setJournalOccupe] = useState<'export' | 'purge' | null>(null);
  const [journalMessage, setJournalMessage] = useState<string | null>(null);
  const [journalErreur, setJournalErreur] = useState<string | null>(null);

  /** Tâche 5.5 — exporte, puis affiche le chemin **que le cœur a réellement écrit**.
   *
   *  Le chemin vient du retour de la commande, pas d'une concaténation locale :
   *  afficher un chemin fabriqué serait afficher un chemin possiblement faux.
   */
  const lancerExport = useCallback(async () => {
    setJournalOccupe('export');
    setJournalMessage(null);
    setJournalErreur(null);
    try {
      const chemin = await exporterRapport(langue);
      setJournalMessage(t('journal.fait', { chemin }));
    } catch (e) {
      setJournalErreur(t('journal.echec', { raison: String(e).slice(0, 160) }));
    } finally {
      setJournalOccupe(null);
    }
    // `langue` est une dépendance **réelle** : sans elle, changer de langue
    // laisserait le rapport exported dans la langue précédente. L'avertissement
    // `exhaustive-deps` signalait un vrai oubli, pas du bruit.
  }, [t, langue]);

  /** Tâche 5.5 — supprime, puis affiche **combien** de fichiers sont partis.
   *
   *  `0` n'est pas une erreur : c'est l'état déjà propre. Les deux cas ont donc
   *  leur propre message, sinon « rien » et « impossible » se ressembleraient.
   */
  const lancerPurge = useCallback(async () => {
    setJournalOccupe('purge');
    setJournalMessage(null);
    setJournalErreur(null);
    try {
      const n = await purgerJournal();
      setJournalMessage(n > 0 ? t('journal.purge.ok', { n }) : t('journal.purge.rien'));
    } catch (e) {
      setJournalErreur(t('journal.echec', { raison: String(e).slice(0, 160) }));
    } finally {
      setJournalOccupe(null);
    }
  }, [t]);

  /** Tâche 3.4 — pousse les libellés du menu de zone vers Rust.
   *
   *  Le menu est créé par le **cœur** (pour qu'il existe même si la webview
   *  échoue), donc ses textes ne peuvent pas être traduits là-bas : ils viennent
   *  d'ici. Envoyés au démarrage et à chaque changement de langue.
   *
   *  L'échec est **ignoré** : un menu resté en français vaut mieux qu'une
   *  exception au montage qui empêcherait le reste de l'écran de s'afficher.
   *
   *  La commande **renvoie les textes relus depuis le menu** : un simple `Ok`
   *  ne prouverait que la réception. Le retour va dans la console, ce qui donne
   *  un moyen de vérifier sans ajouter `core:menu:allow-get-text` aux
   *  capabilities — que la règle « strict minimum » interdit d'ajouter pour les
   *  besoins d'une mesure.
   *
   *  Dépendance : **`langue`, pas `t`.** Mesuré le 2026-10-05 : `t` est une
   *  fonction recréée à chaque rendu, donc `[t]` déclenchait l'effet **à chaque
   *  rendu** — 18 appels de `set_libelles` au seul démarrage, chacun suivi d'un
   *  `lireDemarrage()`. Le libellé ne dépend que de la langue, donc c'est la
   *  seule dépendance qui compte.
   */
  useEffect(() => {
    invoke<[string, string]>('set_libelles', {
      ouvrir: t('zone.ouvrir'),
      quitter: t('zone.quitter'),
    })
      .then((lus) => console.info('menu de zone', { ouvrir: lus[0], quitter: lus[1] }))
      .catch(() => {});
    // Le démarrage automatique est relu à chaque changement de langue : il peut
    // avoir été modifié ailleurs (gestionnaire de tâches, Paramètres Windows), et
    // un état figé afficherait le contraire de la réalité.
    void lireDemarrage().then(setDemarrage);
    // La directive doit être sur la ligne **immédiatement** précédente : elle ne
    // couvre que celle d'après, donc un commentaire de plusieurs lignes entre les
    // deux la rend « inutilisée » tout en laissant l'avertissement réapparaître
    // trois lignes plus bas. La raison est dans le commentaire de l'effet.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [langue]);

  const chargerPlan = useCallback(() => {
    invoke<PlanReglages>('reglages_planificateur')
      .then((r) => {
        setPlan(r);
        setPlanErreur(null);
      })
      .catch((e) => setPlanErreur(t('plan.erreur', { detail: String(e) })));
    // `t` change avec la langue : sans lui, un message d'erreur resterait dans
    // la langue de l'ouverture de l'écran.
  }, [t]);

  /** Écrit un **sous-ensemble** des réglages, puis remplace l'état par ce que
   *  Rust renvoie.
   *
   *  On ne devine pas la nouvelle valeur localement : le cœur applique les
   *  bornes, donc une valeur affichée avant sa relecture serait parfois fausse.
   */
  const majPlan = useCallback(
    (patch: PlanPatch) => {
      // Un champ numérique vidé donne `Number('') === NaN`, et `NaN` sérialisé
      // par IPC devient `null` — donc « plateau nul », pas « inchangé ». Un
      // simple effacement de la frappe ferait donc passer le planificateur à
      // 1 favori vérifié par passe. On ignore la saisie tant qu'elle n'est pas
      // un entier fini, et l'affichage reste la valeur relue.
      const propre: PlanPatch = { ...patch };
      if (typeof patch.maxParPasse === 'number') {
        delete propre.maxParPasse;
        if (Number.isFinite(patch.maxParPasse) && patch.maxParPasse >= 1) {
          propre.maxParPasse = Math.round(patch.maxParPasse);
        }
      }
      if (typeof patch.maxSourcesParPasse === 'number') {
        delete propre.maxSourcesParPasse;
        if (Number.isFinite(patch.maxSourcesParPasse) && patch.maxSourcesParPasse >= 1) {
          propre.maxSourcesParPasse = Math.round(patch.maxSourcesParPasse);
        }
      }
      if (Object.keys(propre).length === 0) return;

      setPlanErreur(null);
      invoke<PlanReglages>('set_reglages_planificateur', propre)
        .then((r) => setPlan(r))
        .catch((e) => {
          setPlanErreur(t('plan.erreur', { detail: String(e) }));
          // On relit : l'écran doit revenir à l'état réel plutôt que garder une
          // valeur que la base n'a jamais portée.
          chargerPlan();
        });
    },
    [t, chargerPlan],
  );

  const refreshDownloads = useCallback(() => {
    setBibliothequeCharge(true);
    invoke<LibraryItem[]>('list_library', { root: '' })
      .then((items) => setLibrary(items ?? []))
      .catch(() => setLibrary([]))
      .finally(() => setBibliothequeCharge(false));
  }, []);

  /** Tâche 2.4 — importe un dossier de `.cbz` déjà téléchargés.
   *
   *  Le dialogue ne fait que choisir un chemin : c'est Rust qui décide de ce
   *  qui est copié, de ce qui est refusé et où ça atterrit. Le dialogue natif
   *  n'est pas testable par CDP (leçon AGENTS.md §14), donc **seule** la
   *  commande est vérifiée de bout en bout.
   *
   *  Le rapport est affiché tel quel, refus compris : afficher « 12 chapitres
   *  importés » sans dire que 3 fichiers ont été écartés serait un compte
   *  flatteur, et l'utilisateur croirait avoir tout son dossier.
   */
  async function importerDossier() {
    setImportEnCours(true);
    setImportRapport(null);
    try {
      const choisi = await open({
        directory: true,
        multiple: false,
        title: t('import.dossier.choisir'),
      });
      if (typeof choisi !== 'string') return; // annulé
      const rapport = await invoke<ImportRapport>('import_webtoon', { dir: choisi });
      setImportRapport(rapport);
      await refreshDownloads();
    } catch (e) {
      setError(t('import.erreur', { raison: String(e) }));
    } finally {
      setImportEnCours(false);
    }
  }

  /** S2.1 — charge les scripts importés. Mémorisée **sans dépendance** : l'effet
   *  qui suit s'en sert, et une identité recréée à chaque rendu le relancerait
   *  en boucle (leçon AGENTS.md §14 sur les dépendances d'effet recréées).
   */
  const chargerSourcesPerso = useCallback(() => {
    invoke<CustomSource[]>('lister_sources_custom')
      .then((cs) => setSourcesPerso(cs ?? []))
      .catch(() => setSourcesPerso([]));
  }, []);

  // Le panneau n'existe que dans la vue Réglages : on charge à son ouverture.
  // On dépend du **primitif** `tab`, jamais de `t` (même raison).
  useEffect(() => {
    if (tab !== 'settings') return;
    void chargerSourcesPerso();
  }, [tab, chargerSourcesPerso]);

  /** S2.1 — importe un script `.js` choisi par le dialogue natif.
   *
   *  Le dialogue ne fait que choisir un chemin : c'est Rust qui valide la
   *  taille (256 Ko), assainit le nom et décide de la destination — même
   *  contrat que l'import de dossiers `.cbz` ci-dessus.
   */
  async function importerSourcePerso() {
    setImportPersoEnCours(true);
    try {
      const choisi = await open({
        multiple: false,
        title: t('sources.perso.importer'),
        filters: [{ name: 'JavaScript', extensions: ['js'] }],
      });
      if (typeof choisi !== 'string') return; // annulé
      const cs = await invoke<CustomSource>('importer_source', { path: choisi });
      setNotice(t('sources.perso.ok', { nom: cs.name }));
      chargerSourcesPerso();
      // Le moteur ne voit pas le fichier dès la fin de la copie : le
      // `ScriptedSourceLoader` le repère au WatchService puis l'exécute.
      // Mesuré le 2026-07 : la tuile apparaît **~580 ms** après la copie (la
      // première tentative est refusée car le fichier est encore en cours
      // d'écriture, l'événement MODIFY suivant réussit ~1 s après). Un seul
      // rafraîchissement immédiat peut donc la rater : on réessaie à 1,5 s
      // puis 4 s.
      chargerSources();
      window.setTimeout(chargerSources, 1500);
      window.setTimeout(chargerSources, 4000);
    } catch (e) {
      setError(t('sources.perso.erreur', { raison: String(e) }));
    } finally {
      setImportPersoEnCours(false);
      setASupprimerPerso(null);
    }
  }

  /** S2.1 — supprime une source après confirmation.
   *
   *  Pas de mise à jour optimiste, contrairement aux catégories : la commande
   *  efface le `.js` du disque. Retirer la ligne avant l'appel afficherait un
   *  état qui n'existe pas.
   */
  async function supprimerSourcePerso(cs: CustomSource) {
    try {
      await invoke('supprimer_source_custom', { id: cs.id });
      setNotice(t('sources.perso.suppr.ok', { nom: cs.name }));
    } catch (e) {
      setError(t('sources.perso.suppr.erreur', { raison: String(e) }));
    } finally {
      setASupprimerPerso(null);
      chargerSourcesPerso();
    }
  }

  /** Recharge favoris et historique. `hideAdult` explicite parce que l'appel
   *  déclenché par l'interrupteur arrive AVANT que l'état change : sans
   *  paramètre, la closure renverrait l'ancienne valeur et l'historique
   *  resterait filtré (mesuré : la liste restait vide après désactivation).
   */
  const refreshShelf = useCallback(
    async (hideAdult?: boolean) => {
      const hide = hideAdult ?? incognito;
      // Le commentaire disait « pas d'indicateur de chargement : la lecture est
      // un SQLite local, elle dure moins d'une image ». **L'hypothèse était
      // fausse** : mesuré le 2026-10-06, le squelette tient 295 ms puis les
      // favoris sont là à 436 ms. Sans cet état, l'écran affiche « aucun
      // favori » pendant tout ce temps — un message qui **contredit** celui
      // qui arrive juste après, et que l'utilisateur lit comme une perte.
      setShelfCharge(true);
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
      } finally {
        setShelfCharge(false);
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
      // Tâche 3.5 : les réglages du planificateur sont relus **à l'ouverture**
      // de l'onglet, pas au montage. L'application passe l'essentiel de son
      // temps ailleurs, et le moteur est séquentiel : une requête au démarrage
      // pour un écran rarement ouvert vole un tour aux listes de l'utilisateur.
      // Le refus est ignoré : il n'y a rien à afficher si l'écran est fermé.
      if (id === 'settings') chargerPlan();
      // Changer d'onglet quitte la vue en cours (source, fiche) : sinon
      // l'ancien écran, rendu avant l'accueil, resterait affiché par-dessus.
      setOpenManga(null);
      if (id !== 'catalog') setSelected(null);
      setTab(id);
    },
    [refreshDownloads, refreshShelf, refreshSignets, chargerPlan],
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

  /** Tâche 3.2 — ouvre la fiche d'un favori signalé par le planificateur.
   *
   *  On délègue à `loadChapters`, qui pose déjà `setOpenManga` et appelle
   *  `details` + `chapters`. J'avais d'abord écrit une version dédiée avec les
   *  deux appels `Promise.allSettled` — c'était un doublon : le même enchaînement
   *  existe déjà, et ma version inventait un `setChapitresCharges` qui n'existe
   *  pas. Le compilateur l'a dit ; autant l'écouter que le redécouvrir plus tard.
   */
  function ouvrirFavori(f: Favorite) {
    void loadChapters({
      url: f.manga_url,
      title: f.title,
      source: f.source,
      coverUrl: f.cover_url,
      coverToken: f.cover_token,
    });
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
  // `lang === 'perso'` n'est pas une langue : c'est la vue des sources
    // personnalisées (tuile du catalogue, ouverte le 2026-10-09). Elle affiche
    // les sources importées **toutes langues confondues** — les filtrer par
    // `s.lang` viderait la vue, puisque leur langue est `ja`, `fr`… et jamais
    // `perso`.
    const byLang =
    lang === 'all'
      ? visibleSources
      : lang === 'perso'
        ? visibleSources.filter((s) => s.custom)
        : visibleSources.filter((s) => s.lang === lang);
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

  // `perso` est une **vue** comme une langue, pas une tuile de plus : le clic sur
    // la tuile doit mener à la LISTE, pas redessiner la grille. Mesuré le
    // 2026-10-09 : en le laissant dans `enTiles`, le clic changeait bien `lang`
    // (le chip actif passait à aucun) mais la grille restait affichée — donc la
    // tuile ne menait nulle part.
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

  // Changer de vue repart du haut. Mesuré le 2026-07 : le défilement de la vue
  // précédente était conservé — scrollY 787 sur la grille des mangas, 432 après
  // le clic sur un webtoon, entête hors champ. Avec une longue liste de
  // chapitres on atterrit au milieu, et la fiche (résumé, genres) n'est jamais
  // vue : l'utilisateur a l'impression que les chapitres se sont affichés à la
  // place des webtoons.
  useEffect(() => {
    window.scrollTo({ top: 0 });
  }, [openManga?.url, selected?.name, tab]);

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
      // `s`, **pas** `selected`. `setSelected(s)` plus haut ne met pas à jour la
      // closure : `selected` y est encore la source du rendu précédent, donc la
      // source précédente. Mesuré le 2026-10-08 : en passant de la source A
      // (en échec) à B, l'échec de B était mémorisé au nom de A, le succès de B
      // effacait la santé de A, et la fenêtre de challenge était rattachée à A.
      if (items.length > 0) clearHealthFor([s.name]);
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
      setChallenge(url);
      // Mémorise la source en échec : elle sera signalée dans le catalogue.
      // Une demande de vérification n'est PAS un échec : à résoudre par l'humain.
      // Même règle qu'au-dessus : c'est bien `s` qui a échoué.
      if (!url) {
        const code = raw.slice(0, raw.indexOf(':') > 0 ? raw.indexOf(':') : 12);
        noteHealth([s.name], code, describeError(raw, s.name, t));
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

  /** Nom **affiché** d'une source. Le moteur nomme une source scriptée
 *  `custom:<id du fichier>` et porte le vrai nom dans `displayName` ; les
 *  sources natives n'ont pas ce champ.
 *
 *  Mesuré le 2026-07 : la fiche d'une source importée s'affichait
 *  « custom:mangadex-api » — l'identifiant, pas le nom. Les **clés** (désactivation,
 *  cookies, requêtes au moteur) doivent garder `name` : c'est cet identifiant
 *  que le moteur attend.
 */
function nomSource(s: Source): string {
  return s.displayName ?? s.name;
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

    // **Visible d'abord.** Mesuré le 2026-10-06 : le catalogue rend 20 cartes
    // et il n'y en a qu'une poignée dans la fenêtre. Avec un intervalle de
    // politesse, interroger les 20 dans l'ordre du tableau remplissait l'écran
    // **entier en ~17 s** — dont une bonne moitié sur des cartes sous la ligne
    // de flottaison, que personne ne regarde. On interroge donc d'abord ce qui
    // est à l'écran ; le reste passe ensuite, à débit constant.
    //
    // L'appariement se fait par la **clé de la carte**, exposede en
    // `data-manga-key`, et non par l'index. Mesuré le 2026-10-06 : la première
    // version appariait le nᵉ bouton du DOM au nᵉ élément de la file, ce qui est
    // faux — la file est filtrée (on saute les entrées déjà en cache) et la
    // grille est rendue dans l'ordre de `latestTrie`, qui est trié. Les deux
    // index divergeaient, donc le tri « visible d'abord » ne triait rien.
    const visibles = new Set<string>();
    document.querySelectorAll<HTMLElement>('.manga-grid .manga[data-manga-key]').forEach((el) => {
      const r = el.getBoundingClientRect();
      // Une fenêtre de marge : ce qui est juste sous le bord remplit déjà la
      // vue, sinon un petit défilement laisserait des cartes vides.
      if (r.bottom > -window.innerHeight && r.top < window.innerHeight * 2) {
        const cle = el.getAttribute('data-manga-key');
        if (cle) visibles.add(cle);
      }
    });
    const ordre = [...todo].sort((a, b) => Number(visibles.has(b.key)) - Number(visibles.has(a.key)));

    let cursor = 0;
    const run = async () => {
      while (cursor < ordre.length) {
        const { m, key } = ordre[cursor++];
        // Porte de politesse **partagée par source** (niveau module) : voir
        // `attendreTourAutomatique`. C'est du trafic de fond — personne n'a
        // demandé ces requêtes — donc il a le droit d'attendre son tour.
        await attendreTourAutomatique(m.source);
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
            .slice(0, 1)
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
      Array.from({ length: Math.min(RECENT_CONCURRENCY, ordre.length) }, () => run()),
    );
    // `t` est volontairement ABSENT des dépendances : c'est la seule forme
    // correcte ici. Mesuré le 2026-10-05 : `useI18n()` rend `t` sous forme de
    // fonction recréée à chaque rendu (le même fait qui faisait appeler
    // `set_libelles` 18 fois au démarrage en 3.4). Avec `t` en dépendance, cet
    // effet se rejoue **à chaque rendu** alors qu'il appelle `setCardRecent`,
    // `setCardTotal` et `setRecentBusy` : chaque exécution provoque un rendu de
    // plus, qui recrée `t`, qui relance l'effet — donc une **boucle infinie**.
    // Mesuré : « Maximum update depth exceeded » × 12 à l'ouverture d'une série,
    // reproduit par le pilotage de l'application (`.mesures/pilot-13.cjs`).
    //
    // `langue` est la bonne dépendance : c'est un **primitif**, il ne change que
    // quand la langue change, et `t` n'est utilisé ici que pour `depuis(t, …)`.
    // La porte `pnpm hooks:check` refuse le retour de `t` dans un tableau de
    // dépendances d'effet.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [latest, showRecent, selected?.name, langue]);

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
    setProgressParSource((prev) => {
      const next = new Map(prev);
      next.set(source, { source, chapter: title, page: 0, total: 0, done: false, error: null });
      return next;
    });
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
      // `file` vient de `downloads_root()`, donc canonicalisé, donc préfixé de
      // `\\?\` (forme longue). Le toast est un texte destiné à l'écran : sans ce
      // retrait, l'utilisateur lisait `\\?\C:\Users\…` — valide mais illisible,
      // et incollable dans un explorateur.
      setNotice(t('notice.telecharge', { f: cheminLisible(file) }));
      refreshDownloads();
    } catch (e) {
      setError(t('erreur.telechargement', { raison: String(e) }));
    } finally {
      setDownloading(false);
      setProgressParSource((prev) => {
        const next = new Map(prev);
        next.delete(selected.name);
        return next;
      });
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

  /** Télécharge tous les chapitres de chaque webtoon de `targets`, en
   *  ignorant ce qui est déjà sur le disque. Annulable, et la file est
   *  persistée : si l'app s'arrête, il reste à la reprise.
   *
   *  **Deux webtoons en parallèle au maximum, et jamais deux de la même
   *  source.** Les chapitres D'UN webtoon restent séquentiels — même source,
   *  donc même `Mutex` dans le moteur, aucun gain à les mêler. Par contre deux
   *  webtoons de sources différentes ont chacune leur instance de parser, et le
   *  verrou global de Rust a été retiré pour leur permettre de se chevaucher.
   *  Mesuré : le téléchargement était strictement séquentiel (un chapitre à la
   *  fois, une page toutes les ~2 s selon la source). */
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

    /** Télécharge les chapitres absents d'un webtoon, dans l'ordre.
     *  Chapitres **séquentiels** : même source = même `Mutex` moteur. */
    async function telechargerWebtoon(m: Manga) {
      const key = mangaKey(m);
      const patch = (changes: Partial<QueueItem>) =>
        setQueue((prev) => prev.map((q) => (q.key === key ? { ...q, ...changes } : q)));

      if (cancelQueue.current) {
        patch({ state: 'pause' });
        return;
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
        return;
      }
      patch({ total: list.length });

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

      if (!cancelQueue.current) {
        const at = pending.findIndex((x) => mangaKey(x) === key);
        if (at >= 0) pending.splice(at, 1);
        saveQueue(pending);
      }
    }

    // Boucle de remplissage : tant qu'il reste des cibles et que les slots
    // libres sont distincts en source.
    const restants = [...targets];
    const enCours = new Map<string, { source: string; p: Promise<void> }>();
    while (restants.length > 0 || enCours.size > 0) {
      if (!cancelQueue.current) {
        for (let i = 0; i < restants.length && enCours.size < 2; i++) {
          const sourceEnCours = new Set([...enCours.values()].map((e) => e.source));
          if (sourceEnCours.has(restants[i].source)) continue;
          const m = restants.splice(i, 1)[0];
          i--;
          const p = telechargerWebtoon(m).finally(() => {
            enCours.delete(mangaKey(m));
          });
          enCours.set(mangaKey(m), { source: m.source, p });
        }
      }
      if (enCours.size === 0) break; // plus rien à lancer et rien à attendre
      await Promise.race([...enCours.values()].map((e) => e.p));
    }

    if (cancelQueue.current) {
      setQueue((prev) =>
        prev.map((q) =>
          q.state === 'attente' || q.state === 'téléchargement'
            ? { ...q, state: 'pause' as const }
            : q,
        ),
      );
      saveQueue(pending);
    }
    setQueueRunning(false);
    setProgressParSource(new Map());
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

  /** Après résolution : pousse les cookies au moteur et rejoue la requête.
   *
   *  **Ne pas effacer `challenge` avant le résultat.** Mesuré le 2026-10-09 :
   *  un `setError(null)` en tête faisait disparaître le bandeau pendant
   *  l'appel, et surtout, si `solve_challenge` échouait ensuite, le `catch`
   *  posait une erreur **sans** remettre `challenge` — donc l'écran basculait
   *  sur « Réessayer » et le bouton de vérification disparaissait. Symptôme vu :
   *  « après avoir fermé la fenêtre, plus rien, même pas le bouton ».
   *
   *  Ici on garde `challenge` pendant toute l'opération, et on ne le vide qu'en
   *  cas de succès. Un échec laisse donc les trois boutons disponibles.
   */
  async function applyChallenge() {
    if (!challenge || !selected) return;
    const url = challenge;
    setChallengeBusy(true);
    try {
      const n = await invoke<number>('solve_challenge', {
        source: selected.name,
        url,
      });
      setNotice(t('libelle.verification.cookies', { n, source: nomSource(selected) }));
      setChallenge(null);
      setError(null);
      await loadSource(selected, 0);
    } catch (e) {
      // L'échec est souvent « la fenêtre est fermée » : dans ce cas les cookies
      // sont illisibles, et c'est justement le moment de proposer de rouvrir la
      // fenêtre plutôt que d'abandonner.
      const msg = String(e);
      setError(
        /fenêtre de vérification est fermée/.test(msg)
          ? t('libelle.verification.fenetre.fermee')
          : `${t('libelle.verification.non.appliquee')} ${msg}`,
      );
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
  // **De la source la plus spécifique à la moins spécifique.** Mesuré le
  // 2026-10-06 : le code ne lisait que `selected?.name`, c'est-à-dire la source
  // *sélectionnée dans le catalogue*. Ouverte depuis l'accueil ou un favori —
  // donc sans source sélectionnée — cette fonction faisait `if (!src) return;`
  // et **rien ne se passait, sans la moindre erreur**. 25 chapitres affichés,
  // clic reçu sur le bouton, lecteur jamais ouvert, 0 erreur console.
  //
  // Or la source est connue par trois autres moyens, tous plus fiables que la
  // sélection du catalogue :
  //   - `reader.source` : on est déjà en train de lire CETTE source — c'est ce
  //     qui débloque le sélecteur de chapitre du lecteur, qui rappelait
  //     `openReaderAt(c)` sans rien passer ;
  //   - `openManga.source` : la fiche affichée porte sa source.
  const src = sourceName ?? reader?.source ?? openManga?.source ?? selected?.name;
  if (!src) {
    // Ne jamais sortir en silence : une garde muette sur un chemin nommé est
    // un bug qui ne se voit pas (leçon AGENTS.md §14).
    setError(t('erreur.source.inconnue'));
    return;
  }
  const mangaTitre = titreExplicite ?? reader?.mangaTitre ?? openManga?.title ?? '';
  if (!mangaTitre) {
    // Sans titre, le chapitre local est introuvable : mieux vaut le dire que
    // ouvrir silencieusement une version en ligne d'un chapitre téléchargé.
    console.warn('openReaderAt: titre du webtoon inconnu, lecture locale impossible');
  }
  setReaderLoading(true);
  setError(null);
  // Un nouveau chapitre repart d'un etat d'echec vide : sinon les index
  // indisponibles du chapitre precedent s'afficheraient sur le suivant, qui
  // peut etre intact (mesure : ch.3 = 0/8 servies, ch.1 = 7/8 — l'inverse
  // existe aussi).
  setPagesEnErreur(new Set());
  // Idem pour les dimensions mesurees : elles sont indexees par numero de page,
  // donc celles du chapitre precedent decaleraient la bande du suivant. Une page
  // de hauteur 1 200 px dupliquee sur un chapitre ou la vraie page fait 500 px
  // donnerait une bande de 2,4 fois trop haute.
  setDimsPages({});
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
      setError(t('erreur.chapitre.vide'));
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
    // TODO(P3): si chapters.length === 0 après l'ouverture (chemin direct sans
    // loadChapters, ouvrirSignet ou resumeReading), charger les chapitres en
    // arrière-plan : invoke('chapters', { source: src, mangaUrl, mangaTitle })
    //   .then((r) => setChapters(r.chapters ?? []))
    //   .catch(console.error)
    // Pré-requis : vérifier que `openManga?.url` est disponible dans ce contexte
    // (mesuré : peut valoir `undefined` si appelé depuis le sélecteur de chapitres
    // du lecteur). Voir analyse-lecteur.md §2.3 pour le diagnostic complet.
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
      {renderChargement()}
      <Toaster toasts={toasts} onClose={fermerToast} />
      <BaseMemoireBanner visible={baseEnMemoire} />
    </>
  );
}

/** Chargeur d'ouverture de chapitre, rendu dans **toutes** les vues.
 *
 *  Mesuré le 2026-10-06 : l'ouverture d'un chapitre laissait l'écran inchangé
 *  pendant 1 à 3 s — le sélecteur de chapitre était simplement `disabled`, et
 *  au premier chapitre on retombait sur la liste sans le moindre retour. C'est
 *  exactement l'absence de signal qui fait appuyer deux fois sur le bouton,
 *  ouvrir deux fois le même chapitre. `readerLoading` tournait déjà, il
 *  n'avait juste personne pour l'afficher.
 */
function renderChargement() {
  if (!readerLoading) return null;
  return (
    <div className="chargement" role="status">
      <span className="roue" aria-hidden="true" />
      <span className="chargement-txt">{t('lecteur.chargement')}</span>
    </div>
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
    setError(t('libelle.webtoon.sans.chapitre'));
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

function changeZoom(next: number) {
  const value = Math.min(200, Math.max(50, next));
  setZoom(value);
  window.localStorage.setItem(READER_ZOOM_KEY, String(value));
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
  scrollToPage(target);
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
  // `offsets` n'est fiable **que** si la bande est virtualisable : il est bâti
  // par sommation des `hauteurs`, et une seule hauteur inconnue suffit à rendre
  // la bande non virtualisable — donc tous les offsets sont nuls.
  //
  // Mesuré le 2026-10-08 : le moteur n'émet aucune dimension (`MangaPage` du
  // parser ne porte que `url` et `preview`, `commands.rs::pages` n'ajoute que
  // `token`), donc `hauteurs` vaut zéro pour **tout** chapitre distant. La garde
  // d'en dessous était donc satisfaite (`haut === 0` est défini) et on
  // défilait vers `max(0, 0 - 8)`, c'est-à-dire nulle part : trois clics sur `→`
  // laissaient `scrollTop` à 0 et la page 1 affichée, tandis que le compteur
  // passait de 3 à 6. L'écran mentait.
  //
  // Hors virtualisation, c'est la **géométrie réelle du DOM** qui fait foi —
  // exactement ce que dit l'effet de positionnement (ligne ~3520). D'où le
  // repli sur `scrollIntoView`, qui était déjà écrit mais jamais atteint.
  const haut = bandeVirtualisable ? offsets[cible] : undefined;
  if (root && haut !== undefined) {
    root.scrollTo({ top: Math.max(0, haut - 8), behavior: 'smooth' });
    setReader((r) => (r ? { ...r, index: cible } : r));
    // Second passage, **exact**. `offsets` est bâti sur des hauteurs encore
    // partielles (les pages après la fenêtre ne sont pas mesurées), donc
    // l'atterrissage ci-dessus peut être décalé d'une page — mesuré le
    // 2026-10-09 : saut vers la page 10, page 9 affichée. `aPositionner` fait
    // déjà le positionnement exact à l'ouverture ; on le réutilise pour que le
    // saut se cale sur la géométrie réelle dès qu'elle est connue.
    aPositionner.current = cible;
    return;
  }
  const el = stripImages.current[cible];
  if (el) {
    // setTimeout(fn, 0) : reporter après le paint pour que les offsetTop soient
    // stabilisés — utile quand `loading="eager"` provoque un reflux immédiat.
    // `?.` : évite un crash si la ref est libérée pendant la tâche (démontage).
    setTimeout(() => {
      stripImages.current[cible]?.scrollIntoView({ behavior: 'smooth', block: 'start' });
    }, 0);
    setReader((r) => (r ? { ...r, index: cible } : r));
  } else {
    // Fallback sans bande rendue : positionne l'état, la bande se replace
    // à son effet suivant via `bandeVirtualisable`.
    setReader((r) => (r ? { ...r, index: cible } : r));
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
const formatBande = zoom / 100;
// Logique déplacée dans `lib/bande-hauteurs.ts` le 2026-10-09 : elle était
  // ici, donc **aucune porte ne pouvait la vérifier** (les portes du front
  // lisent le JSX, pas le comportement). Le calcul change au passage : une page
  // sans dimensions reçoit une hauteur de repli au lieu de 0, parce que le
  // seuil de virtualisation est `h > 0` pour TOUTES les pages — une seule page
  // en échec désactivait donc toute la bande (mesuré dans l'app le 2026-10-09 :
  // ch.6, 12 images chargées à 967 px, bande pourtant non virtualisée).
  const hauteurs = useMemo(
    () => hauteursBande(pagesBande, largeurBande * formatBande, dimsPages),
    [pagesBande, largeurBande, formatBande, dimsPages],
  );

const offsets = useMemo(() => offsetsBande(hauteurs), [hauteurs]);

/** `true` quand **au moins une** page a une hauteur rendue connue.
 *
 *  Le seuil exact : virtualiser une bande dont aucune géométrie n'est connue
 *  positionnait les pages sur des hauteurs inventées (le repli 1600/900) pendant
 *  que les images se rendaient à leur ratio réel. Mesuré le 2026-10-09 sur un
 *  chapitre de 15 pages ouvert pour la première fois : bande de 13 461 px pour
 *  11 661 px rendus, et le saut vers la page 10 atterrissait sur la 9.
 *
 *  La règle et ses tests vivent dans `lib/bande-hauteurs.ts`
 *  (`bandeVirtualisableEst`) : `page.tsx` ne peut pas les porter, il retourne
 *  avant d'arriver aux hooks selon la vue affichée.
 */
const bandeVirtualisable = useMemo(() => {
  if (pagesBande.length === 0) return false;
  return bandeVirtualisableEst(
    pagesBande.map((p, i) => hauteurRendueMesuree(p, largeurBande * formatBande, dimsPages[i])),
  );
}, [pagesBande, largeurBande, formatBande, dimsPages]);


/** Les offsets du rendu precedent, pour mesurer ce que le recalcul a deplace. */
const offsetsAvantRef = useRef<readonly number[]>([]);

/** Recolle le defilement a la page que l'utilisateur regardait.
 *
 *  Les pages se mesurent au fil de l'eau (`onLoad` -> `noterDims`), donc
 *  `offsets` **change apres coup**. Mesure : la bande passe de 15 908 a
 *  14 570 px pendant la lecture, et la page lue sortait du cadre.
 *
 *  On decale donc `scrollTop` de ce que le recalcul a bouge la page situee sous
 *  le haut du cadre. La regle est dans `decalageAncrage`
 *  (`lib/bande-hauteurs.ts`), testee seule.
 *
 *  **Pourquoi un effet et pas un calcul pendant le rendu.** `scrollTop` est une
 *  propriete du DOM, pas un etat : l'ecrire pendant le rendu serait un effet de
 *  bord. Un `useEffect` qui court apres chaque changement d'`offsets` est le
 *  seul moment ou les anciens et les nouveaux offsets sont tous deux connus.
 */
useEffect(() => {
  const root = stripRef.current;
  if (!root || !bandeVirtualisable) return;
  const avant = offsetsAvantRef.current;
  offsetsAvantRef.current = offsets;
  if (avant.length === 0) return;
  const y = decalageAncrage(avant, offsets, root.scrollTop);
  if (y === null) return;
  // Le seuil evite une correction pour un decalage d'un pixel : un `scrollTo`
  // a chaque rendu ferait bouger la page en permanence.
  if (Math.abs(y - root.scrollTop) < 2) return;
  root.scrollTo({ top: y });
  dernierScroll.current = y;
  setScrollY(y);
}, [offsets, bandeVirtualisable]);
const bandeHauteur = useMemo(() => {
  if (!bandeVirtualisable) return 0;
  return hauteurs.reduce((n, h) => n + h, 0);
}, [hauteurs, bandeVirtualisable]);

const fenetre = useMemo(() => {
  const tous = pagesBande.map((_, i) => i);
  if (!bandeVirtualisable) return tous;
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
}, [pagesBande, scrollY, offsets, hauteurs, bandeVirtualisable]);

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
}, [nbPagesBande]);

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
  // Le compteur de pages suit la page réellement visible — mais seulement quand
  // `offsets` est calculable, c'est-à-dire quand la bande est virtualisable.
  //
  // Mesuré le 2026-07 : une source scriptée ne renvoie que `{id, url}`, donc
  // `hauteurs` est une suite de zéros, `offsets` aussi, et la boucle ci-dessous
  // retenait `offsets.length - 1` — soit « 73 / 73 » à chaque défilement, alors
  // que la page visible était la 3ᵉ. Même raison que pour le positionnement de
  // la bande (effet suivant) : en flux normal, c'est la géométrie réelle du DOM
  // qui fait foi, donc c'est l'observateur d'intersection qui suit la page.
  if (bandeVirtualisable) {
    let i = 0;
    for (let k = 0; k < offsets.length; k += 1) {
      if ((offsets[k] ?? 0) <= y + 8) i = k;
      else break;
    }
    if (reader && i !== reader.index) setReader((r) => (r ? { ...r, index: i } : r));
  }
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
  if (!reader) return;
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
}, [reader, offsets, largeurBande, bandeVirtualisable]);

// En mode webtoon, le compteur suit la page réellement visible.
useEffect(() => {
  if (!reader) return;
  const root = stripRef.current;
  if (!root) return;
  const observer = new IntersectionObserver(
    (entries) => {
      // Parmi les entrées intersectées, on retient **la plus visible**, pas la
      // dernière du lot.
      //
      // Mesuré le 2026-10-07 : une source scriptée ne renvoie que `{id, url}`,
      // donc `hauteurs` est vide de zéros, `bandeVirtualisable` vaut `false` et
      // la bande s'affiche en flux normal. Au montage, les images n'ont pas
      // encore de taille : elles sont toutes empilées en haut, et le premier
      // rappel de l'observateur les déclare **toutes** intersectées. La boucle
      // gardait la dernière → le compteur affichait la dernière page alors que
      // la bande était au début. Les sources natives, elles, ont des
      // dimensions : une seule image dépasse le seuil, le symptôme n'existe pas.
      let meilleure: { i: number; visible: number } | null = null;
      const borne = root.getBoundingClientRect();
      for (const entry of entries) {
        if (!entry.isIntersecting) continue;
        const r = entry.boundingClientRect;
        // Pas encore mise en page : elle ne peut pas être « la page visible ».
        if (r.height <= 0) continue;
        const visible = Math.min(r.bottom, borne.bottom) - Math.max(r.top, borne.top);
        if (visible <= 0) continue;
        const i = stripImages.current.indexOf(entry.target as HTMLImageElement);
        if (i < 0) continue;
        if (!meilleure || visible > meilleure.visible) meilleure = { i, visible };
      }
      if (meilleure) {
        const { i } = meilleure;
        setReader((r) => (r && r.index !== i ? { ...r, index: i } : r));
      }
    },
    // `threshold: 0`, et non 0.6 : dans une fenêtre de 627 px, une page webtoon
    // de 913 px ne peut jamais être visible à 60 % — le seuil était donc
    // inatteignable et l'observateur ne déclenchait plus jamais (mesuré le
    // 2026-07 : le compteur restait figé à « 1 / 73 » pendant tout le
    // défilement). Comme c'est le callback qui choisit la page la plus
    // visible, un seuil bas ne fait qu'élargir le lot de candidats.
    { root, threshold: 0 },
  );
  for (const el of stripImages.current) if (el) observer.observe(el);
  return () => observer.disconnect();
  // L'observateur dépend du chapitre, du mode et du nombre de pages — pas de
  // l'objet `reader` complet, qui est recréé à chaque changement de page et
  // rejouerait l'observation sans raison.
}, [reader?.chapter.url, reader?.pages.length]);

useEffect(() => {
  if (!reader) return;
  const onKey = (e: KeyboardEvent) => {
    const last = reader ? reader.pages.length - 1 : 0;
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
      scrollToPage(Math.min(last, reader!.index + 1));
    } else if (back.includes(e.key)) {
      e.preventDefault();
      scrollToPage(Math.max(0, reader!.index - 1));
    } else if (e.key === 'Home') {
      e.preventDefault();
      scrollToPage(0);
    } else if (e.key === 'End') {
      e.preventDefault();
      scrollToPage(last);
    }
  };
  window.addEventListener('keydown', onKey);
  return () => window.removeEventListener('keydown', onKey);
  // `scrollToPage` est redéfinie à chaque rendu ; l'inclure rebouclerait
  // l'écouteur sur chaque changement d'état du lecteur.
}, [reader]);

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
          {/* Le chemin est affiché ICI, dans le panneau qui le change : mesuré le
              2026-10-05, il n'était montré que dans un autre panneau (« Download »)
              alors que la note d'ici prévient « les téléchargements existants
              restent où ils sont ». Sans le chemin sous les yeux, on ne peut ni voir
              où sont ses 20 Mo, ni vérifier que le changement a pris effet. */}
          <p className="muted">
            {t('telechargements.dossier.ligne')}
            <code className="path">{cheminLisible(dossierTelechargements.chemin)}</code>
          </p>
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

        {/* Tâche 3.5 — réglages du planificateur (3.1).
         *
         *  Les bornes affichées viennent de la **commande**, pas d'une constante
         *  écrite ici : `ReglagesVue` les tire de `Reglages::bornes()`, seul
         *  endroit qui les connaît. Les recopier ici ferait diverger l'écran du
         *  moteur au premier réglage modifié.
         *
         *  `dernier_pass` est affiché parce que « actif » ne prouve rien : une
         *  veille active qui n'a pas tourné depuis trois jours semble active.
         */}
        <section className="panel">
          <h2>{t('plan.titre')}</h2>
          {plan === null ? (
            <p className="muted">{t('plan.chargement')}</p>
          ) : (
            <>
              <label className="switch-row">
                <input
                  type="checkbox"
                  role="switch"
                  checked={plan.actif}
                  onChange={(e) => void majPlan({ actif: e.target.checked })}
                />
                <span>{t('plan.actif')}</span>
              </label>
              <p className="muted">
                {plan.dernier_pass > 0
                  ? t('plan.dernier', { quand: depuis(t, plan.dernier_pass) })
                  : t('plan.jamais')}
              </p>

              <label className="field">
                {t('plan.periode')}
                <select
                  value={plan.periode_s}
                  onChange={(e) => void majPlan({ periodeS: Number(e.target.value) })}
                  aria-label={t('plan.periode')}
                >
                  {PERIODES.map((s) => (
                    <option key={s} value={s}>
                      {s === 3600
                        ? t('plan.periode.heure1')
                        : t('plan.periode.valeur', { duree: dureeFr(t, s) })}
                    </option>
                  ))}
                </select>
              </label>

              <label className="field">
                {t('plan.delai.source')}
                <select
                  value={plan.delai_source_s}
                  onChange={(e) => void majPlan({ delaiSourceS: Number(e.target.value) })}
                  aria-label={t('plan.delai.source')}
                >
                  {DELAIS.map((s) => (
                    <option key={s} value={s}>
                      {t('plan.delai.valeur', { n: s })}
                    </option>
                  ))}
                </select>
              </label>

              <label className="field">
                {t('plan.max.passe')}
                <input
                  type="number"
                  min={1}
                  max={plan.max_par_passe_max}
                  value={plan.max_par_passe}
                  onChange={(e) => void majPlan({ maxParPasse: Number(e.target.value) })}
                  aria-label={t('plan.max.passe')}
                />
              </label>

              <label className="field">
                {t('plan.max.sources')}
                <input
                  type="number"
                  min={1}
                  max={plan.max_sources_max}
                  value={plan.max_sources_par_passe}
                  onChange={(e) => void majPlan({ maxSourcesParPasse: Number(e.target.value) })}
                  aria-label={t('plan.max.sources')}
                />
              </label>

              {planErreur && <p className="error">{planErreur}</p>}
              <p className="muted">{t('plan.note')}</p>
            </>
          )}
        </section>

        {/* Tâche 3.3 — notifications système.
         *
         *  Panneau **séparé** et non une ligne de plus dans celui du
         *  planificateur : les deux réglages ne sont pas le même sujet. « Chercher
         *  les nouveaux chapitres » décide s'il y a du neuf à trouver ;
         *  « prévenir » décide si on interrompt l'utilisateur. Un seul interrupteur
         *  pour les deux rendrait impossible « chercher sans déranger », qui est
         *  le réglage que les gens servent.
         *
         *  Le réglage est **local** (`localStorage`) : c'est l'interface qui
         *  envoie, donc le cœur n'a rien à appliquer. L'éteindre n'arrête pas la
         *  veille, seulement l'alerte.
         */}
        <section className="panel">
          <h2>{t('notif.titre.panneau')}</h2>
          <label className="switch-row">
            <input
              type="checkbox"
              role="switch"
              checked={notif}
              onChange={(e) => {
                const v = e.target.checked;
                setNotifActive(v);
                setNotif(v);
              }}
            />
            <span>{t('notif.reglage')}</span>
          </label>
          <p className="muted">{t('notif.reglage.note')}</p>
          <p className="muted">{t('notif.reglage.note2')}</p>
        </section>

        {/* Tâche 3.4 — démarrage automatique.
         *
         *  L'état affiché est celui que le **système** rapporte, relu après
         *  chaque écriture. Optimistement cocher l'interrupteur donnerait un
         *  écran qui ment : sous Linux sans gestionnaire de bureau, ou quand
         *  une politique bloque le dossier d'auto-démarrage, l'appel échoue.
         */}
        <section className="panel">
          <h2>{t('demarrage.titre')}</h2>
          <label className="switch-row">
            <input
              type="checkbox"
              role="switch"
              checked={demarrage === 'actif'}
              disabled={demarrage === 'inconnu' || demarrage === 'indisponible'}
              onChange={(e) => void basculerDemarrage(e.target.checked)}
            />
            <span>{t('demarrage.reglage')}</span>
          </label>
          {demarrage === 'indisponible' && <p className="error">{t('demarrage.indisponible')}</p>}
          {demarrage === 'inconnu' && <p className="muted">{t('demarrage.lecture')}</p>}
          <p className="muted">{t('demarrage.note')}</p>
        </section>

        {/* Tâche 5.5 — journaux de diagnostic.
         *
         *  Le **résumé** est composé ici (elle seule connaît l'état de santé des
         *  sources, qui vit dans son `localStorage`) et écrit par Rust, qui
         *  applique la rédaction en dernier.
         *
         *  Le retour affiché est le **résultat réel** : le chemin écrit par le
         *  cœur, et le nombre de fichiers réellement supprimés. Afficher « fait »
         *  sans relire serait exactement le défaut que la purge compte pour
         *  éviter.
         */}
        <section className="panel">
          <h2>{t('journal.titre')}</h2>
          <p className="muted">{t('journal.mention')}</p>
          <div className="row-actions">
            <button
              type="button"
              disabled={journalOccupe !== null}
              onClick={() => void lancerExport()}
            >
              {t('journal.exporter')}
            </button>
            <button type="button" onClick={() => void lancerPurge()}>
              {t('journal.purger')}
            </button>
          </div>
          <p className="muted">{t('journal.exporter.note')}</p>
          <p className="muted">{t('journal.purger.note')}</p>
          {journalOccupe === 'export' && <p className="muted">{t('demarrage.lecture')}</p>}
          {journalOccupe === null && journalMessage && <p className="note-ok">{journalMessage}</p>}
          {journalOccupe === null && journalErreur && <p className="error">{journalErreur}</p>}
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
            <code className="path">{cheminLisible(dossierTelechargements.chemin)}</code>
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
              {enabled === null || enabled.length === sources.length
                        ? t('sources.tout.desactiver')
                        : t('sources.tout.activer')}
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
                  {s.displayName ?? s.name}
                </button>
              );
            })}
          </div>
          {sources.length > 200 && (
            <p className="muted">200 premiers catalogues sur {sources.length}. Utilise la recherche pour les autres.</p>
          )}
        </section>

        {/* S2.1 — sources personnalisées. La confirmation de suppression est
            en deux temps dans la ligne : la capability ne vaut que
            `dialog:allow-open`, un dialogue natif exigerait de l'élargir. */}
        <section className="panel">
          <h2>{t('sources.perso.titre')}</h2>
          <p className="muted">{t('sources.perso.note')}</p>
          <div className="setting-bar">
            <button
              className="btn"
              disabled={importPersoEnCours}
              onClick={() => void importerSourcePerso()}
            >
              {importPersoEnCours ? t('sources.perso.en.cours') : t('sources.perso.importer')}
            </button>
          </div>
          {sourcesPerso.length === 0 ? (
            <p className="muted">{t('sources.perso.vide')}</p>
          ) : (
            <ul className="custom-list">
              {sourcesPerso.map((cs) => (
                <li className="custom-row" key={cs.id}>
                  <span className="custom-row-name">{cs.name}</span>
                  <span className="muted">
                    {new Date(cs.installed_at).toLocaleDateString(langue)}
                  </span>
                  {aSupprimerPerso === cs.id ? (
                    <>
                      <button
                        className="btn"
                        aria-label={t('sources.perso.confirmer', { nom: cs.name })}
                        onClick={() => void supprimerSourcePerso(cs)}
                      >
                        {t('sources.perso.confirmer', { nom: cs.name })}
                      </button>
                      <button
                        className="btn btn-ghost"
                        onClick={() => setASupprimerPerso(null)}
                      >
                        {t('sources.perso.annuler')}
                      </button>
                    </>
                  ) : (
                    <button
                      className="btn btn-ghost"
                      aria-label={t('sources.perso.supprimer', { nom: cs.name })}
                      title={t('sources.perso.supprimer', { nom: cs.name })}
                      onClick={() => setASupprimerPerso(cs.id)}
                    >
                      ✕
                    </button>
                  )}
                </li>
              ))}
            </ul>
          )}
        </section>
      </main>
    );
  }

  // ---------- Vue : Téléchargements ----------
  if (tab === 'downloads' && !reader) {
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

        {/* Tâche 2.4 — import d'un dossier de webtoon.
         *
         *  Le bouton est **avant** la liste : l'écran « aucun webtoon » est
         *  précisément celui où l'utilisateur a le plus besoin d'importer, et
         *  un bouton sous un texte de vide serait invisible. */}
        <section className="panel import">
          <h2>{t('import.titre')}</h2>
          <p className="muted">{t('import.note')}</p>
          <div className="row-actions">
            <button
              className="btn btn-primary"
              onClick={() => void importerDossier()}
              disabled={importEnCours}
            >
              {importEnCours ? t('import.en.cours') : t('import.dossier.choisir')}
            </button>
            {importRapport && (
              <button className="btn btn-ghost" onClick={() => setImportRapport(null)}>
                {t('import.rapport.fermer')}
              </button>
            )}
          </div>

          {importRapport && (
            <div className="import-rapport">
              <p>
                {importRapport.importes > 1
                  ? t('import.rapport.ok', {
                      n: importRapport.importes,
                      titre: importRapport.titre,
                    })
                  : t('import.rapport.ok1', { titre: importRapport.titre })}
              </p>
              {importRapport.refuses.length > 0 && (
                <>
                  <p className="muted">
                    {importRapport.refuses.length > 1
                      ? t('import.rapport.refuses', { n: importRapport.refuses.length })
                      : t('import.rapport.refuse1')}
                  </p>
                  <ul className="refus-list">
                    {importRapport.refuses.map(([nom, refus]) => (
                      <li key={nom}>
                        <code>{nom}</code>
                        <span className="refus-motif">{motifRefus(t, refus)}</span>
                      </li>
                    ))}
                  </ul>
                </>
              )}
              {/* Le chemin de destination est annoncé : l'import a copié les
                  fichiers, donc l'utilisateur doit savoir où — et c'est aussi
                  *comment* il vérifie que rien n'a été écrit ailleurs. */}
              <p className="muted import-dir">
                <code>{cheminLisible(importRapport.dir)}</code>
              </p>
            </div>
          )}
        </section>

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
                      {q.state === 'téléchargement' && progressParSource.get(q.source)?.total
                        ? ` · ${progressParSource.get(q.source)!.chapter}, page ${progressParSource.get(q.source)!.page}/${progressParSource.get(q.source)!.total}`
                        : q.current
                          ? ` · ${q.current}`
                          : ''}
                    </span>
                  </div>
                );
              })}
            </div>
            {queue
              .filter((q) => q.state === 'téléchargement')
              .map((q) => {
                const p = progressParSource.get(q.source);
                return p && p.total > 0 ? (
                  <p key={q.key} className="muted">
                    {q.title} — {p.chapter} : page {p.page}/{p.total}
                  </p>
                ) : null;
              })}
          </section>
        )}

        {bibliothequeCharge ? (
          <div className="dl-list" aria-busy="true" aria-label={t('telechargements.chargement')}>
            {Array.from({ length: 3 }).map((_, i) => (
              <div className="dl-card squelette-ligne" key={i} aria-hidden="true" />
            ))}
          </div>
        ) : library.length === 0 ? (
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
                          ? // `1/1 chapitre` et non `1/1 chapitres` : le
                            // singulier ne vaut que si **les deux** compteurs
                            // valent 1. `1/3 chapitres` est correct en
                            // français — mesuré : l'import d'un dossier d'un
                            // seul chapitre affichait « 1/1 chapitres », parce
                            // qu'une clé unique ne peut pas porter le pluriel.
                            item.downloaded === 1 && item.totalChapters === 1
                            ? t('telechargements.chapitres.compte1', {
                                n: item.downloaded,
                                total: item.totalChapters,
                              })
                            : t('telechargements.chapitres.compte', {
                                n: item.downloaded,
                                total: item.totalChapters,
                              })
                          : item.downloaded > 1
                            ? t('telechargements.chapitres.sans.total', { n: item.downloaded })
                            : t('telechargements.chapitres.sans.total1')}{' '}
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
                          {t('libelle.supprimer')}
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
    const page = pages[index];
    // Signet présent sur la page affichée : l'état du bouton du lecteur.
    const marque = pageMarquee();
    return (
      <main className="reader pleine-largeur">
        {/* `renderSuperpositions()` n'est rendu qu'après l'AppBar, pas ici — le
            chargeur de chapitre, lui, est aussi utile dans le lecteur (changement
            de chapitre via le sélecteur). */}
        {renderChargement()}
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

          {/* Compteur de pages. Sans lui, sur un chapitre de 303 pages mesuré
              sur Killer Peter, l'utilisateur ne sait pas où il est — ni
              combien il reste. `reader.index` sert déjà au clavier et aux
              signets, mais n'était jamais rendu.
              *
              * `index + 1` parce que `index` est base 0 ; en mode double page
              * on affiche la page de gauche, donc « n / total » reste exact
              * (le lecteur compte les pages, pas les écrans). */}
          <span className="reader-compteur" title={t('lecteur.page.sur', {
            n: index + 1,
            total: pages.length,
          })}>
            {t('lecteur.page.sur', { n: index + 1, total: pages.length })}
          </span>

          <div className="reader-tools">
            {/* Bilan des pages que la source ne sert pas.
                *
                * Le compteur seul ne suffit pas : il se lit comme un compteur
                * de pages, alors que ces pages **n'existent pas** de l'icros de
                * la source. Sans cette ligne, l'utilisateur ne peut pas
                * distinguer un chapitre incomplet d'une application cassee —
                * les deux affichent des images absentes.
                *
                * Mesuré le 2026-10-09 : ch.3 = 0/8 pages servies, ch.1 =
                * 7/8, par le meme code. C'est un fait de la source, pas de
                * l'application. */}
            {pagesEnErreur.size > 0 && (
              <p className="reader-pages-manquantes" role="status">
                {t('lecteur.pages.manquantes', {
                  n: pagesEnErreur.size,
                  total: pages.length,
                })}
              </p>
            )}
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
                onClick={() => scrollToPage(index - 1)}
              >
                {'←'}
              </button>
              <button
                className="seg-btn"
                disabled={index >= pages.length - 1}
                onClick={() => scrollToPage(index + 1)}
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
                // Affiche le numéro de page courant quand aucun destinataire n'est saisi,
                // sinon la valeur tapée par l'utilisateur.
                value={pageJump || String(index + 1)}
                placeholder={String(index + 1)}
                onChange={(e) => setPageJump(e.target.value)}
                // La page affichée est le `value` ; le champ ne sert qu'à
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
                // Hauteur **de positionnement** (toujours un nombre) et hauteur
                // **rendue** (`null` si la mesure manque). Les deux ne sont pas
                // la même chose : poser le repli sur l'image la déformait de
                // 60,9 % (mesuré le 2026-10-09 sur TOOMICSFR, 6 images sur 6).
                const hauteur = hauteurs[i] || 0;
                const hauteurCss = hauteurRendueMesuree(
                  p,
                  largeurBande * formatBande,
                  dimsPages[i],
                );
                const styleImage: CSSProperties = bandeVirtualisable
                  ? {
                      width: `${zoom}%`,
                      position: 'absolute',
                      top: offsets[i] ?? 0,
                      ...(hauteurCss ? { height: `${hauteurCss}px` } : {}),
                    }
                  : { width: `${zoom}%` };
                // Page que la source ne sert pas : on **le dit** plutot que
                // d'afficher une image cassee sans message (voir l'etat
                // `pagesEnErreur` pour la mesure).
                if (pagesEnErreur.has(i)) {
                  return (
                    <div
                      key={p.token}
                      className="sheet-indisponible"
                      data-index={i}
                      role="img"
                      aria-label={`Page ${i + 1} : ${t('lecteur.page.indisponible')}`}
                      style={styleImage}
                    >
                      <span>{t('lecteur.page.indisponible')}</span>
                    </div>
                  );
                }
                return (
                  <img
                    key={p.token}
                    ref={(el) => {
                      stripImages.current[i] = el;
                    }}
                    src={pageSrc(p.token)}
                    alt={`Page ${i + 1}`}
                    // Une page que la source ne sert pas **le dit**. Sans ceci,
                    // le 404 de MangaDex s'affiche en image cassee muette :
                    // l'utilisateur lit « la source ne sert pas cette page »
                    // comme « l'application est cassee ».
                    onError={() =>
                      setPagesEnErreur((prec) => {
                        if (prec.has(i)) return prec;
                        const suivant = new Set(prec);
                        suivant.add(i);
                        return suivant;
                      })
                    }
                    // `lazy` **inconditionnel**, et non `lazy` seulement quand la
                    // bande est virtualisable.
                    //
                    // Mesuré le 2026-10-09 : un chapitre de 30 pages ouvert pour
                    // la première fois n'a **aucune** dimension connue — Rust ne
                    // connaît que les pages déjà en cache (`commands.rs:302`) —
                    // donc `bandeVirtualisable` valait `false`, `loading` passait
                    // à `eager`, et les 30 requêtes partaient d'un coup. Le CDN a
                    // répondu 403 à corps vide aux 17 premières.
                    //
                    // `lazy` règle la rafale **sans dépendre** de la
                    // virtualisation, donc y compris quand elle est inactive :
                    // c'est le navigateur qui ne descend que le voisinage de la
                    // fenêtre. La page courante est dans le champ, donc elle
                    // reste prioritaire — rien ne se perd sur le premier écran.
                    loading="lazy"
                    decoding="async"
                    data-index={i}
                    style={styleImage}
                    // Remonte le ratio réel au composant : Rust ne connaît que
                    // les pages déjà en cache, le navigateur connaît tout ce
                    // qu'il a décodé. Sans cette remontée, une page jamais mise
                    // en cache garde le repli et l'image est étirée — mesuré à
                    // -60,9 % avant la correction.
                    onLoad={(ev) => {
                      const im = ev.currentTarget;
                      noterDims(i, im.naturalWidth, im.naturalHeight);
                    }}
                    onDoubleClick={() => changeZoom(zoom >= 200 ? 100 : 200)}
                  />
                );
              })}
            </div>
          </div>
      </main>
    );
  }

  // ---------- Vue : chapitres ----------
  if (openManga) {
    // Un téléchargement peut être actif sur ce webtoon : l'afficher. Le reste
    // du monde (présuppression de la file, file active) vit ailleurs.
    const progress = progressParSource.get(openManga.source) ?? null;
    const pct = progress && progress.total > 0 ? Math.round((progress.page / progress.total) * 100) : 0;
    // `info` peut être en retard sur les chapitres : on affiche ce qu'on a.
    const fiche = info ?? openManga;
    const rows = infoRows(info, t);
    const summary = plainText(info?.description);
    return (
      <main>
        <AppBar tab={tab} onTab={goTab} />{renderSuperpositions()}
        <button className="crumb" onClick={backToSource}>
          {'←'} {selected ? nomSource(selected) : ''}
        </button>
        <header className="page-head">
          <h1>{openManga.title}</h1>
          <span className="sub">{selected ? nomSource(selected) : ''}</span>
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
                {t('libelle.categories.vides')}
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
            {/* Le pluriel se construit dans le dictionnaire, pas ici : le
                français et l'anglais n'ont pas la même règle (« 1 chapitre »
                mais « 1 chapter » s'écrit pareil, « 2 chapitres » ne s'écrit
                pas comme « 2 chapters »… et surtout `{n > 1 ? 's' : ''}`
                ment dans une des deux langues). Clés séparées, conformément à la
                convention déjà appliquée aux notifications. */}
            {checked.size === 0
              ? chapters.length === 1
                ? t('chapitres.compte1', { n: chapters.length, k: checked.size })
                : t('chapitres.compte', { n: chapters.length, k: checked.size })
              : checked.size === 1
                ? t('chapitres.compte.selection1', { n: chapters.length, k: checked.size })
                : t('chapitres.compte.selection', { n: chapters.length, k: checked.size })}
          </p>
        )}

        <div className="dl-bar">
          <button
            className="btn btn-primary"
            disabled={downloading || checked.size === 0}
            onClick={downloadSelected}
          >
            {downloading ? t('dl.en.cours') : t('dl.selection', { n: checked.size })}
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
              aria-label={t('libelle.selection.nb', { n: pickCount })}
            />
            <button className="btn btn-ghost" onClick={() => pickChapters('last')}>
              N derniers
            </button>
            <button className="btn btn-ghost" onClick={() => pickChapters('first')}>
              N premiers
            </button>
            <button className="btn btn-ghost" onClick={() => pickChapters('all')}>
              {t('libelle.tout')}
            </button>
            <button className="btn btn-ghost" onClick={() => pickChapters('none')}>
              {t('libelle.aucun')}
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
          /* Même principe que le catalogue, mesuré le 2026-10-06 : le
           * squelette de chapitre faisait 193 × **44** px et la vraie ligne
           * 193 × **62** px, sans enfant des deux côtés. 18 px d'écart sur
           * chaque ligne, donc la liste se décalait d'un cran à l'arrivée.
           * Ici la ligne reproduit les **trois enfants** réels (case, numéro,
           * bouton de téléchargement) avec la même structure.
           */
          <div
            className="chapter-grid"
            aria-busy="true"
            aria-label={t('chapitres.chargement')}
          >
            {Array.from({ length: CHAPTER_PAGE }).map((_, i) => (
              <div className="chapter-cell squelette-ligne" key={i} aria-hidden="true">
                <label className="chapter-check">
                  <input type="checkbox" checked={false} readOnly tabIndex={-1} />
                </label>
                {/* Les **deux** lignes de la vraie ligne : `.chapter-open`
                    empile `chapter-num` et `chapter-sub`. Avec une seule, la
                    ligne faisait 46 px au lieu de 62 — mesuré. */}
                <span className="chapter-open">
                  {/* Le contenu insécable est nécessaire : un `<span>` **vide**
                      dans un conteneur flex se réduit à 0 px (mesuré : ligne de
                      46 px au lieu de 62), donc il n'héritait d'aucune boîte de
                      ligne. Le texte est invisible via `color: transparent`. */}
                  <span className="chapter-num squelette-bloc">{' '}</span>
                  <span className="chapter-sub squelette-bloc">{' '}</span>
                </span>
                <span className="chapter-dl squelette-bloc" />
              </div>
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
                      aria-label={t('libelle.selection.un', { chapitre: chapterLabel(c) })}
                    />
                  </label>
                  <button
                    className="chapter-open"
                    disabled={readerLoading}
                    onClick={() => void openReader(c)}
                    aria-label={`Lire ${chapterLabel(c)}`}
                  >
                    <span className="chapter-num">{chapterShort(c)}</span>
                    <span className="chapter-sub">
                      {date ?? chapterLabel(c)}
                    </span>
                  </button>
                  <button
                    className="chapter-dl"
                    disabled={downloading}
                    onClick={() => void downloadChapter(c)}
                    aria-label={t('libelle.telecharger.un', { chapitre: chapterLabel(c) })}
                    title={t('libelle.telecharger')}
                  >
                    ↓
                  </button>
                </div>
              );
            })}
          </div>
        )}

        {!loading && chapters.length > 0 && filteredChapters.length === 0 && (
          <p className="muted">{t('libelle.chapitre.introuvable', { q: chapterQuery })}</p>
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
          <h1>{nomSource(selected)}</h1>
          <span className="sub">{selected.lang.toUpperCase()}</span>
        </header>

        <div className="source-search">
          <input
            className="search"
            type="search"
            aria-label={t('libelle.recherche.aria')}
            placeholder={t('libelle.recherche.placeholder')}
            value={mangaQuery}
            onChange={(e) => setMangaQuery(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') void searchInSource();
            }}
          />
          <button className="btn" onClick={searchInSource} disabled={loading}>
            {t('libelle.rechercher')}
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
                {picked.size === 1
                  ? t('selection.webtoon1', { n: picked.size })
                  : t('selection.webtoon', { n: picked.size })}
                {picked.size > 1 ? 's' : ''}
              </span>
              <div className="select-actions">
                <button
                  className="btn btn-ghost"
                  onClick={() => setPicked(new Set(latest.map(mangaKey)))}
                >
                  {t('libelle.tout')}
                </button>
                <button className="btn btn-ghost" onClick={leaveSelectMode}>
                  {t('libelle.annuler')}
                </button>
                <button
                  className="btn btn-primary"
                  disabled={picked.size === 0 || queueRunning}
                  onClick={startQueue}
                >
                  {queueRunning ? t('dl.en.cours') : t('dl.compter', { n: picked.size })}
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
                      ? t('recent.en.cours', { n: recentBusy })
                      : `${Object.keys(cardRecent).length}/${latest.length}`}
                  </span>
                </label>
                <button className="btn" onClick={() => setSelectMode(true)} disabled={latest.length === 0}>
                  {t('libelle.selectionner')}
                </button>
              </div>
            </>
          )}
        </div>

        {queueRunning &&
          queue
            .filter((q) => q.state === 'téléchargement')
            .map((q) => {
              const p = progressParSource.get(q.source);
              return (
                <div key={q.key} className="progress" role="status">
                  <div
                    className="progress-bar"
                    style={{
                      width: `${
                        p && p.total > 0 ? Math.round((p.page / p.total) * 100) : 15
                      }%`,
                    }}
                  />
                  <span>
                    {q.title}
                    {p ? ` — ${p.chapter} ${p.page}/${p.total || '?'}` : ''}
                  </span>
                </div>
              );
            })}

        {error && selected && (
          <div className="alert">
            <p>{describeError(error, selected.name, t)}</p>
            {challenge && (
              <p className="muted">{t('libelle.verification.note')}</p>
            )}
            <div className="alert-actions">
              {challenge ? (
                <>
                  <button
                    className="btn btn-primary"
                    disabled={challengeBusy}
                    onClick={() => void startChallenge()}
                  >
                    {t('libelle.verification.resoudre')}
                  </button>
                  <button
                    className="btn"
                    disabled={challengeBusy}
                    onClick={() => void applyChallenge()}
                  >
                    {t('libelle.verification.applique')}
                  </button>
                  <button className="btn btn-ghost" onClick={() => void cancelChallenge()}>
                    {t('libelle.annuler')}
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
              {latest.length === 1
                ? t('catalogue.webtoon1', { n: latest.length })
                : t('catalogue.webtoon', { n: latest.length })}
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
          /* Squelette **de la même forme que la carte réelle**.
           *
           * Mesuré le 2026-10-06 : le catalogue met 2,3 s à se remplir, et
           * l'ancien squelette (un simple rectangle `.skeleton` dans un `.grid`)
           * mesurait 234 × 351 px alors que la vraie carte mesurait 188 × 328 :
           * **46 px d'écart en largeur**, 0 enfant contre 2. Au moment où les
           * séries arrivaient, toute la grille se réorganisait — c'est pire
           * qu'un écran vide, parce que l'œil suit un contenu qui bouge.
           *
           * Donc on réutilise les **mêmes classes** que la carte réelle, avec
           * des blocs vides à la place du contenu. La géométrie vient alors du
           * même CSS, donc elle ne peut pas diverger : `.cover` + `.thumb` (ratio
           * 2:3) et `.label` (2 lignes réservées) sont exactement ceux du
           * contenu.
           *
           * Le nombre suit aussi la grille : `minmax(170px, 1fr)` donne 3
           * colonnes sur un écran étroit, donc 6 cartes — pas 8, qui forçaient
           * une ligne supplémentaire à remplir puis à disparaître.
           */
          <div className="manga-grid" aria-busy="true" aria-label={t('catalogue.chargement')}>
            {Array.from({ length: 6 }).map((_, i) => (
              <div className="manga squelette-carte" key={i} aria-hidden="true">
                <div className="manga-open">
                  <span className="cover">
                    <span className="thumb" />
                  </span>
                  <span className="label" />
                </div>
              </div>
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
                  data-manga-key={key}
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
            {selected ? nomSource(selected) : ''} ne renvoie qu&apos;une seule page : son catalogue est plus
            court qu&apos;il n&apos;y paraît (limite du site, pas de l&apos;application).
          </p>
        )}

        {hasMore && (
          <button
            className="btn btn-more"
            onClick={() => loadSource(selected, latestOffset, { tags: activeTags, query: mangaQuery })}
            disabled={loading}
          >
            {loading ? (
              <span className="loader-inline">
                <span className="roue" aria-hidden="true" />
                Chargement…
              </span>
            ) : (
              <>Voir plus ({latest.length} chargés)</>
            )}
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
            {/* Deux clés, pas un `{s}` : le français accorde le participe passé
                (« 1 lu », « 10 lus ») là où l'anglais n'accorde rien. Mesuré le
                2026-10-06 : `{s}` n'était jamais fourni, donc l'écran affichait
                littéralement « 10 lu{s} récemment ». Même famille que « n
                source(s) » — une clé unique ne peut pas servir les deux
                langues (leçon AGENTS.md §14). */}
            {recent.length <= 1
              ? t('accueil.lu.recemment1', { n: recent.length })
              : t('accueil.lu.recemment', { n: recent.length })}
            {' · '}
            {favorites.length <= 1
              ? t('accueil.favori1', { n: favorites.length })
              : t('accueil.favori', { n: favorites.length })}
          </span>
        </header>

        {error && <p className="error">{error}</p>}

        {/* Tâche 3.2 — le flux « mises à jour », en tête de l'accueil.
         *
         *  Il est **avant** « Continuer » et non après : c'est la seule
         *  information de la page qui change tout seule, donc celle qu'on vient
         *  chercher. Le reste est ce qu'on a laissé ouvert.
         *
         *  Le compteur du bandeau est la somme des `total`, **jamais**
         *  `chapitres.length` : le moteur plafonne la liste, donc la longueur
         *  annoncerait un nombre faux dès qu'une série dépasse le plafond.
         */}
        {maj.length > 0 && (
          <section className="maj">
            <div className="band">
              {majNonVus > 0
                ? majNonVus === 1
                  ? t('maj.bandeau1')
                  : t('maj.bandeau', { n: majNonVus })
                : t('maj.bandeau.vu', { n: maj.length })}
              <span className="muted">
                {maj.length > majNonVus
                  ? maj.length - majNonVus === 1
                    ? t('maj.dont.non.vus1')
                    : t('maj.dont.non.vus', { n: maj.length - majNonVus })
                  : ''}
              </span>
              <button
                className="btn btn-ghost maj-tout-vu"
                onClick={() => {
                  setMaj((prec) => {
                    const suivant = prec.map((m) => ({ ...m, vue: true }));
                    saveMaj(suivant);
                    return suivant;
                  });
                }}
                disabled={majNonVus === 0}
              >
                {t('maj.tout.vu')}
              </button>
            </div>
            {maj.map((m) => (
              <article className={`maj-item${m.vue ? ' vue' : ''}`} key={m.source + '|' + m.manga_url}>
                <button
                  className="maj-ouvrir"
                  onClick={() => {
                    setMaj((prec) => {
                      const suivant = prec.map((x) =>
                        x.source === m.source && x.manga_url === m.manga_url ? { ...x, vue: true } : x,
                      );
                      saveMaj(suivant);
                      return suivant;
                    });
                    const cible = favorites.find((f) => f.source === m.source && f.manga_url === m.manga_url);
                    if (cible) void ouvrirFavori(cible);
                  }}
                >
                  <span className="maj-titre">{m.titre}</span>
                  <span className="muted">
                    {m.source} ·{' '}
                    {m.total === 1 ? t('maj.nouveau1') : t('maj.nouveaux', { n: m.total })}
                  </span>
                </button>
                {m.chapitres.length > 0 && (
                  <details className="maj-detail" onToggle={(e) => {
                    if ((e.target as HTMLDetailsElement).open) {
                      setMaj((prec) => {
                        const suivant = prec.map((x) =>
                          x.source === m.source && x.manga_url === m.manga_url ? { ...x, vue: true } : x,
                        );
                        saveMaj(suivant);
                        return suivant;
                      });
                    }
                  }}>
                    <summary>
                      {m.tronque
                        ? m.chapitres.length === 1
                          ? t('maj.voir.tronque1', { reste: m.total - 1 })
                          : t('maj.voir.tronque', { n: m.chapitres.length, reste: m.total - m.chapitres.length })
                        : t('maj.voir1')}
                    </summary>
                    <ul className="maj-chapitres">
                      {m.chapitres.map((c) => (
                        <li key={c.url}>
                          <span>{c.titre}</span>
                          {c.depose_le ? (
                            <time dateTime={new Date(c.depose_le).toISOString()}>
                              {depuis(t, c.depose_le)}
                            </time>
                          ) : null}
                        </li>
                      ))}
                      {m.tronque && m.total > m.chapitres.length && (
                        <li className="muted">
                          {m.total - m.chapitres.length === 1
                            ? t('maj.et.pas.de.plus1')
                            : t('maj.et.pas.de.plus', { n: m.total - m.chapitres.length })}
                        </li>
                      )}
                    </ul>
                  </details>
                )}
              </article>
            ))}
          </section>
        )}

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

        {shelfCharge ? (
          /* Pendant la lecture de la base, on ne dit surtout pas « aucun
           * favori » : c'est faux pendant 1 393 ms mesurés, et l'utilisateur
           * croit avoir perdu sa bibliothèque. */
          <div className="manga-grid" aria-busy="true" aria-label={t('accueil.chargement')}>
            {Array.from({ length: 6 }).map((_, i) => (
              <div className="manga squelette-carte" key={i} aria-hidden="true">
                <div className="manga-open">
                  <span className="cover">
                    <span className="thumb" />
                  </span>
                  <span className="label" />
                </div>
              </div>
            ))}
          </div>
        ) : favorites.length === 0 ? (
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
            ? ` · ${t('catalogue.sources.desactivees', {
                n: sourcesOff.length,
                // `{s}` porte le pluriel, comme les deux autres appels de la même
                // famille. Sans lui, l'en-tête affichait « 8 désactivée{s} »
                // (mesuré le 2026-10-08) : le gabarit a un `{s}`, l'appel ne le
                // fournissait pas.
                s: sourcesOff.length > 1 ? 's' : '',
              })}`
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

          {/* S2.3 — card « Sources importées », AVANT les tuiles de langues.
              Mesuré le 2026-07 : placée après la grille, elle tombait sous la
              ligne de flottaison (19 tuiles de langues, 4 rangées) et son
              existence ne se voyait pas — c'est le retour utilisateur qui l'a
              révélé, la sonde DOM la trouvait sans la voir.

              C'est un <section> et non une tuile <button> : elle contient des
              boutons (ouvrir une source, importer), et deux <button> imbriqués
              seraient du HTML invalide — cf. AGENTS.md §14. */}
          <div className="lang-grid">
            <button
              className="lang-tile import-card"
              data-card-perso=""
              onClick={() => {
                setVueSourcesPerso(true);
                setLang('perso');
              }}
            >
              <b>{t('sources.perso.titre')}</b>
              <span className="lang-nom">{t('sources.perso.note')}</span>
              <span className="muted">
                {t('catalogue.import.compte', {
                  n: sourcesImportees.length,
                  s: sourcesImportees.length > 1 ? 's' : '',
                })}
              </span>
            </button>

            {langCounts.map(([code, n]) => (
              <button
                key={code}
                className="lang-tile"
                data-lang={code}
                onClick={() => {
                  setLang(code);
                  setShownSources(48);
                }}
              >
                <b>{code.toUpperCase()}</b>
                <span className="lang-nom">{NOMS_LANGUE[code] ?? code.toUpperCase()}</span>
                <span className="muted">
                  {n === 1 ? t('catalogue.source1', { n }) : t('catalogue.source', { n })}
                </span>
              </button>
            ))}
          </div>
        </>
      ) : (
        <>
          <p className="count">
            {filtered.length === 1
              ? t('catalogue.source1', { n: filtered.length })
              : t('catalogue.source', { n: filtered.length })}
            {nameFilter ? ` pour « ${nameFilter} »` : ''}
            {lang === 'perso' ? ` · ${t('sources.perso.titre')}` : ''}
            {lang !== 'all' && lang !== 'perso' ? ` · langue ${lang.toUpperCase()}` : ''}
          </p>

          {/* Vue des sources personnalisées : le bouton « importer » vit ici,
              pas dans la tuile (choix humain du 2026-09). Deux boutons dans la
              tuile l'imbriqué seraient du HTML invalide — AGENTS.md §14. */}
          {lang === 'perso' && (
            <div className="setting-bar">
              <button
                className="btn"
                disabled={importPersoEnCours}
                onClick={() => void importerSourcePerso()}
              >
                {importPersoEnCours ? t('sources.perso.en.cours') : t('sources.perso.importer')}
              </button>
              <button
                className="btn btn-ghost"
                onClick={() => {
                  setLang('all');
                  setVueSourcesPerso(false);
                }}
              >
                {t('catalogue.bandeau.langues')}
              </button>
            </div>
          )}

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
                    {/* Le moteur nomme une source perso `custom:<id>` (Server.kt) :
                        c'est `displayName` qui porte le nom lisible. La clé de
                        désactivation reste `s.name` — c'est cet identifiant que
                        le moteur attend. */}
                    <span className="name">{s.displayName ?? s.name}</span>
                    {dead ? (
                      <span className="badge dead-badge">{t('catalogue.hors.service')}</span>
                    ) : null}
                    {lang === 'all' && <span className="badge">{s.lang}</span>}
                    {/* S2.2 — source importée par l'utilisateur. La
                        désactivation ci-dessous est la même que pour les sources
                        natives : elle est indexée sur le nom, donc sans cas
                        particulier. */}
                    {s.custom ? <span className="badge custom-badge">{t('catalogue.perso')}</span> : null}
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
