import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  MAX_CHALLENGES,
  cleChallenge,
  enregistrerEchecChallenge,
  reinitialiserChallenge,
} from './challenge-garde.ts';

// Le défaut mesuré le 2026-10-10 (TOONFR) : le cycle « vérification →
// relecture → vérification » ne se terminait jamais, faute de compteur.
// Ces tests verrouillent la borne : deux tentatives, pas trois.

test('deux échecs consécutifs sont tolérés, le troisième épuise', () => {
  let t: ReadonlyMap<string, number> = new Map();
  const r1 = enregistrerEchecChallenge(t, 'TOONFR', 'latest');
  assert.equal(r1.epuise, false, 'le premier échec ouvre le cycle');
  t = r1.tentatives;
  const r2 = enregistrerEchecChallenge(t, 'TOONFR', 'latest');
  assert.equal(r2.epuise, false, 'la deuxième tentative est encore permise');
  t = r2.tentatives;
  assert.equal(t.get(cleChallenge('TOONFR', 'latest')), MAX_CHALLENGES);
  const r3 = enregistrerEchecChallenge(t, 'TOONFR', 'latest');
  assert.equal(r3.epuise, true, 'le troisième rend la main : plus de bandeau');
  assert.equal(r3.tentatives.get(cleChallenge('TOONFR', 'latest')), MAX_CHALLENGES + 1);
});

test('le compteur retombe à zéro quand l action réussit', () => {
  let t = enregistrerEchecChallenge(new Map(), 'TOONFR', 'latest').tentatives;
  t = enregistrerEchecChallenge(t, 'TOONFR', 'latest').tentatives;
  t = reinitialiserChallenge(t, 'TOONFR', 'latest');
  const r = enregistrerEchecChallenge(t, 'TOONFR', 'latest');
  assert.equal(r.epuise, false, 'après un succès, on repart de zéro');
  assert.equal(r.tentatives.get(cleChallenge('TOONFR', 'latest')), 1);
});

test('une source épuisée ne coupe pas les autres, ni une autre action', () => {
  let t = enregistrerEchecChallenge(new Map(), 'TOONFR', 'latest').tentatives;
  t = enregistrerEchecChallenge(t, 'TOONFR', 'latest').tentatives;
  t = enregistrerEchecChallenge(t, 'TOONFR', 'latest').tentatives; // épuisée
  assert.equal(
    enregistrerEchecChallenge(t, 'MANGADEX', 'latest').epuise,
    false,
    'une autre source garde ses deux tentatives',
  );
  assert.equal(
    enregistrerEchecChallenge(t, 'TOONFR', 'lecture').epuise,
    false,
    'une autre action sur la même source aussi',
  );
});

test('réinitialiser une action saine ne change rien', () => {
  const vide = new Map<string, number>();
  assert.equal(
    reinitialiserChallenge(vide, 'TOONFR', 'latest'),
    vide,
    'même instance : pas de rendu provoqué pour rien',
  );
});
