import { describe, it } from 'node:test';
import assert from 'node:assert/strict';
import { fusionnerPremierePage, nouvellesEntrees } from './catalogue-pagination.ts';

type Fiche = { source: string; url: string; titre: string };

const cle = (m: Fiche) => `${m.source}|${m.url}`;
const fiche = (n: number, source = 'S'): Fiche => ({
  source,
  url: `https://exemple.test/${n}`,
  titre: `Fiche ${n}`,
});

describe('pagination du catalogue', () => {
  it('n’ajoute pas une page que le moteur a renvoyée deux fois', () => {
    // Cas mesuré : custom:lire-manga renvoie 43 fiches identiques quel que
    // soit l'`offset`. La seconde page n'apporte rien.
    const page1 = [fiche(1), fiche(2), fiche(3)];
    assert.deepEqual(nouvellesEntrees(page1, [...page1], cle), []);
  });

  it('ajoute les fiches réellement nouvelles', () => {
    const page1 = [fiche(1), fiche(2)];
    const page2 = [fiche(2), fiche(3)];
    assert.deepEqual(
      nouvellesEntrees(page1, page2, cle).map((m) => m.url),
      [fiche(3).url],
    );
  });

  it('ne réduit pas une grille paginée quand la page 1 est revalidée', () => {
    // Le rafraîchissement d'arrière-plan ne porte que la page 1 : la grille
    // de 6 fiches doit rester à 6, dans le même ordre.
    const page1 = [fiche(1), fiche(2), fiche(3), fiche(4)];
    const page2 = [fiche(5), fiche(6)];
    const grille = [...page1, ...nouvellesEntrees(page1, page2, cle)];
    const apres = fusionnerPremierePage(grille, [fiche(1), fiche(2), fiche(3)], cle);
    assert.deepEqual(apres.map((m) => m.url), grille.map((m) => m.url));
  });

  it('laisse la page fraîche remplacer une grille plus courte', () => {
    const grille = [fiche(1), fiche(2)];
    const fraiche = [fiche(1), fiche(2), fiche(3), fiche(4)];
    assert.deepEqual(
      fusionnerPremierePage(grille, fraiche, cle).map((m) => m.url),
      fraiche.map((m) => m.url),
    );
  });
});