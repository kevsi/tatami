/**
 * Refus de script renvoyés par le moteur.
 *
 * Le moteur écrit ses refus sur **stderr**, jamais sur stdout (cf. contrat du
 * sidecar) : `list_sources` ne peut donc pas les porter. Rust les relaie par la
 * commande `scripts_refuses`, et cette fonction les rend affichables.
 *
 * Mesuré le 2026-10-10 : `mangadexFr.js` était refusé à **chaque** chargement
 * (`Missing close quote`), le journal Rust le voyait, et l'interface
 * n'affichait que le fichier comme s'il était chargé — trois jours après
 * l'import. Un fichier enregistré en base n'est pas un fichier chargé.
 */

/** Forme attendue, relevée dans `tatami.log` :
 *  `AVERTISSEMENT: Script ignoré (<fichier>) : <raison>` */
const MOTIF = /AVERTISSEMENT:\s*Script ignoré\s*\(([^)]+)\)\s*:?\s*(.*)$/s;

/**
 * Transforme les avertissements du moteur en couples `fichier -> raison`.
 *
 * Renvoie une `Map` ordonnée par première apparition. Si le même fichier est
 * refusé plusieurs fois — le moteur recharge ses scripts à chaque
 * `chargerTous()` — **le dernier raison l'emporte** : c'est l'état courant du
 * fichier, pas le premier souvenir qu'on en a.
 */
export function refusMoteur(avertissements: readonly string[]): Map<string, string> {
  const parFichier = new Map<string, string>();
  for (const ligne of avertissements) {
    // La ligne java.util peut être préfixée d'un horodatage et d'une pile
    // d'appels ; le motif cherche le marqueur, donc l'ancrage n'y est pas.
    const m = MOTIF.exec(ligne);
    if (!m) continue;
    const [, fichier, raison] = m;
    parFichier.set(fichier.trim(), raison.trim());
  }
  return parFichier;
}
