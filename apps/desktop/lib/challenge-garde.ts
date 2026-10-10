/**
 * Garde-fou anti-boucle du flux « vérification anti-robot ».
 *
 * ## Le défaut, mesuré le 2026-10-10 (PLAN.md, source TOONFR)
 *
 * Rien ne bornait les tentatives : « la fenêtre de résolution se rouvre à
 * chaque essai, et le cycle ne se termine jamais » (PLAN.md:1331-1332).
 * Chaque échec de relecture remettait le bandeau, chaque clic rouvrait la
 * fenêtre, et aucun compteur n'existait — ni côté front, ni côté Rust
 * (`open_challenge` n'avait aucune limite d'appels).
 *
 * ## Ce que décide ce module
 *
 * Uniquement le **comptage** : combien d'échecs `CHALLENGE_REQUIRED`
 * consécutifs un couple (source, action) a déjà subis. Au-delà de
 * `MAX_CHALLENGES`, l'appelant cesse de proposer la vérification — plus de
 * bandeau, plus de fenêtre — et affiche un message traduit. La remise à zéro
 * appartient à l'appelant : dès que l'action réussit, le compteur retombe.
 *
 * Le compteur est indexé par (source, action) : épuiser TOONFR ne doit pas
 * couper les autres sources, ni une autre action sur la même source (la
 * lecture du catalogue et la reprise d'un chapitre sont deux actions).
 *
 * C'est une fonction pure, sans React ni Tauri, pour être testable seule.
 */

/** Échecs consécutifs tolérés avant de rendre la main à l'utilisateur. */
export const MAX_CHALLENGES = 2;

/** Compteurs en cours : clé → nombre d'échecs consécutifs. */
export type TentativesChallenge = ReadonlyMap<string, number>;

/**
 * Clé d'un couple (source, action). Le séparateur est le unit separator,
 * comme la fusion des passes du planificateur (page.tsx, `maj://…`) : un
 * caractère qu'aucun nom de source ni d'action ne peut contenir.
 */
export function cleChallenge(source: string, action: string): string {
  return `${source}\u001f${action}`;
}

/** Remet le compteur d'une action à zéro — son action vient de réussir. */
export function reinitialiserChallenge(
  tentatives: TentativesChallenge,
  source: string,
  action: string,
): TentativesChallenge {
  const cle = cleChallenge(source, action);
  if (!tentatives.has(cle)) return tentatives; // déjà propre : même instance
  const suivant = new Map(tentatives);
  suivant.delete(cle);
  return suivant;
}

export type ResultatTentative = {
  /** Nouveaux compteurs, à remettre dans l'état. */
  tentatives: TentativesChallenge;
  /** Vrai quand `MAX_CHALLENGES` est dépassé : arrêter le cycle. */
  epuise: boolean;
};

/**
 * Enregistre un échec `CHALLENGE_REQUIRED` sur une action.
 *
 * `epuise` devient vrai **au-delà** de `MAX_CHALLENGES` : les deux premiers
 * échecs laissent le cycle tourner (l'utilisateur a le droit de résoudre
 * deux fois), le troisième lui rend la main.
 */
export function enregistrerEchecChallenge(
  tentatives: TentativesChallenge,
  source: string,
  action: string,
): ResultatTentative {
  const cle = cleChallenge(source, action);
  const echecs = (tentatives.get(cle) ?? 0) + 1;
  const suivantes = new Map(tentatives);
  suivantes.set(cle, echecs);
  return { tentatives: suivantes, epuise: echecs > MAX_CHALLENGES };
}
