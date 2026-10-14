// Eprouver les regex de la source sur du HTML, SANS reseau.
// Une regex qui ne mord pas ne leve aucune erreur : elle rend juste 0 resultat.
// C'est deja arrive ("0 serie sur 238, silencieusement").

import { readFileSync } from "node:fs";

const html = readFileSync(process.argv[2], "utf8");

// On charge la source dans un contexte où `http` renvoie la fixture : on peut
// ainsi éprouver les regex sans réseau. `vm` plutôt qu'un `eval` bricolé, pour
// que le test lise exactement ce que le moteur lira.
import vm from "node:vm";
const code = readFileSync(process.argv[3], "utf8");
const contexte = vm.createContext({ http: { get: () => html }, encodeURIComponent, console });
vm.runInContext(code, contexte);
const SOURCE = contexte.SOURCE;

const liste = JSON.parse(SOURCE.getList(1, ""));
console.log("getList  ->", liste.length, "series");
for (const s of liste) console.log("   ", s.title, "|", s.url);

const fiche = JSON.parse(SOURCE.getDetails(liste[0]?.url ?? "/manga/milf-exchange-plan/"));
console.log("getDetails ->", fiche.title, "|", fiche.chapters.length, "chapitres");
for (const c of fiche.chapters.slice(0, 3)) console.log("   ", c.number, c.title);

const pages = JSON.parse(SOURCE.getPages(fiche.chapters[0]?.url ?? "/manga/milf-exchange-plan/chapitre-87/"));
console.log("getPages ->", pages.length, "pages");
for (const p of pages.slice(0, 3)) console.log("   ", p.url);
