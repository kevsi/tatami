# Corpus d'évaluation T0.2 — description (aucune image)

> Écrit le 2026-10-06. Le corpus vit **hors du dépôt**
> (`…\Workspaces\Tatami-eval\corpus`, protégé par `.gitignore`). Ce fichier est
> la seule trace versionnée : il décrit, il ne montre pas. `AGENTS.md` §2.7 :
> aucun contenu protégé dans le dépôt.

## Contenu

Trois séries, **trois chapitres** (un par série), **36 pages** au total
(12 échantillonnées par chapitre, à pas régulier) :

| Dossier | Source | Série | Chapitre | Pages du chapitre | Taille moyenne | Format |
|---|---|---|---|---|---|---|
| `1-webtoonsen` | WEBTOONS_EN | *Daytime in the Bunker* | 1 | 167 | 78 Ko | JPEG, bandes verticales |
| `2-webtoonsen` | WEBTOONS_EN | *I Love Yoo* | 1 | 108 | 51 Ko | JPEG, bandes verticales |
| `3-webtoonsen` | WEBTOONS_EN | *Like Mother, Like Daughter* | 1 | 166 | ~60 Ko | JPEG, bandes verticales |

Vérification d'intégrité (`verif-corpus.py`) : chaque image s'ouvre et se
décode (PIL `verify()` **et** `load()`), **0 illisible**, et les tailles en
octets sont **distinctes** — pas de page dupliquée (leçon §14 : un `.cbz`
valide peut contenir 45 fois la même page HTML).

**La langue est vérifiée par OCR, pas par le titre de la source** : les trois
séries produisent du texte anglais lisible à l'OCR (exemples dans
`docs/spike-traduction.md` §8).

## Ce que ce premier corpus ne couvre pas encore

Le protocole (T0.3) demande ~20 chapitres variés. Ce corpus de 3 chapitres sert
à **valider le protocole** (décision humaine du 2026-10-06), pas à estimer les
métriques finales. Styles absents à ce stade : fonds très sombres avec texte
clair, bulles à cheval sur deux images, cartouches de narration complexes,
polices manuscrites denses.

## Leçon de constitution — la langue d'une source n'est pas une garantie

Quatre échecs mesurés avant d'obtenir ce corpus anglais :

1. **MANGADEX « Solo Leveling: Ragnarok »** → chapitre servi en **coréen**
   (onomatopées en hangul).
2. **MANGADEX « unOrdinary »** → chapitre servi en **hongrois**
   (« Hogy te szólítottad? »).
3. **MANGADEX « Tower of God: Urek Mazino »** → page de **crédits en anglais**
   mais chapitre en **portugais** (« ONDE CARALHOS EU ESTOU? »). Le piège le
   plus trompeur : la première page semblait valider la langue.
4. **SEIMANGA** avec requête vide → **russe** ; **MANGANATO**, **WEBTOONXYZ**,
   **WEBTOONSCAN** → `CHALLENGE_REQUIRED` (Cloudflare).

Conséquence pour l'app elle-même (hors phase T, signalé dans `PLAN.md`) : le
moteur ne choisit pas la langue d'un chapitre MangaDex — il sert le groupe de
traduction que le parseur retourne en premier. Une **vérification par OCR**
d'un échantillon de pages est le seul test fiable de la langue réelle.

Sources EN qui répondent (mesuré le 2026-10-06) : **WEBTOONS_EN**,
**TOOMICSEN**. Toutes les autres essayées sont en échec (HTTP 500/523/525,
SSL, ou Cloudflare).
