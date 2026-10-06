# Protocole d'évaluation de la traduction (T0.3)

> Écrit le 2026-10-06, phase T0. **Les seuils sont proposés, pas validés.**
> Ils n'ont de valeur qu'une fois acceptés par l'humain (critère de sortie de T0).

## 0. Règle qui gouverne tout le reste

> **Une mesure qui ne peut pas crier n'est pas une mesure.**

Chaque métrique de ce document est donc accompagnée :
1. d'un **contrôle négatif** — un cas qui doit échouer ;
2. d'un **contrôle positif** — un cas **légitime** qui doit réussir.

Ce n'est pas une précaution de forme. Pendant T0, la métrique « l'OCR lit
correctement » a d'abord renvoyé `WHEREISSHE` pour `WHERE IS SHE`, et une
autre fois `HELLO WORLD` parfait sur un texte court alors que les phrases
étaient illisibles. Les deux résultats étaient **faux** : la première faute
était mon décodeur, la seconde l'écrasement d'un texte large dans une image
carrée. Une métrique qui ne distingue pas ces trois cas ne vaut rien.

## 1. Corpus

| Propriété | Valeur | Raison |
|---|---|---|
| Taille | **20 chapitres** EN | plan T0.2 |
| Variété | polices, fonds sombres et dégradés, bulles non rectangulaires, nuages, cartouches, texte sur le dessin, bulles à cheval sur deux images | plan T0.2 |
| Stockage | **local, hors du dépôt** | `AGENTS.md` §2.7 |
| Versionné | **aucune image** ; seulement `docs/eval/corpus.md` | §2.7 |

**Le corpus n'est pas constitué.** C'est un **blocage** : il faut des chapitres
réels d'une source ouverte, et le téléchargement + la sélection took des
décisions qui appartiennent à l'humain (voir `docs/spike-traduction.md` §Blocs).

> Note d'hygiène : un corpus téléchargé n'a pas vocation à finir dans un
> dossier de versionnement. Le chemin hors dépôt
> (`…\Workspaces\Tatami-eval\corpus`) et les entrées `.gitignore` sont en place
> **et prouvées** (un `.png` et un `.jpg` déposés sous `docs/eval/` ont bien été
> rejetés par `git check-ignore -v`, en nommant la ligne).

## 2. Annotation manuelle — le poste qui coûte le plus cher

Les métriques ne se calculent pas toutes seules. Le plan demande environ
**50 pages annotées** (détection) et **200 blocs** (OCR) et **100 blocs**
(qualité de traduction).

| Jeu | Volume | Ce qu'on note | Qui |
|---|---|---|---|
| Détection | ~50 pages | chaque bulle : polygone, type (dialogue/narration/sfx/libre/ignorer) | humain |
| OCR | ~200 blocs | texte anglais de référence, mot pour mot | humain |
| Traduction | ~100 blocs | fidélité / naturel / ton, chacun sur 5 | humain |

**Charge estimée : 4 à 7 h de travail humain.** C'est le principal coût de T0 et
il n'est pas compressible. C'est une raison de plus pour **valider les seuils
avant** d'annoter : annoter 200 blocs pour redéfinir ensuite le critère est
du travail jeté.

## 3. Métriques

### 3.1 Détection (bulles + texte)

- **Rappel** = blocs détectés / blocs annotés (une bulle non détectée = non
  traduite, donc visible).
- **Précision** = blocs vrais / blocs détectés (une fausse bulle = du texte
  effacé pour rien, donc **dégât irréversible** : l'original est perdu).
- Métrique secondaire : **IoU** par bloc apparié, distribution et non moyenne
  seule — une moyenne haute peut cacher 40 % de blocs à 0,1.

> Précision et rappel ne se pèsent pas pareil ici. Le rappel manquant est
> réparable à la main (T5) ; la précision fausse est **définitivement**
> destructrice. Les seuils ci-dessous sont donc volontairement asymétriques.

### 3.2 OCR

- **Taux d'erreur** sur ~200 blocs : distance d'édition ÷ longueur de référence,
  sur le texte normalisé (casse, espaces).
- Taux de blocs **illisibles** (le modèle renvoie n'importe quoi) compté à part :
  ces blocs doivent basculer en `review`, pas en `ok`.

### 3.3 Effacement

- **Résidu d'anglais** : second passage d'OCR sur la zone nettoyée ; un bloc
  dont le second passage trouve du texte est `review` (T2.6).
- Pas de métrique automatique pour l'artefact : **jugement à l'œil**, sur
  fonds dégradés. La note va de 0 (propre) à 3 (disqualifiant).

### 3.4 Mise en page

- **Taux de débordement** parmi les blocs `ok` : cible **0 %**, non « le plus
  bas possible ». C'est l'exigence n°1 du plan.
- Part de blocs `review` et `fallback` — **seuil à fixer après les mesures** ;
  je ne propose pas de chiffre avant d'avoir vu la distribution (inventaire
  vide = mesure qui ne prouve rien).

### 3.5 Traduction

Notation humaine sur 100 blocs, 3 axes notés séparément (fidélité, naturel,
ton) sur 5. Moyenne par axe, **jamais fusionnée en une note unique** : une
moyenne unique cache un « naturel médiocre mais fidèle », qui ne se corrige pas
de la même façon.

### 3.6 Performance (T0.6)

Par tuile : latence et **pic de RAM**, avec les trois étages chargés
simultanément (sinon on sous-estime : le pic n'est pas la somme des charges,
c'est la somme des **arêtes de travail**).

## 4. Seuils proposés — à valider

Justification de chaque ligne : elle vient soit du plan (§1), soit d'une
mesure de T0, soit d'un principe. **Une ligne sans ces trois origines est une
convention inventée.**

| # | Métrique | Seuil proposé | Origine |
|---|---|---|---|
| S1 | Débordement parmi `ok` | **0 %** | exigence explicite du plan §1 |
| S2 | Rappel de détection | ≥ 90 % | un bloc manqué se voit, se corrige à la main |
| S3 | Précision de détection | ≥ 95 % | au-dessus de 90 % de précision, l'effacement destructif devient fréquent ; l'asymétrie vient de §3.1 |
| S4 | Taux d'erreur OCR | ≤ 10 % | au-delà, la traduction automaticque propage des fautes visibles |
| S5 | Blocs illisibles (`review`) | ≤ 10 % | idem S4 |
| S6 | Résidu d'anglais | ≤ 2 % des blocs | au-delà, l'original et le français se superposent |
| S7 | Part de `review` + `fallback` | **à fixer après mesures** | ne pas fixer un chiffre avant d'avoir la distribution |
| S8 | Qualité traduction | ≥ 3,5/5 sur chacun des 3 axes | sous 3,5, le mode automatique gêne la lecture |
| S9 | Latence par page | ≤ 30 s | perception du lecteur : au-delà, on lit l'original d'abord — ce qui est le comportement voulu, mais il faut le savoir |
| S10 | Pic RAM vision | ≤ 2 Go | marge sur 12 Go, lecteur + moteur JVM + interface ouverts |
| S11 | Mode hors ligne | ≥ S8 | sinon il n'est pas retenu (plan §6) |

## 5. Ce que les mesures T0 disent déjà des seuils

Relevé **avant** le corpus, sur images **synthétiques** (donc **aucun** de ces
chiffres ne prédit S2–S6 ; ils servent à vérifier que le protocole est
exécutable et à dimensionner la machine) :

| Poste | Mesuré |
|---|---|
| Détection RT-DETR-V2 int8, 640×640, CPU seul | **2,2 à 3,5 s** par passe |
| idem fp32 (160,7 Mo) | 2,6 s — **l'int8 est 3,8× plus léger pour la même précision** |
| Effacement LaMa fp32, 512×512 | **8,0 à 11,0 s** par tuile |
| Effacement LaMa **int8** | **ne se charge pas** (erreur DFT) |
| OCR encodeur + décodeur, 1 bloc | **~1,0 à 1,8 s** (encodeur seul 623 ms) |
| **Pic RAM, 3 étages chargés + 1 tuile** | **1 342 Mo** → S10 (2 Go) **tenu**, avec 2,1× de marge |

Le poste dominant est **l'effacement**, pas la détection. C'est l'inverse de
l'intuition (« le détecteur est lourd ») et cela change la conception : c'est
l'effacement qu'il faut rendre asynchrone et progressif, pas la détection.

## 6. Reproductibilité

T5.7 prévoit `scripts/eval-translation.ps1`. Deux règles dès maintenant :

- toute mesure porte **sa condition** (« 2,4 s par passe à 640×640 sur
  l'i5-6200U, sans GPU ») ; un chiffre sans condition n'est pas reproductible ;
- un échantillon à un seul instant ne dit rien — chronologie complète ou rien.