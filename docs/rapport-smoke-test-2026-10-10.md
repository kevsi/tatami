# Smoke test manuel — rapport du 2026-10-10

Première exécution de `docs/smoke-test.md` depuis son écriture (2026-07).
Application réelle pilotée en CDP (webview Tauri, port **9224** — 9223 est Vaultly),
moteur `engine/dist/tatami-engine` compilé le 2026-10-09.

Toutes les valeurs ci-dessous sont **relevées**, pas déduites. Les sondes sont
dans `.mesures/smoke/` (non versionné) ; chaque chiffre a été produit par une
commande exécutée.

---

## 1. Environnement et état de départ

| | |
|---|---|
| Windows | 10 (IoT LTSC 19044) |
| JDK | Temurin 21.0.12.1 |
| Rust | 1.99.0 (b940084d7 2026-09-28) |
| Node / pnpm | v26.10.0 / 12.8.1 |
| Base | `cc8ed92`, 1372 sources, 20 `.cbz` |
| Langue / thème à l'arrivée | **anglais / sombre** (hérités d'une session antérieure) → basculés en français par l'UI pour la suite |

**État persistant relevé avant de jouer** (c'est lui qui rend les étapes 6 et §S
non vierges) :

| Élément | Valeur au départ |
|---|---|
| Dossier de téléchargement **réglé** | `C:\Users\alexanders\Documents\Workspaces\Tatami` |
| …son contenu | 20 `.cbz`, 86,0 Mo, 7 webtoons, 3 dossiers |
| Dossier de téléchargement **par défaut** | `%APPDATA%\Tatami\downloads` — **1 252 Mo**, 29 `.cbz`, **orphelins** |
| Sources personnalisées | 3 : `mangadex-api.js`, `mangadexFr.js`, `scan-manga.js` |
| Favoris | 3 · Historique 24 · Signets 2 |
| Sources désactivées | 9 |
| `moteur/actif.json` | **absent** (le moteur de `engine/dist` est bien utilisé) |

> **Piège payé d'entrée** : chercher les téléchargements dans le dossier
> **par défaut** fait conclure « rien n'a été téléchargé » alors que la
> bibliothèque est pleine. C'est la leçon `AGENTS.md` §14 sur le dossier réglé
> par l'utilisateur, et elle a été écrite dans le document **avant** de jouer.

---

## 2. Résultat par section

| Section | Résultat | Une ligne de preuve |
|---|---|---|
| **§0** préparation | ✅ | moteur présent, dialogue natif d'import non automatisable |
| **§1** démarrage (T01) | ✅ | 4 onglets `data-tab`, 1 actif ; bandeaux « 1 mise à jour », « Continuer la lecture », « Vos signets », « Vos favoris » |
| **§2** catalogues (T02) | ✅ | 20 tuiles de langue (19 + perso) ; « 21 sources pour « MANGA » · langue FR » ; désactivation persistante et réversible |
| **§3** source (T03) | ✅ | 24 webtoons ; tri `A→Z` / `Z→A` **inversion exacte sur les 24 titres** ; genre « Boys Love » 24 → 19 puis retour à 24 ; `24/24` chapitres récents |
| **§4** fiche (T04) | ✅ | résumé sans balise (256 car.) ; **6 colonnes**, **100 cellules**, « Afficher plus (1100 restants) » ; filtre « 100 » → 15 cellules ; favori bascule avec `aria-pressed` |
| **§5** lecteur (T05) | ✅ | 1ʳᵉ image en **4,7 s** (1200 × 1878) ; 56 pages, bande de 66 533 px ; `←`/`→` **symétriques** à ±1 044 px ; signet en page 56/56 ; chapitre courant marqué |
| **§6** téléchargements (T06) | ✅ | `.cbz` **relu par un lecteur strict** : 54 entrées, 54 tailles distinctes, 54 JPEG ; pause propre avec chapitre terminé ; **kill en plein téléchargement → « File interrompue » au relancement** |
| **§7** accueil (T07) | ✅ | « Reprendre » ouvre à `1/56` comme la carte l'annonçait ; « Rouvrir » un signet ouvre à `56/56` ; favori et signet de test retirés ensuite |
| **§8** réglages (T08) | ✅ | 14 panneaux ; SAFE→ALL, 60→30, 24→48, 1372→0→1372, tous persistants ; tableau « État des sources » avec regroupement par code |
| **§9** challenge (T09) | ⚠ **partiel** | TOONFR déclenche bien `CHALLENGE_REQUIRED`, alerte et journal le montrent ; la **résolution humaine** n'est pas jouable en automatisation |
| **§10** persistance | ✅ (partiel) | obtenue pendant T06.3 : kill + relance → bandeau, état en base, `.part` conservé |
| **§11** console | ⚠ | 0 erreur sur un cycle complet des 4 onglets ; **1 avertissement** de clé React, localisé (voir §4) |
| **§S** sources perso | ❌ **1 défaut** | import non automatisable ; **le moteur sert 3 sources perso et l'écran n'en montre que 2** |

**Total : 41 étapes vérifiables, 38 réussies, 2 partielles, 1 échec.**

---

## 3. Ce que le test a appris sur le produit (mesures)

### 3.1 Le dossier de téléchargement réglé prime sur le défaut — et l'écart coûte 1,25 Go

`%APPDATA%\Tatami\downloads` contient **1 252 Mo** de fichiers d'avant le
changement de dossier. Rien ne les signale : ils ne sont ni dans la bibliothèque
affichée, ni dans un journal d'erreur. Un utilisateur qui cherche « où sont mes
téléchargements » tombera dessus.

### 3.2 Le tri `A → Z` / `Z → A` est exact

Vérifié sur la **liste entière** (24 éléments), pas sur les 5 premiers : deux
tableaux vides sont égaux, une « inversion » mesurée sur une liste vide ne prouve
rien.

### 3.3 Le filtre de chapitre est une **sous-chaîne**

Filtrer « 100 » sur une série à 1 200 chapitres laisse **15** cellules, pas une.
Le document annonçait « une seule cellule reste » : c'est faux, et le tester
tel quel produirait un échec là où il n'y en a pas.

### 3.4 `←` et `→` sont symétriques — la mesure qui l'accusait était fausse

Premier relevé : `→` +499 px, `←` −1 045 px sur le même chapitre. Second chemin
(attendre la fin du défilement, 6 s de trace) : `→` +1 053, +1 044, +1 045 ;
`←` −1 045, −2 097, 0. **Symétriques.** Le 499 venait d'une mesure prise à
1,2 s, au milieu d'un `scrollTo({ behavior: 'smooth' })`, sur un état de
`reader.index` déjà en retard d'une page.

### 3.5 La file de téléchargement ne se crée **que** depuis la vue source

Lu dans le code, puis confirmé : `downloadSelected()` (le bouton de la fiche)
boucle `downloadChapter()` et n'écrit **rien** dans `QUEUE_KEY`. Seule
`runQueue()` — atteinte par `Sélectionner` puis `Télécharger la sélection` dans
la vue source — persiste une file. Le test documenté partait de la fiche :
**T06.3 et T06.5 étaient injouables**, pas « en échec ».

### 3.6 Le bouton de reprise n'est pas dans le panneau de file

Après une pause, `section.panel.queue` n'offre que « Effacer la file ». La reprise
est dans `section.panel.resume` (« File interrompue »), qui apparaît **dès la
pause**, pas seulement après un redémarrage.

### 3.7 Un téléchargement réussi n'est pas un téléchargement valide

`Chapitre 1.cbz` relu par `.mesures/verifier-cbz.py` : **54 entrées, 54 tailles
distinctes, 54 JPEG, 3 057 971 o**. Un lecteur naïf aurait accepté 54 fois la
même page.

---

## 4. Défauts trouvés

### D1 — Une source servie par le moteur peut rester invisible au catalogue

**Gravité : moyenne.** C'est le critère de sortie de la phase S.
**Cause trouvée et reproduite le 2026-10-10 — voir « D1, cause » plus bas.
La lecture initiale était incomplète, pas fausse : il manquait un terme.**

| Grandeur, **au même instant** (un seul `evaluate`) | Valeur |
|---|---|
| `list_sources` — total | **1373** |
| `list_sources` — sources `custom` | **3** (`custom:autre-source`, `custom:scan-manga`, `custom:mangadex-api`) |
| Second appel, 2 s plus tard | **1373 / 3**, identique |
| `list_disabled_sources` | **`[]`** |
| En-tête de l'écran | « **1372** sources disponibles » |
| Tuiles de la vue « Sources personnalisées » | **2** — `custom:autre-source` absente |

Reproduction : déposer `autre-source.js` (copie de `docs/mangadex-api.js`, donc
le **même** `SOURCE.id`), laisser le WatchService le voir — le journal le
confirme : `Source personnalisée chargée : autre-source (déclarée « mangadex-api »,
MangaDex (API), ja)` — puis recharger la page. **Deux rechargements complets ne
changent rien.**

**Ce qui est établi** : le registre du moteur est bien indexé sur le nom de
fichier (`identiteDe`), donc le même `SOURCE.id` **ne crée pas** de collision —
la règle d'ADR 0010 tient. Le retrait du fichier fait bien disparaître la source
(`Source personnalisée retirée : autre-source`).

#### D1, cause — la liste blanche `tatami.enabledSources` (2026-10-10)

Il manquait un troisième terme à la comparaison : ni `list_sources` ni
`list_disabled_sources` ne décrivent la liste blanche, qui vit dans
`localStorage`.

| Étape, mesurée | Moteur | Liste blanche | En-tête affiché |
|---|---|---|---|
| Avant dépôt | 1372 | 1372 | 1372 |
| `autre-source.js` déposé et chargé | **1373** | 1372 | **1372** |
| **On ajoute le seul nom manquant à la liste** | 1373 | **1373** | **1373** |
| Fichier retiré | 1372 | 1372 | 1372 |

L'écart est stable sur 8 relits espacés de 10 s — ce n'est donc pas React en
retard. La troisième ligne est la preuve du lien : on ne modifie que le nom
dans la liste, et l'écart disparaît.

**Mécanisme** : l'en-tête affiche `visibleSources.length`, qui applique
`enabled === null ? sources : sources.filter(s => enabled.includes(s.name))`.
`enabled` vient de `localStorage['tatami.enabledSources']`, une **liste fermée**
écrite par « Tout activer » et par le basculement d'une source. Toute source
apparue ensuite est donc filtrée en silence.

**Portée plus large que les sources perso** : une source neuve apportée par une
mise à jour du catalogue disparaît exactement de la même façon.

#### Les deux options, chiffrées — et celle qui a été appliquée

Réconcilier en ajoutant les noms manquants au chargement **ne marche pas** : une
source désactivée par l'utilisateur est précisément un nom *absent* de la liste,
la réconciliation la réactiverait. Il faut distinguer « absent parce que
désactivé » de « absent parce que jamais vu ».

| | Option A — deux clés | Option B — inverser le modèle |
|---|---|---|
| Données | `enabledSources` (inchangée) + `seenSources` | une seule clé, les **désactivées** |
| Sens par défaut d'un nom absent | « activée » s'il n'a jamais été vu, sinon inchangé | « activée » |
| « Tout activer » | écrit `enabledSources` = toutes | vide la liste des désactivées |
| Migration | au 1ᵉʳ lancement, `seenSources` = `enabledSources` ∪ courante | reconstruire la liste des désactivées = `seenSources` − `enabledSources` — **impossible sans la clé A** |
| Risque | une clé de plus à maintenir | migration destructive : sans A, les sources désactivées sont **réactivées** |

**Option A appliquée le 2026-10-10** (décision humaine). `seenSources` =
l'univers des noms déjà présentés ; `reconcilierListeBlanche()` est une fonction
pure, testée sur 7 cas, dont le cas qui compte : **une source désactivée ne
revient pas** à la migration. Vérifié sur l'application réelle en trois temps —
migration sans réactivation, import visible sans intervention, persistance au
rechargement. Détail dans le journal `PLAN.md` du 2026-10-10.

**Piège à ne pas rejouer** : deux relevés **non simultanés** ne se contredisent
pas forcément. Trois passes ont comparé un `list_sources` à une capture prise
autant de secondes plus tôt et ont conclu à une contradiction inexistante. La
seule mesure qui tranche est celle où les trois grandeurs sont lues dans le même
`evaluate` — et il fallait en ajouter une **quatrième** : la liste blanche, qui
n'est visible ni dans `list_sources` ni dans `list_disabled_sources`.

### D2 — Un script installé et cassé reste listé sans explication

**CORRIGÉ le 2026-10-10.** La description ci-dessous est celle du constat ; la
cause et le correctif sont dans le journal `PLAN.md` du 2026-10-10.

`mangadexFr.js` est rejeté par le moteur :

```
AVERTISSEMENT: Script ignoré (mangadexFr.js) : Erreur d'évaluation du script :
<eval>:20:36 Missing close quote
```

Il reste **listé dans Réglages → Sources personnalisées** et **absent du
catalogue**. Le joueur voit une source qu'il a installée et qui n'existe nulle
part, sans message. Le journal est la seule trace, et il n'est pas dans l'écran.

**Pourquoi l'écran ne pouvait rien afficher** : le moteur écrit ses refus sur
stderr, jamais sur stdout (cf. contrat) ; le superviseur les journalisait puis
les jetait. `list_sources` ne pouvait donc pas les porter.

**Correctif** : une copie bornée des refus est conservée par le superviseur,
relayée par la commande `scripts_refuses`, et affichée sur la rangée du script
avec la raison et sa position. Les deux scripts valides n'ont **aucune** puce —
c'est ce qui prouve que l'affichage distingue les deux cas.

### D3 — Doublons de clés React : cause trouvée pour SCANVF

`SCANVF` rend **41 cartes pour 4 URL distinctes** (`/one_piece` ×15,
`/kingdom` ×12, `/one-punch-man` ×9, `/boruto` ×5). La clé est
`${source}|${url}` : elle est **correcte**, c'est sa **valeur** qui est
répétée. `react/jsx-key` ne peut donc pas le voir — il vérifie la présence de
l'attribut, pas son unicité. C'est le parseur de la source qui répète ses
entrées, donc le correctif appartient au parseur, pas à l'interface.

**Reste ouvert** : une occurrence de l'avertissement sur `TOOMICSFR`, dont les
48 cartes ont 48 clés distinctes, donc **une autre** liste. Rejouée sur deux
parcours propres, jamais reproduite. Le message exact n'a pas été conservé — le
prochain qui le voit doit **copier le texte entier**, `%s` compris, sinon il ne
sait pas s'il s'agit d'une clé manquante ou dupliquée.

### D4 — Dettes i18n et de fond de bande

- **Libellés en dur en français** (laissé en anglais sur une interface française
  pour certains, ou l'inverse) : `Genres`, `Effacer (n)`, `Choisir`,
  `Sur les chapitres filtrés`, `N derniers`, `N premiers`,
  `Filtrer un chapitre…`, `Nombre de webtoons par page`, `Terminé`, `Partiel`,
  `← Catalogues`, `← Chapitres`, `Réessayer`, `Changer de catalogue`.
  `AGENTS.md` §7 les interdit ; `pnpm i18n:check` ne les voit pas (il vérifie les
  clés, pas les littéraux).
- **Message du moteur en anglais** dans le tableau « État des sources » :
  « The domain of JPMANGAS no longer exists ».
- **Placeholder qui déborde en fond de bande** : **1 205 px** (1 101 annoncés).
  Même famille, valeur qui dérive.
- **Premier plan de la LLM en 50 %** hérité de `localStorage` : le lecteur
  s'ouvre à 50 % sans que rien ne l'explique.

---

## 5. Nettoyage — prouvé par delta

| | Avant | Après |
|---|---|---|
| Bibliothèque (dossier réglé) | 20 `.cbz` / 86,0 Mo / 7 webtoons | **20 `.cbz` / 86,0 Mo / 7 webtoons** |
| Fichiers `.part` | 0 | **0** |
| Dossiers | EXHENTAI, MANGADEX, TOOMICSEN | **identiques** |
| Bandeau affiché | 7 / 20 / 86,0 Mo / 2 | **identique** |
| Sources perso installées | 3 | **3** (`autre-source.js` supprimé) |
| Favoris | 3 (+One Piece du test) | **3** |
| Signets | 2 (+One Piece du test) | **2** |
| Incognito / langue | coché / anglais | coché / **français** (choix du test) |

**Non restauré, et dit** : les **9 sources désactivées** du départ ont été
réactivées par le nettoyage de T02.3 (« Tout réactiver », demandé par le
document lui-même). Elles n'ont pas été notées avant, donc elles ne peuvent pas
être remittees à l'identique. C'est le prix d'un nettoyage qui n'enregistre pas
ce qu'il détruit : **le document doit demander de relever la liste avant**.

---

## 6. Ce que ce smoke test ne prouve toujours pas

- **L'import d'un script** (§S.1, étapes 2 à 6) : le dialogue natif n'est pas
  automatisable. `Escape` confirme au lieu d'annuler pendant un test, et
  `window.__TAURI_INTERNALS__` est gelé.
- **La résolution d'un challenge Cloudflare** (§9, étapes 3 à 7) : action
  humaine dans une fenêtre.
- **La persistance complète** (§10) : obtenue par le kill de T06.3, mais pas
  rejouée sur les favoris, les réglages et l'incognito après un vrai redémarrage.
- **Le catalogue affiché en anglais** : le jeu s'est fait en français. L'i18n de
  l'interface est vérifié par `pnpm i18n:check` et par 30+ littéraux en dur, pas
  par ce document.