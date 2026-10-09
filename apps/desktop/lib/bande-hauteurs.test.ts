// `node:test` ne fournit que `describe`/`it` : l'assertion vient de
// `node:assert`, comme dans tout test de ce projet.
import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import {
  hauteurPage,
  hauteurRendue,
  hauteurRendueMesuree,
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

  // --- Le défaut mesuré le 2026-10-09 : TOOMICSFR, lecteur webtoon ---
  //
  // Rust ne connaît les dimensions que des pages **déjà en cache**
  // (`commands.rs:304`). Un chapitre ouvert pour la première fois n'en a donc
  // aucune, et toutes les pages retombaient sur `HAUTEUR_REPLI` — un ratio
  // de 1600/900 inventé, alors que la vraie page mesurait 720x500.
  //
  // Mesure dans l'application réelle (6 images, toutes étirées) :
  //
  //   |        | dim. réelles | dim. affichées | height inline | écart |
  //   |--------|--------------|----------------|---------------|-------|
  //   | page 0 | 720x500      | 673x1196       | 1196px        | -60,9%|
  //
  // La distinction qui manque, et que ces tests-named : le repli sert à
  // **positionner** les pages (un décalage de défilement se voit à peine), mais
  // pas à **déformer** l'image. Poser le repli en `height` inline écrase le
  // ratio naturel : `box-sizing: border-box` et `height: auto` ne peuvent rien,
  // un style inline gagne toujours.
  describe('hauteur rendue vs hauteur de positionnement', () => {
    it('sans dimensions, ne pose AUCUNE hauteur sur l image', () => {
      // `null` = « ne mets pas de height » : le navigateur garde alors le
      // ratio naturel et l image n'est pas étirée.
      assert.equal(hauteurRendue({}, 673), null);
      assert.equal(hauteurRendue({ largeur: 0, hauteur: 0 }, 673), null);
      assert.equal(hauteurRendue({ largeur: 720, hauteur: 0 }, 673), null);
    });

    it('avec dimensions, pose la hauteur calculee', () => {
      assert.equal(hauteurRendue({ largeur: 720, hauteur: 500 }, 673), 467);
    });

    it('la hauteur de positionnement reste un repli, elle, toujours positive', () => {
      // Les deux roles sont distincts : le positionnement a besoin d'un nombre,
      // le rendu a besoin de ne rien imposer.
      assert.ok(hauteurPage({}, 673) > 0);
    });

    it('une mesure de 1x1 ne remplace pas une vraie dimension', () => {
      // Un `<img>` non decode peut renvoyer 1x1. Le prendre pour une mesure
      // reelle ecraserait une bonne dimension par 1 px de haut.
      assert.equal(hauteurRendue({ largeur: 1, hauteur: 1 }, 673), null);
      assert.equal(hauteurRendue({ largeur: 720, hauteur: 500 }, 673), 467);
    });

    it('ignore une largeur de bande nulle plutot que deReturning zero', () => {
      // Avant le ResizeObserver, largeurBande vaut 0.
      assert.ok(hauteurRendue({ largeur: 720, hauteur: 500 }, 0) > 0);
    });

    it('une bande rendue ne vaut jamais zero', () => {
      const pages = [
        { largeur: 720, hauteur: 500 },
        { largeur: 0, hauteur: 0 },
        { largeur: 800, hauteur: 2000 },
      ];
      const rendues = pages.map((p) => hauteurRendue(p, 673));
      assert.equal(rendues[0], 467);
      assert.equal(rendues[1], null);
      assert.equal(rendues[2], 1683);
      for (const h of rendues.filter((x) => x !== null)) {
        assert.ok(h > 0, 'hauteur rendue nulle');
      }
    });
  });

  // Le cas reel du lecteur : une page sans dimensions Rust, mais dont le
  // navigateur a deja decode le ratio. C'est par la que la bande se recale.
  describe('mesure du navigateur', () => {
    it('la mesure prime quand Rust ne connait rien', () => {
      assert.equal(
        hauteurRendueMesuree({}, 673, { w: 720, h: 500 }),
        hauteurRendue({ largeur: 720, hauteur: 500 }, 673),
      );
    });

    it('la mesure prime sur une dimension Rust obsolete', () => {
      // La page a ete reencodee sur le serveur : la mesure du navigateur est
      // plus fraiche que ce que Rust a lu dans l'entete au premier appel.
      assert.equal(hauteurRendueMesuree({ largeur: 800, hauteur: 800 }, 673, { w: 720, h: 500 }), 467);
    });

    it('sans mesure, on retombe sur les dimensions Rust', () => {
      assert.equal(
        hauteurRendueMesuree({ largeur: 800, hauteur: 2000 }, 673, undefined),
        hauteurRendue({ largeur: 800, hauteur: 2000 }, 673),
      );
    });

    it('une mesure de 1x1 est ignoree, pas prise pour un ratio', () => {
      assert.equal(hauteurRendueMesuree({}, 673, { w: 1, h: 1 }), null);
      assert.equal(hauteurRendueMesuree({ largeur: 800, hauteur: 2000 }, 673, { w: 1, h: 1 }), 1683);
    });

    it('hauteur de positionnement : la mesure prime aussi', () => {
      const pages = [{}, {}];
      const h = hauteursBande(pages, 673, { 0: { w: 720, h: 500 } });
      assert.equal(h[0], 467);
      // La page 1, non mesuree : elle garde le repli, jamais zero.
      assert.ok(h[1] > 0);
      assert.notEqual(h[0], h[1], 'la mesure n a pas ete prise en compte');
    });
  });
});