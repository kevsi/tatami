// Identité d'un signet, côté interface.
//
// La table `bookmarks` a pour clé primaire
// `(source, manga_url, chapter_url, page_index)` (`store.rs:278`), et
// `remove_bookmark` supprime sur ces quatre champs (`store.rs:758-759`).
// L'interface comparait `(chapter_url, page_index)` seulement : deux sources
// partageant une URL de chapitre relative — `/read/abc/1` existe chez
// plusieurs — étaient confondues. Le filtre retirait alors de l'affichage une
// entrée pendant que SQL en supprimait une autre, donc l'écran montrait un
// signet qui n'existait plus (ou l'inverse).
//
// La règle est ici, dans `lib/`, parce qu'elle était écrite dans le
// composant : aucune porte ne pouvait la vérifier (même raison que
// `lib/bande-hauteurs.ts`).
import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { memeSignet, type CleSignet } from './signet-cle.ts';

const base: CleSignet = {
  source: 'MANGADEX',
  manga_url: '/manga/123',
  chapter_url: '/chapter/456',
  page_index: 3,
};

describe('identité d’un signet', () => {
  it('deux signets identiques sont le même', () => {
    assert.equal(memeSignet(base, { ...base }), true);
  });

  it('la page distingue deux signets du même chapitre', () => {
    assert.equal(memeSignet(base, { ...base, page_index: 4 }), false);
  });

  it('la SOURCE distingue deux signets de la même page', () => {
    // Le défaut d'origine : cette comparaison n'existait pas. Deux sources
    // servent couramment la même URL relative de chapitre.
    assert.equal(memeSignet(base, { ...base, source: 'SCANFR' }), false);
  });

  it('le manga distingue deux signets de la même page', () => {
    assert.equal(memeSignet(base, { ...base, manga_url: '/manga/999' }), false);
  });

  it('le chapitre distingue deux signets', () => {
    assert.equal(memeSignet(base, { ...base, chapter_url: '/chapter/789' }), false);
  });

  it('toutes les conditions sont nécessaires — preuve que le test mord', () => {
    // Contre-épreuve : chacune des quatre comparaisons change le verdict.
    // Sans elle, un `memeSignet` qui rendrait toujours `true` passerait les
    // tests « même signet » et échouerait seulement sur les autres.
    for (const champ of ['source', 'manga_url', 'chapter_url'] as const) {
      assert.equal(memeSignet(base, { ...base, [champ]: 'autre' }), false, champ);
    }
    assert.equal(memeSignet(base, { ...base, page_index: 99 }), false, 'page_index');
  });
});
