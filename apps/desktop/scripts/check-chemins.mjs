// Porte « chemins affichés » : un chemin ne s'affiche qu'après le retrait de la
// forme longue `\\?\`.
//
// Le fait mesuré (2026-10-05) : le cœur renvoie le chemin tel que
// `canonicalize` le produit, donc `\\?\C:\Users\alexanders\Documents\Tatami`.
// L'écran affichait cette valeur **telle quelle**. Elle est valide — c'est même
// elle qui débloque les chemins de plus de 260 caractères — mais illisible, et
// surtout incollable dans un explorateur ou un terminal.
//
// La porte refuse donc deux choses :
//  1. un chemin rendu sans passer par `cheminLisible(...)` ;
//  2. un appel à `cheminLisible` qui ne sert à rien (le retour à la valeur
//     brute), c'est-à-dire une porte qui aurait été contournée.
//
// Elle est volontairement lexicale, comme `check-i18n.mjs` et
// `check-hooks.mjs` : c'est une porte d'oubli, pas une preuve d'absence.

import { readdirSync, readFileSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const ici = dirname(fileURLToPath(import.meta.url));
const racine = join(ici, '..');

const modules = readdirSync(join(racine, 'app')).filter(
  (f) => f.endsWith('.ts') || f.endsWith('.tsx'),
);

const problemes = [];
const usages = [];

for (const f of modules) {
  const brut = readFileSync(join(racine, 'app', f), 'utf8');
  const source = brut
    .replace(/\/\*[\s\S]*?\*\//g, (m) => m.replace(/[^\n]/g, ' '))
    .replace(/\/\/[^\n]*/g, (m) => ' '.repeat(m.length));
  const lignes = source.split(/\r?\n/);

  for (let i = 0; i < lignes.length; i++) {
    // 1. Un chemin rendu en clair.
    //    On cherche `{identifiant.chemin}` ou `{identifiant}` juste après un
    //    attribut de présentation, sans appel à `cheminLisible`.
    const rendu = lignes[i].match(
      /className="(?:path|mono)"[^>]*>\{([A-Za-z0-9_$.]+)\}/,
    );
    if (rendu && !rendu[1].includes('cheminLisible')) {
      problemes.push(
        `${f}:${i + 1} — \`${rendu[1]}\` est affiche brut ; ` +
          'passer par cheminLisible(…)',
      );
    }

    // 2. Un appel à `cheminLisible` neutralisé.
    if (/cheminLisible\(/.test(lignes[i])) {
      usages.push(`${f}:${i + 1}`);
      const neutre =
        /\.replace\(/.test(lignes[i]) &&
        /slice\(4\)|startsWith/.test(lignes[i]) === false;
      if (neutre) {
        problemes.push(
          `${f}:${i + 1} — cheminLisible() neutralise par un .replace() : ` +
            'la forme longue doit partir par un test de prefixe',
        );
      }
    }
  }
}

if (problemes.length > 0) {
  console.error('chemins: un chemin est affiche sans retrait de la forme `\\\\?\\`.');
  console.error('La forme longue est valide mais illisible : elle se retire a');
  console.error("l'affichage, jamais en interne (c'est elle qui resout les chemins longs).");
  for (const p of problemes) console.error('  ' + p);
  process.exit(1);
}

console.log(
  `chemins: ${modules.length} modules lus, ` +
    `${usages.length} affichage(s) via cheminLisible, aucune valeur brute.`,
);