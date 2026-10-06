# Licences de la phase T — audit T0.1

> Écrit le 2026-10-06, pendant T0. **Aucun modèle ni police n'entre dans Tatami
> tant que sa licence n'est pas confirmée ici.**
>
> **Méthode** : chaque ligne a été relevée sur la source primaire, jamais de mémoire.
> Deux sources, parce qu'elles ne disent pas la même chose :
>
> | Ce qu'on vérifie | Où |
> |---|---|
> | Code | champ `license.spdx_id` de l'API GitHub **puis** les 3 premières lignes du fichier `LICENSE` lui-même |
> | Poids | champ `cardData.license` de l'API Hugging Face |
>
> Rappel `AGENTS.md` §2.2 : un nom « qui a l'air plausible » est une erreur. Un
> dépôt qui renvoie `404` n'est pas « absent », c'est « déplacé » — chaque
> déplacement a été recherché, pas supposé.

---

## 1. Résultat en une phrase

**Tout ce qu'on a mesuré est redistribuable dans un installeur GPLv3, sauf la
famille NLLB-200, qui est non commerciale et doit rester exclue.** Deux
corrections de licence par rapport aux noms employés dans `PLAN-TRADUCTION.md` :
`comic-translate` n'est plus chez `zyddnys` et n'est **plus en GPLv3**.

---

## 2. Outils (code)

| Outil | Dépôt réel | Code | Dépôt originel du plan | Verdict |
|---|---|---|---|---|
| comic-translate | `ogkalu2/comic-translate` | **Apache-2.0** | `zyddnys/comic-translate` → **404** | Utilisable |
| manga-image-translator | `zyddnys/manga-image-translator` | **GPL-3.0** | idem | Utilisable (même licence que nous) |
| BallonsTranslator | `dmMaze/BallonsTranslator` | **GPL-3.0** | `Helixform/BallonsTranslator` → **404** | Utilisable |
| comic-text-detector | `dmMaze/comic-text-detector` | **GPL-3.0** | `cansalib/…` → **404** | Utilisable |
| manga-ocr (code) | `kha-white/manga-ocr` | **Apache-2.0** | — | Utilisable |
| LaMa (code) | `advimman/lama` | **Apache-2.0** | — | Utilisable |
| ONNX Runtime | binaire `onnxruntime` 1.30.0 | MIT | — | Utilisable |

### 2.1 Deux corrections qu'il fallait mesurer

**`comic-translate` a changé de dépôt et de licence.** Le chemin du plan
(`zyddnys/comic-translate`) renvoie `404`. Le dépôt vivant est
`ogkalu2/comic-translate`, et son fichier `LICENSE` commence par
`Apache License Version 2.0, January 2004`. Il est marqué `fork: false` et a
été créé le 2024-01-25 — ce n'est donc pas un fork déclaré de
`manga-image-translator`, qui reste **vivant et en GPL-3.0**. Conséquence
pratique : Apache-2.0 est compatible avec notre GPLv3, mais la compatibilité
n'est jamais à présumer d'un changement de licence → relire `LICENSE` au moment
de l'intégration, pas aujourd'hui.

**BallonsTranslator et comic-text-detector ont eux aussi été déplacés**
(`Helixform` → `dmMaze`, `cansalib` → `dmMaze`). Les deux sont en **GPL-3.0**,
identique à la nôtre : le choix entre Comic-Translate et BallonsTranslator ne
pose donc **pas** de question de licence côté code.

---

## 3. Poids de modèles

**C'est ici que se décide la phase, et non dans le tableau 2.** Une licence de
code ne dit rien des poids.

| Poids | Licence | Taille mesurée | Verdict |
|---|---|---|---|
| `ogkalu/comic-text-and-bubble-detector` (RT-DETR-V2, 11 k images de comics) | **Apache-2.0** | int8 41,8 Mo · fp32 160,7 Mo | **Retenu** |
| `Carve/LaMa-ONNX` — `lama_fp32.onnx` | **Apache-2.0** | 198,4 Mo | **Retenu** |
| `onnx-community/manga-ocr-base-ONNX` | **Apache-2.0** | enc 327,5 + dec 112,0 Mo | Retenu, sous réserve (§5) |
| `kha-white/manga-ocr-base` (poids d'origine) | **Apache-2.0** | `pytorch_model.bin` 423,6 Mo | même modèle |
| `dreMaz/AnimeMangaInpainting` (LaMa affiné manga) | **MIT** | — | Alternative |
| `Helsinki-NLP/opus-mt-en-fr` | **Apache-2.0** | 286,9 Mo (torch) | Retenu hors ligne |
| `Helsinki-NLP/opus-mt-tc-big-en-fr` | **CC-BY-4.0** | — | Retenu (attribution exigée) |
| `facebook/m2m100_418M` / `m2m100_1.2B` | **MIT** | — | Licence OK, **qualité non mesurée** |
| `facebook/xglm-564M` | **MIT** | — | Licence OK, **qualité non mesurée** |
| `Qwen/Qwen2.5-0.5B-Instruct` / `1.5B-Instruct` | **Apache-2.0** | — | Licence OK, **qualité non mesurée** |
| `ogkalu/AOT-GAN` (effacement alternatif) | **À VÉRIFIER** | — | **Exclu en attendant** |
| `smartywu/big-lama` | Apache-2.0 (carte HF) | — | Doublon de `Carve/LaMa-ONNX` |

### 3.1 NLLB-200 est exclu — non commercial

`facebook/nllb-200-distilled-600M` et `facebook/nllb-200-3.3B` sont en
**`cc-by-nc-4.0`**. La clause **NC** interdit l'usage commercial, donc la
**redistribution** dans un installeur. C'est le modèle hors ligne que la
plupart des projets de traduction de comics citent en premier : il est donc
**écarté**, et pas « à vérifier plus tard ». Le §6 du plan (mode hors ligne) est
servi par opus-mt / M2M100 / XGLM / Qwen, tous permissifs.

### 3.2 Provenance des données d'entraînement — réserve réelle

`dmMaze/comic-text-detector` déclare dans son README : « 1/3 from Manga109-s,
1/3 from DCM, 1/3 synthetic ». **Manga109-s** a ses propres conditions
d/research non commerciales. Cela ne change pas la licence des poids
(GPL-3.0), mais c'est un **risque de provenance** à consigner : si ce
détecteur devenait un composant livré, il faudrait trancher ce point.

Le même README ajoute : « We would not (don't have the right) share training
sets or fonts publicly ». Donc **on ne doit pas récupérer les jeux
d'entraînement** de ce projet — ni pour audit, ni pour fixture.

---

## 4. Polices

Toutes vérifiées dans `google/fonts` (dépôt de référence), licence lue dans le
fichier, puis **couverture française mesurée** sur la table `cmap` avec
`fontTools` (24 caractères : accents, `œ`/`Œ`, `«»`, espace insécable, `€`,
`’`, `—`, `…`).

| Police | Licence | Couverture FR (24 car.) | Taille |
|---|---|---|---|
| Comic Neue (Regular + Bold) | **OFL 1.1** | **24/24** | 55,9 + 54,4 Ko |
| Bangers | **OFL 1.1** | **24/24** | 91,0 Ko |
| Patrick Hand | **OFL 1.1** | **24/24** | 209,7 Ko |
| Architects Daughter | **OFL 1.1** | **24/24** | 42,3 Ko |
| Caveat (variable) | **OFL 1.1** | **24/24** | 394,2 Ko |
| Kalam | **OFL 1.1** | **24/24** | 417,3 Ko |
| Luckiest Guy | **Apache-2.0** | **24/24** | 71,6 Ko |
| Permanent Marker | **Apache-2.0** | **24/24** | 72,9 Ko |

Les 8 polices couvrent **100 %** du jeu exigé par T4.1. Aucune n'est éliminée
par le français — le choix se fera donc sur le rendu en bulle, pas sur la
licence. **OFL et Apache-2.0 imposent toutes deux de redistribuer la licence
elle-même** avec la police, et OFL interdit de vendre la police seule.

`GloriaHallelujah-Regular.ttf` a échoué au téléchargement (`404`) : **non
évaluée**, donc hors liste. Mesurer une police qu'on n'a pas est une faultille
trop confortable.

---

## 5. Ce qui reste À VÉRIFIER — et pourquoi ce n'est pas de la procrastination

| Point | Pourquoi ce n'est pas tranché |
|---|---|
| Fidélité du décodeur OCR sans `transformers` | Mesuré **non fonctionnel** (§7) : les espaces disparaissent. C'est ma sonde qui est fautive, pas le modèle — mais tant que ce n'est pas corrigé, l'export ONNX de l'OCR **n'est pas validé**. |
| `l0wgear/manga-ocr-2025-onnx` | 4 882 téléchargements, **aucune licence déclarée**, encodeur 21,3 Mo seulement. Sans licence : exclu, règle de la phase. |
| AOT-GAN (`ogkalu/AOT-GAN`) | API HF `401` : absent **ou** privé. Impossible de distinguer sans compte. |
| Mécanisme de mise à jour des poids | Un pack signé (T1.4) exige de savoirrvérifier la signature de l'amont. |
| Licences des modèles distillation/quantifiés tiers | `l0wgear`, `snekky123` : Apache-2.0 déclaré sur l'un, absent sur l'autre. |

---

## 6. Effet sur le projet

- **Compatible GPLv3** : tout ce qu'on a mesuré. Apache-2.0 → GPLv3 est
  autorisé dans cette direction ; GPL-3.0 → GPLv3 est identique.
- **Interdit** : NLLB-200 (CC-BY-NC) dans toute redistribution.
- **À éviter** : AOT-GAN et tout modèle sans licence lisible.
- **Obligation d'attribution** : OFL et CC-BY imposent de livrer la licence de
  la police/du modèle avec le pack. À prévoir dans le manifeste de pack (T1.4).

**Ce que « licence OK » ne veut PAS dire.** Les lignes marquées *licence OK,
qualité non mesurée* sont permises à la redistribution mais **n'ont pas été
évaluées** : aucun modèle hors ligne n'a été lancé sur du texte de webtoon dans
cette session (T0.5 non fait, §5). La question « le mode hors ligne atteint-il
le seuil de qualité ? » est **ouverte** : c'est la moitié de l'ADR 0008.

## 7. Journal des mesures

Toutes exécutées le 2026-10-06 sur la machine cible
(i5-6200U, 2 cœurs / 4 threads, **11 906 Mo**, **sans GPU** — `nvidia-smi`
absent, seul un Intel HD 520 ; 77,9 Go libres).

```
GitHub   GET /repos/{owner}/{repo}                    → spdx, archived, fork
HF       GET /api/models/{id}[?blobs=true]            → cardData.license, tailles
raw.githubusercontent.com/{repo}/{branche}/LICENSE    → 3 premières lignes
PyPI     GET /pypi/{pkg}/json                        → requires_python, classifiers
```

Trois résultats qui n'étaient pas attendus et qui **n'auraient pas été trouvés
sans exécuter** :

1. **`onnxruntime` a une roue `cp314`** (Python 3.14.7 de la machine). La pile
   vision s'installe donc sans ajouter d'interpréteur.
2. **LaMa `int8` ne se charge pas** : `DFT … one-sided DFT requires real input`.
   La version fp32 fonctionne. La quantification n'est donc pas disponible pour
   l'effacement — et c'est le poste le plus cher en RAM.
3. **L'OCR ONNX de `manga-ocr` se lance sans torch** et lit `HELLO WORLD`, mais
   ma boucle de décodage maison perd les espaces (`WHEREISSHE` pour
   `WHERE IS SHE`). Le `vocab.txt` est **byte-level, sans marqueur `▁`**, et
   `generation_config.json` demande `no_repeat_ngram_size=3` et
   `num_beams=4`, que ma boucle gloutonne ignore. Corollaire : **`HELLO WORLD`
   ne prouve pas que l'OCR est bon** — la leçon « une fixture qui passe peut
   tester la mauvaise chose » (§14), appliquée à moi-même.