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
  largeurImagePx,
  pasDeDefilement,
  PAS_DEFILEMENT_MIN,
  HAUTEUR_REPLI,
} from './bande-hauteurs.ts';

describe('hauteurs de la bande webtoon', () => {
  it('met la hauteur des dimensions a l\'echelle de la fenetre', () => {
    // Page 800x2171 dans une bande de 400 px de large : 2171 * 400/800.
    assert.equal(hauteurPage({ largeur: 800, hauteur: 2171 }, 400), 1085.5); // ratio exact, pas arrondi
  });

  it('double la hauteur quand on double la largeur de la fenetre', () => {
    const p = { largeur: 800, hauteur: 1600 };
    assert.equal(hauteurPage(p, 400), 800); // ratio exact (2,0) : deja entier
    assert.equal(hauteurPage(p, 800), 1600); // idem, a la double largeur
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
      assert.equal(hauteurRendue({ largeur: 720, hauteur: 500 }, 673), (673 * 500) / 720); // ratio exact : pas d'arrondi
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
      assert.equal(hauteurRendue({ largeur: 720, hauteur: 500 }, 673), (673 * 500) / 720); // ratio exact : pas d'arrondi
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
      assert.equal(rendues[0], (673 * 500) / 720); // ratio exact : pas d'arrondi
      assert.equal(rendues[1], null);
      assert.equal(rendues[2], 1682.5); // ratio exact : 673*500/720
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
      assert.equal(hauteurRendueMesuree({ largeur: 800, hauteur: 800 }, 673, { w: 720, h: 500 }), (673 * 500) / 720); // ratio exact : pas d'arrondi
    });

    it('sans mesure, on retombe sur les dimensions Rust', () => {
      assert.equal(
        hauteurRendueMesuree({ largeur: 800, hauteur: 2000 }, 673, undefined),
        hauteurRendue({ largeur: 800, hauteur: 2000 }, 673),
      );
    });

    it('une mesure de 1x1 est ignoree, pas prise pour un ratio', () => {
      assert.equal(hauteurRendueMesuree({}, 673, { w: 1, h: 1 }), null);
      assert.equal(hauteurRendueMesuree({ largeur: 800, hauteur: 2000 }, 673, { w: 1, h: 1 }), 1682.5); // ratio exact
    });

    it('hauteur de positionnement : la mesure prime aussi', () => {
      const pages = [{}, {}];
      const h = hauteursBande(pages, 673, { 0: { w: 720, h: 500 } });
      assert.equal(h[0], (673 * 500) / 720); // ratio exact : pas d'arrondi
      // La page 1, non mesuree : elle garde le repli, jamais zero.
      assert.ok(h[1] > 0);
      assert.notEqual(h[0], h[1], 'la mesure n a pas ete prise en compte');
    });
  });

  describe('la largeur est arrondie une seule fois, avant les deux calculs', () => {
    // Invariant que l'APPELANT doit respecter : la largeur passée à
    // `hauteursBande` (les offsets) et celle passée à `hauteurRendueMesuree`
    // (le style) doivent être la MÊME valeur entière.
    //
    // Le défaut mesuré le 2026-10-10 : `page.tsx` passait `largeurBande *
    // (zoom / 100)` — non arrondi — aux offsets, et `largeurImagePx(…)` —
    // arrondi — au style. À 1 081 px de large et 50 % de zoom, les offsets
    // valaient 540,5 × h/w et le style 541 × h/w : la boîte et la position de
    // la page suivante ne portaient pas la même valeur, et l'écart
    // s'accumulait sur toute la bande — le « trait entre deux pages » que le
    // bloc ci-dessous dit avoir fermé.
    //
    // Ces fonctions sont pures : un test unitaire ne peut pas voir comment
    // l'appelant les appelle. Il verrouille donc le contrat — « à largeur
    // égale, hauteur égale » — et c'est la porte `pnpm arrondi:check` qui
    // lit l'appel dans `page.tsx`.
    const pages = [
      { largeur: 800, hauteur: 1600 },
      { largeur: 720, hauteur: 1280 },
      { largeur: 1000, hauteur: 2171 },
      {},
    ];
    const mesurees = { 0: { w: 800, h: 1600 }, 2: { w: 1000, h: 2171 } };

    it('offsets et style portent la même hauteur, à largeur entière égale', () => {
      for (const largeurBande of [1081, 977, 843, 1367, 600]) {
        for (const zoom of [50, 75, 80, 90, 100]) {
          const l = largeurImagePx(largeurBande, zoom);
          const hauteurs = hauteursBande(pages, l, mesurees);
          for (let i = 0; i < pages.length; i += 1) {
            const css = hauteurRendueMesuree(pages[i], l, mesurees[i]);
            // Une page sans dimensions n'a pas de hauteur rendue (le repli
            // reste à l'offset) : c'est le seul écart voulu.
            if (css === null) {
              assert.ok(hauteurs[i] > 0, `repli nul pour la page ${i}`);
              continue;
            }
            assert.equal(
              hauteurs[i],
              css,
              `page ${i} : offset ${hauteurs[i]} ≠ style ${css} ` +
                `(largeur ${largeurBande}, zoom ${zoom}%)`,
            );
          }
        }
      }
    });

    it('la largeur non arrondie casserait cet invariant — preuve que le test mord', () => {
      // Contre-épreuve : l'ancien appel passait le produit brut. Le test
      // précédent devient faux, donc il porte bien sur le défaut.
      const largeurBande = 1081;
      const zoom = 50;
      const avecArrondi = hauteursBande(pages, largeurImagePx(largeurBande, zoom), mesurees);
      const sansArrondi = hauteursBande(pages, largeurBande * (zoom / 100), mesurees);
      assert.notEqual(
        avecArrondi[0],
        sansArrondi[0],
        "à 1081 px et 50 %, l'arrondi de la largeur change la hauteur : " +
          'les deux appelants ne pouvaient pas diverger sans conséquence',
      );
    });
  });
});
describe('hauteurs sans arrondi : le trait entre deux pages', () => {
  // Mesuré le 2026-10-09 dans l'app : à 630 px de largeur, une page 720x500
  // mesurait 437,5 px de haut exactement, mais la boîte posait `height: 438px`.
  // `object-fit: contain` centre alors l'image dans la boîte et laisse **0,25 px
  // de fond en haut et en bas** — un trait horizontal fin, **à chaque frontière
  // de page**. L'utilisateur l'a signalé comme « trait toujours présent ».
  //
  // Arrondir la hauteur ne rend pas la géométrie plus propre : cela déplace le
  // demi-pixel **dedans** (lame visible) au lieu de le laisser **entre** les
  // pages. La bonne valeur est la valeur exacte.
  it('la hauteur rendue vaut le ratio exact, sans arrondi', () => {
    const h = hauteurRendue({ largeur: 720, hauteur: 500 }, 630);
    assert.equal(h, 437.5, 'le ratio exact ne doit pas etre arrondi au pixel');
  });

  it('la hauteur de positionnement vaut elle aussi le ratio exact', () => {
    assert.equal(hauteurPage({ largeur: 720, hauteur: 500 }, 630), 437.5);
    assert.equal(hauteurPage({ largeur: 720, hauteur: 700 }, 630), 612.5);
  });

  it('positionnement et rendu donnent la MEME valeur, sinon la lame change de place', () => {
    // C'est l'invariant qui supprime le trait : les deux doivent coïncider au
    // demi-pixel pres, sinon la geometrie ne peut pas etre juste des deux cotes.
    for (const [w, h] of [[720, 500], [720, 700], [720, 297], [800, 2171], [1080, 1620]]) {
      for (const l of [630, 421, 977, 1081, 500]) {
        assert.equal(
          hauteurRendue({ largeur: w, hauteur: h }, l),
          hauteurPage({ largeur: w, hauteur: h }, l),
          `lame possible pour ${w}x${h} a ${l} px de large`,
        );
      }
    }
  });

  it('une page 720x297 a 259,875 px de haut a 630 px de large', () => {
    assert.equal(hauteurRendue({ largeur: 720, hauteur: 297 }, 630), 259.875);
  });

  it('la lame reste nulle sur toute la plage', () => {
    // La lame vaut |boite - ratio exact|. Elle doit etre nulle partout, donc la
    // hauteur rendue doit etre exactement le ratio : on balaie la plage reelle.
    for (const l of [400, 421, 500, 630, 843, 977, 1081, 1400]) {
      for (const [w, h] of [[720, 500], [720, 700], [720, 297], [1080, 1620]]) {
        const rendu = hauteurRendue({ largeur: w, hauteur: h }, l);
        assert.equal(rendu, (l * h) / w, `lame a ${l} px pour ${w}x${h}`);
      }
    }
  });
});

describe('largeur de page : des pixels entiers, jamais un pourcentage', () => {
  // Mesuré le 2026-10-09 : `50 %` d'une largeur impaire vaut 540,5 px, et une
  // image dessinée sur un demi-pixel laisse une colonne translucide à son bord
  // — la ligne verticale signalée, visible seulement en réduisant la fenêtre.
  const cas = [
    [1420, 50, 710],
    [1081, 50, 541],
    [977, 50, 489],
    [843, 50, 422],
    [1081, 100, 1081],
    [1081, 75, 811],
    [1081, 200, 2162],
  ];
  for (const [l, z, attendu] of cas) {
    it(`${l} px a ${z} % -> ${attendu} px (entier)`, () => {
      const r = largeurImagePx(l, z);
      assert.equal(r, attendu);
      assert.ok(Number.isInteger(r), `la largeur doit etre un entier, recu ${r}`);
    });
  }

  it('aucune largeur de la plage 400..1400 ne tombe sur un demi-pixel', () => {
    // Le cas reel n'est pas un point precis : c'est toute la plage. Une largeur
    // fractionnaire est le defaut lui-meme.
    for (const z of [25, 50, 75, 100, 125, 150, 200]) {
      for (let l = 400; l <= 1400; l += 1) {
        const r = largeurImagePx(l, z);
        assert.ok(Number.isInteger(r), `${l} px a ${z} % a donne ${r}`);
      }
    }
  });

  it('une largeur nulle ou negative ne donne pas zero', () => {
    // Une largeur nulle ferait un `width: 0px` : image invisible, et la bande
    // calculee sur une base fausse.
    assert.equal(largeurImagePx(0, 100), 1);
    assert.equal(largeurImagePx(-50, 100), 1);
    assert.equal(largeurImagePx(100, 0), 1);
  });
});

describe('défilement progressif au clavier', () => {
  // Cadre mesuré dans le lecteur réel (2026-10-10) : 683 px.
  const CADRE = 683;
  // Hauteur moyenne d'une page, mesurée le 2026-10-10 sur One Piece ch. 900 :
  // 17 pages pour 18 650 px de bande, soit ~1 097 px. Une page fait donc plus
  // d'une fois et demie le cadre.
  const PAGE = 1097;

  it('un appui ne franchit pas une page', () => {
    // La mesure du jugement : un pas qui dépasse une page EST un saut, quelle
    // que soit sa forme. L'ancien 80 % du cadre valait 546 px — la moitié d'une
    // page — et c'est ce que l'utilisateur a signalé le 2026-10-10 comme
    // « des sauts quand même ».
    const pas = pasDeDefilement(CADRE);
    assert.ok(
      pas < PAGE / 3,
      `un pas de ${pas} px franchit trop d'une page de ${PAGE} px`,
    );
  });

  it('le pas est une fraction du cadre, pas le cadre', () => {
    // 80 % du cadre était un demi-écran : l'œil perd le fil du texte.
    assert.equal(pasDeDefilement(CADRE), Math.round(CADRE / 8));
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

  it('une page se parcourt en un nombre raisonnable d’appuis', () => {
    // C'est la répétition qui donne l'impression de continu, donc le critère
    // n'est pas « dix appuis = une page » (mesuré : 10 x 85 = 850 px, une page
    // en fait 1 097) mais « une page ne demande pas une rafale interminable ».
    // Au-delà de 20 appuis, la touche paraitrait morte et l'utilisateur
    // reviendrait demander le saut qu'on vient de retirer.
    const appuisParPage = Math.ceil(PAGE / pasDeDefilement(CADRE));
    assert.ok(appuisParPage <= 15, `${appuisParPage} appuis pour une page, trop`);
    assert.ok(appuisParPage >= 4, `${appuisParPage} appuis pour une page, c'est redevenu un saut`);
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

  it('la fenetre monte assez loin DEVANT pour mesurer avant le haut du cadre', () => {
    // Mesuré le 2026-10-09 : avec une portée de 2 000 px (~3 pages), les pages
    // étaient encore non mesurées quand elles arrivaient en haut du cadre, et
    // la correction d'ancrage reculait alors scrollTop de 507 px d'un coup.
    const pages = 200;
    const hauteurs = Array(pages).fill(613);
    const offsets = hauteurs.map((_, i) => i * 613);
    const f = fenetreBande(pages, offsets, hauteurs, 40 * 613);
    const premiereSousCadre = 41;
    const montees = f.filter((i) => i >= premiereSousCadre);
    // Compté sur la mesure : avec la portée de 2 000 px, la fenêtre montait
    // 5 pages devant le cadre ; avec 4 200 px, 8. Le seuil est à 7 pour que le
    // test **échoue** sur l'ancienne valeur — vérifié.
    assert.ok(
      montees.length >= 7,
      `seulement ${montees.length} pages montees devant le cadre (recu ${JSON.stringify(montees)})`,
    );
  });

  it('une portee plus large ne monte pas toute la bande', () => {
    const pages = 300;
    const hauteurs = Array(pages).fill(613);
    const offsets = hauteurs.map((_, i) => i * 613);
    const f = fenetreBande(pages, offsets, hauteurs, 0);
    assert.ok(f.length < 15, `la virtualisation tient : ${f.length} pages montees`);
  });
});
