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
> **Ce test a été JOUÉ le 2026-10-10**, pour la première fois depuis son écriture
> (2026-07). Le récapitulatif est dans `docs/rapport-smoke-test-2026-10-10.md`.
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
>    Et surtout : **T06.3 et T06.5 ne se jouent pas depuis la fiche** mais depuis
>    la vue source — sans quoi aucune file n'existe et le bandeau « File
>    interrompue » ne peut pas apparaître.
> 3. **L'état de la machine n'est pas vierge** : 3 sources personnalisées
>    installées (dont une cassée), 86 Mo de téléchargements dans un dossier
>    **réglé**, MangaDex bloqué en API, et 1 252 Mo de fichiers orphelins dans le
>    dossier **par défaut**, qu'il ne faut pas confondre avec le dossier réglé.
>    Tout est détaillé dans **§13** — s'y reporter avant de conclure qu'un
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

Preuve attendue : `sources: N` (`N` > 1 000 — **1 374** au pin `b4df40e9` : le compte
suit l'amont), puis `results: N` (`N` > 0) et cinq titres
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

**État initial** : la case est **cochée par défaut** (`useState(true)`). Il n'y a
donc rien à cocher ; il faut attendre que le compteur atteigne son but.
**Clic** : la case « Chapitres récents » (`.recent-toggle` dans la barre
`.select-bar`), à côté du bouton « Sélectionner ».
**Attendu** : chaque carte affiche ses **derniers** chapitres, décroissants, avec
ancienneté ; le compteur `x/y` monte vers `y`.
**Preuve** : capture d'une carte avec ses lignes + le compteur atteint.
**Négatif attendu** : une requête `chapters` par carte est limitée à **4 en vol**
(`RECENT_CONCURRENCY`) avec **300 ms d'écart minimal** entre deux départs
(`RECENT_INTERVALLE_MS`) et un cache de 30 min — le compteur peut rester en dessous de
`y` sans que ce soit un défaut.
**Piège mesuré** : cocher cette case sur une grille **vide** ne déclenche rien
(`latest` est vide, l'effet ne s'exécute pas). Choisir d'abord une source précise et
attendre l'arrivée des cartes avant de cocher.
**Attentes mesurées le 2026-10-10 (TOOMICSFR, 24 cartes)** : première image de
carte en ~22 s, compteur à `24/24` en ~62 s. Ce n'est pas instantané : c'est une
requête réseau par carte.

**Une carte n'affiche pas forcément 3 lignes.** « ≥ 3 lignes décroissantes » n'est
atteignable que sur une série qui en a assez : mesuré, « Arrête, Alice ! » (3
chapitres) affiche **une seule** ligne « Ch. 3 ». Choisir une série longue.

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

**Clic** : `input.chapter-search` (« Filtrer un chapitre… »), saisir un **numéro rare**.
**Attendu** : seules les cellules dont le libellé **contient** ce nombre restent.

**« Une seule cellule reste » est faux tel quel.** Le filtre est une
**sous-chaîne** sur le libellé du chapitre : filtrer « 100 » sur One Piece
laisse **15** cellules (`Ch. 100`, `Ch. 232`, `Ch. 634`, `Ch. 1000` à
`Ch. 1010`, `Ch. 1105`). Pour n'en garder qu'une, il faut un nombre qui n'est
pas le morceau d'un autre — **calculé depuis la grille affichée**, jamais `2`
(sur une série à 286 chapitres, « 2 » matche `Ch. 2`, `Ch. 20`, `Ch. 21`…).

**Négatif attendu** : `99999` affiche « Aucun chapitre ne correspond à « 99999 ». »
— c'est le comportement correct, pas une absence.

**Preuve** : capture avant (compte de cellules) / après (les cellules restantes,
avec leurs numéros).

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

**Sélecteurs utiles** : la bande est `.webtoon` (le conteneur qui défile) et
`.webtoon-inner` (la colonne positionnée) ; chaque page est un `<img data-index="N">`
ou, si la source ne la sert pas, un `div.sheet-indisponible[data-index]` **avec le même
`data-index`**. Le compteur est `.reader-compteur` ; le champ de saut est
`#reader-page` dans `form.page-jump`, et son frère affiche « / N ».

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

> **Relu ET joué le 2026-10-10. Deux chemins, à ne pas confondre.**
>
> Il y a **deux façons de télécharger**, et une seule crée une file :
>
> | Geste | Chemin | Crée une file ? |
> |---|---|---|
> | `button.chapter-dl` (le `↓`) sur la fiche | `downloadChapter()` | non |
> | `Télécharger la sélection (N)` dans `.dl-bar` (fiche) | `downloadSelected()` — boucle **directe** | **non** |
> | `Sélectionner` puis `Télécharger la sélection (N)` dans la **vue source** | `runQueue()` — écrit `QUEUE_KEY` | **oui** |
>
> Conséquence : **T06.3 et T06.5 ne sont jouables que depuis la vue source.** La
> version précédente de ce document partait de la fiche : elle ne pouvait donc
> pas faire apparaître « File interrompue », ni le bouton de pause. Ce n'était
> pas une subtilité du test, c'était un impossible.
>
> Autre conséquence mesurée : pendant un téléchargement, **tous** les `↓` sont
> `disabled` (`downloading`). Un clic sur le suivant échoue — ce n'est pas un
> bouton cassé, c'est la sérialisation.

Onglet **Téléchargements** (`data-tab="downloads"`) et retour à la fiche (T04).

### T06.1 — Un chapitre

**Clic** : `button.chapter-dl` (le `↓`) sur une cellule de chapitre, ou
`Télécharger la sélection (N)` dans `.dl-bar`.
**Attendu** : une barre `.progress` avec `chapitre — page i/N (x %)` puis `Terminé`.
**Preuve** : capture de la progression + le fichier `.cbz` relu par un **lecteur
strict** (`.mesures/verifier-cbz.py`) : un téléchargement réussi n'est pas un
téléchargement valide. Mesuré le 2026-10-10 : `Chapitre 1.cbz`, **54 entrées,
54 tailles distinctes, 54 JPEG, 3 057 971 o** = conforme.
**Le dossier est le réglage, pas le défaut** : lire « Dossier de téléchargement : »
avant de chercher quoi que ce soit. Sur cette machine il vaut
`C:\Users\alexanders\Documents\Workspaces\Tatami`, **pas** `%APPDATA%\Tatami\downloads`
qui ne contient que des fichiers orphelins. Chercher dans le défaut accuse le
code à tort (`AGENTS.md` §14).

### T06.2 — Vue Téléchargements

**Attendu** : `section.dl-stats` de 4 chiffres (webtoons / chapitres / sur le disque /
à compléter), bandeau « Vos webtoons », `article.dl-card` avec
`SOURCE · x/y chapitres · poids` et badge `Terminé` ou `Partiel`.
**Preuve** : capture de la vue entière.
**Cas limite attendu** : une carte sans couverture enregistrée affiche une **pastille
d'initiale** (`.thumb-empty`), pas un rectangle vide. **Attention** : ce cas
n'est pas toujours observable — il faut une carte *sans* `coverToken`, ce qui
n'est pas le cas de tous les téléchargements. **Ne pas le compter comme un
échec** si toutes les cartes ont une couverture : c'est l'absence de cas, pas
un défaut.
**Ce qui a été ajouté depuis** : un bouton **Lire** par carte (lecture hors ligne d'un
`.cbz`, tâche 2.3) et, si la carte a plusieurs chapitres, un bouton
`Chapitres (n)` qui ouvre une liste. Les deux sont à cliquer au moins une fois.

### T06.3 — Reprise après interruption (EXIGENCE PERMANENTE)

`PLAN.md` §3 : tout téléchargement doit être reprenable.

1. **Clic** : dans la **vue source**, `Sélectionner`, puis cocher **3** cartes,
   puis `Télécharger la sélection (3)`. *(Depuis la fiche, ce bouton
   télécharge directement et ne crée aucune file — voir l'avertissement en tête
   de section.)*
2. **Action** : fermer l'application (croix) **pendant** le 2ᵉ chapitre.
3. **Clic** : relancer, onglet Téléchargements (`data-tab="downloads"`).
4. **Attendu** : le bandeau `section.panel.resume` **« File interrompue »**
   apparaît avec les boutons **Reprendre la file** et **Abandonner la file** ;
   ou, sans file, la carte affiche `Partiel` + le bouton **Reprendre la file**.
5. **Preuve** : capture du bandeau « File interrompue » + `Get-ChildItem` du dossier de
   téléchargement : les chapitres terminés sont en `.cbz`, un éventuel chapitre
   interrompu est en `.cbz.part`.

**Mesuré le 2026-10-10** : kill à 08:35 pendant « Chapitre 3 », relancement, et
le bandeau est là avec les deux boutons ; la carte « La Femme de papa » est
« 2/3 chapitres · Partiel » avec son bouton « Reprendre la file ». Le `.cbz.part`
(1,89 Mo) est bien sur le disque.

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
**Attendu** : le chapitre en cours est **terminé**, puis la file s'arrête et
chaque ligne passe à l'état « en pause » ; le bouton du panneau devient
« Effacer la file ».
**Reprise** : elle n'est **pas** dans `section.panel.queue`. Elle est dans le
bandeau `section.panel.resume` (« File interrompue », bouton **Reprendre la
file**) — qui apparaît **dès la pause**, pas seulement après un redémarrage,
puisque `QUEUE_KEY` reste écrit. Chercher le bouton de reprise dans le panneau
de file est une impasse : il n'y est pas.
**Preuve** : capture de l'état « en pause » + `Get-Process java` ne liste plus de
processus `tatami-engine` après fermeture de l'app.
**Nettoyage obligatoire** : « Abandonner la file », puis « Effacer la file ».

**Mesuré le 2026-10-10** : pause prise à 0/3 s, le chapitre en cours va au bout
(« 1/3 chapitres · Chapitre 1 ») et les 3 lignes passent à « en pause » ;
reprise → « 1/3 chapitres (1 déjà là) » puis « Chapitre 2, page 5/168 » en direct.
Le « (1 déjà là) » est la preuve de l'exigence permanente : un chapitre déjà
présent n'est pas retéléchargé.

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

**État initial** : la case est **cochée** (elle l'est depuis une session antérieure
sur cette machine ; `localStorage`). Le-toggle ne sert qu'à vérifier l'aller-retour.
**Clic** : case `Incognito` dans le bandeau « Continuer la lecture ».
**Attendu** : les titres classés adultes disparaissent de « Continuer la lecture » ; **les
favoris restent visibles** ; un titre jamais lu et jamais mis en favori reste visible (il est
`UNKNOWN`, choix documenté, journal 2026-10-04, tâches 1.1/1.4/1.6).
**Négatif attendu, et c'est le piège principal** : si **aucun** titre de
l'historique n'est classé adulte, cocher et décocher ne change **rien** — 24 cartes
avant, 24 après. Ce n'est pas un défaut ; c'est l'absence de donnée à masquer.
Il faut d'abord avoir lu un titre d'une source adulte pour avoir quelque chose à
voir disparaître.
**Preuve** : capture avant/après + le nombre de favoris lu dans le **sous-titre de
l'en-tête** (`header.page-head .sub` : « n lus récemment · n favoris » — deux clés
distinctes par langue, jamais un `{s}`).

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
catégorie affiche `Sans catégorie (n)`. **Conditionné** : le bandeau n'apparaît
que s'il existe au moins une catégorie **et** un favori. Sur une machine sans
catégorie, il n'y a rien à tester — ce n'est pas un échec.
**Preuve** : capture après chaque action. **Rappel** : retirer un signet ou un favori ne doit
pas toucher l'historique (tests `un_signet_ne_touche_pas_l_historique`,
`desactiver_une_source_ne_touche_pas_les_favoris`).

---

## 8. Paramètres (T08)

> **Relu contre le code le 2026-10-10.** L'écran ne compte **pas quatre panneaux**
> mais **14**, tous titrés par un `<h2>` : Contenu · Catégories de favoris ·
> Apparence · Langue · Dossier de téléchargement · Réseau · Veille des
> nouveautés · Notifications système · Démarrage automatique · Journaux de
> diagnostic · Téléchargement · État des sources · Catalogues affichés ·
> Sources personnalisées. (Mesuré 2026-10-10 après stabilisation du nombre de
> panneaux ; la première mesure a rendu 8 puis 14 sur le même écran, parce que
> certains panneaux arrivent après une commande — attendre la stabilisation.)
> L'**import de dossier `.cbz`** n'est pas ici : il est dans l'onglet
> Téléchargements. Seuls les panneaux couverts ci-dessous sont testés.

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
4. **Attendu automatique** : les cookies partent **sans cliquer** — `surveiller_fenetre`
   (`challenge.rs`) relit les cookies toutes les 1,5 s et applique dès que `cf_clearance`
   apparaît ; un toast « n cookie(s) transmis à <source>. » apparaît **en bas** et la liste
   se recharge. Le bouton « C'est fait, réessayer » reste le secours si rien ne part.
5. **Clic** (secours seulement) : **C'est fait, réessayer**.
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

## 9 bis. Erreur de source — notification, pas bandeau (2026-10-11)

**Ce qui a changé, et pourquoi.** Mesuré le 2026-10-11 : entrer dans une source qui
échoue affichait un bandeau **en haut**, resté jusqu'au changement de source suivant —
et il **collait** : revenir sur une source qui marche affichait encore l'erreur de la
précédente (« la source dit erreur pourtant pas d'erreur »). Demande explicite : un seul
toast, **celui du bas**, sous forme de notification.

1. **Action** : catalogue → une source qui ne répond pas.
2. **Attendu** : un toast **en bas à droite** (`.toaster`), 13,5 px, bouton de fermeture,
   disparaissant seul après 3 s (`TOAST_DUREE_MS`, `page.tsx:523`).
3. **Attendu** : **aucun** bandeau `.alert` dans la vue catalogue (il est réservé au
   challenge, seul cas qui porte des boutons d'action).
4. **Action** : cliquer une source qui **fonctionne**.
5. **Attendu** : **aucun** message d'erreur résiduel. La preuve que l'erreur ne survit
   plus : `setError(null)` + `setChallenge(null)` en tête de `loadSource` (le `catch` les
   repose si la tentative échoue encore).
6. **Contrôle** : l'information n'est pas perdue avec le bandeau — la source reste marquée
   en échec dans le catalogue (`noteHealth`, `page.tsx:2490`), consultable via
   « état des sources ».
7. **Variante challenge** : sur une source sous Cloudflare, le **bandeau** revient (avec
   « Résoudre » / « C'est fait, réessayer » / « Annuler ») — c'est voulu, il porte une
   action, pas une notification. Au 3ᵉ échec, la borne anti-boucle affiche à son tour un
   toast « La source bloque l'application : réessaie plus tard. » **et** ferme le bandeau.

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

> **Joué le 2026-10-10, à la main pour les étapes 2 à 6.** Le dialogue natif
> n'est pas automatisable (§S.3). Ce qui a été mesuré par le DOM et le moteur :
> le panneau liste **3** lignes (`scan-manga`, `mangadexFr`, `mangadex-api`),
> chacune avec son seul bouton ✕ ; la tuile du catalogue fait **203 px** de large,
> comme une tuile de langue.
>
> **Un script installé est cassé, et l'application ne le dit nulle part** :
> `mangadexFr.js` est rejeté par le moteur —
> `Script ignoré (mangadexFr.js) : Erreur d'évaluation du script : <eval>:20:36
> Missing close quote` — il reste donc **listé dans les Réglages** et **absent
> du catalogue**. C'est exactement le cas décrit plus bas (« une source dont le
> script est invalide n'y est pas »), et il faut le dire au joueur ou leehltrer
> dans le panneau. **À corriger hors de ce test.**

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

### §S.2 bis — La grille et le moteur doivent dire la même chose

**C'est l'étape que la version précédente n'avait pas, et elle a trouvé un défaut.**
La tuile « Sources personnalisées » affiche un compteur, et ce compteur vient de la
**base**, alors que la liste de la vue vient du **moteur**. Les deux peuvent
diverger.

**Contrôle** : au même instant, lire `list_sources` **et** la grille, dans un seul
`evaluate`. Ne jamais comparer un relevé de moteur à une capture d'écran prise
autant de secondes plus tôt — on mesure alors deux choses différentes et on
conclut à une contradiction qui peut ne pas exister.

**Il faut lire QUATRE termes, pas trois.** Les deux premiers sont des commandes ;
le quatrième est une clé de `localStorage` que **aucune API ne renvoie** :

| # | Terme | Où il vit |
|---|---|---|
| 1 | Total rendu par le moteur | `invoke('list_sources')` |
| 2 | Sources `custom` | idem, `s.custom === true` |
| 3 | Désactivées côté Rust | `invoke('list_disabled_sources')` |
| 4 | **Liste blanche** | `localStorage['tatami.enabledSources']` |
| 5 | **Univers connu** | `localStorage['tatami.seenSources']` |

Le 4ᵉ est celui qui manquait le 2026-10-10 : sans lui, deux API vides
laissent croire qu'aucun filtre n'existe, alors que l'en-tête affiche
`visibleSources.length` — donc **`sources` déjà filtré** par cette liste.

Le 5ᵉ a suivi : c'est lui qui distingue une source **désactivée** d'une source
**arrivée depuis**. Un nom dans `seenSources` a déjà été vu, donc son absence
est un choix ; un nom hors de `seenSources` est arrivé après, donc il est
activé. Sans cette clé, la réparation réactive tout ce que l'utilisateur a
éteint — c'est vérifié par un test nommé, et par une mesure sur l'application.

**Cause du défaut, mesurée le 2026-10-10** : `enabledSources` est une **liste
fermée** écrite par « Tout activer ». Un nom absent = source non affichée. Donc
une source apparue **après** l'enregistrement de la liste — script importé, ou
source neuve d'une mise à jour du catalogue — disparaît en silence.

| Étape, mesurée | Moteur | Liste blanche | En-tête |
|---|---|---|---|
| Avant dépôt | 1372 | 1372 | 1372 |
| `autre-source.js` déposé et chargé | **1373** | 1372 | **1372** |
| **On ajoute le seul nom manquant** | 1373 | **1373** | **1373** |
| Fichier retiré | 1372 | 1372 | 1372 |

La troisième ligne est la preuve du lien : on ne change que le nom, et l'écart
disparaît. L'écart est **stable sur 8 relits espacés de 10 s** — ce n'est donc
pas React en retard.

**Corrigé, et vérifié en trois temps** : migration qui ne réactive pas une
source désactivée · import visible sans intervention · persistance au
rechargement. Détail et mesures dans le journal `PLAN.md` du 2026-10-10.

**Ce que le test ne doit PAS conclure** : « une source est perdue ». Elle est
servie par le moteur et elle est dans l'état React ; elle était **filtrée à
l'affichage**. La règle d'ADR 0010 tient, et le même `SOURCE.id` ne crée
toujours pas de collision.

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
État de départ à relever AVANT de jouer : langue, thème, dossier de
  téléchargement, nb de sources désactivées, nb de favoris, nb de signets,
  contenu de %APPDATA%\Tatami\sources, taille de la bibliothèque
Étapes jouées : T01 … T11 (+ T09 : partially) + §S
Résultat : <n> réussites / <n> échecs / <n> non jouables (et pourquoi)

Pour CHAQUE étape, une ligne :
  n° | ce qui a été MESURÉ | attendu | oui / non | preuve

Chaque échec : n° d'étape, clic exact, attendu, obtenu, capture,
  ET la seconde voie qui devait réussir et ce qu'elle a rendu
Chaque non-jouable : la raison (dialogue natif, cas limite absent, pas jouable en
  automatisation) — un « non-jouable » est un résultat acceptable, un « non
  testé » ne l'est pas
Nettoyage effectué : favoris / historique / signets / file / sources désactivées /
  dossiers de test / settings — avec la preuve que l'état est revenu (delta sur
  le disque ET sur l'écran)
Écarts et limites rencontrés : …
Défauts trouvés (séparés des échecs de test) : …
```

> **Un nettoyage se prouve par un delta, pas par une intention.** Mesuré le
> 2026-10-10 : après T06, bibliothèque revenue à **20 `.cbz` / 86,0 Mo**, les
> trois dossiers d'origine, **0 `.part`** — et l'écran affichait exactement le
> même bandeau qu'au départ. C'est ce qui permet d'écrire « état restauré » sans
> le croire.

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
| **Bibliothèque déjà peuplée** | Le dossier **réglé** vaut `C:\Users\alexanders\Documents\Workspaces\Tatami` : **20 `.cbz`, 86,0 Mo**, 7 webtoons, 3 dossiers (EXHENTAI, MANGADEX, TOOMICSEN). `%APPDATA%\Tatami\downloads` — le **défaut** — contient encore **1 252 Mo** de fichiers orphelins d'avant le changement de dossier : ne jamais y chercher, ni y compter. | relevé 2026-10-10 |
| **3 sources personnalisées déjà installées** | `%APPDATA%\Tatami\sources` contient `mangadex-api.js`, `mangadexFr.js` (**cassé**, voir §S.1) et `scan-manga.js`. Les comptages de §S.1 doivent être des **deltas**. | relevé 2026-10-10 |
| **Libellés écrits en dur en français** | `Genres`, `Effacer (n)`, `n genres au total`, `Choisir`, `Sur les chapitres filtrés`, `N derniers`, `N premiers`, `Filtrer un chapitre…`, `Nombre de webtoons par page`, `Terminé`, `Partiel`, `← Catalogues`, `← Chapitres`, `Réessayer`, `Changer de catalogue` ne sont **pas** dans le dictionnaire : ils restent français quand l'interface est en anglais. Ce n'est pas un échec de ce test (en français l'écran est correct), mais c'est une dette i18n réelle — `AGENTS.md` §7 l'interdit. | relecture code 2026-10-10 |
| **Un message du moteur en anglais dans une interface française** | le tableau « État des sources » affiche le message **du moteur** tel quel : « The domain of JPMANGAS no longer exists ». C'est une information de diagnostic, pas un libellé d'interface ; à tricher ou à situer. | mesuré 2026-10-10 |
| **La recherche dans la source varie d'une source à l'autre** | mesuré le même jour : `SCANVF` répond (« Piece » → 20 cartes), `TOOMICSFR` et `TOOMICSEN` rendent **0 résultat** pour tout terme. Ce n'est pas l'interface — c'est le parseur de ces deux sources. **Ne pas conclure « la recherche est cassée »** sans une seconde source qui, elle, réussit. | mesuré 2026-10-10 |
| **`SCANVF` renvoie des doublons** | 41 cartes pour **4** URL distinctes (`/one_piece` ×15, `/kingdom` ×12, `/one-punch-man` ×9, `/boruto` ×5). C'est le parseur qui répète, pas l'interface — et c'est la cause de l'avertissement React « clé ». Voir §14. | mesuré 2026-10-10 |

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
- **Défauts connus, non corrigés, à ne pas compter comme des réussites** —
  état après le smoke test du **2026-10-10** :

  | Défaut | Mesure du 2026-10-10 |
  |---|---|
  | Première image d'une source ≈ 8 s | **4,7 s** à froid sur un chapitre non mis en cache (One Piece ch. 1, 1200 × 1878) ; **0,4 s** à chaud |
  | Correction d'ancrage de 507 px au défilement | non revérifié sur ce run |
  | Placeholder qui déborde en fond de bande | **1 205 px** (contre 1 101 px annoncés) — même famille, valeur qui dérive |
  | **« Chapitre suivant » au lieu du début du chapitre** | **corrigé le 2026-10-10.** `.webtoon` est un conteneur de défilement **conservé** d'un chapitre à l'autre. `aPositionner` valant `null` pour `index === 0`, **personne ne remontait** : mesuré, lu à la page 13 puis « chapitre suivant » → `scrollTop` 10 335 px, page 13 du nouveau chapitre, le compteur annonçant « 1 / 15 ». Pire que le décalage : ce viewport était hors du quota de téléchargement, donc **1 image admise sur 10 et 0 décodée en 90 s** — et comme le repositionnement attendait `bandeVirtualisable` (une image décodée), l'effet ne se déclenchait jamais. C'était un circularity. Le correctif fait de la première page le seul cas qui n'attend rien. Non-régression vérifiée : le saut à la page 8 arrive toujours page 7 |
  | Avertissement React « clé » | **localisé pour SCANVF** : 41 cartes, 4 clés distinctes, le parseur répète ses entrées. `react/jsx-key` ne le voit pas parce que la clé est correcte — c'est sa **valeur** qui est dupliquée. Reste **une** occurrence non localisée, sur TOOMICSFR, rejouée une fois sur deux parcours propres et jamais reproduite ensuite |
  | Source servie par le moteur mais invisible au catalogue | **corrigé le 2026-10-10** — cause : la liste blanche `tatami.enabledSources` est fermée, donc un nom absent n'est pas affiché. Deuxième clé `tatami.seenSources` : un nom déjà vu reste sous le choix de l'utilisateur, un nom arrivé depuis est activé. Preuve : migration sans réactivation d'une source désactivée, puis import visible sans intervention. §S.2 bis |

- **Un cas observé une fois et non reproduit, à ne pas compter comme un défaut** :
  le 2026-10-10, un signet rouvrant la **dernière** page d'un chapitre a affiché
  la page cassée et le badge « 1 page non servie » pendant 90 s. Deux
  reproductions par le même code (saut direct à la page 56, et aller-retour
  Début→Fin), sur deux essais chacune : **aucune page cassée**, et le journal
  Rust ne contenait **aucun refus d'image**. Le journal est la seconde voie qui
  devait trancher, et il dit « rien à signaler ». À rejouer, pas à corriger.
