import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { formatTaille } from './format-taille.ts';

// Mesuré le 2026-11-02 dans la vue Téléchargements : 16334,1 Mo affichés pour
// ~15,9 Go. Un nombre à cinq chiffres avec une décimale ne se lit pas.
describe('formatTaille', () => {
  it('garde les octets sous 1 Ko', () => {
    assert.equal(formatTaille(512), '512 o');
  });

  it('passe en Ko jusqu’à 1024 Ko', () => {
    assert.equal(formatTaille(2048), '2 Ko');
    assert.equal(formatTaille(1024 * 1024 - 1), '1024 Ko');
  });

  it('garde les Mo avec une décimale sous 1000 Mo', () => {
    assert.equal(formatTaille(1.5 * 1024 * 1024), '1,5 Mo');
    assert.equal(formatTaille(999 * 1024 * 1024), '999,0 Mo');
  });

  it('passe en Go au-delà de 1000 Mo (cas mesuré : 16334,1 Mo)', () => {
    assert.equal(formatTaille(16334.1 * 1024 * 1024), '16,0 Go');
  });

  it('résiste à une valeur absurde', () => {
    assert.equal(formatTaille(Number.NaN), '0 o');
    assert.equal(formatTaille(-5), '-5 o');
  });
});