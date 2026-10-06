// Porte « dépendances d'effet » : aucune dépendance d'un `useEffect` ne peut
// être une valeur **recréée à chaque rendu**.
//
// Le fait mesuré qui fonde cette porte (2026-10-05) : `useI18n()` rend `t`
// sous forme de fonction recréée à chaque rendu. Un `useEffect` qui la liste
// dans ses dépendances **et** écrit dans l'état est une boucle infinie :
//
//   rendu n   -> `t` est nouvelle  -> l'effet se rejoue
//             -> `setCardRecent`   -> rendu n+1 -> `t` est encore nouvelle -> …
//
// React abandonne après 50 rendus et affiche « Maximum update depth exceeded ».
// Ça a été **observé en pilotant l'application** (12 fois à l'ouverture d'une
// série), pas déduit : le même piège avait déjà fait appeler `set_libelles`
// 18 fois au démarrage en 3.4.
//
// Deux règles, volontairement étroites :
//
//  1. aucun tableau de dépendances d'un `useEffect` ne contient `t` ;
//  2. aucun ne contient un `useCallback` dont les **propres** dépendances
//     contiennent `t` — l'instabilité se transmet par la fonction.
//
// `useCallback([t])` n'est pas interdit en soi : sans danger quand la fonction
// n'est appelée que depuis un gestionnaire de clic (elle n'est alors jamais
// relancée par un effet). La porte vise donc l'effet, pas le rappel.
//
// Limite assumée : la porte est lexicale, comme `check-i18n.mjs`. Un effet dont
// la dépendance est masquée par un spread (`...deps`) n'est pas vu ; le
// compilateur et le pilotage restent les vraies preuves.

import { readdirSync, readFileSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const ici = dirname(fileURLToPath(import.meta.url));
const racine = join(ici, '..');

const modules = readdirSync(join(racine, 'app')).filter(
  (f) => f.endsWith('.ts') || f.endsWith('.tsx'),
);

/**
 * Retire les commentaires **sans décaler les numéros de ligne**.
 *
 * Mesuré le 2026-10-05 : une première version remplaçait un commentaire de
 * bloc par un seul espace, donc un `/* … *\/` de 246 lignes décalait tout ce
 * qui suivait et la porte accusait la ligne 1734 au lieu de 1980. Une porte
 * qui accuse le mauvais endroit est une porte qu'on finit par ignorer : on
 * remplace chaque caractère non-saut-de-ligne par un espace.
 */
function sansCommentaires(source) {
  return source
    .replace(/\/\*[\s\S]*?\*\//g, (m) => m.replace(/[^\n]/g, ' '))
    .replace(/\/\/[^\n]*/g, (m) => ' '.repeat(m.length));
}

/** Découpe le tableau de dépendances `[a, b]` d'une fermeture `}, [...]) ;`. */
function tableauALaFinLigne(ligne) {
  const m = ligne.match(/^\s*\}?,?\s*\[([^\]]*)\]\s*\)?\s*;?\s*$/);
  return m ? m[1] : null;
}

function noms(deps) {
  return deps
    .split(',')
    .map((d) => d.trim())
    .filter(Boolean)
    .map((d) => d.split(/[:.=]/)[0].trim())
    .filter(Boolean);
}

/** Un `useCallback` / `useMemo` dont les dépendances contiennent `t`. */
function rappelsInstables(source, fichier) {
  const lignes = source.split(/\r?\n/);
  const instables = new Map();
  for (let i = 0; i < lignes.length; i++) {
    const m = lignes[i].match(/const\s+(\w+)\s*=\s*useCallback\b/);
    if (!m) continue;
    // Le tableau de dépendances est en fin de l'appel : on descend jusqu'à
    // la première ligne qui ressemble à `}, [...]);`.
    for (let j = i; j < Math.min(lignes.length, i + 200); j++) {
      const deps = tableauALaFinLigne(lignes[j]);
      if (deps === null) continue;
      if (noms(deps).includes('t')) {
        instables.set(m[1], { fichier, ligne: j + 1 });
      }
      break;
    }
  }
  return instables;
}

const problemes = [];
const instables = new Map();

for (const f of modules) {
  const chemin = join(racine, 'app', f);
  const brut = readFileSync(chemin, 'utf8');
  const source = sansCommentaires(brut);
  const lignes = source.split(/\r?\n/);

  for (const [nom, ou] of rappelsInstables(source, f)) {
    instables.set(nom, ou);
  }

  for (let i = 0; i < lignes.length; i++) {
    if (!/^\s*useEffect\s*\(/.test(lignes[i])) continue;
    for (let j = i; j < Math.min(lignes.length, i + 400); j++) {
      const deps = tableauALaFinLigne(lignes[j]);
      if (deps === null) continue;
      for (const dep of noms(deps)) {
        if (dep === 't') {
          problemes.push(`${f}:${j + 1} — useEffect dépend de \`t\``);
        } else if (instables.has(dep)) {
          const ou = instables.get(dep);
          problemes.push(
            `${f}:${j + 1} — useEffect dépend de \`${dep}\`, ` +
              `useCallback([t]) défini en ${ou.fichier}:${ou.ligne}`,
          );
        }
      }
      break;
    }
  }
}

if (problemes.length > 0) {
  console.error('hooks:.useEffect dépend d une valeur recréée à chaque rendu.');
  console.error("`t` est une nouvelle fonction à chaque rendu, donc l'effet se rejoue");
  console.error('indéfiniment dès qu il écrit dans l\'état (« Maximum update depth');
  console.error('exceeded »). Dépendez du primitif `langue`.');
  for (const p of problemes) console.error('  ' + p);
  process.exit(1);
}

console.log(
  `hooks: ${modules.length} modules lus, aucune dépendance d'effet instable.`,
);