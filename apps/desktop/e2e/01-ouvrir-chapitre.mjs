/**
 * E2E 1/3 — Ouvrir un chapitre.
 *
 * Le lecteur s'ouvre et **décode vraiment** une page. Compter les balises ne
 * prouve rien : une `<img>` cassée est « visible » pour le DOM, il faut
 * `complete && naturalWidth > 0` (AGENTS.md §14).
 *
 * On privilégie le **hors ligne** — Téléchargements → « Lire » — parce que
 * c'est déterministe et insensible aux 504 d'un site. C'est aussi le chemin du
 * lecteur qu'on veut protéger en premier. Faute de webtoon téléchargé (machine
 * neuve), on retombe sur le parcours en ligne catalogue → fiche → chapitre.
 *
 * En ligne, une source peut répondre sa grille puis échouer ses chapitres
 * (504 mesurés) : le scénario essaie les suivantes et échoue en nommant ce
 * qu'il a tenté — c'est alors la source qu'il faut regarder, pas le code.
 */
import {
  connecter,
  reinitialiser,
  ouvrirChapitreHorsLigne,
  ouvrirCatalogue,
  choisirLangue,
  ouvrirSource,
  ouvrirFiche,
  lireProgressionLecteur,
  pagesDecodees,
  verifier,
  jusqua,
} from './client.mjs';

const SOURCES = ['LELMANGA', 'SEIMANGA', 'SHOJOSCANS', 'SUSHISCANFR'];

export default async function scenario(session = null) {
  const propre = session === null;
  const s = session ?? (await connecter());

  await reinitialiser(s);

  let via = 'téléchargements';
  let ouvert = await ouvrirChapitreHorsLigne(s);

  if (!ouvert) {
    via = 'catalogue';
    const echecs = [];
    for (const nom of SOURCES) {
      try {
        // Toujours repartir de l'accueil : après une source ouverte, la liste
        // des langues a disparu, et un `continue` sans restauration faisait
        // échouer toutes les tentatives suivantes sur « langue introuvable ».
        await reinitialiser(s);
        await ouvrirCatalogue(s);
        await choisirLangue(s, 'FR');
        if (!(await ouvrirSource(s, nom))) throw new Error('tuile absente ou hors service');
        await ouvrirFiche(s, 0);
        const clic = await s.evaluer(`(function(){
          const b = document.querySelector('.chapter-cell:not(.squelette-ligne) .chapter-open');
          if (!b) return false;
          b.click();
          return true;
        })()`);
        if (!clic) throw new Error('pas de bouton de lecture');
        await jusqua(s.evaluer, `!!document.querySelector('.reader-bar')`, {
          delai: 90000,
          quoi: 'l’ouverture du lecteur (moteur)',
        });
        ouvert = true;
        break;
      } catch (e) {
        echecs.push(`${nom} : ${String(e.message).slice(0, 60)}`);
      }
    }
    verifier(ouvert, `aucune fiche avec chapitres parmi ${SOURCES.join(', ')}\n  - ${echecs.join('\n  - ')}`);
  }

  await jusqua(s.evaluer, `[...document.querySelectorAll('img[data-index]')].filter((i) => i.complete && i.naturalWidth > 0).length > 0`, {
    delai: 90000,
    quoi: 'le décodage de la première page',
  });

  const decodees = await pagesDecodees(s.evaluer);
  const prog = await lireProgressionLecteur(s.evaluer);
  verifier(decodees > 0, `au moins une page décodée (obtenu ${decodees})`);
  verifier(prog && prog.max > 0, `une progression de pages connue (obtenu ${JSON.stringify(prog)})`);

  if (propre) s.fermer();
  return { via, pagesDecodees: decodees, progression: prog };
}
