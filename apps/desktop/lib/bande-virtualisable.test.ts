// Porte : la bande ne se virtualise que si AU MOINS UNE page a une hauteur
// rendue connue. Corollaire : un chapitre entierement inconnu ne se virtualise
// pas.
import assert from 'node:assert/strict';
import { describe, it } from 'node:test';
import {
  bandeVirtualisableEst,
  hauteurPage,
  hauteurRendue,
  hauteursBande,
} from './bande-hauteurs.ts';

describe('virtualisation de la bande', () => {
  it('un chapitre entierement inconnu n est PAS virtualisable', () => {
    // Mesure du 2026-10-09 : chapitre de 15 pages, bande reservee 13 461 px
    // pour 11 661 px rendus, saut vers la page 10 -> page 9.
    const pages = [{}, {}, {}, {}, {}];
    const rendues = pages.map((p) => hauteurRendue(p, 900));
    assert.equal(bandeVirtualisableEst(rendues), false);
  });

  it('un chapitre vide n est pas virtualisable', () => {
    assert.equal(bandeVirtualisableEst([]), false);
  });

  it('une seule page connue suffit a virtualiser', () => {
    const rendues = [null, null, 467, null];
    assert.equal(bandeVirtualisableEst(rendues), true);
  });

  it('toutes connues : virtualisable', () => {
    const pages = [
      { largeur: 720, hauteur: 500 },
      { largeur: 720, hauteur: 700 },
    ];
    assert.equal(
      bandeVirtualisableEst(pages.map((p) => hauteurRendue(p, 900))),
      true,
    );
  });

  it('la hauteur de positionnement reste positive dans tous les cas', () => {
    // Le positionnement a besoin d'un nombre ; la virtualisation, d'une mesure.
    // Les deux rôles ne se contredisent pas.
    for (const pages of [[{}, {}], [], [{ largeur: 720, hauteur: 500 }]]) {
      const h = hauteursBande(pages, 900);
      assert.equal(h.length, pages.length);
      for (const x of h) assert.ok(x > 0);
    }
  });

  it('un chapitre inconnu donne des hauteurs positives sans etre virtualisable', () => {
    // C'est le piege exact : `some(h <= 0)` est toujours faux, donc l'ancien
    // test « la hauteur reste positive » passait alors que la bande etait faux.
    const h = hauteursBande([{}, {}, {}, {}, {}], 900);
    assert.ok(h.every((x) => x > 0), 'les hauteurs sont positives');
    assert.equal(
      bandeVirtualisableEst([{}, {}, {}, {}, {}].map((p) => hauteurRendue(p, 900))),
      false,
      'mais rien est connu, donc la bande ne doit PAS se virtualiser',
    );
  });

  it('une page de 1x1 n est pas une mesure', () => {
    // Un `<img>` non decode renvoie parfois 1x1. Le prendre pour une mesure
    // virtualiserait une bande dont rien n'est su.
    const rendues = [hauteurRendue({ largeur: 1, hauteur: 1 }, 900), null];
    assert.equal(bandeVirtualisableEst(rendues), false);
  });
});
