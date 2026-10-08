# 0011 — Politique des textes hors bulles (sfx, texte libre)
- Date : 2026-10-08
- Statut : acceptée

## Contexte

Lors de la traduction d'une page, le pipeline de vision détecte des blocs de texte
de plusieurs types : dialogue, narration, sfx (sound effects), texte libre. Les
bulles de dialogue et de narration sont effacées et remplacées par la traduction.
Les textes hors bulles (sfx, texte libre, crédits) nécessitent une politique
explicite : les effacer comme les bulles poserait souvent problème (les sfx sont
directement liés au dessin, les crédits sont des mentions légales), mais les
conserver sans aucune indication serait incohérent si leur contenu est traduit.

## Décision

Le texte hors bulles (sfx, texte libre, ignore) **n'est pas effacé**. Il est
affiché tel quel sur la page, avec éventuellement une traduction en légende
superposée discrètement (un tooltip ou un petit texte sous-jacent, au choix de
l'implémentation de la webview).

Concrètement :
- `TypeBloc::Sfx` et `TypeBloc::TexteLibre` : l'effacement est refusé. Le texte
  original reste visible. La traduction est disponible mais n'est pas
  superposée par défaut.
- `TypeBloc::Ignore` : ni effacé, ni traduit, ni affiché en légende.
- `TypeBloc::Dialogue` et `TypeBloc::Narration` : effacé et remplacé par la
  traduction (comportement standard).

## Alternatives étudiées

1. **Effacer aussi les sfx et les remplacer** — rejeté : les sfx sont souvent
   intégrés au dessin (lettrage onomatopéique partiellement fusionné avec les
   lignes de vitesse ou le décor). Les effacer créerait des artefacts visibles.
2. **Ignorer complètement les textes hors bulles** — rejeté : l'utilisateur
   serait frustré de ne pas comprendre un sfx ou une étiquette importante.
3. **Remplacer les sfx par la traduction comme les bulles** — rejeté pour les
   mêmes raisons que l'alternative 1, et parce qu'un sfx romanisé n'a pas
   toujours un équivalent direct (parfois il faut inventer un équivalent
   onomatopéique, ce qui dépasse la portée d'une traduction mécanique).

## Conséquences

- `StrategieEffacement` (module `effacement.rs`) doit retourner `Conserver`
  pour `Sfx` et `TexteLibre`, et `Ignorer` pour `Ignore`.
- La webview peut afficher optionnellement la traduction des sfx et textes
  libres en légende (petit texte sous-jacent ou tooltip), mais ce n'est pas
  obligatoire pour la v1.
- Le pipeline de traduction (`translate` / `traduire_page`) continue de traduire
  tous les blocs, y compris les sfx : la traduction est stockée dans le JSON
  `PageTranslation` même si elle n'est pas affichée par défaut. La webview
  décide de son affichage.
