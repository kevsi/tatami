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

- [ ] **T0.1** Audit des licences, dans `docs/notes/licences-traduction.md`. Pour chaque outil et modèle candidat (comic-translate, BallonsTranslator, manga-image-translator, détecteurs de bulles et de texte, modèles d'OCR, modèles d'inpainting dont LaMa et ses variantes, modèles de traduction hors ligne, polices) : licence du code, licence des **poids**, restrictions éventuelles (usage, données d'entraînement), redistribution possible dans un installeur GPLv3 ? Mettre `À VÉRIFIER` si la licence n'est pas clairement établie. **Règle : aucun modèle ni police n'entre dans le projet sans licence confirmée.**
- [ ] **T0.2** Constituer le corpus d'évaluation : environ 20 chapitres de webtoons en anglais, variés (polices, fonds sombres ou dégradés, bulles non rectangulaires, nuages, cartouches, texte sur le dessin, bulles à cheval sur deux images). **Stocké en local, jamais versionné** (`.gitignore`). Seule une description sans image est versionnée (`docs/eval/corpus.md`).
- [ ] **T0.3** Définir le protocole et les métriques, dans `docs/eval/protocole.md` : rappel et précision de la détection des bulles (annotation manuelle d'une cinquantaine de pages), taux d'erreur de l'OCR sur environ 200 blocs annotés, taux de débordement, résidu d'anglais après effacement, qualité de traduction notée par l'humain sur environ 100 blocs (fidélité, naturel, ton), latence par tuile, RAM de pointe. **Les seuils d'acceptation sont proposés par l'agent et validés par l'humain** avant les mesures.
- [ ] **T0.4** Exécuter comic-translate (et au moins un second outil) sur le corpus. Consigner les mesures et les échecs typiques (descriptions, pas d'images d'œuvres dans le dépôt).
- [ ] **T0.5** Comparer les moteurs de traduction : au moins deux LLM en ligne et un modèle hors ligne, avec et sans glossaire, avec et sans budget de caractères.
- [ ] **T0.6** Mesurer sur la machine cible (CPU seul, 12 Go de RAM) : durée et mémoire par étape.
- [ ] **T0.7** Écrire `docs/spike-traduction.md` (mesures, échecs, recommandations) et les ADR 0007, 0008, 0009. **Point de décision humain : GO / NO-GO / réorientation.**

**Critère de sortie** : licences vérifiées pour tout ce qui est retenu, mesures sur le corpus et sur la machine cible, seuils d'acceptation validés par l'humain, ADR acceptées.

### T1 — Modèle de données et contrat (M)

- [ ] **T1.1** Schéma `PageTranslation` dans `contract/translation/`, types Rust et TypeScript générés, version de schéma.
- [ ] **T1.2** Migrations SQLite (tables de la section 3) et clé de cache composite. Tests de migration.
- [ ] **T1.3** Contrat du sidecar vision : `hello`, `capabilities`, `detect`, `ocr`, `inpaint`, `health`, `loadPack`, `shutdown`. Erreurs typées : `MODEL_MISSING`, `OUT_OF_MEMORY`, `TILE_TOO_LARGE`, `TIMEOUT`, `CANCELLED`, `INTERNAL`.
- [ ] **T1.4** Gestionnaire de packs de modèles : manifeste signé, téléchargement avec reprise, vérification de hash, versions, suppression, espace disque. Sans pack installé, la fonction est désactivée proprement (aucune erreur).
- [ ] **T1.5** Fixtures de contrat (JSON, sans images d'œuvres) et tests de contrat.

**Critère de sortie** : types générés, migrations testées, packs installables et vérifiables, tests de contrat verts.

### T2 — Pipeline vision (L)

- [ ] **T2.1** Assemblage de la bande virtuelle et découpe en tuiles aux **lignes uniformes** (espaces entre cases), avec fusion des détections aux jonctions. Tests sur des images **synthétiques** générées par script.
- [ ] **T2.2** Détection et segmentation des bulles et du texte, avec masque précis de l'intérieur.
- [ ] **T2.3** Classification des blocs : dialogue, narration, sfx, texte libre, à ignorer (filigranes, crédits, logos).
- [ ] **T2.4** OCR de l'anglais, regroupement des lignes d'une même bulle, ordre de lecture (haut vers bas, gauche vers droite).
- [ ] **T2.5** Effacement du texte dans les bulles de dialogue et de narration ; politique pour le texte hors bulles selon l'ADR 0010.
- [ ] **T2.6** Contrôle de résidu : second passage d'OCR sur la zone nettoyée, qui doit ne rien trouver. Sinon, le bloc passe en `review`.
- [ ] **T2.7** Non-régression contre les mesures de T0, budget mémoire respecté sur la machine cible.

**Critère de sortie** : sur le corpus, détection et OCR au niveau des seuils validés en T0, résidu d'anglais sous le seuil, aucune tuile ne dépasse le budget mémoire.

### T3 — Traduction (M)

- [ ] **T3.1** Trait `Translator` côté Rust. Fournisseurs : LLM en ligne (clé de l'utilisateur dans le trousseau de l'OS) et modèle local si l'ADR 0008 le retient.
- [ ] **T3.2** Prompt par page : tous les blocs dans l'ordre de lecture, contexte, glossaire, **budget de caractères par bulle**. Sortie en JSON strict, validée par schéma. Le nombre de blocs en sortie doit égaler le nombre en entrée ; en cas d'écart, nouvel essai puis repli. Jamais de blocs inventés.
- [ ] **T3.3** Glossaire par série (SQLite) : noms propres, termes récurrents, registre (tutoiement ou vouvoiement). Interface minimale, injection dans les prompts, cohérence entre chapitres.
- [ ] **T3.4** Boucle de raccourcissement : si la mise en page échoue, redemander une version plus courte avec le budget exact, avec un nombre maximal d'essais.
- [ ] **T3.5** Résilience : file d'attente, backoff, gestion des quotas et du mode hors ligne.
- [ ] **T3.6** Confidentialité : afficher ce qui est envoyé au fournisseur (le texte, et l'image **seulement** si l'utilisateur l'active), consentement explicite, aucune clé ni texte dans les journaux.

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
