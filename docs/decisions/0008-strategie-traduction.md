# 0008 — Stratégie de traduction : non tranchée en T0

- Date : 2026-10-06
- Statut : **proposée — volontairement incomplète**, à trancher après T0.5

## Contexte

Le plan (`PLAN-TRADUCTION.md` §4) pose la question : « hors ligne seul, LLM en
ligne seul, ou les deux », avec une tendance affichée pour « les deux, le mode
hors ligne conditionné par la qualité mesurée ».

Cette condition est précisément ce que T0 n'a pas pu mesurer. Le critère de
sortie de T0.5 demande **au moins deux LLM en ligne et un modèle hors ligne**,
avec et sans glossaire et avec et sans budget de caractères.

## Décision

**Aucune décision prise. La question reste ouverte.**

Il est volontairement **explicitement non tranché** de ne pas trancher. Aucun
des deux modes ne peut être écarté ni retenu aujourd'hui, et le dire serait
inventer un chiffre : la métrique S8 (qualité ≥ 3,5/5 sur trois axes) n'a pas
été calculée **une seule fois**, faute de corpus.

Ce qui est acquis, et qui réduit le champ :

| Acquis | Conséquence |
|---|---|
| **NLLB-200 est CC-BY-NC** → exclu de toute redistribution | Le premier modèle hors ligne cité par la littérature est écarté pour nous |
| opus-mt en-fr, M2M100, XGLM, Qwen 0.5B/1.5B : licences **permissives** (Apache-2.0, MIT) | Le hors ligne **reste possible** sans problème de licence |
| Aucune clé d'API n'est requise pour un mode hors ligne | L'option « connexion limitée » du §1 du plan reste atteignable |

## Pourquoi ne pas trancher maintenant

Trois raisons, toutes deux fois vérifiées dans cette session :

1. **Aucune donnée de qualité.** Pas de corpus, donc aucun score de fidélité,
   de naturel ou de ton. Comparer « hors ligne » et « en ligne » sans mesure
   reviendrait à trancher par intuition — ce qu'interdit `AGENTS.md` §2.3.
2. **Aucun coût en ligne mesuré.** Sans clé, ni le prix par page, ni la latence,
   ni la qualité. Le mode en ligne a des avantages **structurels** (qualité
   bien supérieure sur du texte de bubble court, glossaire naturel) que je ne
   peux pas quantifier ici.
3. **Le glossaire n'a jamais été testé.** T3.3 le prévoit, mais son effet sur
   la qualité — le principal argument du mode en ligne — est **inconnu**. Un
   glossaire rend le mode local beaucoup plus attractif ; ne pas le mesurer,
   c'est laisser la décision se prendre sur le mauvais critère.

## Ce qu'il faudra pour trancher

1. Un **corpus** (bloc 1 du rapport de spike) et l'annotation.
2. **Deux clés d'API** dans le trousseau ou une variable d'environnement
   (bloc 2) — jamais dans le dépôt ni dans un journal.
3. La comparaison croisée : 2 en ligne + 1 hors ligne, avec et sans glossaire,
   avec et sans budget de caractères, notés sur les trois axes de S8.

## Critère de décision proposé

À appliquer **après** les mesures, non avant :

- si le meilleur hors ligne atteint **S8** (≥ 3,5/5 sur chacun des trois axes)
  → **les deux**, le hors ligne par défaut (pas de clé, confidentialité) ;
- sinon → **en ligne par défaut**, hors ligne **désactivé proprement** plutôt que
  proposé et décevant. Une option hors ligne médiocre est pire que pas
  d'option : elle fait perdre confiance au reste.

## Réserve de sécurité

Dans les deux cas : **le texte source n'est envoyé à un fournisseur qu'avec un
consentement explicite** (T3.6), et l'image ne part jamais par défaut. Le
trousseau de l'OS porte la clé ; rien ne va dans SQLite ni dans les journaux.