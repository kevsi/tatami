// Tests de `texteNotification` : la rédaction d'une notification.
//
// ## Pourquoi un `t` faux, et pas le vrai dictionnaire
//
// La traduction réelle vit dans `useI18n` (un hook), donc inutilisable hors
// React. Le `t` de ce test renvoie un **marqueur qui nomme la clé ET ses
// variables** : `[notif.cn n=12]`. C'est plus strict qu'un dictionnaire recopié
// — il prouve *quelle* clé a été demandée et *avec quelles valeurs*, donc une
// faute de clé ou un oubli de paramètre fait échouer le test au lieu de
// s'afficher en `{n}` à l'écran (défaut déjà mesuré une fois, cf. AGENTS.md §14
// « une correction peut être plus fausse que le bug »).
//
// La existence des clés, elle, est déjà garantie par `pnpm i18n:check`, qui lit
// les clés littérales du code.
import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import type { MessageKey } from './i18n.ts';
import { texteNotification } from './notification.ts';

/** `t` de test : nomme la clé et ses variables. */
function tMarqueur(cle: MessageKey, vars?: Record<string, string | number>): string {
  const params = vars
    ? ' ' +
      Object.entries(vars)
        .map(([k, v]) => `${k}=${v}`)
        .join(' ')
    : '';
  return `[${cle}${params}]`;
}

const t = tMarqueur;

/** Une entrée telle que le planificateur l'envoie. */
function entree(
  source: string,
  manga_url: string,
  total: number,
  extra: { titre?: string; tronque?: boolean } = {},
) {
  return { source, manga_url, total, ...extra };
}

describe('texteNotification', () => {
  it('ne dit rien quand la passe est vide', () => {
    assert.equal(texteNotification(t, []), null);
  });

  it('ne dit rien quand tout est a zero', () => {
    // Le planificateur envoie une entree par favori verifie, la plupart a 0.
    // Une notification « 0 nouveau chapitre » serait un bruit.
    assert.equal(texteNotification(t, [entree('SRC', '/m/1', 0), entree('SRC', '/m/2', 0)]), null);
  });

  it('ignore une entree absente ou sans total', () => {
    // Garde defensive : une serie supprimee pendant la passe ne doit pas
    // Produire `NaN` dans le corps.
    const sale = [
      entree('SRC', '/m/1', 2),
      null as unknown as { source: string; manga_url: string; total: number },
      undefined as unknown as { source: string; manga_url: string; total: number },
      { source: 'SRC', manga_url: '/m/3' } as unknown as { source: string; manga_url: string; total: number },
    ];
    const r = texteNotification(t, sale);
    assert.ok(r, 'une entree a total > 0 doit suffire');
    assert.equal(r.body, `[notif.cn n=2] · SRC`);
  });

  it('un seul chapitre dans une seule serie : le singulier et le nom', () => {
    // Le singulier se teste avec UNE unite, jamais avec le nombre qu'on a sous
    // la main (lecon §14 : « 1 source(s) » et « Voir les 1 chapitres »).
    const r = texteNotification(t, [entree('MANGADEX', '/m/9', 1, { titre: 'Solo Leveling' })]);
    assert.deepEqual(r, {
      title: '[notif.titre]',
      body: '[notif.c1] · Solo Leveling',
    });
  });

  it('plusieurs chapitres : le pluriel et le compte exact', () => {
    const r = texteNotification(t, [
      entree('MANGADEX', '/m/9', 7, { titre: 'A' }),
      entree('MANGADEX', '/m/10', 5, { titre: 'B' }),
    ]);
    assert.equal(r?.body, '[notif.cn n=12] · [notif.sn s=2]');
  });

  it('un total tronque est un PLANCHER, pas un compte', () => {
    // Le moteur plafonne ce qu'il renvoie : annoncer « 12 » quand il peut y en
    // avoir 40 ment.
    const un = texteNotification(t, [entree('SRC', '/m/1', 1, { tronque: true, titre: 'X' })]);
    assert.equal(un?.body, '[notif.c1p] · X');
    const plusieurs = texteNotification(t, [
      entree('SRC', '/m/1', 12, { tronque: true, titre: 'X' }),
    ]);
    assert.equal(plusieurs?.body, '[notif.cnp n=12] · X');
  });

  it('le marqueur de troncature est DANS le fragment, pas accole', () => {
    // Premiere version : « 12 nouveaux chapitres » + « ou plus » donniaient
    // « 12 nouveaux chapitres ou plus » — lisible en anglais, agrammatical en
    // francais. Le marqueur doit donc venir de la cle, pas d'une concatenation.
    const r = texteNotification(t, [entree('SRC', '/m/1', 3, { tronque: true, titre: 'X' })]);
    assert.ok(r?.body.includes('[notif.cnp n=3]'), `corps = ${r?.body}`);
    assert.ok(!r?.body.includes('·') || r.body.split('·').length === 2, 'deux fragments maximum');
  });

  it('une serie vue deux fois dans la meme passe est comptee UNE fois', () => {
    // Le moteur peut renvoyer le meme favori deux fois (deux sources du meme
    // site, ou une relance). « Dans 2 series » serait faux.
    const deux = texteNotification(t, [
      entree('MANGADEX', '/m/9', 4, { titre: 'A' }),
      entree('MANGADEX', '/m/9', 8, { titre: 'A' }),
    ]);
    assert.equal(deux?.body, '[notif.cn n=12] · A');
  });

  it('contre-epreuve : le dedoublement par (source, url) est bien ce qui evite le doublon', () => {
    // Sans la cle `(source, url)`, deux entrees de la meme serie compteraient
    // 2 series. La preuve : deux series DIFFERENTES donnent bien `s=2`, et la
    // meme serie deux fois donne le nom sans compte de series.
    const differentes = texteNotification(t, [
      entree('MANGADEX', '/m/9', 4, { titre: 'A' }),
      entree('SCANFR', '/m/9', 4, { titre: 'A' }),
    ]);
    assert.ok(
      differentes?.body.includes('[notif.sn s=2]'),
      `deux sources distinctes doivent donner s=2, corps = ${differentes?.body}`,
    );
  });

  it('une URL identique sur deux sources differentes reste deux series', () => {
    // Meme URL relative, deux sources : ce ne sont pas les memes series.
    const r = texteNotification(t, [
      entree('SRC_A', '/chapter/1', 1),
      entree('SRC_B', '/chapter/1', 1),
    ]);
    assert.ok(r?.body.includes('[notif.sn s=2]'), `corps = ${r?.body}`);
  });

  it('sans titre, le nom de la source sert de repli', () => {
    const vide = texteNotification(t, [entree('MANGADEX', '/m/9', 1, { titre: '   ' })]);
    assert.equal(vide?.body, '[notif.c1] · MANGADEX');
    const absent = texteNotification(t, [entree('MANGADEX', '/m/9', 1)]);
    assert.equal(absent?.body, '[notif.c1] · MANGADEX');
  });

  it('le titre est toujours la cle du titre', () => {
    for (const r of [
      texteNotification(t, [entree('SRC', '/m/1', 1, { titre: 'X' })]),
      texteNotification(t, [entree('SRC', '/m/1', 9, { tronque: true, titre: 'X' })]),
      texteNotification(t, [entree('SRC', '/m/1', 9), entree('SRC', '/m/2', 9)]),
    ]) {
      assert.equal(r?.title, '[notif.titre]');
    }
  });
});
