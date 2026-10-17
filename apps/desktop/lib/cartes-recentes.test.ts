import { test } from "node:test";
import assert from "node:assert/strict";
import { cartesAFaire, entreesFraiches, type EntreeRecent } from "./cartes-recentes.ts";

// Mesure du 2026-10-17 : 423 appels `chapters` en 13 min pour 223 series, sur un
// moteur a `MAX_WORKERS = 4`. La borne testee ici est ce qui l'empeche de
// saturer le moteur : **une carte hors fenetre n'est jamais demandee**.

const TTL = 30 * 60 * 1000;
const MAINTENANT = 1_700_000_000_000;

const carte = (key: string) => ({ key, titre: `titre ${key}` });
const entree = (at: number): EntreeRecent => ({ total: 42, at });

test("une carte hors fenetre n'est jamais demandee", () => {
  // Le rouge de ce test est la borne elle-meme : sans le filtre de fenetre,
  // les 5 cartes seraient demandees et la marche saturerait les 4 travailleurs.
  const cartes = [carte("a"), carte("b"), carte("c"), carte("d"), carte("e")];
  const faites = cartesAFaire(cartes, new Set(["b", "d"]), {}, new Set(), MAINTENANT, TTL);
  assert.deepEqual(
    faites.map((c) => c.key),
    ["b", "d"],
  );
});

test("une fenetre vide ne demande rien, elle ne demande pas tout", () => {
  // L'inverse : une fenetre lue avant que la grille soit rendue ne doit pas
  // retomber sur « tout le catalogue » — c'est exactement le defaut mesure.
  const cartes = [carte("a"), carte("b")];
  assert.deepEqual(cartesAFaire(cartes, new Set(), {}, new Set(), MAINTENANT, TTL), []);
});

test("une carte sans cache est demandee, une carte fraiche ne l'est pas", () => {
  const cartes = [carte("a"), carte("b")];
  const cache = { a: entree(MAINTENANT - 60_000) };
  const faites = cartesAFaire(cartes, new Set(["a", "b"]), cache, new Set(), MAINTENANT, TTL);
  assert.deepEqual(
    faites.map((c) => c.key),
    ["b"],
  );
});

test("une carte dont le cache a exactement l'age du TTL est encore fraiche", () => {
  // La frontiere est une decision : `<=` garde l'entree a l'instant pile du
  // TTL. La deplacer d'une milliseconde doit se voir ici.
  const cartes = [carte("a"), carte("b")];
  const cache = { a: entree(MAINTENANT - TTL), b: entree(MAINTENANT - TTL - 1) };
  const faites = cartesAFaire(cartes, new Set(["a", "b"]), cache, new Set(), MAINTENANT, TTL);
  assert.deepEqual(
    faites.map((c) => c.key),
    ["b"],
  );
});

test("une carte deja en vol n'est pas redemandee", () => {
  // Depuis que la marche est relancee a chaque changement de fenetre, deux
  // executions se chevauchent : sans ce filtre, la meme carte partirait deux fois.
  const cartes = [carte("a"), carte("b")];
  const faites = cartesAFaire(cartes, new Set(["a", "b"]), {}, new Set(["a"]), MAINTENANT, TTL);
  assert.deepEqual(
    faites.map((c) => c.key),
    ["b"],
  );
});

test("une entree de cache sans date est traitee comme expiree", () => {
  // `localStorage` peut porter n'importe quoi : une entree illisible ne doit
  // pas figer une carte sans pastille pour toujours.
  const cartes = [carte("a")];
  const cache = { a: { total: 0 } as unknown as EntreeRecent };
  assert.deepEqual(
    cartesAFaire(cartes, new Set(["a"]), cache, new Set(), MAINTENANT, TTL).map((c) => c.key),
    ["a"],
  );
});

test("les entrees fraiches sont reportees a l'affichage, meme hors fenetre", () => {
  // L'asymetrie est voulue : reporter une entree deja calculee ne coute aucune
  // requete, et une carte revenue a l'ecran doit afficher son compte tout de suite.
  const cartes = [carte("a"), carte("b")];
  const cache = { a: entree(MAINTENANT - 1_000), b: entree(MAINTENANT - TTL - 1) };
  const fraiches = entreesFraiches(cartes, cache, MAINTENANT, TTL);
  assert.deepEqual(Object.keys(fraiches), ["a"]);
  assert.equal(fraiches.a.total, 42);
});
