// Source d'un téléchargement lancé depuis une fiche.
//
// Le défaut mesuré le 2026-10-18 : une fiche ouverte depuis l'historique a
// `selected === null`, et la garde `if (!openManga || !selected) return`
// rendait le bouton de téléchargement inerte, sans message. La source existe
// pourtant sur la fiche (`openManga.source`) depuis `mangaDepuisHistorique`.
import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { sourceDeLaFiche } from './source-fiche.ts';

describe('sourceDeLaFiche', () => {
  it('rend la source d une fiche ouverte depuis l historique, sans selection', () => {
    assert.equal(sourceDeLaFiche({ source: 'custom:rimuscan' }), 'custom:rimuscan');
  });

  it('rend la source d une fiche ouverte depuis les favoris', () => {
    assert.equal(sourceDeLaFiche({ source: 'MANGADEX' }), 'MANGADEX');
  });

  it('ne rend rien sans fiche ouverte', () => {
    assert.equal(sourceDeLaFiche(null), null);
  });

  it('ne prend pas une source vide pour une source', () => {
    assert.equal(sourceDeLaFiche({ source: '' }), null);
  });
});
