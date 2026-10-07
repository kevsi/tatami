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

---

## 8. Seconde session (2026-10-06, après décision « corpus 3×3 »)

L'humain a choisi l'option (b) du bloc 1 : **3 chapitres de 3 séries
différentes**. Cette section remplace les « non mesuré » ci-dessus par des
chiffres, et corrige **trois erreurs** de la première session.

### 8.1 Corpus constitué — et la langue d'une source n'est pas une garantie

Corpus final : `1-webtoonsen` (*Daytime in the Bunker*), `2-webtoonsen`
(*I Love Yoo*), `3-webtoonsen` (*Like Mother, Like Daughter*) — 36 pages,
0 illisible. Description complète dans `docs/eval/corpus.md`.

**Quatre échecs de langue mesurés avant d'y arriver** : MangaDex a servi du
**coréen**, du **hongrois** et du **portugais** (avec des crédits en anglais,
le piège le plus trompeur) ; SEIMANGA sert du russe. Seules **WEBTOONS_EN** et
**TOOMICSEN** répondent en anglais. La langue réelle d'un chapitre ne se
vérifie que **par OCR d'un échantillon**, jamais par le nom de la série.

### 8.2 Correction n° 1 — l'OCR retenu lit le JAPONAIS, pas l'anglais

`kha-white/manga-ocr-base` (retenu à T0.6) est, par sa carte de modèle,
« Optical character recognition for **Japanese** text ». Son
`tokenizer_config.json` est `cl-tohoku/bert-base-japanese-char-v2` (MeCab).
« HELLO WORLD » avait été lu juste parce que le vocabulaire japonais contient
les caractères ASCII — **une fixture qui passe peut tester la mauvaise chose**
(§14), appliquée une deuxième fois à moi-même.

Le modèle que **comic-translate** utilise réellement pour l'anglais est
`ogkalu/ppocr-v6-onnx` (PP-OCR v6, **Apache-2.0**, 20,2 Mo, dictionnaire
18 708 entrées couvrant latin, chiffres et **tout le français** : é, œ, « »).
Téléchargé et exécuté.

### 8.3 Correction n° 2 — l'« IoU 0,16 » était un bug de coordonnées

Le mystère de la première session (boîtes « lâches », IoU 0,16, letterbox
infirmé) est **résolu**, et la cause n'était ni le modèle ni le letterbox :
**deux conventions mal lues dans mon propre code**.

1. L'export ONNX produit des boîtes `[x1, y1, x2, y2]`, pas `[cx, cy, w, h]`
   (mesuré : avec la mauvaise lecture, une boîte déborde du cadre de 28 % en
   hauteur ; avec la bonne, elle épouse une ligne de texte de 313×48 px).
2. `orig_target_sizes` attend **[largeur, hauteur]**, pas [hauteur, largeur]
   (mesuré : l'ordonnée détectée valait 0,82× l'ordonnée réelle — exactement
   le facteur d'échelle de la largeur).

Avec les deux corrections, la zone détectée tombe **exactement** sur le texte
et l'OCR la lit. Le détecteur était probablement sain depuis le début ; toute
la première mesure de « qualité de détection » est à jeter, pas à conserver.

### 8.4 Le pipeline complet mesuré sur de vraies pages anglaises

Chaîne exécutée : fenêtrage (1 300 px) → détection RT-DETR-V2 → NMS (le
modèle émet la même zone sous 2–3 classes, IoU 0,96–1,00) → découpe en lignes
par projection horizontale → OCR PP-OCRv6 → traduction opus-mt-en-fr.

| Étape | Latence mesurée | RAM |
|---|---|---|
| Détection (int8) | **2,2–3,3 s** / fenêtre 800×1 300 | — |
| Découpe en lignes | 1,2 ms | — |
| OCR PP-OCRv6 | **médiane 24–49 ms** / ligne | — |
| Traduction opus-mt (bulle) | **médiane 214 ms** / bulle | — |
| Effacement LaMa fp32 (§2) | 8–11 s / tuile | — |
| **Détection + OCR chargés** | | **394 Mo** |
| **+ traduction (enc + 2 déc)** | | **933 Mo** |

**L'OCR lit l'anglais réel**, multi-lignes incluses : « SHUT UP! | I HAVE TO
TAKE | THAT ROAD, | YOU KNOW! », « IT'S NOT LIKE | IT'S ANYTHING NEW, BUT… »,
« DON'T LITTER, | PUT THE TRASH IN | YOUR POCKET. », « WAS IT MY FAULT ».
Contrôle sur texte rendu : **6/6 exact**. Défauts restants : SFX stylisées
lues en caractères CJK (≈ 3 zones), zones vides sur onomatopées (≈ 6/21), et
une confusion occasionnelle (« FALLT » pour « FAULT »).

### 8.5 La traduction hors ligne : mesurée, et le découpage change tout

opus-mt-en-fr ONNX (Infomaniak, **Apache-2.0**, enc 189,5 + déc 330,2 Mo,
tokenizer SentencePiece — attention : les `.spm` ont 32 000 pièces, le modèle
attend les ids de `vocab.json` (59 514) ; sans ce mapping la sortie est du
charabia).

**Ligne par ligne**, la qualité s'effondre (« OH MY » + « GOSH! » séparés →
« - Oui ! »). **Par bulle entière**, elle devient lisible :

- « OH MY GOSH! » → « Oh mon Dieu ! »
- « I'M HOME… » → « Je suis chez moi… »
- « NOT HERE. » → « Pas ici. »
- « …BUT HE'S NOT THAT BAD OF A KID IN MY EYES. » → « Mais il n'est pas si
  mal qu'un gamin dans mes yeux. »
- « HOW COULD YOU TEACH YOUR KID LIKE THIS… » → « Comment pourrais-tu
  apprendre à tes enfants quand tu es un prof ? »

Défauts mesurés : un mot tombé (« SHUT UP! » disparait de la première
bulle), du vocabulaire approximatif sur l'argot et les SFX (« GOOSEBUMPS » →
« OBJECTIFS », « pocket » → « poteau », « trash » → « trace »), et les erreurs
d'OCR qui se propagent (« FALLT » → « mon échec »). **Conclusion de mesure** :
le hors ligne est **utilisable mais imparfait** — exactement le signal que
l'ADR 0008 attendait pour être reprise.

### 8.6 T0.5 — comparatif des moteurs, partiel mais le signal est net

Clés fournies par l'utilisateur le 2026-10-06 (dans le registre utilisateur,
jamais dans le dépôt). Trois enseignements mesurés :

**Gemini est visiblement meilleur que le hors ligne**, sur les 6 bulles où il
a répondu :

| Anglais | opus-mt (hors ligne) | Gemini (en ligne) |
|---|---|---|
| SHUT UP! I HAVE TO TAKE THAT ROAD… | Je dois prendre cette road… *(SHUT UP! perdu, « road » non traduit)* | **TAIS-TOI ! JE DOIS PRENDRE CETTE ROUTE, TU SAIS !** |
| IT'S NOT LIKE… THE STING OF BEING SO HATED STILL HURTS. | …le fait d'être si hanté jusqu'à ce jour. *(contresens)* | **…mais ça fait toujours aussi mal d'être détestée à ce point.** |
| I'M HOME… | Je suis chez moi… *(calque)* | **JE SUIS RENTRÉ…** |
| OH MY GOSH! | Oh mon Dieu ! | OH MON DIEU ! |

Et la config « production » (glossaire + budget 130 %) produit des sorties
**plus courtes** que la config brute (« Ce n'est pas nouveau, mais… » contre
« C'est pas comme si c'était nouveau, mais… ») : le budget de caractères est
tenable par un LLM, pas par opus-mt.

**Le quota gratuit d'une clé Gemini récente est très bas** : ~14 appels
journaliers sur `gemini-3.5-flash`, épuisés en deux passes (429, et les pauses
de 30 s ne débloquent rien → quota **journalier**, pas par minute). Deux
pièges de modèle mesurés en route : `gemini-2.0-flash` est **retiré** (404),
`gemini-2.5-flash` est **refusé aux clés récentes** (404 « no longer available
to new users ») ; et sans `thinkingConfig: {thinkingBudget: 0}`, le modèle
3.5 brûle son budget de sortie en « réflexion » et la traduction arrive
**tronquée**. Les 8 bulles restantes seront traduites au reset du quota
(minuit, heure du Pacifique) si nécessaire — le signal qualitatif, lui, est
déjà net.

**Mistral est inutilisable en gratuit sur ce compte** : HTTP 429
`rate_limited` dès le **premier** appel, persistant après 45 s. Blocage côté
compte, pas côté client. T0.5 tourne donc avec **1 LLM en ligne + le hors
ligne** au lieu de 2 + 1 — déviation documentée ici même.

### 8.6 bis — NLLB-200 mesuré sur Colab : la référence hors ligne n'est **pas** meilleure

L'utilisateur a exécuté `facebook/nllb-200-distilled-600M` sur Google Colab
(CPU) le 2026-10-06, sur les **12 bulles** (légal en évaluation ; toujours
exclu de la redistribution, CC-BY-NC). Résultat qui clôt le débat « et si le
meilleur hors ligne était ailleurs » :

| Anglais | opus-mt | NLLB-200 (Colab) | Gemini |
|---|---|---|---|
| SHUT UP! I HAVE TO TAKE… | road *(SHUT UP! perdu)* | cette route *(**SHUT UP! perdu aussi**)* | **TAIS-TOI ! JE DOIS…** |
| …STING OF BEING SO HATED STILL HURTS. | si hanté *(contresens)* | le souffle… toujours douloureux *(sens préservé)* | **…ça fait toujours aussi mal…** |
| WAS IT MY FALLT *(faute d'OCR)* | mon échec | **C'était ma faute** *(corrige la typo !)* | — |
| DON'T LITTER, PUT THE TRASH… | mets la trace dans ton poteau | **Ne pas mentir**, la déchette *(contresens)* | — |
| …NOT THAT BAD OF A KID… | qu'un gamin dans mes yeux | *(« of a kid » perdu)* | — |
| GOOSEBUMPS | OBJECTIFS | Les poules *(les deux faux)* | — |

**Verdict mesuré : NLLB-200 ≈ opus-mt.** Chacun gagne des bulles et en perd
d'autres ; les deux laissent tomber « SHUT UP! ». L'écart de qualité n'est
pas **entre** les modèles de traduction classiques, il est entre eux **et le
LLM**. Conséquence pour l'ADR 0008 : l'exclusion de NLLB-200 (CC-BY-NC) ne
coûte **rien en qualité** — opus-mt est confirmé comme choix hors ligne
livrable. Latence NLLB-200 : médiane 2,8 s/bulle sur **CPU Colab** — non
comparable à la machine cible (modèle ~8× plus gros qu'opus-mt, mesuré lui à
214 ms **sur la machine cible**).

### 8.7 Ce qui reste ouvert après cette session

- **Métriques S1–S6** (rappel/précision détection, taux d'erreur OCR) :
  toujours non calculables sans annotation humaine (4–7 h).
- **Comparatif en ligne (T0.5)** : partiel — 6 bulles sur 12, un seul LLM au
  lieu de deux. Complétable au reset du quota Gemini, ou avec une seconde
  clé qui fonctionne.
- **Second outil (T0.4)** : toujours non fait, mais la chaîne mesurée ici
  **est** celle de comic-translate (son détecteur + son OCR) — la preuve par
  les modèles existe, la preuve par l'outil assemblé reste à faire.
- **Erreur de l'app découverte en passant** : MangaDex ne garantit pas la
  langue du chapitre servi (cf. §8.1). Hors phase T ; signalée dans `PLAN.md`.

### 8.8 Journal des commandes (seconde session)

```
node corpus-probe.mjs      # moteur NDJSON ; 2 series (dont 1 non anglaise)
node serie3.mjs            # identification via details.title + tag "Web Comic"
node sources-en.mjs        # 2 sources EN qui repondent : WEBTOONS_EN, TOOMICSEN
node corpus-en.mjs         # 2 series EN (36 -> 24 pages), 0 refusee
node corpus-en3.mjs        # 3e serie EN, 12 pages
python detecter-reel.py    # detection sur pages reelles (32 fenetres)
python brut-boxes.py       # convention [x1,y1,x2,y2] + orig_target_sizes=[w,h]
python controle-ocr-en.py  # PP-OCRv6 : 6/6 sur texte rendu, espace=indice 18709
python pipeline-en3.py     # pipeline complet : texte anglais reel lu
python traduire.py         # opus-mt ligne/ligne : contexte casse
python traduire-bulles.py  # opus-mt par bulle : lisible, mediane 214 ms
python comparatif-t05.py   # T0.5 : Gemini 3.5 (6/12 bulles) vs opus-mt ;
                           # Mistral 429 des le 1er appel ; quota Gemini ~14/j
```
---

## 9. T2 (2026-10-07) — la reconnaissance, enfin mesurée

Cette section ne mesure pas la qualité : elle mesure **ce qui fonctionne**, sur
la machine cible, avec les poids réels. Les seuils S1 à S11 restent non validés
et le rapport ne conclut toujours pas GO.

### 9.1 Le runtime d'ADR 0007 est validé, pas seulement choisi

`ort 2.0.0-rc.13` (toutes les versions 1.x sont *yanked* sur crates.io, vérifié
sur l'index sparse), en `load-dynamic`, avec le `onnxruntime.dll` déjà présent sur
la machine. Parité Python/Rust sur la même image et les même bandes : **texte
décodé identique**, inférence 51 ms en Rust contre 193 ms en Python. Détail et
justification du choix de version : `docs/decisions/0007-runtime-vision-onnx.md`.

### 9.2 Le détecteur de bulles fonctionne — mais il ne voit que ce qu'il y a

`detector_int8.onnx` (RT-DETR-V2, 3 classes : `bubble`, `text_bubble`,
`text_free`) sur les pages du corpus :

| page | meilleur score | ce que c'est |
|---|---|---|
| `2-webtoonsen/p072.jpg` | **0,983** | page de dialogue |
| `2-webtoonsen/p081.jpg` | **0,332** | page **d'illustration, sans texte** |

La première mesure enflammée était « le détecteur ne détecte rien sur les
webtoons ». Elle était fausse : `p081` a été choisie parce que la sonde d'OCR de
T0 l'utilisait, pas parce qu'elle contient du texte ; à l'œil, c'est un plan de
supermarché sans une seule bulle. La boîte `(-177,-41)-(177,40)` rendue à 0,33
est une boîte dégénérée, pas une détection.

Deux causes écartées par la mesure, et non par le raisonnement :

- **la quantification int8** : fp32 0,350 contre int8 0,332 sur la même fenêtre,
  soit la même précision à 3,8× le poids. La phrase de T0 « précision
  identique » se confirme.
- **l'échelle** : agrandir la fenêtre ×2 puis ×3 ne change rien (0,308 → 0,364).

Le triage des 36 pages du corpus (une fenêtre de 640 px par tranche) donne six
pages à plus de 0,97 : le détecteur est sain sur les pages qui ont du texte.

### 9.3 L'étage qui manquait : la détection de LIGNES

Le reconnaissance PP-OCRv6 est un CTC : **il ne lit qu'une ligne**. Appliqué à une
bulle entière de 641×640 px, il rend des fragments (`LE`, `pe`) — mesuré. Il
falait donc un troisième modèle, absent du dispositif de T0.

`manga_det_v0.2.onnx` (1,7 Mo, Apache-2.0, même dépôt que le reconnaisseur déjà
évalué) sort une carte de probabilité style DBNet. Recette PP-OCR standard
(hauteur multiple de 32, division par 255, centrage, seuil 0,3, composantes
connexes) sur la bulle `(178,149)-(819,789)` de `p072` : **7 composantes**, dont
des lignes de 119 à 188 px de large, et le texte réel relu :

```
PROBABLY
THE SAME PLACE
YOU GOT TO GET
CLOTHES FOR
DOGS
```

Le troisième modèle n'était donc pas optionnel : sans détecteur de lignes, la
chaîne ne produit que des fragments.

### 9.4 Un dictionnaire court d'une entrée supprimeait chaque espace

Le modèle a **18 710 classes**. Le fichier `PP-OCRv6_small_rec.txt` fournit
**18 708 caractères**, plus l'entrée 0 réservée au blanc CTC. L'indice **18709**
n'est donc mappé par rien.

Cet indice est **l'espace**, et il est émis entre chaque mot. Mesuré sur du texte
rendu dont on connaît la réponse :

```
attendu : 'YOU GOT TO GET CLOTHES'
ids     : [67, 57, 63, 18709, 49, 57, 62, 18709, 62, 57, 18709, ...]
```

Le décodage de T0 (`if i != prev and i > 0 and i < len(DICT)`) **supprimait
systématiquement 18709**, et le texte arrivait `YOUGOTTOGETCLOSHESFORDOGS`. Le
contrôle de T0 ne pouvait pas le voir : sa comparaison se faisait **après avoir
retiré les espaces des deux côtés**.

Deux pièges voisins, pour qu'ils ne se reproduisent pas :

- l'entrée 1748 du dictionnaire est **U+3000** (espace idéographique), pas une
  espace ASCII. En Powershell elle s'affiche comme une espace : la console
  convinced le contraire. Une normalisation `\u3000` → `' '` reste nécessaire,
  mais elle ne suffit pas — c'est l'entrée manquante qui compte ;
- `cargo` utilise `CARGO_HOME` (`...\scoop\persist\rustup\.cargo`), **pas**
  `%USERPROFILE%\.cargo`. Chercher le crate au mauvais endroit donne un
  « INTROUVABLE » qui ressemble à un réseau coupé.

### 9.5 Latences mesurées sur la machine cible (i5-6200U, 2 cœurs, CPU seul)

| étape | charge | par exécution |
|---|---|---|
| `detector_int8` (640×640) | 4,6–5,1 s | **2,1–2,5 s** |
| `manga_det_v0.2` (lignes) | < 1 s | **100 ms** |
| `PP-OCRv6_small_rec` (une ligne) | 846 ms | **51 ms** |

Chargement des trois sessions simultanément : à mesurer (T0 annonçait 1 342 Mo
pour le pic, jamais revérifié depuis).

### 9.6 État du corpus

Les pages **mangadex** qui portaient les mesures de détection de T0 (score max
0,98 dans `logs/detection-reelle.json`) **ne sont plus sur le disque**. Il ne
reste que les trois séries WEBTOONS, 36 pages. Le contrôle positif de la
détection a donc disparu au même moment où il aurait servi : à reconstruire
avant toute conclusion sur la qualité.
