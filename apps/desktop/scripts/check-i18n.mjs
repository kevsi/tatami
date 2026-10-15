// Porte i18n. Elle vérifie **exactement** quatre choses :
//
//   1. FR et EN ont le même nombre de clés ;
//   2. aucune clé n'est orpheline (définie et jamais appelée) ;
//   3. aucun `{placeholder}` n'est appelé sans ses valeurs ;
//   4. aucun pluriel n'est construit à la main dans le JSX (voir plus bas).
//
// **Ce qu'elle ne vérifie pas, et qu'il ne faut pas lui demander** : elle ne
// cherche aucune chaîne française en dur. C'était pourtant ce qu'annonçait son
// en-tête jusqu'au 2026-10-08, et l'audit a montré que 23 libellés
// s'affichaient littéralement dans `page.tsx` pendant que la porte était verte.
// Reconnaître du français dans du JSX sans parseur donne 503 faux positifs sur
// `page.tsx` — commentaires multi-lignes et identifiants comme `source` ou
// `chapitres` — donc une porte qui hurle sur du code correct finit ignorée,
// ce qui est pire que pas de porte. La règle 4 ci-dessous a été retenue
// parce qu'elle est **précise** : un pluriel assemblé dans le composant est
// mathématiquement intraduisible, et le motif est rare et sans namesake.
//
// L'audit du 2026-10-04 a mesuré 199 clés définies pour 108 réellement
// utilisées, et 7 libellés utilisateur en français en dehors de `t()`. Une clé
// morte ne casse rien et ne se voit pas : elle donne l'impression que l'écran
// est traduit. La langue anglaise n'affichait donc que la moitié de
// l'interface, silencieusement.
//
// Le compilateur reste la vraie porte — `tsc` tranche quand une clé est absente
// du dictionnaire, ce qu'aucune regex ne peut faire.
import { readFileSync, readdirSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const ici = dirname(fileURLToPath(import.meta.url));
const racine = join(ici, '..');

const i18n = readFileSync(join(racine, 'app', 'i18n.ts'), 'utf8');

/** Tous les modules de l'app **et de `lib/`**, sauf le dictionnaire, récursivement.
 *
 *  Mesuré le 2026-10-05 : la porte ne lisait que `page.tsx`, donc les six clés
 *  de la tâche 3.3 — utilisées dans `app/notification.ts` — étaient déclarées
 *  mortes alors qu'elles sont affichées. Un écran peut vivre dans `page.tsx` dès
 *  qu'on extrait un module ; la porte doit suivre.
 *
 *  La marche est **récursive** depuis le 2026-10-08 : `readdirSync` sans
 *  profondeur ignorait `app/types/`, donc deux fichiers n'étaient jamais
 *  analysés — exactement l'angle mort déjà corrigé deux fois pour `page.tsx`
 *  et pour `aucune_commande_tauri_nest_morte`.
 *
 *  `lib/` est lu depuis le **2026-10-11**, troisième fois le même angle mort :
 *  `describeError` a été extrait vers `lib/erreurs-source.ts` (pour le rendre
 *  testable), et les huit clés `erreur.*` qu'il demande ont été déclarées
 *  mortes alors qu'elles sont affichées. Une fonction qui traduit peut vivre
 *  dans `lib/` ; la porte doit suivre **le module**, pas le dossier `app/`.
 *
 *  `i18n.ts` est exclu : y figurer ferait compter **toute** clé comme utilisée,
 *  puisque c'est là qu'elles sont définies — la porte ne crierait plus jamais.
 */
function modulesDeApp(dossier) {
  const trouves = [];
  for (const entree of readdirSync(dossier, { withFileTypes: true })) {
    const chemin = join(dossier, entree.name);
    if (entree.isDirectory()) trouves.push(...modulesDeApp(chemin));
    else if (/\.tsx?$/.test(entree.name) && entree.name !== 'i18n.ts') trouves.push(chemin);
  }
  return trouves.sort();
}

const appDir = join(racine, 'app');
const libDir = join(racine, 'lib');
// `modulesDeApp` rend des chemins **absolus** : on ne les retranche pas, sinon
// on recolle `app/` devant (mesuré le 2026-10-11 : ENOENT `app/app/…`).
const modules = [...modulesDeApp(appDir), ...modulesDeApp(libDir)];
const page = modules.map((f) => readFileSync(f, 'utf8')).join('\n');
if (!modules.includes(join(appDir, 'page.tsx'))) throw new Error('page.tsx absent de app/');


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
  // Les deux styles de guillemets sont acceptés : le dictionnaire a été
  // reformaté en guillemets doubles, et le motif n'en acceptait que les
  // apostrophes — la porte lisait alors **0 clé** et déclarait quand même
  // « aucune clé morte ». Une porte qui ne voit rien ne peut pas crier.
  const cles = [...bloc.matchAll(/^\s*['"]([a-z][a-zA-Z0-9_.-]*)['"]\s*:/gm)].map((m) => m[1]);
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
// Apostrophe **ou** guillemet double. Les quatre motifs ci-dessous n'en
// acceptaient qu'une, alors que le code et le dictionnaire ont été reformatés
// en guillemets doubles : la porte ne voyait plus aucune clé *utilisée*, donc
// aucune clé n'était morte non plus. Elle annonçait « 0 clé » et « aucun
// problème » — deux verts qui ne voulaient rien dire.
const Q = `['"]`;
const litteraux = [...pageSansCommentaires.matchAll(new RegExp(`\\bt\\(\\s*${Q}(${K})${Q}`, 'g'))].map((m) => m[1]);
const viaTables = [...pageSansCommentaires.matchAll(new RegExp(`\\bcle:\\s*${Q}(${K})${Q}`, 'g'))].map((m) => m[1]);
const viaTernaire = [
  ...pageSansCommentaires.matchAll(new RegExp(`\\bt\\([^)]*\\?\\s*${Q}(${K})${Q}\\s*:\\s*${Q}(${K})${Q}`, 'g')),
].flatMap((m) => [m[1], m[2]]);
// **Table de correspondance** : `const MOTIF_REFUS: Record<Code, Cle> = { 'code': 'clé' }`.
// La clé y est un littéral sans `t()` autour. Sans cette forme, les six motifs
// d'import étaient déclarés morts alors qu'ils sont affichés — mesuré le
// 2026-10-05. Un littéral **égal à une clé déclarée** est une référence ; les
// commentaires étant retirés plus haut, une clé citée en prose ne compte pas.
const viaTable = [...pageSansCommentaires.matchAll(new RegExp(`${Q}(${K})${Q}`, 'g'))]
  .map((m) => m[1])
  .filter((cle) => frSet.has(cle));
// **Clés construites par gabarit** : `t(\`fiche.etat.${state}\`)\`. La partie
// fixe du gabarit est un littéral ; la partie variable ne l'est pas — or les
// DEUX sont de vraies clés. Mesuré le 2026-10-09 : les huit `fiche.etat.*`
// étaient déclarées mortes alors que la fiche affiche l'état de la série.
// `tsc` ne voit rien (`t()` accepte `Cle`, et \`${state}\` est typé `string`),
// donc sans cette passe la porte crie à tort.
//
// On ne peut pas deviner les suffixes depuis l'appel : on part du gabarit
// trouvé, et on complète par **toute clé déclarée qui commence par ce
// préfixe**. Le risque d'un préfixe trop large est couvert par le fait que
// seule une clé réellement déclarée dans le dictionnaire compte (les clés FR
// existent sinon la fabrication serait visible dans le compte total).
//
// Le `/\.$/` retire le point de terminaison, que le motif a déjà avalé : sans
// lui le préfixe vaut `fiche.etat..` (double point) et `startsWith` ne matche
// **jamais** rien — mesuré, la porte restait rouge en déclarant 8 clés mortes
// alors que la passe était censée les couvrir.
const viaTemplate = (
  [...pageSansCommentaires.matchAll(new RegExp('\\bt\\(\\s*`([a-z][a-zA-Z0-9_.-]*)\\$\\{', 'g'))].map(
    (m) => m[1].replace(/\.$/, '') + '.',
  )
).flatMap((prefixe) => fr.filter((cle) => cle.startsWith(prefixe)));
const utilisees = new Set([
  ...litteraux,
  ...viaTables,
  ...viaTernaire,
  ...viaTable,
  ...viaTemplate,
]);

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
  //
  // Apostrophe **ou** guillemet double, comme `cles` plus haut. Mesuré le
  // 2026-10-15 : le dictionnaire compte **730** lignes en guillemets doubles et
  // **0** en guillemets simples. `FR_VALEURS` restait donc vide, donc
  // `appelsSansValeurs` ne pouvait rien retourner, donc la règle 3 — celle qui
  // existe pour attraper « l'écran affiche littéralement {n}/{total} chapitre » —
  // ne pouvait pas crier.
  //
  // C'est le **deuxième** exemplaire du même piège dans ce fichier : le premier,
  // sur `bornes()`/`cles`, était déjà corrigé et le dit en commentaire ligne 88.
  // Les deux motifs voisins doivent être corrigés **ensemble**, sinon l'un reste
  // le trou de l'autre. Le test de la porte est là-dessus : `la_regle_des_
  // placeholders_crie`, plus bas.
  const ligne = new RegExp(
    `^\\s*['"]${echappee}['"]\\s*:\\s*['"]([^'"]*)['"]`,
    'm',
  ).exec(i18n);
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
// Même correction que `FR_VALEURS` ci-dessus : `Q` et non une apostrophe nue,
// sinon une moitié de la règle est morte pendant que l'autre mord.
const appelsSansValeurs = [
  ...pageSansCommentaires.matchAll(
    new RegExp(`\\bt\\(\\s*${Q}(${K})${Q}\\s*(?:\\)|,(?!\\s*\\{))`, 'g'),
  ),
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

// --------------------------------------------------------------------------
// Règle 4 — un pluriel assemblé dans le JSX est intraduisible
// --------------------------------------------------------------------------
//
// `{n} chapitre{n > 1 ? 's' : ''}` produit « 1 chapitre » en français et
// « 1 chapters » en anglais : la langue est écrite **dans** l'expression, donc
// aucune clé ne peut la corriger. Le dictionnaire documente déjà la règle
// inverse (« les pluriels sont des clés séparées »), mais rien ne la vérifiait —
// c'est exactement ce que l'audit du 2026-10-08 a trouvé affiché dans le
// compteur de chapitres.
//
// Le motif est volontairement étroit : `{ … } mot { … condition … 's' … }`. Un
// plurif à la main s'écrit toujours ainsi ; un appel à `t()` ou une
// interpolation normale ne ressemble pas à ça.
const PLURIEL_EN_DUR = /\{[^{}]*\}[ \t]*[A-Za-zÀ-ÿ]{2,}[ \t]*\{[^{}]*\?[^{}]*'[a-zA-ZÀ-ÿ]{1,4}'[^{}]*\}/g;
const plurielsEnDur = [];
for (const chemin of modules) {
  // `modules` porte des chemins **absolus** (app/ ET lib/) : les reinscrire
  // avec `join(racine,'app', …)` donnerait `app/C:\…` (mesuré le 2026-10-11).
  const texte = readFileSync(chemin, 'utf8');
  const nom = chemin.slice(racine.length + 1);
  for (const m of texte.matchAll(PLURIEL_EN_DUR)) {
    plurielsEnDur.push(`    - ${nom} : ${m[0].replace(/\s+/g, ' ').slice(0, 70)}`);
  }
}
if (plurielsEnDur.length) {
  echecs.push(
    `${plurielsEnDur.length} pluriel(s) assemblé(s) dans le JSX :\n` +
      `${plurielsEnDur.join('\n')}\n` +
      `  Le nombre d'accords est écrit dans l'expression, donc aucune clé ne peut ` +
      `le traduire.\n` +
      `  Écris des clés séparées pour le singulier et le pluriel (le dictionnaire ` +
      `le fait déjà pour les notifications).`,
  );
}

console.log(
  `clés FR : ${fr.length} | clés EN : ${en.length} | ` +
    `littéraux trouvés dans app/ et lib/ : ${utilisees.size}`,
);

if (echecs.length) {
  console.error(`\ni18n: ${echecs.length} problème(s)\n`);
  for (const e of echecs) console.error(`  ${e}\n`);
  process.exit(1);
}
console.log('i18n: aucune clé morte, aucune traduction manquante, aucun placeholder nu.');
