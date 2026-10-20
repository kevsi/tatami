/**
 * Source scriptée : Pantheon Scan (https://pantheon-scan.com)
 * =====================================================
 *
 * Source **présente au catalogue** mais inutilisable : le parseur natif
 * `PANTHEONSCAN` (Madara) exige `#filter-wrapper` pour lire la liste, et le
 * site n'en a **aucun** — mesuré le 2026-10-13, 0 occurrence, alors que le
 * domaine du parseur est bien le bon et répond 200. D'où « 200 OK sans
 * résultat » dans `contract/sources-etat.json` : le site vit, le parseur ne
 * voit simplement rien.
 *
 * Ce thème est une **variante** de Madara, et ces écarts sont tous mesurés :
 *
 *  - Attributs **non guillemets** sur la page de catalogue :
 *    `<div class="page-item-detail manga "><div id=manga-item-3289 …>`,
 *    `<a href=https://pantheon-scan.com/manga/… title="…">`. Les regex ci-dessous
 *    acceptent donc `href=` comme `href="` — un motif trop strict ne trouve
 *    rien, sans erreur.
 *  - La couverture est dans `data-src` ; `src` vaut un **SVG placeholder**
 *    (`data:image/svg+xml,…`). Prendre `src` renverrait le placeholder.
 *  - Les chapitres sont **absents du HTML** de la série : la page ne contient
 *    que `<div id=manga-chapters-holder data-id=…><i class="fas fa-spinner…">`.
 *    Ils arrive en `POST <url_de_la_série>ajax/chapters/` — même endpoint que
 *    Hentai-Scantrad, trouvé dans `madara-core/assets/js/script.js`. Ni nonce,
 *    ni `admin-ajax.php`.
 *  - Le message « Sorry, you have Javascript Disabled » est un `<noscript>`
 *    de repli : le contenu est bien rendu côté serveur. Ne pas le prendre pour
 *    un verrou.
 *  - Les images de chapitre portent `data-src=" https://…"` — **espace
 *    initiale**, sans `trim()` l'URL devient relative.
 *
 * Format : `/manga/` puis `/manga/page/<n>/`, 12 fiches par page (mesuré).
 */

var SOURCE = {
    id: "pantheon-scan",
    name: "Pantheon Scan",
    lang: "fr",
    baseUrl: "https://pantheon-scan.com",

    // -----------------------------------------------------------------------
    // Utilitaires
    // -----------------------------------------------------------------------

    trim: function (s) {
        return String(s).replace(/^[\s ]+/, "").replace(/[\s ]+$/, "");
    },

    decodeEntites: function (s) {
        var table = {
            "&amp;": "&", "&quot;": '"', "&#039;": "'", "&apos;": "'",
            "&laquo;": "«", "&raquo;": "»", "&hellip;": "…", "&nbsp;": " ",
            "&#8230;": "…", "&#8217;": "'"
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

    /**
     * Une URL d'attribut, guillemetée ou non.
     *
     * `["']?` puis `[^"'\s>]+` : c'est ce qui fait la différence entre 12
     * fiches trouvées et 0 sur cette variante du thème.
     */
    attrUrl: function (bloc, nom) {
        var re = new RegExp(nom + '=["\']?([^"\'\\s>]+)', "i");
        var m = re.exec(bloc);
        return m ? SOURCE.trim(m[1]) : "";
    },

    // -----------------------------------------------------------------------
    // getList(page, query)
    // -----------------------------------------------------------------------

    urlListe: function (page, query) {
        var base = SOURCE.baseUrl + "/manga/";
        if (query && SOURCE.trim(query).length > 0) {
            // Le thème n'expose pas de recherche mesurée : on parcourt le
            // catalogue et on filtre, comme sur Hentai-Scantrad.
            return base;
        }
        return page > 1 ? base + "page/" + page + "/" : base;
    },

    parseListe: function (html) {
        var morceaux = html.split('class="page-item-detail manga');
        var out = [];
        for (var i = 1; i < morceaux.length; i++) {
            var bloc = morceaux[i].substring(0, 2000);

            var mLien = /<a href=["']?(https:\/\/pantheon-scan\.com\/manga\/[^"'\s>]+)/.exec(bloc);
            if (!mLien) continue;
            var url = mLien[1];

            var titre = "";
            var mTitre = /title=["']([^"']+)["']\s*>/.exec(bloc);
            if (mTitre) {
                titre = SOURCE.trim(SOURCE.decodeEntites(mTitre[1]));
            } else {
                var mH3 = /<h3[^>]*>\s*<a[^>]*>([^<]+)<\/a>/.exec(bloc);
                if (mH3) titre = SOURCE.trim(SOURCE.decodeEntites(mH3[1]));
            }
            if (!titre) {
                titre = url.replace(/^https:\/\/pantheon-scan\.com\/manga\//, "")
                    .replace(/\/$/, "").replace(/-/g, " ");
            }

            // `data-src` et non `src` : mesuré, `src` est un SVG placeholder.
            var couverture = SOURCE.attrUrl(bloc, "data-src");

            out.push({ id: url, title: SOURCE.trim(titre), coverUrl: couverture, url: url });
        }
        return out;
    },

    getList: function (page, query) {
        if (!query || SOURCE.trim(query).length === 0) {
            return JSON.stringify(SOURCE.parseListe(http.get(SOURCE.urlListe(page, query))));
        }
        var cible = SOURCE.sansAccents(query);
        var trouves = [];
        for (var p = 1; p <= 40 && trouves.length < page * 24; p++) {
            var html;
            try {
                html = http.get(SOURCE.urlListe(p, null));
            } catch (e) {
                break;
            }
            var items = SOURCE.parseListe(html);
            if (items.length === 0) break;
            for (var j = 0; j < items.length; j++) {
                if (SOURCE.sansAccents(items[j].title).indexOf(cible) >= 0) {
                    trouves.push(items[j]);
                }
            }
        }
        var debut = (page - 1) * 24;
        if (debut >= trouves.length) return JSON.stringify([]);
        return JSON.stringify(trouves.slice(debut, debut + 24));
    },

    // -----------------------------------------------------------------------
    // getDetails(mangaUrl)
    // -----------------------------------------------------------------------

    getDetails: function (mangaUrl) {
        var url = mangaUrl.indexOf("http") === 0 ? mangaUrl : SOURCE.baseUrl + mangaUrl;
        var html = http.get(url);

        var titre = "";
        var mT = /<h1[^>]*>([\s\S]*?)<\/h1>/.exec(html);
        if (mT) titre = SOURCE.trim(SOURCE.decodeEntites(mT[1].replace(/<[^>]+>/g, "")));
        if (!titre) {
            var mMeta = /<meta property="og:title" content="([^"]*)"/.exec(html);
            if (mMeta) titre = SOURCE.trim(SOURCE.decodeEntites(mMeta[1]));
        }

        var resume = "";
        var mD = /<meta name="description" content="([^"]*)"/.exec(html);
        if (mD) resume = SOURCE.decodeEntites(mD[1]);

        // `og:image` d'abord : prendre le **premier** `data-src` de la page revient
// à attraper le logo du site — mesuré, la couverture rendue était
// « pantheon-scan-modified2….png ». Fallback sur l'ancre de la série.
var couverture = "";
var mOg = /<meta property="og:image" content="([^"]*)"/.exec(html);
if (mOg) couverture = SOURCE.trim(mOg[1]);
if (!couverture) {
    var mLien = /<a href=["']?(https:\/\/pantheon-scan\.com\/manga\/[^"'\s>]+)["']?[^>]*>\s*<img/i.exec(html);
    if (mLien) {
        var apres = html.substring(mLien.index, mLien.index + 900);
        couverture = SOURCE.attrUrl(apres, "data-src");
    }
}
if (!couverture) couverture = SOURCE.attrUrl(html, "data-src");

        // Chapitres : endpoint AJAX du thème, PAS `admin-ajax.php`, sans nonce.
        var chapitres = [];
        var ajax = "";
        try {
            ajax = http.post(url.replace(/\/$/, "") + "/ajax/chapters/", "");
        } catch (e) {
            ajax = "";
        }

        var vus = {};
        var reCh = /<a href="(https:\/\/pantheon-scan\.com\/manga\/[^"?#]+\/chapitre-[^"?#]+\/)"[^>]*>([\s\S]*?)<\/a>/g;
        var m;
        while ((m = reCh.exec(ajax)) !== null) {
            var chUrl = m[1];
            if (vus[chUrl]) continue; // le lien de date de sortie répète l'URL
            vus[chUrl] = true;
            var libelle = SOURCE.trim(SOURCE.decodeEntites(m[2].replace(/<[^>]+>/g, "")));
            var num = 0;
            var mNum = chUrl.match(/chapitre-([0-9]+(?:\.[0-9]+)?)/);
            if (mNum) num = parseFloat(mNum[1]);
            chapitres.push({ id: chUrl, title: libelle, number: num, url: chUrl });
        }
        // L'ajax rend l'ordre du site (21, 20, 19…) ; le contrat veut l'ancien
        // en premier, donc tri croissant.
        chapitres.sort(function (a, b) { return a.number - b.number; });

        return JSON.stringify({
            title: SOURCE.trim(titre),
            description: resume,
            coverUrl: couverture,
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
        var url = chapterUrl.indexOf("http") === 0 ? chapterUrl : SOURCE.baseUrl + chapterUrl;
        var html = http.get(url);

        // Classe `wp-manga-chapter-img` uniquement : les images de pub du thème
        // ont d'autres classes, les prendre reviendrait à afficher de la pub.
        var re = /<img[^>]*class=["'][^"']*wp-manga-chapter-img[^"']*["'][^>]*>/g;
        var pages = [];
        var vus = {};
        var m;
        while ((m = re.exec(html)) !== null) {
            var balise = m[0];
            var mSrc = /data-src=["']\s*([^"']+)/.exec(balise);
            if (!mSrc) continue;
            // Espace initiale mesurée : sans trim() l'URL est relative.
            var src = SOURCE.trim(mSrc[1]);
            if (!src || vus[src]) continue;
            vus[src] = true;
            pages.push({ id: String(pages.length + 1), url: src });
        }
        return JSON.stringify(pages);
    }
};