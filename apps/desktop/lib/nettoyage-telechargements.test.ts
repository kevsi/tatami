import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { chapitreLu } from './nettoyage-telechargements.ts';

const chapitres = [{ url: '/ch/3' }, { url: '/ch/2' }, { url: '/ch/1' }];

describe('chapitre supprimable (lu jusqu’au bout)', () => {
  it('rend le chapitre courant quand la dernière page est atteinte', () => {
    assert.deepEqual(
      chapitreLu(chapitres, { chapter_url: '/ch/2', page_index: 19, page_count: 20 }),
      { url: '/ch/2' },
    );
  });

  it('ne rend rien si le chapitre n’est pas fini', () => {
    assert.equal(chapitreLu(chapitres, { chapter_url: '/ch/2', page_index: 4, page_count: 20 }), null);
  });

  it('ne rend rien sans position de lecture', () => {
    assert.equal(chapitreLu(chapitres, null), null);
    assert.equal(chapitreLu(chapitres, {}), null);
  });

  it('ne rend rien pour un chapitre absent des téléchargements', () => {
    assert.equal(
      chapitreLu(chapitres, { chapter_url: '/ch/9', page_index: 19, page_count: 20 }),
      null,
    );
  });

  it('ne rend rien quand le nombre de pages est inconnu', () => {
    // `page_count = 0` veut dire « pas d'information », pas « chapitre vide » :
    // sans cette garde, `index >= total - 1` serait vrai pour tout index ≥ -1.
    assert.equal(chapitreLu(chapitres, { chapter_url: '/ch/1', page_index: 0, page_count: 0 }), null);
  });
});