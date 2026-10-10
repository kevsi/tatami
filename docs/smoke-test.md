# Smoke test manuel — Tatami

Exigé par `AGENTS.md` §8 (niveau 4 de la pyramide de tests). **À exécuter sur l'application
réelle avant chaque jalon**, et avant de déclarer une tâche terminée (`AGENTS.md` §8).

Ce document ne décrit que des écrans et des commandes **qui existent dans le code au
2026-10-04**. Chaque étape cite le fichier et la ligne qui prouvent que l'écran, le libellé ou
la commande existent. Si une étape ne correspond plus au code, c'est le code qui a changé :
corriger ce document dans le même jalon (AGENTS.md §8, « la documentation concernée est à jour »).

Durée : 15 à 25 min sur une machine avec le moteur déjà construit. RAM limitée : ne pas lancer
`pnpm dev`, Gradle et `cargo build` en même temps (`AGENTS.md` §3).

> ### ⚠ État du document — 2026-10-10
>
> **Ce test n'a jamais été joué.** Écrit le 2026-07, jamais exécuté depuis.
>
> **Ce qu'il faut savoir avant de s'en servir :**
>
> 1. **Les références `page.tsx:NNNN` sont périmées.** Elles pointent sur le fichier
>    d'avant la suppression du mode page et d'avant de nombreux correctifs. Elles
>    ne désignent plus rien. **Chercher le sélecteur dans le DOM**, pas dans le
>    code — un numéro de ligne ne survit pas à une modification du fichier.
>    *Les neuf sections ont été réécrites dans ce sens le 2026-10-10 : il ne reste
>    plus **aucun** numéro de ligne dans ce document.**
> 2. **Toutes les sections ont été relues contre le code** le 2026-10-10.
>    Trois libellés avaient changé sans que le test le suive : « Arrêter après le
>    chapitre en cours » est devenu « Mettre en pause », « Reprendre » est devenu
>    « Reprendre la file », et `aria-label` du réglage de classification n'est plus
>    « Classification de contenu maximale affichée » mais « Contenu affiché ».
> 3. **L'état de la machine n'est pas vierge** : 3 sources personnalisées
>    installées, 1,25 Go de téléchargements, MangaDex bloqué en API. Tout ce qui
>    compte est détaillé dans **§13** — s'y reporter avant de conclure qu'un
>    écart est un bug.
> 4. Une étape qui échoue ne prouve **pas** que l'application est cassée : lire
>    d'abord §13, et se demander si le test est à jour avant de corriger le code
>    (`AGENTS.md` §14).
>
> Si une étape correspond encore au code, **corriger ce document dans le même
> jalon** (`AGENTS.md` §8, « la documentation concernée est à jour »).

---

## 0. Préparation

### 0.1 Variables d'environnement du shell

| Ce qui manque souvent | Correction | Preuve |
|---|---|---|
| Shims Scoop absents du PATH | préfixer `$env:USERPROFILE\scoop\shims` | journal `PLAN.md` 2026-10-02, tâche 0.2 |
| `link.exe` introuvable | charger `vcvars64.bat` avant tout `cargo` | journal `PLAN.md` 2026-10-02, tâche 0.2 |

Vérification :

```powershell
java -version; rustc --version; node -v; pnpm -v
```

### 0.2 Dépendances JS

```powershell
pnpm install
```

### 0.3 Moteur compilé (OBLIGATOIRE avant de lancer l'app)

```powershell
pnpm engine
```

Ce script lance `gradle installDist`, puis `jlink`, puis écrit
`engine/dist/tatami-engine/tatami-engine.bat` — c'est exactement le chemin que le superviseur
Rust résout au démarrage (`supervisor.rs:356-366`, `apps/desktop/src-tauri/src/supervisor.rs`).
Sans ce dossier, l'application démarre sans moteur et toutes les sources paraissent hors service.

Preuve attendue : la dernière ligne du script est `OK: <chemin> (<taille> MB)`
(`scripts/build-engine.ps1:69`).

> Si l'application est ouverte pendant le build, l'échange du dossier `dist` échoue : le script
> le dit explicitement (`scripts/build-engine.ps1:64`). Fermer l'app avant `pnpm engine`.

### 0.4 Contrôle du moteur sans l'application (optionnel, ~30 s)

```powershell
engine\dist\tatami-engine\tatami-engine.bat list-sources
engine\dist\tatami-engine\tatami-engine.bat search MANGADEX naruto
```

Preuve attendue : `sources: 1370` puis `results: N` (`N` > 0) et cinq titres
`Titre | url`. Commandes définies en `engine/src/main/kotlin/org/tatami/engine/Main.kt:29-45`.
Si cette étape échoue, ne perdez pas de temps sur l'interface : le problème est dans le moteur.

### 0.5 Lancer l'application

```powershell
pnpm tauri dev
```

`tauri dev` démarre le serveur Next sur le port 3000 puis la fenêtre (tauri.conf.json:7,9).
Fenêtre attendue : 1280 × 800, titre « Tatami » (`tauri.conf.json:15-17`).

---

## 1. Démarrage et moteur (T01)

> **Relu contre le code le 2026-10-10.**

**Quoi faire** : lancer l'app, ne rien cliquer, attendre 10 s.

| Vérifier | Attendu | Preuve |
|---|---|---|
| Le bandeau d'application | `nav.tabs` avec 4 `button.tab[data-tab]` : `home`, `catalog`, `downloads`, `settings` — libellés Accueil, Catalogues, Téléchargements, Paramètres | `TABS` |
| L'onglet actif | un seul `button.tab[aria-selected="true"]` | attribut, pas la couleur |
| La vue par défaut | `<h1>Accueil</h1>`, bandeau « Continuer la lecture », bandeau « Vos favoris », et — si des mises à jour ont été vues — `section.maj` **avant** les deux autres | DOM |
| Les logs du sidecar | `sidecar java=…` puis `hello OK` dans `%APPDATA%\Tatami\logs\tatami.log` | superviseur |
| Une source morte déjà visitée | badge « hors service » sur sa tuile (`.badge.dead-badge`), et une ligne dans le tableau « État des sources » | DOM |

**Ce qui s'affiche en plus aujourd'hui** : le bandeau « mises à jour » (`section.maj`)
et, dans l'en-tête, le sous-titre « n lus récemment · n favoris ». Un écran d'accueil
sans ces éléments n'est pas forcément cassé — vérifier qu'il n'y a **rien** à afficher.

**Preuve** : capture d'écran de la fenêtre au démarrage + les lignes de log ci-dessus.

**Échec typique** : fenêtre ouverte mais Catalogues vide → le dossier
`engine/dist/tatami-engine` est absent ou le runtime `jlink` n'a pas été reconstruit (§ 0.3).

---

## 2. Catalogues (T02)

> **Relu contre le code le 2026-10-10.** Les références `page.tsx:NNNN` ont été
> remplacées par des **sélecteurs DOM** : un numéro de ligne ne survit pas à une
> modification du fichier (`AGENTS.md` §14).

Onglet **Catalogues** (l'onglet porte `data-tab="catalog"`).

L'écran s'ouvre sur la **grille des langues** (`.lang-grid`), pas sur la liste des
sources : `.lang-tile[data-lang="<code>"]` par langue, plus une tuile
`.lang-tile[data-card-perso]` pour les sources personnalisées. Un champ
`input.search[aria-label="Rechercher un catalogue…"]` filtre les langues.

### T02.1 — Navigation par langue

**Clic** : sur une tuile `.lang-tile[data-lang="fr"]`, puis sur une source de la liste.
**Attendu** : `<h1>` devient le **nom lisible** de la source (`nomSource(selected)`,
et non `s.name` — voir la note S2.2) et le sous-titre `header .sub` porte sa langue
**en majuscules**. Le bouton `button.crumb` (`← Catalogues`) ramène à la grille.
**Preuve** : 2 captures (grille des langues, liste des sources de la langue choisie).

**Sélecteurs utiles** : la source est `div.slot > button.card`, avec
`aria-label="Ouvrir la source <NOM>"` et `title` = source lisible
(`s.displayName ?? s.name`).

### T02.2 — Recherche de catalogue

**Clic** : champ « Rechercher un catalogue… », saisir `MANGA`.
**Attendu** : le compteur `p.count` passe à « n sources pour « MANGA » · langue FR ».
La partie « · langue XX » n'apparaît **que** si une pastille de langue est active ;
sur la vue « Toutes » le compteur s'arrête à « n sources pour « MANGA » ».
**Preuve** : capture du compteur. **Négatif attendu** : un terme qui ne matche rien
affiche l'état vide « Aucune source à afficher » **avec** sa phrase
« Aucune source ne correspond à « … ». », pas une grille vide.

**Piège mesuré** : ce filtre **réduit** la liste des sources, il ne charge pas le
catalogue. Il faut ensuite cliquer une source pour déclencher une requête
(leçon `AGENTS.md` §14, « l'effet ne s'exécute pas »).

### T02.3 — Désactivation d'une source

**Clic** : sur `div.slot > button.slot-switch[role="switch"]` — **pas** sur `button.card`,
qui ouvre la source. L'interrupteur est un frère de la tuile (deux `<button>`
imbriqués seraient du HTML invalide) ; son `aria-label` vaut
`Désactiver la source <NOM>`, puis `Réactiver la source <NOM>` une fois désactivée.
**Attendu** : la tuile disparaît de la liste ; une barre `.row-actions` apparaît avec
la case « Afficher les 1 source désactivée » et le bouton « Tout réactiver » ; le
sous-titre de l'en-tête devient « n sources disponibles · 1 désactivée ».
**Preuve** : capture avant / après, puis rechargement complet de la fenêtre (F5) et
nouvelle capture : la source doit rester désactivée. Elle est **en base**, pas en
`localStorage` — la liste vient de la commande `list_disabled_sources` et l'écriture
passe par `set_source_enabled`. Un F5 la relit donc, et un vrai redémarrage aussi.
**Nettoyage obligatoire** : cliquer « Tout réactiver » avant la suite, sinon les étapes
suivantes peuvent tomber sur un catalogue en moins.

---

## 3. Recherche dans une source (T03)

Vue source, après T02.1. **Utiliser une source ouverte** : voir §13 pour le choix, et
noter que **MangaDex est bloqué en API depuis cette machine** (2026-10-09).

### T03.1 — Liste et recherche

**Clic** : `input.search` dans `.source-search` (placeholder « Rechercher un webtoon… »)
puis le bouton **Rechercher**, ou touche Entrée dans le champ.
**Attendu** : `p.count` affiche « n webtoons » (+ « — chargement… » pendant le vol).
**Preuve** : capture avec **au moins 3 cartes visibles** — une mesure sur une liste vide ne
prouve rien (`AGENTS.md` §14).

**Sélecteurs utiles** : chaque carte est `div.manga[data-manga-key]`, la zone cliquable
est `button.manga-open`. La clé vient du DOM : ne jamais apparier une file d'attente à
une carte par **index** — la grille est triée et la file filtrée, les deux ordres ne
correspondent pas (`AGENTS.md` §14).

**Piège** : au premier rendu, `.manga-grid[aria-busy="true"]` affiche un squelette de
**6** cartes. Le squelette porte les **mêmes classes** que la vraie carte : attendre la
disparition de `aria-busy`, sinon on mesure la géométrie du squelette (mesuré : 190 ×
320 contre 188 × 328 pour la vraie carte).

### T03.2 — Tri des titres

**Clic** : les pastilles de `.chips[aria-label="Tri des titres"]` — `Ordre du site`,
puis `A → Z`, puis `Z → A`. L'ordre par défaut est `Ordre du site`.
**Attendu** : `A → Z` trie réellement la liste chargée ; `Z → A` est l'inverse exact
d'`A → Z` ; le tri survit au changement de source.
**Preuve** : la **liste exacte** des 5 premiers titres recopiée dans le rapport, dans
chaque ordre. **Garde-fou obligatoire** : relever d'abord le nombre de cartes lu dans
le DOM et constater qu'il est ≥ 3 — deux tableaux vides sont égaux, donc une inversion
« réussie » sur une liste vide ne prouve rien (`AGENTS.md` §14).
**Attention** : le tri s'applique à **tout ce qui est chargé**, pas à la seule page
courante (journal 2026-10-04, tâche 1.3).

### T03.3 — Genres

**Clic** : une pastille dans `.chips[aria-label="Genres"]`, puis « Effacer (n) ».
**Attendu** : la liste change ; « Effacer » restaure la liste d'avant.
**Preuve** : capture des deux listes **et le nombre de cartes avant/après**.
**Piège mesuré** : choisir un genre qui ne change rien ne prouve rien.
Les libellés `Genres`, `Effacer (n)` et `n genres au total` sont **écrits en dur en
français** : ils ne suivent pas la langue de l'interface. Ce n'est pas un échec du test,
c'est une chaîne non traduite (cf. §13).

### T03.4 — Chapitres récents sur les cartes

**Clic** : la case « Chapitres récents » (`.recent-toggle` dans la barre `.select-bar`),
à côté du bouton « Sélectionner ».
**Attendu** : chaque carte affiche ses **derniers** chapitres, décroissants, avec
ancienneté ; le compteur `x/y` monte vers `y`.
**Preuve** : capture d'une carte avec ≥ 3 lignes décroissantes + le compteur atteint.
**Négatif attendu** : une requête `chapters` par carte est limitée à **4 en vol**
(`RECENT_CONCURRENCY`) avec **300 ms d'écart minimal** entre deux départs
(`RECENT_INTERVALLE_MS`) et un cache de 30 min — le compteur peut rester en dessous de
`y` sans que ce soit un défaut.
**Piège mesuré** : cocher cette case sur une grille **vide** ne déclenche rien
(`latest` est vide, l'effet ne s'exécute pas). Choisir d'abord une source précise et
attendre l'arrivée des cartes avant de cocher.

---

## 4. Fiche série (T04)

> **Relu contre le code le 2026-10-10.**

**Clic** : `button.manga-open` sur une carte de la liste.

**Sélecteurs utiles** : le fil d'Ariane est `button.crumb` ; le bouton favori est
`button.fav-toggle[aria-pressed]` ; la barre de chapitres est `.dl-bar` ; chaque
chapitre est `div.chapter-cell` contenant trois enfants — `label.chapter-check`,
`button.chapter-open` et `button.chapter-dl`.

### T04.1 — Informations

**Attendu** : couverture `img.sheet-cover`, résumé **sans balises HTML**
(`plainText`, journal 2026-10-04), `dl.info-table` (`.info-row` = `dt`/`dd`)
avec ≥ 1 ligne, bouton `♡ Ajouter aux favoris` dans l'en-tête.
**Preuve** : capture de la fiche entière. **Piège mesuré** : un résumé qui affiche
`<p>…</p>` est un défaut de strippage, pas un bug d'affichage.

### T04.2 — Grille de chapitres et dates

**Attendu** : `.chapter-grid` en `repeat(auto-fill, minmax(190px, 1fr))` — donc
**≥ 5 colonnes** dans la fenêtre par défaut de 1280 px (elle en donne ~6) ;
**100 cellules** visibles (`CHAPTER_PAGE = 100`) puis « Afficher plus (n restants) »
(`CHAPTER_STEP = 200`) ; **une date par cellule** quand la source renvoie
`uploadDate` (journal 2026-10-03 : non nul sur 121/121 chapitres MANGABUFF).
**Preuve** : capture de la grille + le compteur `p.count` (« n chapitres · k
sélectionnés », quatre variantes de pluriel selon les deux compteurs).

**Sélecteur impératif** : compter des **`button.chapter-open`**, jamais
`.chapter-open` — le squelette rend un `<span>` de la même classe et l'on compte
alors 100 lignes vides (mesuré le 2026-10-09).

### T04.3 — Filtre par numéro

**Clic** : `input.chapter-search` (« Filtrer un chapitre… »), saisir un **numéro rare**
(calculé depuis la grille affichée, pas `2` : sur une série à 286 chapitres, filtrer
« 2 » matche `Ch. 2`, `Ch. 20`, `Ch. 21`… — `AGENTS.md` §14).
**Attendu** : une seule cellule reste. Si aucune : « Aucun chapitre ne correspond à
« … » », ce qui est le comportement correct.
**Preuve** : capture avant (compte de cellules) / après (la cellule restante).

### T04.4 — Sélection rapide

**Clic** : le champ nombre de `.pick-bar`, puis `N derniers`, puis `Aucun`.
**Attendu** : les N cases cochées sont **visibles à l'écran** (la grille se déplie et
défile jusqu'à elles, journal 2026-10-04 « retours humains »), le compteur de sélection
monte à N, le bouton devient `Télécharger la sélection (N)`.
**Preuve** : capture avec les cases cochées **et** le compteur. Un test qui coche hors
écran et ne regarde que le compteur est un faux positif.
Les libellés `Choisir`, `N derniers`, `N premiers`, `Sur les chapitres filtrés`,
`Filtrer un chapitre…` sont **écrits en dur en français** (cf. §13).

### T04.5 — Favori

**Clic** : `♡ Ajouter aux favoris`.
**Attendu** : le libellé bascule en `♥ Dans vos favoris` et `aria-pressed` passe à vrai.
**Preuve** : capture + la valeur de `aria-pressed` relevée dans le rapport.
**Note** : le sélecteur de **Catégorie** n'apparaît qu'une fois le titre favori — il
conditionne l'apparition, donc on ne peut pas le tester avant T04.5.

---

## 5. Lecteur (T05)

**Réécrit le 2026-10-10.** La version précédente testait le mode « page »
(boutons `Pagé`/`Webtoon`, `Simple`/`Double`, `Largeur`/`Hauteur`, `RTL`/`LTR`),
**supprimé le 2026-10-06** : le lecteur est désormais **uniquement webtoon**.
Ces contrôles ont été retirés : ils échouaient sur des boutons absents, ce qui
n'apprenait rien. Les numéros de ligne `page.tsx:NNNN` ont tous été retirés
pareil — ils dataient de l'ancien fichier et ne désignaient plus rien. **Chercher
un sélecteur dans le DOM plutôt que dans le code** (`AGENTS.md` §14).

**Clic** : bouton `Lire <chapitre>` sur une cellule (`button.chapter-open`).
Barre du lecteur : titre `webtoon · chapitre`, bouton `Signet`, navigation
`←`/`→`, plein écran, zoom, sélecteur de chapitre, saut de page.

### T05.1 — L'image s'affiche vraiment

**Attendu** : des images dans `.webtoon-inner`, de taille naturelle non nulle.
**Preuve** : `src` = `http://tatami.localhost/page/<token>` et
`naturalWidth > 0`. **Règle** (`AGENTS.md` §14) : une `<img>` cassée est déjà
« visible » pour l'automatisation — attendre `img.complete && img.naturalWidth > 0`.
**Sélecteur** : `button.chapter-open` et **pas** `.chapter-open` : le squelette
rend un `<span>` bearing the same class, donc `.chapter-open` compte 100 lignes
vides avant le chargement (mesuré le 2026-10-09).

### T05.2 — Navigation par page

**Clic** : boutons `←`/`→` de la barre.
**Attendu** : le compteur `Page / N` avance ; l'image affichée change.
**Preuve** : capture du compteur + `data-index` de la première image visible.
**Règle** : mesurer l'**élément visible** (`data-index`), jamais le compteur seul —
un compteur qui bouge ne prouve pas que l'écran a bougé (`AGENTS.md` §14).

### T05.3 — ~~Modes et ajustements~~ (retiré)

`Simple`, `Double`, `Hauteur`, `RTL` : supprimés avec le mode page.
`object-fit: contain` reste en CSS (garde-fou anti-étirement), mais ce n'est plus
un réglage utilisateur.

### T05.4 — La bande webtoon

**Attendu** : une bande continue d'images ; le double-clic bascule le zoom.
**Preuve** : capture d'une bande de ≥ 3 images + mesure de la hauteur du
conteneur, et **l'espace vide sous la dernière image** (`.webtoon-inner` doit
descendre jusqu'au bas de la bande, pas le dépasser).
**Ce qui a été corrigé, à surveiller** :
- images étirées (le repli ne doit jamais être posé en `height`) ;
- trait horizontal à chaque frontière de page (`hauteurRendue` ne doit **pas**
  arrondir : `contain` laisserait 0,25 px de fond) ;
- vide de 1 120 px en bas (pages de queue non encore mesurées) ;
- **64 px de fenêtre vide sous le lecteur** : `.reader` doit faire la hauteur
  du viewport, et `.app` ne doit pas respirer sous une surface plein écran.

### T05.5 — Saut de page et changement de chapitre

**Clic** : champ de saut de page → saisir `3` → Entrée ; puis le sélecteur de chapitre.
**Attendu** : la page 3 s'affiche ; le changement de chapitre recharge les images du nouveau
chapitre et le compteur `N` change.
**Preuve** : captures avant/après avec les compteurs.

### T05.6 — Clavier

| Touche | Attendu | Mesuré le 2026-10-09 |
|---|---|---|
| `↑` / `↓` | **défilement progressif** de 80 % du cadre, **pas** un saut de page | 502 px sur un cadre de 627 |
| `←` / `→` | page précédente / suivante | — |
| `Espace`, `Page bas` | page suivante | — |
| `Début` / `Fin` | 1re / dernière page | — |
| `Échap` | sort du plein écran d'abord, ferme le lecteur sinon | — |

**Preuve** : relever `scrollTop` de `.webtoon` et le `data-index` visible — le
compteur seul ne prouve rien.

### T05.7 — Signet

**Clic** : `Signet`, changer de page, recliquer.
**Attendu** : le libellé devient `Signet ✓` et `aria-pressed` passe à vrai ; la page marquée
est celle affichée au moment du clic.
**Preuve** : capture du libellé + section « Vos signets » de l'accueil (T07.3).

### T05.8 — Chapitre courant et reprise (ajouté le 2026-10-10)

**Clic** : lire un chapitre, revenir à la fiche avec `← Chapitres`.
**Attendu** : la cellule du chapitre lu porte `data-courant` et `aria-current="true"`,
affiche « Vous êtes ici · page X/Y », et un bouton « Reprendre *chapitre* » apparaît.
**Preuve** : compte de `[data-courant]` = 1, `aria-current` = 1, texte du bouton.
**Rappel** : la reprise est relue **au retour du lecteur** et pas seulement à
l'ouverture du webtoon — un `catch` qui la met à `null` sur erreur est
indistinguishable d'une absence de reprise, donc mesurer l'interface ne suffit pas
(`AGENTS.md` §14, leçon `invoke<Type>`).

---

## 6. Téléchargement (T06)

Onglet **Téléchargements** et retour à la fiche (T04).

### T06.1 — Un chapitre

**Clic** : `button.chapter-dl` (le `↓`) sur une cellule de chapitre, ou
`Télécharger la sélection (N)` dans `.dl-bar`.
**Attendu** : une barre `.progress` avec `chapitre — page i/N (x %)` puis `Terminé`.
**Preuve** : capture de la progression + du fichier `.cbz` dans le dossier de
téléchargement **réglé dans les paramètres** — lire d'abord
« Dossier de téléchargement : » (section Téléchargement) : le défaut est
`%APPDATA%\Tatami\downloads`, mais l'utilisateur peut l'avoir déplacé, et chercher
dans le défaut accuse le code à tort (`AGENTS.md` §14).

### T06.2 — Vue Téléchargements

**Attendu** : `section.dl-stats` de 4 chiffres (webtoons / chapitres / sur le disque /
à compléter), bandeau « Vos webtoons », `article.dl-card` avec
`SOURCE · x/y chapitres · poids` et badge `Terminé` ou `Partiel`.
**Preuve** : capture de la vue entière.
**Cas limite attendu** : une carte sans couverture enregistrée affiche une **pastille
d'initiale** (`.thumb-empty`), pas un rectangle vide (journal 2026-10-04
« lint/couvertures »).
**Ce qui a été ajouté depuis** : un bouton **Lire** par carte (lecture hors ligne d'un
`.cbz`, tâche 2.3) et, si la carte a plusieurs chapitres, un bouton
`Chapitres (n)` qui ouvre une liste. Les deux sont à cliquer au moins une fois.

### T06.3 — Reprise après interruption (EXIGENCE PERMANENTE)

`PLAN.md` §3 : tout téléchargement doit être reprenable.

1. **Clic** : sur la fiche, mettre `N derniers` avec N = 3, puis **Télécharger la sélection (3)**.
2. **Action** : fermer l'application (croix) **pendant** le 2ᵉ chapitre.
3. **Clic** : relancer, onglet Téléchargements (`data-tab="downloads"`).
4. **Attendu** : le bandeau `section.panel.resume` **« File interrompue »** apparaît
   avec les boutons **Reprendre la file** et **Abandonner la file** ; ou, sans file,
   la carte affiche `Partiel` + le bouton **Reprendre la file**.
5. **Preuve** : capture du bandeau « File interrompue » + `Get-ChildItem` du dossier de
   téléchargement : les chapitres terminés sont en `.cbz`, un éventuel chapitre
   interrompu est en `.cbz.part`.

**Règle** : un `.cbz.part` compte comme **absent** → il doit être retéléchargé, pas
compté comme acquis. Un `.cbz` déjà présent ne doit **jamais** être retéléchargé. Si un
chapitre déjà téléchargé est re-téléchargé, c'est un défaut.

**Nettoyage obligatoire** : après vérification, « Abandonner la file », sinon les
sessions suivantes héritent d'une file en attente (le bandeau réapparaît à chaque
démarrage).

### T06.4 — Suppression

**Clic** : `Supprimer` sur une carte (`button.btn-ghost` dans `.dl-actions`).
**Attendu** : un dialogue **`Supprimer <titre> ?`** (`dialog.confirm`) avec le nombre de
chapitres et le poids, puis `Supprimer définitivement` ou `Annuler`.
**Preuve** : capture du dialogue ; après `Supprimer définitivement`, `Get-ChildItem` montre
que le dossier du webtoon a disparu et **que le dossier `downloads` racine existe
encore** (`library.rs` : la suppression reste confinée au dossier de téléchargement).

### T06.5 — Mise en pause d'une file

**Clic** : pendant une file, le bouton **Mettre en pause** de `section.panel.queue`
(auparavant « Arrêter après le chapitre en cours » : **le libellé a changé**, la
sémantique non).
**Attendu** : le chapitre en cours est **terminé**, puis la file s'arrête et passe à
l'état « en pause » ; « Reprendre la file » la relance où elle s'est arrêtée.
**Preuve** : capture de l'état « en pause » + `Get-Process java` ne liste plus de
processus `tatami-engine` après fermeture de l'app (`main.rs` : `ExitRequested` tue le
sidecar).
**Nettoyage obligatoire** : « Effacer la file » une fois la vérification faite.

---

## 7. Accueil, historique, favoris, signets (T07)

Onglet **Accueil** (`data-tab="home"`).

> **Relu contre le code le 2026-10-10.** L'accueil affiche aujourd'hui **plus** que ce
> qu'énumère cette section : une section « mises à jour » (`section.maj`, tâche 3.2)
> vient **avant** « Continuer la lecture », avec son compteur de non-vus, un
> `Tout vu` et un `<details>` par série. La vérifier en passant (elle dépend de la
> veille, donc son contenu est variable — ne pas en faire un critère).

### T07.1 — Reprise de lecture

**Attendu** : bandeau « Continuer la lecture » avec une carte `article.dl-card` par
titre lu (`SOURCE · chapitre` puis `page i/N · ancienneté`).
**Clic** : **Reprendre** (`button.btn-primary` de `.dl-actions`).
**Attendu** : le lecteur s'ouvre **au même chapitre et à la même page** ; le bouton
affiche `Ouverture…` pendant le chargement.
**Preuve** : capture du lecteur avec le compteur **et** le `data-index` de l'image
visible (le compteur seul ne prouve pas que l'écran a bougé, `AGENTS.md` §14).
**Piège mesuré** : ouvert depuis l'accueil, le lecteur ne s'ouvre que si le nom de
source est passé explicitement (`openReaderAt`). Si « Reprendre » ne fait rien,
c'est ce chemin. **La carte elle-même est aussi cliquable** (`button.dl-open`,
surimpression) et ouvre la **fiche**, pas le lecteur.

### T07.2 — Incognito

**Clic** : case `Incognito` dans le bandeau « Continuer la lecture ».
**Attendu** : les titres classés adultes disparaissent de « Continuer la lecture » ; **les
favoris restent visibles** ; un titre jamais lu et jamais mis en favori reste visible (il est
`UNKNOWN`, choix documenté, journal 2026-10-04, tâches 1.1/1.4/1.6).
**Preuve** : capture avant/après + le nombre de favoris lu dans le **sous-titre de
l'en-tête** (`header.page-head .sub` : « n lus récemment · n favoris » — deux clés
distinctes par langue, jamais un `{s}`).
**Piège** : l'état d'incognito est relu au **montage**, pas à chaque rendu : écrire
`localStorage` puis attendre ne change rien. Il faut cocher la case, ou changer la
langue et recharger dans la **même** action.

### T07.3 — Signets

**Attendu** : bandeau « Vos signets » listant titre, source, chapitre, `page i/N`,
ancienneté, avec `Rouvrir` et `Retirer` (`.bm-list > .bm`).
**Clic** : `Rouvrir` sur le signet marqué en T05.7.
**Attendu** : la fiche se recharge, puis le lecteur s'ouvre **à la page exacte marquée**.
**Preuve** : capture du lecteur avec `i/N` == page du signet **et** `data-index` visible.
**Nettoyage obligatoire** : `Retirer` tous les signets, sinon les sessions suivantes
héritent d'un état non vierge.

### T07.4 — Favoris, catégories et historique

**Attendu** : « Vos favoris » en grille `.manga-grid` ; `button.fav-remove` (le `♥`)
pour retirer un favori ; `Oublier` sur une carte de lecture ; `Vider l'historique`
dans le bandeau « Vos favoris » (**le bouton n'apparaît que s'il y a ≥ 1 favori**).
**Bonus livré depuis** : un filtre par catégorie dans ce bandeau
(`Toutes` / `Sans catégorie (n)` / une pastille par catégorie) — un favori sans
catégorie affiche `Sans catégorie (n)`. Le filtrer aussi, c'est la preuve que la
catégorie survit au retrait du filtre.
**Preuve** : capture après chaque action. **Rappel** : retirer un signet ou un favori ne doit
pas toucher l'historique (tests `un_signet_ne_touche_pas_l_historique`,
`desactiver_une_source_ne_touche_pas_les_favoris`).

---

## 8. Paramètres (T08)

> **Relu contre le code le 2026-10-10.** L'écran ne compte **pas quatre panneaux**
> mais **seize** : Contenu, Catégories, Apparence, Langue, Dossier de téléchargement,
> Réseau, **Planificateur** (tâche 3.1), **Notifications** (3.3), **Démarrage
> automatique**, **Journal**, Téléchargement, **État des sources** (5.4),
> Catalogues affichés, **Sources personnalisées** (S2.1), **Import** (`.cbz`).
> Seuls ceux couverts ci-dessous sont testés.

Onglet **Paramètres** (`data-tab="settings"`).

### T08.1 — Classification de contenu

**Clic** : `select[aria-label="Contenu affiché"]` → `Toutes`.
**Attendu** : les sources adultes renvoient alors des résultats ; le réglage survit au
redémarrage (`localStorage`, clé `SAFE_KEY`).
**Preuve** : capture du sélecteur + le nombre de résultats d'une source adulte, avant/après.
**Régression à surveiller** : le réglage ne doit jamais être interprété comme `SAFE` quand il
vaut « Toutes » (`AGENTS.md` §14 ; test `une_chaine_vide_ne_signifie_pas_safe`).
Si une source adulte renvoie 0 résultat en « Toutes », c'est ça.

### T08.2 — Délai avant abandon

**Clic** : `select[aria-label="Délai avant abandon"]` → `30 secondes`
(les valeurs sont `30`, `60`, `120` — `TIMEOUTS`).
**Attendu** : une source lente est signalée hors service au lieu de faire attendre.
**Preuve** : capture + le message d'erreur court (« n'a pas répondu à temps. Site mort
ou trop lent. »).

### T08.3 — Taille de page

**Clic** : `select[aria-label="Nombre de webtoons par page"]` → `48`
(valeurs `12`, `24`, `48`, `100`).
**Attendu** : la page source charge 48 cartes au lieu de 24 ; le réglage persiste.
**Preuve** : compteur `48 webtoons` en capture.

### T08.4 — Catalogues affichés

**Clic** : **un seul bouton** qui alterne entre `Tout désactiver` et `Tout activer`
(pas deux boutons), ou une pastille de catalogue dans `.chip-wrap`.
**Attendu** : le compteur `x / y catalogues` se met à jour ; au-delà de 200 catalogues,
le message « 200 premiers catalogues sur n. Utilise la recherche pour les autres »
apparaît.
**Preuve** : capture du compteur dans les deux états.
**Attention** : ce réglage et celui de T02.3 sont **deux réglages différents** — ici on
choisit une liste blanche de catalogues affichés (localStorage), là on désactive une
source précise (base). Les ne pas confondre dans le rapport.

### T08.5 — Bouton « Tout réessayer »

**Condition** : au moins une source hors service a été visitée (le panneau
« État des sources » n'affiche **rien** sinon — pas de bandeau, pas de bouton ; un
panneau vide et un panneau vert sont indiscernables à l'œil, cf. `AGENTS.md` §14).
**Clic** : `Tout réessayer` en bas du panneau « État des sources ».
**Attendu** : le tableau et le compteur disparaissent ; la prochaine visite de la
source retentera la requête.
**Preuve** : capture du tableau avant/après.
**Bonus à vérifier** : le tableau `table.etat-table` liste source / code d'erreur /
date du dernier signalement, et un résumé **regroupé par code** (`ul.code-list`) —
1 200 sources en `CHALLENGE_REQUIRED` sont un problème de Cloudflare, pas 1 200 pannes.

---

## 9. Source protégée (T09) — CONDITIONNEL

> **Relu le 2026-10-10. L'affirmation ci-dessous est désormais fausse.**

~~`PLAN.md` §6 : aucune source de l'échantillon ne renvoie réellement `CHALLENGE_REQUIRED`~~
(MANGALIB et BAOZIMH renvoient un 403 classique) → **Ce test est désormais
déclenchable.** Mesuré le 2026-10-09 sur **TOONFR** (`https://toonfr.com/webtoon/`) : le
moteur répond `{"ok":false,"error":{"code":"CHALLENGE_REQUIRED", …}}`, et le superviseur
note désormais le **code** d'erreur, pas seulement la longueur de la réponse — donc le
cycle est enfin visible au journal.

**Résultat mesuré ce jour-là, à reproduire** : sur TOONFR, **2 challenges visibles au
journal, 0 cookie transmis**. La fenêtre s'ouvre donc, et la reprise ne transmet rien.
**Ne pas conclure ici** : trois causes restent possibles — l'utilisateur ne valide
pas, `solve_challenge` échoue faute de cookie pour le site, ou la fenêtre ne conserve
pas ses cookies. C'est le genre de conclusion arrêtée trop tôt que `AGENTS.md` §14
signale ; trancher par le journal avant d'écrire quoi que ce soit.

**Si** un jour une source renvoie `CHALLENGE_REQUIRED` :

1. **Clic** : **Résoudre la vérification** (bouton de la barre d'alerte `.alert`).
2. **Attendu** : une fenêtre « Vérification — <source> » s'ouvre sur l'URL du challenge.
3. **Action humaine** : résoudre le challenge dans cette fenêtre.
4. **Clic** : **C’est fait, réessayer**.
5. **Attendu** : les cookies (y compris `HttpOnly`) sont transmis au moteur via
   `setCookies`, la fenêtre se ferme, la requête est rejouée et la liste s'affiche.
6. **Preuve** : capture de la fenêtre ; dans le rapport, le **nombre de cookies
   transmis** renvoyé par `solve_challenge` ; puis le **journal** — chercher `challenge`
   et `setCookies`, et relever les **deux** nombres.
7. **Piège mesuré** : un `Annuler` qui renvoie `Ok` ne prouve pas que la fenêtre
   n'existait pas — `cancel_challenge` fait `if let Some(w) { w.close() }`. Compter
   les fenêtres réelles, ne pas les interroger.
8. **Piège mesuré** : une commande Tauri lue par `window.__TAURI_INTERNALS__.invoke`
   depuis une sonde renvoie `not allowed` alors que l'application fonctionne à la même
   seconde — ce n'est pas le chemin d'appel réel. **N'en déduire aucun défaut.**

**Limites à consigner** : la clearance est liée à l'IP et expire ; l'empreinte TLS de
WebView2 diffère de celle d'OkHttp, donc certains sites peuvent refuser la clearance
même après résolution.

---

## 10. Persistance après vrai redémarrage (T10)

**Action** : fermer la fenêtre, puis relancer `pnpm tauri dev`.
**Attendu** :

| Élément | Attendu |
|---|---|
| Favori | présent, **avec sa classification** (`content_rating`) et sa catégorie |
| Historique | présent, avec `page i/N` exact |
| Incognito | état conservé (`localStorage`) |
| Sources désactivées | toujours désactivées (T02.3) — elles sont **en base**, pas en `localStorage` |
| Signets | présents, au même ordre (plus récent d'abord) |
| Réglages (classification, délai, taille de page, apparence, langue) | conservés — tous en `localStorage` |
| Sources personnalisées | toujours listées, et **toujours présentes dans la grille** du catalogue |

**Ce qui distingue les deux mondes** : ce que le **cœur** applique (favoris,
historique, signets, sources désactivées, téléchargements, sources perso) est **en
base** ; ce que seul l'écran lit (apparence, langue, incognito, taille de page,
listes blanches de catalogues) est en `localStorage`. Un réglage en `localStorage`
ne peut pas être appliqué par le cœur — il diverge au premier redémarrage
(`AGENTS.md` §14).

**Preuve** : capture de l'accueil après redémarrage + `Get-ChildItem %APPDATA%\Tatami` montrant
`tatami.db`, `cache\`, `downloads\`.
**Attention** : si une migration a eu lieu, un fichier `tatami-v<N>-<horodatage>.db` doit
apparaître à côté de `tatami.db` (`store.rs:284-295`) — c'est la sauvegarde avant migration,
**pas** une erreur.

---

## 11. Console et réseau (T11)

**Attendu sur tous les écrans** : **0 erreur console**.

| Symptôme console | Cause probable |
|---|---|
| `<img>` cassée en boucle | source qui refuse `fetchImage` (voir §13) ou token périmé |
| `Failed to load resource` sur `tatami.localhost` | page mise en cache disque vide (`pagecache.rs:94-99`) |
| `localStorage` en boucle | preferência relue au montage uniquement (`AGENTS.md` §14) |

**Preuve** : console complète copiée dans le rapport de jalon (vide = conforme).

---

## §S. Sources personnalisées (S2)

Écrit le 2026-10-07, **après** le commit `c83e56f`, puis révisé après le correctif
d'identité (le nom de fichier fait foi, pas `SOURCE.id` — voir ADR 0010).
**Non exécuté à la rédaction** : c'est la preuve qui manque à S2.2 et au critère de
sortie de la phase S.

Le chemin critique est le **dossier** : Rust copie le script dans
`%APPDATA%\Tatami\sources\` (`custom_source.rs:26`) et le moteur scanne
`File(appData, "Tatami/sources")` (`Server.kt:319-321`). Si les deux divergent, l'import
répond « ok » et la source n'apparaît jamais — c'est le défaut à chercher en premier.

Le second chemin critique est l'**identité** : la base enregistre la ligne sous le nom
du fichier (`exemple-source`), le moteur doit donc indexer son registre sur le même
nom. Le script livré déclare `id: "exemple-fr"` — s'il servait de clé, la suppression
de `exemple-source.js` ne déchargerait rien et la source resterait fantôme dans la
grille. C'est vérifié à l'étape 14.

### §S.1 Import

> **Relu contre le code et contre la machine le 2026-10-10.**
>
> **État de départ mesuré** : `%APPDATA%\Tatami\sources\` contient **déjà 3
> scripts** (`mangadex-api.js`, `mangadexFr.js`, `scan-manga.js`) et la table
> `custom_sources` autant de lignes. Les étapes 1, 5 et 6 ci-dessous ont donc été
> réécrites pour cet état — « liste vide » et « 1 ligne » ne sont plus les bonnes
> attentes, et les **compter** à l'aveugle donnerait un échec qui n'existe pas.
> **Relever le nombre de lignes avant et après chaque étape**, et ne conclure sur
> un delta.

| # | Geste | Attendu | Preuve dans le code |
|---|---|---|---|
| 0 | **Relevé** : `Get-ChildItem "$env:APPDATA\Tatami\sources"` | noter les `.js` présents et le nombre de lignes `custom_sources` | — |
| 1 | Réglages → panneau « Sources personnalisées » | panneau présent, **la liste affiche les sources déjà installées**, ou « Aucune source personnalisée. » si le dossier est vide | `section.panel` ; clé `sources.perso.vide` |
| 2 | Bouton « Importer un script… » | dialogue natif ouvert, filtre `*.js` | `open({ filters: [{ extensions: ['js'] }] })` ; capability `dialog:allow-open` |
| 3 | **Annuler** le dialogue | aucun message, aucun fichier créé, bouton non bloqué | le `return` sur annulation remet `importPersoEnCours` à faux |
| 4 | Importer `docs/mangadex-api.js` (déjà installé sur cette machine → c'est un **ré-import**, pas une création) | toast « Source « … » importée. », **une seule** ligne pour cet id, horodatage mis à jour. **La langue n'est pas affichée** : la colonne est écrite en dur à `fr` par la commande d'import (`custom_source.rs`). | `invoke('importer_source', { path })` ; `ON CONFLICT(id) DO UPDATE` |
| 5 | `%APPDATA%\Tatami\sources\` | le `.js` est présent ; `custom_sources` a **le même nombre de lignes qu'à l'étape 0** (pas de doublon) | `store.rs` |
| 6 | Copier `docs/mangadex-api.js` en `autre-source.js`, l'importer | **une ligne de plus**, nommée `autre-source`. Le nom de fichier fait foi — le même `SOURCE.id` ne crée pas de collision | `identiteDe` (`ScriptedSourceLoader.kt`) |
| 7 | **Nettoyage** : supprimer `autre-source` (§S.3, étapes 12-14) | la ligne et le `.js` disparaissent, le compte du dossier revient à celui de l'étape 0 | `supprimer_source_custom` |

**Si l'étape 6 est omise**, le nombre de lignes ne prouve rien : le dossier en contient
déjà 3. Un delta de +1/-1 est la seule preuve lisible.

### §S.2 Le moteur la voit-il

`docs/mangadex-api.js` est le script de démonstration : il appelle l'API JSON de MangaDex
(sans clé, sans compte) et sert de **vraies** pages. Le gabarit `docs/exemple-source.js` sert
à écrire le sien, mais il pointe vers `https://exemple-site.fr`, qui n'existe pas : il se charge
et s'affiche, mais toute recherche renvoie une erreur réseau. Ce n'est pas un défaut du moteur.

**Historique de mesure** (2026-07, contre le moteur réel, NDJSON `serve`) : `listSources` →
`custom:mangadex-api` ; `search "hinamatsuri"` → 2 résultats avec couvertures ; `details` →
137 chapitres ; `chapters` → 137 ; `pages` → 36 URLs ; la 1ʳᵉ page téléchargée en
`HTTP 200 image/jpeg 347 159 o`. Puis dans l'application (CDP) : `list_sources: 1371 sources`
(1370 natives + 1 perso) ; `latest` → 20 items ; la fiche affiche « MangaDex (API) » et non
`custom:mangadex-api`.

> **⚠ Ces chiffres ne sont plus rejouables depuis cette machine.** Le 2026-10-09, MangaDex
> **bloque l'API en TLS** depuis ce poste (`api.mangadex.org` → code `000`,
> handshake `SEC_E_ILLEGAL_MESSAGE`) alors que le nœud d'images répond 200. Les étapes
> **9 et 10 sont donc attendues en échec réseau**. Ce n'est pas un défaut d'import : c'est
> le producteur d'URL qui ne répond plus (`AGENTS.md` §14, « avant d'accuser la donnée,
> vérifier que son producteur répond »). **Constater l'échec, ne pas le corriger.**

Deux choses à savoir avant de jouer cette section :
- **Le moteur doit avoir été reconstruit** (`pnpm engine`) ET aucun moteur installé ne doit
  passer devant : `main.rs` donne la priorité à `%APPDATA%\Tatami\moteur\actif.json`.
  *Vérifié le 2026-10-10* : ce fichier est **absent** (il ne reste qu'un `actif.json.bak`),
  donc c'est bien le moteur de `engine/dist` qui sera utilisé. Le refaire appearing est
  le premier défaut à chercher si une source importée n'apparaît jamais.
- **Une couverture MangaDex peut peser 20 Mo** : l'API ne sert pas de variante `.256.jpg`.

| # | Geste | Attendu |
|---|---|---|
| 8 | Catalogues → tuile « Sources personnalisées » (`[data-card-perso]`) | la vue liste les sources importées, **badge « Perso »**, nom lisible (« MangaDex (API) », pas `custom:mangadex-api`) |
| 8b | Même tuile, avec une langue sélectionnée (ex. `.lang-tile[data-lang="ja"]`) | la source y figure aussi, avec son badge de langue — **la pastille de langue n'apparaît que dans la vue « Toutes »** (`lang === 'all'`) |
| 9 | Ouvrir la source, rechercher `hinamatsuri` | **attendu en échec sur cette machine** (API bloquée). Sur une machine non bloquée : 2 résultats, couvertures visibles |
| 10 | Ouvrir la fiche | idem : 137 chapitres et 36 pages ne sont **pas** rejouables ici |

Un défaut ici n'est pas un défaut d'import : c'est le `ScriptedSourceLoader` (dossier,
WatchService), le script lui-même, ou le réseau — **disposer lequel des trois avant de
conclure**, en relisant §13.

Sur cet écran, une barre affiche « n source importée{s} » et un bouton « Toutes les
langues » pour revenir à la grille ; le bouton « Importer un script… » est **dans la
vue**, pas dans la tuile (deux boutons dans la tuile l'imbriqueraient : HTML invalide).
Après l'import, la nouvelle tuile peut mettre **~0,6 s** à apparaître (le moteur
exécute le script après l'avoir vu au WatchService ; l'interface rafraîchit à 0, 1,5 s
et 4 s). Le compte vient de la grille : une source dont le script est invalide n'y est
pas, alors qu'elle reste listée dans Réglages.

### §S.3 Désactivation et suppression

| # | Geste | Attendu |
|---|---|---|
| 11 | Interrupteur de la tuile (`button.slot-switch[role="switch"]`) | source grisée, comme une source native (même code, clé `custom:mangadex-api`) |
| 12 | Réglages → ✕ sur la ligne | la ligne bascule sur « Supprimer {nom} » + « Annuler » — **pas** de dialogue natif |
| 13 | « Annuler » | la ligne revient à son état initial, rien n'est effacé |
| 14 | « Supprimer {nom} » | toast « Source « … » supprimée. », ligne disparue, `.js` absent du dossier |
| 15 | Catalogues → tuile « Sources personnalisées » | **la source a disparu de la vue** — c'est l'étape qui manquait avant le correctif d'identité du 2026-10-07 (le moteur cherchait `exemple-fr.js` et ne déchargeait rien) |

**À ne pas faire** : supprimer `mangadex-api`, `mangadexFr` ou `scan-manga`. Ce sont
des sources préparées à la main et **retapées à la main** depuis (mangadexFr est une
variante française générée par substitution de chaînes). Le test n'en crée qu'une
(`autre-source`) et ne supprime que celle-là.

Le dialogue natif de l'étape 2 n'est **pas** testable par CDP (leçon `AGENTS.md` §14,
« un dialogue natif n'est pas shimable ») : l'étape 4 se vérifie à l'œil, ou par la
présence du fichier sur le disque.

---

## 12. Récapitulatif à coller dans le rapport de jalon

```text
Smoke test — Tatami — <date> — <agent / personne>
Environnement : Windows <version>, <RAM>, JDK <version>, Rust <version>, Node <version>, pnpm <version>
Source de référence utilisée : <NOM_SOURCE>
Étapes jouées : T01 … T11 (+ T09 : non déclenchable) + §S
Résultat : <n> réussites / <n> échecs
Chaque échec : n° d'étape, clic exact, attendu, obtenu, capture
Nettoyage effectué : favoris / historique / signets / file / sources désactivées / réglages
Écarts known limits rencontrés : …
```

---

## 13. Limites attendues (ne pas les reporter comme des bugs)

**Toutes les lignes ci-dessous sont des mesures du 2026-07 à 2026-10-08 et
doivent être recontrôlées avant d'en conclure qu'une étape échoue.** Deux d'entre
elles ne sont plus vraies et sont signalées comme telles.

| Limite | Détail | Origine |
|---|---|---|
| **Images refusées sur REMANGA / SEIMANGA** | `pages` répond mais `fetchImage` renvoie 403 : le CDN refuse l'user-agent du moteur. Le lecteur reste vide. **Recontrôler** : SEIMANGA a été mesuré servant normalement le 2026-10-04. | journal 2026-10-03, tâche 0.13-0.17 |
| **MANGABUFF demande une authentification** dès `pages` | message d'erreur traduit, bouton « Changer de catalogue » | journal 2026-10-02 |
| **Sources mortes** | COMX, MINTMANGA en 404, NUDEMOON en domaine inexistant, MANGALIB/BAOZIMH en 403. **Recontrôler** : `frscans.com` ne résout plus non plus (2026-10-09). | journal 2026-10-02, sondage de 24 sources |
| **SELFMANGA renvoie 0 résultat en `SAFE`** | d'où le défaut `SUGGESTIVE` | journal 2026-10-02 |
| **EXHENTAI ne pagine pas** | offset ignoré, 35,7 s pour une page ; « Voir plus » disparaît avec une explication | journal 2026-10-02 |
| **Première image d'une source ≈ 8 s** | le moteur sérialise à ~260 ms par image et la **première** coûte ~8 s (chargement du parser, session HTTP). Mesuré dans `tatami.log` (`serve_page` horodaté). Les suivantes : ~260 ms. | journal 2026-10-10 |
| **Titres sans date de publication** | certaines sources ne renvoient pas `uploadDate` : la cellule affiche alors le libellé du chapitre | mesuré 2026-10-04 |
| **Chapitres sous licence, sans images** | une source peut répondre `200 OK` avec `data: []` : `pages` renvoie 0 URL et le lecteur s'ouvre vide. MangaDex a été mesuréLost 7/8 pages sur ch.1 et 0/8 sur ch.3 — le même code, donc c'est la source. | sonde NDJSON 2026-07, mesures 2026-10-09 |
| ~~**Pas de lecture hors ligne**~~ | **ne vaut plus** : un `.cbz` téléchargé se lit hors ligne (tâche 2.3 livrée, `cbz.rs` / `zipread.rs`). | — |
| ~~**Pas de catégories de favoris**~~ | **ne vaut plus** : catégories livrées (tâche 1.5, table `categories`). | — |
| **MangaDex : API coupée en TLS depuis cette machine** | `api.mangadex.org` et `mangadex.org` répondent `000` (handshake `SEC_E_ILLEGAL_MESSAGE`) alors que le nœud d'images répond 200 et que tous les autres sites répondent. Ce n'est ni un blocage d'IP ni un certificat. **Conséquence** : `search`/`details`/`chapters` sur MangaDex et sur `custom:mangadex-api` échouent, **et c'est attendu**. Vérifié avant de conclure. | `PLAN.md` « Blocage MangaDex », 2026-10-09 |
| **Bibliothèque déjà peuplée (1,25 Go, 29 `.cbz`)** | `%APPDATA%\Tatami\downloads` contient HENTAIORIGINES, MANGADEX, MANGASORIGINES et IMPORT. T06.2 doit donc vérifier l'affichage de cartes **existantes**, pas d'une bibliothèque vide ; T06.4 (suppression) doit viser un artefact jetable, jamais ce dossier. | relevé 2026-10-10 |
| **3 sources personnalisées déjà installées** | `%APPDATA%\Tatami\sources` contient `mangadex-api.js`, `mangadexFr.js`, `scan-manga.js`. Les comptages de §S.1 doivent être des **deltas**. | relevé 2026-10-10 |
| **Libellés écrits en dur en français** | `Genres`, `Effacer (n)`, `n genres au total`, `Choisir`, `Sur les chapitres filtrés`, `N derniers`, `N premiers`, `Filtrer un chapitre…`, `Nombre de webtoons par page`, `Terminé`, `Partiel`, `← Catalogues`, `← Chapitres`, `Réessayer`, `Changer de catalogue` ne sont **pas** dans le dictionnaire : ils restent français quand l'interface est en anglais. Ce n'est pas un échec de ce test (en français l'écran est correct), mais c'est une dette i18n réelle — `AGENTS.md` §7 l'interdit. | relecture code 2026-10-10 |

**Rappel de méthode.** Un 404 sans corps n'est pas une page d'erreur mais un
refus de connexion : quand une URL échoue, demander d'abord **qui me l'a donnée,
et ce producteur répond-il ?** Une URL est une valeur *reçue* (`AGENTS.md` §14).

---

## 14. Ce que ce test ne couvre pas

**Révisé le 2026-10-10.** Les phases marquées « non commencées » ci-dessous ont
été livrées depuis : ne pas s'en servir pour conclure qu'elles manquent.

- **Linux / macOS** : hors v1 (`PLAN.md` §3).
- **Traduction de manga** : **retirée du produit** (ADR `0013`). Les phases T1→T4
  qui figuraient ici comme « à venir » ne le sont plus et ne le seront pas.
- **Suivi AniList / MAL / Shikimori / Kitsu, synchronisation** : **livré**
  (état de synchronisation en base, erreurs de jetons gérées).
- **Notifications système, planificateur** : **livré**. Le passe périodique est en
  place ; `latest` respecte un intervalle minimal entre requêtes (mesuré 248 ms).
- **Mise à jour du moteur signée** : **non commencée** (phase 5.1). En
  développement, le sidecar est résolu depuis `engine/dist` ; l'installation dans
  `resources` n'est pas faite. **C'est un manque réel de la v1.**
- **Tests de contrat** : **livrés** (fixtures enregistrées rejouées contre le code
  Rust). Les tests unitaires Rust, le lint et le typecheck ne sont pas remplacés
  par ce document : ils sont listés dans `AGENTS.md` §8.
- **Défauts connus, non corrigés, à ne pas compter comme des réussites** :
  première image d'une source ≈ 8 s ; une correction d'ancrage de 507 px au
  défilement ; un placeholder qui déborde de 1 101 px en fond de bande ; un
  avertissement React « clé manquante » **intermittent**, non localisé
  (`react/jsx-key` ne le voit pas : il vient d'une liste construite hors JSX).
