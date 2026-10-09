// Porte : la fiche d'une série ne doit contenir aucun libellé en dur.
//
// Le défaut mesuré le 2026-10-09 (audit) : `infoRows` était une fonction de
// **module** — sans accès à `t`, six libellés étaient en français en dur
// (`Statut`, `Auteur`, `Genres`, `Titre alternatif`, `Note`, `Source`) et
// `STATE_FR` traduisait les 8 états en français en dur. En anglais, l'écran
// affichait donc « Statut / Auteur / Genres / Note » au milieu d'un reste
// traduit — l'écran *paraissait* traduit.
//
// `check-i18n.mjs` ne peut pas le voir : son propre en-tête (`:8-17`) dit ne pas
// chercher de français en dur, parce qu'une telle porte crierait sur les
// commentaires et les messages d'erreur. Cette porte-là est donc
// **volontairement étroite** : elle ne regarde qu'un fichier et qu'une liste
// de libellés connus.

import { readFileSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const ici = dirname(fileURLToPath(import.meta.url));
const racine = join(ici, '..');

const LITTERAUX = [
  'Statut',
  'Auteur',
  'Genres',
  'Titre alternatif',
  'Note',
  'Source',
  'En cours',
  'Terminé',
  'Abandonné',
  'Sous licence',
  'Publication terminée',
  'En pause',
  'Pas encore publié',
  'Interrompu',
];

const source = readFileSync(join(racine, 'app', 'page.tsx'), 'utf8');
// Commentaires et littéraux de template conservés : c'est le texte affiché
// que l'on cherche, pas le code mort. On ne garde que ce qui est **rendu**.
const lignes = source
  .replace(/\/\*[\s\S]*?\*\//g, (m) => m.replace(/[^\n]/g, ' '))
  .replace(/^\s*\/\/[^\n]*/gm, (m) => ' '.repeat(m.length))
  .split(/\r?\n/);

const problemes = [];

for (let i = 0; i < lignes.length; i++) {
  for (const mot of LITTERAUX) {
    // Un libellé rendu en dur : uniquement `label: 'Mot'`. Le reste — texte
    // dans un `<span>`, clé de dictionnaire, mot quelconque — ne compte pas :
    // mesuré, « Source » apparaît dans « webtoons » et faisait crier la porte
    // pour une ligne qui est déjà traduite. Une porte qui signale des fautes
    // imaginaires apprend à l'ignorer.
    const motif = new RegExp("label:\\s*['\"]" + mot.replace(/[.*+?^${}()|[\]\\]/g, '\\$&') + "['\"]");
    if (motif.test(lignes[i])) {
      problemes.push(`page.tsx:${i + 1} — libellé en dur : '${mot}' (attendu via le dictionnaire)`);
    }
  }
  // États de série en dur : la table de correspondance, et elle seule.
  if (/^\s*(ONGOING|FINISHED|ABANDONED|LICENSED|PUBLISHING_FINISHED|ON_HIATUS|NOT_YET|DISCONTINUED|UNKNOWN):/.test(lignes[i])) {
    problemes.push(`page.tsx:${i + 1} — table d'états en dur`);
  }
  if (/STATE_FR/.test(lignes[i])) {
    problemes.push(`page.tsx:${i + 1} — table d'états STATE_FR (nom attendu : traductions via le dictionnaire)`);
  }
}

if (problemes.length > 0) {
  console.error('fiche: des libelles sont en dur.');
  console.error('Ils ne sont pas traduits : `infoRows` est une fonction de module, donc sans `t`.');
  console.error('Correction : passer `t` a infoRows() et ajouter les cles fiche.* au dictionnaire.');
  for (const p of problemes) console.error('  ' + p);
  process.exit(1);
}
console.log(`fiche: aucun libelle en dur sur ${lignes.length} lignes inspectees.`);
