# 0007 — Runtime vision : ONNX Runtime, pas Python

- Date : 2026-10-06
- Statut : **proposée** — décide de la piste technique, à valider par l'humain

## Contexte

La détection de bulles, l'OCR et l'effacement sont trois modèles de vision.
Deux pistas possibles : Python avec PyTorch (ce qu'est `comic-translate`), ou un
moteur ONNX exécuté directement.

`PLAN-TRADUCTION.md` §2.2 prévoit un sidecar vision dédié, orchestré par Rust,
et les poids livrés en packs versionnés (T1.4). La machine cible est un portable
**i5-6200U, 2 cœurs, 11 906 Mo, sans GPU**.

## Décision

**Runtime ONNX, appelé depuis Rust. Pas de Python dans l'application livrée.**

Python reste accepté pour l'**évaluation** et pour la conversion ponctuelle de
poids, mais aucun interpréteur Python n'est requis par l'application.

## Pourquoi — les quatre mesures qui ont décidé

1. **`comic-translate` n'utilise pas torch.** Ses dépendances déclarées
   contiennent `onnxruntime>=1.22.1` et **aucun** `torch`. L'argument
   « il faut PyTorch » est donc faux pour le principal candidat.
2. **ONNX Runtime s'installe sur la machine cible.** Le Python 3.14.7 de la
   machine a une roue `cp314` pour `onnxruntime` 1.30.0, résolu puis installé.
   Ce n'était pas gagné : `onnxruntime` exige `>=3.11` et la couverture cp314
   varie selon les paquets.
3. **Le détecteur tourne réellement**, sans torch : RT-DETR-V2 en ONNX, 5/5
   bulles détectées sur image synthétique, 2,2–3,5 s par passe.
4. **Le pic RAM tient** : les trois étages chargés simultanément plus une tuile
   d'effacement donnent **1 342 Mo**, pour un seuil proposé de 2 Go.

## Alternatives étudiées

- **Python + PyTorch** : le chemin le plus court pour réutiliser le code de
  `comic-translate`. Rejeté : ajoute un interpréteur et plusieurs centaines de
  Mo de runtime à chaque installation, pour rien — les modèles retenus ont des
  exports ONNX existants et **permissifs** (Apache-2.0).
- **ONNX en Python pour tout** : position intermédiaire. Rejetée : garde le
  Python sans supprimer le coût ; le seul avantage restant (le débogage) s'obtient
  avec Rust.
- **WebGPU ou WASM dans la webview** : écarté, la machine n'a pas de GPU.

## Conséquences

- Le sidecar vision est un **binaire Rust** ; pas de gestion de versions Python
  chez l'utilisateur.
- Les modèles doivent exister en **ONNX**. C'est vérifié pour les trois étages
  retenus ; un modèle uniquement PyTorch demanderait une conversion, donc un
  outillage **et une vérification de parité** (`ONNX` contre `PyTorch`) à faire
  en T2.
- **Réserve connue** : l'export ONNX de `lama` **int8** ne se charge pas
  (`one-sided DFT requires real input`). Le poste le plus cher — l'effacement,
  8 à 11 s par tuile — reste donc en fp32. À explorer : ré-export, ou un autre
  modèle d'effacement.
- **L'OCR ONNX n'est pas encore validé** en exactitude (voir
  `spike-traduction.md` §3.3) : il se lance et lit du texte, mais la sonde de
  décodage utilisée était fautive. À reprendre avant T2.
- Le format de pack (T1.4) doit embarquer **les licences** : OFL et CC-BY
  imposent de les redistribuer avec le modèle.

## Ce que cette décision ne tranche pas

Ni la **qualité** (aucune métrique S2 à S8 n'est mesurée : pas de corpus), ni le
**choix définitif des modèles**. Elle ne fixe que le runtime.