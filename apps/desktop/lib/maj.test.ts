import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  INTERVALLE_VERIF_MS,
  codeErreurMaj,
  doitVerifierAuto,
  horodatage,
  phaseMaj,
  pourcentRecu,
} from './maj.ts';

// Ces tests verrouillent trois décisions qui ne se voient pas à l'écran :
// la fréquence de la requête réseau, la priorité des phases affichées et le
// traitement d'une charge utile qui n'a pas la forme attendue.

test('la vérification automatique respecte la préférence', () => {
  assert.equal(doitVerifierAuto({ actif: false, dernier: null, maintenant: 1_000 }), false);
  assert.equal(doitVerifierAuto({ actif: true, dernier: null, maintenant: 1_000 }), true);
});

test('la borne des 24 h est inclusive, pas approximative', () => {
  const maintenant = 10 * INTERVALLE_VERIF_MS;
  assert.equal(
    doitVerifierAuto({ actif: true, dernier: maintenant - INTERVALLE_VERIF_MS + 1, maintenant }),
    false,
    'une milliseconde avant la borne, on ne requête pas',
  );
  assert.equal(
    doitVerifierAuto({ actif: true, dernier: maintenant - INTERVALLE_VERIF_MS, maintenant }),
    true,
    'exactement 24 h après, on requête',
  );
});

test('une date stockée dans le futur ne bloque pas les vérifications', () => {
  assert.equal(
    doitVerifierAuto({ actif: true, dernier: 5_000, maintenant: 1_000 }),
    true,
    'horloge reculée : la valeur stockée est périmée, on revérifie',
  );
});

test('un horodatage illisible vaut « jamais vérifié »', () => {
  assert.equal(horodatage(null), null);
  assert.equal(horodatage(''), null);
  assert.equal(horodatage('bientôt'), null);
  assert.equal(horodatage('0'), null);
  assert.equal(horodatage('-5'), null);
  assert.equal(horodatage('1729420000000'), 1_729_420_000_000);
});

test("une erreur masque une progression qui n'avance plus", () => {
  assert.equal(
    phaseMaj({
      lue: true,
      occupe: 'installation',
      disponible: true,
      progression: true,
      erreur: true,
    }),
    'erreur',
  );
});

test("la progression l'emporte sur la vérification et sur « disponible »", () => {
  assert.equal(
    phaseMaj({
      lue: true,
      occupe: 'installation',
      disponible: true,
      progression: true,
      erreur: false,
    }),
    'telechargement',
  );
  assert.equal(
    phaseMaj({
      lue: true,
      occupe: 'verification',
      disponible: true,
      progression: false,
      erreur: false,
    }),
    'verification',
  );
});

test('sans vérification aboutie, « aucune mise à jour » ne se prétend pas « à jour »', () => {
  assert.equal(
    phaseMaj({
      lue: false,
      occupe: null,
      disponible: false,
      progression: false,
      erreur: false,
    }),
    'inconnu',
  );
  assert.equal(
    phaseMaj({
      lue: true,
      occupe: null,
      disponible: false,
      progression: false,
      erreur: false,
    }),
    'a_jour',
  );
  assert.equal(
    phaseMaj({
      lue: true,
      occupe: null,
      disponible: true,
      progression: false,
      erreur: false,
    }),
    'disponible',
  );
});

test('le pourcentage est borné et refuse un total inconnu', () => {
  assert.equal(pourcentRecu(50, null), null, 'total absent : pas de barre');
  assert.equal(pourcentRecu(50, 0), null, 'total nul : division impossible');
  assert.equal(pourcentRecu(1, 3), 33);
  assert.equal(pourcentRecu(3, 3), 100);
  assert.equal(pourcentRecu(4, 3), 100, 'un total annoncé trop bas ne déborde pas');
  assert.equal(pourcentRecu(-1, 3), null, 'un reçu négatif est refusé');
});

test("un code inconnu retombe sur « inconnu » au lieu d'afficher undefined", () => {
  assert.equal(codeErreurMaj({ code: 'reseau' }), 'reseau');
  assert.equal(codeErreurMaj({ code: 'signature' }), 'signature');
  assert.equal(codeErreurMaj({ code: 'pirate' }), 'inconnu');
  assert.equal(codeErreurMaj('espace'), 'espace', 'un rejet en chaîne reste lisible');
  assert.equal(codeErreurMaj(undefined), 'inconnu');
  assert.equal(codeErreurMaj(null), 'inconnu');
});
