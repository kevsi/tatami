import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  lireEpingles,
  basculerEpingle,
  estEpinglee,
  separerEpinglees,
  EPINGLES_KEY,
} from './sources-epinglees.ts';

test('lireEpingles : null/vide/absent → vide', () => {
  assert.deepEqual(lireEpingles(null), []);
  assert.deepEqual(lireEpingles(undefined), []);
  assert.deepEqual(lireEpingles(''), []);
  assert.deepEqual(lireEpingles('   '), []);
});

test('lireEpingles : JSON invalide ou non tableau → vide', () => {
  assert.deepEqual(lireEpingles('{'), []);
  assert.deepEqual(lireEpingles('42'), []);
  assert.deepEqual(lireEpingles('"x"'), []);
  assert.deepEqual(lireEpingles('null'), []);
});

test('lireEpingles : déduplique et ignore les non-chaînes', () => {
  const raw = JSON.stringify(['a', 'b', 'a', 42, '', '  ', 'c']);
  assert.deepEqual(lireEpingles(raw), ['a', 'b', 'c']);
});

test('basculerEpingle : ajoute puis retire', () => {
  let e = basculerEpingle([], 'A');
  assert.deepEqual(e, ['A']);
  assert.equal(estEpinglee(e, 'A'), true);
  e = basculerEpingle(e, 'A');
  assert.deepEqual(e, []);
  assert.equal(estEpinglee(e, 'A'), false);
});

test('basculerEpingle : nom vide sans effet', () => {
  assert.deepEqual(basculerEpingle(['A'], '   '), ['A']);
});

test('basculerEpingle : ne mute pas l\'entrée', () => {
  const src = ['A'];
  const r = basculerEpingle(src, 'B');
  assert.deepEqual(src, ['A']);
  assert.deepEqual(r, ['A', 'B']);
});

test('separerEpinglees : conserve l\'ordre dans chaque groupe', () => {
  const src = [{ name: 'a' }, { name: 'b' }, { name: 'c' }, { name: 'd' }];
  const r = separerEpinglees(src, ['c', 'a']);
  assert.deepEqual(
    r.epinglees.map((s) => s.name),
    ['a', 'c'],
  );
  assert.deepEqual(
    r.autres.map((s) => s.name),
    ['b', 'd'],
  );
});

test('EPINGLES_KEY : chaîne stable', () => {
  assert.equal(EPINGLES_KEY, 'tatami.pinnedSources');
});