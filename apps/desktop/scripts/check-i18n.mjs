// Porte i18n : aucune clé morte, aucune chaîne française laissée en dur, et
// aucun `{placeholder}` appelé sans ses valeurs.
//
// L'audit du 2026-10-04 a mesuré 199 clés définies pour 108 réellement
// utilisées, et 7 libellés utilisateur en français en dehors de `t()`. Une clé
// morte ne casse rien et ne se voit pas : elle donne l'impression que l'écran
// est traduit. La langue anglaise n'affichait donc que la moitié de
// l'interface, silencieusement.
//
// Ce script échoue si une régression réapparaît. Il reste volontairement
// grossier : il lit les **littéraux** de `page.tsx`. Le compilateur reste la
// vraie porte — `tsc` tranche quand une clé est absente du dictionnaire, ce
// qu'aucune regex ne peut faire.
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const ici = dirname(fileURLToPath(import.meta.url));
const racine = join(ici, '..');

const i18n = readFileSync(join(racine, 'app', 'i18n.ts'), 'utf8');
const page = readFileSync(join(racine, 'app', 'page.tsx'), 'utf8');

/** Bornes FR et EN d'un dictionnaire. */
function bornes(source, nom) {
  const debut = source.indexOf(`const ${nom}`);
  if (debut < 0) throw new Error(`${nom} introuvable dans i18n.ts`);
  const debutObj = source.indexOf('{', debut);
  // Le dictionnaire se termine à la dernière ligne `};` du bloc.
  const fin = source.indexOf('\n};', debutObj);
  const bloc = source.slice(debutObj, fin);
  // Le `-` est accepté : les motifs d'import de la tâche 2.4 sont des clés
  // kebab-case, et sans lui elles **disparaissaient** du dictionnaire que cette
  // porte analyse. Un trou symétrique : la clé invisible pour la porte ne
  // pouvait pas non plus être déclarée morte.
  const cles = [...bloc.matchAll(/^\s*'([a-z][a-zA-Z0-9_.-]*)':/gm)].map((m) => m[1]);
  return [...new Set(cles)];
}

const fr = bornes(i18n, 'FR');
const en = bornes(i18n, 'EN');
const frSet = new Set(fr);

/** `page.tsx` sans ses commentaires : une clé citée en prose n'est pas
 *  affichée, et la laisser compter rendrait la forme « table » trop laxiste. */
const pageSansCommentaires = page
  .replace(/\/\*[\s\S]*?\*\//g, ' ')
  .replace(/(^|[^:])\/\/[^\n]*/g, '$1 ');

// Une clé est « utilisée » si elle apparaît comme littéral dans un appel `t()`,
// dans une table de libellés, dans un ternaire, ou comme **valeur** d'une table
// de correspondance.
//
// Se fier au seul `t('clé')` donnait 91 orphelines, dont 6 utilisées par une
// variable (`{ cle: 'tri.az' }`). C'est le type de `t()` qui l'a révélé, pas ma
// regex — d'où les passes suivantes. À chaque fois que cette liste s'est
// allongée, c'est le type qui avait tranche la question, jamais la regex. Le
// compilateur reste la vraie porte ; ce script ne doit pas crier plus fort que
// lui.
const K = '[a-z][a-zA-Z0-9_.-]*';
const litteraux = [...pageSansCommentaires.matchAll(new RegExp(`\\bt\\(\\s*'(${K})'`, 'g'))].map((m) => m[1]);
const viaTables = [...pageSansCommentaires.matchAll(new RegExp(`\\bcle:\\s*'(${K})'`, 'g'))].map((m) => m[1]);
const viaTernaire = [
  ...pageSansCommentaires.matchAll(new RegExp(`\\bt\\([^)]*\\?\\s*'(${K})'\\s*:\\s*'(${K})'`, 'g')),
].flatMap((m) => [m[1], m[2]]);
// **Table de correspondance** : `const MOTIF_REFUS: Record<Code, Cle> = { 'code': 'clé' }`.
// La clé y est un littéral sans `t()` autour. Sans cette forme, les six motifs
// d'import étaient déclarés morts alors qu'ils sont affichés — mesuré le
// 2026-10-05. Un littéral **égal à une clé déclarée** est une référence ; les
// commentaires étant retirés plus haut, une clé citée en prose ne compte pas.
const viaTable = [...pageSansCommentaires.matchAll(new RegExp(`'(${K})'`, 'g'))]
  .map((m) => m[1])
  .filter((cle) => frSet.has(cle));
const utilisees = new Set([...litteraux, ...viaTables, ...viaTernaire, ...viaTable]);

const echecs = [];

if (fr.length !== en.length) {
  echecs.push(`FR compte ${fr.length} clés, EN en compte ${en.length}`);
}

const orphelines = fr.filter((cle) => !utilisees.has(cle));
if (orphelines.length) {
  echecs.push(
    `${orphelines.length} clé(s) jamais affichée(s) — supprimez-les ou branchesz-les :\n` +
      orphelines.map((c) => `    - ${c}`).join('\n'),
  );
}

// **Placeholder appelé sans ses valeurs.** Mesuré le 2026-10-05 : `t('…compte1')`
// sans second argument affichait littéralement « {n}/{total} chapitre ». Ni
// `tsc` ni la porte « aucune clé morte » ne le voyaient : la clé existait, elle
// était correcte, et la faute était dans l'appel. On regarde donc le **texte
// français** de la clé, et les appels `t('clé')` qui ne reçoivent pas d'objet
// de paramètres.
const FR_VALEURS = new Map();
for (const cle of fr) {
  const echappee = cle.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  // Le drapeau `m` est **indispensable** : sans lui `^` s'ancre au début du
  // fichier et le dictionnaire ne contient qu'une seule entrée — celle qui
  // commence à la ligne 1. `FR_VALEURS` restait donc vide et la règle
  // n'était jamais appliquée, sans jamais crier non plus. Une porte qui ne
  // peut pas crier est indistinguishable d'une porte verte.
  const ligne = new RegExp(`^\\s*'${echappee}':\\s*'([^']*)'`, 'm').exec(i18n);
  if (!ligne) continue;
  const trouves = [...ligne[1].matchAll(/\{(\w+)\}/g)].map((m) => m[1]);
  if (trouves.length) FR_VALEURS.set(cle, trouves);
}
// Deux formes d'appel **sans** valeurs : `t('clé')` et `t('clé', autre chose)`.
// `t('clé', { … })` a des valeurs et doit être ignoré — d'où `(?!\s*\{)` placé
// **juste après la virgule**, sans `\s*` devant : avec un `\s*` avide, le
// moteur fait un backtracking et l'assertion passe quand même. C'est
// exactement ce qui est arrivé au premier essai : la porte déclarait verte un
// `t('clé', { n })` légitime tout en laissant passer `t('clé')`.
const appelsSansValeurs = [
  ...pageSansCommentaires.matchAll(new RegExp(`\\bt\\(\\s*'(${K})'\\s*(?:\\)|,(?!\\s*\\{))`, 'g')),
]
  .map((m) => m[1])
  .filter((cle) => FR_VALEURS.has(cle));
const placeholdersOublies = [...new Set(appelsSansValeurs)];
if (placeholdersOublies.length) {
  echecs.push(
    `${placeholdersOublies.length} clé(s) à {placeholder} appelée(s) sans leurs ` +
      `valeurs — l'écran affiche les accolades :\n` +
      placeholdersOublies.map((c) => `    - ${c} attend ${FR_VALEURS.get(c).join(', ')}`).join('\n'),
  );
}

const nonTraduites = fr.filter((cle) => !en.includes(cle));
if (nonTraduites.length) {
  echecs.push(
    `${nonTraduites.length} clé(s) sans traduction anglaise :\n` +
      nonTraduites.map((c) => `    - ${c}`).join('\n'),
  );
}

console.log(
  `clés FR : ${fr.length} | clés EN : ${en.length} | ` +
    `t() et tables dans page.tsx : ${utilisees.size}`,
);

if (echecs.length) {
  console.error(`\ni18n: ${echecs.length} problème(s)\n`);
  for (const e of echecs) console.error(`  ${e}\n`);
  process.exit(1);
}
console.log('i18n: aucune clé morte, aucune traduction manquante, aucun placeholder nu.');
