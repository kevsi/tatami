import { test } from 'node:test';
import assert from 'node:assert/strict';
import { reconcilierListeBlanche } from './liste-blanche.ts';

// Le défaut mesuré le 2026-10-10 : `enabledSources` est une liste FERMÉE écrite
// par « Tout activer ». Toute source apparue ensuite — un script importé, une
// source neuve d'une mise à jour — porte un nom absent de la liste, donc
// n'est jamais affichée. Le moteur la sert (1373), l'écran en montre 1372.
//
// Réconcilier « naïvement » (ajouter tous les noms manquants)reactiverait les
// sources que l'utilisateur a désactivées : un nom absent de la liste EST la
// définition d'une source désactivée. D'où la deuxième clé, `seen`.

test('migration : une liste existante ne réactive rien', () => {
  const r = reconcilierListeBlanche({
    nomsConnus: ['A', 'B', 'C'],
    enabled: ['A', 'C'], // B a été désactivée par l'utilisateur
    seen: null, // première exécution de la version migrée
  });
  assert.deepEqual(r.enabled, ['A', 'C'], 'B ne doit pas revenir');
  assert.deepEqual(r.nouveaux, []);
  assert.deepEqual(r.seen.sort(), ['A', 'B', 'C'], 'tout ce qui est connu passe dans seen');
  assert.equal(r.migration, true, 'c’est la première fois, donc rien n’est nouveau');
});

test('après la migration, une source neuve est activée', () => {
  const r = reconcilierListeBlanche({
    nomsConnus: ['A', 'B', 'C', 'NEUVE'],
    enabled: ['A', 'B', 'C'],
    seen: ['A', 'B', 'C'],
  });
  assert.deepEqual(r.nouveaux, ['NEUVE']);
  assert.deepEqual(r.enabled, ['A', 'B', 'C', 'NEUVE']);
});

test('une source désactivée puis re-parue ne revient pas', () => {
  // B est désactivée, donc absente d'`enabled`, mais elle a déjà été vue.
  const r = reconcilierListeBlanche({
    nomsConnus: ['A', 'B'],
    enabled: ['A'],
    seen: ['A', 'B'],
  });
  assert.deepEqual(r.nouveaux, [], 'B n’est pas nouvelle, elle a déjà été vue');
  assert.deepEqual(r.enabled, ['A'], 'et elle reste désactivée');
});

test('sans liste blanche, il n’y a rien à réconcilier', () => {
  const r = reconcilierListeBlanche({
    nomsConnus: ['A', 'NEUVE'],
    enabled: null,
    seen: null,
  });
  assert.equal(r.enabled, null, 'null = tout est activé, on n’y touche pas');
  assert.deepEqual(r.nouveaux, []);
});

test('deux exécutions de suite n’ajoutent rien la deuxième fois', () => {
  const premier = reconcilierListeBlanche({
    nomsConnus: ['A', 'NEUVE'],
    enabled: ['A'],
    seen: ['A'],
  });
  const second = reconcilierListeBlanche({
    nomsConnus: ['A', 'NEUVE'],
    enabled: premier.enabled,
    seen: premier.seen,
  });
  assert.deepEqual(second.nouveaux, [], 'la réconciliation doit être idempotente');
  assert.deepEqual(second.enabled, premier.enabled);
});

test('« tout activer » écrit déjà la liste complète : la réconciliation ne casse rien', () => {
  // « Tout activer » écrit tous les noms connus ; `seen` reste en retard d'un
  // nom. Ce nom est donc « nouveau » pour `seen` mais DÉJÀ dans `enabled` :
  // il ne doit pas être compté comme réactivé, sinon le journal dirait « une
  // source réactivée » alors que rien n'a bougé.
  const r = reconcilierListeBlanche({
    nomsConnus: ['A', 'B', 'NEUVE'],
    enabled: ['A', 'B', 'NEUVE'], // déjà là
    seen: ['A', 'B'],
  });
  assert.deepEqual(r.enabled, ['A', 'B', 'NEUVE']);
  assert.deepEqual(r.nouveaux, [], 'rien n’a été réactivé : NEUVE était déjà active');
  assert.deepEqual(r.seen.sort(), ['A', 'B', 'NEUVE']);
});

test('une source retirée du catalogue ne sort pas de seen', () => {
  // `seen` doit contenir l'UNIVERS connu, pas la liste courante : sinon le
  // retour d'une source retirée passerait pour une nouvelle et serait
  // réactivée malgré la désactivation de l'utilisateur.
  const r = reconcilierListeBlanche({
    nomsConnus: ['A'],
    enabled: ['A'],
    seen: ['A', 'PARTIE'],
  });
  assert.ok(r.seen.includes('PARTIE'), 'seen garde ce qui a disparu');
  assert.deepEqual(r.enabled, ['A'], 'mais la liste active ne le rajoute pas');
});
