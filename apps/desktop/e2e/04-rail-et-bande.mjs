/**
 * E2E 4/4 — La bande et le rail.
 *
 * Couvre les deux parties **délicates** du lecteur que les trois autres
 * scénarios ne touchent pas, et qui sont précisément ce qu'on déplace en
 * sortant la bande de `page.tsx` :
 *  - le **rail d'aperçus** est peuplé et un clic sur une tuile **change la page** ;
 *  - la **bande** monte bien les pages (des `<img data-index>` existent et se
 *    décodent).
 *
 * Hors ligne (Téléchargements → Lire) : déterministe, et c'est le même chemin
 * de code que la lecture en ligne pour la bande.
 */
import {
  connecter,
  reinitialiser,
  ouvrirChapitreHorsLigne,
  lireProgressionLecteur,
  pagesDecodees,
  verifier,
  jusqua,
} from './client.mjs';

export default async function scenario(session = null) {
  const propre = session === null;
  const s = session ?? (await connecter());

  await reinitialiser(s);
  const ouvert = await ouvrirChapitreHorsLigne(s);
  verifier(ouvert, 'aucun webtoon téléchargé : la bande ne peut pas être éprouvée hors ligne');

  const depart = await lireProgressionLecteur(s.evaluer);
  verifier(depart && depart.max > 1, `le chapitre doit avoir plusieurs pages (obtenu ${JSON.stringify(depart)})`);

  // Le rail est ouvert par défaut, mais son état est **mémorisé** : s'il a été
  // masqué, on le rouvre. On le trouve par son **glyphe** (`▥` fermé / `▤`
  // ouvert) et non par son libellé : « Afficher les aperçus de pages » ne
  // contient pas le mot « rail », donc un filtre sur ce mot ne mord rien.
  if (!(await s.evaluer(`!!document.querySelector('.rail-pages')`))) {
    const ouvert2 = await s.evaluer(`(function(){
      const b = [...document.querySelectorAll('.reader-bar button')].find((x) => x.innerText.trim() === '▥');
      if (!b) return false;
      b.click();
      return true;
    })()`);
    verifier(ouvert2, 'le bouton d’ouverture du rail (glyphe ▥) doit exister');
    await jusqua(s.evaluer, `!!document.querySelector('.rail-pages')`, { delai: 15000, quoi: 'l’ouverture du rail' });
  }

  const tuiles = await s.evaluer(`document.querySelectorAll('.rail-tuile').length`);
  verifier(tuiles > 1, `le rail doit montrer des tuiles (obtenu ${tuiles})`);

  // Une tuile d'une **autre** page que la courante, pour que le clic prouve le
  // déplacement et non une absence de mouvement.
  const cible = await s.evaluer(`(function(){
    const courant = Number(document.querySelector('.reader-progress').getAttribute('aria-valuenow')) - 1;
    const vus = [...document.querySelectorAll('.rail-tuile')]
      .map((x) => Number(x.getAttribute('data-rail-index')))
      .filter((i) => Number.isFinite(i) && i !== courant);
    return vus.length ? vus[vus.length - 1] : null;
  })()`);
  verifier(cible !== null, 'au moins une tuile d’une autre page');

  const clic = await s.evaluer(`(function(){
    const t = document.querySelector('.rail-tuile[data-rail-index="${cible}"]');
    if (!t) return false;
    t.click();
    return true;
  })()`);
  verifier(clic, `la tuile ${cible} doit exister`);

  await jusqua(
    s.evaluer,
    `Number(document.querySelector('.reader-progress').getAttribute('aria-valuenow')) === ${cible + 1}`,
    { delai: 15000, quoi: `la page ${cible + 1} après un clic dans le rail` },
  );

  // La bande doit monter et décoder au moins une page (la courante).
  await jusqua(s.evaluer, `[...document.querySelectorAll('img[data-index]')].filter((i) => i.complete && i.naturalWidth > 0).length > 0`, {
    delai: 30000,
    quoi: 'le décodage d’une page de la bande',
  });

  const pagesMontees = await s.evaluer(`document.querySelectorAll('img[data-index]').length`);
  const decodees = await pagesDecodees(s.evaluer);
  verifier(pagesMontees > 0, 'la bande doit monter au moins une page');
  verifier(decodees > 0, 'au moins une page de la bande doit être décodée');

  if (propre) s.fermer();
  return { rail: { tuiles, cible: cible + 1 }, bande: { pagesMontees, decodees }, depart };
}
