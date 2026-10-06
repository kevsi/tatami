# 0005 — Politique de suivi de l'amont : épingler par hash, surveiller mécaniquement

- Date : 2026-10-05
- Statut : acceptée
- Chemins cités : `engine/build.gradle.kts`, `engine/gradle/verification-metadata.xml`,
  `.github/workflows/amont.yml`, `docs/notes/amont.md`, `PLAN.md` § 7 (Risques)

## Contexte

Le moteur JVM dépend d'un unique artefact d'amont, déclaré dans
`engine/build.gradle.kts` :

```kotlin
implementation("com.github.Kotatsu-Redo:kotatsu-parsers-redo:b3c0000")
```

C'est une dépendance `com.github.*` résolue par **JitPack**, épinglée sur un
**commit** et non sur un tag — le fork amont n'en publie pas. C'est le lien le
plus solide du projet : tout le reste du moteur (les ~1 370 sources, les règles de
requête, les sélecteurs) vient de là.

État mesuré de la chaîne amont le 2026-10-05 (API GitHub) :

| Dépôt | Archivé | Dernier push | Rôle |
|---|---|---|---|
| `KotatsuApp/kotatsu-parsers` | **oui** | 2025-11-03 | l'original, parent du fork |
| `Kotatsu-Redo/kotatsu-parsers-redo` | **non** | 2026-09-17 | **celui qu'on consomme**, fork du précédent |
| `KotatsuApp/kotatsu` (l'app) | oui | 2025-11-04 | non consommé |
| `KotatsuApp/kotatsu-parsers-redo` | — | — | **n'existe pas** (404) |

Trois conséquences, et la première infirme une ligne du plan :

1. **La ligne « Amont archivé (dépôts KotatsuApp) » du tableau des risques est mal
   formulée.** Ce qui est archivé est l'original, **un cran en dessous** de ce
   qu'on utilise. Le dépôt qu'on consomme est vivant et poussait il y a
   trois semaines.
2. **Le fork de secours que cette tâche prévoyait existe déjà**, fourni par un
   tiers. Ce n'est pas nous qui l'avons fait, et ce n'est pas à nous de le
   maintenir.
3. **`b3c0000` est la tête de `master`** de ce fork (dernier commit « fixes »,
   2026-09-17). Il n'y a donc **rien à monter** en ce moment — et c'est un fait
   mesuré, pas une supposition.

Le risque réel n'est donc pas « l'amont va mourir », mais deux choses plus
précises :

- **Rien ne garantit que l'artefact servi est celui qu'on a vu.** JitPack reconstruit
  à la demande et le résout en HTTP : la dépendance est identifiée par une
  *promesse de service*, pas par son contenu. `engine/gradle/verification-metadata.xml`
  n'existait pas — mesuré le 2026-10-05 — donc **aucune** vérification par hash
  n'avait lieu au build.
- **Une politique écrite ne surveille rien.** Tant que personne ne regarde, l'épingle
  devient juste la version au moment où on a cessé de regarder.

## Décision

Quatre éléments. Les deux premiers sont une défense, les deux suivants une
politique rendue mécanique.

1. **L'épinglage sur commit est conservé tel quel.** `b3c0000`, jamais `master`,
   jamais `master-SNAPSHOT`, jamais un tag inexistant. Un pin sur une branche
   flottante rendrait le build non reproductible, et la reproductibilité est la
   seule chose qui distingue « le moteur se comporte comme hier » de « le moteur a
   changé et je ne sais pas comment ».

2. **La vérification de dépendances Gradle par hash est activée**, et
   `engine/gradle/verification-metadata.xml` est commité. Le JAR de parsers est
   alors identifié par son **SHA-256**, pas par la promesse de JitPack. Un
   artefact reconstruit différemment, remplacé sur le service, ou servi par un
   intermédiaire malveillant fait **échouer la construction** au lieu de produire
   un moteur silencieux différent. C'est, en un fichier, la moitié de ce que 5.7
   demandait sous le nom « miroir de secours ».

3. **La tête de l'amont se surveille mécaniquement**, par
   `.github/workflows/amont.yml` : un job planifié compare le commit épinglé à la
   tête de `master` du fork et **échoue** si elle a bougé. Il est volontairement
   dans un fichier **séparé** de `ci.yml`, dont l'en-tête interdit explicitement
   d'ajouter une commande qui ne figure pas dans AGENTS.md §8.

4. **La montée de version est une procédure en commandes**, dans
   `docs/notes/amont.md`, pas une intention.

## Alternatives étudiées

**Fork ou miroir de l'amont dans notre dépôt.** Écarté. AGENTS.md §13 interdit de
committer un binaire généré ; un miroir du dépôt amont n'en est pas un, mais il
dupliquerait un dépôt déjà public, déjà forké, et déjà servi par JitPack. Le coût
serait notre propre charge de maintenance (suivi des commits, resolutions de
conflit) pour un avantage que le fork amont fournit déjà. Le « risque » que le
tableau pointait visait — que l'amont disparaisse — est couvert par le fait qu'il
existe **deux** dépôts au lieu d'un.

**Vendoriser le JAR dans `engine/libs/`.** Écarté pour la même raison qu'au-dessus
(§13), et surtout parce que le JAR deviendrait un fichier **mort** : la mise à jour
deviendrait un remplacement manuel de fichier, donc une opération sans trace dans
l'historique, exactement le contraire de ce qu'on cherche.

**Vérification Gradle par clé PGP (`pgp`) au lieu du hash.** Plus fort que le hash
— c'est la seule méthode qui prouve *qui* a publié. Écarté : elle exige de posséder
la clé PGP de JitPack pour ses propres artefacts, ce qu'on n'a pas. Le hash
identifie le contenu ; il ne prouve pas l'auteur. C'est le maximum atteignable sans
être JitPack, et c'est suffisant contre le risque réel (un artefact différent de
celui qu'on a vu).

**Vérifier les dépendances à la main dans la CI** (re-télécharger, comparer une
empreinte stockée dans un script maison). Écarté : c'est réimplementer en shell ce
que Gradle fait déjà nativement, en moins bien et en plus maintenance.

**Supprimer la dépendance.** Explicitement interdit par AGENTS.md §13 : « le moteur
JVM est la source de vérité », on ne réécrit pas 1 370 sources.

## Conséquences

- **Le build devient plus strict, et c'est voulu.** Toute nouvelle dépendance, ou
  tout changement de JDK / version de Gradle qui change un artefact, fera
  **échouer** la construction tant que le hash n'est pas accepté. Il faut connaître
  la commande de régénération (`docs/notes/amont.md` § Régénérer), sinon on croira
  à une panne.
- **Monter l'épingle touche deux fichiers**, `build.gradle.kts` **et**
  `verification-metadata.xml`. C'est le prix de la reproductibilité : un pin seul
  n’engage à rien tant que rien ne le vérifie : c’est un commentaire, pas une
  contrainte.
- **L'épinglage ne garantit pas la disponibilité.** Si le fork amont disparaît,
  JitPack ne pourra plus reconstruire et l'artefact servi peut finir par ne plus
  répondre (mesuré OK le 2026-10-05 : HTTP 200 en 1,4 s). C'est le seul scénario
  qui justifierait vraiment un miroir — et il est **postérieur** au jour où on
  constate la disparition, donc hors de portée d'une politique préventive. C'est
  aussi ce que l'ADR 0005-bis devra dire si le cas se présente.
- **La ligne du tableau des risques est à corriger** : « Amont archivé » décrit un
  fait vrai à un cran du mauvais endroit. Le risque à suivre est « le fork
  `Kotatsu-Redo` cesse d'être maintenu », et sa parade est le job planifié, pas un
  miroir.
