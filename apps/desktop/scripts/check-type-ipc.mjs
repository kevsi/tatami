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
// scripts/ → desktop/ → apps/ → racine du dépôt.
const CONTRAT = join(ICI, '..', '..', '..', 'contract', 'protocol.json');
const protocole = JSON.parse(readFileSync(CONTRAT, 'utf8'));

/**
 * Paires déclarées : type TypeScript, structure Rust, fichier Rust.
 *
 * Étendue le 2026-10-15 : elle ne couvrait qu'**une** paire sur 26 charges utiles
 * déclarées à la main dans `page.tsx`. Un `invoke<Type>` reste une affirmation,
 * donc une porte qui n'en regarde qu'un sur treize laisse passer tous les autres.
 *
 * Chaque entrée a été relevée **à la main** sur les deux fichiers, jamais devinée :
 * c'est ce que le test de cette porte (`ipc:check` couplé à `cargo test`) sert à
 * vérifier. Les types qui n'ont pas de structure Rust correspondante — parce que
 * le cœur renvoie du JSON brut, ou parce que le type est une projection de
 * l'interface — sont traités plus bas, ou explicitement exclus.
 */
const PAIRES = [
  ['Progression', 'HistoryEntry', 'store.rs'],
  ['Favorite', 'Favorite', 'store.rs'],
  ['Category', 'Category', 'store.rs'],
  ['Bookmark', 'Bookmark', 'store.rs'],
  ['CustomSource', 'CustomSource', 'store.rs'],
  ['LibraryItem', 'LibraryItem', 'library.rs'],
  ['ImportRapport', 'ImportRapport', 'library.rs'],
  ['PlanReglages', 'ReglagesVue', 'scheduler.rs'],
  ['Progress', 'Progress', 'download.rs'],
  ['ResumeRecherche', 'Resume', 'recherche.rs'],
  ['EtatMaj', 'EtatMaj', 'maj.rs'],
  ['VersionDisponible', 'VersionDisponible', 'maj.rs'],
  ['ProgressionMaj', 'Progression', 'maj.rs'],
  ['RapportSauvegarde', 'RapportSauvegarde', 'backup.rs'],
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

/** Champs d'un `pub struct X { … }` Rust, **tels que serde les nommera**.
 *
 * Le `rename_all` compte. Mesuré le 2026-10-15 : `library::LibraryItem` porte
 * `#[serde(rename_all = "camelCase")]`, donc le cœur émet `coverUrl` — et la
 * version précédente de cette porte, qui lisait les noms bruts, a accused le
 * type TypeScript de cinq fautes **qui n'existaient pas**. C'est le pire défaut
 * possible pour une porte : une fausse accusation que l'on corrige en dégradant
 * le code. Le contrôle des deux sens l'a révélée.
 */
function champsStructRust(source, nom) {
  const echappe = nom.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
  const debut = source.search(new RegExp(`pub\\s+struct\\s+${echappe}\\s*\\{`));
  if (debut < 0) return null;
  const ouvrante = source.indexOf('{', debut);
  const fin = source.indexOf('\n}', ouvrante);
  if (fin < 0) return null;

  // L'attribut est cherché **avant** l'accolade du corps : c'est là qu'il se
  // trouve, entre le nom de la structure et sa première ligne de champ.
  const avantCorps = source.slice(Math.max(0, debut - 200), debut);
  const camel = /#\[serde\(rename_all\s*=\s*"camelCase"\)\]/.test(avantCorps);

  return source
    .slice(ouvrante + 1, fin)
    .split('\n')
    .map((l) => /^\s*pub\s+([A-Za-z_][\w]*)\s*:/.exec(l))
    .filter(Boolean)
    .map((m) => (camel ? versCamel(m[1]) : m[1]));
}

/** `cover_url` → `coverUrl` : l'inverse de `versSerpent`. */
function versCamel(s) {
  return s.replace(/_([a-z0-9])/g, (_, c) => c.toUpperCase());
}

/** `chapterUrl` → `chapter_url` : le pont que l'erreur avait franchi. */
function versSerpent(s) {
  return s.replace(/([a-z0-9])([A-Z])/g, '$1_$2').toLowerCase();
}

/** Types TS dont le producteur est le **schéma du contrat**, pas une
 *  structure Rust : le cœur renvoie du JSON brut (`serde_json::Value`), donc
 *  aucune discipline de structure ne s'applique. C'est ce qui a laissé passer
 *  `uploadDate` puis `premium` — un `invoke<Type>` reste une assertion, et le
 *  seul endroit décrivant ce que le moteur émet est `contract/protocol.json`.
 */
/**
 * Types dont le producteur est le **schéma du contrat**, pas une structure Rust :
 * le cœur renvoie du JSON brut (`serde_json::Value`), donc aucune discipline de
 * structure ne s'applique.
 *
 * Ce sont des **paires** et non une liste de noms, parce que la correspondance
 * n'est pas mécanique : le type `Source` de l'interface correspond à `$defs/
 * sourceInfo` du contrat, pas à `$defs/source`. Une dérivation par
 * `toLowerCase()` — ce que la version précédente faisait — aurait cherché
 * `$defs/source` et crié sur un schéma parfaitement correct.
 *
 * ## Champs ajoutés par le CŒUR, pas par le moteur
 *
 * `Page.token` est ajouté par `commands::injecter_dims_et_token` après la
 * réponse du moteur : il n'appartient donc pas à `$defs/page`, qui décrit ce que
 * le moteur émet. Le signaler comme absent du schéma serait une faute de la
 * porte, pas du type. Ces champs sont donc énumérés ici, avec leur producteur.
 *
 * `Tag` n'est **pas** une charge utile de résultat : c'est le type d'un
 * *élément* de `filterOptions.result.tags`. Le vérifier contre `$defs/
 * filterOptions` le comparerait à un objet entirely différent — d'où son
 * absence volontaire.
 */
const TYPES_SCHEMA = [
  ['Chapter', 'chapter'],
  ['Manga', 'manga'],
  ['Page', 'page'],
  ['Source', 'sourceInfo'],
];

/** Champs ajoutés par le cœur Rust après la réponse du moteur. */
const CHAMPS_AJOUTES_PAR_LE_COEUR = {
  Page: { token: 'commands::injecter_dims_et_token' },
  // Le jeton `tatami://cover/…` est minté par le cœur à partir de l'URL
  // distante : le moteur ne l'émet **jamais**, et c'est volontaire — la webview
  // n'a ni user-agent ni cookies, donc un appel direct à l'hôte d'images
  // échouerait en silence.
  Manga: { coverToken: 'commands::jeton (couverture locale)' },
};

/** Champs déclarés dans `contract/protocol.json` pour une `$defs`. */
function champsSchema(protocole, def) {
  const noeud = protocole.$defs?.[def];
  if (!noeud?.properties) return null;
  return Object.keys(noeud.properties);
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

  // **L'autre sens.** Étendu le 2026-10-15 : la boucle ne testait que
  // `TS ⊆ Rust`, donc un champ **supprimé ou renommé côté cœur** n'était jamais
  // signalé — le type TypeScript continuait de le nommer, il valait `undefined`
  // à l'écran, et c'est exactement le piège que la porte prétend fermer,
  // arrivé par l'autre côté. Un champ du cœur absent du type est donc signalé
  // aussi.
  const absentsDuType = rust.filter((champ) => !ts.includes(champ));
  if (absentsDuType.length > 0) {
    echecs.push(
      `  ${typeTs} ne nomme pas ${absentsDuType.length} champ(s) que ${structRust} ` +
        `serialise : ${absentsDuType.join(', ')} — ils vaudront undefined cote interface`,
    );
  }
}

for (const [typeTs, def] of TYPES_SCHEMA) {
  const ts = champsTypeTs(sourceTs, typeTs);
  if (!ts) {
    echecs.push(`  ${typeTs} : type introuvable dans app/page.tsx`);
    continue;
  }
  const schema = champsSchema(protocole, def);
  if (!schema) {
    echecs.push(`  ${typeTs} : $defs.${def} introuvable dans contract/protocol.json`);
    continue;
  }
  const ajoutes = CHAMPS_AJOUTES_PAR_LE_COEUR[typeTs] ?? {};
  for (const champ of ts) {
    if (schema.includes(champ)) continue;
    if (champ in ajoutes) continue;
    echecs.push(
      `  ${typeTs}.${champ} : le schéma $defs.${def} ne déclare pas « ${champ} »`,
    );
  }
  for (const champ of Object.keys(ajoutes)) {
    if (!ts.includes(champ)) {
      echecs.push(
        `  ${typeTs}.${champ} : le cœur l'ajoute (${ajoutes[champ]}) mais le type ne le nomme pas`,
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