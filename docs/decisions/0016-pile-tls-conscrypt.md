# 0016 — Pile TLS du moteur : Conscrypt, alignée sur Kotatsu

- Date : 2026-10-11
- Statut : acceptée
- Décideur : humain (choix explicite le 2026-10-11, après présentation du résultat)

## Contexte

Le moteur JVM fait ses requêtes par OkHttp. Sur Android — où tourne Kotatsu —
le provider TLS **par défaut de la plateforme** est Conscrypt, la bibliothèque de
Google, celle de Chrome. Notre moteur tourne sur un **JVM de bureau** (jlink,
Temurin 21) : le provider par défaut y est **JSSE**, la pile du JDK.

Le sujet est venu de `hentaizone.xyz`, qui refuse le rejeu d'une clearance
Cloudflare (`403 cf-mitigated: challenge`) même quand la requête est par ailleurs
identique à celle d'un navigateur. Neuf causes ont été éliminées par la mesure,
dont la pile TLS était la dernière expérience connue.

Référence, lue et non devinée : `Kotatsu-Redo/.../core/BaseApp.kt:88` fait
`Security.insertProviderAt(Conscrypt.newProvider(), 1)`, mais **sous**
`if (Build.VERSION.SDK_INT < Build.VERSION_CODES.Q)`. Ce n'est donc pas un choix
de leur part, c'est un **rattrapage** pour les Android trop vieux : sur un
Android récent, Conscrypt est déjà là sans rien écrire.

## Décision

Le moteur pose **Conscrypt 2.5.2** (`org.conscrypt:conscrypt-openjdk-uber`) sur
son client OkHttp, avec repli silencieux sur JSSE si la construction échoue.

Choix de forme, et pourquoi :

- **`openjdk-uber`, pas `conscrypt-android`.** La variante `android` suppose
  l'environnement Android ; c'est `openjdk-uber` qui embarque le code natif
  BoringSSL pour une JVM de bureau. La version (2.5.2) est celle du
  `libs.versions.toml` de Kotatsu-Redo.
- **`.sslSocketFactory(...)` explicite, pas `Security.insertProviderAt`.**
  Insérer un provider ne change pas l'`SSLContext` que JSSE a déjà choisi. De
  plus, une pose explicite est **vérifiable** : elle se lit dans
  `client.sslSocketFactory`. Une insertion globale serait un état invisible.
- **Le repli est une obligation, pas une commodité.** Un moteur qui démarre avec
  la pile d'hier vaut mieux qu'un moteur qui ne démarre pas. Le repli est
  journalisé.

Contournement nécessaire, mesuré et non deviné : Conscrypt appelle le gestionnaire
de confiance avec `authType = "GENERIC"`, que sa propre implémentation refuse.
Sans le rejouer sous `"RSA"`, la poignée de main échoue sur une chaîne de
certificats parfaitement valide. Sans cela, on lirait « TLS cassé sous Conscrypt »
alors que c'est nous qui l'avons cassé.

## Ce que la mesure dit — et ce qu'elle ne dit pas

Mesuré sur le client **de production** (`EngineContext.httpClient`), pas sur une
sonde parallèle :

| | Avant (JSSE) | Après (Conscrypt) |
|---|---|---|
| JA3 | `0a80f686…` | `4944639f9df167507df68fbe89589dbe` |
| GREASE | absent | `156-157-47-53` — comme Edge |

Conscrypt rapproche fortement l'empreinte de celle de Chrome. **La validation de
chaîne reste active** : `self-signed.badssl.com` échoue toujours en
`CertPathValidatorException: Trust anchor … not found`. On n'a pas acheté Conscrypt
en éteignant une garantie (AGENTS.md §10).

**Et pourtant, `hentaizone.xyz` refuse toujours le rejeu**, avec `cf_clearance`
dans le Jar et l'UA de la fenêtre qui a résolu. **Neuvième cause éliminée.** Le
refus n'est pas un problème de signature TLS ; il reste l'écart déjà mesuré sur
l'**ordre des extensions TLS** et l'empreinte **HTTP/2 d'OkHttp**, qu'un
`OkHttpClient` ne peut pas rendre pareille.

## Alternatives étudiées

1. **Rester en JSSE.** Rejeté par le humain : la pile de Chrome est la
   référence à laquelle Kotatsu doit son succès, et l'abandonner après l'avoir
   mesurée fait perdre l'alignement avec la référence. Le coût assumé est
   explicite ci-dessous.

2. **Prolonger l'imitation d'OkHttp jusqu'au bout** (ordre des extensions,
   table SETTINGS, GREASE) — c'est-à-dire réécrire la couche HTTP/2 pour suivre
   une empreinte. Rejeté : c'est un piel d'empreinte contre une défense active,
   cela casse toute mise à jour d'OkHttp, et cela ne résout pas la question
   structurelle posée plus bas.

3. **Faire passer les requêtes par la WebView** (option B de `PLAN.md`). Rejeté
   en attente : plusieurs jours, une webview par requête, le cœur Rust perd la
   maîtrise du réseau, et c'est un **changement d'architecture** qui ne se
   décide pas dans une ADR de pile TLS. Reste ouverte, et devra être proposée
   séparément si elle est un jour justifiée.

## Conséquences

- **La distribution grossit de 4,5 Mo** (le JAR `uber` embarque BoringSSL).
- **Chaque graduation du moteur** doit régénérer `gradle/verification-metadata.xml`
  (`--write-verification-metadata sha256 --refresh-dependencies`). C'est une
  porte de sécurité, pas une formalité : sans cela la CI échoue sur un cache
  froid alors que le poste de développement passe.
- **99 sources sur 1374** sont derrière un challenge Cloudflare (`PLAN.md`,
  option C). Cette décision ne les débloque pas et ne les gêne pas davantage.
  Elle est **neutre** sur ce front.
- **La dépendance est un point de maintenance** : Conscrypt suit Chrome, et une
  divergence future de TLS serait un comportement, pas un bug.

## Point ouvert, à trancher plus tard

Le test sur téléphone (Kotatsu, même Wi-Fi, cookies de la source effacés) dira
si `hentaizone.xyz` refuse **tous** les clients non-navigateur ou seulement les
notres. Tant qu'il n'a pas été joué, la phrase honnête sur ce dossier est : *le
refus n'est pas expliqué*. Elle ne doit pas être écrite comme « le site bloque
notre application » — `mangas-origines.fr` et `lelscanfr.com`, sur le même
Cloudflare, répondent 200 dans la même trace.