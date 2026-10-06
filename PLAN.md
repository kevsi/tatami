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

> **Exigence permanente (demandée par l'utilisateur le 2026-10-02) :** tout
> téléchargement doit être **reprenable**. Un webtoon entamé doit pouvoir être
> repris là où il s'est arrêté, sans jamais retélécharger ce qui est déjà
> présent — y compris après un redémarrage de l'application ou une coupure
> réseau. Un téléchargement non reprenable est un **défaut**, pas une limite.
> Critères de vérif : `missing_chapters` ne renvoie que l'absent (test
> unitaire), un `.part` compte comme absent, et l'interface expose « Reprendre »
> sur tout webtoon partiel.

**Dans la v1** : sources, recherche et filtres ; bibliothèque, favoris, catégories, historique, signets ; lecteur paginé et webtoon ; téléchargements et CBZ ; notifications de nouveaux chapitres ; suivi AniList/MAL/Shikimori/Kitsu ; sync ; import d'un backup mobile.

**Ordre v1 (décision humain, 2026-10-02)** : catalogue (1.1→1.7) puis téléchargements (phase 2, en bloc ou par sélection) puis lecteur (1.8/1.9) en dernier. La lecture est différée, pas supprimée.

**Hors v1** (ne pas faire sans validation) : recommandations, verrou de l'app, builds macOS/Linux (Windows d'abord), tout nouveau service de suivi, thèmes avancés.

## 4. Statut des phases

| Phase | Titre | Taille | Statut |
|---|---|---|---|
| 0 | Fondations et spike des risques | M | **En cours** — 0.1 à 0.11 faits, 0.12 partiel, 0.13 à 0.17 à faire |
| 1 | Bibliothèque et lecteur | L | **Finie** — 1.1 à 1.10, dont 1.8 et 1.10 livrés le 2026-10-05 |
| 2 | Hors ligne | M | **Faite** — file, reprise, CBZ, lecture hors ligne, assainissement et stats ; durcie le 2026-10-04 (un `.cbz` par chapitre, `sync_all`, verrou RAII) ; dossier de téléchargement au choix et pause de la file le 2026-10-05 |
| 3 | Tâches de fond et notifications | M | **Commencée** — 3.1 livré le 2026-10-05 (planificateur tokio, 19 tests) ; reste 3.2 à 3.5 |
| 4 | Comptes et synchronisation | L | **Pas commencée** |
| 5 | Durcissement et distribution | M | **Partiellement entamée** — audit de code du 2026-10-04 (5 correctifs) ; **5.3 livré le 2026-10-05** : 16 réponses du moteur enregistrées + test de contrat (20 tests) + CI qui exécute les 9 commandes de AGENTS.md §8 |

Phase en cours : **1 et 2 en parallèle**, la phase 2 ayant un manque bloquant.
Prochaine tâche : 1.10 (thèmes clair/sombre), puis la dette de phase 0 (0.13 → 0.17).

**Correction du 2026-10-04** : le tableau disait « phase 2 pas commencée ». C'était
faux, et c'est vérifiable dans le code — `download.rs` (file séquentielle,
`missing_chapters` pour la reprise, `clean_partials`, `sanitize`), `cbz.rs`
(écriture ZIP sans dépendance), `library.rs` (`existing_chapters`,
`remove_download`, tailles, `meta.json`). Un inventaire doit être **lu dans le
code**, jamais déduit de l'absence de cases cochées.

---

## 5. Phases détaillées

### Phase 0 — Fondations et spike des risques (M)

Objectif : prouver que les trois risques majeurs sont maîtrisables avant de construire le reste (sidecar JVM empaquetable, images servies correctement, sources protégées par Cloudflare).

- [x] **0.1** Initialiser le dépôt : git, workspace pnpm, `LICENSE` (GPLv3), `README.md` avec crédits (Kotatsu, Kotatsu-Redo, kotatsu-parsers-redo) et avertissement (l'app est un navigateur, elle n'héberge aucun contenu).
- [x] **0.2** Vérifier l'outillage (`java -version`, `gradle -v`, `rustc --version`, `node -v`, `pnpm -v`) et installer ce qui manque (voir `AGENTS.md` section 3). Consigner les versions dans le journal.
- [x] **0.3** Lire le code de `kotatsu-parsers-redo` et de l'implémentation non-Android de `kotatsu-dl`. Écrire `docs/notes/loader-context.md` : méthodes de `MangaLoaderContext` à implémenter, celles qui posent problème sur desktop (exécution de JavaScript, recomposition d'images), coordonnées de dépendance **exactes** (À VÉRIFIER dans le README du dépôt, ne pas les deviner).
- [x] **0.4** Créer `engine/` (Gradle Kotlin) avec la dépendance sur une version **épinglée** des parsers. CLI de test : lister les sources, rechercher dans une source. Vérifier avec une source ouverte (une API officielle comme MangaDex est un bon candidat).
- [x] **0.5** Rédiger `contract/` v0 (schéma JSON : `hello`, `listSources`, `search`, `details`, `chapters`, `pages`, `fetchImage`, `setCookies`, `health`, `shutdown`, erreurs typées) et l'ADR `0001-transport-sidecar.md` (NDJSON sur stdio vs HTTP local avec jeton, avec compromis).
- [x] **0.6** Implémenter le mode serveur du moteur selon le contrat (stdout = protocole, stderr = logs). Test d'intégration `hello` → `listSources` → `search`.
- [x] **0.7** Empaqueter un runtime réduit avec `jdeps` + `jlink` (script `scripts/build-engine.ps1`). Mesurer et noter : taille du runtime, temps de démarrage à froid, mémoire au repos et en usage (viser un tas limité, par ex. `-Xmx256m`, GC série, à valider par mesure).
- [x] **0.8** Scaffolder Tauri v2 + Next.js (export statique). Déclarer le sidecar (`externalBin`, plugin shell : vérifier la configuration et les permissions dans la doc Tauri v2). Superviseur minimal : démarrage, détection de crash, relance.
- [x] **0.9** Commandes Tauri `search` / `chapters` / `pages` + page d'interface minimale qui affiche une liste de résultats.
- [x] **0.10** Protocole d'URI personnalisé `tatami://page/...` : Rust demande `fetchImage` au moteur, met en cache sur disque, sert l'image à la webview (vérifier l'API Tauri v2 des protocoles asynchrones dans la doc). Afficher une page de chapitre dans l'interface.
- [x] **0.11** Source protégée (Cloudflare/captcha) : si le moteur renvoie `CHALLENGE_REQUIRED`, ouvrir une fenêtre webview, laisser l'utilisateur résoudre, récupérer cookies et user-agent (vérifier ce que l'API de Tauri v2 permet pour les cookies), les transmettre via `setCookies`, rejouer la requête. Documenter les limites observées (liaison à l'IP, expiration).
- [x] **0.12** **Livré** le 2026-10-05. `docs/spike-phase0.md` écrit, `docs/smoke-test.md` déjà livré, commandes de vérification figées dans `AGENTS.md` §8. Première mesure sérieuse du moteur (jamais faite) : bannière 880 ms, `hello` 104 ms, `listSources` 180 ms pour 1 370 sources, arrêt propre 1 208 ms ; RAM 68 Mo au démarrage, 136 Mo après le premier appel réseau puis plate — `-Xmx256m` serré mais tenable. **Trouvé en passant** : `gradlew test` était une commande **fictive** (§2 le prohibait) — pas de `gradlew.bat` dans le dépôt ; et le cache d'images est borné en **nombre** d'entrées (2048) mais pas en **volume**, soit ~521 Mo possibles pour 260 Ko de moyenne sur une machine de 12 Go. Ce plafond rejoint la tâche 2.5.
- [x] **0.13 (nouveau)** Contrat : ajouter un code d'erreur `AUTH_REQUIRED`. Mesuré : `AuthRequiredException` n'est levée que sur HTTP 401 par 16 parseurs, arrive aujourd'hui comme `INTERNAL`, et ne peut pas correspondre à `CHALLENGE_REQUIRED` (qui exige 403/503 + marqueur Cloudflare). **Livré** : `AUTH_REQUIRED` au contrat, déballage de l'exception dans le moteur, message i18n dédié, non transitoire côté superviseur.
- [x] **0.14 (nouveau)** Moteur : ne pas perdre `CHALLENGE_REQUIRED` quand le parser emballe l'exception. Mesuré : MANGANATO et KAGANE renvoient `INTERNAL: IOException: canceled due to CloudFlareProtectionException` — un vrai challenge Cloudflare rapporté comme panne interne, donc la fenêtre de résolution de 0.11 ne s'ouvrirait pas. **Livré** : `Erreurs.kt` déballe la chaîne de causes *et* le message (OkHttp 5 KMP perd parfois la cause). Mesuré : les deux sources renvoient `CHALLENGE_REQUIRED` + URL.
- [x] **0.15 (nouveau)** Superviseur : envoyer `shutdown` avant de fermer stdin. Mesuré : fermer stdin ne tue pas le moteur (process encore vivant 4 s plus tard) ; `shutdown` donne `exit 0` en 137 ms. Garder le `kill` en secours. **Livré** dans le correctif du 2026-10-04 : `workers.shutdown()` + compteur de requêtes en vol au lieu de `shutdownNow()`.
- [x] **0.16** 3 sources sur 31 échouent en `PKIX path building failed`. **Résolu le 2026-10-05 : la cause n'est PAS la nôtre, et aucune action de sécurité n'est justifiée.** Le diagnostic d'origine (le runtime `jlink`) était faux, puis le diagnostic suivant (« il nous manque `Root YE` ») était **lui aussi faux** — mais il se corrigeait par une décision qui élargissait la confiance du système. Ce que les mesures montrent :
  1. **Un JDK plus récent n'aiderait pas.** `Root YE` n'est dans **aucun** magasin : la documentation Let's Encrypt dit « These roots are not yet included in Root Program Trust Stores » et « Chains which terminate at Root YE or Root YR are not expected to work with any of the major trust stores ». Rien ne la contiendra tant qu'elle n'est pas adoptée.
  2. **La théorie est invalidée par un témoin.** Même code, même magasin, même jour : MANGADEX (`YE1`) et SEIMANGA (`YE2`) sont **validés**, MANGAHAUS (`YE2`) est **refusé** avec `UNABLE_TO_GET_ISSUER_CERT_LOCALLY`. Chaîne identique, résultats opposés → la cause est propre au serveur, pas à notre magasin.
  3. **Deux des trois sources ne sont plus atteignables** : `mangalesen.com` et `mangasoriginesunofficial.com` ne résolvent pas en DNS depuis cette machine (`ENOTFOUND`). Pour elles, la cause est **inconnue** — pas « le TLS ».
  - **Décision : ne rien ajouter au magasin de confiance.** Ajouter `Root YE` n'aurait débloqué aucune des trois sources, puisque deux sont mortes et que la troisième se distingue par sa propre configuration serveur — et cela aurait élargi la confiance du système sans bénéfice. `AGENTS.md` §10 reste respecté : jamais de désactivation de la validation TLS.
  - **Reste à faire** : comprendre pourquoi MANGAHAUS se distingue (proxy qui ne sert que la feuille, ou chaîne croisée manquante). Ce n'est **pas** corrigible chez nous : le seul correctif côté client serait de ne pas vérifier — interdit.
- [x] **0.17 (nouveau)** ZENMANGA servait du chiffrement, pas des images. **Mesuré** : 832 513 octets en `application/octet-stream`, dont les **16 premiers octets identiques sur deux pages différentes** (`AA A1 AC 8B 70 20 08 3C 19 2E 77 6D 77 50 32 57`) — ni JPEG, ni WEBP, ni PNG, ni HTML. Le parser porte `ImageCodec { SEC, XOR }` et `detectImageCodec(url)` ; le déchiffrement se fait dans `MangaParser.intercept`, **jamais appelé**. **Livré** : `ParserActif` (ThreadLocal) + `intercepteurParser`, ajouté **en dernier** dans la chaîne (le plus proche du réseau) pour transformer la réponse avant que CloudFlare et le lazy-load ne la voient. Résultat : les mêmes octets commencent par `FF D8 FF E0`, un JPEG valide. **Piège** : `ParserPool.avec` n'est pas réentrant (Mutex) — un appel imbriqué sur la même source fige le moteur.
- [x] **0.19 (nouveau)** La fenêtre de vérification s'ouvre quand le moteur ne signale **aucun** challenge. Deux déclencheurs côté Rust (URL d'image vide, octets non-image), fenêtre ouverte sur `manga.publicUrl`, panneau dans la vue Téléchargements, 9 clés i18n. Walk 17/17, contrôle négatif (0 `.cbz` écrit) et contrôle positif (`.cbz` MANGADEX relu par un lecteur strict). **Résolu en 0.20** : sur HENTAIORIGINES le cookie n'était pas la cause, et le panneau était invisible depuis l'onglet Catalogues.
- [x] **0.20 (nouveau)** Images lazy-loadées illisibles par les parsers : l'URL est dans `data-src` et non `src`. `LazyLoadInterceptor` côté moteur (9 tests), garde `fetchImage` sur URL vide, `testImplementation(kotlin("test"))` — le moteur n'avait aucun test. **Résultat mesuré** : 45 URL remplies, `.cbz` de 9,3 Mo relu par un lecteur strict (45 entrées, 45 WEBP, 45 tailles distinctes), MANGADEX non régressé, aucune fenêtre de vérification. Résiduel : `032.webp` = placeholder 413×42 servi par le site.
- [x] **0.18 (nouveau)** Un chapitre sans titre (`title: null`) donne un `.cbz` nommé `22d20970-38d1-48aa-bbd3-9ca9afbbb3dc.cbz` (mesuré sur MANGADEX). Le fichier est valide mais inutilisable dans un lecteur externe, ce qui vide l'exigence permanente « un `.cbz` par chapitre ». Corriger `chapterLabel` / le nom de fichier : retomber sur un numéro de chapitre lisible (« Chapitre 1.cbz »), jamais sur l'URL ni un UUID. Vérifier aussi l'assainissement du nom (AGENTS.md §6).

### Phase 0 bis — Adaptation de l'interface d'après l'analyse de sushiscan.net (M)

Origine : analyse de structure et de style de `sushiscan.net` (WordPress + thème custom dérivé Madara, LiteSpeed + Cloudflare, 4 écrans, lecteur à 4 contrôles). Demande humaine : 5 chantiers retenus le 2026-10-03. Ces tâches affinent l'interface déjà livrée en 0.9–0.11 ; elles ne remplacent pas la phase 1.

- [x] **0.13** Lecteur : ajustement à la largeur ou à la hauteur, double page, sens de lecture. Critères : `Par défaut` / `Ajusté à la hauteur` → vérifié par `getComputedStyle(img).width/height` distincts entre les deux modes ; double page → vérifié par 2 `<img>` dans `.reader-stage` et un pas de 2 pages ; RTL par défaut.
- [x] **0.14** Lecteur : sélecteur de chapitre + saut de page (`n/N` cliquable). Critères : `<select>` de chapitres dans `.reader-bar` × `aria-label="Changer de chapitre"` ; champ numérique × `aria-label="Aller à la page"` qui positionne l'index.
- [x] **0.15** Fiche série : chapitres en grille 5 colonnes (numéro + date de publication) + filtre par numéro/titre. Critères : `.chapter-grid` à `repeat(auto-fill, minmax(...))` avec ≥ 5 colonnes à 1440 px ; date affichée → vérifié par sonde moteurconfirmant `uploadDate` non nul ; filtre → vérifié par capture.
- [x] **0.16** Catalogue : derniers chapitres dans la carte (comme « Ch. 392 · 5 mois »). Critères : concurrence bornée (4), cache localStorage 30 min, interrupteur on/off, compteur `x/y` chargés. Limite assumée : 1 requête `chapters` par carte affichée.
- [x] **0.17** Style : bandeaux de section, table d'informations 2 colonnes sur la fiche série, badges. La table exige la commande Rust `details` + `mangaToJson` étendu (`description`, `state`, `tags`) → rebuild du moteur + redémarrage de l'app. Critères : `cargo clippy -D warnings` 0, `gradle installDist` OK, table rendue avec ≥ 1 ligne.

**Critère de sortie** : lire un chapitre complet d'une source ouverte dans l'interface Tauri avec le moteur empaqueté, et un résultat documenté (succès ou limites claires) pour une source protégée.

### Phase 1 — Bibliothèque et lecteur (L)

Objectif : pouvoir remplacer le mobile pour lire en ligne.

- [x] **1.1** SQLite côté Rust, système de migrations forward-only, sauvegarde avant migration, tests de migration.
- [x] **1.2** Catalogue de sources : liste, activation/désactivation, langue, état (alimenté par `health`).
- [x] **1.3** Recherche et filtres (nom, genres, tri) avec pagination.
- [x] **1.4** Page détail d'un manga : infos, liste des chapitres, ajout aux favoris.
- [x] **1.5** Favoris organisés en catégories définies par l'utilisateur.
- [x] **1.6** Historique de lecture, reprise au bon chapitre et à la bonne page, mode incognito.
- [x] **1.7** Signets.
- [x] **1.8** Lecteur paginé : sens de lecture, zoom, préchargement via le cache Rust, raccourcis clavier et souris. **Livré** le 2026-10-05. Sens de lecture, zoom, double page et RTL étaient déjà faits ; il restait deux choses, et **les deux n'étaient pas ce qu'elles semblaient être**. La précharge fonctionnait (elle écrit bien dans le cache Rust) mais ne couvrait qu'une page → désormais 4. La molette ne faisait rien en mode paginé → ajoutée, avec seuil de 40 px et verrou de 300 ms par sens (un geste émet 43 événements : mesuré).
- [x] **1.9** Mode webtoon (défilement continu, liste virtualisée).
- [x] **1.10** Thèmes clair/sombre, i18n français et anglais en place. **Livré** le 2026-10-05 : l'i18n existait déjà (152 clés) ; le thème clair, non. Le travail réel n'était pas d'ajouter une palette mais de **tokeniser les 69 couleurs écrites en dur** hors tokens — sans quoi rien n'aurait basculé. `--reader-bg` reste identique dans les deux thèmes (pages blanches, un fond clair ferait disparaître leurs bords).

**Critère de sortie** : recherche → ajout en favori → lecture → fermeture → reprise exacte à la réouverture, sur au moins 3 sources différentes, avec tests passants.

### Phase 2 — Hors ligne (M)

- [x] **2.1** Gestionnaire de téléchargements : file d'attente, reprise après interruption, concurrence limitée. *(fait sauf « débit limité par source » : la file est séquentielle globalement, ce qui est plus strict qu'un débit mais pas équivalent)*
- [x] **2.2** Stockage CBZ, noms de fichiers assainis. *(fait sauf « chemin en base » : le chemin est dérivé de `downloads_root` + source + titre et un `meta.json` fait foi — cf. dérogation signalée au journal, à trancher)*
- [x] **2.3** Lecture hors ligne des chapitres téléchargés. **Bloquant** : `cbz.rs` sait écrire un ZIP mais rien ne sait le lire, le lecteur passe toujours par le moteur.
- [x] **2.4** Import de CBZ tiers et lecture locale. **Livré et vérifié** le 2026-10-05, **import seul** (l'export reste hors périmètre, décision de début de session). Une commande `import_webtoon` : copie les `.cbz` dans `downloads_root/IMPORT/<titre>/`, écrit le `meta.json`, et refuse — avec un motif — tout fichier illisible. L'import **copie** au lieu de pointer, parce que `local_file_pages` refuse par construction tout chemin hors du dossier de téléchargements : un import qui enregistrerait le chemin d'origine serait illisible. La source `IMPORT` est synthétique et son `url` est vide, donc « retélécharger les chapitres manquants » ne part jamais vers un site qui ne les a pas eus.
- [x] **2.5** Gestion de l'espace : taille utilisée, suppression. *(emplacement configurable pas fait)*

**Critère de sortie** : télécharger un chapitre, couper le réseau, le lire ; reprendre un téléchargement interrompu ; importer un CBZ externe.

### Phase 3 — Tâches de fond et notifications (M)

- [x] **3.1** Planificateur Rust (tokio) : vérification périodique des nouveaux chapitres des favoris, avec limites de débit. **Livré** le 2026-10-05 : `scheduler.rs` + 19 tests, horloge et moteur injectés, débit par source avec retour arrière borné, état persisté en base, évènement `maj://nouveaux-chapitres`. Aucune dépendance ajoutée, aucun écran (3.2 décrit dans le journal, non implémenté). **Point ouvert mesuré le 2026-10-05 (session 1) :** le pire cas calculé de la passe est 20 sources × 20 s = **400 s**, et elle part **au démarrage**, en concurrence avec l'interface — alors que le moteur est séquentiel. Mesuré : `EXHENTAI` (la source qui « scanne tout son catalogue ») répond en **1 339 ms**, et une source rapide pendant/après répond en **620 ms** — donc les 20 s sont un **délai de politesse**, pas une durée d'appel, et le pire cas n'est pas 400 s de travail réel. **Décision : ne rien changer au code.** Le risque restant est `N × (durée réelle d'un appel)` avec un `timeoutSecs` de 60 s, à re-mesurer quand la bibliothèque comptera des centaines de favoris.
- [x] **3.2** Flux « mises à jour » dans l'interface. **Livré et vérifié** le 2026-10-05. Écoute `maj://nouveaux-chapitres` (le planificateur est son seul producteur, et il n'émet **rien** quand il n'y a rien de neuf), fusionne par `(source, manga_url)` en cumulant les totaux — deux passes du même favori donnent des chapitres **différents**, pas les mêmes comptés deux fois. Persisté dans `localStorage` (plafonné à 200 entrées), donc visible après un redémarrage. **Le piège documenté par le moteur est vérifié explicitement** : un cas tronqué (`total` 47, 3 chapitres listés) affiche bien « 47 nouveaux chapitres » et « et 44 de plus » — donc l'écran lit `total`, jamais `chapitres.length`. Preuve : 2 entrées rendues, fusion 47 + 2 → 49 sans doublon, persistance vérifiée après rechargement, 0 erreur console, et les **singuliers** vérifiés à part dans les deux langues (1 mise / 1 chapitre / 1 autre). Ouvre la fiche par `loadChapters`, qui pose déjà `setOpenManga` et appelle `details` + `chapters` — un doublon écrit d'abord invoices un `setChapitresCharges` inexistant, que le compilateur a signalé.
- [x] **3.3** Notifications système. **Livré et vérifié** le 2026-10-05. Une alerte par passage de la veille, jamais une par chapitre ; silencieuse tant que la fenêtre a le focus ; réglage dans l'écran Réglages ; texte dans les deux langues. **Ce que la doc a révélé** : sur desktop, `sendNotification` du plugin ne passe **pas** par l'IPC — c'est `new window.Notification(...)` (l'API Web Notifications de WebView2). Le plugin Rust n'intervient que pour la **requête** de permission, et seulement quand `window.Notification.permission === 'default'`. Conséquence appliquée : `notification:allow-notify` a été **retiré** de la capability, mobile étant hors v1 (AGENTS.md §7).
- [x] **3.4** Icône dans la zone de notification, fermeture dans la zone de notification, démarrage automatique optionnel. **Livré et vérifié** le 2026-10-05. Le tray est créé par **Rust** dans `setup()`, pas par l'interface : « fermer met dans la zone » et « l'icône existe » sont le même geste, donc une icône dépendant de la webview rendrait l'application **injoignable**. Les libellés du menu viennent de l'interface via `set_libelles`, qui **renvoie les textes relus depuis le menu** — ce qui donne la preuve sans ajouter `core:menu:allow-get-text`. Démarrage automatique = plugin officiel, état **relu depuis le système** après chaque écriture.
- [x] **3.5** Réglages : fréquence, sources concernées, activation. **Livré et vérifié** le 2026-10-05. Deux commandes Tauri dans `scheduler.rs` (`reglages_planificateur`, `set_reglages_planificateur`) : le Rust avait déjà `Reglages` avec `bornes()`, et le commentaire sur le champ `actif` attendait explicitement cet écran. Écriture **partielle** — chaque paramètre est un `Option`, donc un champ omis garde sa valeur, mesuré : changer la période laisse les trois autres intacts. Les bornes affichées (`min`/`max`) **voyagent depuis Rust**, donc l'écran ne peut pas proposer une valeur que le moteur corrigerait en silence. Affiche aussi `dernier_pass`, parce que « actif » ne prouve rien : une veille active qui n'a pas tourné depuis trois jours semble active.

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
- [x] **5.3** Tests de contrat sur réponses enregistrées, exécutés en intégration continue.
- [x] **5.4** Écran « état des sources » pour repérer ce qui est cassé. **Livré** le 2026-10-05 et **vérifié à l'écran**. Ce qui existait : un compteur « N sources signalées » et un bouton « Tout réessayer » — pas de liste, pas de date, donc impossible de savoir *quelle* source casse ni *depuis quand*. La donnée existait déjà (`noteHealth` enregistre `{code, message, at}` et persiste) : il n'y avait qu'à l'afficher. Regroupement par **code d'erreur** d'abord (3 sources en `CHALLENGE_REQUIRED` sont un problème Cloudflare, pas 3 pannes distinctes), puis le détail, du plus récent au plus ancien. `<time dateTime>` garde la date exploitable par une machine. **Corrigé au passage** : `sinceFr` était français en dur (contre `AGENTS.md` §7) — l'interface anglaise affichait « 3 mois ». Preuve : 7 pannes injectées, 4 chips de regroupement dont la somme vaut 7, 3 colonnes, 0 faute de français en mode `en`, bouton « Tout réessayer » ramène l'état à 0 et la table disparaît. **Non couvert** : la version du parser (exigée par la formulation d'origine) n'est pas affichée — elle vient du moteur et demanderait une commande Rust.
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
- [ ] **0.11 de bout en bout** : confirmer sur un vrai site protege que la fenetre de resolution + `setCookies` debloque la source. Aucune source de l'echantillon (MANGALIB, BAOZIMH) ne renvoie `CHALLENGE_REQUIRED` (403 classiques). Vigilance : clearance liee a l'IP et expirante, empreinte TLS WebView2 differente d'OkHttp.

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

## 8 bis. Travail réparti sur plusieurs sessions (depuis le 2026-10-04)

Décision de l'utilisateur : l'avancement est réparti sur **trois sessions OpenCode
sur le même dossier**. Contraintes mesurées qui ont dicté la répartition :

- **4 Go de RAM libres** sur 11,9 : une seule instance de `pnpm tauri dev`
  (next + cargo + JVM + WebView2) tient à peine ;
- ports **3000** et **9223** déjà pris par l'app de la session 1 ;
- `apps/desktop/app/page.tsx` fait ~3 000 lignes, un seul fichier : toute
  fonction le touche ;
- `SCHEMA_VERSION` est une constante unique dans `store.rs`.

Le **code n'est donc pas partageable**, le reste si.

| Session | Fichiers qu'elle seule peut modifier | Mission |
|---|---|---|
| **1 — code** | `apps/`, `engine/`, `AGENTS.md`, **`PLAN.md`**, **seule à commiter** | 1.5 catégories, 1.9 virtualisation webtoon, 1.10 thèmes + i18n |
| **2 — docs** | `docs/`, `README.md` | 0.12 `docs/smoke-test.md`, ADR de la §9, `README.md` |
| **3 — recherche** | `docs/notes/` uniquement | trancher les « À VÉRIFIER » par la mesure |

Règles données aux trois sessions :

1. **Jamais** de `pnpm dev`, `pnpm tauri dev`, `next dev`, `cargo`, `gradle`
   ailleurs que dans la session 1.
2. **Jamais** de commande git ailleurs que dans la session 1 : un `git add -A`
   prendrait les fichiers des autres dans le même commit.
3. **Jamais** de modification de `PLAN.md` ailleurs que dans la session 1 :
   c'est le tableau de coordination, il est recalculé à partir des sous-tâches.
4. La session 3 n'utilise **ni Playwright ni le port 9223** : elle mesurerait
   l'app pendant que la session 1 s'en sert. Elle parle au moteur seul en
   NDJSON.
5. Chaque session laisse son bilan dans `docs/rapport-session-*.md` ; la session 1
   récupère ces fichiers dans un commit « docs » dédié.

## 9. Journal

### 2026-10-05 — Import d'un dossier de webtoon (2.4)

- Plan : une commande Rust + un panneau dans l'écran Téléchargements. Le
  dialogue natif choisit un chemin, Rust décide du reste.
- **Copier, pas pointer.** `local_file_pages` refuse par construction tout
  chemin hors du dossier de téléchargements — c'est ce qui empêche la webview de
  faire lire n'importe quel fichier du disque. Un import qui se contentait
  d'enregistrer le chemin d'origine aurait donc produit une bibliothèque
  **illisible**. `source = "IMPORT"`, `url = ""` : source synthétique, donc
  aucune reprise de téléchargement ne part vers un site qui n'a jamais eu ces
  chapitres.
- **Un `.cbz` illisible est refusé avant toute écriture.** Un contrôle
  **négatif** en plus du positif : « absent du dossier de destination », pas
  seulement « absent du rapport » — un chapitre listé mais illisible est le
  pire résultat possible.
- **Le `meta.json` est relu sur le disque, pas accumulé pendant la copie.**
  Réimporter un dossier **plus petit** ne fait donc pas disparaître les
  chapitres d'un import précédent, qui sont toujours là.
- **Le nom écrit est celui que le lecteur recalcule.** `chapter_file_name`
  devient `pub(crate)` et l'import écrit exactement ce qu'il renvoie : un nom
  de fichier choisi à part produirait un chapitre listé et introuvable. Test
  d'aller-retour sur 14 noms, dont `.cbz`, `"   "`, `CON`, `Chapitre/12`.
- **Les motifs sont des codes, pas des phrases.** Mesuré : le rapport affichait
  « 4 chapters imported » puis « pas un fichier .cbz » — du français au milieu
  d'un écran anglais. `RefusCode` sérialisé en kebab-case + `detail` technique
  ; l'interface traduit via une table `Record<RefusCode, clé>`, donc un code
  ajouté côté Rust **ne compile pas** tant que sa traduction n'est pas
  branchée.
- **Trois bugs trouvés à la mesure, aucun visible au compilateur :**
  1. **Pluriel.** `1/1 chapitres` et `1/1 chapters`. La clé `compte` n'avait pas
     de singulier. Deux clés, et le singulier ne vaut que si **les deux**
     compteurs valent 1 (`1/3 chapitres` est correct en français).
  2. **Mon correctif du pluriel était lui-même cassé** : `t('…compte1')` sans
     second argument affichait littéralement `{n}/{total} chapitre`. Ni `tsc`
     ni la porte « aucune clé morte » ne le voyaient.
  3. **`\\?\` affiché.** Rust renvoie le chemin canonicalisé, donc préfixé de la
     forme longue Windows ; c'est indispensable en interne, illisible à l'écran.
     Retiré à l'affichage seulement.
- **Deux trous dans la porte `i18n:check`, trouvés en l'écrivant :**
  1. ses classes de caractères **refusaient le `-`** : une clé kebab-case
     disparaissait du dictionnaire que la porte analyse — un trou
     **symétrique**, la clé invisible ne pouvait pas non plus être déclarée
     morte ;
  2. une clé cité dans une **table de correspondance** (`Record<Code, clé>`)
     était déclarée morte alors qu'elle est affichée. Les commentaires sont
     maintenant retirés avant analyse, pour qu'une clé citée en prose ne compte
     pas.
  Nouvelle règle : **`{placeholder}` appelé sans ses valeurs**. Prouvée
  mordante — `t('…compte1')` nu fait échouer la porte en nommant la clé et les
  placeholders attendus. Et sa première version ne mordait **pas** : le
  drapeau `m` manquait, donc `FR_VALEURS` était vide. Une porte qui ne peut pas
  crier est indiscernable d'une porte verte.
- **Mesure sur l'app réelle** (dialogue intercepté au niveau du chunk, tout le
  reste réel) : 4 chapitres importés, ordre **1, 2, 6, 10** (tri numérique,
  pas alphabétique), 5 refus avec leur motif, `complete: true`, les 4 chapitres
  ouverts par le lecteur (3, 2, 2, 2 pages), motifs **0/5 identiques** entre
  les deux langues, **0 erreur console**.
- **Un test qui supposait un refus a révélé une garantie** :
  `chapter_file_name(titre, "")` retombe toujours sur `sanitize(titre).cbz` —
  y compris pour un titre vide, dont le repli d'URL appelle `sanitize("")`. Donc
  l'aller-retour tient toujours, et la branche de refus de
  `nom_chapitre_lecteur` est inatteignable. Le test vérifie l'aller-retour, pas
  un refus qui n'a pas lieu d'être.
- **Porte prouvée mordante** : changer la règle de nommage du lecteur
  (`{base}.cbz` → `ch-{base}.cbz`) fait échouer **12** tests.
- `cargo test` **121** (100 + 1 + 20), dont 14 nouveaux · `cargo fmt --check` 0 ·
  `clippy --all-targets -D warnings` 0 · `tsc` 0 · `lint` 0 erreur (4 warnings
  antérieurs) · `build` OK · `i18n` 222 clés, aucune morte, aucun placeholder nu.

### 2026-10-05 — Réglages du planificateur (3.5)

### 2026-10-05 — Réglages du planificateur (3.5)

- Plan : deux commandes Tauri + l'écran. Le Rust avait déjà tout (`Reglages`,
  `bornes()`, et `actif` déjà lu par la passe) ; il manquait l'accès.
- **Porte prouvée ROUGE puis verte.** `aucune_commande_tauri_nest_morte` a cité
  les deux commandes et dit quoi faire après enregistrement — puis elle est
  passée au vert une fois l'interface écrite. C'est la porte existante qui a
  servi, pas une nouvelle.
- **Un test a trouvé un vrai bug dans mon test.** `ecrire_un_seul_champ…`
  échouait : le helper prenait `Reglages::default()` pour « rien de fourni »,
  alors que `default()` a `actif: true`. Le test confondait une **valeur par
  défaut** avec une **absence de champ**. Le helper prend désormais un champ par
  paramètre — « rien » doit vouloir dire `None`, pas `default()`.
- **Deux bugs réels trouvés à la mesure, aucun visible au compilateur :**
  1. **Les bornes de la vue.** J'utilisais un second `bornes()` pour obtenir les
     maximums — or `bornes()` **écrase** les valeurs au lieu de les lire. Donc
     `max_sources_max` valait **20** (la valeur courante) au lieu de 200, et
     l'écran affichait `max="20"` : le réglage du plafond de sources était
     bloqué à sa valeur par défaut, sans aucun message. Les bornes sont
     désormais des **constantes nommées**, et `bornes()` s'y réfère — une seule
     source de vérité.
  2. **Le nom des paramètres d'écriture.** J'envoyais `periode_s` ; Tauri
     convertit `camelCase` en `snake_case` et **ignore silencieusement** une clé
     déjà snake_case. La commande répondait sans erreur avec les **anciennes
     valeurs** — donc un réglage qui ne bougeait pas paraissait réussi.
     Corrigé en `periodeS`, et le type `PlanPatch` l'impose désormais. Le reste
     du projet suivait déjà cette convention (`mangaUrl`, `timeoutSecs`).
- Preuve sur l'app réelle : période 21600 → 3600 avec les trois autres champs
  inchangés ; `actif` coupé puis **relu depuis la base** (l'écran ne se mentait
  pas) ; `min=1 max=5000` et `min=1 max=200` venus de Rust ; 0 erreur console.
- `cargo test` **107** (86 + 1 + 20), dont 3 nouveaux · `cargo fmt --check` 0 ·
  `clippy --all-targets -D warnings` 0 · `tsc` 0 · `lint` 0 erreur (4 warnings
  antérieurs) · `i18n` sans clé morte.
- **Porte prouvée mordante** : remettre `max_sources_max: 20` fait échouer
  `la_vue_expose_les_bornes_et_non_les_valeurs` en nommant `20` contre `200`.

### 2026-10-05 — Flux « mises à jour » (3.2)

### 2026-10-05 — Flux « mises à jour » (3.2)

- Plan : écouter `maj://nouveaux-chapitres` et afficher le flux. Le planificateur
  était livré mais son résultat n'était **visible nulle part** : il n'avait
  aucun effet sur l'utilisateur. C'est la fin de 3.1 qui ne servait à rien.
- Le contrat a été lu **avant** d'écrire : `ResultatPasse` avec `nouveaux`,
  `rates`, `verifies`, `sources`, `ignores`, `desactive`. Les champs sont en
  `snake_case` sans couche de conversion — donc `chapitres`, `total`, `tronque`
  arrivent tels quels, et un `camelCase` aurait donné `undefined`.
- **Le piège que le moteur documente, vérifié explicitement** : afficher
  `Nouveaute.total`, jamais `chapitres.len()` — le moteur plafonne la liste, donc
  la longueur annoncerait un nombre faux. Le cas émis est volontairement
  **tronqué** (`total` 47, 3 chapitres listés) : l'écran affiche « 47 nouveaux
  chapitres » et « et 44 de plus ». Lecture de la mauvaise colonne → 3.
- **Fusion, pas empilement.** Une passe remplace le flux précédent, car le même
  favori est signalé à chaque passe tant que ses chapitres sont dans la fenêtre
  « non lus » de la source. Fusion par `(source, manga_url)` avec cumul des
  totaux : mesuré, 2 passes du même favori donnent **49** (47 + 2) et **2
  entrées**, pas 3.
- Persistance : `localStorage`, plafonné à 200. Vérifié — les 2 entrées sont
  là après un rechargement.
- **Un doublon écrit avant d'être lu.** J'ai d'abord écrit une fonction
  `ouvrirFavori` dédiée avec les deux appels `Promise.allSettled`, en copiant
  `ouvrirSignet`. `loadChapters` faisait déjà exactement ça, et ma version
  inventait un `setChapitresCharges` qui **n'existe pas** — le compilateur l'a
  dit. Corrigé en déléguant, donc il n'y a plus qu'un chemin d'ouverture de fiche.
- **Les pluriels, encore.** Première passe de mesure : 2 entrées et 3 chapitres
  listés, donc le singulier **n'était pas testé** — et la capture montrait « 1
  mise(s) à jour » et « Voir les 1 chapitres signalés ». Même défaut que 5.4,
  quatre clés de plus. Re-mesuré avec 1 mise / 1 chapitre listé / 12 au total,
  dans les deux langues : aucune faute.
- Une **fausse alerte de la sonde** : `/1 autres\b/` matchait « **11** autres ».
  Le nombre précède le mot, il faut donc borner par `[^0-9]`. Un test trop
  lâche apprend à ignorer ses propres alertes.

### 2026-10-05 — Vérification sur l'app de 5.4 et du correctif d'hydratation

### 2026-10-05 — Vérification sur l'app de 5.4 et du correctif d'hydratation

Deux travaux étaient livrés **sans avoir été vus**. L'app redémarrant à chaque
écriture dans `src-tauri`, la vérification attendait. Elle est faite.

**1. Correctif d'hydratation : vérifié.** Plus aucune erreur en console, et les
attributs suivent la valeur stockée dans les deux sens :

| | valeur en base | attribut après | |
|---|---|---|---|
| au démarrage | `lang=en` | `lang=en` | cohérent |
| bascule | `fr` | `lang=fr` | suit |
| apparence | `sombre` → `clair` | `data-apparence=clair` | suit |
| 3 navigations | — | **0 erreur d'hydratation** | |

**2. Écran 5.4 : vérifié, puis deux fautes trouvées à la lecture.**

La première mesure a trouvé **0 source en échec** — le message « aucune source en
échec » s'affiche, ce qui est correct, mais **la table n'avait jamais été
rendue**. Un panneau qui ne s'affiche qu'à vide ne prouve rien. Avec 7 pannes
injectées :

- 3 colonnes (`Source`, `Erreur`, `Signalée`), 7 lignes, tri du plus récent ;
- regroupement : `CHALLENGE_REQUIRED 3`, `PARSE_ERROR 2`, `TIMEOUT 1`,
  `AUTH_REQUIRED 1` — **la somme des compteurs vaut 7**, elle coïncide donc avec
  le détail et pas seulement avec le nombre affiché ;
- contrôle négatif en anglais : **0** des 7 anciennetés contient du français ;
- « Tout réessayer » : 7 → 0 en base, et la table disparaît de l'écran.

**Les deux fautes, trouvées sur la capture et non dans le code :**

1. l'anglais affichait **`1 months ago`** — le pluriel anglais ne se déduit pas
   du nombre comme en français. Une clé `{n} mois` ne peut pas servir les deux ;
   `temps.mois1` a été ajoutée ;
2. **`1 source(s)`** partout, y compris au pluriel — même cause, même correction
   (`etat.resume1`, `etat.sources1`).

Les deux n'étaient visibles qu'**à l'œil**, pas dans `tsc` ni dans `i18n:check` :
les clés existaient, elles étaient correctes, et la faute était dans le choix
d'une clé unique.

**Une faute de sonde, à ne pas confondre avec un défaut du code.** La sonde
signalait un défaut de traduction à une seule panne : elle affichait l'anglais
alors que la langue demandée était le français. La valeur n'était fausse nulle
part — la sonde changeait la langue par le `<select>` **avant** le rechargement,
dans un `evaluate` séparé, et React n'avait pas vu le changement. Stocker langue
et données dans le **même** `evaluate` avant le rechargement a rendu le test
exact. C'est le troisième cas de la session où la mesure, et non le code, était
en cause.

### 2026-10-05 — Vérification de 3.1 (planificateur), session 1

### 2026-10-05 — Vérification de 3.1 (planificateur), session 1

La session 2 a livré `scheduler.rs` (~900 lignes, 19 tests) et demandé deux
décisions. J'ai **remesuré** avant d'accepter, plutôt que de recopier ses chiffres.

**Ce que j'ai vérifié moi-même :**

| porte | résultat |
|---|---|
| `cargo fmt --check` | 0 |
| `cargo clippy --all-targets -D warnings` | 0 |
| `cargo test` | **104 passés / 0 échec** (83 + 1 + 20) |

Le travail est réel. Deux tests prouvent qu'elle a compris les bugs coûteux de ce
projet : `le_type_dechduit_vient_des_octets_pas_du_contenttype` et
`un_interstitiel_html_nest_pas_une_image` — les deux défauts que le projet a
payés cher.

**Point ouvert n° 2 — concurrence au démarrage : mesuré, et moins grave
qu'il n'y paraît.** Le pire cas calculé est 20 sources × 20 s = **400 s** de
passe, partant au démarrage en concurrence avec l'interface (le moteur est
séquentiel, `supervisor.rs`). J'ai chronométré sur le vrai moteur :

- `EXHENTAI` — la source qui « scanne tout son catalogue » — répond en **1 339 ms** ;
- une source rapide pendant/après : **620 ms**.

Donc les 20 s sont un **délai de politesse**, pas une durée d'appel : le pire cas
n'est pas 400 s de travail réel. **Décision : ne rien changer au code.** Le risque
restant est `N × durée réelle d'un appel`, avec un `timeoutSecs` de 60 s, à
re-mesurer quand la bibliothèque comptera des centaines de favoris.

**Point ouvert n° 1 — état dans `settings` : l'estimation était 4× trop
basse.** La session annonçait « 200 favoris × 2 000 URLs ≈ 8 Mo ». Mesuré avec
des URLs réelles, pas des chaînes de même longueur :

| favoris | chapitres/favori | taille |
|---|---|---|
| 10 | 100 | 82 Ko |
| 50 | 500 | 2,0 Mo |
| 200 | 1 000 | **15,8 Mo** |
| 200 | 2 000 | **31,7 Mo** |

Et le coût réel n'est pas le stockage, c'est le **parsing** : `JSON.parse` de
31,7 Mo prend **80 ms**. Une passe toutes les 6 h s'en accommode ; une lecture
par page à chaque navigation non.

**Décision : ne rien changer maintenant**, pour deux raisons mesurées. D'abord
une table `chapitres_vus` implique de toucher `SCHEMA_VERSION`, ce qu'`AGENTS.md`
§2 interdit sans arbitrage humain — et 3.1 est à 19 tests sans migration. Ensuite
le volume n'est atteignable qu'avec 200 favoris **et** 1 000 chapitres vus par
série, ce qui n'est pas le cas d'usage. **Point ouvert conservé** : avant
d'atteindre ce volume, ou dès que 3.5 affichera l'état, il faudra une table
dédiée — pas parce que l'estimation était alarmante, mais parce que 80 ms de
parsing par navigation est une latence **constante**, elle.

### 2026-10-05 — Tâche 3.1 : planificateur Rust (tokio)

- Plan : écrire `scheduler.rs` sans aucun couplage au moteur, pour que la décision
  « ce chapitre est-il nouveau ? » et le débit par source soient **mesurables**
  sans JVM. Trois règles structurantes : (1) le premier relevé d'un favori ne
  signale rien, sinon le premier lancement annonce 286 chapitres par série ; (2)
  débit borné par source avec retour arrière exponentiel et borné ; (3) une
  réponse `{chapters: []}` est un **échec**, pas un succès. Horloge et moteur
  injectés, sinon un intervalle de 4,9 s serait impossible à vérifier.
- **Fait** : `apps/desktop/src-tauri/src/scheduler.rs` (~900 lignes, 19 tests).
  `passe()` fait le tour : ordre « moins récemment vérifié d'abord » (donc
  **aucun favori n'est affamé** par le plafond), plafond de tentatives qui
  compte les essais et non les favoris parcourus, plafond de sources distinctes,
  diff par **URL de chapitre** (jamais par numéro — les sources réordonnent),
  fenêtre de chapters vus bornée à 2 000, tri du plus récent au plus ancien
  quand les dates existent, `total` complet même si la liste d'annonce est
  tronquée à 10. État persisté en base (clé `settings` `planificateur.etat`) :
  une lecture et une écriture par passe. Branché dans `main.rs` au démarrage ;
  émet `maj://nouveaux-chapitres`, que la tâche 3.2 consommera.
- **Deux détails mesurés, pas déduits :**
  - **l'heure est relue à chaque itération, pas figée au début de la passe.** Une
    passe de 20 sources × 20 s dure 400 s ; avec une heure figée, toutes les
    requêtes porteraient le même horodatage et **le délai par source ne
    s'appliquerait jamais** — la politesse aurait été fictive, et aucun test ne
    l'aurait vu ;
  - **le premier `tick` d'un `interval` tokio est immédiat.** C'est ce qui pose
    l'état de référence au démarrage. Le retirer ferait annoncer toute la
    bibliothèque à la première passe utile. En revanche `MissedTickBehavior::Skip`
    est obligatoire : par défaut `interval` rattrape les ticks perdus, donc une
    passe de 400 s avec une période de 5 min déclencherait 1,3 passe d'affilée.
- **Aucune dépendance ajoutée** : `tokio` était déjà présent (`full`), donc pas
  d'ADR. `async-trait` aurait été la seule dépendance imaginable — le type de
  future est écrit à la main, une ligne.
- **Preuve** (`cd apps\desktop\src-tauri`) :
  - `cargo fmt --check` → exit 0
  - `cargo clippy --all-targets -- -D warnings` → exit 0
  - `cargo test` → **104 passés, 0 échec** (83 lib dont 19 pour le planificateur,
    1 binaire, 20 de contrat)
- **Portes prouvées ROUGES puis vertes** (une porte qui n'a jamais rien trouvé
  n'est pas une porte) — 4 mutations, le nom du test qui échoue est cité :
  | mutation | tests en échec |
  |---|---|
  | garde « premier relevé » supprimé | 4 — dont `le_premier_releve_annonce_rien_et_pose_l_etat_de_reference` |
  | liste vide traitée comme un succès | 1 — `une_liste_vide_est_un_echec_qui_preserve_l_etat_de_reference` |
  | limite de débit supprimée | 1 — `le_delai_par_source_est_applique_a_la_seconde_pres` |
  | ordre figé (plus de priorité au moins-vérifié) | 2 — dont `le_plafond_par_passe_ne_famine_aucun_favori` |
- **Deux défauts trouvés dans mes propres tests, en les exécutant** (et non en
  les relisant) :
  - le test de la boucle était **aléatoire** : la réponse du moteur était
    modifiée « juste après » le démarrage, donc la première passe pouvait la voir
    déjà mise à jour, mémoriser le chapitre comme vu, et ne jamais l'annoncer —
    une fois sur deux. Corrigé par des **réponses consécutives** et une horloge
    injectée ;
  - le test « pas de faim » échouait alors que le code était bon : je relisais
    l'état par favori, mais l'état ne retient qu'une **date**, et cinq favoris
    partageaient le même `verifie_le` au millième. La preuve de l'équité est
    l'**ordre réel des requêtes** dans le journal du moteur, pas l'état.
- **Points ouverts** :
  - **Aucun écran.** 3.2 (flux « mises à jour ») et 3.5 (réglages) restent à
    faire ; décrits ci-dessous, pas implémentés.
  - **L'état vit dans `settings` en JSON, pas dans une table dédiée.** Choix
    volontaire pour ne pas toucher au schéma dans cette tâche (le `SCHEMA_VERSION`
    partagé est un point de rupture documenté). À mesurer : 200 favoris × 2 000
    URLs ≈ 8 Mo dans une seule valeur. Une table `chapitres_vus` sera plus saine
    quand 3.5 affichera l'état ; **à trancher avant d'atteindre ce volume**.
  - **Le planificateur démarre actif** (6 h de période, 20 s de délai par source,
    200 favoris et 20 sources par passe) et sa **première passe part au
    démarrage**, en concurrence avec les requêtes de l'interface. Le moteur est
    séquentiel : sur une grosse bibliothèque, la passe peut ralentir l'affichage.
    Non mesuré (l'agent n'a pas lancé l'application, ports 3000/9223 occupés) ;
    à mesurer à l'œil ou par journal lors d'une passe réelle.
  - **`3.5` doit exposer `actif`, `periode_s`, `delai_source_s`** : les trois sont
    déjà lus et bornés par `Reglages::bornes()`, donc l'interface n'aura qu'à
    écrire la clé `planificateur.reglages`.
  - **Le réglage « sources concernées » de 3.5 n'existe pas** : aujourd'hui la
    liste vient de `store.disabled_sources()`, ce qui est déjà un filtre
    par source, mais global. Un filtre par favori demandera une colonne.
  - **Non commité** : PLAN.md §8 bis réserve les commits à la session 1, et
    `AGENTS.md`, `Cargo.toml`, `supervisor.rs`, `tests/` et `lib.rs` portaient
    déjà des modifications d'autres sessions au moment où ce travail a commencé.

#### Écrans à faire pour 3.2 et 3.5 — décrits, **pas implémentés** (demande explicite)

- **3.2 — vue « Mises à jour ».** Onglet de navigation, alimenté par
  `maj://nouveaux-chapitres`. Une ligne par favori, `Nouveaute.titre`, puis le
  nombre **complet** (`Nouveaute.total`, pas `chapitres.len()` — les deux
  diffèrent quand `tronque` est vrai) ; la liste des chapitres n'est affichée que
  si `tronque` est faux, sinon « et N de plus ». Chaque ligne ouvre la fiche du
  webtoon (les `manga_url` sont déjà là). Marqueur « lu » à mémoriser en base —
  sinon la même notification revient à chaque application ouverte, et c'est
  précisément le défaut que `total`/`tronque` permettent d'éviter.
- **3.2 — dernier passage.** `Etat.dernier_pass` existe déjà et n'est pas
  encore lu par l'interface : l'afficher dans les réglages rassure
  (« dernière vérification il y a 6 h ») et prouve d'un coup que la tâche de
  fond tourne.
- **3.2 — erreurs.** `ResultatPasse.rates` n'est pas émis (seul `nouveaux`
  l'est, volontairement). Si l'interface veut montrer « 3 sources n'ont pas
  répondu », il faudra émettre un second évènement : **à trancher en 3.2**.
- **3.3/3.4 — notifications système.** Le plugin Tauri n'est ni ajouté ni
  documenté ici (3.3 le dit « à vérifier dans la doc »). Rien à faire avant que
  3.2 ait un écran à notifier.

### 2026-10-05 — Notifications système (3.3) — fait

**Ce que la doc a changé dans le plan.** J'avais lu « plugin Tauri » et écrit
une capability à tout hasard. En cherchant le code réel de `sendNotification`
dans le chunk chargé :

```js
function sendNotification(options) {
  new window.Notification(options.title, options);
}
```

Sur desktop, **aucun IPC**. C'est l'API Web Notifications de WebView2. Et
`isPermissionGranted()` interroge d'abord le navigateur, puis ne tombe sur
`plugin:notification|is_permission_granted` que si `permission === 'default'`.
Donc :

- `notification:allow-notify` est **mobile seul** → **retiré** de la capability
  (AGENTS.md §7 : strict minimum ; §5 : le mobile est hors v1) ;
- `notification:allow-is-permission-granted` et `allow-request-permission` sont
  gardés, parce qu'un profil WebView2 neuf est précisément dans l'état
  `default` ;
- le plugin Rust reste enregistré : c'est lui qui répond à cette requête, et il
  portera le chemin mobile le jour où il existera.

**Validation des capabilities — prouvée, pas supposée.** `tauri-build` refuse un
permission inconnue : injecter `notification:allow-notify-FAUX` fait échouer le
build en nommant `Permission ... not found`. Le build vert ne prouve donc pas
seulement que ça compile — il prouve que les trois identifiants **existent**.
Vérifié aussi dans le `capabilities.json` **généré**, qui ne contient que les
quatre permissions voulues.

**Rust décide quand, l'interface décide quoi dire.** Même raison que les motifs
d'import de 2.4 : du texte dans Rust s'affiche tel quel dans les deux langues.
Rust émet `maj://nouveaux-chapitres`, l'interface traduit et envoie.

**Le réglage est dans `localStorage`, et c'est cohérent** avec la règle du dépôt :
`localStorage` pour ce que l'interface lit, la base pour ce que le cœur applique.
Ici le cœur n'applique rien — et surtout, **éteindre les notifications n'arrête
pas la veille** : le flux « mises à jour » de 3.2 est une autre chose et continue
de fonctionner. Une chaîne vide n'est jamais un sentinelle : `'1'` / `'0'`, et
une clé absente vaut « actif ».

**Silence quand la fenêtre a le focus.** `document.hasFocus()` plutôt que
`getCurrentWindow().isFocused()` : attribut DOM standard, donc **aucune**
permission à ajouter.

**Un défaut de rédaction que seule la lecture de la sortie a révélé.** La
première version composait « 12 nouveaux chapitres » + « ou plus », ce qui
donnait « 12 nouveaux chapitres · ou plus » : correct en anglais, agrammatical en
français. Le marqueur de troncature est désormais **dans** le fragment
(`notif.c1p` / `notif.cnp`). Un fragment qui se tient seul est la seule façon de
composer deux langues sans connaître la grammaire de l'autre.

**Mesure sur l'app réelle** — l'échographie **enveloppe** le constructeur
`window.Notification` et **notifie pour de vrai** ; le focus est la seule chose
falsifiée, et c'est précisément le cas que la politesse doit couvrir :

| | FR | EN |
|---|---|---|
| 1 chapitre, 1 série | `1 nouveau chapitre · No. 6` | `1 new chapter · No. 6` |
| 2 chapitres, 2 séries | `2 nouveaux chapitres · 2 séries` | `2 new chapters · 2 series` |
| 3 chapitres, 1 série | `3 nouveaux chapitres · No. 6` | `3 new chapters · No. 6` |
| 12 chapitres, 5 séries, tronqué | `12 nouveaux chapitres ou plus · 5 séries` | `12 new chapters or more · 5 series` |
| total nul | *(aucune)* | *(aucune)* |

- `plugin:notification|is_permission_granted` → `true` (le plugin Rust répond) ;
- fenêtre **active** → **0** notification ; sans focus → **1** ;
- `tatami.notif = '0'` → **0** ; clé supprimée → **1** ;
- une série répétée dans la passe est comptée **une fois** (5, pas 6) ;
- `role="switch"` sur l'interrupteur, clic → `tatami.notif = "0"` ;
- **0 erreur console**.

**Un trou de plus dans la porte i18n.** Elle ne lisait que `page.tsx`, donc les
six clés utilisées dans `app/notification.ts` étaient déclarées **mortes**. Elle
lit maintenant **tous** les modules de `app/`, sauf le dictionnaire — y inclure
`i18n.ts` ferait compter toute clé comme utilisée et la porte ne crierait plus
jamais. Prouvée mordante après le changement.

`cargo test` **121** (100 + 1 + 20) · `fmt` 0 · `clippy --all-targets -D
warnings` 0 · `tsc` 0 · `lint` 0 erreur (4 warnings antérieurs) · `build` OK ·
`i18n` 232 clés, aucune morte, aucun placeholder nu.

### 2026-10-05 — Zone de notification et démarrage automatique (3.4) — fait

**Le trait de fer de cette tâche** : on ne masque la fenêtre que si l'utilisateur
a un moyen de la ramener. Trois issues, donc un verrou extrait dans une fonction
pure `doit_cacher(quitte, tray_visible)` :

```rust
pub fn doit_cacher(quitte: bool, tray_visible: bool) -> bool { !quitte && tray_visible }
```

Le cas grave est mesuré dans le code du plugin : `TrayIconBuilder::icon()`
attend une `Image` et **ne fait rien** si on ne lui en donne pas. Un tray sans
icône existe mais n'a rien de cliquable — masquer la fenêtre dans ce cas
interdirait à l'utilisateur de retrouver l'application autrement que par le
gestionnaire de tâches. D'où `TRAY_VISIBLE`, mis à **vrai seulement** après un
`build()` réussi avec icône.

**Porte prouvée mordante** : remplacer `!quitte && tray_visible` par `!quitte`
fait échouer `on_ne_masque_la_fenetre_que_si_le_tray_est_cliquable`, en nommant
le scénario — « sans icone, masquer la fenetre rendrait l'application
injoignable ».

**Le tray est créé par Rust, les libellés par l'interface.** Même arbitrage que
3.3 : Rust garantit l'existence, l'interface possède la langue. `set_libelles`
renvoie les textes **relus depuis `MenuItem::text()`**, pas ceux reçus : c'est la
seule preuve que le système a pris. Et cela évite d'ajouter
`core:menu:allow-get-text` aux capabilities — que la règle « strict minimum »
interdit d'ajouter pour les besoins d'une mesure.

**Zéro capability ajoutée pour le tray.** Trois sondes ont été **refusées** par
le moteur : `tray.get_by_id`, `menu.get`, `window.is_visible`. Ce refus est la
preuve que la discipline tient — mon code n'a besoin de rien de tout cela, donc
il n'est pas déclaré. Vérifié autrement : journal Rust « icone de zone creee », et
la lecture du registre par `set_libelles`.

**Démarrage automatique, état relu et jamais deviné.** Mesuré contre le système :
`is_enabled` → `false`, `enable` → `true`, `disable` → `false`, registre de
départ laissé intact. Un état `indisponible` existe pour le cas réel où le
système refuse (Linux sans gestionnaire de bureau, politique d'entreprise) :
l'interrupteur est alors **désactivé** et le dit, au lieu de rester coché pour
rien.

**Une sonde qui lisait la langue au mauvais endroit.** Je testais la langue via
`document.documentElement.lang` — l'attribut que le script bloquant de
`layout.tsx` laisse à `fr` (la régression signalée hors périmètre). La sonde
envoyait donc toujours du français et concluait que l'anglais ne passait pas.
Corrigée sur `localStorage`, avec l'onglet affiché comme preuve
indépendante (« Home »).

`cargo test` **125** (104 + 1 + 20), dont 4 nouveaux · `fmt` 0 · `clippy
--all-targets -D warnings` 0 · `tsc` 0 · `lint` 0 erreur (4 warnings antérieurs) ·
`build` OK · `i18n` 240 clés, aucune morte, aucun placeholder nu.

### 2026-10-05 — Notifications système (3.3) — mini-plan

Doc officielle lue avant d'écrire quoi que ce soit (règle d'or n° 2) :
<https://v2.tauri.app/plugin/notification/> et docs.rs 2.5.1.

1. **Qui envoie ?** Rust décide **quand**, l'interface décide **quoi dire**.
   Le planificateur émet déjà `maj://nouveaux-chapitres` ; le texte part de
   l'interface, seule couche à connaître les langues. Même raison que les motifs
   d'import de 2.4 : du français dans Rust s'affiche dans un écran anglais.
2. **Une notification par passe**, pas par chapitre — une passe peut en trouver
   12 dans 5 séries, et 12 toasts d'un coup sont un utilisateur hostile.
3. **Silence quand la fenêtre a le focus.** Un utilisateur en plein chapitre ne
   veut pas être interrompu ; `document.hasFocus()` suffit et ne demande
   **aucune permission** Tauri.
4. **Un réglage** dans le panneau 3.5, sinon l'utilisateur ne peut pas éteindre
   ce qu'il n'a pas demandé.
5. **Porte** : `pnpm i18n:check` (aucune clé morte) + une clé par variante de
   pluriel, et l'erreur du plugin **visible** — un `show()` qui échoue en
   silence est une notification qui n'arrive jamais.

**Limite connue, mesurée dans la doc** : sur Windows le plugin ne fonctionne que
pour une application **installée** ; en développement c'est le nom et l'icône de
PowerShell qui s'affichent. La preuve sera donc « ça part », pas « ça a la bonne
identité ».


### 2026-10-05 — Tâche 5.3 : tests de contrat sur réponses enregistrées + CI

- Plan : (1) enregistrer de **vraies** réponses du moteur en NDJSON, pas les écrire
  à la main — une fixture écrite à la main ne prouve qu'une chose, qu'elle est
  conforme au schéma ; (2) les rendre accessibles au test d'intégration, ce qui
  supposait de sortir le code du binaire ; (3) écrire le test de contrat, avec
  des **contre-exemples** pour chaque règle, sinon une porte qui n'a jamais rien
  trouvé n'est pas une porte ; (4) exécuter en CI la liste de AGENTS.md §8.

- **Fait — 16 réponses enregistrées, 13 méthodes couvertes.** Le moteur compilé
  (`engine/dist/tatami-engine`, protocole 0.1, parsers `b3c0000`) a été interrogé
  en NDJSON en deux passes (`.mesures/enregistrer-reponses.mjs`, puis `-2.mjs`
  pour la progression `latest → details → chapters → pages → fetchImage`, qui
  exige une URL réelle à chaque étape). Les réponses brutes restent **hors du
  dépôt** ; ce qui est committé est la version anonymisée
  (`apps/desktop/src-tauri/tests/fixtures/`, + un `README.md` qui explique la
  procédure).

- **Anonymisation (AGENTS.md §2.7).** Titres, auteurs, genres, UUID et URLs de
  sites remplacés par des valeurs fictives. `listSources` : 1370 entrées → 4.
  `chapters` : 222 → 4 (les 3 formes de `chapterToJson`, dont le nombre flottant
  mesuré à l'écran). **Les octets de `fetchImage` / `fetchCover` sont une image
  de synthèse 1×1** : une page de manga n'a rien à faire dans le dépôt. La
  longueur base64 réelle est conservée en métadonnée (807 504 et 61 940
  caractères).

- **Fait — `src/lib.rs` : le code sort du binaire.** Le crate était un pur
  binaire (`main.rs` avec ses `mod`), donc `tests/` ne voyait **aucun** code de
  production : il ne pouvait valider que le schéma, jamais la façon dont Rust lit
  une réponse. `src/lib.rs` déclare les 12 modules en `pub`, `main.rs` ne garde
  que l'assemblage Tauri. Cinq éléments sont devenus `pub`, chacun pour une
  raison écrite dans `lib.rs` : `commands::into_result`,
  `supervisor::is_transient`, `supervisor::PROTOCOL_MAJOR`,
  `commands::ajouter_jetons_couverture`, et la cible `lib` dans `Cargo.toml`.
  Vérifié dans tauri-macros 2.7.1 que `generate_handler!` résout un chemin
  jusqu'au dernier segment puis invoque le macro `__cmd__x` **au même chemin**,
  rendu possible depuis un autre crate par le `pub use` que le wrapper émet —
  donc l'import de `commands` dans `main.rs` n'est pas cosmétique.

- **Fait — le test de contrat (20 tests).** Trois familles : (a) validation de
  chaque réponse contre `contract/protocol.json` ; (b) rejeu de chaque réponse
  contre le **vrai code** (`into_result`, `is_transient`, `image::detect`,
  `PageCache`) ; (c) contre-exemples, pour que le validateur soit démontré
  capable de **refuser** (8 cas d'enveloppe, 7 cas de `result`). Le validateur
  est écrit dans le test — ajouter `jsonschema` ferait entrer un arbre de
  dépendances dans le dépôt pour juger 16 fichiers, ce qu'AGENTS.md §9 interdit
  sans ADR. Il couvre le sous-ensemble exact du schéma, et une porte
  (`le_validateur_couvre_tout_le_schema_du_contrat`) échoue si le schéma
  gagne un mot-clé que le validateur ne gère pas : sinon le test validerait des
  réponses qu'il ne sait pas juger, et passerait toujours.

- **Un défaut réel du contrat trouvé et consigné.** `contract/protocol.json`
  exige `id` et `ok`, mais **n'exige pas `error` quand `ok` vaut `false`** : une
  enveloppe `{id, ok:false}` sans `error` est valide, et `into_result` en déduit
  `INTERNAL: ` — un message vide affiché à l'utilisateur. Le test
  `le_schema_admet_une_erreur_sans_bloc_error_et_nous_le_savons` **fige** ce
  comportement au lieu de corriger le schéma en douce : AGENTS.md §5 impose une
  ADR pour toute évolution du contrat, et `protocol.json` est partagé avec le
  moteur. La correction est un point ouvert ci-dessous.

- **Les portes ont été prouvées mordantes, pas seulement vertes.** Casser
  `image::detect` pour qu'un interstitiel HTML passe pour une image →
  `un_interstitiel_html_nest_pas_une_image` **échoue**. Faire rejouer
  `PARSE_ERROR` dans `is_transient` → **2 tests échouent**. Un troisième essai
  avait **échoué** : retirer le cas `PARSE_ERROR` de `is_transient` ne casse
  rien, parce que la liste de marqueurs finals exclut déjà ces messages — le test
  passait donc pour une mauvaise raison. C'est exactement la leçon « une porte
  qui n'a jamais rien trouvé n'est pas une porte », appliquée à mes propres
  portes.

- **Fait — `.github/workflows/ci.yml`.** Un job unique sur `windows-latest`,
  étapes **séquentielles**, qui exécute les 9 commandes de AGENTS.md §8 avec
  leurs dossiers. Choix argumentés dans l'en-tête du fichier : Windows d'abord
  (§1, §3), `cargo test` sur Linux exigerait les libs système de Tauri, et un
  job unique partage un seul `engine/dist/` entre `pnpm build` et les tests
  Kotlin. Gradle est installé par l'action officielle (`gradle 9.8.0`, la version
  mesurée) puisqu'il n'y a pas de wrapper dans le dépôt — AGENTS.md §13 interdit
  de committer le `gradle-wrapper.jar`.

- Preuve (exécutée dans `apps\desktop\src-tauri`, puis `apps\desktop`, puis
  `engine`, dans cet ordre) :
  - `cargo fmt --check` → **0**
  - `cargo clippy --all-targets -- -D warnings` → **0**
  - `cargo test` → **80 passés, 0 échec** (59 bibliothèque + 1 binaire +
    20 contrat)
  - `pnpm typecheck` → **0** · `pnpm lint` → **0 erreur**, 4 warnings
    `exhaustive-deps` antérieurs · `pnpm i18n:check` → **152/152** ·
    `pnpm build` → OK
  - `gradle test --no-build-cache --rerun-tasks` → **BUILD SUCCESSFUL en 1 m 08**,
    **24 tests, 0 échec, 0 ignoré** (5 classes). Les deux premiers essais
    n'avaient **exécuté aucun test** : `UP-TO-DATE`, puis `FROM-CACHE` (le cache
    de build Gradle est activé dans `gradle.properties`). Le décompte vient des
    5 XML de `build/test-results/test/`, seul endroit qui prouve l'exécution.
  - `ci.yml` : YAML parsé (PyYAML), 20 étapes, 1 job, `timeout-minutes: 45`.

  **État de l'arbre au moment où cette ligne est écrite** : le dépôt est
  partagé avec deux autres sessions, et `apps/desktop/app/page.tsx` est
  **modifié en ce moment** par le travail 5.4 (écran d'état des sources) : ses
  clés `etat.*` ne sont pas encore dans `app/i18n.ts`, donc `pnpm typecheck`
  échoue désormais sur **13 erreurs TS2345/TS2554**, et `pnpm lint` compte
  5 warnings au lieu de 4. Ce n'est pas lié à 5.3 — je n'ai touché ni
  `page.tsx` ni `app/` — mais la preuve ci-dessus date de **mon** exécution,
  avant ce changement. Une preuve est datée : elle vaut pour l'instant où elle
  a été prise, pas pour l'arbre du lendemain. Qui commite doit repasser la
  liste sur un arbre tranquille.

- Points ouverts :
  - **`contract/protocol.json` n'exige pas `error` quand `ok:false`.** Le test
    le fige ; le corriger (un `if/then` sur `ok`) demande une ADR (AGENTS.md §5).
  - **AGENTS.md §8 écrit `cargo clippy -- -D warnings`, qui ne regarde pas
    `tests/`** : le test de contrat ne serait jamais linté. La CI utilise
    `--all-targets` (sur-ensemble strict). §8 gagnerait à le préciser — c'est
    dans mon droit de mettre à jour les commandes de vérification (§12).
  - **4 tests Kotlin dépendent du réseau** (`ParserPoolPaginationTest` ×3,
    `ZenMangaInterceptionTest` ×1) : verts le 2026-10-05, mais peuvent échouer en
    CI pour une IP de runner. Je n'ai **pas** réduit la couverture pour faire
    passer. Correctif durable : `@Tag("reseau")` + exclusion du job courant avec
    un job planifié qui les garde.
  - **`gradle test` peut être vert sans rien exécuter** (mesuré deux fois). La CI
    passe `--no-build-cache` et compte les tests ; §8 devrait le dire aussi.
  - **Coordination avec la session 3.1** : `src/scheduler.rs` (non committé, pas
    encore déclaré nulle part → pas compilé) attend `crate::commands::into_result`
    et `crate::commands::DEEP_TIMEOUT` — les deux sont désormais visibles
    (`into_result` est même passé `pub` pour mon test). Il reste à ajouter
    `pub mod scheduler;` **dans `src/lib.rs`**, qui est le fichier que j'ai créé,
    et non de recréer des `mod` dans `main.rs`.
  - **`.gitignore` cassé par une autre session** : faute de retour à la ligne
    après `*.tsbuildinfo`, le motif est devenu `*.tsbuildinfo# Scripts de
    mesure…`, donc `apps/desktop/tsconfig.tsbuildinfo` **n'est plus ignoré** (il
    apparaît en `??`). Une seule ligne à ajouter, mais le fichier est modifié et
    non committé par une autre session : je n'y touche pas.
  - Rust n'est pas épinglé (pas de `rust-toolchain.toml`) : la CI prend `stable`.
    À figer quand la phase 5 démarrera pour de bon.

### 2026-10-05 — Spike phase 0 (0.12)

- Plan : écrire `docs/spike-phase0.md` et figer les commandes de vérification.
  Choisi pour cette période parce que c'est le seul travail **sans fichier
  partagé** — les deux autres sessions écrivent dans `src-tauri`, et un doc ne
  peut pas entrer en conflit avec elles.
- **Commande fictive trouvée.** `AGENTS.md` §8 disait `./gradlew test`. Vérifié :
  `Get-ChildItem gradlew*` ne renvoie rien — Gradle est installé globalement via
  Scoop, il n'y a pas de `gradlew.bat` dans le dépôt. La commande n'avait **jamais
  pu tourner**, et figurait dans le document de règles depuis le début. C'est
  exactement ce que la règle d'or n° 2 interdit. Corrigée : `gradle test` depuis
  `engine/`, et chaque commande porte maintenant son dossier.
- **Moteur mesuré pour la première fois**, en le lançant exactement comme le
  superviseur (mêmes options JVM — j'avais d'abord mis `-Xmx1g` et oublié
  `serve`, le moteur sortait en 323 ms sans répondre et la mesure ne voulait rien
  dire) : bannière 880 ms, `hello` 104 ms, `listSources` 180 ms pour 1 370
  sources (47 417 octets), arrêt propre 1 208 ms, code de sortie 0. RAM 68,5 Mo au
  démarrage → **136,5 Mo après le premier appel réseau**, puis plate.
- **Ce que je n'ai pas mesuré, et dit comme tel** : la *cause* des 57 Mo ajoutés.
  L'hypothèse (3 844 classes de parseurs) est plausible mais demanderait
  `-verbose:class`. Et MANGAFR a renvoyé 0 octet, MANHWA 0,1 Ko : ces deux
  appels ne disent rien sur un vrai parcours, donc je ne les cite pas comme
  « source rapide ».
- **Trouvé en passant** : le cache d'images est borné en nombre (`CAP_PAGE =
  2048`, éviction qui supprime bien le fichier) mais **pas en volume**. Mesure :
  260,5 Ko de moyenne, 6 121 Ko max → **~521 Mo possibles** pour une base de
  64 Ko. Croissance non bornée et silencieuse ; rejoint 2.5.
- Porte vérifiée réellement exécutée : `gradle test` = **24 tests, 0 échec**
  (5 fichiers XML). Un `UP-TO-DATE` seul ne prouve rien.
- `docs/spike-phase0.md` : 215 lignes, aucune mesure inventée, chaque limite
  énoncée.

### 2026-10-05 — Lecteur paginé (1.8)

- Plan : auditer ce qu'il restait vraiment de la 1.8 avant de coder. Sens de
  lecture, zoom, double page et RTL étaient déjà faits ; le `PLAN.md` disait
  « la précharge ne couvre que la page suivante », et la molette n'était pas
  mentionnée du tout.
- **Ce que j'ai d'abord conclu, et qui était faux** : « la précharge ne fait
  rien ». Mesuré après 30 s d'attente, zéro écriture sur le disque. Cause : je
  supprimais le fichier de cache **après** que Chromium l'avait déjà en
  mémoire, donc son cache HTTP répondait en 4 ms et Rust n'était jamais appelé.
  La preuve était dans l'**écriture disque** d'une page jamais demandée, pas
  dans un chronomètre — avec `Network.setCacheDisabled` pour écarter le second
  cache, la balise a écrit 147 Kio en moins de 2 s. Même famille d'erreur que le
  scroll : 173 px de course sur une page de 918 px, c'est le maximum, pas un
  blocage.
- Codé : `PRECHARGE_PAGES = 4` (une balise par page, plafond volontaire pour ne
  pas télécharger ce que l'utilisateur n'atteindra pas), et `onMoletteStage` sur
  `.reader-stage` avec seuil de 40 px, verrou de 300 ms par sens, et `Ctrl`
  réservé au zoom. En webtoon la molette n'est pas interceptée.
- Preuve (sur l'app réelle, port CDP 9223) : 4 balises distinctes, aucune visible
  (`getBoundingClientRect` 1×1), image affichée 800×4375 ; molette delta 300 →
  page change, delta 12 → rien, delta −300 → recul ; **contrôle négatif webtoon** :
  la bande défile de 660 px et le compteur ne bouge pas.
- **Bug trouvé en passant, non demandé** : `apparence.ts` et `i18n.ts` posaient
  `data-apparence` et `lang` sur `<html>` au chargement du module, donc **avant**
  l'hydratation de React, qui signalait l'écart en boucle à chaque navigation.
  Corrigé par un script bloquant dans `layout.tsx` + `suppressHydrationWarning`.
  Pour le lire, `console.error` ne suffit pas (message tronqué) : il faut un
  `addInitScript` qui intercepte avant React — la fin du message porte le diff.
- **Non vérifié** : la lecture de ce correctif d'hydratation *sur l'app*. La
  session 2 écrit dans `src-tauri`, ce qui redémarre Tauri à chaque modification
  et fait tomber le port 9223. Vérifié sans elle : le script est bien dans le
  HTML produit et précède React (les 5 bundles qui le précèdent portent
  `async=""`). **À confirmer à l'œil dès que les sessions Rust s'arrêtent.**

### 2026-10-05 — Thème clair (1.10)

- Plan : ajouter le thème clair et le sélecteur d'apparence. Deux surprises
  mesurées avant de coder, qui ont changé le contenu de la tâche.
- **Le vrai travail était la tokenisation.** La feuille contenait **69 couleurs
  écrites en dur** hors des tokens ; un bloc `[data-apparence='clair']` qui ne
  surcharge que des tokens ne les aurait pas fait basculer. Il a fallu créer les
  tokens manquants (`--line-strong`, `--accent-hover`, `--accent-dim-texte`,
  `--danger-*`, `--succes-*`, `--favori`, `--voile`, `--sur-voile`…), puis
  remplacer chaque usage. Contrôle final : **0** couleur en dur hors tokens (les
  8 couleurs de catégories sont des données, pas du style, et restent).
- **L'état devait vivre dans le module.** Première version : `useState` +
  appel à `setApparence`. Le setter du composant était **écrasé par l'import du
  module** — React n'était jamais prévenu, et le `<select>` affichait une valeur
  que rien ne portait. Mesuré : `localStorage` restait vide après un clic. Corrigé
  par `useSyncExternalStore`, comme la langue.
- **Deux contrastes trouvés en mesurant, pas en relisant le CSS :**
  - le texte posé sur `--accent-dim` : `--text` ne donnait que **2,98:1** en
    thème clair. Il fallait son propre token (`--accent-dim-texte`), car un voile
    à 12 % n'a pas la même profondeur sur fond noir et sur fond blanc → **9,22:1** ;
  - la bordure de champ : `#b4b8c2` ne faisait que **1,99:1** sur blanc, la
    bordure était invisible. Une bordure demande 3:1, pas 4,5:1 — ce n'est pas
    du texte (WCAG 1.4.11) → `#8b8f99`, **3,24:1**.
- Le **lecteur reste sombre dans les deux thèmes**, par choix : les pages sont
  majoritairement blanches, un fond clair ferait disparaître leurs bords. Les
  voiles posés sur les images aussi — ils sont faits pour être sombres.
- Corrigé au passage : `document.documentElement.lang` valait `fr` pendant que
  l'écran affichait l'anglais. L'attribut n'était posé que dans `setLang`, jamais
  au démarrage ; un lecteur d'écran annonçait donc la mauvaise langue.
- Preuve : bascule depuis l'interface, les deux thèmes, quatre vues capturées
  plus le lecteur sur un chapitre réel. Contrastes mesurés —
  texte/fond **21,00 → 16,17**, muted **7,37 → 4,59**, bouton primaire
  **5,05 → 5,97**, accent **17,40 → 10,94**, chip pressé **14,97 → 9,22**,
  bordure de champ **1,79 → 3,24**. `localStorage tatami.apparence = clair`
  survit au rechargement. `tsc` 0, `lint` 0 erreur, `build` OK, `i18n:check` 152/152.

### 2026-10-05 — Dossier de téléchargement au choix + pause de la file

- Plan : deux demandes de l'utilisateur. (1) Pouvoir choisir le dossier de
  téléchargements. (2) Pouvoir mettre en pause un téléchargement en cours. La
  première touche au stockage et aux permissions, la seconde à la file : deux
  questions d'architecture posées **avant** de coder, l'utilisateur ayant tranché
  « sélecteur natif » et « pause au chapitre ».
- **Dossier de téléchargement.** Le chemin était figé dans
  `downloads_root()` (`%APPDATA%\Tatami\downloads`). Il est maintenant un
  réglage en base, appliqué par Rust :
  - table `settings` (`store.rs`, migration v7, `CREATE TABLE IF NOT EXISTS` —
    et non `if current < 7`, pour qu'un binaire plus ancien n'écrase jamais une
    base estampillée par un plus récent) + `get_setting` / `set_setting` ;
  - `downloads_root()` lit un global, initialisé **une fois** au démarrage
    depuis la base. Une lecture SQLite dans un chemin appelé des dizaines de
    fois par téléchargement n'apportait rien, et le rendait intestable ;
  - `verifier_dossier_ecriture` **essaie vraiment** d'écrire un fichier :
    un dossier existe sans pour autant être accessible, et l'échec
    n'apparaîtrait sinon qu'au premier téléchargement ;
  - `set_downloads_dir` valide **avant** d'enregistrer. Enregistrer un chemin
    invalide ferait échouer tous les téléchargements ensuite, sans retour
    possible depuis l'interface ;
  - sélecteur natif `tauri-plugin-dialog` + **seule** permission
    `dialog:allow-open` (pas `allow-save`, pas `allow-message`, pas le lot
    `dialog:default`). ADR 0004 révisée : elle interdisait tout plugin.
- **Pause.** La logique existait déjà (arrêt au chapitre suivant, file
  persistée et reprise) mais se lisait comme un abandon : le bouton disait
  « Arrêter après le chapitre en cours » et les éléments restants étaient
  marqués `annulé`. C'est trompeur, parce que la file **se reprend** — c'est
  exactement la différence entre un abandon et une pause. Libellé
  « Mettre en pause », état `pause` affiché « en pause », info-bulle qui
  explique la sémantique (« le chapitre en cours est terminé, puis la file
  s'arrête ; reprendre la reprendra où elle s'est arrêtée »). Au passage :
  « Effacer la file » était du français en dur, hors i18n.
- Preuve :
  - `pnpm typecheck` 0 · `pnpm lint` 0 erreur (4 warnings `exhaustive-deps`
    antérieurs) · `pnpm build` OK · `pnpm i18n:check` **138/138**
  - `cargo fmt --check` 0 · `cargo clippy --all-targets -D warnings` 0 ·
    `cargo test` **60 passés, 0 échec** (dont `un_reglage_ecrit_relu_apres_redemarrage`)
  - Démarrage : `telechargements : \\?\C:\Users\alexanders\AppData\Roaming\Tatami\downloads`
    — la racine vient bien de la base, canonisée.
  - Interface réelle (CDP sur WebView2) : panneau « Dossier de téléchargement »
    présent, chemin affiché, aller-retour `get_downloads_dir` →
    `set_downloads_dir` → `get_downloads_dir` correct, `reset_downloads_dir`
    ramène à 100 % (13 webtoons de nouveau visibles).
  - Pause mesurée de bout en bout : `Mettre en pause` + badge
    `téléchargement` → 10 s plus tard badge **`en pause`** + panneau
    « File interrompue » ; `Reprendre la file` → badge `téléchargement`,
    bannière disparue. File persistée dans `localStorage` entre les deux.
  - La porte `aucune_commande_tauri_nest_morte` a **échoué** juste après
    l'ajout des trois commandes : elles n'étaient pas encore appelées par le
    front. Elle a fait son travail avant même que je pense à m'en servir.
- Points ouverts :
  - Changer de dossier **ne déplace rien** : les `.cbz` déjà écrits restent
    dans l'ancien dossier et la bibliothèque n'affiche plus que ceux du
    nouveau. C'est dit dans l'interface, mais un « migrer les fichiers
    existants » reste à faire si l'utilisateur le veut.
  - La granularité de la pause est le **chapitre**, par choix humain. Une
    pause à la page exigerait un `.part` réellement reprenable (offsets et
    entrées persistés) — chantier distinct, avec un risque sur les `.cbz`.
  - `escape` sur le dialogue natif **a confirmé** un dossier au lieu de
    l'annuler pendant un test CDP, ce qui a déplacé la racine le temps d'une
    mesure. Détournement de test, pas défaut de l'app, mais cela veut dire que
    le dialogue natif n'est pas testable par CDP : seul l'aller-retour de la
    commande l'est.
  - La file affichée n'a aucun réglage « arrêter après N chapitres » ; le seul
    contrôle est la pause.

### 2026-10-04 — Audit du code réel et correction des 5 constats

- Plan : audit du dépôt **sans se fier aux `.md`**, puis correction de chaque
  constat mesuré. Périmètre validé par l'utilisateur : le suivi AniList/MAL est
  **hors v1** (decision humaine : l'app sera terminee). L'audit a fait
  ressortir deux constats d'architecture et trois de robustesse.
- **Constat 1 — les couvertures contournaient le moteur.** Les 6 vignettes
  faisaient `<img src={coverUrl}>` sur une adresse de site, et la CSP
  l'autorisait (`img-src ... https:`) : violation directe du §4 règle 4, avec
  un échec silencieux (vignette vide, console propre). Corrigé de bout en bout :
  `fetchCover` côté moteur, `PageRef::Couverture` + `tatami://cover/<jeton>`
  côté Rust, `coverToken` dans toutes les réponses (moteur **et** base), `https:`
  retiré de la CSP. Au passage : index O(1) au lieu d'un balayage O(n²),
  plafonds d'éviction, écriture de cache atomique.
- **Constat 2 — le contrat était en retard de deux méthodes.** `latest` (qui
  porte l'essentiel du parcours) et `filterOptions` étaient implémentées et
  utilisées mais absentes de l'`enum` `method`. Une porte de validation contre
  le schéma serait passée à côté. `fetchCover` ajoutée, formes de `result`
  décrites, points ouverts honnêtes (`health` n'est appelé par personne).
- **Constat 3 — deux commandes mortes et deux sémantiques opposées.**
  `commands::search` (jamais appelée depuis la 0.9) envoyait `SAFE` par défaut
  quand `latest` n'en envoie aucun : même réglage, deux comportements
  contraires. `library::existing_chapters` remplacée par `missing_chapters`.
  Retirées. Nouvelle porte `aucune_commande_tauri_nest_morte`.
- **Constat 4 — cinq défauts sur le chemin de production d'un `.cbz`.**
  Collision de nom entre chapitres homonymes (« un `.cbz` par chapitre » n'était
  pas tenu), `note_chapter` ignoré en silence, noms réservés Windows non
  gérés, débordement `u32` tronqué sans bruit dans l'écrivain ZIP, `flush` au
  lieu de `sync_all` (donc un `.cbz` tronqué portant le nom définitif). Plus le
  verrou de téléchargement qui restait pris sur une panique.
- **Constat 5 — i18n mesuré sur le dictionnaire, pas sur l'affichage.**
  199 clés pour 108 utilisées : **91 clés jamais affichées**. Sept libellés
  restaient en français en dur. Nettoyé, plus `pnpm i18n:check`.
- **Bonus mesuré — le moteur jetait les requêtes en vol.** Cinq requêtes d'un
  bloc puis fermeture de stdin : **0 ligne sur stdout** (`shutdownNow()`
  interrompait les workers). Corrigé, avec test.
- Preuve :
  - `pnpm typecheck` 0 · `pnpm lint` 0 erreur (4 warnings `exhaustive-deps`
    antérieurs) · `pnpm build` OK · `pnpm i18n:check` 127/127
  - `cargo fmt --check` 0 · `cargo clippy --all-targets -D warnings` 0 ·
    `cargo test` **56 passés, 0 échec**
  - `gradle test` **10 passés, 0 échec** (dont `FermetureStdinTest`)
  - `fetchCover` sur MangaDex : **43 981 octets, signature `FF D8 FF E0`** ;
    `file://` refusé ; repli `coverUrl` OK
  - Fermeture stdin : **0 → 5 réponses** ; `shutdown` : 6/6
  - 4 commits : `7bab520`, `2d26a45`, `df77322`, `a7e83bb`, `4eedc99`
- Points ouverts :
  - **Suivi AniList/MAL, sync et notifications : hors v1** (décision humaine du
    2026-10-04). Le tableau des phases les garde visibles, pas engagements.
  - Aucune porte ne couvre encore le rendu visuel : `page.tsx` fait 3750 lignes
    et n'a aucun test. Une régression d'affichage ne serait vue qu'à l'écran.
  - `store.rs` : `history_title` est la seule brique encore verrouillée par
    version alors que le fichier prône la réparation par présence.
  - Un commit (`5be3849`) est apparu pendant l'audit sans que je l'aie lancé.

_Format d'une entrée :_

```
### AAAA-MM-JJ — Tâche X.Y
- Plan : (3 à 6 lignes)
- Fait :
- Preuve : commande + résumé de la sortie
- Points ouverts :
```

### 2026-10-04 — Tâche 0.19 : la fenêtre de vérification se déclenche vraiment

**Problème mesuré, pas déduit.** Le moteur ne signale `CHALLENGE_REQUIRED` que sur HTTP **403/503** avec un marqueur Cloudflare. Or HENTAIORIGINES répond **200 OK** avec la page HTML de son interstitiel de vérification d'âge (`document.documentElement.className += document.cookie.indexOf('ori_majeur=1')>-1?'':' ori-age-verif'`). Aucun challenge n'était donc jamais signalé, et rien ne s'ouvrait.

Sonde NDJSON directe sur le moteur (`sonde-ho.js`) :
```
pages     = 45
url image = { previewUrl: null, index: 0, url: '' }   ← VIDE, et non pas relative
contentType= text/html  |  taille = 193 283 octets
```
Les 45 URLs d'images sont **vides** : le téléchargement partait chercher l'adresse `""` et écrivait la page d'accueil du site 45 fois. Les 37 entrées identiques de 193 043 octets vues au diagnostic s'expliquent exactement.

**Correctif.** Rust déclenche la fenêtre de vérification (tâche 0.11, déjà construite) dans **deux** cas que le moteur ne voit pas :
1. une page sur N a une URL d'image vide → échec propre, aucun octet écrit ;
2. les octets reçus ne sont pas une image (`image::detect`) → échec propre, fenêtre ouverte.

Le chapitre reste **absent** du disque, donc la règle « ne jamais retélécharger ce qui est là » le retentera au lieu de le croire terminé. L'URL ouverte est `details.result.manga.publicUrl` (mesuré présent sur cette source : `https://hentai-origines.com/manga/i-love-you-very-much/`), sinon `mangaUrl`. Côté interface : un état `verif` distinct de `challenge` (celui-ci est une URL rattachée à la source ouverte, alors que l'évènement porte source + URL + motif et que l'utilisateur peut être sur la vue Téléchargements sans source sélectionnée), un panneau dans la vue Téléchargements, et 9 clés i18n FR/EN.

**Ce que ça ne fait PAS** : rien n'est contourné. L'utilisateur clique lui-même dans une vraie fenêtre de navigateur, sur le parcours prévu du site. La fenêtre utilise déjà l'agent utilisateur **du moteur** (`challenge.rs`), ce qui limite le risque de cookie refusé.

**Preuves — walk sur l'app réelle, 17/17** (`walk-verif.js`, deux contrôles) :
- **Négatif** (HENTAIORIGINES) : échec en 2 359 ms avec `« HENTAIORIGINES demande une vérification (45 page(s) sur 45 sans URL d'image). Une fenêtre va s'ouvrir : confirmez-y vous-même, puis relancez le téléchargement. »` · **0 `.cbz` écrit** (44 avant, 44 après) · 0 nouveau `.part` · fenêtre ouverte (`cancel_challenge` accepte) · panneau présent, visible dans la vue Téléchargements, avec « C'est fait, appliquer ».
- **Positif** (MANGADEX) : téléchargement **réussi** en 16 453 ms, `.cbz` de 8 099 239 octets. Contrôle par un lecteur ZIP **strict** (`zipfile`) : intégrité OK, 11 entrées, genres `{PNG: 2, JPEG: 9}`, **11 tailles distinctes** (donc 11 vraies pages différentes, pas 11 copies du même HTML), 0 entrée vide, médiane 761 959 octets. Sans ce contrôle, le test négatif ne prouverait que « rien de mauvais n'arrive » — pas que la correction n'a pas cassé ce qui marchait.
- Portes : `pnpm typecheck` / `pnpm lint` / `pnpm build` = 0 · `cargo test` **29 passés** · `cargo clippy -D warnings` = 0 · `cargo fmt --check` = 0.

**Limite honnête** : la dernière étape est manuelle et non automatisable (le clic « j'ai plus de 18 ans »). Que la pose du cookie rende les URLs d'images réelles **n'est pas encore prouvé** — c'est la prochaine étape, à faire à la main. Risque documenté dans `challenge.rs` : le cookie est lié au navigateur, et le moteur parle en agent Firefox pendant que la fenêtre est en Chromium.

### 2026-10-04 — Défaut trouvé par le walk : nom de `.cbz` en UUID

Mesuré : quand la source ne renvoie **pas** de titre de chapitre (`title: null`, cas de MANGADEX ici), `chapterLabel` retombe sur l'URL et le fichier s'appelle `22d20970-38d1-48aa-bbd3-9ca9afbbb3dc.cbz`. Le `.cbz` est valide et lisible par Tatami, mais **inutilisable** dans un lecteur externe, ce qui contredit l'intention de l'exigence permanente (« un `.cbz` par chapitre »). Nouvelle tâche 0.18.

### 2026-10-04 — Trait horizontal entre les pages du webtoon

Signalé par l'utilisateur : « il y a des espèces de trait horizontal quand je lis le webtoon ».

Cause : `.webtoon img` portait `border-bottom: 1px solid #0e0d13`, un liseré sombre ajouté en bas de chaque page.

**Cette bordure cassait aussi ma virtualisation, et c'est le plus important.** La virtualisation calcule la position de chaque page à partir d'une hauteur ; en `box-sizing: content-box` (le défaut CSS), toute bordure s'ajoute à cette hauteur. Mesuré : **1 px de dérive par page**, soit **303 px** sur Killer Peter — la fenêtre affichée finissait par ne plus correspondre à son contenu. Le correctif est `box-sizing: border-box` sur `.webtoon img`, plus la suppression du liseré que l'utilisateur ne veut pas.

Mesuré après correction (5/5) : `box-sizing` = `border-box`, 0 bordure, **écart entre hauteur rendue et hauteur calculée = 0** sur chaque image, **écart entre position rendue et offset calculé = 0**, et **aucun espace ni chevauchement aux jointures** entre pages.

L'écran ne peut pas confirmer visuellement : la capture CDP rend la bande noire dès qu'on descend dans le chapitre (au sommet elle fonctionne). C'est l'artefact déjà documenté, et il grandit avec la distance de défilement. Les mesures ci-dessus valent la preuve ; un coup d'œil de l'utilisateur confirmera.

### 2026-10-04 — Navigation dans la barre du lecteur + mode plein écran

**Demande de l'utilisateur** : « en haut, à côté de Signet, dans le même header ». La navigation webtoon occupait une **ligne séparée sous la barre**. Elle est désormais dans `.reader-bar`, juste après le bouton Signet, sous forme de deux flèches `←` `→` discretes. La ligne dédiée disparaît, ce qui rend aussi de la hauteur à l'image. Mesuré : Signet en x = 728, navigation en x = 802, plein écran en x = 900, tous dans la barre de 62 px ; la flèche `→` avance la bande (page 3, `scrollTop` 6 808) ; plus aucune navigation en bas.

**Plein écran.** Le plein écran porte sur l'élément `.reader`, pas sur `document` : la fenêtre de l'application reste visible si l'utilisateur en sort, et la navigation de Tatami ne disparaît pas définitivement. `:fullscreen` est une pseudo-classe CSS, donc l'état est lu **depuis le DOM** au lieu d'être deviné — c'est la seule source fiable, `Échap` et le bouton système pouvant sortir du plein écran sans passer par notre handler. Deux règles CSS étaient nécessaires : sans elles, `.reader` gardait `height: calc(100vh - s5*2)` et `margin: -s5`, ce qui laissait une bande vide en bas et un débordement latéral. Mesuré : hauteur 900 px pour une fenêtre de 900 px, bouton à `aria-pressed="true"`.

**Un vrai défaut trouvé par le walk : `Échap` fermait le lecteur ET le plein écran d'un coup.** Le handler faisait `closeReader()` inconditionnellement. En plein écran, `Échap` doit d'abord sortir du plein écran, sinon l'utilisateur perd sa page sans l'avoir voulu. Corrigé : si `document.fullscreenElement` est présent, on sort du plein écran et on s'arrête là. Mesuré : après `Échap`, plus de plein écran, lecteur **toujours ouvert**, bouton resynchronisé à `aria-pressed="false"`.

**Zone d'ombre de la tache 1.9 — tranchee.** Les captures de la bande webtoon revenaient noires depuis la tâche 1.9 ; tout indiquait un artefact (géométrie exacte, images décodées, pointage correct, fond rouge non peint, même jetons corrects en mode pagé). **Confirmé par l'œil** : la capture en plein écran montre l'image pleine largeur, correctement peinte. C'était bien la capture CDP au-dessus d'un conteneur de 686 000 px. Aucune correction de code n'était nécessaire — l'écart était dans la mesure, pas dans l'application. Leçon : devant une contradiction entre mesure et doute, chercher un troisième témoin (ici la capture plein écran, qui change le mode de rendu) avant de casser du code.

Portes : `pnpm typecheck` / `lint` / `build` = 0. Walk 11/11.

### 2026-10-04 — Demande utilisateur : bande webtoon pleine largeur, navigation en haut

**Pleine largeur.** Mesuré avant : 1 200 px de bande dans une fenêtre de 1 440, donc 120 px de marge de chaque côté — le lecteur était encadré. `.reader.pleine-largeur` retire le padding horizontal en mode webtoon ; les barres (titre, navigation) gardent leur marge, sinon leurs boutons colleraient aux bords. Après : bande à **1 420 px** (les 20 px restants sont la gouttière de la barre de défilement, `scrollbar-gutter: stable`) et image à 1 420 px.

**Navigation en haut.** En mode webtoon, la navigation était rendue **après** la bande : revenir en haut de l'écran à chaque changement de page revenait à parcourir 650 000 px sur un chapitre de 303 pages. Elle est désormais rendue juste sous la barre de titre. Un `order: -1` a été retiré après coup : `.reader-bar` est le premier enfant de `.reader` (`order: 0`), donc un `order: -1` aurait placé la navigation **au-dessus** d'elle. L'ordre du DOM suffit.

Mesuré : navigation à `top = 62 px`, sous la barre de titre (`top = 0`), les deux boutons présents, aucune navigation restante en bas, le bouton « suivante » avance bien la bande (page 3, `scrollTop` 6 808), et le **mode pagé garde sa navigation en bas** (non régressé). 9/9.

**Zone d'ombre non tranchée, à l'œil humain.** Les captures de la bande webtoon reviennent **noires**, alors que tout le reste est correct : géométrie vérifiée (boîtes 1 420 × 2 272 aux offsets 0 / 2 272 / 4 544), images **décodées** (`naturalWidth 800 × 1280`), `elementFromPoint` renvoie bien l'`IMG`, et **le fond rouge posé sur l'image ne peint pas non plus** — même en `position: static`, donc pas un problème de positionnement. Aucun `content-visibility` résiduel dans le CSS servi. Les mêmes jetons locaux s'affichent correctement en mode pagé. Tout penche vers un artefact de capture CDP au-dessus d'un conteneur de 686 000 px, mais **ce n'est pas prouvé** : il faut l'œil de l'utilisateur.

### 2026-10-04 — Tâches 1.9, 2.3 (suite), et les trois défauts que j'avais laissés

**1.9 — bande webtoon virtualisée.** Rust lit désormais la **taille réelle** de chaque page dans l'en-tête (`image::dimensions` : JPEG par parcours des marqueurs SOF, PNG via IHDR, WebP VP8/VP8L/VP8X), en ne lisant que **512 octets** par entrée — lire les images entières aurait relu ~90 Mo pour rien. L'interface en déduit la hauteur affichée, la position de chaque page, et ne monte qu'une **fenêtre glissante**. `flex: none` sur `.webtoon-inner` était indispensable : sans lui, le `height: 657 485px` posé en style inline était réduit par `flex-shrink` à 540 px, la barre de défilement mentait et la position sautait.

Preuves sur Killer Peter (303 pages dans un seul `.cbz`) : bande **657 485 px exacte**, **3 nœuds à l'ouverture et 6 en défilement au lieu de 303**, chaque cible atteinte au pixel près (50 000 → page 23, 200 000 → 92, 450 000 → 207, 650 000 → 299), position monotone, aucun trou blanc, compteur de page suit. Mode pagé non régressé (647 px dans un cadre de 691). 7/7.

Limite assumée : un chapitre lu **en ligne** n'a pas de dimensions (le moteur ne les renvoie pas encore), donc la virtualisation ne s'y applique pas et l'interface rend toutes les pages. Aucune estimation n'est faite dans ce cas — measured : une estimation fait perdre la position de lecture.

**2.3 — sélecteur de chapitre** dans la vue Téléchargements. Le bouton « Lire » ouvrait toujours le premier chapitre, ce qui est inutilisable sur une série de 137. Mesuré : Petite Rebelle affiche « Choisir un chapitre (37) », le dialogue liste 37 chapitres et tient dans l'écran (liste bornée à 52vh).

**Défaut d'interface corrigé** : le panneau de vérification n'existait que dans la vue Téléchargements, alors qu'un téléchargement peut être lancé depuis n'importe quelle vue — c'est exactement le « rien ne se passe »  experienced par l'utilisateur. Il est désormais rendu après la barre de navigation dans les **six** points d'entrée de vue.

**0.18 — nom de fichier lisible.** Un chapitre sans titre produisait `22d20970-38d1-48aa-bbd3-9ca9afbbb3dc.cbz` : valide, mais inutilisable dans un lecteur externe. Le nom retient maintenant le **numéro** de chapitre trouvé dans l'URL (`Chapitre 12.cbz`), et un UUID est **détecté explicitement** (`segments_uuid`, 8-4-4-4-12) avant tout repli. Ce n'était pas suffisant de filtrer les groupes de chiffres : l'UUID en contient, et le premier essai produisait `Chapitre 48.cbz` — échec révélé par le test.

Portes : `pnpm typecheck` / `lint` / `build` = 0 · `cargo test` **43 passés** · clippy 0 · fmt 0 · `gradle test` 9/9.

### 2026-10-04 — Tâche 2.3 : lecture hors ligne (le trou le plus visible de la v1)

L'application écrivait des `.cbz` qu'elle ne savait pas **lire**. Le lecteur passait toujours par le moteur, donc le seul moyen de lire un téléchargement était d'ouvrir un autre logiciel — exactement ce que l'utilisateur a fait.

**Rust** : `zipread.rs` (le pendant de `cbz.rs`), sans dépendance — `Cargo.toml` n'a aucun crate ZIP et en ajouter un est une décision d'architecture (AGENTS.md §9). Nos fichiers sont en `STORED` : lire ce qu'on écrit ne demande que le répertoire central. Une entrée `DEFLATE` (beaucoup de `.cbz` tiers) rend une erreur explicite au lieu de renvoyer des octets faux. **CRC32 vérifié à la lecture** : un `.cbz` corrompu est refusé, pas affiché. `PageRef` devient un enum `Moteur` / `Local`, donc **le lecteur n'a pas eu à changer** — il manipule toujours des jetons. Deux commandes : `local_chapter_pages` (recalcule le chemin avec la **même** fonction que `download_chapter`, pour que les deux règles ne puissent pas diverger) et `local_file_pages` (chemin fourni par l'interface, avec **confinement vérifié** sous le dossier de téléchargement, sinon l'interface pourrait faire lire n'importe quel fichier — AGENTS.md §7). Tri des pages par **numéro** et non alphabétiquement : `page_10` après `page_9`, sinon l'ordre s'inverse dès la dixième page, invisible sur une série de 9.

**Défaut structurel trouvé en route, préexistant.** Chaque vue s'affiche par un `return` conditionnel (`if (tab === 'downloads') { … return … }`), donc le composant retournait **avant** d'atteindre `if (reader)`. Le lecteur était structurellement inatteignable depuis la vue Téléchargements : le clic « Lire » ne faisait strictement rien — aucun message, aucune erreur console. Même famille que la leçon « une fonction qui sort en silence est un piège ».

**Preuves — 8/8 sur l'app réelle** : 14 images décodées, `src = http://tatami.localhost/page/l3713806…` (le préfixe `l` identifie un jeton local, donc aucun moteur impliqué) · mention « Hors ligne » · **moteur tué** (`taskkill /F /IM java.exe`) puis navigation : les images restent décodées. C'est cette dernière mesure qui distingue une vraie lecture locale d'un simple cache. Portes : `pnpm typecheck` / `lint` / `build` = 0 · `cargo test` **37 passés** · clippy 0 · fmt 0 · `gradle test` 9/9.

**Reste pour 2.3** : un sélecteur de chapitre dans la vue Téléchargements (le bouton « Lire » ouvre le premier chapitre téléchargé).

### 2026-10-04 — Tâche 1.9 (diagnostic) : le mode webtoon fonctionne mais n'est pas virtualisé

**Plainte de l'utilisateur** : « la page de lecture n'est pas vraiment longue ». Mesuré sur le plus long chapitre réel du disque — **Killer Peter, 303 pages dans UN seul `.cbz`**.

**Mode Webtoon, état réel** : bande de **657 788 px** de haut, images empilées à `width: 100 %` / `height: auto`, conteneur borné et défilant, `defile: true`. `scrollTop` = 48 000 avant **et** après un défilement de 12 × 4 000 px → **la position ne saute pas**. Coût : 174 ms pour ce défilement. **Le mode webtoon est donc correct et la bande est bien longue** : le reproche ne porte pas sur la longueur.

**Le vrai défaut** : `noeudsDom: 303` et `imagesChargees: 303` — le lecteur rend **toutes** les pages en DOM et le navigateur **charge les 303 images** à l'ouverture. Un chapitre de 303 pages coûte ainsi 211 requêtes `tatami://` dès le premier affichage. Ça passe sur cette machine, pas sur un portable modeste.

**Une tentative retirée, et pourquoi.** J'ai d'abord posé `content-visibility: auto` + `contain-intrinsic-size: auto 1200px` sur `.webtoon img`. Mesure : la bande annonçait **2 479 px** au lieu de 657 788, les 303 images se chargeaient **quand même**, et surtout `scrollTop` **reculait de 3 964 à 1 788** pendant le défilement — la position de lecture était perdue. Cause : la hauteur de réserve est une estimation, donc la barre de défilement est fausse, `loading="lazy"` croit que tout est proche, et la position est recalculée quand la vraie hauteur arrive. **CSS retiré**, retour mesuré au comportement correct et stable. Une virtualisation qui perd la position de lecture est pire que pas de virtualisation.

**La bonne voie, donc** : que Rust **renvoie les dimensions réelles** de chaque page (en-tête JPEG/PNG/WebP, déjà savoir le lire — `image::detect` le fait), pour que l'interface sache la hauteur exacte de chaque page. La hauteur de la bande devient juste dès le premier rendu, `loading="lazy"` fonctionne, et une **fenêtre glissante** peut alors ne rendre qu'une dizaine de nœuds au lieu de 303. C'est la suite de 1.9.

### 2026-10-04 — Tâche 0.20 : les images lazy-loadées sont illisibles par les parsers

**L'utilisateur a testé la tâche 0.19 et le résultat a été « rien ne se passe ».** Deux causes, mesurées séparément.

**1. Le cookie n'a jamais été transmis — défaut d'interface, mien.** Le panneau de vérification n'existait que dans la vue Téléchargements. L'utilisateur était dans l'onglet **Catalogues** : relevé sur l'app vivante, `panneauxH2: []` et `boutonsVisibles: []`. Il n'y avait rien à cliquer. Le panneau doit être global, pas lié à une vue.

**2. Mais surtout : le cookie n'aurait rien donné.** La source ne demande aucune vérification d'âge. Mesuré en récupérant la page du chapitre **sans aucun cookie** :
```
HTTP 200 | 162 740 caractères | 54 balises <img>
dont 45 balises WP-manga = exactement les 45 pages annoncées par le parser
< img id="image-0" data-src=" https://…/page_0001.webp"
      class="wp-manga-chapter-img img-responsive lazyload-ordered effect-fade" >
```
Les URL sont **dans `data-src`**, `src` est absent : le thème sert ses images en lazy-load. Et ces URL servent de **vraies WebP sans cookie** (HTTP 200, `image/webp`, 535 200 octets). Le HTML porte les URL ; c'est la lecture qui est en retard. Deux pièges mesurés : l'espace après `data-src="` (45/45) et le nom d'attribut `data-src`, pas `src`.

**Correctif : intercepteur HTTP du moteur**, `LazyLoadInterceptor`, branché dans `EngineContext`. Il fait ce qu'un navigateur fait : il place l'URL réelle dans `src` avant que le parser ne lise la page. Il ne touche que les `<img>` dont `src` est absent, vide ou placeholder ; une source qui fonctionne garde donc exactement son comportement. Il s'appuie sur `peekBody`, donc une page sans image lazy est renvoyée **octet pour octet**, charset d'origine compris. Attributs couverts : `data-src`, `data-lazy-src`, `data-original`, `data-original-src`, `data-echo`, `data-url`, `data-full-src`, `data-image`, plus `data-srcset`/`srcset` (plus grande variante).

Le correctif est **générique etfondu dans le moteur**, pas dans un parser : corriger le parser supposerait de forker `kotatsu-parsers-redo` (interdit par AGENTS.md §13), alors qu'adapter le contenu HTTP reçu est exactement le rôle du moteur. Plusieurs thèmes WordPress de lecture (Madara, MangaThemesia…) sont concernés.

Garde-fou ajouté dans `fetchImage` : une URL d'image vide lève désormais `ParamException` au lieu de partir sur l'origine du site. C'est ce silence qui avait produit les 37 `.cbz` pleins de HTML ; et le cache était en prime indexé sur cette chaîne vide, donc **les 45 pages partageaient la même clé**.

**Le moteur n'avait aucun test** : `build/test-results` était vide, donc la porte `gradle test` passait sans rien vérifier. Ajout de `testImplementation(kotlin("test"))` (test seul, sansJUnit) et de **9 tests** sur du HTML réel extrait de la page du 2026-10-04, dont les 45 balises. Un des tests a d'abord échoué **à tort** : il cherchait la sous-chaîne `src=" https://`, que `data-src=" https://` contient aussi — l'assertion visait l'attribut d'origine, intact.

**Preuves — 9/9 tests moteur, puis walk sur l'app réelle 9/10 (le 10ᵉ était une assertion fausse, voir plus bas).**
- Les 45 URL ne sont plus vides : `page_0001.webp` → `page_0045.webp`.
- Téléchargement **réussi en 12 750 ms** : `HENTAIORIGINES\I Love You Very Much\Chapitre 1.cbz`, 9 330 396 octets.
- Relecture par un lecteur ZIP **strict** : intégrité OK, **45 entrées, toutes WEBP**, **45 tailles distinctes**, 0 entrée vide, médiane 234 594 octets, premier octet `52494646…57454250` = `RIFF`/`WEBP`.
- **Non-régression** : MANGADEX télécharge toujours.
- Aucune fenêtre de vérification ne s'ouvre plus (1 seule fenêtre réelle, comptée via CDP).

**Ce qui reste imparfait, mesuré :** `032.webp` fait 310 octets. Décodé, c'est un WebP valide de **413×42 px** — un placeholder servi par le site lui-même. 44 pages réelles sur 45. Résiduel côté site, pas côté Tatami.

**Une assertion fausse à signaler :** j'ai voulu prouver « plus aucune fenêtre de vérification », en appelant `cancel_challenge`. Or cette commande fait `if let Some(w) = … { w.close() }` puis `Ok(())` **inconditionnellement** : elle réussit donc toujours, même sans fenêtre. Le test passait sans prouver rien. Remplacé par un comptage des fenêtres réelles via CDP (`pages.length === 1`). Retenir : **une commande d'annulation qui réussit toujours ne peut pas servir de preuve d'état**.

### 2026-10-04 — Correctif : des `.cbz` illisibles (signalé par l'utilisateur)

- Symptôme : « j'ai téléchargé des webtoon, ouvert le `.cbz` avec un logiciel dédié, je ne voyais rien — tout noir ».
- Diagnostic (mesuré, pas supposé) : les 44 `.cbz` du dossier de téléchargement ouverts avec un lecteur **strict** (`zipfile` de Python). **37 sont corrompus**, tous de HENTAIORIGINES. Les 7 autres (MANGADEX, MANGASORIGINES) sont des ZIP valides, intégrité vérifiée, vraies images JPEG (`ffd8ffe0`). Dans les 37, **toutes** les entrées contiennent la même chose : `202020200a3c21444f4f435459` = `"    \n<!DOC TY` — du **HTML**. Contenu lu : la page du site, avec `ori-age-verif`, son **interstitiel de vérification d'âge**.
- Cause : trois défauts enchaînés.
  1. `fetchImage` renvoie `200 OK` avec le HTML de l'interstitiel : aucune erreur, donc rien ne signale le problème.
  2. `download.rs` écrivait les octets **sans jamais vérifier que c'est une image**.
  3. `cbz.rs` déduisait l'extension du `contentType` annoncé, et `extension()` retombait sur `jpg` pour tout ce qui n'était pas png/webp/gif : le HTML partait donc sous le nom `001.jpg`.
  Aggravant : la logique « ne jamais retélécharger ce qui est là » voyait ces fichiers comme téléchargés — **ces chapitres étaient irrécupérables** sans suppression.
- Correctif : nouveau module `image.rs` avec `detect()` (signature sur les octets : JPEG, PNG, WebP, GIF, BMP) et un message d'erreur parlant. Appliqué aux **deux** sites d'écriture : le `.cbz` et le cache du lecteur (`pagecache.rs`, qui lui aussi écrivait sans vérifier). L'extension écrite vient désormais des **octets**, jamais du `contentType`. `add_image` (basé sur le `contentType`) est **supprimé** : c'était précisément l'API fautive, la garder aurait laissé le pied de gun. Le doublon `guess_ct` de `pagecache.rs` a disparu au profit du détecteur partagé.
- Preuve : `cargo test` → **29 passés** (dont `une_page_web_nest_pas_une_image`, qui rejoue le cas réel : l'interstitiel mesuré) · `cargo clippy -D warnings` → 0 · `cargo fmt --check` → 0.
- **En attente de décision humaine** : les 37 fichiers corrompus sont toujours sur le disque. Ils sont reconnus comme téléchargés, donc il faut les supprimer pour pouvoir retélécharger — mais HENTAIORIGINES est **derrière une vérification d'âge**, donc le retéléchargement échouera tant que la vérification n'est pas faite (l'interface doit le dire au lieu d'écrire du noir).

### 2026-10-04 — Tâche 1.10 (partiel) : i18n français / anglais
- Plan : (1) construire la couche i18n avec une **garantie de compilation**, pas seulement un dictionnaire ; (2) convertir écran par écran ; (3) prouver sur l'app réelle que la bascule est immédiate, complète et persistante.
- Fait : `app/i18n.ts`, sans dépendance. Deux propriétés voulues :
  1. **Exhaustivité vérifiée par le compilateur** — `EN` est typé `Record<Cle, string>`, donc une clé française sans traduction fait échouer `pnpm typecheck`. Aucun trou silencieux possible, contrairement à un `JSON` que rien ne vérifie.
  2. **Conversion possible par écrans** — `t()` retombe sur le français, donc convertir l'interface morceau par morceau n'affiche jamais une clé brute.
  La langue est une préférence d'affichage : `localStorage` (`tatami.lang`), conformément à AGENTS.md §6. Sélecteur dans les Réglages, `document.documentElement.lang` suit.
- Converti : navigation, Réglages (contenu, catégories, langue, réseau), accueil (continuer, signets, favoris, filtres), catalogues, téléchargements, fiche, lecteur, et tous les messages d'erreur.
- **Une fuite que seule la capture a révélée** : le walk annonçait 15/15 alors que le titre de l'accueil et le compteur étaient restés en français — mes assertions regardaient les bandeaux et les boutons, pas le `h1`. Les deux ont été convertis **et** ajoutés au walk. Un test qui passe peut masquer ce qu'il ne regarde pas.
- Preuve :
  - `pnpm typecheck` / `pnpm lint` / `pnpm build` → 0 / 0 / 0.
  - Parcours sur l'app réelle via CDP (`walk-i18n.js`) : **17 mesures, 0 échec** — onglets FR puis EN, titre de vue, sections, `html lang`, les trois écrans, retour au français, survie au rechargement, et **aucune clé brute affichée** (détectée par balayage du DOM).
  - Captures : `audit/tatami4/23-anglais-reglages.png`, `audit/tatami4/24-anglais-accueil.png`.
- Points ouverts : les **titres de mangas restent dans leur langue d'origine** (volontaire : on ne traduit pas les œuvres). Restent à convertir les libellés menores : sélection de chapitres, fiche détaillée, invite de vérification Cloudflare.

### 2026-10-04 — Tâche 1.5 : catégories de favoris
- Plan : (1) demander à l'humain une catégorie ou plusieurs, plutôt que de deviner (deux applications de référence ne font pas la même chose) ; (2) table `categories` + `favorites.category_id` ; (3) le prouver sur l'app réelle, y compris la suppression d'une catégorie.
- Décision humaine : **une seule catégorie par favori**, comme dans Kotatsu-Redo, l'app dont on vise la parité.
- Fait : table `categories` (schéma **v6**) avec nom unique, index de couleur et position ; `favorites.category_id` en clé étrangère `ON DELETE SET NULL`. Commandes `list_categories`, `add_category`, `rename_category`, `delete_category`, `reorder_categories`, `set_favorite_category`. Interface : sélecteur sur la fiche, section de gestion dans les Réglages (renommage, 8 couleurs, monter/descendre, supprimer, ajout), pastille de couleur sur la carte, et filtre sur l'accueil (« Toutes », une pastille par catégorie avec son compte, « Sans catégorie »).
- Choix : la base ne stocke qu'un **index** de couleur (1..8), jamais une valeur CSS ; l'interface la traduit en variables `--cat-1`…`--cat-8`. Respecte AGENTS.md §9.2 (aucune valeur magique dans le code). L'ordre est réécrit en renvoyant **tous** les identifiants, pas un échange de deux lignes : l'ordre final est alors exactement celui demandé.
- **Un défaut trouvé en mesurant** : les Réglages n'affichaient aucune erreur. Un échec déclenché dans cet écran (catégorie en double) était totalement invisible — l'utilisateur cliquait et rien ne se passait. Les quatre autres vues affichaient déjà cette erreur ; les Réglages ont été alignés.
- Preuve :
  - `cargo test` → **26 passés, 0 échec** (dont `supprimer_une_categorie_ne_supprime_pas_ses_favoris`, `affecter_une_categorie_inexistante_echoue`, `les_categories_ordonnees_et_sans_doublon_de_nom`).
  - `cargo clippy -D warnings` → 0 · `cargo fmt --check` → 0 · `pnpm typecheck` / `pnpm lint` / `pnpm build` → 0.
  - Parcours sur l'app réelle via CDP (`walk-categories.js`) : **20 mesures, 0 échec** — création, doublon refusé **avec message**, réordonnancement, affectation depuis la fiche, filtre exact, pastille réellement peinte (`rgb(54, 106, 211)`), catégorie vide qui le dit, survie au rechargement, et suppression qui **conserve** le favori.
  - Captures : `audit/tatami4/20-reglages-categories.png`, `audit/tatami4/21-favori-filtre.png`.
- Note de méthode : une mesure échouait alors que l'application était correcte — mon assertion cherchait `existe deja` sans accent alors que le message dit « existe déjà ». Un walk qui coule sur une regex accentuée prouve un défaut de regex, pas un défaut de l'app.

### 2026-10-04 — Tâche 1.7 : signets
- Plan : (1) table `bookmarks` (schéma **v5**) avec la page dans la clé, pour pouvoir marquer plusieurs pages d'un même chapitre ; (2) bouton dans le lecteur + section sur l'accueil avec réouverture au bon endroit ; (3) prouver la persistance et l'exactitude de la page.
- Fait : `add_bookmark` / `remove_bookmark` / `list_bookmarks` (+ filtre par webtoon et limite). Bouton « Signet » dans la barre du lecteur (état actif + libellé « Signet ✓ », `aria-pressed`). Section « Vos signets » sur l'accueil : couverture, titre, source, chapitre, page, ancienneté, « Rouvrir » et « Retirer ». `ouvrirSignet` recharge la fiche, les chapitres, puis rouvre à la page exacte.
- Choix : pas de commande `has_bookmark`. Un seul appel `list_bookmarks` alimente l'état du bouton du lecteur et l'affichage ; une commande de plus aurait été du poids mort (`clippy` l'a d'ailleurs signalé comme morte). L'état du bouton du lecteur est donc dérivé d'une liste — si l'utilisateur dépasse 200 signets, les plus anciens ne sont plus en mémoire (limite documentée dans la commande).
- **Deux bugs réels trouvés en mesurant :**
  1. `ouvrirSignet` n'ouvrait **rien** : `openReaderAt(c, index, sourceName)` sort en silence quand aucune source n'est sélectionnée, ce qui est exactement le cas depuis l'accueil. Le nom de source doit être passé explicitement, comme le fait déjà `resumeReading`.
  2. Un walk précédent mesurait « ouvert à la page 1 » alors que l'app ouvrait bien la page 9 : la forme chaînée `locator(...).locator(...).click()` visait une ligne périmée. `.first()` + un affichage de la ligne réellement cliquée ont rendu la mesure fiable. **Encore un cas où le test, pas l'app, était faux.**
- Preuve :
  - `cargo test` → **23 passés, 0 échec** (dont `un_signet_memorise_la_page_exacte`, `les_signets_se_lisent_du_plus_recent_au_plus_ancien`, `un_signet_ne_touche_pas_l_historique` — marquer une page ne fabrique pas d'historique, retirer un signet n'efface rien).
  - `cargo clippy -D warnings` → 0 · `cargo fmt --check` → 0 · `pnpm typecheck` / `pnpm lint` / `pnpm build` → 0.
  - Parcours sur l'app réelle via CDP (`walk-signets.js`) : **22 mesures, 0 échec** — marquage, libellé, page exacte, pas de doublon en marquer/retirer/marquer, deux pages distinctes, section d'accueil, réouverture à la **page 9/9**, bouton déjà marqué, retrait, survie au rechargement, et table vide après nettoyage.
  - Captures : `audit/tatami4/18-signets-accueil.png`, `audit/tatami4/19-signet-ouvert.png`.
- Points ouverts : pas encore de pastille de signet sur les chapitres de la fiche (un signet n'est donc visible que depuis l'accueil et le lecteur) ; les signets ne sont pas encore exportables ni attachés à une catégorie (1.5).

### 2026-10-04 — Tâche 1.3 : tri des titres (alphabétique, choisi avec l'utilisateur)
- Plan : (1) demander à l'humain quel tri, plutôt que d'inventer (le moteur n'expose pas de tri « dernière mise à jour ») ; (2) implémenter alphabétique ; (3) prouver sur l'app réelle que l'ordre est bien appliqué et qu'il revient à l'ordre du site.
- Décision humaine : **alphabétique seul** (ordre du site / A → Z / Z → A). Écarté : « dernier chapitre en premier », trop approximatif sur les sources sans dates.
- Fait : barre de liste (nombre de webtoons à gauche, tris à droite), tri appliqué à **tout ce qui est chargé** et non à la seule page courante — sinon l'ordre changerait à chaque « Voir plus ». `localeCompare` sans locale explicite (celle du navigateur) range correctement les accents, `numeric: true` place « Ch. 2 » avant « Ch. 10 ». L'état du tri est global, il survit donc au changement de source.
- Preuve :
  - `pnpm typecheck` / `pnpm lint` / `pnpm build` → 0 / 0 / 0. Aucun changement Rust.
  - Parcours sur l'app réelle via CDP (`walk-tri.js`) : **11 mesures, 0 échec** — les 3 tris présents, « Ordre du site » par défaut, A→Z effectivement trié, Z→A = inverse **exact** d'A→Z, retour à l'ordre du moteur, et conservation après changement de source.
  - Capture : `audit/tatami4/17-tri-az.png`.
- Point de méthode : ma première version du walk lisait les titres avec un sélecteur faux et renvoyait des listes **vides** — trois mesures passaient alors à vide sans rien prouver. Un garde-fou « les titres sont bien lus dans le DOM (≥ 3) » a été ajouté avant de croire le résultat. Une mesure qui passe sur un tableau vide ne prouve rien.

### 2026-10-04 — Tâche 1.2 : activation/désactivation des sources
- Plan : (1) auditer 1.2 et 1.3 avant d'ajouter 1.7, dans l'ordre du plan ; (2) combler le seul manque de 1.2 (activation/désactivation) ; (3) le prouver sur l'app réelle, persistance comprise.
- Fait : table `sources_state` (schéma **v4**, sauvegarde avant migration vérifiée dans le journal : `tatami-v3-1791099395803.db`). Seules les sources **désactivées** sont écrites : l'absence de ligne signifie « active », donc pas de `DEFAULT` à maintenir. Commandes `list_disabled_sources`, `set_source_enabled`, `enable_all_sources`. Côté interface : interrupteur sur chaque tuile de source, compteur « n désactivée(s) », « Afficher les n sources désactivées », « Tout réactiver », et un **état vide** explicite quand la recherche ne donne rien (avant : une grille vide sous un bandeau vide, capturé à l'écran).
- Choix d'implémentation : la tuile est passée de `<button>` à `<div class="slot">` contenant le bouton d'ouverture et, en **frère**, l'interrupteur — un `<button>` dans un `<button>` est du HTML invalide (cf. AGENTS.md §14). Le CSS de `.card` est resté intact, la grille garde la même largeur (266 px mesurés avant/après).
- Preuve :
  - `cargo test` → **20 passés, 0 échec** (dont `les_sources_desactivees_sont_memorisees` et `desactiver_une_source_ne_touche_pas_les_favoris` — écarter une source ne perd ni l'historique ni les favoris).
  - `cargo clippy -D warnings` → 0 · `pnpm typecheck` / `pnpm lint` / `pnpm build` → 0.
  - Parcours sur l'app réelle via CDP (`walk-sources.js`) : **16 mesures, 0 échec** — désactivation, disparition de la liste, compteur, survie au rechargement complet, réapparition atténuée à la demande, « Tout réactiver », base vide à la fin.
  - Audit d'accessibilité : 48 interrupteurs sur 48 tuiles, tous `role="switch"` et nommés (« Désactiver la source MANGABUFF »).
  - Captures : `audit/tatami4/14-source-desactivee.png`, `15-sources-actives.png`, `16-grille-sources.png`.
- Points ouverts : **le tri de 1.3 reste à faire** — quel tri ? (ordre du site / alphabétique / dernière mise à jour) : choix à confirmer avant de coder, le moteur n'exposant pas de tri « dernière mise à jour » sans requête supplémentaire par source.

### 2026-10-04 — Tâches 1.1, 1.4, 1.6 : favoris, accueil, incognito, reprise
- Plan : (1) prouver que favori + historique survivent à un vrai redémarrage de l'application ; (2) corriger ce qui casse l'incognito ; (3) ne déclarer 1.1/1.4/1.6 fait qu'avec une preuve exécutée.
- Fait : base SQLite (schéma v3) avec `favorites` et `history` ; accueil par défaut avec « Continuer la lecture », « Vos favoris », interrupteur Incognito et « Vider l'historique » ; `resumeReading` rouvre le dernier chapitre à la dernière page ; `note_content_rating` apprend le classement d'un titre déjà connu (historique **et** favori) sans jamais créer de ligne.
- **5 défauts trouvés en mesurant, tous corrigés :**
  1. l'accueil ne chargeait rien au démarrage (`tab` vaut « home » sans passer par `goTab`) ;
  2. la reprise écrasait le classement connu par `NULL` → `COALESCE(NULLIF(?5,'UNKNOWN'), …)` ;
  3. les deux sources de classification se contredisent (WAMANGA : `ADULT` dans la liste, `UNKNOWN` sur la fiche ; MANGABUFF l'inverse) → on prend la première valeur connue, quel que soit l'endroit ;
  4. le réglage « Toutes » envoyait `SAFE` (chaîne vide interprétée comme SAFE côté Rust) : WAMANGA renvoyait 0 manga. Corrigé par `safeOnly: false` + `requested_rating` ;
  5. `user_version` estampillée sans la colonne : un binaire intermédiaire (version poussée dans une édition, migration dans l'édition suivante) a laissé la base réelle cassée définitivement. Les colonnes sont désormais ajoutées par **présence** (`PRAGMA table_info`), pas seulement par version.
- Preuve :
  - `cargo test --manifest-path apps\desktop\src-tauri\Cargo.toml` → **18 passés, 0 échec** (dont `une_version_stampillee_sans_la_colonne_est_reparee`, `la_migration_idempotente_ne_casse_rien`, `une_chaine_vide_ne_signifie_pas_safe`), stable sur 2 exécutions.
  - `cargo clippy --all-targets -- -D warnings` → 0 · `cargo fmt --check` → 0 · `pnpm typecheck` / `pnpm lint` / `pnpm build` → 0.
  - Parcours complet sur l'app réelle via CDP (`walk-final.js`) : catalogue → fiche → lecture → favori → retour accueil → **19 mesures, 0 échec**. Dont `l'incognito retire exactement les titres adultes = 1`.
  - **Redémarrage réel** (`redemarrage.js`, processus tué puis relancé) : **9 mesures, 0 échec** — le favori revient avec son classement `ADULT`, l'historique revient avec `3/3` et `1/9`, l'incognito reste actif, le titre adulte reste masqué de « Continuer » et le favori reste visible.
  - Captures : `audit/tatami4/11-accueil-incognito.png`, `audit/tatami4/13-redemarrage.png`.
- Points ouverts : 1.5 reste à faire pour les **catégories** (le favori simple est en place) ; le classement d'un titre jamais ouvert et jamais mis en favori reste `UNKNOWN` et donc visible sous incognito — choix prudent assumé, documenté dans l'interface.

### 2026-10-03 — Analyse de sushiscan.net (site de référence, aucune source ajoutée)
- Plan : (1) ne PAS toucher aux sources malgré la présence de `SushiScan` dans le JAR ; (2) analyser l'architecture, les écrans et le style pour adapter l'interface de Tatami.
- Fait : site mesuré au navigateur (4 captures). **Architecture** : WordPress + thème custom `wp-content/themes/sushiscan` (dérivé Madara) + LiteSpeed Cache + Cloudflare devant (`cf-cache-status: HIT`, `cdn-cgi/challenge-platform` = détection active). Pas d'API REST de contenu : la 1re page du chapitre est dans le HTML (`img.ts-main-image`), les suivantes par `POST /wp-admin/admin-ajax.php`. **Images** : CDN séparé `c.sushiscan.net`, WebP, nommage `OPChap1180-01.webp`. **Routes** : `/catalogue/<slug>/`, `/<slug>-chapitre-<N>/`, `/catalogue/?order=update`, `/random`, plus Liste de Lecture et Historique.
- **Lecteur mesuré** : 4 contrôles (`chapter`, `readingmode` = *Lecture Verticale* | *Lecture par Page*, `customreader` = *Par défaut* | *Ajusté à la Hauteur*, `select-paged` = `1/15`). En mode page 1 seule image dans le DOM ; en mode vertical 11 images sur 15, page de 17 885 px de haut.
- **Tokens mesurés** : fond accueil `#000000`, fond lecteur `#16151D`, texte `#B8B8B8`, Roboto 14px, contrôles `bg #111` + `border 2px #444` + `radius 20px` + `13px/600`, couverture `radius 6px` en `loading=lazy` affichée 167×228, bandeau de section `bg #111` + `radius 5px 5px 0 0` + `padding 5px 20px` + `14px/600`.
- Preuve : captures `audit/sushi/{1-accueil,2b-detail-complet,3-lecteur,4-catalogue}.png` ; mesures dans `audit/sushi/mesures.txt` et `livraison.txt` (scripts `analyse-sushi*.js`).
- **À retenir** : le parser `mangareader/fr/SushiScan` du JAR cible la famille MangaReader.to, **pas** ce site (schéma d'URL différent). Le DOM est bien dérivé Madara, donc un parser Madara sur mesure fonctionnerait. Non fait, hors demande.
- Points ouverts : les pubs (3 emplacements dans le flux) et le logo servi en 3965×660 pour 180×30 sont des défauts, pas des références à copier.

### 2026-10-04 — Résolution des deux points ouverts (lint, couvertures absentes)
- Plan : (1) remettre `pnpm lint` en service selon la doc Next 16 ; (2) supprimer les 2 erreurs et les avertissements réels ; (3) remplacer les rectangles de couverture vides par une pastille d'initiale ; (4) repasser la passe visuelle.
- **Lint** : `next lint` a été retiré de Next 16 et `next build` ne lance plus le linter. Installation de `eslint` + `eslint-config-next@16.3.8`, création de `apps/desktop/eslint.config.mjs` (`defineConfig` + `globalIgnores` + `eslint-config-next/core-web-vitals`), script passé à `eslint`. **Piège mesuré** : `eslint-config-next@16.3.8` déclare le peer `eslint >=9.0.0` mais son `eslint-plugin-react@7.37.5` échoue sur ESLint 10 (`contextOrFilename.getFilename is not a function`) → **ESLint 9.39.5 épinglé**.
- **2 erreurs corrigées** : `react-hooks/set-state-in-effect` sur l'effet de montage (bloqué par un `eslint-disable` borné à cet effet, avec le motif : en export statique le HTML est produit au build, les préférences ne peuvent pas venir d'un initialiseur paresseux sans créer un écart serveur/client) et `react/no-unescaped-entities` (apostrophe échappée en `&apos;`).
- **Avertissements traités** : `role="tab"` portait `aria-pressed`, interdit par `jsx-a11y/role-supports-aria-props` → passé à `aria-selected`, sélecteur CSS aligné (mesuré : 1 seul onglet actif, fond `rgb(51,51,51)`). 6 directives `eslint-disable` inutiles supprimées — dont 2 qui masquaient de vrais avertissements `exhaustive-deps`, remis avec justification. `@next/next/no-img-element` désactivé **avec motif écrit** : en webview Tauri les images viennent du protocole `tatami://` ou de CDN, `images.unoptimized` est déjà actif, `next/image` n'apporterait rien et casserait le protocole.
- **Couvertures absentes** : les téléchargements antérieurs à la colonne `coverUrl` de `meta.json` affichaient un rectangle vide qui se lisait comme une image cassée. Remplacé par une pastille portant l'initiale du titre, à la taille exacte de la couverture (84 px).
- Preuve : `pnpm lint` **exit 0, 0 erreur 0 avertissement** · `pnpm typecheck` exit 0 · `pnpm build` `✓ Compiled successfully` · passe visuelle **6 mesures, 0 échec** (8 cartes, 3 couvertures réelles, 5 pastilles, 0 image cassée, pastille de 84 px, 1 onglet actif).
- Points ouverts : `pnpm add` renvoie le code 1 à cause de `ERR_PNPM_IGNORED_BUILDS` sur `unrs-resolver@1.12.2` (script natif non approuvé) — le linter fonctionne malgré tout ; je n'approuve pas un script de build natif sans décision humaine. Les couvertures réellement absentes ne sont pas récupérées : une commande de réparation qui reconsulte la source reste possible, non faite.

### 2026-10-04 — Retours humains sur la vague 0.13–0.17 (style + 5 défauts)
- Plan : (1) mesurer les VRAIS tokens du site dans sa feuille de style ; (2) réécrire le thème sur ces tokens ; (3) derniers chapitres et non premiers ; (4) page Téléchargements aérée ; (5) sélection en masse de chapitres ; (6) page des sources refaite ; (7) repasser la passe visuelle.
- **Retour 1 — le style n'était pas appliqué.** Je n'avais repris que la *structure* du site (grille, table, bandeau), pas son *look*. Tokens lus dans la feuille `litespeed/css/2d73d5a6….css` du site : fonds `#000` / `#16151d` / `#1a1a1a` / `#222` / `#333`, textes `#fff` / `#ddd` / `#ccc` / `#999`, bordures `#333` / `#323232` / `#383838`, **accent `#366ad3`** (46 fonds, 22 textes, 5 bordures), danger `#f13e3e`, Roboto 14 px, titre **32 px/600**, contrôles `bg #111` + `border 2px #444` + `radius 20 px` + `13 px/600`, couverture `radius 6 px`. `globals.css` réécrit sur cette base : noir, Roboto en tête (Segoe UI en repli, aucune police téléchargée), titres 32 px, pilules sur les contrôles, rayons 6/8/12, espacement grossi d'un cran. L'accent bleu reste celui de Tatami par identité.
- **Retour 2 — les cartes affichaient les PREMIERS chapitres.** `slice(0, 3)` corrigé en tri explicite décroissant (date, puis numéro) puis `slice(0, 3)`. Mesuré : Ch. 255/254/253, Ch. 7/6/5, Ch. 45/44/43…
- **Retour 3 — page Téléchargements vide et serrée.** Bandeau de chiffres (webtoons / chapitres / poids / à compléter), cartes à 3 colonnes de 340 px, écart 22 px, couverture passée de 56 à 84 px, bandeau « Vos webtoons ».
- **Retour 4 — cocher 286 chapitres un par un.** Barre de sélection rapide : champ N + `N derniers` / `N premiers` / `Tout` / `Aucun`. `Tout` et `Aucun` portent sur les chapitres **filtrés**, pas sur toute la série.
- **Retour 5 — page des sources fade.** Le mur de 1 370 entrées est remplacé par un écran de **19 tuiles de langue** (code, nom, volume) triées par volume ; la liste n'apparaît qu'une fois une langue choisie ou une recherche lancée.
- **Preuve** : `pnpm typecheck` exit 0 · `pnpm build` `✓ Compiled successfully` · `cargo clippy -D warnings` exit 0 · `cargo test` `7 passed` · **21 mesures visuelles, 0 échec**. Thème mesuré dans le navigateur : `rgb(0,0,0)`, `rgb(255,255,255)`, `Roboto`, `h1 = 32px/600/rgb(255,255,255)`, 19 tuiles, 0 carte de source sur l'accueil, 48 dans une langue, 21 lignes de chapitres récents toutes décroissantes, 5 cases cochées par « N derniers », bandeau de 4 chiffres, écart 22 px, couverture 84 px.
- **Bug trouvé par cette passe** : « N derniers » cochait bien les 5 derniers chapitres mais **hors de l'écran** (la grille affichait 100 cellules sur 286) et le défilement restait en tête de série. Corrigé : la grille se déplie jusqu'à la fin *et* un `scrollIntoView` sur la première case cochée — mesuré à `top = 703` dans une fenêtre de 900 px.
- **Deux erreurs de harnais, pas de code** : mes prédicats de test étaient passés en arguments inversés, ce qui affichait 8 « échecs » sur des mesures justes. Corrigé. Leçon : un rapport de vérification qui affiche des faux échecs est pire qu'aucun rapport.
- Points ouverts : les couvertures absentes des téléchargements anciens laissent un bloc vide (donnée réellement manquante) ; `pnpm lint` toujours cassé.

### 2026-10-03 — Tâches 0.13 à 0.17 (adaptation de l'interface)
- Plan : (1) 0.13+0.14 lecteur (TS/CSS seuls) ; (2) 0.15 grille de chapitres + date ; (3) 0.16 chapitres récents sur les cartes (TS, concurrence bornée + cache) ; (4) 0.17 bandeaux + table d'informations (CSS + commande Rust `details` + `mangaToJson` étendu, donc rebuild moteur).
- **Fait 0.13** : types `ReaderFit` (width/height) et `ReaderDir` (rtl/ltr), état `fit`/`spread`/`readerDir` persistés (`tatami.readerFit`, `tatami.readerSpread`, `tatami.readerDir`). Un cran de navigation = 2 pages en double page (`step()`), utilisé par le clavier comme par les boutons. Le cadre porte `fit-width` (page pleine largeur, défilement vertical) ou `fit-height` (page entière visible), et `spread` (grille 2 colonnes, ordre DOM inversé en RTL — la page courante est à droite, comme un manga).
- **Fait 0.14** : sélecteur de chapitre (`aria-label="Changer de chapitre"`) et champ de saut de page (`#reader-page`, min 1 max N, validation à la soumission) dans la barre du lecteur. La barre passe en `flex-wrap` pour tenir sur une fenêtre étroite.
- **Fait 0.15** : `.chapter-list` (liste verticale) remplacée par `.chapter-grid` (`auto-fill minmax(170px, 1fr)`). Chaque cellule = case à cocher + numéro + date + bouton de téléchargement révélé au survol **ou au focus** (donc atteignable au clavier). Champ de filtre : un terme numérique compare aussi les numéros, « 1180 » trouve le chapitre 1180 même si son titre n'a que des lettres.
- **Fait 0.16** : les 3 derniers chapitres + leur ancienneté (« Ch. 1 · 2 ans ») dans la carte du catalogue, et le nombre total de chapitres en pastille sur la couverture. Coût assumé : 1 requête `chapters` par carte, concurrence bornée à 4, cache `localStorage` 30 min, interrupteur activable, compteur `x/y` affiché.
- **Fait 0.17** : bandeau de section sur tous les `.page-head h1` ; fiche série avec couverture + résumé + table d'informations 2 colonnes (`.info-table`) alimentée par une nouvelle commande Rust `details`. `mangaToJson` étendu côté Kotlin (`state`, `rating`, `description`, `tags`, `altTitle` via `altTitles.firstOrNull()` — `altTitle` est déprécié côté parsers).
- **Preuve** : `pnpm typecheck` → exit 0. `pnpm build` → `✓ Compiled successfully in 10.4s`. `cargo clippy --all-targets -- -D warnings` → exit 0, aucune alerte. `cargo test` → `7 passed; 0 failed`. `gradle installDist` → `BUILD SUCCESSFUL in 14s` (1er essai en échec : `MangaState?` nullable sur `.state.name`, corrigé en `m.state?.name ?: "UNKNOWN"`), 18 JAR copiés dans `engine/dist/tatami-engine/lib`.
- **Preuve moteur** : `details` sur MANGABUFF → `state="ONGOING"`, `altTitle="Tears on withered flowers"`, `rating=0.89`, `tags` = 9 genres, `description` = 1470 caractères. `chapters` → 121 chapitres, `uploadDate` non nul sur **121/121**, lu et affiché « 22 juin 2024 ». Le champ était donc déjà transmis (Rust fait du pass-through), seul le type TS l'ignorait.
- **Passe visuelle (app réelle via CDP, WebView2 port 9223, source WAMANGA)** : **20 mesures, 0 échec**. Bandeau `rgb(19,22,30)` + rayon `7px 7px 0 0`. Cartes : 60 lignes de chapitres récents, 20 pastilles de compte (`286 ch.`, `8 ch.`…), compteur `20/20`. Fiche : 5 lignes d'infos, grille à **7 colonnes** à 1440 px, 100 cellules dont **100 avec date**, filtre « 145 » → 1 cellule. Lecteur : 1 image en simple, **2** en double page (`676px 676px`), ajusté hauteur 70×653 dans un cadre de 685 contre 1344×12525 en largeur, saut de page et bascule RTL/LTR fonctionnels.
- **Trois bugs trouvés par ces mesures, corrigés** : (1) double page + ajusté à la hauteur débordait (6 300 px dans un cadre de 685) — `grid-template-rows` valait `auto`, donc `max-height: 100%` se résolvait en `auto`, ajouté `grid-template-rows: minmax(0, 1fr)` ; (2) une carte servie par le cache restait vide (aucune requête ne restait à faire pour la remplir) — le cache est maintenant reporté dans l'état ; (3) les numéros de chapitre flottants fuitaient (« Ch. 38.09999847… », division flottante du site) — arrondis à 2 décimales, et `Math.floor` → `Math.round` dans le filtre.
- Deux autres faits mesurés : les résumés arrivent **en HTML** (REMANGA : `<p>…</p>`) → `plainText()` retire balises et entités ; les préférences du lecteur sont relues **seulement au montage** de l'app, un walk qui veut des valeurs connues doit cliquer les segments, pas écrire `localStorage`.
- Limite mesurée sur les sources : `pages` répond sur REMANGA/SEIMANGA mais `fetchImage` renvoie **HTTP 403** (CDN `img.reimg2.org` qui refuse l'user-agent du moteur) → le lecteur reste vide sur ces deux-là. Images réellement servies : WAMANGA, SEIMANGA, ANZMANGASHD. MANGABUFF demande une authentification dès `pages`.
- Preuve gates après correctifs : `pnpm typecheck` exit 0 · `pnpm build` `✓ Compiled successfully` · `cargo clippy --all-targets -- -D warnings` exit 0 · `cargo test` `7 passed; 0 failed`.
- Commit `7efdc1f` poussé sur `origin/master`. Points ouverts : `pnpm lint` toujours cassé (`next lint` n'existe plus en Next 16) ; 0.16 ajoute 1 requête `chapters` par carte affichée, mesuré à 20/20 en 14 s sur WAMANGA puis servi par le cache 30 min — à surveiller sur une source lente ; le lecteur reste non vérifié sur REMANGA/SEIMANGA à cause du 403 d'images.

### 2026-10-02 — Création
- Plan et directives créés. Nom provisoire "Tatami". Aucune tâche commencée.

### 2026-10-02 — Tâche 0.1
- Plan : (1) git init + identité locale → vérif rev-parse ; (2) LICENSE GPLv3 depuis gnu.org + README crédits/avertissement + workspace pnpm + .gitignore → vérif pnpm install ; (3) commit + cocher 0.1.
- Fait : dépôt init (root-commit `4c19a26`), 8 fichiers (`LICENSE`, `README.md`, `package.json` avec `packageManager pnpm@12.8.1`, `pnpm-workspace.yaml` sur `apps/*`, `pnpm-lock.yaml`, `.gitignore`, `AGENTS.md`, `PLAN.md`). Identité locale : kevsi roughi <kev@mail.com>.
- Preuve : `git log --oneline` → `4c19a26 chore: initialise le depot` ; `git ls-files` → 8 fichiers ; `pnpm install` → `Already up to date / Done in 754ms` ; `LICENSE` = 35149 octets, tête `GNU GENERAL PUBLIC LICENSE / Version 3, 29 June 2007`.
- Points ouverts : `packageManager` épinglé à la version constatée (12.8.1) ; structure `apps/desktop`, `engine/`, `contract/`, `scripts/`, `docs/` à créer dans les tâches suivantes (0.4–0.8).

### 2026-10-02 — Tâche 0.2
- Plan : (1) mesurer le présent (5 versions) ; (2) installer le manquant via scoop ; (3) prouver la chaîne Rust jusqu'au binaire ; (4) consigner + cocher.
- Fait : manquaient Java, Gradle, Rust. Installés via scoop (`bucket add java` + `install temurin21-jdk gradle rustup`). Scoop vit en `~\scoop` mais ses shims ne sont pas sur le PATH du shell → préfixe manuel à chaque appel. MSVC déjà présent (BuildTools 18 + MSVC 14.51.36231 + WinSDK 10), `link.exe` trouvé, chaîne prouvée par un hello-world compilé et exécuté.
- Preuve : `java -version` → Temurin 21.0.12.1 LTS ; `gradle -v` → 9.8.0 ; `rustc --version` → 1.99.0 ; `cargo --version` → 1.99.0 ; `node -v` → v26.10.0 ; `pnpm -v` → 12.8.1 ; `cargo build` hello-world → `Finished dev in 3.03s` + `Hello, world!`. Install : `scoop install` → `temurin21-jdk`, `gradle`, `rustup` = `installed successfully`, EXIT 0.
- Points ouverts : shells futurs doivent charger `vcvars64.bat` avant `cargo build` (link.exe hors PATH) ; `JAVA_HOME`, `CARGO_HOME`, `RUSTUP_HOME` persistés en vars user par scoop ; `gradle-all` (252 MB) a fait timeouter une extraction à 10 min → relance OK depuis le cache.

### 2026-10-02 — Tâche 0.3
- Plan : (1) localiser les 2 dépôts amont ; (2) lire README + `MangaLoaderContext.kt` + `MangaLoaderContextImpl.kt` (dl) + `build.gradle.kts` (dl) ; (3) écrire `docs/notes/loader-context.md` ; (4) cocher + commiter.
- Fait : note écrite (`docs/notes/loader-context.md`, 53 lignes) : coords exactes `com.github.Kotatsu-Redo:kotatsu-parsers-redo:<commit>` via JitPack (README du fork périmé, cite encore KotatsuApp), inventaire abstract/open du contexte, 5 points durs desktop avec réponses (nashorn-core:15.6, Bitmap sur BufferedImage, fenêtre 0.11 pour WebView/Cloudflare, cookies en Rust), 5 À VÉRIFIER pour 0.4.
- Preuve : lectures via API GitHub — redo HEAD `b3c0000` (2026-09-17, 0 tags, 0 releases), dl `build.gradle.kts` (parsers `14cff0d651`, okhttp 4.12.0, nashorn 15.6), `MangaLoaderContext.kt` (5217 o), `Bitmap.kt` (3 membres) ; fichier relu sur disque (53 lignes, 7 marqueurs clés).
- Points ouverts : JitPack doit encore construire le SHA redo (0.4) ; `evaluateJs` 3 args du fork vs 2 args chez dl — implémenter la 3 args.

### 2026-10-02 — Tâche 0.4
- Plan : (1) lire build amont (versions) + helpers dl ; (2) scaffold `engine/` + CLI ; (3) build, `list-sources`, `search MANGADEX` ; (4) stop daemon, journal, commit.
- Fait : `engine/` créé (settings/build/gradle.properties + 5 sources : Main, EngineContext, BitmapImpl, InMemoryCookieJar, DefaultMangaSourceConfig). Parsers `b3c0000` via JitPack, okhttp 5.1.0/okio 3.16.0/kotlin 2.2.10/coroutines 1.10.2/nashorn 15.6. Daemon stoppé.
- Preuve : `gradle run list-sources` → BUILD SUCCESSFUL, `sources: 1370`, MANGADEX listé ; `gradle run search MANGADEX naruto` → BUILD SUCCESSFUL, `results: 20` (ex. `Naruto: The Whorl within the Spiral`). 1er build JitPack : timeout Gradle → build serveur OK (pom 200, 2619 o). 2 incidents fixés : `google()` manquant (androidx.collection) + `requireBody()` déprécié sous OkHttp 5.
- Points ouverts : `evaluateJs`/Nashorn non exercé (MangaDex sans JS) ; usage d'`interceptWebViewRequests` par les sources non inventorié ; pas de wrapper Gradle (système 9.8.0) ; sorties capturées dans `AppData\Local\Temp\opencode\listsources.txt` et `searchmdx.txt`.

### 2026-10-02 — Tâche 0.5
- Plan : (1) écrire `protocol.json` + `contract/README.md` (10 méthodes, 7 codes) ; (2) ADR 0001 stdio vs HTTP ; (3) valider le JSON par exécution, journal, commit.
- Fait : `contract/protocol.json` (enveloppe + 8 defs), `contract/README.md` (table méthodes/params/result + points ouverts v1), `docs/decisions/0001-transport-sidecar.md` (statut acceptée). Incident : `node -e` inline cassé par l'expansion `$` du shell → script fichier (règle connue).
- Preuve : script node → `methods=10` (hello..shutdown), `codes=7`, `defs=method,errorCode,request,response,manga,chapter,page,sourceInfo`.
- Points ouverts : annulation par requête reportée (v0 = fermer stdin) ; `fetchImage` base64 à réévaluer si pages > ~10 Mo.

### 2026-10-02 — Tâche 0.6
- Plan : (1) lire modèles Manga/Chapter/Page + CloudFlareHelper ; (2) écrire Server.kt (10 méthodes, cache jetable, timeouts) ; (3) installDist + tests pipés ; (4) stop daemon, journal, commit.
- Fait : `Server.kt` (dispatch + cache session + `withTimeout`), `CloudFlareInterceptor.kt` + `CloudFlareProtectionException.kt` (→ CHALLENGE_REQUIRED), `putCookies` + `userAgentOverride`, commande `serve`. CloudFlareHelper confirmé présent dans le fork.
- Preuve : 4 runs pipés via `installDist`, EXIT 0 partout — hello ✓, listSources=1370 ✓, search=3 ✓, details ✓, chapters=40 ✓, pages=19 ✓, fetchImage=2393492 o image/jpeg ✓, setCookies ✓, health ×2 ✓, shutdown ✓ (exit 0). Soit 10/10 méthodes exercées. Sorties : `AppData\Local\Temp\opencode\out{1,2a,2b,2c,2d}.ndjson`.
- Points ouverts : `fetchImage` sans désembrouillage (note dans le code, MangaDex direct) ; cache session = erreur PARSE_ERROR « call search first » si process neuf (comportement voulu, vu en run2a).

### 2026-10-02 — Tâche 0.7
- Plan : (1) inspecter installDist ; (2) jdeps → modules ; (3) script build-engine.ps1 ; (4) mesurer froid + RAM sur le runtime ; (5) journal + commit.
- Fait : `scripts/build-engine.ps1` (installDist → jdeps → jlink strip/compress → launcher `-Xmx256m -XX:+UseSerialGC`). Modules : base, desktop, instrument, logging, net.http, scripting, dynalink, unsupported + `jdk.crypto.ec` ajouté à la main. Incidents : `java -version` sur stderr tuait le script sous `Stop` → `Continue` + `$LASTEXITCODE` ; 1re mesure RAM = wrapper cmd.exe → harness v2 sur le java.exe enfant ; TLS `handshake_failure` sur le runtime nu (témoin JDK complet OK) → SunEC manquant, fix vérifié.
- Preuve (runtime jlink, harnais flux redirigés) : dist = **64.1 MB** (runtime 47.2 + lib 16.8) ; démarrage à froid **1576 ms** (lancement → hello) ; idle WS **71.9** / privée 231.7 MB ; search 2047 ms, pic WS **114.5** / privée 256.2 MB (≈ plafond Xmx, respecté) ; search OK 3 résultats, shutdown exit 0. Private ≈ réserve adresses (tas), WorkingSet = RAM réelle.
- Points ouverts : `--compress=2` déprécié (warning, non bloquant) ; revue du plafond Xmx si sources lourdes en 0.11.

### 2026-10-02 — Tâche 0.8
- Plan : (1) docs Tauri v2 (Next export, sidecar, shell) ; (2) scaffold apps/desktop + superviseur ; (3) install/build/typecheck/check/clippy ; (4) preuve dev live crash→relance ; (5) journal + commit.
- Fait : Next 16.0.10 + React 19 (export `out/`, `GET / 200`), tauri 2.12.1, `supervisor.rs` (spawn java serve, hello MAJEUR=0, watchdog backoff 1s→30s, kill à la sortie), `main.rs` (ExitRequested → shutdown, pas de java orphelin), capabilities vides (0 permission : Rust spawn direct, pas de plugin shell). Icônes placeholder générées (`tauri icon`, logo réel à valider).
- Preuve : `cargo check`/`clippy -D warnings`/`fmt --check` verts ; `pnpm build` → `out/index.html` ; `pnpm typecheck` vert ; `tauri dev` live — `sidecar démarré (pid=16540)`, `hello OK (protocol=0.1)`, `taskkill 16540` → `sidecar sorti, relance` → pid 11844 → `hello OK`. Logs : `AppData\Local\Temp\opencode\tauridev2.txt`.
- Points ouverts : `externalBin` NON déclaré (binaire JVM, pas natif → ira en `resources` en 0.9+, cf. commentaire `supervisor.rs`) ; versions `@tauri-apps/*` corrigées `^2.12.1` (registre) ; `allowBuilds: sharp` persisté ; bug chemin dev `../../` → `../../../` trouvé par l'exécution ; fenêtre non capturée (preuves = logs).

### 2026-10-02 — Audit UX + refonte visuelle + correctifs (non commité)
- Plan : (1) audit UX piloté (captures + console + mesures) ; (2) corriger chaque finding ; (3) refonte « cinéma sombre » + grille compacte ; (4) refaire le runtime jlink sans casser le dist précédent.
- Fait : **encodage** — `page.tsx` ré-écrit en double UTF-8 par un `-replace` PowerShell (42 occurrences) : réparé, plus aucun U+FFFD. **Filtre genre inerte** — le `onClick` déclenchait un second `loadSource` dans un `setTimeout` qui relisait l'état *ancien* ; supprimé, la liste change vraiment (preuve ci-dessous). **Progression** — `capabilities/default.json` avait `permissions: []` → `listen()` levait `core:event:allow-listen` ; `core:event:default` ajouté. **Titres de cartes** — `overflow:hidden` clippe sur la *padding-box* : la 3ᵉ ligne se peignait dans le padding bas ; borné par `max-height: 2 lignes + padding haut`. **Barre d'onglets** extraite en `AppBar` (présente dans les 4 vues, avant : absente hors catalogue). **Chapitres** — 778 lignes rendues d'un bloc, plafonnées à 100 + « Afficher plus ». **Contenu adulte** — `safeOnly` (défaut vrai) → `MangaListFilter.contentRating = SAFE/SUGGESTIVE`, seulement si la source expose une classification ; réglage exposed dans Paramètres. **Bandeau tips rotatif** supprimé (incohérent : présent uniquement sur le catalogue). **`.gitignore` corrompu** — la ligne `*.tsbuildinfo` était devenue `*` : *tout* nouveau fichier du dépôt était ignoré (8 fichiers, dont `cbz.rs`, `download.rs`, `commands.rs`, `globals.css`). **Script build** — jlink + swap versés dans `dist\.stage`, échange en fin de script : un build raté ne détruit plus un dist valide.
- Preuve : `pnpm typecheck` EXIT 0 ; `cargo clippy --all-targets -- -D warnings` EXIT 0 ; `cargo test` 1 passed EXIT 0 ; `gradle installDist` BUILD SUCCESSFUL ; runtime reconstruit **64.1 MB**, `latest` répond sur le dist posé (n=5). Parcours CDP (walks `walk-fix.js`, `walk-filtre.js`, `walk-download.js`, `walk-refonte.js`, `%TEMP%\opencode\audit\*.png`) : **filtre genre** `liste_identique=false` puis « Effacer » restaure la liste initiale ; **titres** `hauteur_anormale=0` ; **progression** `Quiero vivir — préparation…` puis `Téléchargé : …\Chiikawa_ Nanka Chiisakute Kawaii Yatsu\Quiero vivir.cbz` ; **chapitres** `chapitres_rendus=100` + bouton « Afficher plus » ; **0 erreur console** sur tous les écrans. **A/B moteur** (`safeOnly` true/false sur MANGADEX via `tatami-engine.bat serve`) : `true` → `Fukushuu Doumei… | Yowamushi Pedal | Drive My Heart!`, `false` → `Chiikawa… | Yumeneconnect | Dragon Quest…` : le filtre change bien la source, mesuré.
- Points ouverts : `pnpm lint` est cassé (`next lint` n'existe plus dans Next 16) → à remplacer (eslint direct) ; `contract/protocol.json` n'énumère pas `latest`/`filterOptions` dans son enum de méthodes ; adult filtering réservé aux sources qui exposent `availableContentRating` (MangaDex oui, Mes sources non) ; rien n'est commité, en attente de validation visuelle à la relance.

### 2026-10-02 — Sources vides / sources bloquées (non commité)
- Plan : (1) sonder un échantillon de sources sur le moteur installé ; (2) corriger la cause des 0 résultat et des attentes ; (3) échouer vite et lisiblement.
- Fait : **`Multiple Content ratings`** — le moteur renvoyait `SAFE+SUGGESTIVE`, or les parsers n'en acceptent qu'un (`setOf(SAFE)`). Réglage passé à 3 états, défaut **SUGGESTIVE** (mesuré : SELFMANGA renvoie 0 résultat en SAFE — c'est ce qui vidait les sources). **Attentes** — deux causes distinctes : (a) le moteur traitait les requêtes **séquentiellement**, donc une source morte gelait toutes les autres ; (b) `timeoutMs` n'était pas transmis, le moteur ignorait le timeout de Rust et tenait 120 s. Corrigé : un thread par requête (`Executors.newCachedThreadPool`, écritures stdout synchronisées), `ConcurrentHashMap` pour les cookies, `@Volatile` sur le user-agent, `timeoutMs` dans l'enveloppe, timeouts par méthode (listes 30 s, détails/chapitres 60 s, `listSources` 10 s). **UI** — erreurs traduites en français court (`describeError` : 404 / hôte inconnu / 403 / timeout / anti-bot) avec « Réessayer » et « Changer de catalogue », plus d'attente muette.
- Preuve : sondage de 24 sources sur le runtime installé → `MANGADEX 12/2634 ms`, `MANGASORIGINES 12/3279 ms`, `MANGABUFF 12/1026 ms`, `SEIMANGA/ZENMANGA/WAMANGA 12` ; échecs réels : `COMX 404`, `NUDEMOON` hôte inconnu, `MINTMANGA 404`, `MANGALIB 403`, `BAOZIMH 403`, `SELFMANGA 0 résultat en SAFE`. Concurrence (6 requêtes simultanées dont 3 sources mortes) : **6/6 réponses entre 7 170 et 8 640 ms**, les sources mortes n'ont pas retenu les autres. `contentRating` SAFE / SUGGESTIVE / vide : 3 réponses distinctes, aucune erreur. Gates : `clippy -D warnings` 0 · `cargo test` 1 passed · `tsc` 0 · `gradle installDist` OK.
- Points ouverts : les sources mortes (404 / domaine disparu / 403 anti-bot) sont un fait du fork `kotatsu-parsers-redo b3c0000` — aucun correctif applicatif possible, il faudra une liste « hors service » ou un tri par échec ; `CHALLENGE_REQUIRED` (fenêtre de résolution) toujours non implémentée ; le timeout annule la requête mais le thread du parser peut rester occupé jusqu'à son propre délai OkHttp.

### 2026-10-02 — Téléchargements par webtoon + sélection multiple (8045ea2)
- Plan : (1) persister l'état d'avancement par webtoon ; (2) refaire la vue Téléchargements en webtoons ; (3) sélection multiple + file d'attente ; (4) mesurer le cas « source incomplète ».
- Fait : `library.rs` (nouveau) — `meta.json` par webtoon (titre, source, url, couverture, total, chapitres) écrit à chaque `.cbz`, commande `list_library` (vue webtoon : téléchargés, total, complet, poids) et `existing_chapters` pour ignorer ce qui est déjà là ; état reconstruit quand le meta est absent. Vue Téléchargements en cartes webtoon (couverture, `x/y chapitres`, poids, badge). Mode sélection dans la vue source + file séquentielle annulable, chapitres présents ignorés, bascule automatique vers Téléchargements. Titre de chapitre UUID remplacé par son numéro. Délai d'abandon 60 s, réglable (30/60/120).
- Preuve : `cargo test` 3 passed (2 nouveaux : meta idempotent, reconstruction sans meta), `clippy -D warnings` 0, `tsc` 0. Parcours CDP : `8 webtoons · 15 chapitres`, cartes `曼波～曼波～ part 1 · EXHENTAI · 9/99 · 43,9 Mo` et `No. 6 · MANGADEX · 2/3 · 64,2 Mo`, 0 erreur console, un vrai `.cbz` de 43 Mo produit par la file. **Pagination mesurée** : EXHENTAI offset 10 renvoie 0 résultat (le parser ignore l'offset) alors que MANGADEX, MANGASORIGINES et SEIMANGA renvoient 10 puis 10 ; EXHENTAI met 35,7 s pour une page. D'où : « Voir plus » disparaît avec une explication, et le délai est passé de 30 à 60 s.
- Points ouverts : une file de 99 chapitres tourne des heures et n'est pas reprise après redémarrage (les chapitres déjà là sont ignorés, donc un simple clic repart, mais l'état de la file est perdu) ; pas de suppression d'un webtoon téléchargé depuis l'interface ; le poids affiché est celui des `.cbz`, pas des images décompressées.

### 2026-10-03 — Taches 0.9 et 0.10 : listes + lecteur pagine
- Plan : (1) cocher 0.9 (couvert par les commandes existantes) ; (2) 0.10, afficher une page de chapitre ; (3) prouver par capture et navigation clavier.
- Fait : bouton **Lire** sur chaque chapitre ; la commande `pages` renvoie des jetons et l'image arrive par le protocole `tatami://page/<token>` (Rust : `fetchImage` -> cache disque -> webview, jamais d'appel direct aux hotes distants). Lecteur plein ecran : compteur de pages, titre webtoon + chapitre, boutons et clavier (gauche, droite, espace, Debut, Fin, Echap), prechargement de la page suivante.
- Preuve : parcours CDP sur MANGADEX — `src = http://tatami.localhost/page/2f7a4dbe...`, image naturelle **800x1148** affichee, `1/33`, deux fleches -> `3/33` avec une **seconde image differente** (800x1149), Echap referme, **0 erreur console et 0 erreur d'image**. Hauteur du lecteur 701 px pour une fenetre de 745 px (plus de debordement). `tsc` 0, `clippy` 0, `cargo test` 6 passes.
- Points ouverts : pas de zoom ni de sens de lecture (1.8), pas de reprise de position par chapitre (1.6), pas de mode webtoon defilant (1.9) ; l'image est bornee par la fenetre sans etre agrandiee, donc les petites pages restent petites ; la precharge ne couvre que la page suivante.

### 2026-10-03 — Tache 0.11 : sources protegees (fenetre de verification)
- Plan : (1) verifier que les cookies HttpOnly sont recuperables sur Windows ; (2) ouvrir une fenetre de resolution, transmettre cookies + user-agent au moteur, rejouer ; (3) mesurer.
- Fait : challenge.rs (nouveau) — open_challenge ouvre une WebView2 sur l'URL du challenge avec **le user-agent du moteur** (le moteur l'expose desormais dans `hello`), solve_challenge lit les cookies **HttpOnly** via `WebviewWindow::cookies_for_url` et les pousse via `setCookies`, cancel_challenge ferme sans appliquer. Cote interface : « Resoudre la verification », « C'est fait, reessayer », « Annuler » ; une demande de verification n'est **pas** marquée source hors service.
- Preuve : appel reel des trois commandes via l'IPC — `open_challenge` ok et fenetre listee par CDP (`http://localhost:3000/` + `https://mangadex.org/`), `solve_challenge` = **4 cookies** transmis, fermeture propre ; UA de la fenetre = `Mozilla/5.0 (X11; Linux x86_64; rv:109.0) Gecko/20100101 Firefox/116.0`, identique au `defaultUserAgent` du moteur ; 0 erreur console. Bug trouve par ce test : un second clic **refermait** la fenetre au lieu de la reutiliser (`close()` est asynchrone, le label existe encore) -> desormais on la reutilise. `clippy` 0, `cargo test` 7 passes, `tsc` 0.
- Points ouverts : **aucune source de l'echantillon ne renvoie reellement `CHALLENGE_REQUIRED`** (MANGALIB et BAOZIMH renvoient un 403 classique, pas un challenge Cloudflare) : la fin de chaine reste donc a confirmer sur un vrai site protege, cf. §6. Limites connues : la clearance est liee a l'IP et expire ; l'empreinte TLS de WebView2 differe de celle d'OkHttp, donc certains sites peuvent refuser la clearance malgre tout.
