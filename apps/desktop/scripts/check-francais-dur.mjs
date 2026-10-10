// Porte : aucune chaîne française en dur dans le JSX rendu.
//
// Le défaut mesuré le 2026-10-09 (audit) : 14 occurrences signalées, dont
// plusieurs étaient des faux positifs (chaînes citées dans des commentaires ou
// du code). Vérifié après nettoyage : **3 vraies**, toutes des `aria-label`
// (`Aller à la page`, `Sélection rapide des chapitres`, `Filtrer les chapitres
// par numéro ou titre`). Toutes sont lues par un lecteur d'écran dans les DEUX
// langues, donc un `aria-label` non traduit est invisible pour qui navigue au
// clavier.
//
// `check-i18n.mjs` ne peut PAS couvrir ça : son propre en-tée explique qu'une
// recherche de français en dur crierait sur les commentaires et les messages
// d'erreur. Cette porte est donc volontairement étroite — elle ne regarde que
// ce qui est **rendu** : attributs (`aria-label`, `title`, `placeholder`,
// `alt`) et contenu texte d'une balise.
//
// Elle ne cherche pas les fautes de frappe dans un nom correct, ni une chaîne
// sans accent : `pages` ou `Terminé` (déjà traduit ailleurs) ne comptent pas.
// Une porte trop large s'apprend à ignorer.

import { readFileSync, readdirSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const ici = dirname(fileURLToPath(import.meta.url));
const racine = join(ici, '..');

const modules = readdirSync(join(racine, 'app')).filter(
  (f) => f.endsWith('.ts') || f.endsWith('.tsx'),
);

// Accent français : ce qui distingue un libellé traduit d'un identifiant.
// Ne sert plus qu'au texte d'une balise (voir l'en-tête).
const FRANCAIS = /[àâçéèêëîïôùûü]/i;
// Une lettre quelconque, accentuée ou non.
const LETTRE = /[a-zà-ÿ]/i;

const problemes = [];

for (const f of modules) {
  const lignes = readFileSync(join(racine, 'app', f), 'utf8')
    .replace(/\/\*[\s\S]*?\*\//g, (m) => m.replace(/[^\n]/g, ' '))
    // Commentaires de ligne retirés : une chaîne citée en prose n'est pas
    // affichée.
    //
    //    `[ \t]*` et non `\s*` : `\s` matche `\n`, donc un `\s*` en tête d'un
    //    commentaire de ligne avalait le saut de ligne PRÉCÉDENT et décalait
    //    tout le fichier. Mesuré le 2026-10-10 : **47 lignes** de décalage, la
    //    porte nommait `page.tsx:4395` pour une chaîne située en `4436`. Une
    //    porte qui accuse la mauvaise ligne est une porte qu'on finit par
    //    ignorer (même leçon que `check-hooks.mjs`, §14).
    .replace(/^[ \t]*\/\/[^\n]*/gm, (m) => ' '.repeat(m.length))
    .split(/\r?\n/);

  for (let i = 0; i < lignes.length; i++) {
    const l = lignes[i];
    // 1. Attributs rendus à l'utilisateur.
    //
    //    Trois formes de valeur littérale : `'…'`, `"…"`, et `` `…` `` — le
    //    gabarit peut porter une interpolation (`Couleur de ${c.name}`), qui
    //    est du français en dur AVEC une partie variable. Elle est signalée
    //    elle aussi : la moitié fixe n'est pas traduite.
    //
    //    Le `\{?` avant la citation est indispensable : en JSX, un littéral
    //    passé en attribut s'écrit `aria-label={`Couleur de ${c.name}`}`. Sans
    //    lui, la porte ne voyait que `attr="…"` et `attr='…'` — c'est-à-dire
    //    qu'elle ratait précisément les valeurs construites avec une
    //    interpolation. Mesuré le 2026-10-10 : `aria-label={`Couleur de
    //    ${c.name}`}` (page.tsx:4102) passait.
    for (const m of l.matchAll(
      /\b(?:aria-label|title|placeholder|alt)\s*=\s*\{?\s*(?:'([^']*)'|"([^"]*)"|`([^`]*)`)/g,
    )) {
      const v = m[1] || m[2] || m[3] || '';
      // Une valeur sans aucune lettre n'est pas un libellé : `alt=""`,
      // `aria-label="3"`, `title="…"`.
      if (LETTRE.test(v)) {
        const fixe = v.includes('{');
        problemes.push(
          `${f}:${i + 1} — attribut affiché en dur${fixe ? ' (avec interpolation)' : ''} : « ${v} »`,
        );
      }
    }
    // 2. Contenu texte d'une balise, sans expression ni imbrication.
    for (const m of l.matchAll(/>\s*([^<>{}]*[àâçéèêëîïôùûü][^<>{}]*?)\s*</gi)) {
      const v = m[1].trim();
      // `</` ouvrirait une balise fermante : ce n'est pas du texte.
      if (v.length > 1 && !/^<\//.test(v)) {
        problemes.push(`${f}:${i + 1} — texte affiché en français : « ${v} »`);
      }
    }
  }
}

if (problemes.length > 0) {
  console.error('libelles: du texte est affiché en dur.');
  console.error('Ces chaînes ne sont pas traduites, et un `aria-label` non traduit');
  console.error('est invisible pour qui navigue au clavier.');
  console.error('Un attribut doit venir de `t(…)` : sa valeur est lue dans les deux langues.');
  for (const p of problemes) console.error('  ' + p);
  process.exit(1);
}
console.log(`libelles: aucun libellé en dur sur ${modules.length} modules de app/.`);
