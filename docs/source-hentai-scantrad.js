/**
 * Source scriptée : Hentai-Scantrad (https://hentai-scantrad.org)
 * =============================================================
 *
 * Nécessaire parce que le parseur natif `SCANTRAD` pointe sur `scan-trad.com`,
 * qui ne résout plus (mesuré le 2026-10-13, HTTP 000), alors que le site vit
 * sur `hentai-scantrad.org`.
 *
 * Tout ce qui suit est mesuré sur le HTML réel du site (2026-10-13), pas
 * supposé :
 *
 *  - Liste     : `/manga/` puis `/manga/page/<n>/`, 7 fiches par page.
 *                Chaque fiche est un `<div class="page-item-detail manga">`
 *                contenant, dans cet ordre, `<a href=… title=…>` puis
 *                `<img data-src=…>`.
 *  - Chapitres : **absents du HTML de la série.** Le thème les charge en
 *                POST sur `<url_de_la_serie>ajax/chapters/` (endpoint trouvé
 *                dans `madara-core/assets/js/script.js` : `url: manga.base_url
 *                + 'ajax/chapters/'`). Pas de nonce, ce n'est pas
 *                `admin-ajax.php`.
 *  - Pages     : `<div class="page-break"><img class="wp-manga-chapter-img"
 *                data-src=" …">`. **`data-src` a une espace initiale** sur
 *                cette page, et les images sont hébergées sur un autre hôte
 *                (`stockage.hentai-scantrad.org`) : les Couvers de la liste
 *                sont sur le domaine du site, celles du chapitre non.
 *
 * Limite connue, assumée : la recherche. Le formulaire du thème est en AJAX
 * et aucun endpoint de recherche live n'a été trouvé (mesuré : `0` renvoyé
 * par `admin-ajax.php` sur `manga_live_search`, `wp_manga_live_search` et
 * `manga_search`) ; `GET /?s=…&post_type=wp-manga` renvoie « Page not found ».
 * `getList` filtre donc le catalogue **localement**, ce qui est exact mais
 * parcourt les pages une à une. Le catalogue fait 30 pages, donc c'est
 * acceptable — à reconsidérer si le catalogue grossit beaucoup.
 */

var SOURCE = {
    id: "hentai-scantrad",
    name: "Hentai-Scantrad",
    lang: "fr",
    baseUrl: "https://hentai-scantrad.org",

    /** Nombre de pages de catalogue parcourues par une recherche. */
    MAX_PAGES_RECHERCHE: 30,

    // -----------------------------------------------------------------------
    // Utilitaires
    // -----------------------------------------------------------------------

    /** Espaces (y compris insécables) aux extrémités. */
    trim: function (s) {
        return String(s).replace(/^[\s ]+/, "").replace(/[\s ]+$/, "");
    },

    /** Minuscules sans accents : la recherche doit trouver « epreuve » sur « Épreuve ». */
    sansAccents: function (s) {
        var table = {
            "à": "a", "â": "a", "ä": "a", "á": "a", "ã": "a", "å": "a",
            "ç": "c", "è": "e", "é": "e", "ê": "e", "ë": "e",
            "î": "i", "ï": "i", "í": "i",
            "ô": "o", "ö": "o", "ó": "o", "õ": "o",
            "ù": "u", "û": "u", "ü": "u", "ú": "u",
            "ý": "y", "ÿ": "y", "ñ": "n"
        };
        return SOURCE.trim(s).toLowerCase().replace(/[àâäáãåçèéêëîïíôöóõùûüúýÿñ]/g, function (c) {
            return table[c];
        });
    },

    /** Entités HTML présentes dans les titres de ce thème. */
    decodeEntites: function (s) {
        // Mesuré le 2026-10-13 : le thème sert `&rsquo;` dans ses titres —
        // « Il n&rsquo;y a pas Assez D&rsquo;Hommes » — et la table ne le
        // prévoyait pas, donc l'entité s'affichait telle quelle. Ce sont les
        // entités typographiques que WordPress produit via `wptexturize`.
        var table = {
            "&amp;": "&", "&quot;": '"', "&#039;": "'", "&apos;": "'",
            "&laquo;": "«", "&raquo;": "»", "&hellip;": "…",
            "&nbsp;": " ", "&ndash;": "-", "&mdash;": "-",
            "&rsquo;": "’", "&lsquo;": "‘",
            "&rdquo;": "”", "&ldquo;": "“",
            "&thinsp;": " ", "&eacute;": "é", "&egrave;": "è",
            "&agrave;": "à", "&ccedil;": "ç", "&ugrave;": "ù"
        };
        var out = String(s);
        for (var cle in table) {
            if (table.hasOwnProperty(cle)) {
                out = out.split(cle).join(table[cle]);
            }
        }
        return out;
    },

    /** Retire le suffixe de site que Yoast ajoute dans `og:title`. */
    nettoyeTitre: function (s) {
        return String(s).replace(/\s*[-|]\s*Hentai[- ]Scantrad\s*$/i, "").trim();
    },

    // -----------------------------------------------------------------------
    // getList(page, query) → [{id, title, coverUrl, url}]
    // -----------------------------------------------------------------------

    urlListe: function (page) {
        return page > 1
            ? SOURCE.baseUrl + "/manga/page/" + page + "/"
            : SOURCE.baseUrl + "/manga/";
    },

    /** Extrait les fiches d'une page de catalogue. */
    parseListe: function (html) {
        // La coupe s'arrête sur `<div class="page-item-detail ` — l'espace final
        // est ce qui distingue une **carte** d'un **sélecteur CSS** : la feuille
        // de style du site contient `.page-item-detail .item-summary`, sans
        // `class="`, donc les deux formes ne se confondent pas.
        //
        // Mesuré le 2026-10-13 : couper sur `page-item-detail manga` ne
        // ramassait que **7** fiches sur 10. Le site classe 3 de ses séries en
        // `manhwa` et non `manga` — donc un tiers du catalogue disparaissait
        // sans le moindre message. Couper sur `page-item-detail` seul, sans
        // `class="`, aurait ramassé 15 : les 5 restantes sont du CSS.
        var morceaux = html.split('<div class="page-item-detail ');
        var out = [];
        for (var i = 1; i < morceaux.length; i++) {
            // Le bloc utile tient dans les premiers caractères : on borne
            // pour ne pas attraper la fiche voisine.
            var bloc = morceaux[i].substring(0, 1500);

            var ancre = bloc.match(
                /<a href="(https:\/\/hentai-scantrad\.org\/manga\/[^"?#]+)"[^>]*?title="([^"]*)"/
            );
            if (!ancre) {
                ancre = bloc.match(/<a href="(https:\/\/hentai-scantrad\.org\/manga\/[^"?#]+)"/);
                if (!ancre) continue;
            }

            var url = ancre[1];
            var titre;
            if (ancre[2]) {
                titre = SOURCE.decodeEntites(ancre[2]);
            } else {
                // Sans `title` : on dérive le titre du slug, en mots.
                titre = url.replace(/\/$/, "").split("/").pop().replace(/-/g, " ");
            }

            // `data-src`, jamais `src` : sur ce thème `src` vaut le
            // placeholder `themes/madara/images/dflazy.jpg`.
            var couverture = "";
            var mCov = bloc.match(/data-src="([^"]+)"/);
            if (mCov) couverture = SOURCE.trim(mCov[1]);

            out.push({ id: url, title: SOURCE.trim(titre), coverUrl: couverture, url: url });
        }
        return out;
    },

    getList: function (page, query) {
        if (!query || SOURCE.trim(query).length === 0) {
            return JSON.stringify(SOURCE.parseListe(http.get(SOURCE.urlListe(page))));
        }

        // Recherche : filtrage local du catalogue. Voir la note d'en-tête.
        var cible = SOURCE.sansAccents(query);
        var trouves = [];
        for (var p = 1; p <= SOURCE.MAX_PAGES_RECHERCHE && trouves.length < page * 24; p++) {
            var html;
            try {
                html = http.get(SOURCE.urlListe(p));
            } catch (e) {
                break; // fin du catalogue
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
    // getDetails(mangaUrl) → {title, description, coverUrl, status, …, chapters}
    // -----------------------------------------------------------------------

    getDetails: function (mangaUrl) {
        var url = mangaUrl.indexOf("http") === 0 ? mangaUrl : SOURCE.baseUrl + mangaUrl;
        var html = http.get(url);

        // Titre, description et couverture via les balises `og:` : mesurées
        // présentes sur chaque page de série, alors que les blocs
        // `.post-title` / `.post-content` sont ambigus sur ce thème.
        var mTitre = html.match(/property="og:title" content="([^"]*)"/);
        var mDesc = html.match(/property="og:description" content="([^"]*)"/);
        var mCov = html.match(/property="og:image" content="([^"]*)"/);
        if (!mTitre) {
            var mAlt = html.match(/<h1[^>]*>([^<]*)<\/h1>/);
            mTitre = mAlt ? mAlt : null;
        }

        // Statut : absent de la page. Le bloc `.post-status` y porte la date
        // de sortie, pas l'état de la série — mesuré. `unknown` est une valeur
        // autorisée par le contrat et dit vrai ; deviner « ongoing » serait
        // une affirmation sans preuve.
        var statut = "unknown";

        // Chapitres : endpoint AJAX du thème, PAS `admin-ajax.php`.
        var chapitres = [];
        var ajax = "";
        try {
            ajax = http.post(url + "ajax/chapters/", "");
        } catch (e) {
            ajax = "";
        }

        var reChap = /<a href="(https:\/\/hentai-scantrad\.org\/manga\/[^"?#]+\/chapitre-[^"?#]+\/)"[^>]*>([^<]*)<\/a>/g;
        var vus = {};
        var m;
        while ((m = reChap.exec(ajax)) !== null) {
            var chUrl = m[1];
            if (vus[chUrl]) continue; // l'ajax répète le lien de date de sortie
            vus[chUrl] = true;
            var libelle = SOURCE.trim(SOURCE.decodeEntites(m[2]));
            var num = 0;
            var mNum = chUrl.match(/chapitre-([0-9]+(?:\.[0-9]+)?)/);
            if (mNum) num = parseFloat(mNum[1]);
            chapitres.push({ id: chUrl, title: libelle, number: num, url: chUrl });
        }

        // Trié du plus ancien au plus récent — l'ordre du contrat. L'ajax rend
        // l'ordre du site (89, 88, 87…), donc on inverse par numéro croissant.
        chapitres.sort(function (a, b) { return a.number - b.number; });

        return JSON.stringify({
            // `og:title` vaut « Titre - Hentai-Scantrad » : on retire le
            // suffixe, sinon le catalogue affiche le nom du site sur chaque
            // fiche (mesuré).
            title: SOURCE.nettoyeTitre(mTitre ? SOURCE.trim(SOURCE.decodeEntites(mTitre[1])) : SOURCE.trim(url)),
            description: mDesc ? SOURCE.decodeEntites(mDesc[1]) : "",
            coverUrl: mCov ? SOURCE.trim(mCov[1]) : "",
            status: statut,
            authors: [],
            tags: [],
            chapters: chapitres
        });
    },

    // -----------------------------------------------------------------------
    // getPages(chapterUrl) → [{id, url}]
    // -----------------------------------------------------------------------

    getPages: function (chapterUrl) {
        var url = chapterUrl.indexOf("http") === 0 ? chapterUrl : SOURCE.baseUrl + chapterUrl;
        var html = http.get(url);

        // On ne prend que les images de classe `wp-manga-chapter-img` : les
        // bandeaux publicitaires du thème ont d'autres classes, et les
        // prendre reviendrait à afficher de la pub comme une page.
        var re = /<img[^>]*class="[^"]*wp-manga-chapter-img[^"]*"[^>]*>/g;
        var pages = [];
        var vus = {};
        var m;
        while ((m = re.exec(html)) !== null) {
            var balise = m[0];
            var mSrc = balise.match(/data-src="([^"]+)"/);
            if (!mSrc) continue;
            // `data-src` a ici une espace initiale : sans trim() l'URL
            // devient relative et l'image ne se charge pas.
            var src = SOURCE.trim(mSrc[1]);
            if (!src || vus[src]) continue;
            vus[src] = true;
            pages.push({ id: String(pages.length + 1), url: src });
        }
        return JSON.stringify(pages);
    }
};