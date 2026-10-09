// Porte : les sources personnalisées doivent s'ouvrir comme les autres
// catalogues — une tuile, puis la liste — et non une card à part qui occupe
// toute la largeur.
//
// Demande explicite du 2026-10-09 : « la card des sources personnalisées, je
// préfère que ça soit comme les autres cards des autres sources ; quand on
// clique dans cette card on pourra voir les sources personnalisées ».
//
// Le défaut mesuré avant le correctif : `.import-card` portait
// `grid-column: 1 / -1`, donc elle occupait **toute la largeur** de la grille
// alors que les 19 tuiles de langues se répartissent en rangées de 4. Elle se
// lisait comme un encart, pas comme une entrée de catalogue.
//
// Cette porte est textuelle, comme les autres portes du front (`check-hooks`,
// `check-chemins`) : le projet n'a pas de rendu DOM en test. Elle vérifie donc
// la **structure** (une tuile dans la grille, pas une section à part entière),
// pas le rendu.
import { readFileSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const ici = dirname(fileURLToPath(import.meta.url));
const page = readFileSync(join(ici, '..', 'app', 'page.tsx'), 'utf8');
const css = readFileSync(join(ici, '..', 'app', 'globals.css'), 'utf8');

const echecs = [];

// 1. La card ne doit plus occuper toute la largeur : c'est ce qui la faisait
//    se distinguer des autres tuiles.
//
//    On ne cherche la propriété que **dans la règle** `.import-card`, jamais
//    dans tout le fichier : la première version de cette porte tombait sur le
//    **commentaire** qui explique l'ancien défaut — un motif cité n'est pas un
//    motif appliqué. On retire donc les commentaires avant de chercher, et on
//    lit le bloc de la règle.
const regles = css
  .replace(/\/\*[\s\S]*?\*\//g, '') // commentaires de bloc
  .replace(/\/\/[^\n]*/g, ''); // commentaires de ligne
const blocImportCard = (() => {
  const i = regles.indexOf('.import-card');
  if (i === -1) return '';
  const fin = regles.indexOf('}', i);
  return regles.slice(i, fin === -1 ? undefined : fin);
})();

if (/grid-column:\s*1\s*\/\s*-1/.test(blocImportCard)) {
  echecs.push(
    '.import-card porte encore `grid-column: 1 / -1` : la tuile occupe toute ' +
      "la largeur au lieu d'être une tuile comme les 19 autres",
  );
}

// 2. Elle doit être une TUILE (un bouton dans la grille des langues), pas une
//    <section> à part entière. C'est la demande : « comme les autres cards ».
//    Un <section> ne peut pas être cliquable ; il faudrait un conteneur et un
//    <button> dedans.
const carteEstSection = /<section className="lang-tile import-card"/.test(page);
if (carteEstSection) {
  echecs.push(
    'la card est encore un <section> : elle doit être une tuile cliquable, ' +
      'comme les tuiles de langue',
  );
}

// 3. Elle doit porter le même attribut stable que les tuiles de langue, pour
//    que la mesure automatisée et le test manuel visent la même chose.
if (!/data-card-perso/.test(page)) {
  echecs.push(
    "la tuile des sources personnalisées ne porte pas `data-card-perso` : " +
      'impossible de la cibler de façon stable (les libellés sont traduits)',
  );
}

// 4. Le clic doit ouvrir la liste — pas afficher la liste sur place.
if (!/vueSourcesPerso|ouvrirVuePerso/.test(page)) {
  echecs.push(
    "aucun état d'ouverture pour les sources personnalisées : le clic sur la " +
      'tuile doit mener à une vue dédiée, comme le clic sur une tuile de langue',
  );
}

// 5. Le bouton importer doit avoir quitté la tuile (choix humain explicite :
//    « tout dans le panneau »). S'il reste dans la tuile, on a deux actions
//    sur le même élément, dont une imbriquée — du HTML invalide.
const blocTuile = page.slice(
  page.indexOf('data-card-perso'),
  page.indexOf('data-card-perso') + 1200,
);
if (/importPersoEnCours[\s\S]{0,400}importerSourcePerso/.test(blocTuile)) {
  echecs.push(
    "le bouton « importer » est encore dans la tuile : il doit vivre dans le " +
      'panneau ouvert au clic (choix humain du 2026-09), sinon deux boutons ' +
      "s'imbriquent — du HTML invalide",
  );
}

if (echecs.length) {
  console.error("ECHEC — la card des sources personnalisées n'est pas une tuile du catalogue :");
  for (const e of echecs) console.error('  - ' + e);
  console.error(
    '\nDemande du 2026-10-09 : une tuile comme les autres, et le clic ouvre la liste.',
  );
  process.exit(1);
}

console.log('OK — les sources personnalisées sont une tuile du catalogue, et le clic ouvre la liste.');