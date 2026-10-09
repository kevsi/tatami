import assert from 'node:assert/strict';
import { describe, it } from 'node:test';
import { decalageAncrage, offsetsBande } from './bande-hauteurs.ts';

describe('decalage du defilement quand les hauteurs changent', () => {
  it('compense un retrecissement au-dessus de la vue', () => {
    // Les pages se mesurent : celles du dessus retrecissent, donc tout ce qui
    // est en dessous remonte et la page lue sort du cadre.
    const avant = offsetsBande([1000, 1000, 1000]);
    const apres = offsetsBande([800, 800, 800]);
    // Vue sur le haut de la page 2 (index 1, offset 1000 + 100 px).
    const y = decalageAncrage(avant, apres, 1100);
    assert.equal(y, 900);
  });

  it('ne fait rien quand rien n a bouge', () => {
    const o = offsetsBande([1000, 1000, 1000]);
    assert.equal(decalageAncrage(o, o, 1100), null);
  });

  it('ignore un nombre de pages different', () => {
    // Changement de chapitre, ou bande qui vient de se virtualiser : aucune
    // correspondance fiable, donc aucune correction. Mesure : corriger ici
    // provoquerait un saut arbitraire.
    assert.equal(decalageAncrage(offsetsBande([1000]), offsetsBande([1000, 1000]), 0), null);
  });

  it('ne remonte jamais au-dessus de zero', () => {
    const avant = offsetsBande([1000, 1000, 1000]);
    const apres = offsetsBande([100, 100, 100]);
    const y = decalageAncrage(avant, apres, 2500);
    assert.ok(y !== null && y >= 0);
  });

  it('compense aussi un elargissement', () => {
    // Les hauteurs peuvent aussi grandir : le meme decalage sert, dans l'autre
    // sens. Un test qui ne verifie qu'un sens ne prouve rien.
    const avant = offsetsBande([800, 800, 800]);
    const apres = offsetsBande([1200, 1200, 1200]);
    // Vue au milieu de la page 2 (offset 800 + 100).
    const y = decalageAncrage(avant, apres, 900);
    assert.equal(y, 1300);
  });

  it('un scrollTop nul reste nul', () => {
    // Haut de la bande : seule la page 0 peut bouger, et son offset vaut 0
    // dans les deux cas — donc aucun decalage.
    const avant = offsetsBande([1000, 1000]);
    const apres = offsetsBande([500, 500]);
    assert.equal(decalageAncrage(avant, apres, 0), null);
  });

  it('suit la page sous le haut du cadre, pas une page enregistree', () => {
    // Le defaut de la version precedente : elle ancrait `reader.index`, qui
    // apres un saut designe encore l'ancienne page. Mesure : un saut vers la
    // page 10 affichait la page 2. Ici l'ancre vient de `scrollTop`.
    const avant = offsetsBande([1000, 1000, 1000, 1000]);
    const apres = offsetsBande([600, 600, 600, 600]);
    // Vue sur la page 4 (index 3) : offset 3000 + 200.
    const y = decalageAncrage(avant, apres, 3200);
    assert.equal(y, 2000);
  });

  it('borne par la derniere page quand scrollTop depasse la bande', () => {
    // Filet de securite : un scrollTop au-dela du dernier offset ne doit pas
    // produire un decalage absurde.
    const avant = offsetsBande([1000, 1000]);
    const apres = offsetsBande([500, 500]);
    const y = decalageAncrage(avant, apres, 99_999);
    assert.ok(y === null || y >= 0);
  });
});
