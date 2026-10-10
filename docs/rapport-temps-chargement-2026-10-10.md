# Temps de chargement et de réception des données — webtoons

Date des mesures : **2026-10-10**. Machine : portable Windows 10, i5-6200U, 12 Go
de RAM. Sources : `SCANVF` (One Piece VF), `TOOMICSFR` (webtoons FR),
`TOOMICSIT` / `TOOMICSPT` (Toomics, couvertures jamais téléchargées avant).

Toutes les valeurs ci-dessous sont **exécutées**, pas estimées. Chaque tableau
indique d'où vient la mesure. Les scripts sont dans `.mesures/smoke/` (hors
dépôt) : `synthese-temps.cjs`, `journal.cjs`, `temps-1` à `temps-9`.

---

## 1. Deux sources de temps, à ne jamais confondre

| Source | Ce qu'elle donne | Ce qu'elle ne donne pas |
|---|---|---|
| `tatami.log` — `stdout recu id=<méthode>-<epoch_ms>-<n>` | l'instant d'**envoi** (l'`epoch_ms` est dans l'identifiant) et l'instant d'**arrivée** | le détail réseau/HTTP interne au moteur |
| `tatami.log` — `pagecache: page <jeton> moteur a repondu en <n> ms` | le temps du moteur pour **une image**, cache absent | le temps d'affichage à l'écran |

L'interface, elle, ne mesure que la **réponse**. Le journal est la source
autoritative du **départ**. Les deux sont rapportées séparément ci-dessous.

---

## 2. Démarrage à froid

Mesuré par `temps-6c-demarrage.cjs` : le script tue le processus, attend 5 s,
relance, puis sonde CDP lui-même jusqu'à ce que la fenêtre réponde — une mesure
prise depuis un `connectOverCDP` déjà ouvert aurait mesuré une application déjà
chargée. `t0` = juste avant `Start-Process`.

| Étape | t depuis `t0` |
|---|---|
| La fenêtre existe (CDP répond) | **1 156 ms** |
| L'accueil est utilisable (bandes + 24 cartes de reprise) | **2 609 ms** |
| La grille des langues est là (20 tuiles, « 1372 sources disponibles ») | **5 084 ms** |

Côté moteur, sur la même fenêtre :

| Requête | reçue à | traitement |
|---|---|---|
| `hello` | 4 787 ms | 3 715 ms |
| `listSources` | 4 942 ms | **2 883 ms** |

**Lecture** : l'accueil n'attend **pas** le moteur — il sort de SQLite locale et
est utilisable à 2,6 s. Le catalogue, lui, attend le démarrage de la JVM
(`hello` à 4,8 s) puis la construction du catalogue des 1 372 sources
(**2,9 s**, aucun réseau : c'est l'instanciation des parseurs). Le « listSources
à 10 s » du journal du 2026-10-08 se reproduit : c'est ce palier.

---

## 3. Latence par méthode (session entière, 2 journaux)

`synthese-temps.cjs`, `tatami.log` + `tatami.log.prec`. L'écart `p10 → médiane`
est l'indicateur de file : un rapport proche de 1 dit que la lenteur vient de la
source, un rapport élevé dit que des requêtes attendent.

| méthode | n | p10 | médiane | p90 | max | total | p10→méd |
|---|---|---|---|---|---|---|---|
| `chapters` | 1 510 | 878 ms | **5 757 ms** | 17 251 ms | 37 115 ms | 190 min | 6,6× |
| `fetchImage` | 4 390 | 270 ms | **4 633 ms** | 24 821 ms | 135 958 ms | 683 min | **17,2×** |
| `fetchCover` | 712 | 1 422 ms | **4 427 ms** | 19 690 ms | 47 017 ms | 96 min | 3,1× |
| `hello` | 69 | 3 367 ms | 4 159 ms | 7 437 ms | 12 956 ms | 6 min | 1,2× |
| `latest` | 145 | 1 044 ms | 2 938 ms | 8 678 ms | 34 873 ms | 10 min | 2,8× |
| `details` | 137 | 884 ms | 2 381 ms | 12 957 ms | 38 015 ms | 11 min | 2,7× |
| `filterOptions` | 120 | 3 ms | 1 289 ms | 5 463 ms | 25 253 ms | 5 min | — (réponses en cache) |
| `pages` | 179 | 423 ms | 969 ms | 5 737 ms | 22 858 ms | 6 min | 2,3× |
| `listSources` | 288 | 21 ms | 56 ms | 763 ms | 8 939 ms | 2 min | 2,7× |

---

## 4. Images : le cache fait 90 % du travail, et le reste est en file

| | n | p10 | médiane | p90 | max | total |
|---|---|---|---|---|---|---|
| Cache absent (moteur appelé) | 442 | 752 ms | **3 698 ms** | 22 310 ms | 47 030 ms | **59 min** |
| Cache présent (disque) | 4 039 | 0 ms | **1 ms** | 10 ms | 141 ms | < 1 min |

Cache disque au moment de la mesure : **2 177 fichiers, 254,4 Mo**
(`%APPDATA%\Tatami\cache`).

**Ce n'est pas le réseau qui est lent, c'est la file.** Preuve dans le journal,
ouverture du chapitre 500 de `SCANVF` (fenêtre 09:02:05 → 09:03:05 UTC) :

```
+7 428 ms  page entree ×6        ← les 6 premières images, en 11 ms d'écart
+7 429 ms  cache absente, appel moteur  (×6)
+12 128 ms  ped71f47 moteur a repondu en 4 700 ms
+13 133 ms  p7fd41e1 moteur a repondu en 5 704 ms
+14 433 ms  pcb0f645 moteur a repondu en 6 998 ms
+15 432 ms  pedd40d0 moteur a repondu en 8 000 ms
+16 423 ms  p9080def moteur a repondu en 8 995 ms
+17 247 ms  pe42b752 moteur a repondu en 9 814 ms
```

Six demandes **simultanées**, six réponses **espacées de ~1 000 ms**, et chaque
requête déclare un temps de plus en plus long parce qu'elle attend son tour. Sur
442 images froides, **22 min des 59 min** de temps moteur ne sont que de
l'attente.

**Cause, lue dans le code et non devinée** — `engine/src/main/kotlin/org/tatami/engine/ParserPool.kt` :

```
l. 68-72  « Une instance par source, sérialisée par un Mutex. Les parsers ont un
           état mutable (PagedMangaParser porte un Paginator) : deux requêtes
           concurrentes sur la même source se marcheraient dessus. »
l. 85     val mutex = Mutex()
l. 111    return entry.mutex.withLock {
```

`latest`, `search`, `chapters`, `details`, `pages`, `fetchImage` et `fetchCover`
passent **tous** par `avec(source)`. Un `Mutex` par source, donc **une seule
opération à la fois par source** — y compris entre l'image que l'utilisateur
attend et les 36 requêtes de fond déclenchées par l'écran catalogue.

---

## 5. Chaîne mesurée, écran par écran

Chaque ligne est une mesure exécutée dans une fenêtre de journal identifiée.
Ces étapes ont été mesurées **séparément** : les additionner compose un trajet,
ce n'est pas un trajet mesuré d'un bout à l'autre.

| Écran | Mesure | Valeur | Journal |
|---|---|---|---|
| Accueil | utilisable | **2 609 ms** | — |
| Catalogue | grille des langues | **5 084 ms** | `listSources` 2 883 ms |
| Source ouverte (48 cartes) | 1ʳᵉ carte | **1 601 – 2 734 ms** | `latest` 1 233 – 3 933 ms |
| Source ouverte | couvertures à froid | **11,3 s pour la 12ᵉ** | `fetchCover`, voir §6 |
| Fiche, machine calme | 1ʳᵉ bouton chapitre | **3 126 – 3 658 ms** | `details` 1,5 s + `chapters` 3,1–3,5 s |
| Fiche, fond en cours | 1ʳᵉ bouton chapitre | **jusqu'à 12 305 ms** | `chapters` méd. 5 770, max 15 289 |
| Lecteur, chapitre froid | 1ʳᵉ image **décodée** | **2 851 – 3 268 ms** | `pages` 528–831 ms |
| Lecteur, fond saturé | 1ʳᵉ image **décodée** | **13 571 ms** | 36 `chapters` en vol |
| Lecteur | 6ᵉ image | **5 703 – 7 105 ms** | 6 `fetchImage` |

« Image décodée » veut dire `img.complete && img.naturalWidth > 0` : une `<img>`
cassée compte comme visible pour Playwright, et le compteur de pages bouge sans
que la bande bouge.

---

## 6. Le fond « chapitres récents »

C'est l'effet de la case cochée par défaut dans l'écran source. Mesuré sur
`TOOMICSFR`, cache local vidé au préalable :

- **36 requêtes `chapters` en 59 s**, latences **1 470 → 15 289 ms**, médiane **5 770 ms**
- la case affiche encore « 4 en cours » à la fin de la fenêtre : la rafale n'est pas terminée
- dans la même fenêtre, `fetchCover` : 9 requêtes, **7 171 – 14 371 ms**
- fiche ouverte **pendant** la rafale : `chapters` à **12 305 ms** jusqu'au 1ʳᵉ bouton
- fiche ouverte **après** : **3 126 – 3 658 ms**

**Le fond est la cause, pas la source** : la même fiche, le même chapitre, la
machine dans le même état, varie de 3,1 s à 12,3 s selon qu'une rafale tourne.

**Mais il ne se déclenche qu'à la première visite.** Au deuxième passage sur la
même source, le cache local (30 min) sert tout : le compteur affiche « 48/48 »
d'emblée et **une seule** requête `chapters` part pour toute l'ouverture. Le
coût est donc un coût de **première visite**, ce que le smoke test joue à chaque
fois.

---

## 7. Couvertures : deux raisons distinctes, il faut les séparer

**Première tentative, et sa correction.** J'ai mesuré « 0 `fetchCover` en 150 s
sur `TOOMICSIT`, 18 couvertures sur 48 jamais chargées » et j'en ai conclu à une
file. C'était faux. La seconde voie qui devait réussir a réussi : dans
`pagecache.rs`, `PageRef::Couverture` n'est résolu que **quand la webview
réclame le jeton**, et `<img loading="lazy">` ne réclame que ce qui entre dans le
viewport. Je n'avais pas fait défiler — je mesurais l'absence de demande, pas une
file.

**Reprise avec défilement**, source `TOOMICSPT` (couvertures jamais
téléchargées : 0 chargée au départ), fond décoché :

| | |
|---|---|
| Chargées au départ | **0 / 48** |
| Chargées après 8 paliers de défilement de 600 px | **12 / 48** |
| Écart entre deux arrivées de couverture | **503 – 2 041 ms**, médiane **~720 ms** |
| Latence de la 1ʳᵉ → de la 12ᵉ | **1 861 → 11 319 ms** |
| Requêtes `chapters` sur la même fenêtre | **1** |

Le 1 à la fin est ce qui tranche : avec une seule requête de concurrence, les
**~720 ms par couverture sont le coût intrinsèque**, pas de l'attente. La file
reste visible dans la croissance linéaire (1,9 s → 11,3 s), mais le plancher est
réel.

**Une grille de 24 couvertures visibles prend donc ~17 s à se remplir**, et les
couvertures sont petites (270 × 400). C'est le poste le plus lent de l'écran
catalogue, et il est invisible tant qu'on ne fait pas défiler — donc invisible
pour un test qui ne défile pas.

---

## 8. Ce que ces chiffres ne disent pas

- **Rien sur le réseau intérieur au moteur.** Le journal donne l'aller-retour ;
  séparer le temps HTTP du temps d'analyse demanderait une sonde dans le JVM.
- **Rien sur la répartition par source.** Les médianes agrègent `SCANVF`
  (One Piece, 300 Ko/page) et `TOOMICSFR`. Le p10 est le meilleur estimateur du
  coût non bruité ; la médiane mélange les sources lentes et les files.
- **Rien sur l'interruption d'un téléchargement.** Un téléchargement de 168
  pages en cours produit un `fetchImage` à 135 958 ms — c'est le fond de la
  file, pas une page lente.
- **La rafale de « chapitres récents » n'a pas été mesurée jusqu'à son bout.**
  36 requêtes en 59 s, et le compteur affichait encore « 4 en cours ».

---

## 9. Deux leviers, tous deux des décisions de conception

Aucun n'est implémenté ici : les deux touchent `ParserPool` et le contrat de
priorité, donc §2.9 de `AGENTS.md` impose de demander.

**Levier 1 — sortir les images du `Mutex` de la source.**
`fetchImage` et `fetchCover` ne créent pas de page et ne consultent pas de
paginator ; le motif du `Mutex` (l'état mutable de `PagedMangaParser`) ne
s'applique pas à elles. Leur donner une file séparée, ou une priorité dans la
file existante, ramènerait la 1ʳᵉ image au plancher mesuré (~270 ms au lieu de
3,3 s en fond actif). **À lire avant de proposer** : je n'ai pas vérifié que le
`MangaLoaderContext` n'a pas d'état partagé qui rendrait la simultanéité
impossible — le commentaire de `ParserPool.kt` dit le contraire pour les
parseurs paginés, pas pour le client HTTP.

**Levier 2 — réduire le fond.**
36 requêtes pour 48 cartes, dont la plupart hors écran. Le même raisonnement que
la pagination des couvertures s'applique : ne demander que ce qui est visible.
Gain mesuré par le bras « CALME » : 1ʳᵉ image à 2 851 ms au lieu de 3 268 ms, et
1,4 s gagnés à la 6ᵉ image. Modeste tant que le fond est partiel ; le cas
dégradé mesuré (36 requêtes en vol) donne 3,3 s → 13,6 s.

---

## 10. Deux défauts de mesure rencontrés en route, pour mémoire

- **Ma regex d'image ratait les jetons.** `page ([0-9a-f]+) …` ne matche pas
  `pa5a93fe` ni `pd733163` : les jetons commencent souvent par `p`. Sur les deux
  journaux, elle voyait **4 144 lignes sur 4 481** — elle en ratait **337**, dont
  **234 « moteur a repondu »** et 103 « servie du cache ». Le biais allait donc
  contre les images froides, et le résultat rendu — « aucune image » — se lisait
  comme « instantané ». Corrigé en `[0-9a-z]+`, **toute** l'analyse refaite :
  les tableaux des §3 et §4 totalisent bien 4 481 lignes.
- **« 0 `fetchCover` » n'était pas une file.** Voir §7 : c'était du
  `loading="lazy"`. Un zéro se lit comme une absence d'événement ; il faut
  toujours nommer l'événement qu'on attendait.
