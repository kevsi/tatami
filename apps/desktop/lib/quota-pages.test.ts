import { test } from 'node:test';
import assert from 'node:assert/strict';
import { admettreUne, admettreLaCible } from './quota-pages.ts';

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
