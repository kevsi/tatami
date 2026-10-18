import { test } from 'node:test';
import assert from 'node:assert/strict';
import { admettreLaCible, admettreUne, admettreCibleEtVoisins, pagesAffichees } from './quota-pages.ts';

// Le défaut mesuré le 2026-10-10 : `admis` — l'ensemble des pages qui ont le
// droit de recevoir un `src` — ne grandit QUE sur un `onLoad`. À l'ouverture,
// il contient les 6 premières pages ; puis chaque décodage en admet une de plus.
//
// Conséquence : un SAUT vers une page lointaine atterrit dans une fenêtre dont
// aucune page n'est admise. Pas de `src`, donc pas de chargement, donc pas de
// `onLoad`, donc plus rien n'est admis — la bande reste vierge. Signalé par
// l'utilisateur comme « je passe de la page 3 à la page 48, la page ne
// s'affiche pas, je dois revenir à la page 3 et continuer la lecture ».

test('un saut admet la page demandée', () => {
  const apres = admettreLaCible([0, 1, 2, 3, 4, 5], 48);
  assert.deepEqual(apres, [0, 1, 2, 3, 4, 5, 48]);
});

test('un saut vers une page déjà admise ne change rien', () => {
  const prec = [0, 1, 2, 3, 4, 5];
  assert.deepEqual(admettreLaCible(prec, 3), prec);
});

test('une cible absurde n’entre pas dans la liste', () => {
  const prec = [0, 1, 2, 3, 4, 5];
  assert.deepEqual(admettreLaCible(prec, -1), prec);
  assert.deepEqual(admettreLaCible(prec, Number.NaN), prec);
  assert.deepEqual(admettreLaCible(prec, 2.5), prec);
});

test('un saut puis la lecture normale continuent d’admettre', () => {
  // Le quota doit rester une FILE : admettre la cible ne doit pas vider le
  // mécanisme, sinon 50 requêtes repartiraient d'un coup (défaut du 2026-10-09).
  let admis = admettreLaCible([0, 1, 2, 3, 4, 5], 48);
  // La cible chargée libère un crédit, la page suivante de la fenêtre entre.
  admis = admettreUne(admis, [45, 46, 47, 48, 49, 50]);
  assert.ok(admis.length > 7, 'la file continue de se remplir');
  assert.ok(admis.includes(48));
});

test('admettreUne n’admet qu’une page par appel', () => {
  // La garantie de politesse envers le moteur : un crédit, une page.
  const apres = admettreUne([0, 1, 2], [5, 6, 7]);
  assert.equal(apres.length, 4);
  assert.ok(apres.includes(5));
});

test('admettreUne ne double pas une page déjà admise', () => {
  const apres = admettreUne([0, 1, 2, 3], [1, 2, 3]);
  assert.deepEqual(apres, [0, 1, 2, 3]);
});

// Défaut mesuré le 2026-10-18 : en défilement continu, l'admission « une page
// par onLoad » cessait d'avancer dès que les chargements prenaient du retard.
// La fenêtre montée n'avait alors plus aucun `src` — 6 requêtes au total, puis
// une bande vide. La fenêtre **est** ce que l'utilisateur regarde : elle doit
// pouvoir charger, sans attendre un décodage qui ne viendra pas.

test('en bande virtualisée, toute la fenêtre est affichable', () => {
  const vues = pagesAffichees([0, 1, 2, 3, 4, 5], [4, 5, 6, 7, 8, 9], true);
  assert.deepEqual([...vues].sort((a, b) => a - b), [0, 1, 2, 3, 4, 5, 6, 7, 8, 9]);
});

test('hors virtualisation, la fenêtre n’élargit pas le quota', () => {
  // En flux normal `fenetreBande` rend toutes les pages : les unionner
  // lancerait 303 requêtes d'un coup.
  const vues = pagesAffichees([0, 1, 2, 3, 4, 5], [0, 1, 2, 3, 4, 5, 6, 7, 8, 9], false);
  assert.deepEqual([...vues].sort((a, b) => a - b), [0, 1, 2, 3, 4, 5]);
});

test('une fenêtre déjà admise ne change pas l’ensemble', () => {
  const vues = pagesAffichees([0, 1, 2], [0, 1, 2], true);
  assert.deepEqual([...vues].sort((a, b) => a - b), [0, 1, 2]);
});
