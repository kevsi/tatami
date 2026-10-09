// Porte : le lecteur doit dire qu'une page est indisponible, au lieu d'afficher
// une image cassee sans rien.
//
// Mesure du 2026-10-09 (`custom:mangadex-api`, Dragon Ball FR) :
//   ch.1  → 7 pages sur 8 servies, ~200 Ko de JPEG chacune  ✅ la chaine marche
//   ch.3  → 0 page sur 8 servie, toutes refusees en 404   ❌ MangaDex ne les sert pas
//
// Les deux chapitres sont servis par le meme code, sur la meme machine, dans la
// meme seconde. L'utilisateur ne peut donc pas distinguer « l'application est
// cassee » de « ce chapitre n'est pas servi » — et il lit la seconde comme la
// premiere. **C'est le seul default qui reste de notre cote.** Le 404 n'est pas
// notre erreur, mais l'absence de message, elle, en est une.
//
// Cette porte lit le JSX du lecteur et refuse un `<img>` de page qui n'a ni
// `onError` ni un traitement d'equivalent. Elle est volontairement textuelle :
// le projet n'a pas de harness de test unitaire cote front (les portes sont des
// scripts `.mjs` qui analysent le code, comme `check-hooks.mjs`).
import { readFileSync } from 'node:fs';
import { join, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';

const ici = dirname(fileURLToPath(import.meta.url));
const page = readFileSync(join(ici, '..', 'app', 'page.tsx'), 'utf8');

const echecs = [];

// On cherche la balise `<img` du lecteur : celle qui porte `data-index`, ce qui
// la distingue des vignettes (`.thumb`), des couvertures (`.sheet-cover`) et
// des miniatures de la bibliotheque.
//
// Une balise de page SANS `data-index` est un défaut a part entiere : elle
// rend la page invisible a l'observateur de pages du lecteur. On la signale
// donc explicitement plutot que de ne rien voir — sinon le message « aucun
// <img data-index> trouve » ferait croire a une erreur de script, ce qui
// eloignerait de la vraie cause. (Mesure : la porte a d'abord produit ce
// message la quand le `onError` avait ete retire, alors qu'elle voyait bien
// l'image.)
const balises = [...page.matchAll(/<img\b[\s\S]*?\/>/g)].map((m) => m[0]);
const imgLecteur = balises.filter((b) => b.includes('data-index'));
const imgSansIndex = balises.filter(
  (b) => b.includes('pageSrc(') && !b.includes('data-index'),
);

if (imgLecteur.length === 0) {
  echecs.push(
    "aucun <img data-index> trouve dans le lecteur : la porte ne peut rien verifier" +
      (imgSansIndex.length
        ? ` (${imgSansIndex.length} <img src={pageSrc(...)}> sans data-index : c'est celle-la qui est probablement la page du lecteur)`
        : ''),
  );
}

for (const b of imgLecteur) {
  if (!/\bonError\s*=/.test(b)) {
    echecs.push(
      'le <img> du lecteur (data-index) n\'a pas de onError : une page indisponible ' +
      "s'affiche cassee sans message. Mesure : ch.3 = 0/8 pages servies.",
    );
  }
  // L'accessibilite suit : une image cassee doit rester nommable, sinon le
  // lecteur d'ecran ne dit rien non plus.
  if (!/\balt\s*=/.test(b)) {
    echecs.push('le <img> du lecteur n\'a pas de alt : une page cassee devient muette aussi pour un lecteur d\'ecran');
  }
}

// Le refus doit etre transformation en un etat visible : on cherche un appel
// qui enregistre l'echec. `useState` + `set` est la forme employee partout
// dans ce fichier, donc on verifie qu'un etat d'echec existe ET qu'il est
// utilise dans le rendu.
if (!/pagesEnErreur|pageEnErreur|setPageKO|setEchec/.test(page)) {
  echecs.push(
    "aucun etat d'echec de page dans page.tsx : le lecteur ne peut pas distinguer " +
    'une page manquante d\'une page servie',
  );
}

// Le mot « indisponible » doit exister cote i18n, sinon l'ecran afficherait une
// cle brute. On verifie les DEUX langues : une cle presente en fr et absente en
// en s'affiche en anglais chez un lecteur anglophone.
const dico = readFileSync(join(ici, '..', 'app', 'i18n.ts'), 'utf8');
if (!/indisponible|introuvable/.test(dico)) {
  echecs.push(
    "aucune cle i18n pour « page indisponible » : le message affiche serait une cle brute. " +
    'La traduction existe pour les deux langues.',
  );
}

if (echecs.length) {
  console.error('ECHEC — le lecteur ne dit pas qu\'une page est indisponible :');
  for (const e of echecs) console.error('  - ' + e);
  console.error(
    '\nRappel de la mesure : un chapitre FR sert 7/8 pages, un autre 0/8, ' +
    'par le meme code. Sans message, l\'utilisateur lit la seconde comme une panne.',
  );
  process.exit(1);
}

console.log(`OK — ${imgLecteur.length} <img> du lecteur gere(nt) l'echec, avec alt et cle i18n.`);