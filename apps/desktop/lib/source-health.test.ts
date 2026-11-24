import assert from 'node:assert/strict';
import test from 'node:test';
import {
  effacerEchecsSources,
  lireSanteSources,
  noterEchecsSources,
} from './source-health.ts';

test('le journal de santé ignore les entrées illisibles', () => {
  assert.deepEqual(lireSanteSources('{'), {});
  assert.deepEqual(lireSanteSources(JSON.stringify({
    valide: { code: 'TIMEOUT', message: 'délai dépassé', at: 42 },
    incomplet: { code: 'INTERNAL', at: 'hier' },
  })), {
    valide: { code: 'TIMEOUT', message: 'délai dépassé', at: 42 },
  });
});

test('un échec enregistre le code, le message et son horodatage', () => {
  const avant = { AUTRE: { code: 'HTTP', message: '403', at: 10 } };
  const apres = noterEchecsSources(avant, ['SUSHISCANFR'], 'TIMEOUT', 'délai', 20);

  assert.deepEqual(apres.SUSHISCANFR, { code: 'TIMEOUT', message: 'délai', at: 20 });
  assert.deepEqual(avant, { AUTRE: { code: 'HTTP', message: '403', at: 10 } });
});

test('une réponse réussie efface la panne même si la liste est vide', () => {
  const avant = {
    VIDE: { code: 'PARSE_ERROR', message: 'ancienne erreur', at: 10 },
    AUTRE: { code: 'TIMEOUT', message: 'toujours en échec', at: 11 },
  };
  const apres = effacerEchecsSources(avant, ['VIDE']);

  assert.equal('VIDE' in apres, false);
  assert.deepEqual(apres.AUTRE, avant.AUTRE);
  assert.equal('VIDE' in avant, true);
});
