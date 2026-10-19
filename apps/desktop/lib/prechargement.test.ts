// Cible du préchargement d'une fiche (chantier perf 2026-10-19, B).
//
// Mesuré le 2026-10-19 sur l'artefact livré : la première ouverture d'une
// fiche coûte ~3,3 s et le premier clic utile — « Reprendre » — rouvrait un
// lecteur à ~3,8 s, alors qu'un chapitre déjà chargé se rouvre en 10 à 76 ms.
// Le préchargement vise ce premier clic, et sa cible se décide ici : une règle
// qui déclenche une requête réseau mérite un test.
import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { ciblePrechargement } from './prechargement.ts';

const chapitres = [{ url: '/ch/3' }, { url: '/ch/2' }, { url: '/ch/1' }];

describe('cible du préchargement d’une fiche', () => {
  it('sans reprise, vise le premier chapitre', () => {
    assert.deepEqual(ciblePrechargement(chapitres, null), { url: '/ch/3' });
    assert.deepEqual(ciblePrechargement(chapitres, undefined), { url: '/ch/3' });
  });

  it('avec une reprise présente, vise ce chapitre', () => {
    assert.deepEqual(ciblePrechargement(chapitres, '/ch/1'), { url: '/ch/1' });
  });

  it('reprise absente de la liste : retombe sur le premier', () => {
    // Contre-épreuve du cas nominal : le `find` manqué ne doit pas faire
    // précharger une URL inconnue du moteur.
    assert.deepEqual(ciblePrechargement(chapitres, '/ch/9'), { url: '/ch/3' });
  });

  it('liste vide : ne vise rien (aucune requête)', () => {
    assert.equal(ciblePrechargement([], '/ch/1'), null);
    assert.equal(ciblePrechargement([], null), null);
  });
});
