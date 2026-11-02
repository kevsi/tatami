/**
 * Lanceur des E2E de Tatami.
 *
 * Usage : `pnpm e2e [filtre]` — `filtre` est une sous-chaîne du nom de fichier
 * (`chapitre`, `reprendre`…). Sans filtre, les scénarios tournent dans l'ordre,
 * sur **une seule** session CDP (les plus légers et hors ligne d'abord, le
 * téléchargement en dernier et seul — un téléchargement complet fatigue une
 * source, et un scénario en ligne qui suit le paie).
 *
 * Prérequis : `pnpm tauri dev` lancé avec le port de débogage (cf.
 * `e2e/client.mjs`). Sans lui, le lanceur sort en code **2** — distinct de 1
 * (un scénario a échoué) — pour qu'on ne confonde pas « l'app n'est pas là »
 * avec « le code est cassé ».
 */
import { connecter, pause } from './client.mjs';

const SCENARIOS = [
  './01-ouvrir-chapitre.mjs',
  './04-rail-et-bande.mjs',
  './03-reprendre-lecture.mjs',
  './02-telecharger-chapitre.mjs',
];

const filtre = process.argv[2] ?? '';
const liste = SCENARIOS.filter((f) => f.includes(filtre));
if (liste.length === 0) {
  console.error(`Aucun scénario ne correspond à « ${filtre} ». Disponibles :\n  - ${SCENARIOS.join('\n  - ')}`);
  process.exit(2);
}

let session;
try {
  session = await connecter();
} catch (e) {
  console.error(`\n[E2E] ${String(e.message)}`);
  process.exit(2);
}

const resultats = [];
for (const chemin of liste) {
  const nom = chemin.replace('./', '').replace('.mjs', '');
  const debut = Date.now();
  process.stdout.write(`\n▶ ${nom} …`);
  try {
    const { default: scenario } = await import(chemin);
    const detail = await scenario(session);
    await pause(500);
    const ms = Date.now() - debut;
    if (detail?.ignore) {
      resultats.push({ nom, statut: 'IGNORÉ', detail: detail.ignore, ms });
      process.stdout.write(` ignoré (${ms} ms)\n`);
    } else {
      resultats.push({ nom, statut: 'OK', detail, ms });
      process.stdout.write(` OK (${ms} ms)\n`);
    }
  } catch (e) {
    const ms = Date.now() - debut;
    // On garde le message **entier** : le tronquer à la première ligne a déjà
    // rendu un « Uncaught (in promise) » indiagnosticable.
    resultats.push({ nom, statut: 'ÉCHEC', detail: String(e?.stack ?? e), ms });
    process.stdout.write(` ÉCHEC (${ms} ms)\n`);
  }
}

session.fermer();

console.log('\n────────── Bilan E2E ──────────');
for (const r of resultats) {
  const marque = r.statut === 'OK' ? '✅' : r.statut === 'IGNORÉ' ? '⏭️ ' : '❌';
  console.log(`${marque} ${r.nom} — ${r.statut} (${r.ms} ms)`);
  if (r.statut !== 'OK') console.log(`     ↳ ${r.detail.split('\n').slice(0, 6).join('\n       ')}`);
  else if (r.detail) console.log(`     ↳ ${JSON.stringify(r.detail)}`);
}

const echecs = resultats.filter((r) => r.statut === 'ÉCHEC').length;
console.log(`\n${resultats.length - echecs}/${resultats.length} scénario(s) vert(s).`);
process.exit(echecs > 0 ? 1 : 0);
