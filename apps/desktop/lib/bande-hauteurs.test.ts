// `node:test` ne fournit que `describe`/`it` : l'assertion vient de
// `node:assert`, comme dans tout test de ce projet.
import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import {
  hauteurPage,
  hauteursBande,
  offsetsBande,
  HAUTEUR_REPLI,
} from './bande-hauteurs.ts';

describe('hauteurs de la bande webtoon', () => {
  it('met la hauteur des dimensions a l\'echelle de la fenetre', () => {
    // Page 800x2171 dans une bande de 400 px de large : 2171 * 400/800.
    assert.equal(hauteurPage({ largeur: 800, hauteur: 2171 }, 400), 1086);
  });

  it('double la hauteur quand on double la largeur de la fenetre', () => {
    const p = { largeur: 800, hauteur: 1600 };
    assert.equal(hauteurPage(p, 400), 800);
    assert.equal(hauteurPage(p, 800), 1600);
  });

  it('donne une hauteur de repli a une page dont les dimensions manquent', () => {
    // C'est le cas mesure : une page que la source ne sert pas.
    assert.equal(hauteurPage({}, 900), HAUTEUR_REPLI);
    assert.equal(hauteurPage({ largeur: 0, hauteur: 0 }, 900), HAUTEUR_REPLI);
  });

  it('ne rend jamais une hauteur nulle', () => {
    // Le seuil de virtualisation est `h > 0` : une hauteur nulle desactive
    // toute la bande, donc une seule page en echec suffisait a la desactiver.
    for (const largeurBande of [0, 1, 400, 800]) {
      for (const p of [{}, { largeur: 0, hauteur: 0 }, { largeur: 800, hauteur: 2171 }]) {
        const h = hauteurPage(p, largeurBande);
        assert.ok(h > 0, `hauteur nulle pour ${JSON.stringify(p)} a ${largeurBande}px`);
        assert.ok(Number.isFinite(h), `hauteur non finie pour ${JSON.stringify(p)}`);
      }
    }
  });

  it('reste stable quand la fenetre n\'a pas encore de largeur', () => {
    // Avant le ResizeObserver, largeurBande vaut 0. Une valeur nulle ferait
    // tomber toutes les hauteurs a zero.
    assert.ok(hauteurPage({ largeur: 800, hauteur: 2171 }, 0) > 0);
  });

  it('donne un offset cumule coherent', () => {
    const hauteurs = [100, 200, 300];
    assert.deepEqual(offsetsBande(hauteurs), [0, 100, 300]);
  });

  it('donne un offset vide pour une bande vide', () => {
    assert.deepEqual(offsetsBande([]), []);
    assert.deepEqual(hauteursBande([], 400), []);
  });

  it('conserve l\'ordre des pages dans les hauteurs', () => {
    const pages = [
      { largeur: 800, hauteur: 1000 },
      {},
      { largeur: 800, hauteur: 2000 },
    ];
    const h = hauteursBande(pages, 800);
    assert.equal(h.length, 3);
    assert.equal(h[0], 1000);
    assert.equal(h[2], 2000);
    assert.ok(h[1] > 0, 'la page sans dimensions garde une hauteur');
  });
});