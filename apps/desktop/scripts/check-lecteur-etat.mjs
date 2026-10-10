// Porte : tout chemin qui **pose un lecteur** doit remettre à zéro l'état du
// lecteur — dimensions mesurées, pages en échec, pages décodées, quota
// d'admission, position à prendre.
//
// Le défaut d'origine, mesuré le 2026-10-10 : `openReaderAt` faisait les cinq
// remises à zéro (`setPagesEnErreur`, `setDimsPages`, `setPagesDecodees`,
// `setAdmis`, `aPositionner.current`), `openLocalChapter` aucune des cinq.
// Lire un `.cbz` depuis Téléchargements après un chapitre en ligne héritait
// donc :
//   - des dimensions du chapitre précédent, indexées par numéro de page — une
//     page de 1 200 px dupliquée sur un chapitre dont la vraie page fait 500 px
//     donne une bande 2,4× trop haute ;
//   - d'un quota `admis` périmé — bande affichée sans `src`.
// Les deux chemins partagent désormais `preparerLecture`, et cette porte
// vérifie qu'un **troisième** chemin ne pourra pas oublier de l'appeler.
//
// Porte textuelle, comme `check-hooks` / `check-chemins` / `check-tuile-perso` :
// le projet n'a pas de rendu DOM en test. Elle lit la structure.
import { readFileSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const ici = dirname(fileURLToPath(import.meta.url));
const page = readFileSync(join(ici, '..', 'app', 'page.tsx'), 'utf8');

/** Nom de la remise à zéro partagée. */
const HELPER = 'preparerLecture';

const echecs = [];

// --- Découpage en blocs par déclaration de fonction -------------------------
// Le fichier mélange les indentations : les fonctions du bloc lecteur
// (pageSrc, openReaderAt, openLocalChapter, closeReader…) sont à la colonne 0,
// les autres à deux espaces. On accepte donc les deux, sinon un bloc coupé au
// mauvais endroit ferait accuser la ligne suivante — une porte qui accuse le
// mauvais fichier est une porte qu'on finit par ignorer.
const blocs = [];
let courant = null;
for (const ligne of page.split('\n')) {
  const entete = ligne.match(/^\s*(?:export\s+)?(?:async\s+)?function\s+(\w+)\s*\(/);
  if (entete) {
    courant = { nom: entete[1], corps: [] };
    blocs.push(courant);
  }
  if (courant) courant.corps.push(ligne);
}

const corpsDe = (nom) => blocs.find((b) => b.nom === nom)?.corps.join('\n') ?? '';

// 1. La fonction partagée doit exister. Sans elle la porte n'aurait plus rien
//    à vérifier, donc elle resterait verte sur n'importe quel code.
if (corpsDe(HELPER) === '') {
  echecs.push(
    `aucune fonction \`${HELPER}\` dans page.tsx : la remise à zéro de l'état ` +
      'du lecteur doit être une seule fonction, appelée par tous les chemins',
  );
}

// 2. Chaque bloc qui pose un lecteur doit l'appeler.
//
//    On compte les blocs trouvés : si ce nombre tombe sous 2, la porte est
//    devenue aveugle (les chemins ont fusionné, ou le découpage a raté) et un
//    vert veut dire « je n'ai rien vu », pas « c'est bon ».
let poses = 0;
for (const bloc of blocs) {
  if (!/setReader\(\s*\{/.test(bloc.corps.join('\n'))) continue;
  poses += 1;
  if (!new RegExp(`\\b${HELPER}\\s*\\(`).test(bloc.corps.join('\n'))) {
    echecs.push(
      `\`${bloc.nom}\` pose un lecteur sans appeler \`${HELPER}(…)\` : le ` +
        'chapitre ouvert hérite des dimensions, des pages en échec, du quota ' +
        "d'admission et de la position du chapitre précédent",
    );
  }
}
if (poses < 2) {
  echecs.push(
    `${poses} seul chemin pose un lecteur : la porte attendait au moins deux ` +
      '(openReaderAt et openLocalChapter). Si les deux ont fusionné en un seul ' +
      'chemin, corrige ce nombre — sinon la porte ne voit plus rien.',
  );
}

if (echecs.length) {
  console.error("ECHEC — l'état du lecteur n'est pas remis à zéro à chaque ouverture :");
  for (const e of echecs) console.error('  - ' + e);
  console.error(
    "\nCorrectif attendu : un appel à `preparerLecture(nbPages, position)`" +
      ' avant `setReader({ … })`.',
  );
  process.exit(1);
}

console.log(
  `OK — ${poses} chemins posent un lecteur, tous appellent \`${HELPER}(…)\`.`,
);
