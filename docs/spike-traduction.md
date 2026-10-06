# Spike traduction — T0 (2026-10-06)

> Phase T0 : audit des licences et évaluation. **Aucune intégration dans
> l'application** — rien de ce rapport n'est branché dans Tatami.
>
> Ce rapport ne conclut pas « ça marche ». Il dit ce qui a été **mesuré**, ce
> qui n'a pas pu l'être, et ce qui doit être décidé par un humain.

## Verdict en une ligne

**GO conditionnel sur trois conditions**, dont une est bloquante (corpus).
Deux tiers des fondations/licenses sont vérifiées ; **toutes** les mesures de
qualité sont encore absentes, faute de corpus. Je ne peux pas recommander un GO
sur la base de ce que j'ai : je peux seulement dire que **rien n'a été trouvé
qui interdise la phase**.

---

## 1. Ce que cette session a réellement fait

| Tâche | État | Preuve |
|---|---|---|
| **T0.1** licences | **Fait** | `docs/notes/licences-traduction.md` — chaque ligne sur API primaire |
| **T0.2** corpus | **NON FAIT — bloquant** | §4 |
| **T0.3** protocole | **Fait**, seuils à valider | `docs/eval/protocole.md` |
| **T0.4** exécuter comic-translate | **Partiel** | §3 : le **modèle** de détection a tourné ; l'outil complet, non |
| **T0.5** comparer les moteurs | **NON FAIT** | §4 : pas de corpus, pas de clé |
| **T0.6** mesure machine | **Fait** | §2 |
| **T0.7** ce rapport + ADR | **Fait** | ADR 0007, 0008, 0009 |

## 2. T0.6 — mesures sur la machine cible

Machine : i5-6200U, **2 cœurs / 4 threads**, **11 906 Mo**, **sans GPU**
(`nvidia-smi` absent ; seul un Intel HD 520), 77,9 Go libres. Python 3.14.7.

| Poste | Version | Latence (CPU seul) | RAM |
|---|---|---|---|
| Détection RT-DETR-V2 **int8** (41,8 Mo) | Apache-2.0 | **2,2–3,5 s** / passe 640×640 | 347 Mo |
| Détection RT-DETR-V2 **fp32** (160,7 Mo) | Apache-2.0 | **2,6 s** | — |
| Effacement LaMa **fp32** (198,4 Mo) | Apache-2.0 | **8,0–11,0 s** / tuile 512×512 | 926 Mo |
| Effacement LaMa **int8** | Apache-2.0 | **NE SE CHARGE PAS** | — |
| OCR manga-ocr (enc 327,5 + dec 112,0 Mo) | Apache-2.0 | encodeur 623 ms · bloc ~1,0–1,8 s | 518 Mo |
| **Pic, 3 étages chargés + 1 tuile** | | | **1 342 Mo** |

### 2.1 Quatre résultats qui changent une décision

1. **L'effacement est le poste dominant**, pas la détection : 8 à 11 s contre
   2,4 s. L'intuition « le détecteur est lourd » est fausse ici. Conséquence
   de conception : c'est l'effacement qu'il faut rendre progressif, pas la
   détection.
2. **LaMa int8 ne se charge pas** — `one-sided DFT requires real input`. La
   quantification, qui aurait résolu le poste le plus cher, est indisponible
   pour ce modèle. À vérifier au niveau du port ONNX avant d'y compter.
3. **int8 n'accélère pas** le détecteur (3,4 s contre 2,6 s en fp32) mais
   coûte **3,8× moins de disque** à précision identique. Sur une machine sans
   GPU, la quantification sert ici à la **taille du pack**, pas à la vitesse.
4. **Le pic tient dans le seuil S10** (1 342 Mo contre 2 Go), avec 2,1× de
   marge. C'est une bonne nouvelle et elle est mesurée.

## 3. T0.4 — ce qui a tourné, et ce qui n'a pas tourné

### 3.1 Ce qui a tourné : le modèle de détection, pour de vrai

`ogkalu/comic-text-and-bubble-detector` (RT-DETR-V2, Apache-2.0, 3 classes :
`bubble`, `text_bubble`, `text_free`) a été **téléchargé et exécuté** sur
ONNX Runtime, sans torch, sans GPU. Les classes correspondent exactement au
besoin de classification de T2.3.

Sur une image **synthétique** (5 ellipses dessinées, aucune œuvre) : **5/5
bulles détectées**, score max 0,957. Contrôle négatif effectué : la classe
`text_free` sort à 0,31 quand elle ne doit pas.

### 3.2 Ce que ces chiffres ne valent pas

**IoU moyen 0,16.** C'est mauvais, et je ne vais pas l'expliquer par le modèle
sans preuve : une hypothèse (l'écrasement 1000×800 → 640×640) a été **testée
et infirmée** (letterbox, IoU 0,151 — pas mieux). La cause n'est pas établie.
Elle est très probablement dans ma sonde — images synthétiques trop simples,
modèle entraîné sur de vrais comics. **À VÉRIFIÉ**, et c'est pourquoi aucune
qualité de détection n'est annoncée dans ce rapport.

> Leçon appliquée : une valeur basse qui ne s'explique pas est un **défaut de
> mesure** jusqu'à preuve du contraire, pas un verdict sur le modèle.

### 3.3 L'OCR : il démarre, mais ma mesure n'est pas fiable

`manga-ocr` en ONNX **lit `HELLO WORLD`** sans torch — la faisabilité est
établie. Mais sur des phrases, il perd des caractères (`WHERE ISSHE`,
`THEQUCKBROWAFOX`) et un découpage en deux lectures le récupère exactement
(`WHERE` + `...!SHE`).

J'ai cherché la cause au lieu de conclure « l'OCR est mauvais ». Deux faits
trouvés dans les fichiers du modèle : le `vocab.txt` est **byte-level, sans
marqueur `▁`** (mon décodeur cherchait un marqueur d'espace qui n'existe pas) et
`generation_config.json` demande `no_repeat_ngram_size=3` et `num_beams=4`, que
ma boucle gloutonne ignore. **La faute est dans ma sonde.**

Mais je n'ai pas corrigé la sonde, donc **l'export ONNX de l'OCR reste non
validé** pour T0. C'est honnête : « ça se lance » ≠ « ça est juste ».

### 3.4 Ce que je n'ai pas fait

**comic-translate n'a pas été exécuté de bout en bout.** J'ai vérifié que sa
liste de dépendances **complète** se résout sur le Python 3.14 de la machine
(incluant PySide6), mais je n'ai pas construit l'application ni obtenu son
second moteur. T0.4 demande « au moins un second outil » : **non satisfait**.

## 4. Blocs — décisions qui appartiennent à l'humain

### Bloc 1 — Le corpus (bloquant pour T0.2, T0.4, T0.5)

T0.2 demande **~20 chapitres de webtoons anglais**. Je ne les ai pas
téléchargés, pour trois raisons :

1. C'est un **téléchargement massif** de contenus protégés vers le disque.
   L'appli sait déjà le faire (c'est sa fonction), mais une fois pour toutes
   pour une évaluation, ça mérite une décision explicite, pas un effet de bord
   de session.
2. Choisir **quels** webtoons est un choix de l'humain (ceux qu'il veut lire
   en français, ceux qui sont représentatifs de ses styles).
3. Sans corpus, **aucune** métrique de qualité (S2–S8) n'est calculable. Tout
   ce que je peux mesurer aujourd'hui, c'est la machine.

**Deux options :**

- **(a) Corpus choisi par toi** — tu me dis 4–6 séries (ou tu poses 20
  chapitres dans `…\Workspaces\Tatami-eval\corpus`). Je fais le reste, et le
  coût est l'annotation : **4 à 7 h humaines**.
- **(b) Corpus restreint de départ** — 3 chapitres (≈ 40 pages) pour valider
  le protocole et les seuils, puis on élargit si le GO tient. Coût humain
  réduit à ~1 h 30, et on découvre les défauts de protocole pendant qu'il est
  encore peu coûteux à changer.

Je recommande **(b)** : annoter 200 blocs avant d'avoir validé les seuils, c'est
du travail potentiellement jeté (cf. `protocole.md` §2).

### Bloc 2 — Les clés d'API (bloquant pour T0.5)

T0.5 demande **≥ 2 LLM en ligne**. Je n'ai aucune clé et **je n'en mets aucune
dans le dépôt ni dans un journal** (`AGENTS.md` §2.6).

**Deux options :**

- **(a) Tu fournis 2 clés** (OpenAI et un concurrent) dans le trousseau ou une
  variable d'environnement. Je compare avec/sans glossaire et avec/sans budget
  de caractères, comme demandé.
- **(b) Comparaison 100 % hors ligne** (opus-mt Apache-2.0, M2M100, XGLM,
  Qwen 0.5B/1.5B — tous permissifs). Aucune clé, coût nul, confidentialité
  totale, **mais** on ne mesure pas ce que la majorité des utilisateurs
  utilisera, et 0.5B sur CPU sera probablement trop faible.

Je recommande **(a)**, avec (b) en complément : c'est le seul moyen de savoir si
le hors ligne mérite d'exister comme option (S11).

### Bloc 3 — Le second outil de T0.4

BallonsTranslator (`dmMaze/BallonsTranslator`, GPL-3.0) est le candidat naturel.
Il est **lourd** (PySide6 + torch selon la version) et sur cette machine déjà
saturée. Lancer les deux en parallèle n'est pas raisonnable.

**Deux options :** (a) je fais le second outil sur **3 chapitres** seulement,
après le corpus ; (b) on renonce à la comparaison d'outils et on garde
comic-translate comme référence unique, en documentant que la comparaison n'a
pas eu lieu.

## 5. Recommandation

**GO conditionnel** pour la phase, avec ces réserves :

- **Favorable**, et mesuré : licences compatibles (une seule famille exclue) ;
  ONNX Runtime s'installe sur la machine cible ; pic RAM à 1,3 Go ; le
  détecteur tourne sans torch ; les 8 polices couvrent 100 % du français.
- **Défavorable / non démontré** : la **qualité** (S2–S8) est **entièrement
  non mesurée** ; l'OCR ONNX n'est pas validé ; la comparaison d'outils n'a pas
  eu lieu ; `int8` ne sauve pas l'effacement.
- **Le coût caché** : l'effacement à 8–11 s par tuile sur CPU est le vrai
  sujet de conception. Sur une page de webtoon à 10 bulles, la traduction
  complète ne serait **pas** instantanée. Le mode progressif du plan est donc
  **indispensable**, pas optionnel.

**Ce que je ne recommande pas**, c'est de fixer les seuils de qualité avant le
corpus : ils seraient inventés.

## 6. Journal des commandes

Toutes exécutées le 2026-10-06 depuis
`C:\Users\alexanders\Documents\Workspaces\Tatami-eval` (hors dépôt).

```
python -m venv venv                                   # Python 3.14.7
pip install --dry-run -r requirements.txt comic-translate   # résout, PySide6 inclus
pip install onnxruntime pillow numpy shapely requests       # venv = 136,9 Mo
python infer3.py     # détection : 5/5 bulles, 2 229 ms
python fp32.py       # fp32 2 564 ms contre int8 3 450 ms, IoU 0,161 contre 0,160
python lama3.py      # LaMa fp32 : 8,0 s / tuile ; int8 : échec DFT
python ocr9.py       # OCR : lit "HELLO WORLD", perd les espaces
python cmap.py       # 8 polices × 24 caractères français : 24/24 partout
python pic.py        # pic 3 étages + 1 tuile : 1 342 Mo
git check-ignore -v docs/eval/corpus/essai.png      # .gitignore:28 → rejeté
git status --short                                    # aucun fichier d'image
```

## 7. Suite

1. Décision humaine sur les **trois blocs** ci-dessus.
2. Constitution du corpus (option a ou b du bloc 1).
3. Annotation, puis **validation des seuils**, puis seulement les mesures.
4. ADR 0008 à reprendre quand la qualité hors ligne sera mesurée — il est
   aujourd'hui **explicitement ouvert**, et c'est honnête : le trancher sans
   mesure serait inventer.