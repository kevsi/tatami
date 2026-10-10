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
  fenetreBande,
  pasDeDefilement,
  PAS_DEFILEMENT_MIN,
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

  // --- Le défaut mesuré le 2026-10-09, deuxième passe ---
  //
  // `hauteurPage` donne maintenant une hauteur de repli (1600/900) à une page
  // sans dimensions, pour que la virtualisation ne s'effondre pas. Conséquence
  // non vue : `bandeVirtualisable` (`hauteurs.length > 0 && !some(h <= 0)`)
  // devient **toujours vrai** — y compris quand AUCUNE page n'est mesurée.
  //
  // Mesuré dans l'application réelle, chapitre de 15 pages ouvert pour la
  // première fois : bande de 13 461 px pour 11 661 px rendus, et le saut vers
  // la page 10 atterrit sur la 9. La bande est donc positionnée sur des
  // hauteurs **inventées** alors que les images se rendent à leur ratio réel.
  //
  // La règle correcte : une page dont on ne connaît pas la hauteur rendue ne
  // doit pas être comptée comme connue.
  describe('virtualisation : ce qui est « connu »', () => {
    it('un chapitre entierement inconnu n est pas virtualisable', () => {
      // Rust ne connaît que les pages déjà en cache : un chapitre neuf n'a
      // donc aucune dimension, et le repli ne doit pas faire croire l'inverse.
      const pages = [{}, {}, {}, {}, {}];
      const h = hauteursBande(pages, 900);
      assert.equal(h.length, 5);
      for (const x of h) assert.ok(x > 0, 'hauteur de positionnement, donc positive');
      // Mais la virtualisation, elle, exige des hauteurs RENDUES connues.
      assert.equal(pages.every((p) => hauteurRendue(p, 900) !== null), false);
      assert.equal(pages.some((p) => hauteurRendue(p, 900) !== null), false);
    });

    it('un melange de connues et d inconnues est virtualisable', () => {
      // Dès qu'une page est mesurée, la bande peut se virtualiser : le rendu
      // apprend le ratio des autres au passage (onLoad -> noterDims).
      const pages = [{ largeur: 720, hauteur: 500 }, {}, {}];
      assert.equal(hauteurRendue(pages[0], 900) !== null, true);
      assert.equal(hauteurRendue(pages[1], 900), null);
      assert.equal(pages.some((p) => hauteurRendue(p, 900) !== null), true);
    });

    it('une bande vide n est pas virtualisable', () => {
      assert.equal(hauteursBande([], 900).length > 0, false);
    });
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
describe('défilement progressif au clavier', () => {
  it('un pas vaut 80 % du cadre, pas une page entière', () => {
    // Mesuré le 2026-10-09 : cadre de 627 px, page de 18 629 px. Un pas calqué
    // sur la page fait un saut ; 80 % du cadre défile.
    assert.equal(pasDeDefilement(627), 502);
    assert.ok(pasDeDefilement(627) < 1000, 'le pas doit rester dans le cadre');
  });

  it('un cadre minuscule ne donne pas un pas décoratif', () => {
    assert.equal(pasDeDefilement(10), PAS_DEFILEMENT_MIN);
    assert.equal(pasDeDefilement(0), PAS_DEFILEMENT_MIN);
  });

  it('le pas est strictement positif et inférieur au cadre', () => {
    for (const h of [0, 40, 200, 627, 1440]) {
      const p = pasDeDefilement(h);
      assert.ok(p > 0, 'pas nul');
      if (h > PAS_DEFILEMENT_MIN) assert.ok(p <= h, 'le pas ne doit pas dépasser le cadre');
    }
  });
});

describe('fenêtre de montage de la bande', () => {
  // 5 pages de 1 000 px : offsets 0, 1000, 2000, 3000, 4000.
  const offsets = [0, 1000, 2000, 3000, 4000];
  const hauteurs = [1000, 1000, 1000, 1000, 1000];

  it('au sommet, la fenêtre couvre le cadre et un peu plus', () => {
    const f = fenetreBande(5, offsets, hauteurs, 0);
    assert.deepEqual(f, [0, 1, 2, 3, 4]);
  });

  it('la dernière page est TOUJOURS montée', () => {
    // C'est la garantie qui supprime le « footer » : sans elle, la queue n'est
    // chargée qu'à l'arrivée, et sa hauteur de repli reste réservée.
    for (const y of [0, 1500, 3000, 4200, 5000, 99999]) {
      const f = fenetreBande(5, offsets, hauteurs, y);
      assert.ok(
        f.includes(4),
        `la dernière page doit être montée pour scrollY=${y}, reçu ${JSON.stringify(f)}`,
      );
    }
  });

  it('la dernière page n est pas montée deux fois', () => {
    const f = fenetreBande(5, offsets, hauteurs, 4200);
    assert.equal(f.filter((i) => i === 4).length, 1);
  });

  it('une ancre au-delà de la bande ne vide pas la fenêtre', () => {
    // `premier` peut valoir `nbPages` quand l'ancre est après la fin de bande :
    // la fenêtre doit alors montrer la queue, pas du vide.
    const f = fenetreBande(5, offsets, hauteurs, 99999);
    assert.ok(f.length > 0, 'la fenêtre ne doit jamais être vide');
    assert.ok(f.includes(4), 'elle doit contenir la dernière page');
  });

  it('au milieu, la fenêtre suit le cadre', () => {
    const f = fenetreBande(20, Array.from({ length: 20 }, (_, i) => i * 1000), Array(20).fill(1000), 5000);
    assert.ok(f.includes(5), 'page sous le cadre');
    assert.ok(f.includes(20 - 1), 'et la dernière page');
    assert.ok(f.length < 20, 'la virtualisation reste active');
  });

  it('une bande vide ne rend pas d indices', () => {
    assert.deepEqual(fenetreBande(0, [], [], 0), []);
  });

  it('une page unique est toujours montée', () => {
    assert.deepEqual(fenetreBande(1, [0], [1000], 0), [0]);
  });
});
