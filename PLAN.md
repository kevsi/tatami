# PLAN.md — Tatami (nom provisoire)

> Feuille de route vivante. L'agent la met à jour selon les règles de `AGENTS.md` (section 12).
> Créé le 2026-10-02. Tailles relatives : S, M, L (pas des durées).

---

## 1. Vision

Un client desktop qui fait sur PC tout ce que fait l'app mobile **Kotatsu-Redo** (lecteur de manga Android open source, https://github.com/Kotatsu-Redo/Kotatsu-Redo), en réutilisant ses sources via la librairie `kotatsu-parsers-redo` (Kotlin/JVM, 1200+ sources).

Principe directeur : **séparer ce qui change tout le temps (les sources) de ce qui doit rester stable (l'app et les données)**. Les sites cassent chaque semaine ; le moteur de sources doit pouvoir être mis à jour sans republier l'app.

## 2. Architecture

```
Interface (Next.js export statique, dans la webview Tauri)
        │  commandes / événements Tauri
        ▼
Cœur Rust (Tauri v2)
   ├─ Données        SQLite (bibliothèque, favoris, historique, signets)
   ├─ Hors ligne     téléchargements, CBZ
   ├─ Planificateur  mises à jour de chapitres, notifications
   └─ Sync / suivi   AniList, MAL, Shikimori, Kitsu, sync entre appareils
        │  contrat versionné (NDJSON sur stdio)
        ▼
Moteur de sources (sidecar JVM) ── kotatsu-parsers-redo ──► sites de manga
```

Structure du dépôt :

```
tatami/
  AGENTS.md  PLAN.md  README.md  LICENSE (GPLv3)
  apps/desktop/    Next.js (export statique) + src-tauri (Rust)
  engine/          Projet Gradle Kotlin : le sidecar
  contract/        Schéma JSON partagé -> types Rust et TypeScript
  scripts/         Scripts PowerShell de build (jlink, packaging)
  docs/            notes/, decisions/ (ADR), smoke-test.md, spike-phase0.md
```

## 3. Périmètre

**Dans la v1** : sources, recherche et filtres ; bibliothèque, favoris, catégories, historique, signets ; lecteur paginé et webtoon ; téléchargements et CBZ ; notifications de nouveaux chapitres ; suivi AniList/MAL/Shikimori/Kitsu ; sync ; import d'un backup mobile.

**Hors v1** (ne pas faire sans validation) : recommandations, verrou de l'app, builds macOS/Linux (Windows d'abord), tout nouveau service de suivi, thèmes avancés.

## 4. Statut des phases

| Phase | Titre | Taille | Statut |
|---|---|---|---|
| 0 | Fondations et spike des risques | M | À faire |
| 1 | Bibliothèque et lecteur | L | À faire |
| 2 | Hors ligne | M | À faire |
| 3 | Tâches de fond et notifications | M | À faire |
| 4 | Comptes et synchronisation | L | À faire |
| 5 | Durcissement et distribution | M | À faire |

Phase en cours : **0**. Prochaine tâche : **0.2**.

---

## 5. Phases détaillées

### Phase 0 — Fondations et spike des risques (M)

Objectif : prouver que les trois risques majeurs sont maîtrisables avant de construire le reste (sidecar JVM empaquetable, images servies correctement, sources protégées par Cloudflare).

- [x] **0.1** Initialiser le dépôt : git, workspace pnpm, `LICENSE` (GPLv3), `README.md` avec crédits (Kotatsu, Kotatsu-Redo, kotatsu-parsers-redo) et avertissement (l'app est un navigateur, elle n'héberge aucun contenu).
- [ ] **0.2** Vérifier l'outillage (`java -version`, `gradle -v`, `rustc --version`, `node -v`, `pnpm -v`) et installer ce qui manque (voir `AGENTS.md` section 3). Consigner les versions dans le journal.
- [ ] **0.3** Lire le code de `kotatsu-parsers-redo` et de l'implémentation non-Android de `kotatsu-dl`. Écrire `docs/notes/loader-context.md` : méthodes de `MangaLoaderContext` à implémenter, celles qui posent problème sur desktop (exécution de JavaScript, recomposition d'images), coordonnées de dépendance **exactes** (À VÉRIFIER dans le README du dépôt, ne pas les deviner).
- [ ] **0.4** Créer `engine/` (Gradle Kotlin) avec la dépendance sur une version **épinglée** des parsers. CLI de test : lister les sources, rechercher dans une source. Vérifier avec une source ouverte (une API officielle comme MangaDex est un bon candidat).
- [ ] **0.5** Rédiger `contract/` v0 (schéma JSON : `hello`, `listSources`, `search`, `details`, `chapters`, `pages`, `fetchImage`, `setCookies`, `health`, `shutdown`, erreurs typées) et l'ADR `0001-transport-sidecar.md` (NDJSON sur stdio vs HTTP local avec jeton, avec compromis).
- [ ] **0.6** Implémenter le mode serveur du moteur selon le contrat (stdout = protocole, stderr = logs). Test d'intégration `hello` → `listSources` → `search`.
- [ ] **0.7** Empaqueter un runtime réduit avec `jdeps` + `jlink` (script `scripts/build-engine.ps1`). Mesurer et noter : taille du runtime, temps de démarrage à froid, mémoire au repos et en usage (viser un tas limité, par ex. `-Xmx256m`, GC série, à valider par mesure).
- [ ] **0.8** Scaffolder Tauri v2 + Next.js (export statique). Déclarer le sidecar (`externalBin`, plugin shell : vérifier la configuration et les permissions dans la doc Tauri v2). Superviseur minimal : démarrage, détection de crash, relance.
- [ ] **0.9** Commandes Tauri `search` / `chapters` / `pages` + page d'interface minimale qui affiche une liste de résultats.
- [ ] **0.10** Protocole d'URI personnalisé `tatami://page/...` : Rust demande `fetchImage` au moteur, met en cache sur disque, sert l'image à la webview (vérifier l'API Tauri v2 des protocoles asynchrones dans la doc). Afficher une page de chapitre dans l'interface.
- [ ] **0.11** Source protégée (Cloudflare/captcha) : si le moteur renvoie `CHALLENGE_REQUIRED`, ouvrir une fenêtre webview, laisser l'utilisateur résoudre, récupérer cookies et user-agent (vérifier ce que l'API de Tauri v2 permet pour les cookies), les transmettre via `setCookies`, rejouer la requête. Documenter les limites observées (liaison à l'IP, expiration).
- [ ] **0.12** Écrire `docs/spike-phase0.md` : mesures (taille, démarrage, RAM), ce qui marche, ce qui bloque, décisions, ajustements proposés au plan. Créer `docs/smoke-test.md`. Figer et mettre à jour les commandes de vérification dans `AGENTS.md`.

**Critère de sortie** : lire un chapitre complet d'une source ouverte dans l'interface Tauri avec le moteur empaqueté, et un résultat documenté (succès ou limites claires) pour une source protégée.

### Phase 1 — Bibliothèque et lecteur (L)

Objectif : pouvoir remplacer le mobile pour lire en ligne.

- [ ] **1.1** SQLite côté Rust, système de migrations forward-only, sauvegarde avant migration, tests de migration.
- [ ] **1.2** Catalogue de sources : liste, activation/désactivation, langue, état (alimenté par `health`).
- [ ] **1.3** Recherche et filtres (nom, genres, tri) avec pagination.
- [ ] **1.4** Page détail d'un manga : infos, liste des chapitres, ajout aux favoris.
- [ ] **1.5** Favoris organisés en catégories définies par l'utilisateur.
- [ ] **1.6** Historique de lecture, reprise au bon chapitre et à la bonne page, mode incognito.
- [ ] **1.7** Signets.
- [ ] **1.8** Lecteur paginé : sens de lecture, zoom, préchargement via le cache Rust, raccourcis clavier et souris.
- [ ] **1.9** Mode webtoon (défilement continu, liste virtualisée).
- [ ] **1.10** Thèmes clair/sombre, i18n français et anglais en place.

**Critère de sortie** : recherche → ajout en favori → lecture → fermeture → reprise exacte à la réouverture, sur au moins 3 sources différentes, avec tests passants.

### Phase 2 — Hors ligne (M)

- [ ] **2.1** Gestionnaire de téléchargements : file d'attente, reprise après interruption, concurrence limitée, débit limité par source.
- [ ] **2.2** Stockage CBZ ou dossier, chemin en base, noms de fichiers assainis.
- [ ] **2.3** Lecture hors ligne des chapitres téléchargés.
- [ ] **2.4** Import de CBZ tiers et lecture locale ; export d'un chapitre en CBZ.
- [ ] **2.5** Gestion de l'espace : taille utilisée, suppression, emplacement configurable.

**Critère de sortie** : télécharger un chapitre, couper le réseau, le lire ; reprendre un téléchargement interrompu ; importer un CBZ externe.

### Phase 3 — Tâches de fond et notifications (M)

- [ ] **3.1** Planificateur Rust (tokio) : vérification périodique des nouveaux chapitres des favoris, avec limites de débit.
- [ ] **3.2** Flux "mises à jour" dans l'interface.
- [ ] **3.3** Notifications système (plugin Tauri, à vérifier dans la doc).
- [ ] **3.4** Icône dans la zone de notification, fermeture dans la zone de notification, démarrage automatique optionnel.
- [ ] **3.5** Réglages : fréquence, sources concernées, activation.

**Critère de sortie** : un nouveau chapitre (simulé ou réel) déclenche une notification pendant que l'app est réduite dans la zone de notification.

### Phase 4 — Comptes et synchronisation (L)

- [ ] **4.1** Étude (docs/notes/) : protocole de `kotatsu-syncserver` (dépôt archivé, GPL-3.0) et format du backup mobile. Ne rien implémenter avant d'avoir lu le code.
- [ ] **4.2** Suivi AniList (OAuth avec redirection locale ou deep link, à vérifier), stockage des jetons dans le trousseau OS.
- [ ] **4.3** Suivi MyAnimeList, puis Shikimori, puis Kitsu.
- [ ] **4.4** Synchronisation de la progression sur chaque chapitre lu.
- [ ] **4.5** Client de sync compatible avec le serveur de sync de Kotatsu (si l'étude 4.1 montre que c'est faisable) ; sinon ADR avec alternative.
- [ ] **4.6** Import du backup mobile (favoris, catégories, historique).

**Critère de sortie** : suivi mis à jour après lecture, import d'un backup mobile réel sans perte de favoris ni d'historique, sync fonctionnelle entre l'app et un second appareil ou une instance de test.

### Phase 5 — Durcissement et distribution (M)

- [ ] **5.1** Mise à jour du moteur : manifeste signé, vérification de signature et de hash, remplacement atomique, retour arrière si échec au démarrage.
- [ ] **5.2** Mise à jour de l'app via l'updater Tauri.
- [ ] **5.3** Tests de contrat sur réponses enregistrées, exécutés en intégration continue.
- [ ] **5.4** Écran "état des sources" (dernière réussite, dernière erreur, version du parser) pour repérer ce qui est cassé.
- [ ] **5.5** Journaux de diagnostic exportables, sans données personnelles.
- [ ] **5.6** Installeurs Windows (`.msi`), signature du code, documentation utilisateur et contributeur.
- [ ] **5.7** Politique de suivi de l'amont : procédure pour intégrer les nouvelles versions de `kotatsu-parsers-redo`, miroir/fork de secours.

**Critère de sortie** : installation propre sur une machine neuve, mise à jour du moteur sans republier l'app, suite de tests verte en CI.

---

## 6. Questions ouvertes / À VÉRIFIER

- [ ] Coordonnées de dépendance exactes de `kotatsu-parsers-redo` (README du dépôt).
- [ ] Ce que `MangaLoaderContext` exige (exécution de JS, recomposition d'images, config par source) et comment `kotatsu-dl` le gère.
- [ ] API Tauri v2 pour : sidecar `externalBin` + permissions, protocole d'URI asynchrone, cookies d'une webview, notifications, deep link / redirection locale OAuth, updater.
- [ ] Protocole de `kotatsu-syncserver` et format du backup mobile.
- [ ] Disponibilité du nom "Tatami" (GitHub, stores, domaine) ; alternatives : Irori, Mikan.
- [ ] Empreinte mémoire réelle du sidecar sur la machine cible (12 Go de RAM, outils de dev ouverts).

## 7. Risques

| Risque | Impact | Parade |
|---|---|---|
| Cloudflare/captchas sur certaines sources | Sources inutilisables | Fenêtre de résolution + cookies (0.11), documenter les limites |
| Parsers qui cassent souvent | Perte de confiance | Moteur mis à jour séparément (5.1), écran d'état (5.4) |
| Amont archivé (dépôts KotatsuApp) | Dépendance fragile | Fork/miroir (5.7), version épinglée |
| JVM lourde sur machine modeste | Lenteur | Runtime `jlink`, tas limité, mesures en 0.7 |
| Périmètre trop large pour un seul développeur | Épuisement, abandon | Phases courtes, critères de sortie, hors v1 explicite |
| Divergence de protocole de sync avec le mobile | Pas de sync | Étude 4.1 avant tout code, ADR |

## 8. Blocages

_Aucun pour l'instant. L'agent ajoute ici : date, tâche, description, ce qui a été essayé, option proposée._

## 9. Journal

_Format d'une entrée :_

```
### AAAA-MM-JJ — Tâche X.Y
- Plan : (3 à 6 lignes)
- Fait :
- Preuve : commande + résumé de la sortie
- Points ouverts :
```

### 2026-10-02 — Création
- Plan et directives créés. Nom provisoire "Tatami". Aucune tâche commencée.

### 2026-10-02 — Tâche 0.1
- Plan : (1) git init + identité locale → vérif rev-parse ; (2) LICENSE GPLv3 depuis gnu.org + README crédits/avertissement + workspace pnpm + .gitignore → vérif pnpm install ; (3) commit + cocher 0.1.
- Fait : dépôt init (root-commit `4c19a26`), 8 fichiers (`LICENSE`, `README.md`, `package.json` avec `packageManager pnpm@12.8.1`, `pnpm-workspace.yaml` sur `apps/*`, `pnpm-lock.yaml`, `.gitignore`, `AGENTS.md`, `PLAN.md`). Identité locale : kevsi roughi <kev@mail.com>.
- Preuve : `git log --oneline` → `4c19a26 chore: initialise le depot` ; `git ls-files` → 8 fichiers ; `pnpm install` → `Already up to date / Done in 754ms` ; `LICENSE` = 35149 octets, tête `GNU GENERAL PUBLIC LICENSE / Version 3, 29 June 2007`.
- Points ouverts : `packageManager` épinglé à la version constatée (12.8.1) ; structure `apps/desktop`, `engine/`, `contract/`, `scripts/`, `docs/` à créer dans les tâches suivantes (0.4–0.8).
