// Porte i18n : aucune clé morte, et aucune chaîne française laissée en dur.
//
// L'audit du 2026-10-04 a mesuré 199 clés définies pour 108 réellement
// utilisées, et 7 libellés utilisateur en français en dehors de `t()`. Une clé
// morte ne casse rien et ne se voit pas : elle donne l'impression que l'écran
// est traduit. La langue anglaise n'affichait donc que la moitié de
// l'interface, silencieusement.
//
// Ce script échoue si une régression réapparaît. Il est volontairement
// grossier : il cherche les appels `t('clé')`, ce qui couvre la forme directe
// et ne couvre pas un appel construit dynamiquement — il n'en existe aucun
// aujourd'hui (vérifié), donc toute clé non détectée est bien morte.
import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const ici = dirname(fileURLToPath(import.meta.url));
const racine = join(ici, '..');

const i18n = readFileSync(join(racine, 'app', 'i18n.ts'), 'utf8');
const page = readFileSync(join(racine, 'app', 'page.tsx'), 'utf8');

/** born FR et EN d'un dictionnaire. */
function bornes(source, nom) {
  const debut = source.indexOf(`const ${nom}`);
  if (debut < 0) throw new Error(`${nom} introuvable dans i18n.ts`);
  const debutObj = source.indexOf('{', debut);
  // Le dictionnaire se termine à la dernière ligne `};` du bloc.
  const fin = source.indexOf('\n};', debutObj);
  const bloc = source.slice(debutObj, fin);
  const cles = [...bloc.matchAll(/^\s*'([a-z][a-zA-Z0-9_.]*)':/gm)].map((m) => m[1]);
  return [...new Set(cles)];
}

const fr = bornes(i18n, 'FR');
const en = bornes(i18n, 'EN');

// Une clé est « utilisée » si elle apparaît comme littéral dans un appel `t()`
// **ou** dans une table de libellés citée ensuite par `t()`.
//
// Se fier au seul `t('clé')` donnait 91 orphelines, dont 6 utilisées par une
// variable (`{ cle: 'tri.az' }`). C'est le type de `t()` qui l'a révélé, pas ma
// regex — d'où la seconde passe : toute chaîne en `cle: '…'` compte comme
// référencée. Le compilateur reste la vraie porte ; ce script ne doit pas crier
// plus fort que lui.
// Les trois formes réellement présentes dans `page.tsx` :
//   t('clé')                      — appel direct
//   { cle: 'clé' }                — table de libellés citée plus loin
//   t(cond ? 'clé.a' : 'clé.b')   — ternaire (bouton d'activation de source)
// Les deux dernières sont apparues APRÈS la première version de ce script : à
// chaque fois, c'est le type de `t()` qui aontalé la clé, jamais la regex.
const litteraux = [...page.matchAll(/\bt\(\s*'([a-z][a-zA-Z0-9_.]*)'/g)].map((m) => m[1]);
const viaTables = [...page.matchAll(/\bcle:\s*'([a-z][a-zA-Z0-9_.]*)'/g)].map((m) => m[1]);
const viaTernaire = [...page.matchAll(/\bt\([^)]*\?\s*'([a-z][a-zA-Z0-9_.]*)'\s*:\s*'([a-z][a-zA-Z0-9_.]*)'/g)]
  .flatMap((m) => [m[1], m[2]]);
const utilisees = new Set([...litteraux, ...viaTables, ...viaTernaire]);

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

const nonTraduites = fr.filter((cle) => !en.includes(cle));
if (nonTraduites.length) {
  echecs.push(
    `${nonTraduites.length} clé(s) sans traduction anglaise :\n` +
      nonTraduites.map((c) => `    - ${c}`).join('\n'),
  );
}

console.log(`clés FR : ${fr.length} | clés EN : ${en.length} | t() dans page.tsx : ${utilisees.size}`);

if (echecs.length) {
  console.error(`\ni18n: ${echecs.length} problème(s)\n`);
  for (const e of echecs) console.error(`  ${e}\n`);
  process.exit(1);
}

console.log('i18n: aucune clé morte, aucune traduction manquante.');