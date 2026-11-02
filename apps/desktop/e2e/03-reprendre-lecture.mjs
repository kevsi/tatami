/**
 * E2E 3/3 — Reprendre une lecture.
 *
 * Hors ligne : on part d'une carte « Continuer la lecture » de l'accueil, on lit
 * la page qu'elle annonce, on clique « Reprendre », et on exige que le lecteur
 * s'ouvre **à cette page** — pas à la première.
 *
 * L'ordre compte : la position enregistrée suit l'écran, donc lire la carte
 * APRÈS avoir ouvert le lecteur mesurerait la position que l'ouverture vient
 * d'écrire (leçon AGENTS.md §14). On lit, puis on agit, puis on compare.
 *
 * Si aucune lecture n'est récente (machine neuve), le scénario se déclare
 * **ignoré** au lieu de mentir : sans état de reprise, il n'y a rien à prouver.
 */
import { connecter, reinitialiser, lireProgressionLecteur, verifier, jusqua } from './client.mjs';

export default async function scenario(session = null) {
  const propre = session === null;
  const s = session ?? (await connecter());

  await reinitialiser(s);
  // Les cartes viennent de `list_recent`, en asynchrone : sans cette attente,
  // un rechargement juste avant les fait lire vides (le scénario se croirait
  // « sans lecture récente » alors qu'il n'a pas attendu).
  await jusqua(s.evaluer, `document.querySelectorAll('.dl-card').length > 0`, {
    delai: 30000,
    quoi: 'les cartes « Continuer la lecture » de l’accueil',
  }).catch(() => 0);

  // Toutes les cartes « Continuer la lecture » qui annoncent une page et un
  // bouton « Reprendre ». On préfère une page **> 1** : une reprise à la page 1
  // ne distingue pas « avoir repris » de « avoir ouvert au début ».
  const cartes = await s.evaluer(`[...document.querySelectorAll('.dl-card')].map((c, i) => {
    const m = c.innerText.match(/page (\\d+)\\/(\\d+)/);
    const aReprendre = [...c.querySelectorAll('button')].some((b) => /Reprendre/.test(b.innerText));
    return m && aReprendre
      ? { i, titre: c.innerText.split('\\n')[0].trim(), page: Number(m[1]), total: Number(m[2]) }
      : null;
  }).filter(Boolean)`);

  if (!cartes || cartes.length === 0) {
    if (propre) s.fermer();
    return { ignore: 'aucune lecture récente avec un bouton « Reprendre »' };
  }

  const carte = cartes.find((c) => c.page > 1) ?? cartes[0];
  verifier(carte.page >= 1 && carte.page <= carte.total, `la carte annonce une page valide (${carte.page}/${carte.total})`);

  const clic = await s.evaluer(`(function(){
    const c = [...document.querySelectorAll('.dl-card')][${carte.i}];
    const b = c && [...c.querySelectorAll('button')].find((x) => /Reprendre/.test(x.innerText));
    if (!b) return false;
    b.click();
    return true;
  })()`);
  verifier(clic, 'le bouton « Reprendre » doit être cliquable');

  await jusqua(s.evaluer, `!!document.querySelector('.reader-bar')`, {
    delai: 60000,
    quoi: 'l’ouverture du lecteur depuis « Reprendre »',
  });
  await jusqua(s.evaluer, `!!document.querySelector('.reader-progress')`, {
    delai: 30000,
    quoi: 'la barre de progression du lecteur',
  });

  const prog = await lireProgressionLecteur(s.evaluer);
  const titreLecteur = await s.evaluer(`document.querySelector('.reader-title')?.innerText.replace(/\\n+/g, ' ') ?? ''`);

  verifier(
    prog.now === carte.page,
    `la reprise doit rouvrir page ${carte.page}, le lecteur est page ${prog.now}/${prog.max}`,
  );
  verifier(
    titreLecteur.includes(carte.titre),
    `le lecteur doit porter « ${carte.titre} » (titre lu : « ${titreLecteur} »)`,
  );

  if (propre) s.fermer();
  return { titre: carte.titre, page: carte.page, total: carte.total, titreLecteur };
}
