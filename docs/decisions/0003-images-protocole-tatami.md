# 0003 — Images de pages : protocole d'URI personnalisé `tatami://`, cache disque Rust
- Date : 2026-10-04 (consignée après coup ; le code applique déjà cette décision depuis la tâche 0.10)
- Statut : acceptée
> Chemins cités : `apps/desktop/src-tauri/src/pagecache.rs`, `.../main.rs`, `.../commands.rs`,
> `.../download.rs`, `apps/desktop/src-tauri/tauri.conf.json`, `apps/desktop/app/page.tsx`,
> `docs/notes/loader-context.md`, `docs/decisions/0001-transport-sidecar.md`.
## Contexte
`AGENTS.md` §4.4 est explicite : les images passent par le moteur (en-têtes, cookies et
recomposition propres à chaque source), sont mises en cache sur disque par Rust, et sont servies
à la webview via un protocole d'URI personnalisé. **Pas d'appel direct aux hôtes d'images
depuis la webview.** `AGENTS.md` §7 (Tauri) ajoute : le protocole n'accepte que des identifiants
connus, jamais une URL arbitraire fournie par l'interface.

Pourquoi cette contrainte n'est pas négociable, avec des faits mesurés :

- **Le moteur sait des choses que la webview ne sait pas.** Il tient le `CookieJar` de la source,
  le user-agent par défaut (`docs/notes/loader-context.md:33-38`) et sait recomposer une image
  embrouillée (`redrawImageResponse`, `docs/notes/loader-context.md:39`).
- **Le CDN refuse l'user-agent du moteur.** Mesuré : REMANGA et SEIMANGA répondent à `pages`
  mais `fetchImage` renvoie **HTTP 403** (`img.reimg2.org`), le lecteur reste vide
  (`PLAN.md`, journal 2026-10-03, tâche 0.13-0.17).
- **Une source peut exposer des URL d'images en clair alors que le site refuse le hotlinking.**
  Le seul client légitime est celui qui rejoue la session de la source.

## Décision
**Un jeton opaque, un cache disque, un protocole `tatami://` asynchrone.**

1. **Le jeton est ce que l'interface utilise.** La commande `pages` parcourt la réponse du moteur
   et, pour chaque `page.url`, appelle `cache.register(source, url)` puis **ajoute** un champ
   `token` dans l'objet renvoyé à l'interface (`commands.rs:240-248`). Le champ `url` reste donc
   présent dans la réponse, mais l'interface ne s'en sert jamais : elle ne construit que des URL
   `tatami://` à partir du jeton (`page.tsx:1409-1412`, `page.tsx:2197`, `page.tsx:2215`).
   *Honnêteté sur la surface :* l'URL est transmise mais inutilisée ; la retirer du JSON serait un
   changement de contrat d'interface à faire à part. Ce qui protège réellement est le point 3.
2. **Le jeton est déterministe et unique par couple (source, url)** : 32 caractères hexadécimaux
   produits par deux hachages distincts, et `register` **déduplique** — un même couple renvoie
   toujours le même jeton (`pagecache.rs:33-53`).
3. **Résolution par table, pas par URL.** `cache.resolve(token)` ne retrouve qu'un `PageRef`
   (`source` + `page_url`) enregistré par le processus ; un jeton inconnu, un chemin qui ne
   commence pas par `page/`, ou une erreur moteur → **404** (`main.rs:68-88`). Il n'existe donc
   aucun chemin du protocole qui accepte une URL fournie par l'interface.
4. **Cache disque : `%APPDATA%\Tatami\cache\page-<token>.bin`**, créé au démarrage
   (`main.rs:33-38`, `pagecache.rs:63-65`). Un fichier non vide est renvoyé directement, sans
   appel moteur (`pagecache.rs:93-100`).
5. **Cache miss = `fetchImage`**, délai 120 s, `data` base64 décodée puis écrite sur disque avant
   d'être renvoyée (`pagecache.rs:101-125`).
6. **Type de contenu deviné sur les octets**, pas sur une extension ni sur un en-tête fourni :
   JPEG `FF D8`, PNG `89 50 4E 47`, WebP `RIFF….WEBP`, sinon `application/octet-stream`
   (`pagecache.rs:128-138`).
7. **Côté interface**, l'URL est construite en une seule fonction, `pageSrc(token)` →
   `http://tatami.localhost/page/<token>` (`page.tsx:1409-1412`). Le nom d'hôte `tatami.localhost`
   est la forme sous laquelle Windows atteint un protocole d'URI personnalisé enregistré.
8. **La CSP l'autorise explicitement et rien de plus** :
   `img-src 'self' https: data: http://tatami.localhost asset: http://asset.localhost`
   (`tauri.conf.json:21`).
9. **Le même chemin sert le téléchargement** : `download.rs:267-305` appelle `fetchImage` en
   direct et écrit la page dans le `.cbz`. Il ne passe pas par le protocole, parce qu'un `.cbz` n'a
   rien à afficher dans la webview.

## Alternatives étudiées
- **Servir directement l'URL d'image du site à la webview** : le plus simple possible, et
  interdit. Le hotlinking est refusé (403 mesuré), les cookies de source ne partiraient pas, et
  la CSP `img-src` devrait s'ouvrir à n'importe quel hôte. Rejeté.
- **`asset:` / `convertFileSrc` de Tauri** : convient à des fichiers déjà sur disque, pas à un
  contenu qui doit être **demandé au moteur à la demande** (cache miss = requête réseau). De plus,
  `convertFileSrc` construit une URL à partir d'un chemin : on réintroduirait la surface
  « identifiant arbitraire » que la règle 3 supprime. Rejeté.
- **Base64 dans la réponse JSON de `pages`** : aucune requête réseau, mais `pages` deviendrait
  énorme (toutes les pages d'un chapitre en mémoire avant le premier rendu) et la base dupliquée
  entre Rust et l'interface. Rejeté.
- **HTTP local avec jeton (`127.0.0.1:PORT`)** : c'est la solution standard, mais elle rouvre un
  port, donc une surface d'attaque et un conflit de ports, et elle a été écartée pour le transport
  Rust ↔ moteur (`docs/decisions/0001-transport-sidecar.md`). L'utiliser seulement pour les
  images créerait deux mécanismes pour le même problème. Rejeté.
- **Un handler synchrone au lieu d'asynchrone** : le handler est asynchrone
  (`register_asynchronous_uri_scheme_protocol`, `main.rs:64`) parce que le chemin touche le
  moteur. Un handler synchrone bloquerait le thread qui sert les requêtes de la webview pendant
  une requête réseau de plusieurs secondes. Rejeté.

## Conséquences
- **Le cache n'est pas borné.** `%APPDATA%\Tatami\cache` accumule un `page-<token>.bin` par page
  déjà vue, sans expiration ni quota. Le cache n'est jamais vidé au démarrage
  (`pagecache.rs:24-31`). Conséquence immédiate : de la place disque consommée sans borne, et des
  fichiers dont plus personne ne connaît la provenance. **À traiter** : un quota ou un
  ramasse-miettes au démarrage, ou au minimum un plafond par webtoon. Non fait à ce jour —
  `PLAN.md` 2.5 (« gestion de l'espace ») est le rendez-vous.
- **Le cache survit au redémarrage, la table des jetons non.** `PageCache::map` vit en mémoire
  (`pagecache.rs:19-22`) et est reconstruite à chaque `pages`. Un jeton d'une session précédente
  renvoie donc 404 même si le fichier `.bin` est encore là (`main.rs:73-76`) — le cache disque
  n'est atteignable que par un jeton vivant. Ce n'est pas un défaut aujourd'hui (l'interface
  redemande `pages`), mais un `openReaderAt` depuis un signet ou l'historique passe bien par un
  `pages` : si un jour un chemin rouvre le lecteur **sans** rappeler `pages`, le cache disque
  devient inatteignable et l'image ne s'affiche pas.
- **Les 404 sont le seul signal observable côté interface.** Le protocole répond 404 sur jeton
  inconnu, chemin inconnu et erreur moteur (`main.rs:69-88`) ; l'erreur elle-même n'est
  journalisée que dans les logs Rust (`main.rs:85`). Conséquence pour le test de fumée : une
  image absente se diagnose par les logs, pas par l'écran.
- **Le type de contenu est deviné, donc un format non couvert devient illisible.** Un GIF ou un
  AVIF passe en `application/octet-stream` et le navigateur peut refuser de l'afficher
  (`pagecache.rs:135-137`). Le moteur peut renvoyer du WebP, couvert ; GIF/AVIF non vérifiés.
- **Le cache et le téléchargement font deux fois le même travail.** Un chapitre téléchargé puis
  relu n'est pas lu depuis le `.cbz` : il est relu via `fetchImage`, parce que la lecture hors
  ligne n'existe pas encore (`PLAN.md` 2.3 non coché). À brancher quand 2.3 sera fait.
- **Le `base64` dans `fetchImage` reste le point de tension** : une page très grande gonfle le
  message et la mémoire. Le repli envisagé est un passage à des fichiers temporaires, ce qui est
  un **breaking change du contrat** et exige donc une nouvelle ADR (`docs/decisions/0001-transport-sidecar.md`,
  § Conséquences). Seuil à surveiller : quelques mégaoctets par page au pire, mesure 0.6 : une
  image de 2 393 492 octets renvoyée en `image/jpeg`.
- **CSP** : `img-src` autorise encore `https:` et `data:` (`tauri.conf.json:21`) à cause des
  **couvertures**, qui sont des URL distantes du site et ne passent pas par le moteur. Ce n'est
  pas le même chemin que les pages : si les couvertures doivent un jour passer par `tatami://`,
  `https:` pourra être retiré de `img-src` — à traiter dans la même ADR que le durcissement CSP.
