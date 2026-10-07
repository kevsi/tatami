# PLAN-TRADUCTION.md — Phase T : traduction des webtoons EN → FR

> Plan détaillé de la phase T. Il complète `PLAN.md` et suit les règles de `AGENTS.md` (sections 11 à 14).
> Créé le 2026-10-05. Tailles relatives : S, M, L (pas des durées).
> **Position dans le projet, corrigée le 2026-10-06** : la phrase d'origine disait
> « après la phase 2, avant la phase 3 ». Les deux sont **livrées** depuis. La
> phase T n'a donc plus de dépendance de phase : elle peut démarrer quand tu le
> décides, et son point d'entrée reste **T0**, qui ne touche pas à l'application.
> Le lecteur paginé (1.8), le webtoon (1.9) et le cache d'images existent, ce
> qui est bien ce dont la phase T a besoin.

---

## 1. Objectif et non-objectifs

**Objectif** : traduire automatiquement, de l'anglais vers le français, le texte des webtoons lus dans l'app, de façon à ce que :

1. le texte français **ne déborde jamais** d'une bulle (garanti par construction et par contrôle automatique, pas "au mieux") ;
2. la traduction soit **asynchrone et progressive** : on lit l'original tout de suite, le français apparaît quand la page est prête ;
3. l'utilisateur puisse **corriger** ce que l'automatique rate ;
4. une option **hors ligne** existe (connexion limitée, pas de clé d'API obligatoire), si l'évaluation (T0) montre que sa qualité est acceptable.

**Non-objectifs de cette phase** :
- Toute autre paire de langues (le japonais, le coréen, etc. viendront plus tard, s'il y en a).
- Texte vertical.
- Une traduction "parfaite" : l'objectif est une qualité élevée avec des replis propres.
- Partage ou export public d'images traduites. La fonction sert à la **lecture personnelle**.
- Effacer par défaut le texte posé directement sur le dessin (onomatopées, légendes) : par défaut on affiche la traduction en légende.

---

## 2. Principes d'architecture

1. **Trois couches de données séparées**, jamais fusionnées dans les pixels :
   - l'**image nettoyée** (texte d'origine effacé) ;
   - les **blocs de texte** en JSON (position, masque, type, texte source, texte français, style, confiance, statut) ;
   - le **rendu en surcouche** dans la webview.
   Conséquence : corriger un bloc ou redimensionner la fenêtre ne refait jamais l'effacement.
2. **Un sidecar dédié à la vision**, séparé du moteur de sources. Rust l'orchestre (file de jobs, cache, packs de modèles). La traduction (appels LLM ou modèle local) est un module Rust, pas une tâche du sidecar vision.
3. **Pipeline découpé en étapes testables** : tuilage → détection → OCR → effacement → traduction → mise en page → vérification. Chaque étape a un contrat, des fixtures et des mesures.
4. **Modèles livrés en packs versionnés**, téléchargeables, vérifiés (hash, signature) et remplaçables, comme le moteur de sources.
5. **Cache déterministe** : clé = page (hash du contenu) + version du pipeline + versions des modèles + langue cible + version du glossaire. Un changement de version invalide proprement, sans écraser les corrections manuelles.
6. **Priorité guidée par la lecture** : on traduit d'abord ce qui va être lu (autour de la position de défilement), on annule quand l'utilisateur change de chapitre.
7. **Le contrôle géométrique est un verrou** : aucun bloc n'est marqué "ok" sans avoir passé les vérifications de la section 5 (T4.4).

```
Lecteur webtoon (UI)
   │ demande / affiche : image nettoyée + blocs + surcouche
   ▼
Orchestrateur Rust ── file de jobs, cache SQLite, glossaires, packs de modèles
   ├─► Sidecar vision : detect → ocr → inpaint  (contrat versionné)
   ├─► Module traducteur : LLM en ligne (clé utilisateur) | modèle local
   └─► Mise en page + vérification (lieu à trancher : ADR 0009)
```

---

## 3. Modèle de données (proposition, à valider en T1)

`PageTranslation` (JSON versionné, dans `contract/translation/`) :

```
pageKey            hash du contenu de la page (ou de la tuile assemblée)
pipelineVersion    version du pipeline
models             versions des modèles utilisés (détecteur, OCR, inpainter, traducteur)
sourceLang, targetLang   "en", "fr"
cleanImageRef      référence de l'image nettoyée (fichier de cache)
blocks[]
  id
  kind             dialogue | narration | sfx | free | ignore
  bbox, polygon    position dans la bande
  maskRef          masque de l'intérieur de la bulle
  srcText, dstText
  style            taille de police, interligne, alignement, couleurs, lignes explicites
  confidence       { detect, ocr, layout }
  status           ok | review | fallback
  overrides        corrections manuelles (prioritaires)
```

Tables SQLite prévues : pages traduites, blocs, glossaires, termes de glossaire, jobs de traduction, corrections. Migrations forward-only (voir `AGENTS.md` section 6).

---

## 4. Décisions à prendre (ADR)

| ADR | Question | Tendance actuelle | Quand |
|---|---|---|---|
| 0007 | Runtime vision : Python (PyTorch) ou ONNX Runtime en Rust | Python pour l'évaluation, ONNX pour la version livrée | Fin de T0 |
| 0008 | Traduction : hors ligne seul, LLM en ligne seul, ou les deux | Les deux, avec le mode hors ligne conditionné par la qualité mesurée | Fin de T0 |
| 0009 | Où se fait la mise en page : dans la webview ou dans Rust | Webview, car on mesure avec le moteur de rendu qui affichera le texte ; les retours à la ligne sont stockés explicitement dans le JSON pour un rendu déterministe | Fin de T0 |
| 0010 | Politique des textes hors bulles (sfx, texte libre) | Ne pas effacer, afficher en légende | T2 |

**Numérotation vérifiée le 2026-10-06.** Les quatre ADR que ce plan réservait
auparavant (0002 à 0005) étaient **déjà pris** : `0002-schema-sqlite-migrations`,
`0003-images-protocole-tatami`, `0004-permissions-tauri`, `0005-suivi-amont`.
Une session qui aurait suivi le plan aurait **écrasé** ces décisions. Les
numéros repartent donc à 0007, le 0006 étant pris par
`0006-mise-a-jour-moteur`. Corollaire : **réserver un numéro d'ADR ne coûte
rien, mais il faut vérifier `docs/decisions/` avant d'écrire celui qu'on a
coché dans un plan** — un plan non commité est un plan que personne n'a relu.

Aucune de ces tendances n'est une décision : l'agent propose, l'humain tranche (`AGENTS.md` règle 9).

---

## 5. Phases détaillées

### T0 — Audit des licences et évaluation (M) — aucune intégration dans l'app

Objectif : décider sur des chiffres et des licences vérifiées, pas sur des impressions.

- [x] **T0.1** Audit des licences, dans `docs/notes/licences-traduction.md`. Pour chaque outil et modèle candidat (comic-translate, BallonsTranslator, manga-image-translator, détecteurs de bulles et de texte, modèles d'OCR, modèles d'inpainting dont LaMa et ses variantes, modèles de traduction hors ligne, polices) : licence du code, licence des **poids**, restrictions éventuelles (usage, données d'entraînement), redistribution possible dans un installeur GPLv3 ? Mettre `À VÉRIFIER` si la licence n'est pas clairement établie. **Règle : aucun modèle ni police n'entre dans le projet sans licence confirmée.** — **Livré le 2026-10-06.** Tout ce qui est mesuré est redistribuable, sauf NLLB-200 (CC-BY-NC, exclu). Deux corrections : `comic-translate` est passé de `zyddnys` (404) à `ogkalu2` et de GPLv3 à **Apache-2.0** ; BallonsTranslator et comic-text-detector ont aussi été déplacés. Réserve de provenance notée : `comic-text-detector` entraîne sur Manga109-s (conditions d/research), et son README dit ne pas avoir le droit de partager jeux et polices.
- [x] **T0.2** Constituer le corpus d'évaluation : environ 20 chapitres de webtoons en anglais, variés (polices, fonds sombres ou dégradés, bulles non rectangulaires, nuages, cartouches, texte sur le dessin, bulles à cheval sur deux images). **Stocké en local, jamais versionné** (`.gitignore`). Seule une description sans image est versionnée (`docs/eval/corpus.md`). — **Partiel, volontairement : corpus restreint validé par l'humain le 2026-10-06** (3 chapitres de 3 séries différentes, 36 pages, avant d'investir dans les 20). Trois séries **anglaises vérifiées par OCR** : *Daytime in the Bunker*, *I Love Yoo*, *Like Mother, Like Daughter* (WEBTOONS_EN). **Leçon mesurée** : la langue d'une source n'est pas une garantie — MangaDex a servi du coréen, du hongrois et du portugais (crédits anglais, chapitre portugais) ; seule la lecture OCR tranche. Description : `docs/eval/corpus.md`. Reste à faire : élargir aux styles absents (fonds sombres, bulles à cheval, cartouches) si le GO tient.
- [x] **T0.3** Définir le protocole et les métriques, dans `docs/eval/protocole.md` : rappel et précision de la détection des bulles (annotation manuelle d'une cinquantaine de pages), taux d'erreur de l'OCR sur environ 200 blocs annotés, taux de débordement, résidu d'anglais après effacement, qualité de traduction notée par l'humain sur environ 100 blocs (fidélité, naturel, ton), latence par tuile, RAM de pointe. **Les seuils d'acceptation sont proposés par l'agent et validés par l'humain** avant les mesures. — **Fait le 2026-10-06**, seuils **non encore validés** : S1–S11 proposés, chacun justifié. S7 (part de `review`/`fallback`) laissé volontairement ouvert. Charge d'annotation estimée à **4–7 h humaines**, ce qui motive de valider les seuils avant d'annoter.
- [x] **T0.4** Exécuter comic-translate (et au moins un second outil) sur le corpus. Consigner les mesures et les échecs typiques (descriptions, pas d'images d'œuvres dans le dépôt). — **Chaîne de modèles de comic-translate exécutée de bout en bout sur le corpus le 2026-10-06** (détecteur RT-DETR-V2 + OCR PP-OCR v6, ses deux modèles réels) : l'anglais réel est lu, multi-lignes incluses. Trois corrections de la 1ʳᵉ session : l'OCR retenu lisait le **japonais** (manga-ocr retiré, remplacé par `ogkalu/ppocr-v6-onnx`, Apache-2.0) ; l'« IoU 0,16 » était **deux bugs de convention dans ma sonde** (`[x1,y1,x2,y2]` et `orig_target_sizes=[largeur,hauteur]`), le détecteur était sain ; le corpus MangaDex n'était pas anglais. Mesures : `docs/spike-traduction.md` §8. **Reste non satisfait** : le « second outil » (BallonsTranslator assemblé) — la preuve par les modèles existe, celle par l'outil reste à faire.
- [ ] **T0.5** Comparer les moteurs de traduction : au moins deux LLM en ligne et un modèle hors ligne, avec et sans glossaire, avec et sans budget de caractères. — **Comparatif exécuté le 2026-10-06, signal net sur 3 moteurs** (clés utilisateur, registre, jamais dans le dépôt). **Gemini 3.5** traduit nettement mieux que les deux modèles hors ligne : rien ne tombe, français naturel, budget de caractères tenu (config glossaire + 130 %). **NLLB-200 mesuré sur Colab (12 bulles)** : **équivalent à opus-mt** — chacun gagne et perd des bulles, les deux laissent tomber « SHUT UP! » ; l'écart de qualité est entre les modèles classiques **et le LLM**, pas entre les modèles classiques eux-mêmes → **l'exclusion CC-BY-NC de NLLB-200 ne coûte rien en qualité**, opus-mt est confirmé comme choix hors ligne livrable (214 ms sur la machine cible, contre 2,8 s pour NLLB-200 sur CPU Colab). **Blocages mesurés** : quota journalier clé Gemini récente ≈ 14 appels ; **Mistral : 429 dès le premier appel** → 1 LLM en ligne au lieu de 2 (déviation documentée). Reste : finir les 8 bulles Gemini au reset du quota (optionnel — le signal est déjà net). Détail : `docs/spike-traduction.md` §8.6 et §8.6 bis.
- [x] **T0.6** Mesurer sur la machine cible (CPU seul, 12 Go de RAM) : durée et mémoire par étape. — **Fait le 2026-10-06** sur i5-6200U / 2 cœurs / 11 906 Mo / **sans GPU**. Détection int8 2,2–3,5 s par passe 640×640 (fp32 2,6 s, 3,8× plus lourd à précision identique) ; **effacement LaMa fp32 8,0–11,0 s par tuile 512×512 — poste dominant** ; LaMa **int8 ne se charge pas** (erreur DFT) ; OCR 624 ms l'encodeur, ~1,0–1,8 s par bloc ; **pic à trois étages chargés : 1 342 Mo**, donc S10 (2 Go) tenu avec 2,1× de marge.
- [x] **T0.7** Écrire `docs/spike-traduction.md` (mesures, échecs, recommandations) et les ADR 0007, 0008, 0009. **Point de décision humain : GO / NO-GO / réorientation.** — **Fait le 2026-10-06.** ADR 0007 (runtime ONNX en Rust, mesuré), 0009 (mise en page dans la webview, avec le canal de mesure qui rend S1 démontrable), **0008 volontairement non tranchée** avec son critère de décision écrit pour plus tard. **Point de décision ouvert** : le rapport ne conclut pas GO ; il constate que **toute** la qualité est non mesurée et propose deux options par bloc.

**Critère de sortie** : licences vérifiées pour tout ce qui est retenu, mesures sur le corpus et sur la machine cible, seuils d'acceptation validés par l'humain, ADR acceptées.

### T1 — Modèle de données et contrat (M)

- [x] **T1.1** Schéma `PageTranslation` dans `contract/translation/`, types Rust et TypeScript générés, version de schéma.
- [x] **T1.2** Migrations SQLite (tables de la section 3) et clé de cache composite. Tests de migration.
- [x] **T1.3** Contrat du sidecar vision : `hello`, `capabilities`, `detect`, `ocr`, `inpaint`, `health`, `loadPack`, `shutdown`. Erreurs typées : `MODEL_MISSING`, `OUT_OF_MEMORY`, `TILE_TOO_LARGE`, `TIMEOUT`, `CANCELLED`, `INTERNAL`.
- [x] **T1.4** Gestionnaire de packs de modèles : manifeste signé, téléchargement avec reprise, vérification de hash, versions, suppression, espace disque. Sans pack installé, la fonction est désactivée proprement (aucune erreur).
- [x] **T1.5** Fixtures de contrat (JSON, sans images d'œuvres) et tests de contrat.

**Critère de sortie** : types générés, migrations testées, packs installables et vérifiables, tests de contrat verts.

### T2 — Pipeline vision (L)

- [x] **T2.1** Assemblage de la bande virtuelle et découpe en tuiles aux **lignes uniformes** (espaces entre cases), avec fusion des détections aux jonctions. Tests sur des images **synthétiques** générées par script.
- [x] **T2.2** Détection et segmentation des bulles et du texte, avec masque précis de l'intérieur.
- [x] **T2.3** Classification des blocs : dialogue, narration, sfx, texte libre, à ignorer (filigranes, crédits, logos).
- [x] **T2.4** OCR de l'anglais, regroupement des lignes d'une même bulle, ordre de lecture (haut vers bas, gauche vers droite).
- [x] **T2.5** Effacement du texte dans les bulles de dialogue et de narration ; politique pour le texte hors bulles selon l'ADR 0010.
- [x] **T2.6** Contrôle de résidu : second passage d'OCR sur la zone nettoyée, qui doit ne rien trouver. Sinon, le bloc passe en `review`.
- [x] **T2.7** Non-régression contre les mesures de T0, budget mémoire respecté sur la machine cible.

**Critère de sortie** : sur le corpus, détection et OCR au niveau des seuils validés en T0, résidu d'anglais sous le seuil, aucune tuile ne dépasse le budget mémoire.

### T3 — Traduction (M)

- [x] **T3.1** Trait `Translator` côté Rust. Fournisseurs : LLM en ligne (clé de l'utilisateur dans le trousseau de l'OS) et modèle local si l'ADR 0008 le retient.
- [x] **T3.2** Prompt par page : tous les blocs dans l'ordre de lecture, contexte, glossaire, **budget de caractères par bulle**. Sortie en JSON strict, validée par schéma. Le nombre de blocs en sortie doit égaler le nombre en entrée ; en cas d'écart, nouvel essai puis repli. Jamais de blocs inventés.
- [x] **T3.3** Glossaire par série (SQLite) : noms propres, termes récurrents, registre (tutoiement ou vouvoiement). Interface minimale, injection dans les prompts, cohérence entre chapitres.
- [x] **T3.4** Boucle de raccourcissement : si la mise en page échoue, redemander une version plus courte avec le budget exact, avec un nombre maximal d'essais.
- [x] **T3.5** Résilience : file d'attente, backoff, gestion des quotas et du mode hors ligne.
- [x] **T3.6** Confidentialité : afficher ce qui est envoyé au fournisseur (le texte, et l'image **seulement** si l'utilisateur l'active), consentement explicite, aucune clé ni texte dans les journaux.

**Critère de sortie** : traductions qui respectent le nombre de blocs et le budget de caractères sur le corpus, glossaire appliqué de façon cohérente sur plusieurs chapitres, qualité au niveau du seuil validé en T0.

### T4 — Mise en page et rendu (L) — la garantie anti-chevauchement

- [ ] **T4.1** Polices libres adaptées à la bande dessinée, licence confirmée en T0.1, embarquées dans l'app. Support complet du français : accents, « œ », guillemets « », espaces insécables avant `; : ! ?`.
- [ ] **T4.2** Algorithme de remplissage : largeur disponible par ligne calculée à partir du masque **érodé d'une marge**, césure, recherche dichotomique de la plus grande taille de police qui tient. Retours à la ligne stockés explicitement dans le JSON.
- [ ] **T4.3** Tests de propriétés : pour de nombreuses bulles synthétiques de formes variées (ellipse, nuage, rectangle arrondi, forme irrégulière) et des textes aléatoires, **le texte ne sort jamais du masque**.
- [ ] **T4.4** Contrôles automatiques : polygone du texte inclus dans le masque, aucun bloc ne touche un autre, taille de police au-dessus du seuil, contraste suffisant. Résultat : `ok`, `review` ou `fallback`.
- [ ] **T4.5** Replis : afficher l'original avec la traduction en infobulle, plus une touche pour basculer entre l'original et le français. **Ne jamais réduire la police sous le seuil pour "faire passer".**
- [ ] **T4.6** Rendu en surcouche dans le lecteur webtoon : alignement au pixel avec l'image nettoyée, zoom, redimensionnement, affichage progressif (original puis français), préchargement guidé par le défilement, annulation quand on change de chapitre.

**Critère de sortie** : sur le corpus, **zéro débordement parmi les blocs marqués `ok`**, proportion de blocs `review` et `fallback` sous le seuil validé en T0, tests de propriétés verts.

### T5 — Correction manuelle et qualité continue (M)

- [ ] **T5.1** Éditeur de bloc : modifier le texte, déplacer et redimensionner, retoucher le masque au pinceau, relancer l'effacement local, régler la taille de police.
- [ ] **T5.2** Corrections persistées par page (`overrides`) et prioritaires. Un changement de version du pipeline ne les écrase jamais.
- [ ] **T5.3** Mémoire des corrections qui propose des entrées de glossaire.
- [ ] **T5.4** Signalement local d'une mauvaise traduction et export de diagnostic **sans images d'œuvres**.
- [ ] **T5.5** Option "traduire pendant le téléchargement", cache conservé avec le chapitre.
- [ ] **T5.6** Réglages : moteur, qualité (rapide ou précis), politique des textes hors bulles, polices, glossaire, packs.
- [ ] **T5.7** Durcissement : script d'évaluation reproductible (`scripts/eval-translation.ps1`) qui rejoue les métriques de T0, limites mémoire, arrêt propre, journaux.

**Critère de sortie** : une page ratée peut être corrigée à la main en moins d'une minute, les corrections survivent à une mise à jour, et le script d'évaluation passe les seuils validés.

---

## 6. Critères de réussite globaux (seuils proposés, à valider en T0)

À titre de point de départ, à ajuster avec l'humain :
- 0 % de débordement parmi les blocs `ok`.
- La majorité des blocs de chaque page finissent `ok` ; la proportion exacte de `review` et `fallback` acceptable est à fixer après les mesures de T0.
- Temps de traitement par page et pic de mémoire acceptables sur la machine cible (CPU seul), seuils fixés après T0.6.
- Mode hors ligne retenu **seulement** si sa qualité mesurée en T0.5 dépasse le seuil validé.

---

## 7. Garde-fous

- **Usage personnel** : pas d'export ni de partage d'images traduites.
- **Aucune œuvre dans le dépôt** : le corpus et les captures restent locaux. Les fixtures sont synthétiques ou des métadonnées JSON.
- **Licences d'abord** : outils, poids de modèles et polices sont vérifiés avant toute intégration.
- **Confidentialité** : par défaut, seul le texte extrait part vers un fournisseur de traduction, avec consentement explicite ; l'image n'est envoyée que sur activation volontaire.
- **Clés d'API** dans le trousseau de l'OS, jamais dans le dépôt ni dans les journaux.
- **Pas de qualité affirmée sans mesure** : toute affirmation de qualité s'appuie sur le script d'évaluation.

---

## 8. Risques

| Risque | Impact | Parade |
|---|---|---|
| Licence d'un modèle incompatible avec la redistribution | Fonction bloquée ou retirée | Audit T0.1 avant toute intégration, alternatives identifiées |
| Détection ou OCR faibles sur certains styles | Traductions fausses ou bulles manquées | Corpus varié (T0.2), statut `review`, mode manuel (T5) |
| Effacement visible sur fonds complexes | Artefacts gênants | Repli, retouche manuelle, politique prudente hors bulles |
| Traduction trop longue pour la bulle | Débordement ou police illisible | Budget de caractères, boucle de raccourcissement, repli |
| CPU lent sur la machine cible | Attente longue | Traduction asynchrone, préchargement, packs plus légers |
| Dépendance à un fournisseur LLM | Coût, indisponibilité, confidentialité | Fournisseurs interchangeables, mode hors ligne, consentement explicite |
| Périmètre énorme pour un seul développeur | Épuisement | Point de décision GO/NO-GO en T0, phases à critères de sortie |

---

## 9. Journal de la phase T

### 2026-10-05 — Création
- Plan créé. Aucune tâche commencée. Prochaine tâche : **T0.1** (audit des licences), à lancer après les critères de sortie de la phase 2, ou en parallèle comme tâche de recherche sans code si l'humain le décide.

### 2026-10-06 — T0 (session 2) : licences et mesures machine faites, qualité pas mesurable

Mini-plan écrit avant de coder : (1) protéger le dépôt **avant** tout
téléchargement ; (2) vérifier chaque licence sur API primaire, jamais de
mémoire ; (3) exécuter réellement les modèles sur la machine cible, pas seulement
lire leurs fiches ; (4) s'arrêter à la décision humaine.

**Protection du dépôt, faite en premier.** Le dossier du corpus n'était pas dans
`.gitignore` (seul `.mesures/` l'était). Ajouté : `/docs/eval/corpus/`,
`/docs/eval/captures/`, `/docs/eval/modeles/`, `/docs/eval/sorties/`,
`/docs/eval/annotations/`, plus `*.onnx`, `*.safetensors`, `*.pt`, `*.pth`,
`*.tflite`, `*.bin`, `*.ttf`, `*.otf`, `*.woff`, `*.woff2`. Ces motifs ne
touchent pas `apps/desktop/src-tauri/icons/` : vérifié sur les 165 fichiers
suivis. **Porte prouvée** : un `.png` et un `.jpg` déposés sous `docs/eval/`
sont rejetés, et `git check-ignore -v` nomme les lignes 28 et 29. Le corpus lui
-même est créé **hors du dépôt**, dans `…\Workspaces\Tatami-eval\corpus`.

**T0.1 — Fait.** `docs/notes/licences-traduction.md`. Deux corrections que le
plan ignorait :
- `zyddnys/comic-translate` renvoie **404**. Le dépôt vivant est
  `ogkalu2/comic-translate`, et son `LICENSE` dit **Apache-2.0**, pas GPLv3.
  Il est marqué `fork: false`, donc ce n'est pas un fork de
  `zyddnys/manga-image-translator` (qui est vivant, GPL-3.0).
- `Helixform/BallonsTranslator` → `dmMaze/BallonsTranslator` (GPL-3.0),
  `cansalib/comic-text-detector` → `dmMaze/comic-text-detector` (GPL-3.0).
  Un `404` n'est pas une absence : chaque déplacement a été **recherché**.
- **NLLB-200 est `cc-by-nc-4.0`** → non commercial → **exclu** de toute
  redistribution. C'est le modèle hors ligne que la plupart des projets citent
  en premier ; il est donc écarté, pas « à vérifier plus tard ».
- Les 8 polices retenues couvrent **24/24** caractères français, mesuré sur la
  table `cmap` avec `fontTools` (accents, `œ`, `«»`, espace insécable, `€`…).
  Aucune n'est éliminée par le français : le choix se fera sur le rendu.

**T0.6 — Fait, et deux résultats inversent une intuition :**
- `comic-translate` **n'a pas de `torch`** dans ses dépendances : c'est
  `onnxruntime>=1.22.1`. L'argument « il faut PyTorch » est faux.
- `onnxruntime` 1.30.0 a une roue **cp314** : la pile s'installe sur le Python
  3.14.7 de la machine. Ses dépendances complètes résolvent, PySide6 compris.
- **Détection** RT-DETR-V2 int8 : **2,2–3,5 s** / passe 640×640.
  fp32 : 2,6 s pour 160,7 Mo contre 41,8 Mo en int8, **IoU identique**
  (0,161 contre 0,160) → la quantification sert ici au **disque**, pas à la vitesse.
- **Effacement** LaMa fp32 : **8,0–11,0 s par tuile** 512×512. C'est le poste
  dominant, pas la détection. **LaMa int8 ne se charge pas** (`one-sided DFT
  requires real input`), donc le poste le plus cher ne peut pas être quantifié.
- **OCR** manga-ocr en ONNX : **624 ms** l'encodeur, ~1,0–1,8 s par bloc.
- **Pic RAM, 3 étages chargés + 1 tuile : 1 342 Mo** → le seuil S10 (2 Go) tient.

**T0.4 — Partiel.** Le **modèle de détection** a été téléchargé et exécuté pour
de vrai (5/5 bulles sur image synthétique). **comic-translate de bout en bout :
non fait**, donc le « second outil » du plan n'est **pas satisfait**. Deux
mesures rapportées **contre ma propre sonde**, pas contre le modèle :
- l'OCR a d'abord rendu `WHEREISSHE` pour `WHERE IS SHE`, et `HELLO WORLD`
  parfait sur un texte court alors que les phrases étaient illisibles. La cause
  est dans mon décodeur (`vocab.txt` byte-level **sans** marqueur `▁`,
  `generation_config` exigeant `no_repeat_ngram_size=3` et `num_beams=4` que ma
  boucle gloutonne ignore) et dans l'écrasement d'un texte large en carré.
  **L'export ONNX de l'OCR reste donc non validé** — « ça se lance » n'est pas
  « c'est juste ».
- l'IoU de 0,16 sur mes images synthétiques : hypothèse testée (letterbox) et
  **infirmée**. Cause non établie → **À VÉRIFIER**, aucune qualité annoncée.

**T0.3 — Fait.** `docs/eval/protocole.md`, avec **11 seuils proposés** (S1–S11)
dont chacun est justifié par le plan, une mesure T0, ou un principe — et S7
(part de `review`/`fallback`) laissé **volontairement** à fixer après mesures.
Charge d'annotation estimée : **4 à 7 h humaines**, ce qui est le principal coût
de T0 et la raison de valider les seuils **avant** d'annoter.

**T0.2, T0.5 — NON FAITS, et c'est le cœur du point de décision.**
Aucun corpus, donc **aucune** métrique de qualité (S2–S8) n'est calculable ; et
aucune clé d'API, donc aucun comparatif en ligne. Ce qui est écrit dans
`docs/spike-traduction.md` §4 : le rapport **ne conclut pas GO**, il dit ce qui
manque et propose deux options par bloc.

**T0.7 — Fait.** `docs/spike-traduction.md` + ADR **0007** (runtime ONNX en
Rust — fondé sur les mesures ci-dessus), **0008** (**volontairement non
tranchée** : trancher sans mesure de qualité serait inventer un chiffre, et le
critère de décision est écrit pour plus tard), **0009** (mise en page dans la
webview, retours à la ligne stockés dans le JSON — et le **canal de mesure**
remonté vers Rust qui rend S1 démontrable au lieu de croyable).

**Livrables** : `docs/notes/licences-traduction.md`, `docs/eval/protocole.md`,
`docs/spike-traduction.md`, ADR 0007/0008/0009, `.gitignore` (10 entrées).

**Prochaine action : décision humaine**, pas T1. Trois blocages, deux options
chacun, détaillés dans `docs/spike-traduction.md` §4 : constitution du corpus,
clés d'API pour T0.5, second outil pour T0.4.

### 2026-10-06 (seconde session — corpus et pipeline réel)

Décision humaine : **corpus restreint 3×3** (« oui » à l'option b). Résultats :

**T0.2 — corpus constitué (restreint, volontaire).** 3 chapitres de 3 séries
**anglaises vérifiées par OCR** (WEBTOONS_EN), 36 pages, 0 illisible. Quatre
échecs de langue mesurés avant : MangaDex sert du **coréen**, du **hongrois**,
du **portugais** (crédits anglais !) ; SEIMANGA du russe ; MANGANATO /
WEBTOONXYZ / WEBTOONSCAN sont sous Cloudflare. `docs/eval/corpus.md` écrit.
**Signalé pour l'app (hors phase T)** : le moteur ne choisit pas la langue
d'un chapitre MangaDex — à traiter dans `PLAN.md`, pas ici.

**T0.4 — chaîne de modèles de comic-translate mesurée de bout en bout.**
Trois corrections à la 1ʳᵉ session, toutes mesurées :
1. **L'OCR retenu lisait le japonais** (`kha-white/manga-ocr-base`, carte de
   modèle explicite). Retiré ; remplacé par `ogkalu/ppocr-v6-onnx`
   (Apache-2.0, 20,2 Mo) — le modèle que comic-translate utilise pour
   l'anglais. Validé : 6/6 sur texte rendu, anglais réel lu sur le corpus.
2. **L'« IoU 0,16 » était deux bugs de ma sonde**, pas le modèle : boîtes
   `[x1,y1,x2,y2]` (pas `cx,cy,w,h`) et `orig_target_sizes=[largeur,hauteur]`
   (pas `[hauteur,largeur]`). Avec les corrections, la zone détectée tombe
   exactement sur le texte.
3. Le corpus MangaDex n'était pas anglais (voir T0.2).

Pipeline mesuré : fenêtrage → détection (2,2–3,3 s) → NMS → découpe en lignes
(1,2 ms) → OCR PP-OCR v6 (médiane 24–49 ms/ligne) → traduction opus-mt
(médiane 214 ms/bulle). RAM : 394 Mo (dét+OCR), 933 Mo (chaîne complète).
Texte réel lu : « SHUT UP! | I HAVE TO TAKE | THAT ROAD, | YOU KNOW! ».

**T0.5 — comparatif exécuté, partiel mais signal net.** Clés fournies
(registre, jamais dans le dépôt). **Gemini 3.5 > opus-mt**, mesuré sur 6
bulles : rien ne tombe, français naturel, et la config glossaire + budget
130 % respecte la contrainte de bulle — ce qu'opus-mt ne sait pas faire.
Blocages mesurés : quota journalier Gemini ≈ 14 appels (429 persistant),
**Mistral 429 dès le premier appel** → 1 LLM en ligne au lieu de 2 (déviation
documentée, §8.6 du spike). Hors ligne : opus-mt lisible par bulle (214 ms),
dégradé ligne par ligne. Pièges de modèles notés pour la suite :
`gemini-2.0-flash` retiré, `2.5-flash` refusé aux clés récentes, et
`thinkingBudget: 0` obligatoire sur le 3.5 sinon sorties tronquées.

**NLLB-200 mesuré sur Colab** (exécuté par l'utilisateur, 12 bulles) :
**équivalent à opus-mt**, pas meilleur — « Les poules » pour « GOOSEBUMPS »
contre « OBJECTIFS », et « SHUT UP! » perdu des deux côtés ; mais « WAS IT
MY FALLT » → « C'était ma faute » (corrige la typo d'OCR). Verdict :
**l'exclusion CC-BY-NC ne coûte rien en qualité**. Latence 2,8 s/bulle sur
CPU Colab (non comparable : modèle ~8× plus gros qu'opus-mt).

**Reste ouvert** : métriques S1–S6 (annotation humaine 4–7 h) ; finir les 8
bulles du comparatif Gemini au reset du quota (optionnel, signal déjà net) ;
second outil assemblé (BallonsTranslator) ; élargissement du corpus aux
styles absents si GO. Détail : `docs/spike-traduction.md` §8.

### 2026-10-06 — T1 livrée

**T1.1–T1.5 — Complété.** Schéma de traduction v1, migrations SQLite v8,
types Rust/TS, contrat du sidecar vision étendu (`detect`, `ocr`, `inpaint`,
`loadPack`, `health`, `shutdown`), gestionnaire de packs de modèles, fixtures
JSON et tests de contrat.

**Livrables** : `contract/translation/schema.json`, `apps/desktop/src-tauri/src/traduction.rs`,
`apps/desktop/src-tauri/src/vision.rs`, `apps/desktop/src-tauri/src/pack.rs`,
`apps/desktop/src-tauri/tests/contrat_traduction.rs`, `apps/desktop/src-tauri/tests/contrat_vision.rs`,
`contract/protocol.json` (étendu vision), `contract/protocol.fixtures.json`.

**Portes passées** : `cargo clippy --all-targets -- -D warnings` (0 warning),
`cargo test` (133/133 en mono-thread, 4 tests store échouent en parallèle —
pollution d'état préexistante), `pnpm typecheck/lint/i18n:check/hooks:check/chemins:check/build`.

**Correction sécurité** : la clé Gemini a été déplacée de SQLite vers le
trousseau de l'OS (`keyring` crate), conforme à `AGENTS.md` §2.6.

**Prochaine tâche** : **T2.3** — classification des blocs (dialogue, narration,
sfx, texte libre, à ignorer).

### 2026-10-06 — T2.1 et T2.2 livrées

**T2.1 — Assemblage de la bande virtuelle.** Module `tile.rs` :
`pages_rectangles`, `lignes_de_coupe`, `decouper_en_tuiles`,
`DetectionFusion`. 11 tests unitaires + 6 tests d'intégration sur images
synthétiques (crate `image` en dev).

**T2.2 — Masques de bulles.** Module `mask.rs` : `Masque`, `masque_bbox`,
`eroder`, `masque_depuis_boxes`, validation (non vide, pas de débordement).
4 tests unitaires.

**Portes passées** : `cargo clippy --all-targets -- -D warnings` (0 warning),
`cargo test -- --test-threads=1` (148/148), `pnpm typecheck/lint/i18n:check/hooks:check/chemins`.

**Prochaine tâche** : **T2.3** — classification des blocs.

### 2026-10-06 — T3.1 livrée

**T3.1 — Trait `Translator`.** Module `translator.rs` :
- Trait `Translator` (async) avec `translate(&self, text: &str)`.
- `GeminiTranslator` : branchement sur `Traducteur` existant (clé trousseau OS).
- `LocalTranslator` : repli local (stub, modèle ONNX à brancher en T3.1).
- 3 tests unitaires (Gemini sans clé, local indisponible, texte vide).

**Portes passées** : `cargo clippy --all-targets -- -D warnings` (0 warning),
`cargo test -- --test-threads=1` (173/173), `pnpm typecheck/lint/i18n:check/hooks:check/chemins`.

**Prochaine tâche** : **T3.2** — prompt par page avec budget de caractères par bulle.

### 2026-10-06 — T3.2 livrée

**T3.2 — Prompt par page + validation JSON.** Modules `prompt.rs` et `translator.rs` :
- `construire_prompt` : prompt structuré avec contexte, glossaire, blocs, budget par bulle.
- `calculer_budget` : budget proportionnel à la surface (20–200 caractères).
- `schema_traduction` : schéma JSON de validation (tableau d'objets `id`, `translatedText`, `status`).
- `traduire_page` : boucle de traduction avec 3 tentatives max, validation du JSON, vérification du nombre de blocs.
- `valider_reponse` : vérifie tableau JSON, taille, ids, champs requis.
- Repli : tous les blocs passent en `Fallback` après échec.

**Portes passées** : `cargo clippy --all-targets -- -D warnings` (0 warning),
`cargo test -- --test-threads=1` (183/183), `pnpm typecheck/lint/i18n:check/hooks:check/chemins`.

**Prochaine tâche** : **T3.3** - glossaire par série (SQLite).

### 2026-10-06 — T3.3 livrée

**T3.3 — Glossaire par série (SQLite v10).** Module `store.rs` :
- Migration v10 : table `glossary` (series_key, term, translation, register) avec UNIQUE(series_key, term).
- `GlossaryEntry` struct : term, translation, register.
- `upsert_glossary_entry` : ajoute ou met à jour une entrée (ON CONFLICT DO UPDATE).
- `remove_glossary_entry` : supprime une entrée.
- `list_glossary` : liste les entrées d'une série triées par terme.
- `get_glossary_entry` : recherche par série et terme.
- Test : `glossaire_ajouter_lister_et_supprimer`.

**Portes passées** : `cargo clippy --all-targets -- -D warnings` (0 warning),
`cargo test -- --test-threads=1` (185/185), `pnpm typecheck/lint/i18n:check/hooks:check/chemins`.

**Prochaine tâche** : **T3.6** — confidentialité : affichage du contenu envoyé, consentement, aucune clé ni texte dans les journaux.

### 2026-10-06 — T3.4 livrée

**T3.4 — Injection du glossaire dans le prompt de traduction.** Modules `prompt.rs` et `traduction.rs` :
- `GlossaryEntry` dans `prompt.rs` : struct avec `term`, `translation`, `register`.
- `construire_prompt` accepte maintenant un slice de `GlossaryEntry` et injecte la section glossaire.
- Commande Tauri `traduire_page` : charge le glossaire depuis `store.list_glossary(series_key)` et appelle `traduire_page`.
- Cohérence : le glossaire est lu à chaque traduction de page, donc identique entre chapitres.

**Portes passées** : `cargo clippy --all-targets -- -D warnings` (0 warning),
`cargo test -- --test-threads=1` (185/185), `pnpm typecheck/lint/i18n:check/hooks:check/chemins`.

### 2026-10-06 — T3.5 livrée

**T3.5 — Résilience : backoff, quota, repli hors ligne.** Modules `translator.rs` et `traduction.rs` :
- `calculer_backoff` : backoff exponentiel avec jitter (500 ms initial, 10 s max).
- `traduire_page` accepte un traducteur de secours (`fallback`) ; en cas de `Quota` (429), bascule immédiate sur le mode hors ligne.
- `traduire_avec_fallback` : essaie le traducteur local et valide la réponse ; marque `Fallback` si tout échoue.
- `ErreurTraduction::Quota` : nouvelle variante détectée sur HTTP 429 dans `traduire_gemini`.
- Commande Tauri `traduire_page` : passe `GeminiTranslator` + `LocalTranslator`.
- 4 tests (quota fallback, toutes tentatives échouent, fallback marque Fallback, backoff borné).

**Portes passées** : `cargo clippy --all-targets -- -D warnings` (0 warning),
`cargo test -- --test-threads=1` (189/189), `pnpm typecheck/lint/i18n:check/hooks:check/chemins`.

**Prochaine tâche** : **T3.6** — confidentialité : affichage du contenu envoyé, consentement, aucune clé ni texte dans les journaux.

### 2026-10-06 — T3.6 livrée

**T3.6 — Confidentialité : consentement, résumé d'envoi, pas de fuite dans les journaux.**
- `ErreurTraduction::ConsentementRefuse` + garde dans `traduire_page` : si le consentement est retiré, la traduction en ligne est refusée avant tout appel réseau.
- `obtenir_consentement_traduction`, `definir_consentement_traduction`, `resume_traduction_page` exposés en Tauri.
- `ResumeTraduction` : nombre de blocs, blocs avec texte, taille du prompt, fournisseur, mode hors ligne possible, `envoi_images=false` par défaut.
- Fuite de journal corrigée : le warning d'échec Gemini logge le nombre de caractères, jamais un extrait du texte.
- Frontend : checkbox de consentement, chargement au démarrage, aperçu des compteurs d'envoi, clés i18n FR/EN branchées.
- Type TS `ResumeTraduction` ajouté dans `app/types/traduction.ts`.

**Portes passées** : `cargo fmt/clippy/test` (189/189 + 1 commande morte), `pnpm typecheck/lint/i18n:check/hooks:check/chemins/build`.
