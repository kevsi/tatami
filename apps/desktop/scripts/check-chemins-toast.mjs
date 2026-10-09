// Porte « chemins affichés », cas n° 4 : un chemin interpolé dans un **toast**
// ou une **notice**.
//
// Mesuré le 2026-10-09 (audit) : `page.tsx:2506` fait
// `setNotice(t('notice.telecharge', { f: file }))`, où `file` vient de
// `run_download`, donc de `downloads_root()` — que `verifier_dossier_ecriture`
// a canonicalisé, donc de la forme longue `\\?\C:\Users\…`.
//
// La porte existante ne le voyait pas : elle ne cherche que `{ident}` **juste
// après** `className="path"` ou `className="mono"` (`:43-45`). Un toast n'a ni
// classe ni balise. C'est la troisième fois que la forme longue fuit par un
// chemin que la porte ne couvre pas — les deux autres sont dans AGENTS.md §14.
//
// Cette variante est donc la preuve que la porte couvre le cas qu'elle prétend
// couvrir. Elle doit ACCUSER ce fichier tant que `cheminLisible` n'y est pas.

import { readFileSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const ici = dirname(fileURLToPath(import.meta.url));
const racine = join(ici, '..');
const brut = readFileSync(join(racine, 'app', 'page.tsx'), 'utf8');
const lignes = brut
  .replace(/\/\*[\s\S]*?\*\//g, (m) => m.replace(/[^\n]/g, ' '))
  .replace(/\/\/[^\n]*/g, (m) => ' '.repeat(m.length))
  .split(/\r?\n/);

const problemes = [];

// Un chemin interpolé dans `t('…', { … })` : c'est un texte destiné à l'écran.
for (let i = 0; i < lignes.length; i++) {
  const m = lignes[i].match(/setNotice\(\s*t\(\s*'([^']+)'\s*,\s*\{([^}]*)\}/);
  if (!m) continue;
  const params = m[2];
  for (const p of params.matchAll(/([A-Za-z0-9_$]+)\s*:/g)) {
    const nom = p[1];
    if (/^(file|f|chemin|cheminComplet|dir|path|dossier)$/i.test(nom)) {
      if (!lignes[i].includes('cheminLisible')) {
        problemes.push(
          `page.tsx:${i + 1} — \`${m[1]}\` recoit \`${nom}\` brut ; ` +
            'un chemin dans un toast doit passer par cheminLisible(…)',
        );
      }
    }
  }
}

if (problemes.length > 0) {
  console.error('chemins: un chemin est affiche sans retrait de la forme `\\\\?\\`.');
  console.error('Cas cover : un toast ou une notice, pas seulement un <code>.');
  for (const p of problemes) console.error('  ' + p);
  process.exit(1);
}
console.log(`chemins: ${lignes.length} lignes inspectees, aucun chemin brut dans un toast.`);
