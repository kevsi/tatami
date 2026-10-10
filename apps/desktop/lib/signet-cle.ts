/** Identité d'un signet, à l'identique de la clé de la table `bookmarks`.
 *
 *  `store.rs:278` : `PRIMARY KEY (source, manga_url, chapter_url, page_index)`,
 *  et `remove_bookmark` supprime sur ces quatre champs (`store.rs:758-759`).
 *  L'interface ne comparait que `chapter_url` et `page_index`.
 *
 *  Conséquence mesurable : deux sources qui servent la même URL relative de
 *  chapitre (`/chapter/456` existe chez plusieurs) étaient confondues. Le
 *  filtre d'affichage retirait une entrée pendant que SQL en supprimait une
 *  autre — l'écran montrait donc un signet qui n'existait plus, ou en cachait
 *  un qui existait encore.
 */
export type CleSignet = {
  source: string;
  manga_url: string;
  chapter_url: string;
  page_index: number;
};

/** Les quatre champs de la clé primaire décident, pas seulement deux. */
export function memeSignet(a: CleSignet, b: CleSignet): boolean {
  return (
    a.source === b.source &&
    a.manga_url === b.manga_url &&
    a.chapter_url === b.chapter_url &&
    a.page_index === b.page_index
  );
}
