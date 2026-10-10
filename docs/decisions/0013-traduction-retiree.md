# 0013 — La traduction de manga est retirée du produit
- Date : 2026-10-10
- Statut : acceptée — remplace 0007, 0008 et 0011

## Contexte

Les phases T1 à T4 (contrat de traduction, découpe en tuiles, masques de bulles,
classification des blocs, OCR et ordre de lecture, stratégie d'effacement,
contrôle de résidu, budget mémoire, traducteur Gemini + repli local, prompt par
page, glossaire par série, résilience, consentement, polices embarquées) ont été
conçues puis **retirées du code** : ni modules Rust, ni commandes Tauri
enregistrées, ni fichiers Kotlin du sous-système vision, ni méthodes déclarées
dans `contract/protocol.json`, ni tables SQLite — `page_translations` et `blocks`
sont supprimées par une migration.

Autrement dit, la décision est **déjà appliquée** ; c'est la trace documentaire
qui n'était pas à jour, et qui proposait encore un chantier — « écran pour saisir
la clé de traduction » — dont la cible n'existe plus.

Décision humaine du 2026-10-10 : ne pas reprendre cette feature.

## Décision

**Le produit est un lecteur de manga. La traduction automatique de manga n'en fait pas partie.**

- La chaîne de vision (détection, OCR, effacement, mise en page) reste hors périmètre.
- L'**interface** reste bien traduite en français et en anglais — ce sont deux choses
  distinctes, et la confusion a été levée explicitement le 2026-10-10 : « traduction
  de manga », pas « traduction de l'interface ».
- Les documents historiques sont **conservés** : ils racontent d'où vient la
  décision et pourquoi les choix techniques ont été pris. Supprimer un journal
  n'est pas une façon de prendre une décision.

### Ce qui n'est PAS retenu

- **Reprendre la traduction plus tard** : rien n'empêche de la réimplémenter, mais
  alors à partir de ce qu'on a appris — le pipeline actuel serait à refaire, pas à
  réactiver.
- **Supprimer les ADR 0007 / 0008 / 0011 et le spike** : ce serait effacer le
  contexte qui évite à quelqu'un de refaire les mêmes mesures.

## Conséquences

- Le périmètre de la v1 se réduit au lecteur : sources, recherche, bibliothèque,
  lecteurs, téléchargements, suivi, reprise de lecture.
- `PLAN.md` cesse de proposer un écran de clé de traduction.
- Le moteur JVM reste centré sur `kotatsu-parsers-redo` : le sous-système vision
  Onnx (0007) ne représente plus de coût au démarrage ni d'occupation mémoire.
- Si la traduction revient un jour, `docs/spike-traduction.md` contient les
  mesures de référence (opus-mt 214 ms/bulle, NLLB exclu pour sa licence
  CC-BY-NC, quota Gemini gratuit), et `docs/notes/licences-traduction.md`
  l'inventaire des licences des modèles.
- **Le dépôt est-il encore « fini » ?** Non, et cette ADR ne change rien à ce
  constat : les pages distantes n'ont pas de dimensions (0012), le smoke test
  n'a jamais été joué, et l'éditeur de bloc (T5.1) n'existe pas.