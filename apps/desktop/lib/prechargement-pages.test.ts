import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { PAGES_AVANCE, pagesAPrecharger } from './prechargement-pages.ts';

describe('préchargement des pages du lecteur', () => {
  it('demande les pages devant la page courante', () => {
    assert.deepEqual(pagesAPrecharger(0, 20, new Set()), [1, 2, 3, 4, 5, 6]);
  });

  it('ne redemande pas ce qui a déjà été préchargé', () => {
    assert.deepEqual(pagesAPrecharger(0, 20, new Set([1, 2, 3])), [4, 5, 6]);
  });

  it('s’arrête à la dernière page du chapitre', () => {
    assert.deepEqual(pagesAPrecharger(18, 20, new Set()), [19]);
  });

  it('ne dépasse jamais la fin du chapitre', () => {
    assert.deepEqual(pagesAPrecharger(19, 20, new Set()), []);
  });

  it('refuse un index incohérent plutôt que de demander au hasard', () => {
    assert.deepEqual(pagesAPrecharger(-1, 20, new Set()), []);
    assert.deepEqual(pagesAPrecharger(1.5, 20, new Set()), []);
    assert.deepEqual(pagesAPrecharger(0, 0, new Set()), []);
  });

  it('la fenêtre par défaut couvre plus d’un écran de marge', () => {
    assert.ok(PAGES_AVANCE >= 4, 'moins de 4 pages d’avance ne couvrirait pas un défilement rapide');
  });
});