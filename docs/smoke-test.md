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
> 2. **Les sections 5 (Lecteur), 13 (limites) et 14 (périmètre) ont été mises à
>    jour** aujourd'hui : elles testaient des écrans supprimés et annonçaient des
>    phases livrées comme « non commencées ».
> 3. **Les sections 2, 3, 4, 6, 7, 8 et 11 (S2) n'ont pas été relues.** Leurs
>    contrôles sont formulés sur des intention, pas sur des mesures : à vérifier
>    avant d'en tirer une conclusion, et à corriger au passage.
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

**Quoi faire** : lancer l'app, ne rien cliquer, attendre 10 s.

| Vérifier | Attendu | Preuve |
|---|---|---|
| Le bandeau d'application | 4 onglets : Accueil, Catalogues, Téléchargements, Paramètres | `apps/desktop/app/page.tsx:152-157` |
| L'onglet actif | un seul onglet.selected, fond `rgb(51,51,51)` | `page.tsx:181` + journal 2026-10-04 « lint/couvertures » |
| La vue par défaut | titre `Accueil`, section « Continuer la lecture », section « Vos favoris » | `page.tsx:2735,2746,2881` |
| Les logs du sidecar | `sidecar java=… (dev)` puis `hello OK` | `supervisor.rs:6,212-224` |
| Un source morte déjà visitée | pastille « hors service » sur sa tuile, ou message en Paramètres | `page.tsx:3086,1765-1779` |

**Preuve** : capture d'écran de la fenêtre au démarrage + les 3 lignes de log ci-dessus.

**Échec typique** : fenêtre ouverte mais Catalogues vide → le dossier
`engine/dist/tatami-engine` est absent ou le runtime `jlink` n'a pas été reconstruit (§ 0.3).

---

## 2. Catalogues (T02)

Onglet **Catalogues**.

### T02.1 — Navigation par langue

**Clic** : sur une tuile de langue (par exemple `FR`), puis sur une source de la liste.
**Attendu** : le titre devient le nom de la source et le sous-titre sa langue en majuscules
(`page.tsx:2447-2448`). Le fil d'Ariane `← Catalogues` ramène à la liste (`page.tsx:2444`).
**Preuve** : 2 captures (liste des langues, liste des sources de la langue choisie).

### T02.2 — Recherche de catalogue

**Clic** : champ « Rechercher un catalogue… » (`page.tsx:2961-2965`), saisir `MANGA`.
**Attendu** : le compteur de la page passe à « n sources pour « MANGA » · langue … »
(`page.tsx:3037-3041`).
**Preuve** : capture du compteur. **Négatif attendu** : un terme qui ne matche rien affiche
l'état vide « Aucune source à afficher » (`page.tsx:3045`), pas une grille vide.

### T02.3 — Désactivation d'une source

**Clic** : l'interrupteur à droite d'une tuile de source (`page.tsx:3089-3102`).
**Attendu** : la tuile disparaît de la liste ; la barre affiche « Afficher les 1 source
désactivée » et « Tout réactiver » (`page.tsx:3002-3006`) ; le compteur d'en-tête indique
« · 1 désactivée » (`page.tsx:2957`).
**Preuve** : capture avant / après, puis rechargement complet de la fenêtre (F5) et
nouvelle capture : la source doit rester désactivée (table `sources_state`, `store.rs:205-208`).
**Nettoyage obligatoire** : cliquer « Tout réactiver » avant la suite, sinon les étapes
suivantes peuvent tomber sur un catalogue en moins.

---

## 3. Recherche dans une source (T03)

Vue source, après T02.1. **Utiliser une source ouverte** (MANGADEX ou WAMANGA) : ce sont les
seules où images et chapitres ont été vérifiés (journal 2026-10-03, tâche 0.13-0.17).

### T03.1 — Liste et recherche

**Clic** : champ « Rechercher un webtoon… » + bouton **Rechercher**, ou touche Entrée
(`page.tsx:2453-2467`).
**Attendu** : `n webtoons` dans le compteur (`page.tsx:2613-2616`).
**Preuve** : capture avec **au moins 3 cartes visibles** — une mesure sur une liste vide ne
prouve rien (`AGENTS.md` §14).

### T03.2 — Tri des titres

**Clic** : `A → Z`, puis `Z → A`, puis `Ordre du site` (`page.tsx:2617-2627`).
**Attendu** : « Ordre du site » est actif par défaut ; `A → Z` trie réellement la liste
chargée ; `Z → A` est l'inverse exact d'`A → Z` ; le tri survit au changement de source.
**Preuve** : capture des 5 premiers titres dans chaque ordre + la liste exacte recopiée dans
le rapport de jalon. **Attention** : le tri s'applique à **tout ce qui est chargé**, pas à la
seule page courante (journal 2026-10-04, tâche 1.3).

### T03.3 — Genres

**Clic** : une pastille de genre, puis « Effacer (n) » (`page.tsx:2473-2485`).
**Attendu** : la liste change ; « Effacer » restaure la liste d'avant.
**Preuve** : capture des deux listes. **Piège mesuré** : choisir un genre qui ne change rien
ne prouve rien — noter le nombre de cartes avant/après.

### T03.4 — Chapitres récents sur les cartes

**Clic** : case « Chapitres récents » (`page.tsx:2523-2535`).
**Attendu** : chaque carte affiche ses **derniers** chapitres, décroissants, avec ancienneté
(`page.tsx:2671-2679`) ; le compteur `x/y` monte vers `y` ; un sélecteur
`286 ch.` apparaît sur la couverture (`page.tsx:2664-2668`).
**Preuve** : capture d'une carte avec ≥ 3 lignes décroissantes + le compteur atteint.
**Négatif attendu** : sur une source lente, 1 requête `chapters` par carte affichée est
limité à 4 en parallèle et mis en cache 30 min (journal 2026-10-03) — le compteur peut
 rester en dessous de `y` sans que ce soit un défaut.

---

## 4. Fiche série (T04)

**Clic** : une carte de la liste (`page.tsx:2651-2655`).

### T04.1 — Informations

**Attendu** : couverture, résumé **sans balises HTML** (`plainText`, journal 2026-10-04),
table d'informations `.info-table` à 2 colonnes avec ≥ 1 ligne
(`page.tsx:2293-2302`), bouton `♡ Ajouter aux favoris` (`page.tsx:2282`).
**Preuve** : capture de la fiche entière. **Piège mesuré** : un résumé qui affiche
`<p>…</p>` est un défaut de strippage, pas un bug d'affichage.

### T04.2 — Grille de chapitres et dates

**Attendu** : `.chapter-grid` en ≥ 5 colonnes à 1440 px (`page.tsx:2380`), 100 cellules
visibles puis « Afficher plus (n restants) » (`page.tsx:2426-2432`), **une date par cellule**
(lournal 2026-10-03 : `uploadDate` non nul sur 121/121 chapitres MANGABUFF).
**Preuve** : capture de la grille + compteur « n chapitres · n sélectionnés » (`page.tsx:2312`).

### T04.3 — Filtre par numéro

**Clic** : champ « Filtrer un chapitre… » (`page.tsx:2350-2357`), saisir un **numéro rare**
(calculé depuis la grille affichée, pas `2` : `AGENTS.md` §14).
**Attendu** : une seule cellule reste.
**Preuve** : capture avant (compte de cellules) / après (la cellule restante).

### T04.4 — Sélection rapide

**Clic** : champ nombre + `N derniers`, puis `Aucun` (`page.tsx:2329-2348`).
**Attendu** : les N cases cochées sont **visibles à l'écran** (la grille se déplie et défile
jusqu'à elles, journal 2026-10-04 « retours humains »), le compteur de sélection monte à N,
le bouton devient `Télécharger la sélection (N)` (`page.tsx:2323`).
**Preuve** : capture avec les cases cochées **et** le compteur. Un test qui coche hors écran
et ne regarde que le compteur est un faux positif.

### T04.5 — Favori

**Clic** : `♡ Ajouter aux favoris`.
**Attendu** : le libellé bascule en `♥ Dans vos favoris` et `aria-pressed` passe à vrai
(`page.tsx:2278-2283`).
**Preuve** : capture + la valeur de `aria-pressed` relevée dans le rapport.

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

**Clic** : bouton `↓` sur une cellule de chapitre (`page.tsx:2403-2411`) ou
`Télécharger la sélection (N)` (`page.tsx:2323`).
**Attendu** : une barre de progression avec `chapitre — page i/N (x %)` (`page.tsx:2360-2371`)
puis `Terminé` ; l'app bascule sur l'onglet Téléchargements en fin de file (`page.tsx:1296`).
**Preuve** : capture de la progression + du fichier `.cbz` dans
`%APPDATA%\Tatami\downloads\<source>\<webtoon>\` (`download.rs:68-81`).

### T06.2 — Vue Téléchargements

**Attendu** : bandeau de 4 chiffres (webtoons / chapitres / sur le disque / à compléter)
(`page.tsx:1905-1922`), bandeau « Vos webtoons », cartes avec `SOURCE · x/y chapitres · poids`
et badge `Terminé` ou `Partiel` (`page.tsx:1945-1959`).
**Preuve** : capture de la vue entière.
**Cas limite attendu** : une carte sans couverture enregistrée affiche une **pastille
d'initiale** de 84 px, pas un rectangle vide (journal 2026-10-04 « lint/couvertures »).

### T06.3 — Reprise après interruption (EXIGENCE PERMANENTE)

`PLAN.md` §3 : tout téléchargement doit être reprenable.

1. **Clic** : sur la fiche, mettre `N derniers` avec N = 3, puis **Télécharger la sélection (3)**.
2. **Action** : fermer l'application (croix) **pendant** le 2ᵉ chapitre.
3. **Clic** : relancer, onglet Téléchargements.
4. **Attendu** : le bandeau **« File interrompue »** apparaît avec le bouton **Reprendre**
   (`page.tsx:1841-1856`) ; ou, sans file, la carte affiche `Partiel` + bouton **Reprendre**
   (`page.tsx:1961-1968`).
5. **Preuve** : capture du bandeau « File interrompue » + le dossier **`Get-ChildItem`** :
   les chapitres 1 et 2 sont en `.cbz`, un éventuel chapitre interrompu est en `.cbz.part`
   (`download.rs:247`).

**Règle** : un `.cbz.part` compte comme **absent** (`download.rs:116,122`) → il doit être
retéléchargé, pas compté comme acquis. Un `.cbz` déjà présent ne doit **jamais** être
retéléchargé. Si un chapitre déjà téléchargé est re-téléchargé, c'est un défaut.

### T06.4 — Suppression

**Clic** : `Supprimer` sur une carte (`page.tsx:1970-1972`).
**Attendu** : dialogue « Supprimer <titre> ? » avec le nombre de chapitres et le poids, puis
`Supprimer définitivement` ou `Annuler` (`page.tsx:1989-2007`).
**Preuve** : capture du dialogue ; après `Supprimer définitivement`, `Get-ChildItem` montre que
le dossier du webtoon a disparu et **que le dossier `downloads` racine existe encore**
(`library.rs:202-205` : la suppression reste confinée à `%APPDATA%\Tatami\downloads`).

### T06.5 — Annulation de file

**Clic** : pendant une file, `Arrêter après le chapitre en cours` (`page.tsx:1863-1865`).
**Attendu** : la file s'arrête **proprement**, le chapitre en cours est terminé ou laissé en
`.part`, aucun processus `java.exe` orphelin ne reste (T01.3 ci-dessous).
**Preuve** : `Get-Process java` ne liste plus de processus `tatami-engine` après fermeture de
l'app (`main.rs:131-139` : `ExitRequested` tue le sidecar).

---

## 7. Accueil, historique, favoris, signets (T07)

Onglet **Accueil**.

### T07.1 — Reprise de lecture

**Attendu** : section « Continuer la lecture » avec une carte par titre lu, indication
`page i/N` et ancienneté (`page.tsx:2762-2817`).
**Clic** : **Reprendre**.
**Attendu** : le lecteur s'ouvre **au même chapitre et à la même page** que la reprise depuis
la source ; le bouton affiche `Ouverture…` pendant le chargement (`page.tsx:2796`).
**Preuve** : capture du lecteur avec le compteur `i/N` **egal à** celui de la carte.
**Piège mesuré** : ouvert depuis l'accueil, le lecteur ne s'ouvre que si le nom de source est
passé explicitement (`openReaderAt`, journal 2026-10-04, tâche 1.7). Si « Reprendre » ne fait
rien, c'est ce chemin.

### T07.2 — Incognito

**Clic** : case `Incognito` dans le bandeau « Continuer la lecture » (`page.tsx:2747-2759`).
**Attendu** : les titres classés adultes disparaissent de « Continuer la lecture » ; **les
favoris restent visibles** ; un titre jamais lu et jamais mis en favori reste visible (il est
`UNKNOWN`, choix documenté, journal 2026-10-04, tâches 1.1/1.4/1.6).
**Preuve** : capture avant/après + le nombre de favoris lu dans le sous-titre
`n lus récemment · n favoris` (`page.tsx:2737-2738`).

### T07.3 — Signets

**Attendu** : section « Vos signets » listant titre, source, chapitre, `page i/N`, ancienneté,
avec `Rouvrir` et `Retirer` (`page.tsx:2819-2876`).
**Clic** : `Rouvrir` sur le signet marqué en T05.7.
**Attendu** : la fiche se recharge, puis le lecteur s'ouvre **à la page exacte marquée**.
**Preuve** : capture du lecteur avec `i/N` == page du signet.
**Nettoyage obligatoire** : `Retirer` tous les signets, sinon les sessions suivantes
héritent d'un état non vierge (journal 2026-10-03 : « un walk qui modifie un état persistant
doit le remettre à zéro au démarrage »).

### T07.4 — Favoris et historique

**Attendu** : « Vos favoris » en grille ; `♥` pour retirer un favori (`page.tsx:2932-2938`) ;
`Oublier` sur une carte de lecture (`page.tsx:2798-2811`) ; `Vider l'historique` dans le
bandeau « Vos favoris » (`page.tsx:2883-2892`).
**Preuve** : capture après chaque action. **Rappel** : retirer un signet ou un favori ne doit
pas toucher l'historique (tests `un_signet_ne_touche_pas_l_historique`,
`desactiver_une_source_ne_touche_pas_les_favoris`).

---

## 8. Paramètres (T08)

Onglet **Paramètres** (`page.tsx:1681-1821`). Quatre panneaux : Contenu, Réseau, Téléchargement,
Catalogues affichés.

### T08.1 — Classification de contenu

**Clic** : `Contenu affiché` (`aria-label="Classification de contenu maximale affichée"`,
`page.tsx:1702`) → `Toutes`.
**Attendu** : les sources adultes renvoient alors des résultats ; le réglage survit au
redémarrage (`localStorage`, `page.tsx:1700`).
**Preuve** : capture du sélecteur + le nombre de résultats d'une source adulte, avant/après.
**Régression à surveiller** : le réglage ne doit jamais être interprété comme `SAFE` quand il
vaut « Toutes » (`AGENTS.md` §14 ; test `une_chaine_vide_ne_signifie_pas_safe`,
`commands.rs:257-265`). Si une source adulte renvoie 0 résultat en « Toutes », c'est ça.

### T08.2 — Délai avant abandon

**Clic** : `Délai avant abandon` → `30 secondes` (`page.tsx:1721-1735`).
**Attendu** : une source lente est signalée hors service au lieu de faire attendre.
**Preuve** : capture + le message d'erreur court (`page.tsx:402` :
« n'a pas répondu à temps. Site mort ou trop lent. »).

### T08.3 — Taille de page

**Clic** : `Nombre de webtoons par page` → `48` (`page.tsx:1748-1758`).
**Attendu** : la page source charge 48 cartes au lieu de 24 ; le réglage persiste.
**Preuve** : compteur `48 webtoons` en capture.

### T08.4 — Catalogues affichés

**Clic** : `Tout désactiver` puis `Tout activer` (`page.tsx:1784-1794`), ou une pastille de
catalogue (`page.tsx:1805-1813`).
**Attendu** : le compteur `x / y catalogues` se met à jour ; au-delà de 200 catalogues, le
message « 200 premiers catalogues sur n » apparaît (`page.tsx:1816-1818`).
**Preuve** : capture du compteur dans les deux états.

### T08.5 — Bouton « Tout réessayer »

**Condition** : au moins une source hors service a été visitée.
**Clic** : `Tout réessayer` (`page.tsx:1769-1777`).
**Attendu** : le bandeau disparaît ; la prochaine visite de la source retentera la requête.
**Preuve** : capture du bandeau avant/après.
**Nettoyage obligatoire** : revenir sur Paramètres après le test.

---

## 9. Source protégée (T09) — CONDITIONNEL

`PLAN.md` §6 : **aucune source de l'échantillon ne renvoie réellement `CHALLENGE_REQUIRED`**
(MANGALIB et BAOZIMH renvoient un 403 classique). Ce test est donc à **skipper avec la mention
« non déclenchable »** tant qu'aucun site protégé n'est identifié — c'est un résultat
acceptable, pas un échec.

**Si** un jour une source renvoie `CHALLENGE_REQUIRED` :

1. **Clic** : **Résoudre la vérification** (`page.tsx:2576-2582`).
2. **Attendu** : une fenêtre « Vérification — <source> » s'ouvre sur l'URL du challenge
   (`challenge.rs:70-71`), avec le **user-agent du moteur** (`challenge.rs:75-78`).
3. **Action humaine** : résoudre le challenge dans cette fenêtre.
4. **Clic** : **C'est fait, réessayer** (`page.tsx:2583-2589`).
5. **Attendu** : les cookies (y compris `HttpOnly`) sont transmis au moteur via `setCookies`
   (`challenge.rs:106-141`), la fenêtre se ferme, la requête est rejouée et la liste s'affiche.
6. **Preuve** : capture de la fenêtre de vérification ; dans le rapport, le **nombre de cookies
   transmis** renvoyé par `solve_challenge` (mesuré : 4 le 2026-10-03) ; capture de la liste
   rejouée.
7. `Annuler` (`page.tsx:2590-2592`) doit fermer la fenêtre **sans** appliquer les cookies.

**Limites à consigner** (`challenge.rs:9-15`) : la clearance est liée à l'IP et expire ;
l'empreinte TLS de WebView2 diffère de celle d'OkHttp, donc certains sites peuvent refuser la
clearance même après résolution.

---

## 10. Persistance après vrai redémarrage (T10)

**Action** : fermer la fenêtre, puis relancer `pnpm tauri dev`.
**Attendu** :

| Élément | Attendu |
|---|---|
| Favori | présent, **avec sa classification** (`content_rating`) |
| Historique | présent, avec `page i/N` exact |
| Incognito | état conservé (`page.tsx:2754`) |
| Sources désactivées | toujours désactivées (T02.3) |
| Signets | présents, au même ordre (plus récent d'abord) |
| Réglages (classification, délai, taille de page) | conservés |

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

| # | Geste | Attendu | Preuve dans le code |
|---|---|---|---|
| 1 | Réglages → section « Sources perso » | panneau présent, liste vide, message « Aucune source personnalisée. » | `page.tsx` section `sources.perso.titre` ; clés `sources.perso.vide` |
| 2 | Bouton « Importer un script… » | dialogue natif ouvert, filtre `*.js` | `open({ filters: [{ extensions: ['js'] }] })` ; capability `dialog:allow-open` |
| 3 | **Annuler** le dialogue | aucun message, aucun fichier créé, bouton non bloqué | le `return` sur annulation remet `importPersoEnCours` à faux |
| 4 | Choisir `docs/mangadex-api.js` | toast « Source "…" importée. », la ligne apparaît (nom `mangadex-api`, date du jour). **La langue n'est pas affichée** : la colonne est écrite en dur à `fr` par la commande d'import (`custom_source.rs`, TODO S2). | `invoke('importer_source', { path })` |
| 5 | `%APPDATA%\Tatami\sources\` | un `.js` présent, base `custom_sources` à 1 ligne | `custom_source.rs` §Commandes |
| 6 | Copier le script en `autre-source.js` (contenu identique), l'importer | **deux** lignes : `mangadex-api` et `autre-source`. Le nom de fichier fait foi — le même `SOURCE.id` ne crée pas de collision | `identiteDe` (`ScriptedSourceLoader.kt`) |
| 7 | Réimporter le **même** fichier | toujours 1 seule ligne pour cet id, horodatage mis à jour — pas de doublon | `ON CONFLICT(id) DO UPDATE` (`store.rs:1066`) |

### §S.2 Le moteur la voit-elle

`docs/mangadex-api.js` est le script de démonstration : il appelle l'API JSON de MangaDex
(sans clé, sans compte) et sert de **vraies** pages. Le gabarit `docs/exemple-source.js` sert
à écrire le sien, mais il pointe vers `https://exemple-site.fr`, qui n'existe pas : il se charge
et s'affiche, mais toute recherche renvoie une erreur réseau. Ce n'est pas un défaut du moteur.

Chaîne déjà mesurée contre le moteur réel le 2026-07 (NDJSON, `serve`) : `listSources` →
`custom:mangadex-api` ; `search "hinamatsuri"` → 2 résultats avec couvertures ; `details` →
137 chapitres ; `chapters` → 137 ; `pages` → 36 URLs ; la 1ʳᵉ page téléchargée en
`HTTP 200 image/jpeg 347 159 o`.

Puis dans **l'application** (pilotage CDP du 2026-07, moteur reconstruit) : `list_sources:
1371 sources` (1370 natives + 1 perso) ; `latest source=custom:mangadex-api` → `latest ok:
20 items` ; les 20 couvertures sont téléchargées (141 Ko à **20 Mo** pour la plus grande) ;
la fiche affiche « MangaDex (API) » et non `custom:mangadex-api`.

Deux choses à savoir avant de jouer cette section :
- **Le moteur doit avoir été reconstruit** (`pnpm engine`) ET aucun moteur installé ne doit
  passer devant : `main.rs` donne la priorité à `%APPDATA%\Tatami\moteur\actif.json`. Avec un
  moteur installé antérieur à la phase S, l'import « marche » (c'est Rust qui copie le
  fichier) mais rien n'apparaît dans le catalogue — c'est ce qui a été observé le premier
  jour.
- **Une couverture MangaDex peut peser 20 Mo** : l'API ne sert pas de variante `.256.jpg`
  (404 mesuré, seules `.jpg` et le nom brut répondent 200). Un script qui veut des
  vignettes légères doit passer par un CDN qui propose des tailles.

| # | Geste | Attendu |
|---|---|---|
| 8 | Catalogues | la source apparaît dans la grille, **badge « Perso »**, nom lisible. La tuile porte l'**identifiant** `custom:mangadex-api` et la langue du script (`ja`) |
| 9 | Ouvrir la source, rechercher `hinamatsuri` | 2 résultats, couvertures visibles, « Hinamatsuri » en tête |
| 10 | Ouvrir la fiche | titre, résumé, 4 genres, auteur « Ootake Masao », **137 chapitres** ; le lecteur d'un chapitre affiche **36 pages** |

Un défaut ici n'est pas un défaut d'import : c'est le `ScriptedSourceLoader` (dossier,
WatchService) ou le script lui-même.

Sur cet écran, une barre affiche « n source importée(s) » et le bouton
« Importer un script… » (S2.3) : c'est **le même dialogue** que l'étape 2. Après
l'import, la nouvelle tuile peut mettre **~0,6 s** à apparaître (le moteur exécute le
script après l'avoir vu au WatchService) — si elle ne vient pas au premier coup d'œil,
attendre 2 s avant de conclure. Le compte vient de la grille : une source dont le script
est invalide n'y est pas, alors qu'elle reste listée dans Réglages.

### §S.3 Désactivation et suppression

| # | Geste | Attendu |
|---|---|---|
| 11 | Interrupteur de la tuile | source grisée, comme une source native (même code, clé `custom:mangadex-api`) |
| 12 | Réglages → ✕ sur la ligne | la ligne bascule sur « Supprimer {nom} » + « Annuler » — **pas** de dialogue natif |
| 13 | « Annuler » | la ligne revient à son état initial, rien n'est effacé |
| 14 | « Supprimer {nom} » | toast « Source "…" supprimée. », ligne disparue, `.js` absent du dossier |
| 15 | Catalogues | **la source a disparu de la grille** — c'est l'étape qui manquait avant le correctif d'identité du 2026-10-07 (le moteur cherchait `exemple-fr.js` et ne déchargeait rien) |

Le dialogue natif de l'étape 2 n'est **pas** testable par CDP (leçon `AGENTS.md` §14) :
l'étape 4 se vérifie à l'œil, ou par la présence du fichier sur le disque.

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
