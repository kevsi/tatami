# Suivre l'amont : épinglage, vérification, montée de version

> Tâche 5.7. La décision et ses alternatives sont dans
> [`docs/decisions/0005-suivi-amont.md`](../decisions/0005-suivi-amont.md).
> Ce fichier est le **mode d'emploi** : uniquement des commandes.

## Ce dont on dépend, exactement

`engine/build.gradle.kts` :

```kotlin
implementation("com.github.Kotatsu-Redo:kotatsu-parsers-redo:b3c0000")
```

C'est un artefact `com.github.*` résolu par **JitPack**, épinglé sur un **commit**.
Le dépôt amont ne publie pas de tag : il n'y a donc rien d'autre à épingler.

## État mesuré le 2026-10-05

| Dépôt | Archivé | Dernier push |
|---|---|---|
| `KotatsuApp/kotatsu-parsers` (l'original, parent) | **oui** | 2025-11-03 |
| `Kotatsu-Redo/kotatsu-parsers-redo` (**le nôtre**) | non | 2026-09-17 |
| `KotatsuApp/kotatsu` (l'app amont) | oui | 2025-11-04 |

`b3c0000` est **la tête de `master`** de notre fork. Il n'y a rien à monter.

> Le tableau des risques de `PLAN.md` disait « Amont archivé (dépôts KotatsuApp) ».
> C'est vrai **d'un cran trop bas** : le dépôt archivé est le parent, pas celui
> qu'on utilise. La ligne a été corrigée le 2026-10-05.

## Les trois commandes qui comptent

### 1. Quel commit est la tête de l'amont ?

```powershell
curl.exe -s "https://api.github.com/repos/Kotatsu-Redo/kotatsu-parsers-redo/commits?sha=master&per_page=1" |
  Select-String '"sha"'
```

### 2. L'artefact est-il toujours servi ?

```powershell
curl.exe -s -o NUL -w "http=%{http_code}`n" `
  "https://jitpack.io/com/github/Kotatsu-Redo/kotatsu-parsers-redo/b3c0000/kotatsu-parsers-redo-b3c0000.pom"
```

Un `200` est la normale. C'est la **seule** défaillance qui justifierait
réellement un miroir — voir la fin de l'ADR 0005.

### 3. Monter l'épingle — la procédure complète

Une montée de version touche **deux fichiers**, pas un. C'est voulu : un pin seul
n'engage à rien tant que rien ne le vérifie.

```powershell
# (a) Noter la nouvelle tête de master (commande 1 ci-dessus), ex. 1a2b3c4

# (b) Changer le pin dans engine/build.gradle.kts
#     b3c0000 -> 1a2b3c4

# (c) Régénérer les empreintes. --refresh-dependencies est OBLIGATOIRE :
#     sans lui Gradle réutilise le cache et n'inscrit pas les artefacts
#     résolus depuis un autre dépôt — le fichier reste incomplet et la CI
#     échoue ensuite sur une résolution à froid.
cd engine
gradle --write-verification-metadata sha256 --refresh-dependencies build

# (d) Vérifier que la résolution à FROID passe bien, sans le drapeau d'écriture.
#     C'est ce que fait la CI sur un checkout neuf.
gradle clean build --refresh-dependencies

# (e) Prouver que la porte mord : altérer d'un caractère le sha256 du JAR de
#     parsers dans engine/gradle/verification-metadata.xml, puis (d) doit
#     ÉCHOUER en nommant kotatsu-parsers-redo.
```

Après (c), `engine/gradle/verification-metadata.xml` **doit avoir changé**. S'il
n'a pas changé, c'est que (b) n'a rien modifié — donc que le pin ne sert à rien.

## Ce qui fait échouer le build, et pourquoi c'est voulu

```
Dependency verification failed for configuration 'classpath'
  One artifact failed verification: kotlinx-coroutines-bom-1.8.0.pom
```

Trois causes possibles, dans cet ordre de fréquence :

1. **Un artefact nouveau** (nouvelle dépendance, nouveau JDK, nouvelle version de
   Gradle qui change un `.pom`). → régénérer (c).
2. **Le fichier est incomplet pour une résolution à froid.** →
   `--refresh-dependencies` obligatoire à la génération (étape c). C'est ce qui a
   été mesuré le 2026-10-05 : un fichier généré sans ce drapeau passait en local
   grâce au cache, et échouait en résolution à froid.
3. **L'artefact a réellement changé** (JitPack a reconstruit, le miroir a été
   compromis). → **ne pas régénérer**. C'est précisément ce que la porte doit
   attraper.

## La surveillance automatique

`.github/workflows/amont.yml` — job planifié **hebdomadaire** qui :

- compare le pin à la tête de `master` et **échoue** si elle a bougé ;
- échoue si le commit épinglé a disparu (dépôt supprimé ou réécrit) ;
- échoue si JitPack ne sert plus l'artefact (lève le signal du « vrai miroir »).

Il est **volontairement dans un fichier séparé** de `ci.yml`, dont l'en-tête
interdit d'ajouter une commande qui ne figure pas dans AGENTS.md §8.

Une alerte n'est pas une urgence : ce n'est pas une alerte sécurité, c'est une
liste de travail. Le job échoue, quelqu'un regarde, on décide.
