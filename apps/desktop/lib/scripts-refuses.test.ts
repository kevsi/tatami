import { test } from 'node:test';
import assert from 'node:assert/strict';
import { refusMoteur } from './scripts-refuses.ts';

// La forme réelle, relevée dans `tatami.log` le 2026-10-10 :
//   WARN sidecar: AVERTISSEMENT: Script ignoré (mangadexFr.js) :
//                 Erreur d'évaluation du script : <eval>:20:36 Missing close quote
const LIGNE = 'AVERTISSEMENT: Script ignoré (mangadexFr.js) : Erreur d\'évaluation du script : <eval>:20:36 Missing close quote';

test('le nom du fichier et la raison sont extraits', () => {
  const r = refusMoteur([LIGNE]);
  assert.deepEqual([...r.keys()], ['mangadexFr.js']);
  assert.match(r.get('mangadexFr.js')!, /Missing close quote/);
});

test('la position dans le message est conservée, car elle dit où regarder', () => {
  const r = refusMoteur([LIGNE]);
  assert.match(r.get('mangadexFr.js')!, /20:36/);
});

test('le préfixe horodaté de java.util ne pollue pas la raison', () => {
  const avecPrefixe = `Oct 10, 2026 10:13:13 AM org.tatami.engine.ScriptedSourceLoader charger\n${LIGNE}`;
  const r = refusMoteur([avecPrefixe]);
  assert.equal(r.get('mangadexFr.js')!.includes('ScriptedSourceLoader'), false);
});

test('une ligne qui ne parle pas de script est ignorée', () => {
  const r = refusMoteur([
    'Oct 10, 2026 10:13:13 AM org.tatami.engine.ScriptedSourceLoader charger',
    'SLF4J: Failed to load class org.slf4j.impl.StaticLoggerBinder',
  ]);
  assert.equal(r.size, 0);
});

test('un refus sans raison reste un refus : on ne perd pas le fichier', () => {
  const r = refusMoteur(['AVERTISSEMENT: Script ignoré (mangadexFr.js)']);
  assert.deepEqual([...r.keys()], ['mangadexFr.js']);
});

test('le dernier refus d’un même fichier gagne : le moteur recharge', () => {
  const r = refusMoteur([
    'AVERTISSEMENT: Script ignoré (a.js) : première erreur',
    'AVERTISSEMENT: Script ignoré (a.js) : erreur corrigée puis nouvelle',
  ]);
  assert.equal(r.size, 1);
  assert.match(r.get('a.js')!, /nouvelle/);
});

test('plusieurs fichiers refusés sont tous rapportés', () => {
  const r = refusMoteur([
    'AVERTISSEMENT: Script ignoré (a.js) : x',
    'AVERTISSEMENT: Script ignoré (b.js) : y',
  ]);
  assert.deepEqual([...r.keys()].sort(), ['a.js', 'b.js']);
});
