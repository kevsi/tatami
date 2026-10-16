# Cloudflare : toutes les pistes envisagées, et ce que chacune a donné

> Document de référence, écrit le 2026-10-16. Chaque ligne porte **la mesure qui
> la tranche**, pas une impression. Les sources de vérité sont
> `PLAN.md` (journal) et `docs/mesures/*.json` (relevés).
>
> Convention de la colonne **Statut** :
> `implémenté` = dans le code et vérifié par une porte ou une mesure ·
> `écarté` = une mesure l'a infirmé ·
> `non testé` = jamais essayé, et c'est dit · `écarté (volontaire)` = approche
> volontairement non retenue, avec la raison.

---

## 1. Le client qui parle au site

| piste | ce que c'est | statut | mesure qui tranche | verdict |
|---|---|---|---|---|
| Requête directe par le moteur | OkHttp + Conscrypt, sans navigateur | implémenté | catalogues lus sans défi sur les zones permissives | **nécessaire, insuffisant** : sur une zone agressive, `403` + `cf-mitigated: challenge` |
| Résolution DNS par le système | `getaddrinfo` | **écarté** | `mangas-origines.fr` : « nom DNS inexistant » 3 fois de suite, et `curl`/Node en `http=000` — alors que le moteur lit la source (`OK`) | le résolveur du système **ment** pour notre pile ; le moteur résout en **DNS-over-HTTPS** (`DoHManager`) |
| Client « navigateur furtif » | JA3/JA4, `navigator.webdriver`, client TLS modifié | **sans objet** | Edge **réel** et `curl` rendent **la même page** (28 860 o contre 5 433 o) | l'axe est **déjà maximal** — notre transport est un vrai Chromium — donc il n'explique rien |
| Source personnalisée | parseur JS (Nashorn) + regex | implémenté | `custom:hhentai-fr` 12/90/6 pages, `bananascans` 5/3/190, `lire-manga` 43/1/258, toutes `image/jpeg` | **fonctionne** — mais elle est **en aval** du refus : sur hentaizone elle rend 0 résultat |
| Alignement UA webview → moteur | la clearance n'est honorée que pour l'agent exact qui l'a obtenue | implémenté | `challenge.rs` lit `navigator.userAgent` et l'impose au rejeu | **nécessaire**, en place |
| Client Hints haute entropie | `fullVersionList`, `platformVersion`, `architecture`, `bitness`, `model`, `languages` | implémenté, **au-delà de la référence** | la référence ne lit que l'UA et se contente de *bloquer* trois en-têtes | en place |
| Reconstruction `sec-ch-ua*`, `Sec-Fetch-*`, `Accept-Language` | `ChallengeHeaders.kt` retire puis reconstruit | implémenté | un `curl` naïf reçoit `403` de mangas-origines, notre moteur non | en place, et **démontré utile** |
| Transfert des cookies webview → moteur | `setCookies` | implémenté | 495 cookies poussés, et le rejeu est **refusé quand même** | en place, **insuffisant seul** |

## 2. La fenêtre qui exécute le défi

| piste | ce que c'est | statut | mesure qui tranche | verdict |
|---|---|---|---|---|
| Fenêtre de transport **détachée** | `.visible(false)` : ni surface, ni mise en page, ni focus — l'état actuel | implémenté | défi figé 76 s, 0 lien manga | à l'écran mais non rendu ; **c'est l'écart à explorer** |
| Fenêtre **attachée hors écran** + `.focused(true)` | même fenêtre, positionnée à (−20000, −20000) | **essayé, expérience invalide** | `is_focused()` = `false` **3/3** pendant que `document.hasFocus()` = `true` **26/26** — Windows **n'active pas** une fenêtre hors écran | prouve une chose utile : hors écran, la fenêtre **reste attachée avec une vraie viewport** (`1024x768`). Ne dit **rien** sur l'attachement |
| **Build sans le port CDP** | le lanceur de mesure pose `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9223` ; le build livré n'a pas ce drapeau | **écarté, mesuré** | relance **sans** le drapeau : `http=000` sur le port 9223 (donc l'absence est confirmée), et HENTAIZONE aboutit au même état final — `latest` → `CHALLENGE_REQUIRED`, puis `page de verification refusee`, puis `transport: … CHALLENGE_NOT_RESOLVED`. Identique au build instrumenté | le drapeau **n'est pas** ce qui empêche le défi de se résoudre. Attention : HENTAIZONE est bloquée au niveau de l'IP, donc elle ne peut aboutir dans **aucun** des deux builds — cette cellule ne pouvait rien prouver, et il fallait TOONFR |
| Bibliothèque « navigateur furtif » (`nodriver`, undetected-chromedriver) | supprimer la couture WebDriver, CDP direct sur un Chrome non modifié | **sans objet** | Tatami n'a pas de couture : sa webview est celle de l'application, elle navigue elle-même — ni WebDriver, ni binaire, ni drapeau d'automatisation | coût (Python + un second Chrome) sans bénéfice mesuré, et **AGPL-3.0** : décision de licence, donc humaine |
| Fenêtre **visible** + clic humain | la voie que propose l'application (« Résoudre ») | implémenté | 495 cookies, refus persistant sur hentaizone | seule voie qui marche **quand le défi est soluble**, mais non automatisable |
| Recréation à profil conservé | recréer le résolveur une fois en gardant les cookies (préchauffage) | non porté | `CloudFlareActivity.kt:329`, `MAX_AUTO_RECREATE_COUNT = 1` | non porté, non testé |
| Révélation sur place | la fenêtre cachée **devient** visible après 15 s, au lieu d'en ouvrir une seconde | non porté | `CloudFlareActivity.kt:340` | non porté — aujourd'hui **2 webviews restent ouvertes** après un échec |
| Détecteur d'état de page façon référence | géométrie des nœuds `#turnstile-wrapper`, `#cf-please-wait`, `opacity`, `getBoundingClientRect` | **retiré par la mesure** | l'interstitielle de hentaizone **n'a aucun widget** : `{"div":2,"script":1}`, **0 iframe**, `readyState: complete`, script du challenge `display:none` 0×0 | **inutile ici** — il n'y a pas de « widget qui tourne » à détecter, et le test par titre dit déjà la bonne chose |
| Détecteur « vérification inefficace » | vérification réussie **et** toujours protégé → cooldown, arrêter de boucler | non porté | `CaptchaAutoResolveCoordinator.kt:128` | non porté ; c'est la pièce qui empêche une boucle de fenêtres |

## 3. Le réseau

| piste | ce que c'est | statut | mesure qui tranche | verdict |
|---|---|---|---|---|
| **Adresse de sortie actuelle** | `154.72.113.241`, **AS36924 GVA Cote d'Ivoire SAS**, Cotonou (BJ), bordure Cloudflare **JNB** | mesuré | 18 sources en `523` et **14 en `403` + marqueur** sur les 35 concernées | **cause la plus probable** du déclenchement des défis : hébergement, pas résidentiel |
| **Autre réseau** | partage de connexion mobile, 2ᵉ box | **non testé** — sonde prête | `node scripts/verifier-egress.mjs` → code `0` lisible, `2` ciblé, `3` indéterminé | **le test qui tranche.** Si le code passe à 0, Tatami n'a **rien** à changer et les 35 sources redeviennent lisibles |
| VPN (tunnel système) | IP d'un fournisseur, partagée | non testé | — | probablement **aucun gain** : c'est *encore* une IP de datacenter, souvent plus marquée. Condition : le tunnel doit être **système**, sinon l'engine et la webview sortent par deux IP différentes |
| Proxy résidentiel tiers | la connexion de gens qui l'ignorent | **écarté (volontaire)** | — | ce n'est pas une amélioration de Tatami mais un contournement décidé par un tiers |
| IP d'origine / en-tête Origin Pull | contourner la configuration du site | **écarté (volontaire)** | — | idem : c'est une attaque contre l'accès d'un tiers, pas une fonctionnalité |

## 4. Le produit

| piste | ce que c'est | statut | mesure qui tranche | verdict |
|---|---|---|---|---|
| Marquage des sources inutilisables | signaler dans le catalogue les zones qui ne répondront pas d'ici | **non fait**, décision humaine | le census des 35 est prêt (`docs/mesures/cloudflare-depuis-cette-machine-2026-10-16.json`) | visible et réversible en un clic ; **à décider** |
| Suppression des webviews après échec | la fenêtre de transport reste ouverte après un abandon | non fait | 2 webviews ouvertes après l'échec mesuré | correctif de propreté, sans effet sur le blocage |
| Critère « un chapitre affiché sur une source Cloudflare » | critère de sortie du spike | **bloqué** | nécessite un **clic humain** dans la fenêtre de vérification | non atteignable par une mesure automatique |

---

## CE QUI MARCHE — mesuré le 2026-10-16 à 12 h 15 (HENTAIZONE)

La source qui a résisté **toute la journée** devient lisible, et l'utilisateur
charge des webtoons. Rien n'a été codé pour cela : le binaire est celui du dépôt,
sans le drapeau CDP, et **aucune fenêtre de vérification n'a été ouverte** dans
cette instance.

| mesure | valeur |
|---|---|
| lectures de transport réussies | **53** |
| fenêtres de vérification ouvertes | **0** |
| catalogue | `latest source=HENTAIZONE offset=96` — pagination qui marche |
| chapitres | 149 177 à 184 330 octets |
| pages | 169 166 octets |
| ce que la webview lit | 158 757 à 169 899 octets — **du contenu, pas un interstitiel** (28 7xx) |
| requêtes OkHttp brutes sur le même hôte | **60 × `403`** et **35 × `200`**, bocal à cookies **vide** |

Deux lectures, et elles ne se contredisent pas :

1. **La webview de transport est devenue la voie qui marche.** Elle lit la vraie
   page, le moteur l'injecte (`webview html injecte … totalMemoire=N`), et
   l'interface affiche. C'est 53 fois, pas un accident.
2. **Le chemin OkHttp du moteur, lui, échoue toujours** — 60 `403` contre 35 `200`,
   bocal vide. Donc la clearance n'est **pas** honorée pour les requêtes
   directes du moteur : le transport n'est pas un filet de sécurité, il est
   devenu **porteur**. Toute source qui exige le transport dépend de lui
   entièrement.

**Ce qui n'est pas attribué, et qui ne doit pas l'être.** Le journal ne dit pas
*pourquoi* la webview est passée de 28 ko à 161 ko. L'hypothèse la plus solide
est un **profil partagé WebView2 réchauffé** par la résolution manuelle de
l'instance précédente (11:06–11:07 : fenêtre ouverte, 8 cookies lus et appliqués,
puis `CHALLENGE_UNREPLAYABLE`). C'est exactement l'idée de la référence — recréer
en conservant le profil du navigateur (`MAX_AUTO_RECREATE_COUNT`) — atteinte ici
par accident. **À prouver ou à renverser, et le test est d'un redémarrage** :
relancer l'application et regarder si la webview relit 161 ko ou retombe à 28 ko.
Tant que ce test n'est pas fait, on ne sait pas si ce que nous avons est
**durable** ou **emprunté**.

## Ce qui reste vrai, en une phrase

**Notre client n'est pas en cause** — il passe là où un `curl` naïf est
refusé, et les zones permissives nous laissent passer sans défi. Ce qui bloquait
n'était pas notre client : c'était que **la webview de transport rendait
l'interstitielle au lieu de la page** (28 ko), et elle rend la page maintenant
(161 ko). Le reste des hypothèses — drapeau CDP, attachement de la fenêtre,
méthode « Cloudflare » — a été mesuré et n'était pas la cause.

## Les trois fautes de mesure à ne pas refaire

1. **Une taille de page ne dit pas l'état.** hentaizone et TOONFR servaient
   toutes deux ~28,7 ko ; j'en ai conclu « le même état ». Faux : TOONFR avait un
   **iframe Turnstile vivant**, hentaizone aucun.
2. **Un domaine ne s'invente pas.** Trois domaines devinés pour MANGASORIGINES
   (`mangasorigines.fr`, `www.mangasorigines.fr`, `mangasoriginesunofficial.com`)
   avant de lire le bon : **`mangas-origines.fr`**.
3. **Un `hasFocus()` à `true` ne prouve pas qu'une fenêtre est active.**
   L'instrument qui interroge l'OS (`is_focused()`) a raison ; celui de la page
   peut être périmé.