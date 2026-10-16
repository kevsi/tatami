// Porte : aucune classe CSS utilisee sans etre definie.
//
// Mesure le 2026-10-09 : j'ai ecrit `--erreur` dans `.queue-raison`, et
// **rien ne l'a signalise**. `var(--erreur)` est valide syntaxiquement ; quand
// la variable n'existe pas, la propriete retombe sur sa valeur heritee. Le
// texte se serait donc affiche en gris au lieu de rouge, sans erreur, sans
// console, sans test.
//
// C'est le troisieme cas de token invente en deux jours (AGENTS.md §14 en
// documente deux). La regle etait « chercher le token avant de l'ecrire » ;
// cette porte la rend mecanique.
//
// Elle compare les classes ** utilisees** dans le JSX a celles ** definies **
// dans globals.css, et les variables `var(--x)` aux tokens declares. Elle ne
// cherche pas les fautes de frappe dans un nom correct — seulement ce qui
// n'existe nulle part.

import { readFileSync, readdirSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const ici = dirname(fileURLToPath(import.meta.url));
const racine = join(ici, '..');

const modules = readdirSync(join(racine, 'app')).filter(
  (f) => f.endsWith('.ts') || f.endsWith('.tsx'),
);
const jsx = modules
  .map((f) => {
    let t = readFileSync(join(racine, 'app', f), 'utf8');
    // Commentaires retires : une classe citee en prose n'est pas utilisee.
    t = t.replace(/\/\*[\s\S]*?\*\//g, (m) => m.replace(/[^\n]/g, ' '));
    t = t.replace(/^\s*\/\/[^\n]*/gm, (m) => ' '.repeat(m.length));
    return t;
  })
  .join('\n');

// --- Variables CSS : `var(--x)` utilisees contre les tokens declares ---
const cssBrut = readFileSync(join(racine, 'app', 'globals.css'), 'utf8');
// Commentaires retires **en preservant la numerotation** (`[^\n]` -> espace) :
// sinon une classe citee en commentaire compte comme definie, et la porte ne
// voit plus la faute qu'elle pretend fermer. Mesure le 2026-10-15 : c'etait le
// cas — `definiesClasses` etait extrait du texte brut, commentaires inclus.
const css = cssBrut.replace(/\/\*[\s\S]*?\*\//g, (m) => m.replace(/[^\n]/g, ' '));
const declares = new Set(
  [...css.matchAll(/^\s*(--[a-z0-9-]+)\s*:/gm)].map((m) => m[1]),
);
// Un `var(--x, defaut)` a un repli qui, lui, est valide : on ne compte donc
// que `var(--x)` sans second argument, faute de quoi une porte crierait pour
// un usage parfaitement correct.
const utiliseesVar = new Set(
  [...jsx.matchAll(/var\((--[a-z0-9-]+)\)/g)].map((m) => m[1]),
);
// **Le meme controle sur globals.css lui-meme.** Mesure le 2026-10-15 : les
// usages n'etaient releves que dans le JSX, alors que la feuille de style en
// compte elle aussi — dont tous ceux du commit `fbef430`
// (`color: var(--accent-texte)`). Un `--accent-textee` mal orthographie y serait
// tombe sur la valeur heritee, sans erreur, sans console, sans test : exactement
// le defaut que cette porte existe pour fermer, mais dans un fichier qu'elle ne
// lisait pas. Un token declare `--x: var(--y)` est un usage de `--y`, donc il
// compte aussi.
for (const m of css.matchAll(/var\((--[a-z0-9-]+)\)/g)) {
  utiliseesVar.add(m[1]);
}
// Les variables definies dans un `style={{ ... }}` inline sont elles aussi
// legales : on les cherche dans le JSX.
const definiesInline = new Set(
  [...jsx.matchAll(/(--[a-z0-9-]+)\s*:/g)].map((m) => m[1]),
);

// --- Classes : `className="x"` (liste separee par espaces) utilisees ---
// Les classes litterales. **Les gabarits (`\`…\``) sont exclus ici** : ils sont
// traites separement plus bas, avec une decomposition correcte. Les inclure
// ici ferait analyser `${c.color === k + 1 ? ' on' : ''}` naivement et en
// sortir une « classe » `k` — mesure : faux positif de cette porte.
const utiliseesClasses = new Set();
for (const m of jsx.matchAll(/className=(?:"([^"]*)"|\{'([^']*)'\})/g)) {
  for (const brut of [m[1], m[2]]) {
    if (!brut) continue;
    for (const c of brut.split(/\s+/)) {
      if (/^[a-z][a-z0-9-]*$/i.test(c)) utiliseesClasses.add(c);
    }
  }
}
// Les classes qui viennent d'un gabarit avec `${}` : on extrait la partie fixe.
// Pourquoi ce n'est pas trivial : `${c.color === k + 1 ? ' on' : ''}` ferait
// sortir « k + 1 », c'est-a-dire une « classe » `k` qui n'existe pas. Mesuré :
// deuxieme faux positif de cette porte en une seule execution. Une porte qui
// accuse une classe imaginaire apprend a etre ignoree.
// On ne garde donc que les segments **hors** expression, et on coupe au premier
// caractere qui ne peut pas appartenir a un nom de classe.
for (const m of jsx.matchAll(/className=\{`([^`]*)`\}/g)) {
  const parties = m[1].split(/\$\{[^}]*\}/);
  for (const partie of parties) {
    for (const c of partie.split(/\s+/)) {
      // Un segment finissant par `-` est un **prefixe** (`badge state-${…}`),
      // pas une classe : la classe complete est formee ailleurs. Le compter
      // ferait crier la porte pour un usage correct — `state-` n'est jamais
      // une classe, `state-attente` en est une.
      if (c.endsWith('-')) continue;
      if (/^[a-z][a-z0-9-]*$/i.test(c)) utiliseesClasses.add(c);
    }
  }
}
// `classList.add('x')` et compagnie.
for (const m of jsx.matchAll(/classList\.(?:add|remove|toggle)\(\s*'([^']+)'/g)) {
  utiliseesClasses.add(m[1]);
}
// Une classe precedee de `.` dans un querySelector est un **usage** aussi.
for (const m of jsx.matchAll(/querySelector(?:All)?\(\s*['"`]\.([a-z][a-z0-9-]*)/g)) {
  utiliseesClasses.add(m[1]);
}

const definiesClasses = new Set(
  [...css.matchAll(/\.([a-z][a-z0-9-]*)/g)].map((m) => m[1]),
);

// --- Comparaison ---
const problemes = [];

for (const v of utiliseesVar) {
  if (!declares.has(v) && !definiesInline.has(v)) {
    problemes.push(
      `variable CSS \`${v}\` utilisee mais jamais declaree : ` +
        'une valeur inconnue retombe sur l\'heritee, donc l\'erreur est muette',
    );
  }
}

for (const c of [...utiliseesClasses].sort()) {
  if (!definiesClasses.has(c)) {
    problemes.push(`classe \`${c}\` utilisee mais jamais definie dans globals.css`);
  }
}

// Un token qui se reference lui-meme. Mesure le 2026-10-15 :
// `--encours-texte: var(--encours-texte)` dans le theme sombre. Le CSS est
// parfaitement valide a la compilation, donc `tsc` ne dit rien, `styles:check`
// disait « declare », et `eslint` non plus — mais la valeur calculee est
// `guaranteed-invalid`, donc `color: var(--encours-texte)` retombe sur la valeur
// **heritee**. Le theme clair redefinissait le token correctement, donc le defaut
// n'existait que sur le theme sombre, c'est-a-dire celui par defaut.
for (const m of css.matchAll(/^\s*(--[a-z0-9-]+)\s*:\s*var\(\s*\1\s*[,)]/gm)) {
  problemes.push(
    `token \`${m[1]}\` se reference lui-meme : sa valeur calculee est invalide, ` +
      `donc \`var(${m[1]})\` retombe sur l'heritee au lieu de la couleur voulue`,
  );
}

if (problemes.length > 0) {
  console.error('styles: des classes ou variables sont utilisees sans etre definies.');
  console.error('Une valeur inconnue ne casse rien : elle se taise. D\'ou cette porte.');
  for (const p of problemes) console.error('  ' + p);
  process.exit(1);
}
console.log(
  `styles: ${utiliseesClasses.size} classes et ${utiliseesVar.size} variables, ` +
    'toutes definies.',
);
