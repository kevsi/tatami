# 0008 — Stratégie de traduction
- Date : 2026-10-06
- Statut : acceptée

## Contexte

T0.5 a comparé trois moteurs de traduction EN→FR sur 12 bulles de webtoon :

- **opus-mt** (`Infomaniak-AI/onnx-opus-mt-en-fr`, Apache-2.0) : hors ligne, 214 ms/bulle sur la machine cible, lisible par bulle entière.
- **NLLB-200** (`facebook/nllb-200-distilled-600M`, CC-BY-NC) : équivalent à opus-mt, pas meilleur, exclu pour la redistribution.
- **Gemini 3.5** (API en ligne) : qualité nettement supérieure, budget de caractères tenu, mais quota gratuit limité (~14 appels/jour sur une clé récente).

Le vrai fossé n'est pas entre les modèles classiques, c'est entre eux et le LLM.

## Décision

**Hybride : Gemini en ligne par défaut, opus-mt en repli hors ligne.**

1. L'utilisateur fournit sa propre clé Gemini dans les paramètres.
2. La traduction en ligne est tentée en premier (qualité maximale).
3. En cas d'échec (quota épuisé, réseau, clé absente), la traduction bascule sur opus-mt hors ligne (qualité acceptable, zéro coût, zéro dépendance externe).
4. L'utilisateur voit toujours l'original en attendant la traduction (affichage asynchrone et progressif, conforme au principe T).

### Ce qui n'est PAS retenu

- **Gemini seul** : le quota gratuit peut s'épuiser ; l'utilisateur se retrouve sans traduction.
- **opus-mt seul** : qualité mesurée insuffisante pour des bulles de webtoon (contresens, mots tombés).
- **NLLB-200** : CC-BY-NC incompatible avec la redistribution GPLv3 de l'installeur, et qualité ≈ opus-mt.

## Conséquences

- Dépendance à un service tiers (Google) pour la qualité maximale, mais optionnelle.
- Clé stockée dans la base de l'application (SQLite) : réévaluer pour le trousseau de l'OS avant la 1.0.
- opus-mt livré en pack de modèles (comme le moteur de sources) : téléchargement vérifié, hash, remplacement atomique.
- Latence : Gemini ~1-6 s selon le réseau ; opus-mt 214 ms mesuré sur la machine cible.
