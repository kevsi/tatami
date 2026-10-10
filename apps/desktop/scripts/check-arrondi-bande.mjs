// Porte : la largeur d'une page de la bande est arrondie **une seule fois**,
// et cette valeur unique sert aux offsets, au test de virtualisation et au
// style. Aucun appelant ne recalcule une largeur de son côté.
//
// Le défaut mesuré le 2026-10-10 : `page.tsx` passait
// `largeurBande * formatBande` — **non arrondi** — à `hauteursBande` (les
// offsets, donc la position des pages et la hauteur totale de la bande), et
// `largeurImagePx(largeurBande, zoom)` — **arrondi** — à `hauteurRendueMesuree`
// (le style de chaque image). Les deux valeurs divergent dès que le produit est
// fractionnaire, et la divergence s'accumule sur toute la bande :
//
//   largeurBande = 1081, zoom = 50 %  →  offsets : 540,5 × h/w
//                                    →  style   : 541 × h/w   (arrondi)
//
// `bande-hauteurs.ts:278-280` énonce pourtant la règle : « on arrondit une
// fois, ici, et la même valeur sert à calculer la hauteur », et `:237` :
// « il faut que la boîte et la position de la page suivante portent la même
// valeur exacte ». Le code ne le faisait pas — c'est le défaut « trait entre
// deux pages » que ces commentaires disent avoir fermé.
//
// Porte textuelle, comme `check-hooks` / `check-chemins` / `check-tuile-perso` :
// le défaut est dans un **appelant**, or une fonction pure ne peut pas voir
// comment on l'appelle. Un test unitaire de `bande-hauteurs.ts` reste vert
// quelle que soit la largeur qu'on lui passe ; c'est l'appel qu'il faut lire.
import { readFileSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const ici = dirname(fileURLToPath(import.meta.url));
const page = readFileSync(join(ici, '..', 'app', 'page.tsx'), 'utf8');

/** Nom de la largeur unique, arrondie une fois, partagée par les trois usages. */
const LARGEUR = 'largeurImage';

/** Les fonctions qui prennent une largeur de page en deuxième argument. */
const CONSOMMATEURS = ['hauteursBande', 'hauteurRendueMesuree'];

const echecs = [];

// 1. La largeur partagée doit être déclarée, et venir de `largeurImagePx` :
//    c'est là que l'arrondi a lieu, une fois.
const declaration = page.match(
  new RegExp(`const\\s+${LARGEUR}\\s*=\\s*largeurImagePx\\s*\\(`),
);
if (!declaration) {
  echecs.push(
    `aucune déclaration \`const ${LARGEUR} = largeurImagePx(…)\` dans page.tsx : ` +
      "l'arrondi doit avoir une seule source, et cette source doit être `largeurImagePx`",
  );
}

// 2. Chaque consommateur reçoit **cette** largeur, jamais un produit.
//
//    Un produit (`largeurBande * …`) est le signe du défaut : la largeur est
//    recalculée ici au lieu d'être reprise. On lit l'argument tel quel plutôt
//    qu'avec une regex approximative, sinon la porte accuserait un appel qu'elle
//    n'a pas vraiment lu.
const appels = [...page.matchAll(/\b(hauteursBande|hauteurRendueMesuree)\s*\(([^)]*)\)/g)];
let verifies = 0;
for (const appel of appels) {
  const [, fonction, args] = appel;
  const arguments_ = args.split(',').map((a) => a.trim());
  const largeur = arguments_[1] ?? '';
  verifies += 1;
  if (largeur !== LARGEUR) {
    echecs.push(
      `\`${fonction}\` reçoit \`${largeur}\` comme largeur au lieu de \`${LARGEUR}\` : ` +
        'une largeur recalculée ici arrondit (ou pas) autrement que les autres ' +
        'appels, et les offsets se décalent du style',
    );
  }
}

// 3. Garde de la porte : si on ne voit plus les deux consommateurs, elle est
//    devenue aveugle — un vert voudrait dire « je n'ai rien vu », pas « c'est bon ».
for (const fonction of CONSOMMATEURS) {
  if (!new RegExp(`\\b${fonction}\\s*\\(`).test(page)) {
    echecs.push(
      `aucun appel à \`${fonction}\` dans page.tsx : la porte vérifiait deux ` +
        "consommateurs, elle n'en trouve plus aucun — corrige ce nombre si " +
        "l'appel a disparu, sinon la porte ne mord plus.",
    );
  }
}
if (verifies < 2) {
  echecs.push(
    `${verifies} seul appel à un consommateur de largeur : la porte en attendait ` +
      'au moins deux (offsets et style). Si la bande ne se virtualise plus, ' +
      'corrige ce nombre — sinon elle ne voit plus rien.',
  );
}

if (echecs.length) {
  console.error("ECHEC — la largeur d'une page n'est pas arrondie une seule fois :");
  for (const e of echecs) console.error('  - ' + e);
  console.error(
    `\nCorrectif attendu : \`const ${LARGEUR} = largeurImagePx(largeurBande, zoom)\`,` +
      ` puis passer \`${LARGEUR}\` à TOUS les consommateurs.`,
  );
  process.exit(1);
}

console.log(
  `OK — ${verifies} consommateurs de largeur, tous servis par \`${LARGEUR}\`` +
    ' (largeurImagePx, arrondie une fois).',
);
