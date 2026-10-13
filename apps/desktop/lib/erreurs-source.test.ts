import { test } from "node:test";
import assert from "node:assert/strict";
import { describeError, estRejeuRefuse } from "./erreurs-source.ts";

// Un rejeu refusé doit être render comme tel, et surtout **pas** comme une
// demande de verification : sinon le bandeau propose de rouvrir une fenetre qui
// ne peut qu'echouer (cycle mesure le 2026-10-11 sur HENTAIZONE).

/** Faux traducteur : renvoie la cle, donc on observe le CLASSEMENT. */
const t = (key: string, vars?: Record<string, string | number>) =>
  vars?.source ? `${key}:${String(vars.source)}` : key;

test("un rejeu refuse est reconnu", () => {
  assert.equal(
    estRejeuRefuse(
      "CHALLENGE_UNREPLAYABLE: challenge at https://x.fr (non rejouable)",
    ),
    true,
  );
});

test("une demande de verification ordinaire n est PAS un rejeu refuse", () => {
  assert.equal(
    estRejeuRefuse("CHALLENGE_REQUIRED: challenge at https://x.fr"),
    false,
  );
});

test("le rejeu refuse est classe avant la verification ordinaire", () => {
  // Les deux chaines partagent leur debut : sans l'ordre des tests, la seconde
  // absorberait la premiere et le message promettrait une action impossible.
  const message = describeError(
    "CHALLENGE_UNREPLAYABLE: challenge at https://x.fr (non rejouable)",
    "HENTAIZONE",
    t,
  );
  assert.equal(message, "erreur.verification.non.rejouable:HENTAIZONE");
});

test("une verification ordinaire garde son message", () => {
  const message = describeError(
    "CHALLENGE_REQUIRED: challenge at https://x.fr",
    "TOONFR",
    t,
  );
  assert.equal(message, "erreur.verification:TOONFR");
});

test("le message du rejeu refuse ne promet aucun reessai", () => {
  // `erreur.verification` dit « pas encore geree » : c'est un mensonge ici,
  // parce que rien ne peut reussir. La distinction doit rester dans les cles.
  const rejeter = describeError("CHALLENGE_UNREPLAYABLE: x", "X", t);
  const demander = describeError("CHALLENGE_REQUIRED: x", "X", t);
  assert.notEqual(
    rejeter,
    demander,
    "les deux cas doivent avoir des messages distincts",
  );
});

// --- Codes du moteur, ajoutes le 2026-10-12 ------------------------------
//
// Mesure : 83 des 98 sources FR echouaient avec une exception Java brute.
// `HttpStatusException: . Status=523` ne dit pas a l'utilisateur si son reseau
// est en cause, si la source est morte, ou si le programme est perime.

test("un code SOURCE_DEAD donne un message dedie", () => {
  // Message reel mesure sur ANIMESAMA.
  assert.equal(
    describeError(
      "SOURCE_DEAD: UnknownHostException: Hote inconnu (anime-sama.eu)",
      "ANIMESAMA",
      t,
    ),
    "erreur.source.morte:ANIMESAMA",
  );
});

test("un code ORIGIN_DOWN est distingue de SOURCE_DEAD", () => {
  // Les deux disent « indisponible », mais seul ORIGIN_DOWN peut revenir : le
  // domaine existe et le serveur sera rallume. Les confondre decourageait de
  // reessayer. Message reel mesure sur VINLANDSAGA.
  const origine = describeError(
    "ORIGIN_DOWN: HttpStatusException: . Status=523, URL=[https://vinlandsaga.fr/]",
    "VINLANDSAGA",
    t,
  );
  assert.equal(origine, "erreur.origine.injoignable:VINLANDSAGA");
  assert.notEqual(origine, "erreur.source.morte:VINLANDSAGA");
});

test("un code PARSER_OUTDATED donne un message dedie", () => {
  // Message reel mesure sur SCANTRADVF.
  assert.equal(
    describeError(
      "PARSER_OUTDATED: ParseException: Root not found at https://qemow.com/go/x",
      "SCANTRADVF",
      t,
    ),
    "erreur.parseur.perime:SCANTRADVF",
  );
});

test("les codes du moteur passent AVANT les heuristiques de message", () => {
  // Sans cet ordre, `Status=523` serait rattrape par la regle des 5xx et
  // retomberait sur le message generique : le typage serait sans effet.
  const brut =
    "ORIGIN_DOWN: HttpStatusException: . Status=523, URL=[https://x.fr/]";
  assert.equal(describeError(brut, "S", t), "erreur.origine.injoignable:S");
});

test("le reste du classement n a pas bouge", () => {
  assert.equal(describeError("TIMEOUT: 60s", "S", t), "erreur.delai:S");
  assert.equal(
    describeError("AUTH_REQUIRED: compte", "S", t),
    "erreur.compte.requis:S",
  );
  assert.equal(
    describeError("RATE_LIMITED: trop", "S", t),
    "erreur.surcharge:S",
  );
  assert.equal(describeError("403Forbidden", "S", t), "erreur.403:S");
});
