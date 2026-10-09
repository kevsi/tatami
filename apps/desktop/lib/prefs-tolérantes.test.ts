import assert from 'node:assert/strict';
import { describe, it } from 'node:test';
import { estLisible, lireListeSources } from './prefs-tolérantes.ts';

describe('lecture tolérante des préférences (localStorage)', () => {
  it('lit une liste valide', () => {
    assert.deepEqual(lireListeSources('["MANGADEX","TOOMICSFR"]'), ['MANGADEX', 'TOOMICSFR']);
  });

  it('lit une liste vide comme un réglage, pas comme une absence', () => {
    // `[]` est une réponse valide : « aucune source activée ». C'est `null` qui
    // dit « pas de préférence », et les deux ne doivent pas être confondus.
    assert.deepEqual(lireListeSources('[]'), []);
    assert.equal(estLisible('[]'), true);
  });

  it('rend null pour une valeur absente', () => {
    assert.equal(lireListeSources(null), null);
    assert.equal(lireListeSources(''), null);
    assert.equal(estLisible(null), false);
  });

  it('rend null pour du JSON invalide - le défaut mesuré', () => {
    // C'est le cas qui privait l'app de sa bibliothèque, de ses téléchargements
    // et de ses écouteurs au démarrage, sans message.
    for (const casse of ['[', '{"a":1}', 'pas du json', '{"a":', '[1,2', 'null', '42', '"texte"']) {
      assert.equal(lireListeSources(casse), null, `« ${casse} » devrait être illisible`);
    }
  });

  it('rend null si un élément n est pas une chaîne', () => {
    // Un tableau partiellement valide ne doit PAS être réécrit en liste propre :
    // cela ferait disparaître des sources en silence.
    assert.equal(lireListeSources('["MANGADEX", 42]'), null);
    assert.equal(lireListeSources('["MANGADEX", null]'), null);
    assert.equal(lireListeSources('["MANGADEX", []]'), null);
  });

  it('accepte une liste de chaînes vides', () => {
    // Une chaîne vide reste une chaîne : on n'a pas à deviner ce qu'elle veut
    // dire, et la refuser supprimerait une source au prochain enregistrement.
    assert.deepEqual(lireListeSources('[""]', ), ['']);
  });

  it('ne confond jamais absence et liste vide', () => {
    // Le défaut symétrique : si `null` et `[]` étaient interchangeables, une
    // préférence illisible serait persistée comme « aucune source ».
    assert.notEqual(lireListeSources(null), lireListeSources('[]'));
    assert.notEqual(lireListeSources('cassé'), lireListeSources('[]'));
  });
});
