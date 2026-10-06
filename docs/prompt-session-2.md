# Session 2 — Phase T, étape T0 (audit et évaluation)

Tu travailles sur **Tatami**, un client desktop de lecture de manga (Tauri v2 +
Rust + interface Next.js statique + moteur de sources Kotlin/JVM), réalisé dans
`C:\Users\alexanders\Documents\Workspaces\Tatami`. Licence **GPLv3**.

## 1. Lis ces deux fichiers AVANT toute action

1. `AGENTS.md` — surtout **§2 (règles d'or)**, **§3 (environnement)**, **§8
   (commandes de vérification figées)** et **§14 (leçons de session)**. Ce
   dernier chapitre contient ~70 faits mesurés ; plusieurs s'appliquent
   directement à ton travail (mesurer avant de conclure, ne pas écrire une
   fixture à la main, etc.). Ne le survole pas.
2. `PLAN-TRADUCTION.md` — le plan de la phase dont tu t'occupes.

Le contexte projet (état réel des phases, ce qui reste à faire) est dans
`PLAN.md`, sections 4 et 4 bis.

## 2. Ta mission : T0 uniquement

Sept tâches, numérotées T0.1 à T0.7 dans `PLAN-TRADUCTION.md`. En résumé :

- **T0.1** — Audit des licences dans `docs/notes/licences-traduction.md` : pour
  chaque outil, modèle, détecteur, modèle d'OCR, modèle d'inpainting, modèle de
  traduction et police candidat : licence **du code**, licence **des poids**,
  restrictions d'usage, et redistribution possible dans un installeur GPLv3.
- **T0.2** — Constituer le corpus d'évaluation, **local uniquement**.
- **T0.3** — Définir protocole et métriques dans `docs/eval/protocole.md`, et
  **proposer** des seuils d'acceptation.
- **T0.4** — Exécuter comic-translate et au moins un second outil sur le corpus,
  consigner mesures et échecs typiques.
- **T0.5** — Comparer les moteurs de traduction (≥ 2 LLM en ligne, ≥ 1 modèle
  hors ligne, avec/sans glossaire, avec/sans budget de caractères).
- **T0.6** — Mesurer sur la machine cible : **CPU seul, 12 Go de RAM**.
- **T0.7** — Écrire `docs/spike-traduction.md` et les ADR **0007, 0008, 0009**.

Tu t'arrêtes à la **décision humaine GO / NO-GO / réorientation**. Tu proposes,
l'humain tranche. Ne commence pas T1.

## 3. Contraintes non négociables

- **Aucune œuvre dans le dépôt.** Le corpus, les captures et les images restent
  sur le disque de la machine. Seules des **descriptions sans image** sont
  versionnées.
- **Première action de la session : protéger le dépôt.** `docs/eval/` n'existe pas
  encore et le dossier du corpus **n'est pas dans `.gitignore`** (seul `.mesures/`
  y est). Ajoute les entrées nécessaires **avant** de télécharger quoi que ce
  soit, et vérifie avec `git status` qu'aucun fichier d'image n'apparaît.
- **Aucune modèle ni police n'entre dans le projet sans licence confirmée.**
  Si une licence n'est pas clairement établie, écris `À VÉRIFIER` — n'invente
  rien, ne conclus pas « probablement libre ».
- **Les clés d'API vont dans le trousseau ou une variable d'environnement**,
  jamais dans le dépôt ni dans un journal.
- **Le dépôt est propre et compilé au moment où tu t'arrêtes.** Ne laisse pas de
  travail non commité.

## 4. ADR : les numéros sont déjà réservés

`0007` runtime vision · `0008` stratégie de traduction · `0009` lieu de mise en
page · `0010` politique des textes hors bulles.

`0001` à `0006` sont **déjà pris** (transport du sidecar, schéma SQLite,
protocole images, permissions Tauri, suivi amont, mise à jour moteur). **Vérifie
`docs/decisions/` avant d'écrire**, et n'écrase rien.

## 5. Vérification avant de dire « fait »

Rien n'est déclaré terminé sans commande réellement exécutée. Les commandes
figées sont en `AGENTS.md` §8, **avec leur dossier** — la plupart échouent
depuis la racine :

```powershell
cd apps\desktop        # typecheck · lint · i18n:check · hooks:check · chemins:check · build
cd apps\desktop\src-tauri   # cargo fmt --check · cargo clippy --all-targets -D warnings · cargo test
cd engine              # gradle test   (PAS ./gradlew : il n'y a pas de gradlew.bat)
```

T0 ne touche pas au code de l'application, donc ces portes servent surtout à
prouver que tu n'as rien cassé.

## 6. Ce que tu ne fais pas

- Tu ne touches **ni à la phase 2, ni à la phase 3, ni à la phase 5** de
  `PLAN.md`. Le produit est livré jusqu'à la phase 3 ; 5.2 (updater) et 5.6
  (installeurs) sont volontairement laissés pour la fin.
- Tu n'implémentes **rien** de T1 à T5 (pas de sidecar vision, pas de pack de
  modèles, pas d'écran). T0 est une phase d'évaluation, sans intégration.
- Tu ne déplaces pas le plan de travail : T0 est déjà la première tâche.

## 7. Méthode attendue

Mesure avant de conclure, et écris la mesure dans le rapport. Un rapport de
session antérieur raconte le même genre de travail
(`docs/rapport-session-recherche.md`) — c'est un bon modèle de forme.

En cas de blocage, **dis-le et arrête-toi** : propose deux options avec leurs
compromis, comme le veut `AGENTS.md` §2. N'écarte aucun blocage en silence.