# De l'affichage du catalogue à la lecture d'un chapitre

> Document de travail du 2026-10-14. Il explique **par où passent les requêtes**
> et, surtout, **comment distinguer un site mort d'un parseur cassé** — la
> question à laquelle on répondait depuis des heures sans savoir.

---

## 1. Le parcours, en cinq étapes

Chaque étape est **une commande de l'application** qui va chercher des données.

| étape | ce que l'utilisateur fait | commande | qui parle au site |
|---|---|---|---|
| 1 | ouvre l'onglet **Catalogues** | `listSources` | personne — le moteur lit son JAR |
| 2 | choisit une langue, puis une source | `latest` | **le moteur**, en HTTP |
| 3 | clique une série | `details` puis `chapters` | **le moteur**, en HTTP |
| 4 | ouvre un chapitre | `pages` | **le moteur**, en HTTP |
| 5 | fait défiler | *aucune commande* | **Rust** télécharge les images et les sert depuis son cache |

Deux choses à retenir :

- **L'étape 1 ne touche pas Internet.** Les 1380 sources viennent d'un fichier
  (`kotatsu-parsers-redo`). Un site mort reste donc visible dans le catalogue,
  pour toujours, jusqu'à ce qu'on le retire à la main. C'est la première raison
  de ce document.
- **L'étape 5 n'est pas une commande.** Les images ne passent pas par le
  dialogue application ↔ moteur : Rust les télécharge, les met en cache sur
  disque, et l'interface les affiche. Un problème d'image est donc un problème
  Rust, même si la cause est un site.

---

## 2. Comment une requête arrive au site

```
Interface (page.tsx)
   │  invoke('latest', …)
   ▼
Cœur Rust (supervisor.rs)
   │  une ligne NDJSON sur stdin/stdout
   ▼
Moteur JVM (Server.kt)          ← les parseurs sont des .class dans un JAR
   │  OkHttp
   ▼
Le site
```

Le **parseur** est le code qui sait où sont les mangas sur *ce* site : il construit
des URL et du HTML à analyser. Il vient de `kotatsu-parsers-redo`, un projet
amont que **nous ne contrôlons pas**.

C'est la source de presque tous nos problèmes : un parseur peut demander une URL
qui n'existe plus, ou une API qui ne rend plus rien. Aucun client ne peut alors
arranger ça.

---

## 3. Le triangle du diagnostic

Quand une source ne marche pas, il y a **trois causes possibles**, et les
confondre fait perdre des journées. La bonne question n'est jamais « est-ce que
ça marche ? » mais **« qui me l'a dit, et est-ce que ce producteur répond ? »**.

### A. Le site est mort — aucun client ne peut le lire

**Signe :** un vrai navigateur reçoit `523 Origin is unreachable` ou
`522 Connection timed out`. Cloudflare répond, mais le serveur derrière est
éteint ou ne répond plus.

**Test :** ouvrir l'URL dans un navigateur. Si l'on voit une page Cloudflare
« 523 », le site est mort.

**Ce qu'on ne peut pas faire :** rien côté client. Retirer du catalogue, avec la
date du relevé.

### B. Le parseur demande la mauvaise page — le site va très bien

**Signe :** un vrai navigateur obtient du contenu, mais le moteur ne le demande
pas. Typiquement : le parseur garde une URL par défaut qui a changé, ou il
interroge une API interne qui rend désormais 0 octet.

**Test :** ouvrir dans un navigateur **l'URL que le moteur a réellement
demandée** — pas celle du site, celle du journal.

**Ce qu'on peut faire :** écrire une source personnalisée qui interroge la bonne
page. Ou attendre une correction amont.

### C. Le client est refusé — le site et le parseur sont bons

**Signe :** le navigateur obtient 200, le moteur obtient 403. C'est Cloudflare qui
fait la différence.

**Test :** même URL, deux clients.

**Ce qu'on peut faire :** c'est exactement ce que fait le transport webview
(§4).

### La grille de décision

| le navigateur | le moteur | conclusion | qui agit |
|---|---|---|---|
| 52x / mort | 52x | **A — site mort** | catalogue |
| contenu | refus sur une **autre** URL | **B — parseur cassé** | source personnalisée / amont |
| contenu | 403 sur la **même** URL | **C — client refusé** | transport webview |

La troisième colonne est tout. Toute la difficulté du travail vient d'avoir
confondu B et C — et d'avoir mesuré l'URL du site au lieu de l'URL du moteur.

---

## 4. Le transport webview, et sa limite honnête

Quand on est en C, on peut faire lire la page par un vrai navigateur intégré à
l'application, puis servir le résultat au moteur.

```
le cœur reçoit un CHALLENGE_REQUIRED
   │  une seule fois par URL
   ▼
une webview **cachée** ouvre l'URL, résout le challenge, rend le corps
   │  on relit l'URL **par fetch**, pour obtenir la réponse brute
   ▼
le corps est injecté dans la mémoire du moteur
   │
   ▼
la commande est rejouée : elle trouve la réponse en mémoire, sans réseau
```

**Ce que ça règle :** les cas C. Mesuré le 2026-10-14 sur HENTAIZONE, le
navigateur intégré obtient 158 285 octets là où OkHttp obtient 403.

**Ce que ça ne règle pas :** les cas A et B. Un site mort reste mort, et un
parseur qui demande la mauvaise URL demandera toujours la mauvaise URL — le
transport lit très bien une page vide.

C'est pour cela que **les deux mesures doivent être croisées** : le verdict du
navigateur seul ne dit pas si la source est utilisable, et le verdict du moteur
seul ne dit pas si le site est en faute.

---

## 5. Ce que l'audit doit mesurer

Pour trancher, il faut **deux mesures par source, le même jour** :

1. **Côté moteur** — le parcours complet `latest` → `details` → `chapters` →
   `pages` → image. C'est `scripts/sonde-sources.mjs --deep`.
2. **Côté navigateur** — l'URL que le moteur a réellement demandée, ouverte dans
   un vrai navigateur. C'est `scripts/sonde-navigateur.mjs`.

Le croisement des deux donne la colonne « conclusion » de la grille ci-dessus.

### Deux règles de mesure, apprises aujourd'hui

- **Un `200` ne prouve rien.** Une page peut répondre 200 et rendre 0 octet
  (l'API de TOONFR). On compte donc les liens, jamais seulement le statut.
- **Une URL est une valeur reçue.** Avant de juger un hôte, il faut demander au
  **producteur** — ici le bytecode du parseur. EPSILONSOFT illustre l'erreur :
  le parseur interroge `beta.epsilonsoft.to`, nous avions mesuré l'apex
  `epsilonsoft.to`, qui sert une fausse mise à jour de navigateur.

---

## 6. Le résultat, en une phrase par source

Le fichier normalisé est `docs/mesures/sites-etat-normalise.json`. Chaque source
y porte :

- `conclusion` : `site_mort` / `parseur_casse` / `client_refuse` / `ok` / `a_verifier`
- `preuve_moteur` : le verdict brut du parcours
- `preuve_navigateur` : statut, taille, nombre de liens
- `url_demandee` : **l'URL que le moteur a demandée**, pas celle du site
- `action` : ce qu'il reste à faire, et qui le fait

Une source sans `preuve_navigateur` n'est **pas** classée : on ne conclut pas
« morte » sur la seule parole du moteur, pas plus que l'inverse.