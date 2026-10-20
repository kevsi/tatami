// Persiste la liste des sources épinglées par l'utilisateur.
//
// Un épinglage n'est pas un réglage que Rust applique : c'est un ordre
// d'affichage. Il vit donc dans `localStorage` (comme les préférences du
// lecteur), jamais en base — AGENTS.md §6. La liste est stockée sous une
// clé unique, sous forme de tableau JSON de noms de sources.
//
// Le **nom** (et non l'index) est la clé : une source peut changer de place
// dans le catalogue d'une version à l'autre, mais son identifiant reste.

export const EPINGLES_KEY = 'tatami.pinnedSources';

/** Lit la liste épinglée depuis une chaîne brute (valeur de `localStorage`). */
export function lireEpingles(raw: string | null | undefined): string[] {
  if (!raw) return [];
  try {
    const parsed: unknown = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    // Déduplication et filtrage des valeurs non textuelles ou vides : un
    // `localStorage` corrompu ne doit pas casser l'affichage.
    const vus = new Set<string>();
    const resultat: string[] = [];
    for (const v of parsed) {
      if (typeof v !== 'string') continue;
      const nom = v.trim();
      if (!nom || vus.has(nom)) continue;
      vus.add(nom);
      resultat.push(nom);
    }
    return resultat;
  } catch {
    return [];
  }
}

/** Bascule l'épinglage d'une source et rend la nouvelle liste. */
export function basculerEpingle(epingles: readonly string[], nom: string): string[] {
  const propres = nom.trim();
  if (!propres) return [...epingles];
  if (epingles.includes(propres)) {
    return epingles.filter((n) => n !== propres);
  }
  return [...epingles, propres];
}

/** Vrai si la source est épinglée. */
export function estEpinglee(epingles: readonly string[], nom: string): boolean {
  return epingles.includes(nom);
}

/** Sépare une liste de sources en épinglées et non épinglées, en conservant
 *  l'ordre d'origine dans chaque groupe. */
export function separerEpinglees<T extends { name: string }>(
  sources: readonly T[],
  epingles: readonly string[],
): { epinglees: T[]; autres: T[] } {
  const epinglees: T[] = [];
  const autres: T[] = [];
  for (const s of sources) {
    if (epingles.includes(s.name)) epinglees.push(s);
    else autres.push(s);
  }
  return { epinglees, autres };
}
