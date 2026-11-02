import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { prioriserChapitreEnCours } from './ordre-telechargement.ts';

const liste = [{ url: '/ch/3' }, { url: '/ch/2' }, { url: '/ch/1' }];
const urls = (l: { url: string }[]) => l.map((c) => c.url);

describe('priorité du chapitre en cours de lecture', () => {
  it('met le chapitre lu en tête', () => {
    assert.deepEqual(urls(prioriserChapitreEnCours(liste, '/ch/2')), ['/ch/2', '/ch/3', '/ch/1']);
  });

  it('ne change rien quand le chapitre est déjà en tête', () => {
    assert.deepEqual(urls(prioriserChapitreEnCours(liste, '/ch/3')), ['/ch/3', '/ch/2', '/ch/1']);
  });

  it('ne change rien sans position de lecture', () => {
    assert.deepEqual(urls(prioriserChapitreEnCours(liste, null)), ['/ch/3', '/ch/2', '/ch/1']);
    assert.deepEqual(urls(prioriserChapitreEnCours(liste, undefined)), ['/ch/3', '/ch/2', '/ch/1']);
  });

  it('ignore un chapitre absent de la liste (source réordonnée)', () => {
    assert.deepEqual(urls(prioriserChapitreEnCours(liste, '/ch/9')), ['/ch/3', '/ch/2', '/ch/1']);
  });

  it('ne modifie pas la liste reçue', () => {
    const copie = urls(liste);
    prioriserChapitreEnCours(liste, '/ch/1');
    assert.deepEqual(urls(liste), copie);
  });

  it('conserve tous les chapitres (aucun n’est perdu)', () => {
    const resultat = prioriserChapitreEnCours(liste, '/ch/1');
    assert.equal(resultat.length, liste.length);
    assert.deepEqual([...urls(resultat)].sort(), [...urls(liste)].sort());
  });
});