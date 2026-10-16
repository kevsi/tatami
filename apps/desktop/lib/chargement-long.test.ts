import { test } from "node:test";
import assert from "node:assert/strict";
import { attenteDepasseLeSeuil, SEUIL_CHARGEMENT_MS } from "./chargement-long.ts";

// Mesure du 2026-10-16 : 60 s de squelettes sans bandeau ni notification pendant
// qu'une webview attendait une vérification Cloudflare (F3 + F13). Le seuil est
// une **décision** : 8 s, soit trois fois le pire temps de chargement connu.

test("le seuil est trois fois le pire temps de chargement mesure", () => {
  // Si quelqu'un change SEUIL, ce test doit le dire : la justification est
  // « 3 x 2,3 s », pas « ça paraissait bien ».
  assert.equal(SEUIL_CHARGEMENT_MS, 8000);
});

test("une attente courte ne declenche rien", () => {
  // Le cas nominal : le catalogue met 1,3 s a froid, 2,3 s a chaud. Ne rien
  // afficher dans ce cas est le comportement correct, pas une erreur.
  assert.equal(attenteDepasseLeSeuil(0), false);
  assert.equal(attenteDepasseLeSeuil(1300), false);
  assert.equal(attenteDepasseLeSeuil(2300), false);
});

test("une attente juste sous le seuil ne declenche rien", () => {
  // La frontiere compte : une grille qui clignote a 7,999 s puis annonce « lent »
  // a 8,000 s serait un message qui scintille.
  assert.equal(attenteDepasseLeSeuil(SEUIL_CHARGEMENT_MS - 1), false);
});

test("une attente au-dela du seuil declenche", () => {
  assert.equal(attenteDepasseLeSeuil(SEUIL_CHARGEMENT_MS), true);
  assert.equal(attenteDepasseLeSeuil(60000), true);
});

test("le seuil est configurable, et une valeur absurde reste respectee", () => {
  // Le test inverse : une fonction qui ignorerait son parametre passerait tous
  // les tests precedents, puisque le seuil par defaut conviendrait a tout.
  assert.equal(attenteDepasseLeSeuil(100, 50), true);
  assert.equal(attenteDepasseLeSeuil(100, 200), false);
});