# 0018 — Identité WebView pour le rejeu de challenge
- Date : 2026-10-13
- Statut : acceptée

## Contexte

La fenêtre WebView2 qui résout Cloudflare transmet déjà ses cookies et son
`User-Agent` au moteur. Pour les challenges liés à une signature navigateur,
l'UA seul ne décrit pas l'identité réellement utilisée par la fenêtre.

## Décision

Étendre `setCookies` de manière additive avec un `browserProfile` optionnel.
Rust lit les Client Hints bas et haute entropie de la fenêtre, ainsi que
`navigator.language` et `navigator.languages`, puis transmet le profil dans le
même message que la clearance. Le moteur le conserve en mémoire sous le même
domaine normalisé que `cf_clearance`. Les anciens appelants restent valides ;
la version majeure du protocole ne change pas.

Les valeurs de `User-Agent` et de Client Hints proviennent exclusivement de la
WebView qui a résolu le défi. Le profil n'est ni persisté ni journalisé. La
reconstruction des en-têtes n'est activée que lorsqu'une requête porte une
`cf_clearance`.

## Alternatives étudiées

- **Message distinct `setBrowserIdentity`** : rejeté, car cookies et profil
  décrivent la même résolution, doivent partager le même domaine et seraient
  autrement sujets à un ordre d'application incorrect.
- **Garder uniquement l'UA** : rejeté, car il ne transporte pas les Client
  Hints que Chromium expose pour sa propre identité.

## Conséquences

- L'ajout de champs optionnels reste compatible avec les moteurs existants du
  même protocole majeur.
- Le profil disparaît avec le processus JVM ; aucune donnée de navigateur
  supplémentaire n'est persistée.
- Les hôtes sans clearance conservent leur chemin d'en-têtes existant.
