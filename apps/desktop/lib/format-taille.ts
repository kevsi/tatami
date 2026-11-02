/**
 * Formatte une taille en octets pour l'affichage.
 *
 * Mesuré le 2026-11-02 : la vue Téléchargements affichait « 16334,1 Mo » — un
 * nombre à cinq chiffres avec une décimale française, illisible d'un coup d'œil.
 * Au-delà de 1000 Mo on passe en Go (une décimale) : « 16,0 Go ».
 *
 * Le séparateur décimal est la virgule (interface FR) ; l'anglais accepte aussi
 * la virgule dans ce contexte de mesure, et la clé n'est pas traduite pour
 * éviter deux unités divergentes dans la même session (leçon AGENTS.md §14 :
 * une valeur partagée n'a qu'une seule forme).
 */
export function formatTaille(bytes: number): string {
  if (!Number.isFinite(bytes) || bytes < 1024) return `${Math.round(bytes || 0)} o`;
  const ko = bytes / 1024;
  if (ko < 1024) return `${Math.round(ko)} Ko`;
  const mo = ko / 1024;
  if (mo < 1000) return `${mo.toFixed(1).replace('.', ',')} Mo`;
  const go = mo / 1024;
  return `${go.toFixed(1).replace('.', ',')} Go`;
}