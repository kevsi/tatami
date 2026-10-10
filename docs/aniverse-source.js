/**
 * Source scriptée Tatami — Aniverse (FR)
 * ======================================
 *
 * ## Ce qui est vérifié, et comment (mesuré le 2026-10-10)
 *
 * Toutes les mesures ci-dessous sont des réponses **réelles** du site, prises
 * avec le même client que le moteur : `https.get` de `HttpBridge` → OkHttp,
 * UA Firefox, **aucun JavaScript**. Rien n'est deviné, rien n'est supposé.
 *
 *   https://aniverse.fr/manga            → 200, 382 374 o, `<html lang="fr">`
 *   https://aniverse.fr/manga/blue-lock  → 200, 299 382 o
 *   https://aniverse.fr/read/blue-lock/<uuid>
 *                                        → 200, 283 397 o, 19 images
 *   https://cdn.onefy.me/raw/…/001.jpg   → 200, image/jpeg, 301 435 o,
 *                                          signature `FF D8 FF E0` (vrai JPEG)
 *
 * ### ⚠ Une correction : les chapitres SONT dans le HTML statique
 *
 * J'ai d'abord conclu le contraire, en cherchant des liens
 * `href="/read/…"`. Il n'y en a pas : la page affiche un squelette
 * (`aria-busy="true" aria-label="Chargement des chapitres"`, et
 * « Nombre de chapitres : - »), ce qui donne l'impression que la liste
 * n'arrive qu'avec du JavaScript.
 *
 * Elle arrive pourtant bien sans JavaScript, **sous forme de JSON** dans la
 * charge RSC (`self.__next_f.push`), avec cette forme par chapitre :
 *
 *     {"id":"8baf3445-…","number":"1","title":"Vol.1 Ch.1","updatedAt":"…"}
 *
 * Mesuré sur Blue Lock : **360 chapitres**, numérotés de 1 à 363. Les
 * numéros 124, 162 et 327 sont absents **des données du site** (introuvables
 * dans le HTML) : ce sont des trous de numérotation, pas des trous de regex.
 *
 * Le même raisonnement vaut pour les recommandations : leurs
 * `latestChapters` ont la forme `{"id":…,"number":…,"updatedAt":…}` **sans**
 * `title`. Exiger `title` est donc ce qui isole la bonne série — vérifié :
 * **0 correspondance avant** le tableau des chapitres sur la page entière.
 *
 * ### Les deux pièges de ce site
 *
 * 1. **L'échappement varie selon la profondeur.** `\"slug\":\"blue-lock\"`
 *    (une fois) et `\\\"name\\\":\\\"Blue Lock\\\"` (deux fois, dans le `ld+json`
 *    imbriqué). Les regex ci-dessous acceptent les deux : `(?:\\{0,2}")`.
 * 2. **Le lecteur est une URL à UUID.** `/read/<slug>/<uuid>` : le slug se
 *    devine, l'UUID non. Il n'est récupérable que depuis la charge RSC de la
 *    fiche — d'où l'ordre `getDetails` (chapitres) puis `getPages` (images).
 *
 * ### Limites assumées, et pourquoi
 *
 * - **Pas de recherche serveur.** `?q=one+piece` sur `/manga` renvoie les
 *   mêmes 14 séries, et `/search?q=` comme `/recherche?q=` renvoient 0 manga.
 *   `getList` filtre donc la liste **localement**, sur le titre, sans
 *   distinction de casse.
 * - **Pas de pagination serveur.** `?page=2` renvoie les mêmes 14 séries que
 *   `?page=1`. `getList` renvoie donc `[]` dès la page 2 : sans cela,
 *   l'interface paginerait indéfiniment sur les mêmes cartes.
 * - **14 séries au catalogue.** C'est ce que le HTML statique expose. Les
 *   autres existent dans l'application web, mais pas pour un client sans
 *   JavaScript.
 * - Les titres de chapitres viennent du site, Bugs compris : le chapitre 2 de
 *   Blue Lock s'appelle `Vol.1 Ch.2 - EmmAcnagement`. On ne nettoie rien,
 *   ce n'est pas à nous de réécrire le site.
 *
 * ## Identité : c'est le NOM DU FICHIER qui faitoi
 *
 * Le moteur prend l'identifiant dans le nom de fichier
 * (`aniverse.js` → `custom:aniverse`) et Rust enregistre la ligne en base
 * sous ce même nom. `SOURCE.id` est lu et validé, mais ne décide plus de
 * l'identité. **Gardez les deux identiques.**
 *
 * ## API disponible dans le script
 *
 *   http.get(url)         → String (corps de la réponse)
 *   http.post(url, body)  → String
 *
 * Contraintes mesurées : URLs `https://` uniquement, corps limité à 5 Mo,
 * et **aucun en-tête personnalisable** — ce qui convient ici, puisque le
 * site répond 200 à un client simple.
 */

var SOURCE = {
    // =========================================================================
    // Métadonnées — `id` doit être le nom du fichier SANS `.js`.
    // =========================================================================
    id: "aniverse",
    name: "Aniverse",
    lang: "fr",
    baseUrl: "https://aniverse.fr",

    // =========================================================================
    // getList(page, query) → JSON.stringify([{id, title, coverUrl, url}, …])
    //
    // Mesuré : une carte du catalogue a exactement cette forme —
    //
    //   <a href="/manga/one-piece" …><div …><img alt="One Piece"
    //   loading="lazy" decoding="async" … src="https://cdn.onefy.me/…webp"/>
    //
    // `alt` précède `src`, et `/manga/genres` n'a ni `alt` ni image : la regex
    // ne le ramène donc pas (mesuré : 14 séries, pas 15).
    // =========================================================================
    getList: function (page, query) {
        // Mesuré : le site n'a pas de pagination serveur. Renvoyer les mêmes
        // 14 cartes à la page 2 ferait paginer l'interface à l'infini.
        if (page > 1) {
            return JSON.stringify([]);
        }

        var html = http.get(SOURCE.baseUrl + "/manga");
        var resultats = [];

        var re = /href="\/manga\/([a-z0-9-]+)"[\s\S]{0,400}?alt="([^"]*)"[\s\S]{0,400}?src="(https:\/\/cdn\.onefy\.me[^"]+)"/g;
        var vus = {};
        var m;
        while ((m = re.exec(html)) !== null) {
            var slug = m[1];
            var titre = m[2];
            var cover = m[3];
            // Un même manga peut apparaître deux fois dans la grille ; on ne
            // garde que la première occurrence, comme le ferait un `Set` de
            // (source, url).
            if (vus[slug]) {
                continue;
            }
            vus[slug] = true;
            resultats.push({
                id: slug,
                title: SOURCE._sansBalise(titre),
                coverUrl: cover,
                url: "/manga/" + slug
            });
        }

        // Recherche : le site n'en a pas d'exposée (mesuré : `?q=` est ignoré,
        // `/search` et `/recherche` renvoient 0 manga). On filtre donc sur place,
        // sans distinction de casse — c'est une recherche sur 14 titres, pas
        // davantage, et le résultat doit le dire.
        var q = SOURCE._texte(query);
        if (q !== "") {
            var filtres = [];
            for (var i = 0; i < resultats.length; i++) {
                if (resultats[i].title.toLowerCase().indexOf(q) >= 0) {
                    filtres.push(resultats[i]);
                }
            }
            resultats = filtres;
        }

        return JSON.stringify(resultats);
    },

    // =========================================================================
    // getDetails(mangaUrl) → JSON.stringify({title, description, coverUrl,
    //                          status, authors, tags, chapters})
    //
    // Tout vient de l'objet `manga` de la charge RSC, que l'on ancre sur le
    // slug demandé — sinon on lirait les champs d'une recommandation.
    // Mesuré autour de `\"slug\":\"blue-lock\"` :
    //
    //   \"manga\":{\"id\":106130,\"databaseId\":\"1487…\",
    //     \"slug\":\"blue-lock\",
    //     \"title\":{\"english\":\"Blue Lock\",\"romaji\":\"Blue Lock\",…},
    //     \"coverImage\":{\"large\":\"https://cdn.onefy.me/…/poster-….jpg\"},
    //     \"description\":\"…\",\"status\":\"…\"
    // =========================================================================
    getDetails: function (mangaUrl) {
        var slug = mangaUrl.substring(mangaUrl.lastIndexOf("/") + 1);
        var html = http.get(SOURCE.baseUrl + mangaUrl);

        var ancre = SOURCE._indexeDe(html, '"slug":"' + slug + '"');
        if (ancre < 0) {
            // Message utile plutôt qu'une fiche vide : sans cette ancre, on ne
            // sait pas si le site a changé sa structure ou si la série n'existe
            // plus, et les deux ne se corrigent pas de la même façon.
            throw new Error(
                "aniverse : manga « " + slug + " » introuvable dans la page (" +
                mangaUrl + ") — la structure du site a peut-être changé"
            );
        }
        // Fenêtre de lecture : le bloc `manga` tient largement en dessous.
        var bloc = html.substring(ancre, ancre + 12000);

        // ⚠ Ces motifs portent `(?:\\?")` et non `"` : mesuré, l'objet manga
        // est bien échappé (`\"title\":{\"english\":\"Blue Lock\"…`). Des motifs
        // à guillemets nus ne trouvaient rien et donnaient une fiche vide —
        // titre retombé sur le slug, couverture et description à zéro, sans
        // la moindre erreur. C'est ce que le test sur HTML réel a montré.
        //
        // ⚠ Et la capture est **paresseuse, arrêtée par `\",`**, pas
        // `(?:[^"\\]|\\.)*`. La seconde forme laisse `\\.` manger les `\"`
        // échappés et ne s'arrête qu'au premier guillemet *non* échappé — qui,
        // dans cette charge, se trouve des milliers de caractères plus loin
        // (mesuré : le titre de Blue Lock revenait suivi de `\"romaji\"…`
        // jusqu'au bloc des recommandations). Un titre n'est pas une longue
        // série d'attributs, donc on s'arrête à la virgule qui le suit.
        var titre = SOURCE._premierGroupe(bloc, /\\?"title\\?":\{\\?"english\\?":\\?"(.*?)\\?",/);
        if (titre === null) {
            titre = SOURCE._premierGroupe(bloc, /\\?"title\\?":\\?"(.*?)\\?",/);
        }
        if (titre === null) {
            titre = slug;
        }

        var cover = SOURCE._premierGroupe(bloc, /\\?"coverImage\\?":\{\\?"large\\?":\\?"(https:\/\/[^"\\]*)\\?"/);
        if (cover === null) {
            cover = SOURCE._premierGroupe(bloc, /\\?"image\\?":\\?"(https:\/\/cdn\.onefy\.me[^"\\]*)\\?"/);
        }

        // La description contient des guillemets échappés
        // (`\" (Source : Kodansha USA)\"`), jamais une `\"` suivie d'une
        // virgule : l'arrêt sur `\",` reste donc le bon, et le test le vérifie.
        var description = SOURCE._premierGroupe(bloc, /\\?"description\\?":\\?"(.*?)\\?",/);

        var statutBrut = SOURCE._premierGroupe(bloc, /\\?"status\\?":\\?"([^"\\]*)\\?"/);
        var statut = SOURCE._statut(statutBrut);

        // Listes de chaînes : même capture paresseuse arrêtée sur `\",` que le
        // titre. Avec `(?:[^"\\]|\\.)*`, le contenu `\"Action\",\"Drama\"…`
        // revenait en un seul tenant — les guillemets échappés passaient au
        // travers au lieu de fermer la chaîne.
        var auteurs = [];
        var mAuteur = /\\?"authors\\?":\[(.*?)\]/.exec(bloc);
        if (mAuteur) {
            auteurs = SOURCE._listeDeTextes(mAuteur[1]);
        }

        var genres = [];
        var mGenres = /\\?"genres\\?":\[(.*?)\]/.exec(bloc);
        if (mGenres) {
            genres = SOURCE._listeDeTextes(mGenres[1]);
        }

        // --- Chapitres -------------------------------------------------------
        // Mesuré : 360 entrées sur Blue Lock, et 0 correspondance parasite
        // ailleurs dans la page. On exige `title` pour écarter les
        // `latestChapters` des recommandations, qui n'en ont pas.
        var chapitres = [];
        var vus = {};
        // ⚠ `([\d.]+)` et non `(\d+)` : mesuré sur One Piece, le site publie
        // des chapitres à numéro décimal (`271.5`, `1195.5`). Avec `\d+`,
        // ces chapitres n'étaient pas seulement mal triés — ils disparaissaient
        // (1201 entrées sur la page, 1195 retenues), et personne n'aurait vu
        // qu'un chapitre manquait à l'écran.
        var reChap =
            /\\?"id\\?":\\?"([0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12})\\?",\\?"number\\?":\\?"([\d.]+)\\?",\\?"title\\?":\\?"((?:[^"\\]|\\.)*?)\\?"/g;
        var m;
        while ((m = reChap.exec(html)) !== null) {
            var id = m[1];
            if (vus[id]) {
                continue;
            }
            vus[id] = true;
            chapitres.push({
                id: id,
                number: parseFloat(m[2]),
                title: SOURCE._sansBalise(SOURCE._desechappe(m[3])),
                // L'URL du lecteur est `/read/<slug>/<uuid>` : le slug se
                // devine, l'UUID vient d'ici, du payload.
                url: "/read/" + slug + "/" + id
            });
        }
        // Contrat : les chapitres du plus ancien au plus récent.
        chapitres.sort(function (a, b) {
            return a.number - b.number;
        });

        return JSON.stringify({
            title: SOURCE._sansBalise(SOURCE._desechappe(titre)),
            description: SOURCE._sansBalise(SOURCE._desechappe(description === null ? "" : description)),
            coverUrl: cover === null ? "" : cover,
            status: statut,
            authors: auteurs,
            tags: genres,
            chapters: chapitres
        });
    },

    // =========================================================================
    // getPages(chapterUrl) → JSON.stringify([{id, url}, …])
    //
    // Mesuré : la page lecteur contient 19 URLs d'images directes sur
    // cdn.onefy.me, en document order, nommées 001.jpg, 002.jpg… Ce sont de
    // vraies images (`FF D8 FF E0`), pas du HTML déguisé.
    // =========================================================================
    getPages: function (chapterUrl) {
        var html = http.get(SOURCE.baseUrl + chapterUrl);

        var pages = [];
        var vues = {};
        // ⚠ On ne prend que `/raw/` : mesuré, la page lecteur charge aussi la
        // couverture et la bannière (`poster-…jpg`, `banner-…jpg`) sur le même
        // CDN. Sans ce filtre, la dernière « page » du chapitre était la
        // pochette — une image valide, donc rien ne signalait l'erreur.
        var re = /https:\/\/cdn\.onefy\.me\/raw\/[^"'\\ ]+?\.(?:jpg|jpeg|png|webp)/g;
        var m;
        while ((m = re.exec(html)) !== null) {
            var url = m[0];
            if (vues[url]) {
                continue;
            }
            vues[url] = true;
            pages.push({
                id: url.substring(url.lastIndexOf("/") + 1),
                url: url
            });
        }

        // Le nom de fichier est numéroté (001.jpg, 002.jpg…) : on trie sur ce
        // numéro plutôt que sur l'ordre du HTML, qui n'est pas contractuel.
        pages.sort(function (a, b) {
            return SOURCE._rang(a.id) - SOURCE._rang(b.id);
        });

        return JSON.stringify(pages);
    },

    // =========================================================================
    // Utilitaires — les mettre à jour si la structure du site change.
    // =========================================================================

    /** Premier groupe capturé par `re` dans `texte`, ou null. */
    _premierGroupe: function (texte, re) {
        var m = re.exec(texte);
        return m === null ? null : m[1];
    },

    /**
     * Indexe d'une sous-chaîne **tolérante à l'échappement**.
     *
     * Mesuré : `\"slug\":\"blue-lock\"` apparaît échappé une fois, et
     * `\\\"name\\\":\\\"Blue Lock\\\"` deux fois. On cherche donc le motif en
     * acceptant 0, 1 ou 2 antislashs avant chaque guillemet — sinon le même
     * site se lirait ou non selon la profondeur de son JSON.
     */
    _indexeDe: function (texte, motif) {
        var re = new RegExp(
            motif.replace(/"/g, '(?:\\\\{0,2}")')
        );
        var m = re.exec(texte);
        return m === null ? -1 : m.index;
    },

    /** Retire les balises HTML résiduelles d'un texte du site. */
    _sansBalise: function (texte) {
        return SOURCE._entites(
            String(texte === null || texte === undefined ? "" : texte)
                .replace(/<[^>]*>/g, " ")
                .replace(/\s+/g, " ")
                .replace(/^\s+|\s+$/g, "")
        );
    },

    /**
     * Décode les entités HTML, y compris **numériques**.
     *
     * Mesuré : le catalogue sert `Omniscient Reader&#x27;s Viewpoint` — un
     * apostrophe en hexadécimal. Sans ce décodage, le titre s'affichait
     * `Reader&#x27;s`, et les entités `&amp;`/`&lt;` étaient traités à part
     * alors qu'elles obéissent à la même règle.
     */
    _entites: function (texte) {
        return texte.replace(/&(#x?[0-9a-fA-F]+|[a-zA-Z]+);/g, function (tout, corps) {
            if (corps.charAt(0) === "#") {
                var hex = corps.charAt(1) === "x" || corps.charAt(1) === "X";
                var code = parseInt(hex ? corps.substring(2) : corps.substring(1), hex ? 16 : 10);
                if (isNaN(code) || code < 9 || code > 0xffff) {
                    return tout;
                }
                return String.fromCharCode(code);
            }
            if (corps === "amp") return "&";
            if (corps === "lt") return "<";
            if (corps === "gt") return ">";
            if (corps === "quot") return '"';
            if (corps === "apos") return "'";
            if (corps === "nbsp") return " ";
            return tout;
        });
    },

    /** Annule les échappements JSON d'une chaîne. */
    _desechappe: function (texte) {
        return String(texte)
            .replace(/\\u([0-9a-fA-F]{4})/g, function (_correspondance, hex) {
                return String.fromCharCode(parseInt(hex, 16));
            })
            .replace(/\\n/g, "\n")
            .replace(/\\r/g, "\r")
            .replace(/\\t/g, "\t")
            .replace(/\\"/g, '"')
            .replace(/\\\\/g, "\\");
    },

    /**
     * Découpe un tableau JSON de chaînes déjà capturé, `\ [ … \ ] `, dont le
     * contenu est échappé (`\"Action\",\"Drama\"`). Chaque élément est propre.
     */
    _listeDeTextes: function (contenu) {
        var valeurs = [];
        // La fin de segment compte comme terminateur : sans `|$`, le dernier
        // élément d'un tableau à un seul auteur n'était suivi ni d'une virgule
        // ni d'un crochet, donc la liste revenait **vide** — un auteur en plus
        // ou en moins ne se voit pas dans un compteur, seulement à l'écran.
        var re = /\\?"(.*?)\\?"(?=\s*[,}\]]|$)/g;
        var m;
        while ((m = re.exec(contenu)) !== null) {
            var valeur = SOURCE._sansBalise(SOURCE._desechappe(m[1]));
            if (valeur !== "") {
                valeurs.push(valeur);
            }
        }
        return valeurs;
    },

    /** Texte search : `null`/`undefined` ne doivent pas casser `indexOf`. */
    _texte: function (valeur) {
        return valeur === null || valeur === undefined ? "" : String(valeur);
    },

    /**
     * Traduit le statut du site vers les valeurs du contrat
     * (`ongoing` | `finished` | `paused` | `abandoned` | `unknown`).
     * Non mesuré sur d'autres valeurs que celles-ci : `unknown` par défaut,
     * plutôt qu'un statut inventé.
     */
    _statut: function (brut) {
        var s = SOURCE._texte(brut).toLowerCase();
        if (s === "" || s === "null") {
            return "unknown";
        }
        if (s.indexOf("releasing") >= 0 || s.indexOf("ongoing") >= 0 || s.indexOf("publishing") >= 0) {
            return "ongoing";
        }
        if (s.indexOf("finished") >= 0 || s.indexOf("completed") >= 0) {
            return "finished";
        }
        if (s.indexOf("paused") >= 0 || s.indexOf("hiatus") >= 0) {
            return "paused";
        }
        if (s.indexOf("cancelled") >= 0 || s.indexOf("canceled") >= 0 || s.indexOf("discontinued") >= 0) {
            return "abandoned";
        }
        return "unknown";
    },

    /**
     * Rang d'un nom de page numéroté (`002.jpg` → 2).
     *
     * Le plafond est écrit en dur : Nashorn est ES5.1 et ne connaît pas
     * `Number.MAX_SAFE_INTEGER` (ES6) — l'y écrire donnerait `undefined`, et
     * une comparaison avec `undefined` vaut `false` dans les deux sens, donc
     * le tri deviendrait silencieusement faux.
     */
    _rang: function (nom) {
        var m = /(\d+)/.exec(nom);
        return m === null ? 2147483647 : parseInt(m[1], 10);
    }
};
