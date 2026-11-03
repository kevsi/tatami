'use client';

/**
 * Barre du lecteur Tatami.
 *
 * Extrait de `page.tsx` le 2026-11-04, deuxième étage après la bande : titre du
 * chapitre, badge hors ligne, navigation entre pages, zoom, chapitre, saut de
 * page et plein écran.
 *
 * Comme pour la bande, ce composant ne **possède** aucune logique : il reçoit
 * les handlers et l'état par props. Le déplacement à petits pas est la règle.
 */
import type { Dispatch, SetStateAction } from 'react';
import { ZOOM_MAX, ZOOM_MIN } from './lecteur-constantes';
import type { I18n } from './i18n';
import type { Chapter } from './lecteur-types';

export type LecteurBarreProps = {
  /** Titre à afficher, déjà arbitré par le père (`reader.mangaTitre || openManga?.title`). */
  titre: string | undefined;
  horsLigne: boolean;
  chapter: Chapter;
  chapters: Chapter[];
  chapterLabel: (chapter: Chapter) => string;
  /** Index de la page courante, 0-based. */
  index: number;
  totalPages: number;
  /** Nombre de pages que la source ne sert pas — le compteur à dire. */
  pagesEnErreur: number;
  marque: boolean;
  pleinEcran: boolean;
  railOuvert: boolean;
  zoom: number;
  pageJump: string;
  readerLoading: boolean;
  setPageJump: Dispatch<SetStateAction<string>>;
  closeReader: () => void;
  basculerSignet: () => void;
  scrollToPage: (index: number) => void;
  basculerPleinEcran: () => void;
  basculerRail: () => void;
  changeZoom: (zoom: number) => void;
  jumpToPage: (raw: string) => void;
  openReader: (chapter: Chapter) => void;
  t: I18n['t'];
};

export function LecteurBarre({
  titre,
  horsLigne,
  chapter,
  chapters,
  chapterLabel,
  index,
  totalPages,
  pagesEnErreur,
  marque,
  pleinEcran,
  railOuvert,
  zoom,
  pageJump,
  readerLoading,
  setPageJump,
  closeReader,
  basculerSignet,
  scrollToPage,
  basculerPleinEcran,
  basculerRail,
  changeZoom,
  jumpToPage,
  openReader,
  t,
}: LecteurBarreProps) {
  return (
    <header className="reader-bar">
      <button className="btn btn-ghost" onClick={closeReader}>
        {'←'} {t('nav.chapitres')}
      </button>
      <span className="reader-title">
        {titre} · {chapterLabel(chapter)}
      </span>

      {/* Sans cette mention, impossible de savoir si l'affichage vient du
          disque ou du site — donc si une coupure réseau cassera la lecture.
          Cette information change tout pour l'utilisateur. */}
      {horsLigne && (
        <span className="reader-offline" title={t('telechargements.lecture.horsligne')}>
          {t('lecteur.horsligne')}
        </span>
      )}

      {/* Le compteur de pages vivait ici en plus du champ « Page n / total »
          plus bas : deux fois la même information, et ~60 px de barre.
          Il ne reste que le champ, qui est aussi le seul à permettre
          d'aller à une page précise. */}
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
        {pagesEnErreur > 0 && (
          <p className="reader-pages-manquantes" role="status">
            {t('lecteur.pages.manquantes', {
              n: pagesEnErreur,
              total: totalPages,
            })}
          </p>
        )}
        <button
          className={`seg-btn${marque ? ' on' : ''}`}
          aria-pressed={marque}
          onClick={() => void basculerSignet()}
          title={t('lecteur.signet.info')}
        >
          {marque ? t('lecteur.signet.actif') : t('lecteur.signet')}
        </button>

        {/* Navigation du bandeau, dans la barre du lecteur (demande
            explicite). En mode webtoon, la navigation vivait sous la
            barre : changer de page imposait de remonter en haut de
            l'écran, soit parcourir 650 000 px sur un chapitre de 303
            pages. */}
        <div className="seg" role="group" aria-label={t('lecteur.navigation')}>
          {/* Retour au début du chapitre. Demande du 2026-10-23 : sur une
              bande de plusieurs milliers de pixels, remonter à la main
              revient à défiler tout le chapitre. `scrollToPage(0)` couvre
              les deux modes (offset calculé en virtualisé, géométrie DOM
              en flux). Inactif sur la page 1, comme « ← ». */}
          <button
            className="seg-btn"
            disabled={index === 0}
            onClick={() => scrollToPage(0)}
            aria-label={t('lecteur.haut')}
            title={t('lecteur.haut')}
          >
            {'⤒'}
          </button>
          <button
            className="seg-btn"
            disabled={index === 0}
            onClick={() => scrollToPage(index - 1)}
            aria-label={t('lecteur.page.precedente')}
            title={t('lecteur.page.precedente')}
          >
            {'←'}
          </button>
          <button
            className="seg-btn"
            disabled={index >= totalPages - 1}
            onClick={() => scrollToPage(index + 1)}
            aria-label={t('lecteur.page.suivante')}
            title={t('lecteur.page.suivante')}
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

        {/* Rail d'aperçus : affiché par défaut, se souvient d'être masqué.
            Le libellé seul ne dit rien — un bouton à glyphe porte son nom
            accessible ET son info-bulle. */}
        <button
          className={`seg-btn solo${railOuvert ? ' on' : ''}`}
          aria-pressed={railOuvert}
          aria-label={railOuvert ? t('lecteur.rail.masquer') : t('lecteur.rail.afficher')}
          onClick={basculerRail}
          title={railOuvert ? t('lecteur.rail.masquer') : t('lecteur.rail.afficher')}
        >
          {railOuvert ? '▤' : '▥'}
        </button>

        <label className="zoom">
          <span className="muted">{zoom}%</span>
          <input
            type="range"
            min={ZOOM_MIN}
            max={ZOOM_MAX}
            step={10}
            value={zoom}
            onChange={(e) => changeZoom(Number(e.target.value))}
            onWheel={(e) => {
              if (!e.ctrlKey) return;
              e.preventDefault();
              changeZoom(zoom + (e.deltaY < 0 ? 10 : -10));
            }}
            aria-label={t('lecteur.largeur')}
          />
        </label>
        {chapters.length > 0 && (
          <>
            <select
              className="chapter-select"
              aria-label={t('lecteur.chapitre')}
              title={t('lecteur.chapitre.raccourci')}
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
            {/* Demande du 2026-10-25 : le raccourci n'était qu'une info-bulle sur
                le sélecteur — personne ne la voit. Il reste donc visible,
                mais en pastille « N/P » : la phrase complète occupait
                ~200 px et poussait toute la barre sur une deuxième ligne
                (mesuré : outillage de 932 px pour 1 280 px de fenêtre).
                Le texte entier reste en info-bulle. */}
            <span className="raccourci-clavier" title={t('lecteur.chapitre.raccourci')}>
              {t('lecteur.chapitre.raccourci.court')}
            </span>
          </>
        )}
        <form
          className="page-jump"
          onSubmit={(e) => {
            e.preventDefault();
            jumpToPage(pageJump);
          }}
        >
          <label className="page-jump-label" htmlFor="reader-page">
            {t('lecteur.page.intitre')}
          </label>
          <input
            id="reader-page"
            type="number"
            inputMode="numeric"
            min={1}
            max={totalPages}
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
            aria-label={t('lia.aller.page')}
          />
          <span className="muted">/ {totalPages}</span>
        </form>
      </div>
    </header>
  );
}
