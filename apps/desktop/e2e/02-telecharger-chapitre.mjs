/**
 * E2E 2/3 — Télécharger un chapitre.
 *
 * Clic `↓` d'une ligne de chapitre → le cœur écrit un `.cbz` → la bibliothèque
 * le voit. On ne croit pas la notice (« Téléchargé : … ») : elle dure deux
 * secondes et ne prouve rien sur le disque. On interroge `list_library` avant
 * et après, et on exige que l'écriture soit **visible** dans l'état persistant
 * (le dossier a gagné un chapitre).
 *
 * Le scénario est **auto-nettoyant** : il choisit un webtoon absent de la
 * bibliothèque, télécharge son premier chapitre, puis le retire avec
 * `remove_download`. Il ne supprime jamais une donnée préexistante — si aucun
 * webtoon « vierge » n'est trouvé, il échoue au lieu de toucher à l'existant.
 */
import {
  connecter,
  reinitialiser,
  ouvrirCatalogue,
  choisirLangue,
  ouvrirSource,
  ouvrirFiche,
  verifier,
  pause,
} from './client.mjs';

const SOURCES = ['LELMANGA', 'SEIMANGA', 'SHOJOSCANS', 'SUSHISCANFR'];
const TENTATIVES_FICHE = 4;

const appeler = (s, cmd, args = {}) =>
  s.evaluer(`window.__TAURI_INTERNALS__.invoke(${JSON.stringify(cmd)}, ${JSON.stringify(args)})`);

/** La bibliothèque : `root: ''` = la racine de téléchargements configurée. */
const listerBibliotheque = (s) => appeler(s, 'list_library', { root: '' });

const cle = (i) => `${i.source}|${i.title}`;

export default async function scenario(session = null) {
  const propre = session === null;
  const s = session ?? (await connecter());

  await reinitialiser(s);
  await ouvrirCatalogue(s);

  // Choisit une source dont la grille répond. On repart **toujours** de
  // l'accueil : après une source ouverte, la liste des langues a disparu.
  let nom = null;
  const echecsSources = [];
  for (const candidat of SOURCES) {
    try {
      await reinitialiser(s);
      await ouvrirCatalogue(s);
      await choisirLangue(s, 'FR');
      if (!(await ouvrirSource(s, candidat))) throw new Error('tuile absente ou hors service');
      nom = candidat;
      break;
    } catch (e) {
      echecsSources.push(`${candidat} : ${String(e.message).slice(0, 60)}`);
    }
  }
  verifier(nom, `aucune source utilisable parmi ${SOURCES.join(', ')}\n  - ${echecsSources.join('\n  - ')}`);

  const avant = await listerBibliotheque(s);
  const connues = new Set(avant.map(cle));

  // Cherche un webtoon que la bibliothèque ne connaît pas : télécharger un
  // chapitre « neuf » est la seule façon de prouver une écriture.
  let titre = null;
  for (let i = 0; i < TENTATIVES_FICHE && titre === null; i++) {
    if (i > 0) {
      await reinitialiser(s);
      await ouvrirCatalogue(s);
      await choisirLangue(s, 'FR');
      await ouvrirSource(s, nom);
    }
    try {
      const entete = await ouvrirFiche(s, i);
      const t = entete.split('\n')[0].trim();
      if (t && !connues.has(`${nom}|${t}`)) titre = t;
    } catch {
      // fiche sans chapitres : on tente la suivante
    }
  }
  verifier(
    titre,
    `aucun webtoon absent de la bibliothèque parmi les ${TENTATIVES_FICHE} premiers de ${nom} — ` +
      `le scénario refuse de supprimer une donnée existante`,
  );

  // Le clic sur `↓` (premier chapitre de la fiche ouverte).
  const declenche = await s.evaluer(`(function(){
    const b = document.querySelector('.chapter-cell:not(.squelette-ligne) .chapter-dl');
    if (!b) return false;
    b.click();
    return true;
  })()`);
  verifier(declenche, 'le bouton de téléchargement du premier chapitre doit exister');

  // On attend l'**état persistant**, pas la notice. Un chapitre entier se
  // télécharge page par page : plusieurs dizaines de secondes sont normales.
  let finale = null;
  const fin = Date.now() + 300000;
  while (Date.now() < fin) {
    const items = await listerBibliotheque(s);
    const trouve = items.find((x) => cle(x) === `${nom}|${titre}`);
    if (trouve && trouve.downloaded > 0) {
      finale = trouve;
      break;
    }
    await pause(3000);
  }
  verifier(
    finale,
    `« ${titre} » n'est jamais apparu dans la bibliothèque après 300 s — le téléchargement n'a rien écrit`,
  );
  verifier(finale.downloaded >= 1, `« ${titre} » doit compter au moins un chapitre téléchargé`);

  const chapitre = finale.chapters[0];
  verifier(chapitre?.file?.toLowerCase().endsWith('.cbz'), `un chapitre doit être un .cbz (obtenu ${chapitre?.file})`);

  // Nettoyage : on retire exactement ce qu'on a ajouté (le webtoon n'existait
  // pas avant — garanti par la recherche ci-dessus).
  const retires = await appeler(s, 'remove_download', { source: nom, title: titre });
  await pause(500);
  const apres = await listerBibliotheque(s);
  verifier(!apres.some((x) => cle(x) === `${nom}|${titre}`), `« ${titre} » doit avoir été retiré (auto-nettoyage)`);

  if (propre) s.fermer();
  return { source: nom, titre, fichier: chapitre.file, chapitresRetires: retires };
}
