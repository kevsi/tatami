/**
 * Source scriptée : Ortega Scans (https://ortegascans.fr)
 * =====================================================
 *
 * Source **absente du catalogue** `kotatsu-parsers-redo` : aucune redirection
 * de domaine ne peut la faire apparaître, il faut donc un script.
 *
 * Le site est une application **Next.js (App Router)** : le HTML initial ne
 * contient aucun balisage de données, tout est dans le payload RSC, sous
 * forme de `self.__next_f.push([1,"…"])` où **tous les guillemets sont
 * échappés** (`\"`). C'est ce que les regex ci-dessous cherchent — un `\"`
 * littéral, pas un `"`. Un script écrit comme si le HTML était du JSON
 * normal ne trouve rien, sans erreur.
 *
 * Routes mesurées le 2026-10-13 :
 *  - `/api/covers/<slug>`            → couverture (image/webp, testée : 97 Ko)
 *  - `/api/banners-mangas/<slug>`    → bannière
 *  - `/api/chapters/<slug>/<id>/image/<n>.webp` → page de chapitre
 *  - `/serie/<slug>`                 → fiche
 *  - `/serie/<slug>/chapter/<n>`     → lecture
 *
 * Limites réelles du site, mesurées, à ne pas confondre avec des bugs :
 *  - **Aucun catalogue ni recherche.** `/serie`, `/catalogue`, `/mangas`,
 *    `/recherche` répondent tous 404, et `/?page=2` renvoie une page vide.
 *    Seul l'accueil liste des séries (23 au 2026-10-13) et la recherche se
 *    fait donc sur cette liste, comme sur Hentai-Scantrad.
 *  - **Les images ne sont pas toutes dans le HTML.** Le lecteur n'en rend que
 *    5 sur 10 dans les balises `<img>` ; la liste complète est dans le payload
 *    (`"total":10`). Lire les `<img>` perdrait la moitié des pages — mesuré.
 *  - **Chapitres premium.** Chaque chapitre porte `isPremium` ; le payload
 *    du lecteur renvoie alors `protected:true` et les images ne sont pas
 *    servies. On ne peut pas les lire sans compte, et ce n'est pas un défaut.
 */

var SOURCE = {
    id: "ortega-scans",
    name: "Ortega Scans",
    lang: "fr",
    baseUrl: "https://ortegascans.fr",

    // -----------------------------------------------------------------------
    // Utilitaires — le payload est échappé, d'où les `\"` dans les motifs.
    // -----------------------------------------------------------------------

    trim: function (s) {
        return String(s).replace(/^[\s ]+/, "").replace(/[\s ]+$/, "");
    },

    decodeEntites: function (s) {
        var table = {
            "&amp;": "&", "&quot;": '"', "&#039;": "'", "&apos;": "'",
            "&laquo;": "«", "&raquo;": "»", "&hellip;": "…", "&nbsp;": " ",
            "\\u00e9": "é", "\\u00e8": "è", "\\u00ea": "ê", "\\u00e0": "à",
            "\\u00e7": "ç", "\\u00f4": "ô", "\\u00fb": "û"
        };
        var out = String(s);
        for (var cle in table) {
            if (table.hasOwnProperty(cle)) out = out.split(cle).join(table[cle]);
        }
        // Le payload peut garder des \\uXXXX : on les résout au cas par cas.
        return out.replace(/\\u([0-9a-fA-F]{4})/g, function (m, h) {
            return String.fromCharCode(parseInt(h, 16));
        });
    },

    sansAccents: function (s) {
        var table = {
            "à": "a", "â": "a", "ä": "a", "á": "a", "ã": "a", "å": "a",
            "ç": "c", "è": "e", "é": "e", "ê": "e", "ë": "e",
            "î": "i", "ï": "i", "í": "i", "ô": "o", "ö": "o", "ó": "o", "õ": "o",
            "ù": "u", "û": "u", "ü": "u", "ú": "u", "ý": "y", "ÿ": "y", "ñ": "n"
        };
        return SOURCE.trim(s).toLowerCase()
            .replace(/[àâäáãåçèéêëîïíôöóõùûüúýÿñ]/g, function (c) { return table[c]; });
    },

    // -----------------------------------------------------------------------
    // getList(page, query)
    // -----------------------------------------------------------------------

    /**
     * Les fiches de l'accueil, lues dans le payload.
     *
     * L'accueil est un carrousel « personnage » : chaque entrée est
     * `{\"position\":N,\"character\":…,\"manga\":{\"title\":…,\"slug\":…}}`.
     * On prend le bloc `manga` pour ne pas confondre avec les données de
     * navigation, qui portent les mêmes clés.
     */
    parseListe: function (html) {
        // L'accueil porte **deux formes** pour la même chose, mesurées :
//  - 5 entrées du carrousel, en `\"manga\":{\"id\":…,\"title\":…,\"slug\":…}`
//  - 18 entrées de la liste, en `\"title\":…,\"slug\":…` sans ce Bourreau
// Les deux portent les mêmes clés adjacentes, donc on lit la paire
// `title`+`slug` partout et on déduplique par slug — sinon on ne verrait
// que 5 séries sur 23, sans la moindre erreur visible.
//
// Le motif est **non avide et strict** (`((?:[^"\\]|\\[^"])*?)`) : une version
// plus permissif enjambe plusieurs entrées et fabrique un titre fait de
// plusieurs titres — mesuré : « A Young Auntie  (https://… ».
var out = [];
        var vus = {};
        var re = /\\"title\\":\\"((?:[^"\\]|\\[^"])*?)\\",\\"slug\\":\\"([^"\\]+)\\"/g;
        var m;
        while ((m = re.exec(html)) !== null) {
            var slug = m[2];
            if (vus[slug]) continue;
            vus[slug] = true;
            out.push({
                id: SOURCE.baseUrl + "/serie/" + slug,
                title: SOURCE.trim(SOURCE.decodeEntites(m[1])),
                // L'API de couverture prend le slug : mesuré, `/api/covers/
                // <slug>` renvoie un webp, alors que le `coverImage` du payload
                // est un chemin de stockage interne sans base connue.
                coverUrl: SOURCE.baseUrl + "/api/covers/" + slug,
                url: SOURCE.baseUrl + "/serie/" + slug
            });
        }
        return out;
    },

    getList: function (page, query) {
        var items = SOURCE.parseListe(http.get(SOURCE.baseUrl + "/"));
        if (query && SOURCE.trim(query).length > 0) {
            // Pas de recherche sur le site (mesuré : 404 sur `/recherche`) :
            // on filtre la liste de l'accueil.
            var cible = SOURCE.sansAccents(query);
            var trouves = [];
            for (var i = 0; i < items.length; i++) {
                if (SOURCE.sansAccents(items[i].title).indexOf(cible) >= 0) {
                    trouves.push(items[i]);
                }
            }
            items = trouves;
        }
        // L'accueil n'a pas de pagination : au-delà de la page 1, c'est vide.
        if (page > 1 && !(query && SOURCE.trim(query).length > 0)) {
            return JSON.stringify([]);
        }
        return JSON.stringify(items);
    },

    // -----------------------------------------------------------------------
    // getDetails(mangaUrl)
    // -----------------------------------------------------------------------

    getDetails: function (mangaUrl) {
        var url = mangaUrl.indexOf("http") === 0 ? mangaUrl : SOURCE.baseUrl + mangaUrl;
        var html = http.get(url);

        var slug = url.replace(/^https?:\/\/[^/]+\/serie\//, "").replace(/\/$/, "");

        // Titre : le bloc `initialData` d'abord, le `<title>` en secours. Même
        // précaution qu'au catalogue : on ancre la recherche sur la clé plutôt
        // que de laisser le motif traverser le payload.
        var mTitre = html.match(/\\"manga\\":\{\\"id\\":\\"[^"\\]*\\",\\"title\\":\\"((?:[^"\\]|\\[^"])*?)\\"/);
        var titre = mTitre ? SOURCE.trim(SOURCE.decodeEntites(mTitre[1])) : "";
        if (!titre) {
            var mT = html.match(/<title>([\s\S]*?)<\/title>/);
            if (mT) titre = SOURCE.trim(SOURCE.decodeEntites(mT[1])).split("|")[0];
        }

        // `\\.` — et non `\\[^"]` — matchait **aussi** `\"`, donc il
        // consommait le guillemet de fin comme un échappement et le motif
        // continuait dans le payload : l'écran affichait le JSON brut à la
        // place du résumé, jusqu'à `\"bannerImage\"`. C'est exactement ce que
        // le contenu de `\"` doit **interrompre** la lecture.
        var mDesc = html.match(/\\"description\\":\\"((?:[^"\\]|\\[^"])*)\\"/);

        // Statut : mesuré dans le payload de l'accueil (« en cours »).
        var mStatut = html.match(/\\"status\\":\\"([^"\\]*)\\"/);
        var etats = { "en cours": "ongoing", "termine": "finished", "en pause": "paused" };
        var statut = "unknown";
        if (mStatut) {
            var brut = SOURCE.sansAccents(mStatut[1]);
            for (var cle in etats) {
                if (etats.hasOwnProperty(cle) && SOURCE.sansAccents(cle) === brut) {
                    statut = etats[cle];
                }
            }
        }

        // Chapitres : `{"id":"…","number":N,"isPremium":true|false,…}`
        //
        // `isPremium` est lu et renvoyé tel quel. Le site sert bien les
        // images d'un chapitre gratuit (mesuré : 200) mais refuse celles d'un
        // chapitre premium avec « Accès non autorisé » (403) — et le refus
        // ne dépend pas du compte : mesuré sur un compte connecté dont la
        // session portait `isPremium:false`. Sans ce marqueur, l'utilisateur
        // n'apprend qu'en ouvrant le chapitre qu'il ne peut pas le lire.
        var chapitres = [];
        var vus = {};
        var re = /\\"chapters\\":\[([\s\S]*?)\]/g;
        var bloc = re.exec(html);
        if (bloc) {
            var reCh = /\\"id\\":\\"([^"\\]+)\\",\\"number\\":([0-9]+),\\"title\\":[^,]*,\\"isPremium\\":(true|false)/g;
            var m;
            while ((m = reCh.exec(bloc[1])) !== null) {
                var num = parseInt(m[2], 10);
                if (isNaN(num) || vus[num]) continue;
                vus[num] = true;
                var chUrl = SOURCE.baseUrl + "/serie/" + slug + "/chapter/" + num;
                chapitres.push({
                    id: chUrl,
                    title: "Chapitre " + num,
                    number: num,
                    url: chUrl,
                    premium: m[3] === "true"
                });
            }
        }
        chapitres.sort(function (a, b) { return a.number - b.number; });

        return JSON.stringify({
            title: SOURCE.trim(titre),
            description: mDesc ? SOURCE.decodeEntites(mDesc[1]) : "",
            coverUrl: SOURCE.baseUrl + "/api/covers/" + slug,
            status: statut,
            authors: [],
            tags: [],
            chapters: chapitres
        });
    },

    // -----------------------------------------------------------------------
    // getPages(chapterUrl)
    // -----------------------------------------------------------------------

    getPages: function (chapterUrl) {
        var url = chapterUrl.indexOf("http") === 0 ? chapterUrl : SOURCE.baseUrl + chapterUrl;
        var html = http.get(url);

        // On lit **le payload**, pas les `<img>` : le lecteur n'en rend que 5
        // sur 10 dans le balisage (mesuré), le payload en a tous.
        var vus = {};
        var pages = [];
        var re = /\/api\/chapters\/[^"\\\s]+\/image\/([0-9]+)\.webp/g;
        var m;
        while ((m = re.exec(html)) !== null) {
            var src = m[0];
            if (vus[src]) continue;
            vus[src] = true;
            pages.push({ id: String(pages.length + 1), url: SOURCE.baseUrl + src });
        }
        // L'ordre des pages est celui du payload (index 0..n) ; on garde cet
        // ordre, qui est l'ordre de lecture.
        return JSON.stringify(pages);
    }
};