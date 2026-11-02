import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { PAGES_AVANCE, DELAI_SURVOL_MS, doitPrechargerAuSurvol, pagesAPrecharger } from './prechargement-pages.ts';

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

describe('préchargement au survol d’un chapitre', () => {
  it('précharge un chapitre inconnu quand on ne lit pas', () => {
    assert.equal(doitPrechargerAuSurvol(false, false), true);
  });

  it('ne redemande pas un chapitre déjà en mémoire', () => {
    assert.equal(doitPrechargerAuSurvol(true, false), false);
  });

  it('ne précharge rien pendant une lecture', () => {
    assert.equal(doitPrechargerAuSurvol(false, true), false);
  });

  it('le délai de survol ignore un simple passage de la souris', () => {
    assert.ok(DELAI_SURVOL_MS >= 150, 'un délai trop court préchargerait au moindre mouvement');
    assert.ok(DELAI_SURVOL_MS <= 500, 'un délai long retarderait le bénéfice du survol');
  });
});