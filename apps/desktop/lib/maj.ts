// Fonctions pures de la mise à jour de l'application (ADR 0020).
//
// **Pourquoi un module à part.** Le panneau des réglages mélange trois
// décisions qui n'ont rien de visuel : « faut-il vérifier maintenant ? »,
// « que doit-on afficher ? », « quelle clé de traduction pour ce code
// d'erreur ? ». Les écrire dans du JSX les rendrait mesurables seulement par
// une capture ; ici elles se testent avec `node:test`, sans DOM ni Tauri.
//
// Aucune de ces fonctions ne lit `localStorage` : le stockage est une
// dépendance de l'appelant, donc les tests n'ont pas besoin de le simuler.

/** Délai entre deux vérifications automatiques. */
export const INTERVALLE_VERIF_MS = 24 * 60 * 60 * 1000;

/** Codes d'erreur émis par le cœur (`maj::CodeErreurMaj`). */
export type CodeErreurMaj = 'reseau' | 'signature' | 'espace' | 'droits' | 'inconnu';

const CODES: readonly string[] = ['reseau', 'signature', 'espace', 'droits', 'inconnu'];

/**
 * Code d'erreur d'un rejet de commande, sans jamais faire confiance à sa forme.
 *
 * Un `invoke` rejette avec la charge utile sérialisée par Rust
 * (`{ code: "reseau" }`), mais aussi avec une chaîne quand la commande n'existe
 * pas ou que l'IPC casse. Un code inconnu doit rester **affichable** : on
 * retombe sur `inconnu` plutôt que d'afficher `undefined`.
 */
export function codeErreurMaj(erreur: unknown): CodeErreurMaj {
  const brut =
    typeof erreur === 'object' && erreur !== null
      ? (erreur as { code?: unknown }).code
      : erreur;
  return typeof brut === 'string' && CODES.includes(brut)
    ? (brut as CodeErreurMaj)
    : 'inconnu';
}

/**
 * Faut-il lancer la vérification automatique ?
 *
 * - `actif` : la préférence de l'utilisateur (activée par défaut).
 * - `dernier` : instant du dernier contrôle, `null` si aucun.
 * - `maintenant` : instant courant, fourni par l'appelant (jamais `Date.now()`
 *   ici, sinon la fonction ne serait pas testable).
 *
 * **Une date dans le futur vaut « jamais vérifié ».** Une horloge qui recule
 * (changement d'heure mal appliqué, VM restaurée) ne doit pas bloquer les
 * vérifications jusqu'à ce que le temps rattrape la valeur stockée.
 */
export function doitVerifierAuto(entree: {
  actif: boolean;
  dernier: number | null;
  maintenant: number;
}): boolean {
  if (!entree.actif) return false;
  const { dernier, maintenant } = entree;
  if (dernier === null || !Number.isFinite(dernier) || dernier <= 0) return true;
  if (dernier > maintenant) return true;
  return maintenant - dernier >= INTERVALLE_VERIF_MS;
}

/** Horodatage stocké en `localStorage` ; toute valeur illisible vaut `null`. */
export function horodatage(valeur: string | null): number | null {
  if (valeur === null) return null;
  const n = Number(valeur);
  return Number.isFinite(n) && n > 0 ? n : null;
}

/** Phase affichée par le panneau. */
export type PhaseMaj =
  | 'inconnu'
  | 'verification'
  | 'disponible'
  | 'a_jour'
  | 'telechargement'
  | 'erreur';

/**
 * Phase à afficher, dérivée de l'état (aucun état intermédiaire à maintenir).
 *
 * L'ordre des priorités est une décision, pas un détail : une erreur survenue
 * pendant un téléchargement doit masquer la barre de progression, sinon
 * l'écran affiche une progression qui n'avance plus. Inversement, une
 * progression reçue pendant une vérification prouve qu'une installation a
 * commencé — c'est `telechargement` qui est vrai.
 */
export function phaseMaj(entree: {
  /** Un `verifier_maj` a déjà abouti : `disponible: null` veut alors dire « à jour ». */
  lue: boolean;
  /** Commande longue en cours. */
  occupe: 'verification' | 'installation' | null;
  disponible: boolean;
  /** Un événement de progression est arrivé pour l'installation en cours. */
  progression: boolean;
  erreur: boolean;
}): PhaseMaj {
  if (entree.erreur) return 'erreur';
  if (entree.progression || entree.occupe === 'installation') return 'telechargement';
  if (entree.occupe === 'verification') return 'verification';
  if (entree.disponible) return 'disponible';
  return entree.lue ? 'a_jour' : 'inconnu';
}

/**
 * Pourcentage reçu, borné à `[0, 100]`.
 *
 * `null` quand le serveur n'annonce pas de longueur : afficher 0 % dans ce cas
 * ferait croire à une progression bloquée au lieu d'une information absente.
 */
export function pourcentRecu(recu: number, total: number | null): number | null {
  if (total === null || !Number.isFinite(total) || total <= 0) return null;
  if (!Number.isFinite(recu) || recu < 0) return null;
  const p = Math.round((recu / total) * 100);
  return Math.max(0, Math.min(100, p));
}
