import { test } from 'node:test';
import assert from 'node:assert/strict';
import { describeError, estRejeuRefuse } from './erreurs-source.ts';

// Un rejeu refusé doit être render comme tel, et surtout **pas** comme une
// demande de verification : sinon le bandeau propose de rouvrir une fenetre qui
// ne peut qu'echouer (cycle mesure le 2026-10-11 sur HENTAIZONE).

/** Faux traducteur : renvoie la cle, donc on observe le CLASSEMENT. */
const t = (key: string, vars?: Record<string, string | number>) =>
  vars?.source ? `${key}:${String(vars.source)}` : key;

test('un rejeu refuse est reconnu', () => {
  assert.equal(
    estRejeuRefuse('CHALLENGE_UNREPLAYABLE: challenge at https://x.fr (non rejouable)'),
    true,
  );
});

test('une demande de verification ordinaire n est PAS un rejeu refuse', () => {
  assert.equal(estRejeuRefuse('CHALLENGE_REQUIRED: challenge at https://x.fr'), false);
});

test('le rejeu refuse est classe avant la verification ordinaire', () => {
  // Les deux chaines partagent leur debut : sans l'ordre des tests, la seconde
  // absorberait la premiere et le message promettrait une action impossible.
  const message = describeError(
    'CHALLENGE_UNREPLAYABLE: challenge at https://x.fr (non rejouable)',
    'HENTAIZONE',
    t,
  );
  assert.equal(message, 'erreur.verification.non.rejouable:HENTAIZONE');
});

test('une verification ordinaire garde son message', () => {
  const message = describeError('CHALLENGE_REQUIRED: challenge at https://x.fr', 'TOONFR', t);
  assert.equal(message, 'erreur.verification:TOONFR');
});

test('le message du rejeu refuse ne promet aucun reessai', () => {
  // `erreur.verification` dit « pas encore geree » : c'est un mensonge ici,
  // parce que rien ne peut reussir. La distinction doit rester dans les cles.
  const rejeter = describeError('CHALLENGE_UNREPLAYABLE: x', 'X', t);
  const demander = describeError('CHALLENGE_REQUIRED: x', 'X', t);
  assert.notEqual(rejeter, demander, 'les deux cas doivent avoir des messages distincts');
});

test('le reste du classement n a pas bouge', () => {
  assert.equal(describeError('TIMEOUT: 60s', 'S', t), 'erreur.delai:S');
  assert.equal(describeError('AUTH_REQUIRED: compte', 'S', t), 'erreur.compte.requis:S');
  assert.equal(describeError('RATE_LIMITED: trop', 'S', t), 'erreur.surcharge:S');
  assert.equal(describeError('403Forbidden', 'S', t), 'erreur.403:S');
});