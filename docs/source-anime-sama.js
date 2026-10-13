/**
 * Source scriptée : Anime-Sama (https://anime-sama.to)
 * =================================================
 *
 * Le parseur natif `ANIMESAMA` cible `anime-sama.eu`, qui ne résout plus
 * (mesuré le 2026-10-13, HTTP 000). Avec la redirection de domaine il
 * retrouve le catalogue, mais **les chapitres restent vides** : son mécanisme
 * est mort. Mesuré :
 *
 *  - `episodes.js` → **404**. C'est ce que le parseur appelle : il construit
 *    `<baseUri>/episodes.js?title=<titre>` (lu dans le bytecode). Le fichier
 *    n'existe plus.
 *  - `/scan/vf/<slug>/` → 404 aussi (le parseur tente ce chemin).
 *
 * Le vrai chemin de lecture est `/catalogue/<slug>/scan/vf/`, et la liste
 * des chapitres vient d'une API. Tout ce qui suit est mesuré, pas supposé :
 *
 *  - Catalogue : `/catalogue/?page=<n>` (48 fiches/page) et
 *    `/catalogue/?search=<q>` pour la recherche — mesuré : `search=naruto`
 *    ramène 6 fiches contre 48 sans recherche.
 *  - Fiche : `<div class="shrink-0 catalog-card">` contenant `<a href>`,
 *    `<img class="card-image" src>` et `<h2 class="card-title">`. **La
 *    couverture est dans `src`** ici — contrairement à Hentai-Scantrad, où
 *    elle est dans `data-src`. Deux sites, deux formes.
 *  - Nom de l'œuvre : la page lecteur `/catalogue/<slug>/scan/vf/` porte
 *    `<div id="titreOeuvre">`. Attention : ce nom n'est pas le titre de la
 *    fiche — mesuré, « One Piece » côté catalogue donne
 *    `#titreOeuvre` = « One Piece Couleur », et l'API ne connaît que ce
 *    dernier. Lire le titre de la fiche serait un 404 silencieux.
 *  - Chapitres : `GET /s2/scans/get_nb_chap_et_img.php?oeuvre=<nom>` renvoie
 *    `{"1":57,"2":25,…}` : clé = numéro de chapitre, valeur = nombre de
 *    pages. Une œuvre sans scans répond `{"error":"… not found"}` — mesuré
 *    pour Naruto et 07 Ghost : le catalogue les liste, l'API non. C'est un
 *    fait du site, pas une panne.
 *  - Images : `/s2/scans/<œuvre>/<chapitre>/<page>.jpg`, `page` à partir
 *    de 1 (lu dans `scans.js` : `img.src = \`/s2/scans/${activeNom}/${
 *    realChap}/${i}.jpg\``).
 */

var SOURCE = {
    id: "anime-sama",
    name: "Anime-Sama",
    lang: "fr",
    baseUrl: "https://anime-sama.to",

    /** Racine des images et de l'API chapters. */
    s2: "https://anime-sama.to/s2/scans",

    // -----------------------------------------------------------------------
    // Utilitaires
    // -----------------------------------------------------------------------

    trim: function (s) {
        return String(s).replace(/^[\s ]+/, "").replace(/[\s ]+$/, "");
    },

    decodeEntites: function (s) {
        var table = {
            "&amp;": "&", "&quot;": '"', "&#039;": "'", "&apos;": "'",
            "&laquo;": "«", "&raquo;": "»", "&hellip;": "…", "&nbsp;": " "
        };
        var out = String(s);
        for (var cle in table) {
            if (table.hasOwnProperty(cle)) out = out.split(cle).join(table[cle]);
        }
        return out;
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

    urlListe: function (page, query) {
        if (query && SOURCE.trim(query).length > 0) {
            return SOURCE.baseUrl + "/catalogue/?search=" + encodeURIComponent(SOURCE.trim(query));
        }
        return SOURCE.baseUrl + "/catalogue/?page=" + page;
    },

    /** Extrait les fiches d'une page de catalogue. */
    parseListe: function (html) {
        var morceaux = html.split('class="shrink-0 catalog-card');
        var out = [];
        for (var i = 1; i < morceaux.length; i++) {
            var bloc = morceaux[i].substring(0, 1800);

            var mHref = bloc.match(/<a href="(https:\/\/anime-sama\.to\/catalogue\/[^"?#]+)"/);
            if (!mHref) continue;

            // La couverture est dans `src` sur ce site. Prendre `data-src`
            // reviendrait à ne rien trouver — vérifié sur la page réelle.
            var mCov = bloc.match(/<img[^>]*class="card-image"[^>]*src="([^"]+)"/);
            if (!mCov) mCov = bloc.match(/<img[^>]*src="([^"]+)"[^>]*class="card-image"/);

            var mTitre = bloc.match(/<h2 class="card-title">([\s\S]*?)<\/h2>/);

            var titre = "";
            if (mTitre) {
                // Le catalogue préfixe le titre d'un `#` : c'est décoratif,
                // pas une partie du nom.
                titre = SOURCE.trim(SOURCE.decodeEntites(
                    mTitre[1].replace(/<[^>]+>/g, "")
                )).replace(/^#\s*/, "");
            }
            if (!titre) {
                titre = mHref[1].replace(/\/$/, "").split("/").pop().replace(/-/g, " ");
            }

            out.push({
                id: mHref[1],
                title: SOURCE.trim(titre),
                coverUrl: mCov ? SOURCE.trim(mCov[1]) : "",
                url: mHref[1]
            });
        }
        return out;
    },

    getList: function (page, query) {
        return JSON.stringify(SOURCE.parseListe(http.get(SOURCE.urlListe(page, query))));
    },

    // -----------------------------------------------------------------------
    // getDetails(mangaUrl)
    // -----------------------------------------------------------------------

    /** Nom exact de l'œuvre, lu sur la page lecteur. */
    oeuvreDe: function (mangaUrl) {
        var lecteur = mangaUrl.replace(/\/$/, "") + "/scan/vf/";
        var html = http.get(lecteur);
        var m = html.match(/id="titreOeuvre"[^>]*>([^<]*)</);
        if (!m) return "";
        // Le site préfixe aussi le nom d'œuvre d'un `#` décoratif — mesuré :
        // « #Zombie Sagashitemasu ». L'API attend le nom nu.
        return SOURCE.trim(SOURCE.decodeEntites(m[1])).replace(/^#\s*/, "");
    },

    /** Nombre de pages par chapitre, via l'API du site. */
    chapitresDe: function (oeuvre) {
        if (!oeuvre) return {};
        var url = "https://anime-sama.to/s2/scans/get_nb_chap_et_img.php?oeuvre="
            + encodeURIComponent(oeuvre);
        var brut = http.get(url);
        try {
            var data = JSON.parse(brut);
            // Une œuvre sans scans répond 200 + `{"error": …}` : ce n'est pas
            // une panne, il n'y a simplement rien à lire.
            if (!data || data.error) return {};
            // Défense supplémentaire : on ne garde que des valeurs entières
            // plausibles. Une clé qui ne dit pas « combien d'images » n'est
            // pas un chapitre, et le site sert aussi du streaming.
            var propre = {};
            for (var cle in data) {
                if (!data.hasOwnProperty(cle)) continue;
                var n = parseInt(cle, 10);
                var nb = parseInt(data[cle], 10);
                if (isNaN(n) || n <= 0 || isNaN(nb) || nb <= 0 || nb > 1000) continue;
                propre[String(n)] = nb;
            }
            return propre;
        } catch (e) {
            return {};
        }
    },

    getDetails: function (mangaUrl) {
        var url = mangaUrl.indexOf("http") === 0 ? mangaUrl : SOURCE.baseUrl + mangaUrl;
        var html = http.get(url);

        // Titre et synopsis : mesurés **absents** des balises `og:` sur certaines
        // fiches (Zombie Sagashitemasu n'a ni `og:title` ni `og:description`,
        // alors que 07 Ghost les a). Le `<title>` et la `<meta
        // name="description">`, eux, sont présents sur les deux — donc on ne
        // se fie pas aux seules `og:`, sous peine d'afficher l'URL comme titre.
        var mTitre = html.match(/property="og:title" content="([^"]*)"/)
            || html.match(/<title>([\s\S]*?)<\/title>/);
        var mDesc = html.match(/property="og:description" content="([^"]*)"/)
            || html.match(/<meta name="description" content="([^"]*)"/);
        var mCov = html.match(/property="og:image" content="([^"]*)"/);

        var oeuvre = SOURCE.oeuvreDe(url);
        var pagesParChapitre = SOURCE.chapitresDe(oeuvre);

        var chapitres = [];
        for (var cle in pagesParChapitre) {
            if (!pagesParChapitre.hasOwnProperty(cle)) continue;
            var num = parseInt(cle, 10);
            if (isNaN(num)) continue;
            // URL synthétique : elle encode l'œuvre et le chapitre pour que
            // `getPages` n'ait plus qu'un aller-retour à l'API.
            var chUrl = SOURCE.baseUrl + "/scan/vf/" + num + "/"
                + encodeURIComponent(oeuvre);
            chapitres.push({
                id: chUrl,
                title: "Chapitre " + num,
                number: num,
                url: chUrl
            });
        }
        chapitres.sort(function (a, b) { return a.number - b.number; });

        var titre = "";
        if (mTitre) {
            // `<title>` vaut « #Titre | Anime-Sama - … » : on garde la
            // partie avant la barre verticale et on retire le `#` décoratif.
            titre = SOURCE.trim(SOURCE.decodeEntites(mTitre[1])).split("|")[0].replace(/^#\s*/, "");
        }

        return JSON.stringify({
            title: SOURCE.trim(titre),
            description: mDesc ? SOURCE.decodeEntites(mDesc[1]) : "",
            coverUrl: mCov ? SOURCE.trim(mCov[1]) : "",
            status: "unknown",
            authors: [],
            tags: [],
            chapters: chapitres
        });
    },

    // -----------------------------------------------------------------------
    // getPages(chapterUrl)
    // -----------------------------------------------------------------------

    getPages: function (chapterUrl) {
        // `<base>/scan/vf/<chapitre>/<œuvre encodée>`
        var url = chapterUrl.indexOf("http") === 0 ? chapterUrl : SOURCE.baseUrl + chapterUrl;
        var urlSansBase = url.replace(/^https?:\/\/[^/]+\//, "");
        var morceaux = urlSansBase.replace(/\/$/, "").split("/");
        var oeuvre = "";
        var num = 0;
        for (var i = 0; i < morceaux.length; i++) {
            if (morceaux[i] === "vf" && i + 2 < morceaux.length) {
                num = parseInt(morceaux[i + 1], 10);
                oeuvre = decodeURIComponent(morceaux[i + 2]);
            }
        }
        if (!oeuvre || isNaN(num) || num <= 0) return JSON.stringify([]);

        var pagesParChapitre = SOURCE.chapitresDe(oeuvre);
        var total = pagesParChapitre[String(num)];
        if (!total || isNaN(total) || total <= 0) return JSON.stringify([]);

        var pages = [];
        for (var p = 1; p <= total; p++) {
            var src = SOURCE.s2 + "/" + encodeURIComponent(oeuvre) + "/" + num + "/" + p + ".jpg";
            // Garde-fou : Anime-Sama sert aussi du **streaming** sur le même
            // domaine (`panneauAnime(…, "saison1/vostfr")`). Un lecteur de
            // manga ne doit jamais renvoyer une vidéo — d'où ce refus
            // explicite plutôt qu'une confiance dans la construction d'URL.
            if (!/^https:\/\/anime-sama\.to\/s2\/scans\/.+\/[0-9]+\.jpg$/.test(src)) {
                continue;
            }
            pages.push({ id: String(p), url: src });
        }
        return JSON.stringify(pages);
    }
};