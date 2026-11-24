export type SourceHealthEntry = { code: string; message: string; at: number };
export type SourceHealth = Record<string, SourceHealthEntry>;

/** Lit le journal local en écartant les entrées anciennes ou mal formées. */
export function lireSanteSources(raw: string | null): SourceHealth {
  if (!raw) return {};
  try {
    const parsed: unknown = JSON.parse(raw);
    if (!parsed || typeof parsed !== 'object' || Array.isArray(parsed)) return {};
    return Object.fromEntries(
      Object.entries(parsed).filter(([, value]) => {
        if (!value || typeof value !== 'object' || Array.isArray(value)) return false;
        const entree = value as Record<string, unknown>;
        return typeof entree.code === 'string' &&
          typeof entree.message === 'string' &&
          typeof entree.at === 'number' && Number.isFinite(entree.at);
      }),
    ) as SourceHealth;
  } catch {
    return {};
  }
}

/** Ajoute une panne au journal, avec l'instant du constat. */
export function noterEchecsSources(
  sante: SourceHealth,
  sources: string[],
  code: string,
  message: string,
  at: number,
): SourceHealth {
  const suivante = { ...sante };
  for (const source of sources) {
    if (source) suivante[source] = { code, message, at };
  }
  return suivante;
}

/** Retire seulement les sources dont une visite vient de réussir. */
export function effacerEchecsSources(sante: SourceHealth, sources: string[]): SourceHealth {
  const suivante = { ...sante };
  for (const source of sources) delete suivante[source];
  return suivante;
}
