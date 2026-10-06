// Tâche 3.4 — démarrage automatique : réglage d'interface.
//
// Même cadre que 3.3 : la case est **étatique** dans l'interface, elle ne
// demande que d'appeler le plugin. Un état local aurait deux vérités (ce que
// l'écran montre, ce que le système a réellement enregistré) et l'écran
// finirait par mentir — mesuré le 2026-10-05 sur les réglages du lecteur, où
// les préférences n'étaient relues qu'au montage.
import {
  disable as arreterDemarrage,
  enable as lancerDemarrage,
  isEnabled as demarrageActif,
} from '@tauri-apps/plugin-autostart';

export type EtatDemarrage = 'inconnu' | 'actif' | 'inactif' | 'indisponible';

/** État réel du démarrage automatique, tel que le système le rapporte.
 *
 *  `indisponible` est un état à part entière, pas une erreur : sous Linux sans
 *  gestionnaire de bureau, ou sur une machine où la politique de l'entreprise
 *  bloque l'écriture dans le dossier d'auto-démarrage, l'appel échoue. Dans ce
 *  cas l'interrupteur doit être **désactivé** et le dire, plutôt que rester
 *  coché alors que rien n'est enregistré.
 */
export async function lireDemarrage(): Promise<EtatDemarrage> {
  try {
    return (await demarrageActif()) ? 'actif' : 'inactif';
  } catch {
    return 'indisponible';
  }
}

/** Active ou désactive, et renvoie l'état **réellement** obtenu.
 *
 *  On relit après avoir écrit, au lieu de renvoyer ce qu'on a demandé : une
 *  demande refusée par le système donnerait un interrupteur coché pour rien.
 */
export async function reglerDemarrage(actif: boolean): Promise<EtatDemarrage> {
  try {
    if (actif) await lancerDemarrage();
    else await arreterDemarrage();
  } catch {
    return 'indisponible';
  }
  return lireDemarrage();
}
