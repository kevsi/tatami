// Porte : un type TypeScript déclaré pour la charge utile d'une commande Tauri
// doit nommer **exactement** les champs que la structure Rust sérialise.
//
// Mesuré le 2026-10-09 : `type Progression` était déclaré en camelCase
// (`chapterUrl`, `pageIndex`…) alors que `HistoryEntry` ne porte qu'un
// `#[derive(Serialize)]` **sans** `rename_all`, donc le cœur envoie
// `chapter_url`, `page_index`. Résultat : `progression.chapterUrl` valait
// `undefined`, `repriseUtile()` renvoyait `false`, et ni le marqueur de chapitre
// courant ni le bouton « reprendre » n'apparaissaient — sans qu'`invoke<Progression>`
// ni `tsc` ne signale quoi que ce soit. Un `invoke<Type>` est une **affirmation** :
// rien ne la vérifie, donc il faut une porte.
//
// Chaque paire (type TS ↔ struct Rust) est déclarée ici, avec son fichier. C'est
// une liste explicite plutôt qu'une heuristique : une règle automatique sur
// « tous les `invoke<T>` » ne saurait pas distinguer un type de charge utile
// d'un type local, et produirait des faux positifs qu'on finirait par ignorer.
//
// Ordre de grandeur attendu à l'échec : nom du type, champ TS, champ Rust le
// plus proche — pour que le message dise quoi corriger.

import { readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const ICI = dirname(fileURLToPath(import.meta.url));
const RENDU = join(ICI, '..', 'app', 'page.tsx');
const SRC_TAURI = join(ICI, '..', 'src-tauri', 'src');

/** Paires déclarées : type TypeScript, structure Rust, fichier Rust. */
const PAIRES = [
  ['Progression', 'HistoryEntry', 'store.rs'],
];

/** Champs d'un `type X = { … }` de `page.tsx`, dans l'ordre. */
function champsTypeTs(source, nom) {
  // Le nom est échappé : `HistoryEntry` ne contient pas de métacaractère, mais
  // une liste éditée à la main peut un jour contenir un `+` ou un `$`.
  const debut = source.search(
    new RegExp(`\\btype\\s+${nom.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}\\s*=\\s*\\{`),
  );
  if (debut < 0) return null;
  const ouvrante = source.indexOf('{', debut);
  let niveau = 0;
  for (let i = ouvrante; i < source.length; i += 1) {
    const c = source[i];
    if (c === '{') niveau += 1;
    else if (c === '}') {
      niveau -= 1;
      if (niveau === 0) {
        return source
          .slice(ouvrante + 1, i)
          .split('\n')
          .map((l) => /^\s*([A-Za-z_$][\w$]*)\s*[?]?\s*:/.exec(l))
          .filter(Boolean)
          .map((m) => m[1]);
      }
    }
  }
  return null;
}

/** Champs d'un `pub struct X { … }` Rust. */
function champsStructRust(source, nom) {
  const debut = source.search(
    new RegExp(`pub\\s+struct\\s+${nom.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}\\s*\\{`),
  );
  if (debut < 0) return null;
  const ouvrante = source.indexOf('{', debut);
  const fin = source.indexOf('\n}', ouvrante);
  if (fin < 0) return null;
  return source
    .slice(ouvrante + 1, fin)
    .split('\n')
    .map((l) => /^\s*pub\s+([A-Za-z_][\w]*)\s*:/.exec(l))
    .filter(Boolean)
    .map((m) => m[1]);
}

/** `chapterUrl` → `chapter_url` : le pont que l'erreur avait franchi. */
function versSerpent(s) {
  return s.replace(/([a-z0-9])([A-Z])/g, '$1_$2').toLowerCase();
}

const sourceTs = readFileSync(RENDU, 'utf8');
const echecs = [];

for (const [typeTs, structRust, fichier] of PAIRES) {
  const ts = champsTypeTs(sourceTs, typeTs);
  const rust = champsStructRust(readFileSync(join(SRC_TAURI, fichier), 'utf8'), structRust);

  if (!ts) {
    echecs.push(`  ${typeTs} : type introuvable dans app/page.tsx`);
    continue;
  }
  if (!rust) {
    echecs.push(`  ${typeTs} ↔ ${structRust} : structure introuvable dans ${fichier}`);
    continue;
  }
  // Un champ Rust peut être sérialisé sous un autre nom (`#[serde(rename)]`),
  // mais aucune structure du cœur n'en porte : on compare donc les noms bruts.
  for (const champ of ts) {
    if (!rust.includes(champ)) {
      const serpent = versSerpent(champ);
      const voisin = rust.includes(serpent)
        ? `le cœur sérialise « ${serpent} »`
        : `aucun champ de ${structRust} ne s'appelle « ${champ} »`;
      echecs.push(
        `  ${typeTs}.${champ} : ${voisin}` +
          (serpent !== champ && rust.includes(serpent)
            ? ` (le nom est en camelCase, le cœur envoie du snake_case)`
            : ''),
      );
    }
  }
}

if (echecs.length > 0) {
  console.error('ECHEC types IPC : le TypeScript ne nomme pas les champs que le cœur envoie.');
  echecs.forEach((l) => console.error(l));
  process.exit(1);
}

PAIRES.forEach(([a, b, c]) => console.log(`  ${a} ↔ ${b} (${c}) : concordance verifiee`));