/**
 * Tâche 3.3 — notifications système.
 *
 * ## Pourquoi ce module existe, et pourquoi il est **hors** de Rust
 *
 * Le planificateur (Rust) décide **quand** il y a du neuf ; ce module décide
 * **quoi dire**. La raison est la même que pour les motifs d'import de 2.4 : un
 * texte dans Rust s'affiche tel quel dans les deux langues, donc un écran
 * anglais annonce « 3 nouveaux chapitres » pendant qu'il affiche « Downloads »
 * juste à côté. La couche à connaître les langues est l'interface, donc c'est
 * elle qui écrit le texte.
 *
 * ## Le réglage vit dans `localStorage`, et c'est voulu
 *
 * Règle du dépôt : `localStorage` pour ce que l'interface lit (l'affichage), la
 * base pour ce que le cœur applique. Ici le cœur n'applique rien — c'est
 * l'interface qui décide d'envoyer ou non — donc un réglage en base serait une
 * donnée que Rust ne lirait jamais. Et surtout : couper les notifications ne doit
 * **pas** arrêter le flux « mises à jour » de 3.2, qui est une autre chose.
 *
 * ## Sur Windows, l'identité affichée n'est pas la nôtre en développement
 *
 * Mesuré dans la doc du plugin : « Only works for installed apps. Shows
 * powershell name & icon in development. » La notification **part**, mais on la
 * voit au nom de PowerShell tant que l'application n'est pas installée. Ce
 * n'est pas un défaut du code, et il ne doit pas servir de excuse pour ne pas
 * vérifier.
 */
import {
  isPermissionGranted,
  requestPermission,
  sendNotification,
} from '@tauri-apps/plugin-notification';

import type { MessageKey } from './i18n';

const CLE = 'tatami.notif';

/** Réglage par défaut : **actif**. Kotatsu-Redo notifie, et l'utilisateur peut
 *  éteindre. Coupler le silence à un réglage qu'il n'a pas demandé serait une
 *  décision prise à sa place. */
const ACTIF_PAR_DEFAUT = true;

export function notifActive(): boolean {
  if (typeof window === 'undefined') return false;
  const v = window.localStorage.getItem(CLE);
  // Une chaîne vide ne doit **jamais** être un sentinelle : elle est indistinguable
  // d'une absence de clé. C'est exactement le piège mesuré le 2026-10-05 sur le
  // filtre de classification, où l'option « Toutes » vidait les sources adultes.
  return v === null ? ACTIF_PAR_DEFAUT : v === '1';
}

export function setNotifActive(actif: boolean): void {
  window.localStorage.setItem(CLE, actif ? '1' : '0');
}

/** Une notification en attente, déjà rédigée. */
export type NotifTexte = { title: string; body: string };

/** Traduit une passe en notification, **sans** l'envoyer.
 *
 * Séparé de l'envoi pour deux raisons : c'est la seule partie testable sans
 * fenêtre, et le décompte est fait une fois ici plutôt que deux fois dans le
 * composant.
 *
 * Deux fautes classics que ce calcul évite :
 *  - `total` peut être `tronque` : c'est alors un **plancher**, pas un compte.
 *    On l'annonce donc comme tel plutôt que comme un nombre faux.
 *  - une meme serie peut apparaitre deux fois dans la passe ; on la **compte
 *    une fois** par `(source, url)` pour que « dans 3 séries » soit vrai.
 */
export function texteNotification(
  t: (cle: MessageKey, vars?: Record<string, string | number>) => string,
  entrees: { source: string; manga_url: string; titre?: string; total: number; tronque?: boolean }[],
): NotifTexte | null {
  const reels = entrees.filter((e) => e && (e.total ?? 0) > 0);
  if (reels.length === 0) return null;

  const serie = reels.reduce((n, e) => n + (e.total ?? 0), 0);
  const tronque = reels.some((e) => e.tronque === true);
  // Un plancher ne doit pas s'annoncer comme un compte exact : « 20 » quand il
  // y en a peut-être 40 ment.
  //
  // Le marqueur de troncature est **dans** le fragment, et non posé à côté :
  // la première version composait « 12 nouveaux chapitres » + « ou plus », ce
  // qui donnait « 12 nouveaux chapitres · ou plus » — lisible en anglais, agrammatical
  // en français. Un fragment qui se tient seul est la seule façon de composer
  // deux langues sans avoir à connaître la grammaire de l'autre.
  const fragments: string[] = [
    tronque
      ? serie === 1
        ? t('notif.c1p')
        : t('notif.cnp', { n: serie })
      : serie === 1
        ? t('notif.c1')
        : t('notif.cn', { n: serie }),
  ];

  // Clés distinctes : deux séries peuvent afficher le même compte.
  const distinctes = new Set(reels.map((e) => e.source + '' + e.manga_url));
  if (distinctes.size === 1) {
    // Une seule série : la nommer vaut mieux qu'un compte.
    const nom = reels[0].titre?.trim() || reels[0].source;
    fragments.push(nom);
  } else {
    fragments.push(distinctes.size === 1 ? t('notif.s1') : t('notif.sn', { s: distinctes.size }));
  }

  return { title: t('notif.titre'), body: fragments.join(' · ') };
}

/** Envoie la notification si l'utilisateur l'a demandé **et** que la fenêtre
 *  n'a pas le focus.
 *
 *  Le silence quand la fenêtre est active est la politesse de base : un
 *  utilisateur en plein chapitre ne veut pas être interrompu par une alerte
 *  qu'il peut déjà voir dans le flux « mises à jour ».
 *
 *  `document.hasFocus()` plutôt que `getCurrentWindow().isFocused()` : c'est un
 *  attribut DOM standard, donc **aucune permission** Tauri à ajouter. Limite
 *  assumée : il reflète le focus du document, pas celui de la fenêtre système ;
 *  sur Windows les deux coïncident en pratique.
 *
 *  Renvoie `false` quand rien n'a été envoyé, pour que l'appelant puisse le dire
 *  plutôt que croire que c'est parti.
 */
export async function notifie(texte: NotifTexte | null): Promise<boolean> {
  if (!texte || !notifActive()) return false;
  if (typeof document !== 'undefined' && document.hasFocus()) return false;

  try {
    let granted = await isPermissionGranted();
    if (!granted) {
      // Sur desktop la doc dit que ça vaut toujours `Granted` sans invite ; la
      // demande reste là pour les plateformes qui en ont besoin, et elle n'est
      // faite **qu'une fois**, au premier envoi.
      granted = (await requestPermission()) === 'granted';
    }
    if (!granted) return false;
    sendNotification({ title: texte.title, body: texte.body });
    return true;
  } catch (e) {
    // Un `show()` qui échoue en silence est une notification qui n'arrive
    // jamais : on le signale dans la console, c'est la seule trace possible
    // depuis l'interface.
    console.warn('notification système impossible', e);
    return false;
  }
}
