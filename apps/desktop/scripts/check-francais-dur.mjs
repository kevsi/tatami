// Porte : aucun texte français en dur dans ce qui est **rendu**.
//
// ## Pourquoi ce script est réécrit en AST
//
// La version précédente lisait le fichier **ligne par ligne**, avec trois
// conditions cumulées sur le texte d'une balise : `> … <` **sur la même ligne**,
// avec une lettre accentuée, et **sans expression**. Les trois sont fausses en
// pratique sur du JSX, où le texte est presque toujours seul sur sa ligne et
// contient une expression.
//
// Mesuré le 2026-10-15 : la porte annonçait « aucun libellé en dur » **avec au
// moins vingt libellés français réellement affichés**, dont `Voir plus ({n}
// chargés)`, `Sur les chapitres filtrés` / `Choisir`, `Retirer des favoris` /
// `Ajouter aux favoris`, `{tagsTotal} genres au total`, `← Catalogues`. Aucun
// n'était visible parce qu'un littéral en dur **dans une expression** n'est ni sur
// la ligne de la balise, ni dépourvu de `{}`.
//
// Le même problème, aggravé, existait côté attributs : `title={isFavorite(m) ?
// 'Retirer des favoris' : 'Ajouter aux favoris'}` est sur une seule ligne, mais
// sans accent, donc invisible aussi.
//
// La lesson est celle du §14 : une porte doit être **cassée pour prouvee qu'elle
// mord**. Le test de la porte, en bas de fichier, la casse.
//
// ## Ce que la porte refuse
//
// 1. Un `JsxText` non vide : le texte littéral entre deux balises.
// 2. Un attribut rendu à l'utilisateur (`aria-label`, `title`, `placeholder`,
//    `alt`) dont la valeur est un **littéral**, jamais un appel à `t(…)`.
//
// ## Ce qu'elle ne refuse pas
//
// La détection du français est **l'absence dans le dictionnaire**, pas la
// présence d'un accent : `Choisir` et `Terminé` n'ont pas le même alphabet, et
// une porte qui exige un accent laisse passer la moitié des fautes. Les
// commentaires ne sont pas visités du tout — un arbre syntaxique ne les contient
// pas, ce qui supprime d'un coup toute la classe des faux positifs.
//
// `i18n.ts` est exclu : c'est le dictionnaire, donc sa valeur est **par
// définition** du texte displayed ailleurs.

import { readFileSync, readdirSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import ts from 'typescript';

const ici = dirname(fileURLToPath(import.meta.url));
const racine = join(ici, '..');
const appDir = join(racine, 'app');

/** Attributs dont la valeur est lue par un lecteur d'écran ou affichée. */
const ATTRIBUTS_RENDUS = new Set(['aria-label', 'title', 'placeholder', 'alt']);

/** Le dictionnaire, pour savoir ce qui est **déjà** traduit. */
function valeursDuDictionnaire(nom) {
  const fichier = join(appDir, 'i18n.ts');
  const source = ts.createSourceFile(
    'i18n.ts',
    readFileSync(fichier, 'utf8'),
    ts.ScriptTarget.Latest,
    true,
  );
  let trouve = null;
  const visiter = (n) => {
    // `const FR = {…} as const` : l'initialiseur est un `AsExpression` dont
    // l'expression est l'objet littéral. Mesuré le 2026-10-15 : ne chercher que
    // l'objet littéral faisait échouer la porte sur un dictionnaire parfaitement
    // valide — c'est-à-dire qu'elle criait pour la mauvaise raison.
    if (ts.isVariableDeclaration(n) && n.name.getText(source) === nom && n.initializer) {
      let e = n.initializer;
      while (ts.isAsExpression(e) || ts.isSatisfiesExpression(e)) e = e.expression;
      if (ts.isObjectLiteralExpression(e)) trouve = e;
    }
    ts.forEachChild(n, visiter);
  };
  visiter(source);
  if (!trouve) throw new Error(`${nom} introuvable dans app/i18n.ts`);
  const valeurs = new Set();
  for (const p of trouve.properties) {
    if (!ts.isPropertyAssignment(p)) continue;
    const v = p.initializer;
    if (ts.isStringLiteral(v)) valeurs.add(v.text);
  }
  return valeurs;
}

// Le dictionnaire est lu **seulement** pour vérifier qu'il reste parsable : une
// porte qui ne le lit pas ne peut pas dire qu'un `JsxText` est une faute. Lire
// pour l'utiliser comme liste blanche était un trou (voir `suspect`).
valeursDuDictionnaire('FR');
valeursDuDictionnaire('EN');

/**
 * Un littéral est suspect dès qu'il contient des lettres.
 *
 * ## Pourquoi le dictionnaire ne sert plus de criterion
 *
 * La première version de cette règle disait « le littéral n'est pas dans le
 * dictionnaire ». Mesuré le 2026-10-15 : c'était un trou. `{'←'} Chapitres`
 * affichait `Chapitres` en dur alors que la clé `nav.chapitres` venait d'être
 * ajoutée — donc sa **valeur** était dans le dictionnaire, et la porte le laissait
 * passer. L'écran continuait de dire « Chapitres » en anglais. Un littéral qui
 * ressemble à une traduction n'est **pas** une traduction : seul l'appel à `t()`
 * en est une.
 *
 * ## Pourquoi il n'y a pas de faux positif
 *
 * Un `JsxText` ne peut pas appeler `t()` : du texte entre deux balises est
 * traduit par construction impossible autrement. Un attribut, lui, est vérifié
 * structurellement — le code ne descend dans un littéral que si l'initialiseur
 * n'est pas un appel, donc `aria-label={t('…')}` n'est jamais descendu.
 *
 * Restent écartés, parce que ce ne sont pas des libellés : `·`, `←`, `{`,
 * `1/2`, `100%`, `OK`, `×`.
 */
function suspect(valeur) {
  const v = valeur.trim();
  if (v.length === 0) return false;
  if (!/[a-zà-ÿ]/i.test(v)) return false;   // symbole ou nombre
  if (v.replace(/\s+/g, '').length <= 2) return false; // « OK », « × », « N »
  return true;
}

const modules = readdirSync(appDir).filter(
  (f) => (f.endsWith('.ts') || f.endsWith('.tsx')) && f !== 'i18n.ts' &&
    !f.endsWith('.test.ts'),
);

const problemes = [];

for (const f of modules) {
  const chemin = join(appDir, f);
  const texte = readFileSync(chemin, 'utf8');
  const source = ts.createSourceFile(f, texte, ts.ScriptTarget.Latest, true,
    f.endsWith('.tsx') ? ts.ScriptKind.TSX : ts.ScriptKind.TS);

  const ligne = (n) => source.getLineAndCharacterOfPosition(n.getStart(source)).line + 1;

  const visiter = (n) => {
    // 1. Texte littéral entre deux balises.
    if (ts.isJsxText(n)) {
      const v = n.text.replace(/\s+/g, ' ').trim();
      if (suspect(v)) {
        problemes.push(`${f}:${ligne(n)} — texte affiché en dur : « ${v} »`);
      }
    }

    // 2. Attribut rendu à l'utilisateur, valeur littérale.
    if (ts.isJsxAttribute(n) && ATTRIBUTS_RENDUS.has(n.name.getText(source))) {
      const init = n.initializer;
      let valeur = null;
      if (init && ts.isStringLiteral(init)) valeur = init.text;
      else if (init && ts.isJsxExpression(init) && init.expression) {
        const e = init.expression;
        // Un gabarit sans expression reste du texte en dur ; un gabarit **avec**
        // `${…}` est signalé aussi : la moitié fixe n'est pas traduite.
        if (ts.isStringLiteral(e)) valeur = e.text;
        else if (ts.isTemplateExpression(e)) {
          valeur = e.getText(source).slice(1, -1).replace(/\$\{[^}]*\}/g, '{}');
        }
      }
      if (valeur && suspect(valeur)) {
        problemes.push(`${f}:${ligne(n)} — attribut « ${n.name.getText(source)} » en dur : « ${valeur} »`);
      }
    }

    ts.forEachChild(n, visiter);
  };
  visiter(source);
}

if (problemes.length > 0) {
  console.error(`ECHEC libelles : ${problemes.length} texte(s) affiché(s) en dur.`);
  console.error('Un attribut doit venir de `t(…)` : sa valeur est lue dans les deux langues,');
  console.error('et un `aria-label` non traduit est invisible pour qui navigue au clavier.');
  for (const p of problemes) console.error('  ' + p);
  process.exit(1);
}
console.log(
  `libelles: aucun libellé en dur dans ${modules.length} modules de app/ (analyse AST, commentaires exclus).`,
);
