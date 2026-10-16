import { test } from "node:test";
import assert from "node:assert/strict";
import { chapitresSansDoublons } from "./chapitres-doublons.ts";

// Mesure du 2026-10-15 (audit `docs/audit-ux-2026-10-15.md`, F6) : sur la fiche
// « One Piece » (SCANVF), la source rend des URL de chapitre en double, donc
// 40 a 100 avertissements `Encountered two children with the same key`. La cle
// React n'est qu'un symptome : l'utilisateur voit aussi les doublons, et deux
// lignes de meme URL sont indiscernables pour lui (la case, le telechargement et
// le signet sont tous indexes sur l'URL).

test("une liste sans doublon reste identique", () => {
  // Le cas nominal : une source sage ne doit subir aucune transformation, sinon
  // le test suivant prouverait un code qui casse le cas courant.
  const chapitres = [
    { url: "https://x.fr/ch-1", number: 1 },
    { url: "https://x.fr/ch-2", number: 2 },
  ];
  assert.deepEqual(chapitresSansDoublons(chapitres), chapitres);
});

test("le premier passage d une URL reste visible", () => {
  // L'ordre est preserve : la liste ne doit pas sauter sous les doigts de
  // l'utilisateur, et le chapitre garde le premier nom donne par la source.
  const chapitres = [
    { url: "https://x.fr/ch-1", name: "Ch. 1" },
    { url: "https://x.fr/ch-1", name: "Ch. 1 (bis)" },
  ];
  const uniques = chapitresSansDoublons(chapitres);
  assert.equal(uniques.length, 1);
  assert.equal(uniques[0].name, "Ch. 1");
});

test("le doublon au milieu disparait, les autres restent dans l ordre", () => {
  // Forme reelle mesuree sur SCANVF : le doublon n'est ni en tete ni en fin,
  // donc un test qui n'en verifierait qu'un extreme passerait pour rien.
  const chapitres = [
    { url: "https://x.fr/ch-1" },
    { url: "https://x.fr/ch-2" },
    { url: "https://x.fr/ch-2" },
    { url: "https://x.fr/ch-3" },
  ];
  assert.deepEqual(
    chapitresSansDoublons(chapitres).map((c) => c.url),
    ["https://x.fr/ch-1", "https://x.fr/ch-2", "https://x.fr/ch-3"],
  );
});

test("trois passages de la meme URL rendent un seul chapitre", () => {
  // L'audit relevait « 40 a 100 avertissements selon les rendus » : le nombre
  // de repetitions n'est pas borne a deux.
  const chapitres = Array.from({ length: 3 }, () => ({ url: "https://x.fr/ch-9" }));
  assert.equal(chapitresSansDoublons(chapitres).length, 1);
});

test("une liste vide reste vide", () => {
  assert.deepEqual(chapitresSansDoublons([]), []);
});

test("deux URL differentes ne sont jamais confondues", () => {
  // Le piege inverse : une deduplication trop large fusionnerait deux chapitres
  // reels. Le test inverse de la deduplication, indispensable ici.
  const chapitres = [
    { url: "https://x.fr/ch-1" },
    { url: "https://x.fr/ch-10" },
  ];
  assert.equal(chapitresSansDoublons(chapitres).length, 2);
});