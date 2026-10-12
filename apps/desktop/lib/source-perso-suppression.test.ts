import { test } from 'node:test';
import assert from 'node:assert/strict';
import { supprimerPuisRafraichirCatalogue } from './source-perso-suppression.ts';

test('la suppression est suivie du rafraichissement du catalogue moteur', async () => {
  const appels: string[] = [];

  await supprimerPuisRafraichirCatalogue(
    async () => {
      appels.push('supprimer_source_custom');
    },
    async () => {
      appels.push('list_sources');
    },
  );

  assert.deepEqual(appels, ['supprimer_source_custom', 'list_sources']);
});

test('un echec de suppression ne rafraichit pas le catalogue', async () => {
  const appels: string[] = [];

  await assert.rejects(
    supprimerPuisRafraichirCatalogue(
      async () => {
        appels.push('supprimer_source_custom');
        throw new Error('echec de suppression');
      },
      async () => {
        appels.push('list_sources');
      },
    ),
    /echec de suppression/,
  );

  assert.deepEqual(appels, ['supprimer_source_custom']);
});
