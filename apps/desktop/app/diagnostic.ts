// Tâche 5.5 — composition du rapport de diagnostic.
//
// Le texte est composé **ici** et écrit par Rust, qui applique la rédaction en
// dernier. Le partage est net : elle est la seule à connaître l'état de santé
// des sources — il vit dans son `localStorage` (tâche 5.4) et nulle part
// ailleurs, donc aucun cœur n'a pu le voir.
//
// Ce qui n'est **pas** ici, et pourquoi :
// - les versions du moteur : elles sont déjà dans le journal, relayées du stderr
//   par le superviseur. Les réinventer créerait une seconde source de vérité ;
// - les URL de manga : cet écran ne les manipule pas, et le cœur les a déjà
//   réduites à une empreinte avant d'écrire.

import { invoke } from '@tauri-apps/api/core';

/** L'état de santé d'une source, tel que 5.4 le persiste. */
export type SanteSource = { code: string; message: string; at: number };

const CLE_SANTE = 'tatami.sourceHealth';

/** Relit l'état de santé persisté par l'écran d'état des sources. */
export function lireSante(): Record<string, SanteSource> {
  try {
    const brut = window.localStorage.getItem(CLE_SANTE);
    return brut ? (JSON.parse(brut) as Record<string, SanteSource>) : {};
  } catch {
    // Un `localStorage` corrompu ne doit pas empêcher l'export : on part d'un
    // état vide, et le reste du rapport est encore utile.
    return {};
  }
}

/** Une synthèse par **code d'erreur**, et non par source.
 *
 * Raison : dix sources en `CHALLENGE_REQUIRED` sont **un** problème (Cloudflare),
 * pas dix pannes distinctes. Compter les occurrences dit ce qu'il faut réparer.
 */
function resumerSante(sante: Record<string, SanteSource>): string {
  const entrees = Object.entries(sante);
  if (!entrees.length) return '  (aucune source signalee)';

  const parCode = new Map<string, { sources: string[]; dernier: number }>();
  for (const [source, s] of entrees) {
    const code = s.code || 'INCONNU';
    const group = parCode.get(code) ?? { sources: [], dernier: 0 };
    group.sources.push(source);
    group.dernier = Math.max(group.dernier, s.at || 0);
    parCode.set(code, group);
  }

  const lignes: string[] = [];
  for (const [code, group] of [...parCode.entries()].sort((a, b) =>
    b[1].sources.length - a[1].sources.length,
  )) {
    const quand = group.dernier ? new Date(group.dernier).toISOString() : 'date inconnue';
    // Les sources sont triées : deux rapports du meme incident doivent se lire
    // pareil, donc l'ordre d'un `Object.entries` ne peut pas decider.
    const liste = group.sources.sort().join(', ');
    lignes.push(`  ${code} x${group.sources.length} (dernier : ${quand})`);
    lignes.push(`    ${liste}`);
  }
  return lignes.join('\n');
}

/** Compose le résumé que le cœur va redacter puis ecrire sur disque. */
export function composerResume(lang: string): string {
  const sante = lireSante();
  const nombre = Object.keys(sante).length;
  const genere = new Date().toISOString();

  return [
    '=== ETAT DES SOURCES ===',
    `  genere le      : ${genere}`,
    `  langue de l UI : ${lang}`,
    `  sources signalees : ${nombre}`,
    resumerSante(sante),
    '',
  ].join('\n');
}

/** Demande l'ecriture du rapport et renvoie le chemin ecrit.
 *
 * Le chemin est **relis** depuis ce que le cœur a réellement produit : une
 * commande qui renvoie `Ok` ne prouve que la réception.
 */
export async function exporterRapport(lang: string): Promise<string> {
  return invoke<string>('exporter_rapport', { resume: composerResume(lang) });
}

/** Supprime les journaux. Renvoie **combien** de fichiers ont ete retires.
 *
 * Compter est la seule preuve de l'etat propre : une suppression qui renvoie
 * toujours `Ok` n'affirme rien.
 */
export async function purgerJournal(): Promise<number> {
  return invoke<number>('purger_journal');
}
