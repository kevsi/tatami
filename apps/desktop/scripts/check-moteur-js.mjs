// Porte : un script de source personnalisee doit etre evaluable par le moteur
// de l'application — c'est Nashorn 15.6 (`engine/build.gradle.kts`, ligne 24),
// pas Node.
//
// Mesure du 2026-10-14 (`%APPDATA%\Tatami\logs\tatami.log`, 15:53 locales) :
//   AVERTISSEMENT: Rechargement AcchouAc (source-bananascans.js) :
//     Erreur d'Acvaluation du script : <eval>:115:8 Expected an operand but found )
//   AVERTISSEMENT: Rechargement AcchouAc (source-lire-manga.js) : <eval>:113:8 ...
//
// Les trois sources ecrites ce jour-la ont passe `scripts/test-source.mjs`, qui
// les evalue avec **Node**. Nashorn n'accepte pas la virgule finale dans la liste
// d'arguments d'un appel (ES2017, lui s'arrete a l'ES6). Le test hors ligne
// prouvait donc le code avec un **autre moteur** : il ne pouvait pas voir ce
// defaut. La source etait correcte pour le test, invalide pour l'application.
//
// Cette porte refuse donc la virgule finale avant une parenthese fermante.
// Elle est volontairement etroite : elle ne pretend pas valider le niveau ECMA
// de Nashorn (que nous n'avons pas mesure ligne a ligne), elle refuse la forme
// reellement mesuree. Ne pas elargir sans avoir mesure un deuxieme cas reel.
import { readdirSync, readFileSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const ici = dirname(fileURLToPath(import.meta.url));
const dossier = join(ici, '..', '..', '..', 'docs');

// Une virgule, puis uniquement des espaces (une ligne peut etre un commentaire,
// meme si les sources du depot n'en contiennent pas dans cette position), puis
// une parenthese fermante. `[1, 2,]` et `{ a: 1, }` ne sont PAS vises : ils sont
// legaux en ES5, donc acceptes par Nashorn.
const VIRGULE_FINALE_AVANT_PARENTHESE = /,[\s]*\)/g;

// Tous les `.js` du dossier, et pas seulement `source-*.js` : `mangadex-api.js`,
// `scan-manga.js` et `aniverse-source.js` sont charges eux aussi (le journal le
// prouve : « Source personnalisee chargee : mangadex-api »). Un filtre sur un
// nom de fichier laisserait passer une source cassee sans jamais la voir.
const fichiers = readdirSync(dossier)
  .filter((n) => n.endsWith('.js'))
  .sort();

if (fichiers.length === 0) {
  console.error('ECHEC — aucun script `.js` dans `docs/` : la porte ne peut rien verifier.');
  process.exit(1);
}

const echecs = [];
let total = 0;

for (const nom of fichiers) {
  const texte = readFileSync(join(dossier, nom), 'utf8');
  // Le numero de ligne se recompte depuis le debut a chaque occurrence, au lieu
  // d'avancer d'un match a l'autre. La version incrementale etait fausse : le
  // match consomme le saut de ligne qui termine la ligne de la virgule, donc
  // chaque occurrence apres la premiere etait accusee une ligne trop haut.
  // Mesure : `,[\s]*\)` en 108/114/123/136/173, la porte annoncait
  // 107/112/120/132/168. Une porte qui nomme la mauvaise ligne est une porte
  // qu'on finit par ignorer.
  for (const m of texte.matchAll(VIRGULE_FINALE_AVANT_PARENTHESE)) {
    const numero = (texte.slice(0, m.index).match(/\n/g) || []).length + 1;
    total++;
    echecs.push(
      `docs/${nom}:${numero} — virgule finale avant « ) » : ` +
        'Nashorn (le moteur de l\'application) refuse cette forme, ' +
        'Node (le test hors ligne) l\'accepte. ' +
        "Le source est donc charge a l'ecran et refuse a chaud, sans erreur cote interface.",
    );
  }
}

if (echecs.length) {
  console.error(
    `ECHEC — ${echecs.length} construction(s) que Nashorn refuse, sur ${total} trouvee(s) ` +
      `dans ${fichiers.length} source(s) personnalisee(s) :`,
  );
  for (const e of echecs) console.error('  - ' + e);
  console.error(
    "\nRappel : une source rejetee ne charge pas. Le journal le dit " +
      '(`Rechargement <nom> : Erreur d\'evaluation du script`), et l\'application ' +
      'ignore silencieusement la tuile absente de la liste des sources.',
  );
  process.exit(1);
}

console.log(
  `OK — ${fichiers.length} source(s) personnalisee(s) sans virgule finale d'appel, ` +
    'donc evaluables par Nashorn.',
);
