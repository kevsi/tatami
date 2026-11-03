'use client';

/**
 * Bande de lecture + rail d'aperçus du lecteur Tatami.
 *
 * Extrait de `page.tsx` le 2026-11-04 : c'est le bloc le plus dense à lire
 * dans le composant principal, et il porte les parties les plus délicates du
 * lecteur (virtualisation de la bande, admission des pages, rail à tuiles).
 *
 * Le composant ne **possède** pas la logique : tout l'état, les mémos et les
 * effets restent dans `page.tsx` pour l'instant. Il reçoit donc la fenêtre,
 * les hauteurs et les callbacks par props — le déplacement d'un module de
 * 9 300 lignes se fait par étapes vérifiées, jamais en une seule passe.
 */

import { Fragment, type CSSProperties, type Dispatch, type RefObject, type SetStateAction } from 'react';
import { hauteurRendueMesuree } from '../lib/bande-hauteurs';
import { admettreUne } from '../lib/quota-pages';
import { RAIL_PAS, RAIL_TUILE_H, ZOOM_MAX } from './lecteur-constantes';
import type { I18n } from './i18n';

/** Une page de la bande — la charge utile de `reader.pages`. */
export type Page = {
  index: number;
  url: string;
  previewUrl?: string | null;
  token: string;
  /** Taille réelle en pixels, lue par Rust dans l'en-tête. 0 = inconnue. */
  largeur?: number;
  hauteur?: number;
};

export type LecteurBandeProps = {
  /** Pages du chapitre ouvert, déjà triées. */
  pages: Page[];
  /** Largeur entière (en px CSS) commune à toutes les pages. */
  largeurImage: number;
  hauteurs: number[];
  offsets: number[];
  dimsPages: Record<number, { w: number; h: number }>;
  bandeVirtualisable: boolean;
  bandeHauteur: number;
  /** Indices montés dans la fenêtre virtualisée. */
  fenetre: number[];
  /** Pages admises à charger (`admis ∪ fenetre` si virtualisée). */
  admisAffiches: ReadonlySet<number>;
  pagesDecodees: Set<number>;
  pagesEnErreur: Set<number>;
  /** Index de la page courante (0-based). */
  index: number;
  reprisePages: number;
  railOuvert: boolean;
  /** Indices de tuiles montées dans le rail. */
  railFenetre: number[];
  stripRef: RefObject<HTMLDivElement | null>;
  railRef: RefObject<HTMLElement | null>;
  stripImages: RefObject<(HTMLImageElement | null)[]>;
  observateurBande: RefObject<IntersectionObserver | null>;
  setPagesEnErreur: Dispatch<SetStateAction<Set<number>>>;
  setPagesDecodees: Dispatch<SetStateAction<Set<number>>>;
  setAdmis: Dispatch<SetStateAction<number[]>>;
  noterDims: (i: number, w: number, h: number) => void;
  pageSrc: (token: string, reprise: number) => string;
  changeZoom: (zoom: number) => void;
  zoom: number;
  scrollToPage: (index: number) => void;
  onScrollBande: () => void;
  onScrollRail: () => void;
  t: I18n['t'];
};

export function LecteurBande({
  pages,
  largeurImage,
  hauteurs,
  offsets,
  dimsPages,
  bandeVirtualisable,
  bandeHauteur,
  fenetre,
  admisAffiches,
  pagesDecodees,
  pagesEnErreur,
  index,
  reprisePages,
  railOuvert,
  railFenetre,
  stripRef,
  railRef,
  stripImages,
  observateurBande,
  setPagesEnErreur,
  setPagesDecodees,
  setAdmis,
  noterDims,
  pageSrc,
  changeZoom,
  zoom,
  scrollToPage,
  onScrollBande,
  onScrollRail,
  t,
}: LecteurBandeProps) {
  return (
    // Le rail d'aperçus vit À CÔTÉ de la bande, pas par-dessus (demande
    // « à côté du lecteur ») : `.reader-corps` réserve sa largeur, donc la
    // page n'est jamais recouverte, et la largeur de lecture se recalcule
    // toute seule via le ResizeObserver déjà posé sur `.webtoon`.
    <div className="reader-corps">
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
            // La largeur vient d'en haut (`largeurImage`) : la même valeur
            // **entière** sert à la boîte, au ratio et aux offsets.
            const hauteurCss = hauteurRendueMesuree(
              p,
              largeurImage,
              dimsPages[i],
            );
            const styleImage: CSSProperties = bandeVirtualisable
              ? {
                  // **Pixels entiers, pas un pourcentage.** Mesuré le
                  // 2026-10-09 : `50 %` d'une largeur impaire vaut 540,5 px,
                  // et une image dessinée sur un demi-pixel laisse une
                  // colonne translucide à son bord — la ligne verticale
                  // signalée, visible seulement en réduisant la fenêtre.
                  // La même valeur entière sert à calculer la hauteur, sinon
                  // la boîte et le ratio seraient arrondis dans deux sens.
                  width: `${largeurImage}px`,
                  position: 'absolute',
                  top: offsets[i] ?? 0,
                  ...(hauteurCss ? { height: `${hauteurCss}px` } : {}),
                }
              : { width: `${largeurImage}px` };
            // Page que la source ne sert pas : on **le dit** plutôt que
            // d'afficher une image cassée sans message (voir l'état
            // `pagesEnErreur` pour la mesure).
            if (pagesEnErreur.has(i)) {
              return (
                <div
                  key={p.token}
                  className="sheet-indisponible"
                  data-index={i}
                  role="img"
                  aria-label={t('lecteur.page.indisponible.numero', {
                    n: i + 1,
                    raison: t('lecteur.page.indisponible'),
                  })}
                  style={styleImage}
                >
                  <span>{t('lecteur.page.indisponible')}</span>
                </div>
              );
            }
            return (
              // La clé est sur le **Fragment**, pas sur l'<img> à
              // l'intérieur : c'est l'élément du tableau que React doit
              // suivre. Mesuré le 2026-10-15 — 1 erreur console
              // « Each child in a list should have a unique key prop » à
              // l'ouverture du lecteur (audit `docs/audit-ux-2026-10-15.md`,
              // F1), et sans clé l'identité d'une page est perdue quand la
              // fenêtre virtuelle se déplace.
              <Fragment key={p.token}>
                <img
                  key={p.token}
                  ref={(el) => {
                    // Inscrire **et** désinscrire ici. C'est le seul endroit
                    // qui voit les pages montées au défilement : l'effet
                    // d'observation ne se rejoue pas pour elles.
                    const ancien = stripImages.current[i];
                    if (ancien && ancien !== el) observateurBande.current?.unobserve(ancien);
                    stripImages.current[i] = el;
                    if (el) observateurBande.current?.observe(el);
                  }}
                  src={admisAffiches.has(i) ? pageSrc(p.token, reprisePages) : undefined}
                  alt={t('lecteur.page.une', { n: i + 1 })}
                  // Une page que la source ne sert pas **le dit**. Sans ceci,
                  // le 404 de MangaDex s'affiche en image cassée muette :
                  // l'utilisateur lit « la source ne sert pas cette page »
                  // comme « l'application est cassée ».
                  onError={() => {
                    setPagesEnErreur((prec) => {
                      if (prec.has(i)) return prec;
                      const suivant = new Set(prec);
                      suivant.add(i);
                      return suivant;
                    });
                    // Le crédit se libère aussi sur un échec. Le commentaire
                    // de `FENETRE_TELECHARGEMENT` l'annonçait, mais seul
                    // `onLoad` rappelait `admettreUne` : une page que la
                    // source ne sert pas gardait son crédit pour toujours et
                    // six échecs suffisaient à bloquer la file.
                    if (!bandeVirtualisable) {
                      setAdmis((prec) => admettreUne(prec, fenetre));
                    }
                  }}
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
                  // fenêtre. La page courante doit donc être **dans le champ**
                  // pour être prioritaire — et le 2026-10-23 elle ne l'était
                  // pas : le placeholder la précédait de 2 249 px, d'où
                  // l'écran vide mesuré à l'ouverture (cf. l'ordre corrigé
                  // plus bas). Ce commentaire décrivait l'intention, pas le
                  // comportement.
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
                    setPagesDecodees((prec) => {
                      if (prec.has(i)) return prec;
                      const suivant = new Set(prec);
                      suivant.add(i);
                      return suivant;
                    });
                    // Hors virtualisation seulement : en bande virtualisée,
                    // l'effet ci-dessus admet déjà toute la fenêtre.
                    if (!bandeVirtualisable) {
                      setAdmis((prec) => admettreUne(prec, fenetre));
                    }
                  }}
                  onDoubleClick={() => changeZoom(zoom >= ZOOM_MAX ? 100 : ZOOM_MAX)}
                />
                {/* Placeholder tant que la page n'est pas décodée.
                    Il vient **après** l'image, pas avant. Mesuré le
                    2026-10-23 : en mode flux, l'ancien ordre plaçait le
                    placeholder de 2 249 px avant l'image, donc la première
                    page se trouvait à y = 2 314 dans une fenêtre de 749 px —
                    hors du seuil de `loading="lazy"`. Aucune requête n'était
                    émise, donc aucune dimension n'arrivait, donc la bande
                    n'entrait jamais en virtualisation : écran vide pendant
                    les 25 s d'observation, et il fallait défiler de 2 400 px
                    pour que la première image parte. L'ordre inverse ne
                    change pas la hauteur réservée (placeholder = repli +
                    image = 0), mais met la page dans le champ.
                    `aria-hidden` car il ne décrit rien — c'est un état, pas
                    un contenu. */}
                {!pagesDecodees.has(i) && (
                  <div
                    className="feuille"
                    aria-hidden="true"
                    style={{
                      ...styleImage,
                      // Sans hauteur mesurée, on prend la place réservée :
                      // c'est elle qui connaît la page.
                      height: `${hauteurCss ?? hauteur}px`,
                    }}
                  />
                )}
              </Fragment>
            );
          })}
        </div>
      </div>
      {railOuvert && pages.length > 1 && (
        /* Tuiles **virtualisées** par la fenêtre du rail (même mécanisme
           que `fenetreBande`) ; clic = `scrollToPage`. Les vignettes ne
           demandent **aucune** requête propre : leur `src` n'existe que
           pour une page **déjà décodée** par la bande (`pagesDecodees`),
           donc servie du cache navigateur — le rail est un miroir de ce
           que le lecteur a chargé, jamais un concurrent de la file. Une
           page jamais lue montre son numéro, et le prend en charge dès
           qu'elle est passée dans la bande. Une page que la source ne
           sert pas se voit **aux deux endroits** (même état
           `pagesEnErreur`), sans double comptage. */
        <nav
          className="rail-pages"
          ref={railRef}
          onScroll={onScrollRail}
          aria-label={t('lecteur.rail')}
        >
          <div
            className="rail-inner"
            style={{ height: `${pages.length * RAIL_PAS}px` }}
          >
            {railFenetre.map((i) => {
              const pr = pages[i];
              if (!pr) return null;
              const active = i === index;
              const indisponible = pagesEnErreur.has(i);
              return (
                <button
                  key={pr.token}
                  type="button"
                  className={`rail-tuile${active ? ' active' : ''}${indisponible ? ' indisponible' : ''}`}
                  style={{
                    top: i * RAIL_PAS,
                    height: RAIL_TUILE_H,
                  }}
                  onClick={() => scrollToPage(i)}
                  aria-label={t('lecteur.rail.page', { n: i + 1 })}
                  aria-current={active ? 'true' : undefined}
                  data-rail-index={i}
                >
                  {pagesDecodees.has(i) && !indisponible && (
                    <img
                      src={pageSrc(pr.token, reprisePages)}
                      alt=""
                      loading="lazy"
                      decoding="async"
                      onError={() => {
                        // Même état que la bande : la vignette d'une page
                        // refusée bascule sur la tuile en pointillés au
                        // lieu d'une image cassée muette.
                        setPagesEnErreur((prec) => {
                          if (prec.has(i)) return prec;
                          const suivant = new Set(prec);
                          suivant.add(i);
                          return suivant;
                        });
                      }}
                    />
                  )}
                  <span className="rail-num" aria-hidden="true">
                    {i + 1}
                  </span>
                </button>
              );
            })}
          </div>
        </nav>
      )}
    </div>
  );
}
