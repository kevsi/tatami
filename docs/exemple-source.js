/**
 * Exemple de source personnalisée Tatami (ADR 0010)
 * ===================================================
 * Copiez ce fichier, renommez-le (ex: mon-site.js), adaptez-le à votre site,
 * puis importez-le depuis l'écran "Sources perso" de Tatami.
 *
 * API disponible dans le script :
 *   http.get(url)           → String (corps de la réponse)
 *   http.post(url, body)    → String (corps de la réponse)
 *
 * Règles :
 *   - Uniquement des URLs https://
 *   - Taille maximale du script : 256 Ko
 *   - Retourner des valeurs JSON sérialisées en String (JSON.stringify)
 *   - Les méthodes peuvent être synchrones (http.get est synchrone)
 */

var SOURCE = {
    // -------------------------------------------------------------------------
    // Métadonnées (obligatoires)
    // -------------------------------------------------------------------------

    /** Identifiant unique : lettres minuscules, chiffres, tirets. */
    id: "exemple-fr",

    /** Nom affiché dans Tatami. */
    name: "Exemple Source FR",

    /** Code de langue BCP-47 : "fr", "en", "ja", "ko"… */
    lang: "fr",

    /** URL de base du site. */
    baseUrl: "https://exemple-site.fr",

    // -------------------------------------------------------------------------
    // getList(page, query) → JSON.stringify([{id, title, coverUrl, url}, ...])
    //
    // page  : numéro de page à partir de 1
    // query : chaîne de recherche, vide si liste générale
    //
    // Retourner un tableau vide [] quand il n'y a plus de résultats.
    // -------------------------------------------------------------------------
    getList: function(page, query) {
        var url;
        if (query && query.length > 0) {
            url = SOURCE.baseUrl + "/recherche?q=" + encodeURIComponent(query) + "&page=" + page;
        } else {
            url = SOURCE.baseUrl + "/manga-list?page=" + page;
        }

        var html = http.get(url);

        // Exemple d'extraction HTML naïve avec des regex.
        // Pour de vraies pages, adaptez les patterns à la structure du site.
        var resultats = [];
        var re = /<a class="manga-card" href="([^"]+)"[^>]*>.*?<img src="([^"]+)"[^>]*>.*?<span class="title">([^<]+)<\/span>/gs;
        var m;
        while ((m = re.exec(html)) !== null) {
            var href = m[1];
            resultats.push({
                id:       href,
                title:    m[3].trim(),
                coverUrl: m[2],
                url:      href,
            });
        }

        return JSON.stringify(resultats);
    },

    // -------------------------------------------------------------------------
    // getDetails(mangaUrl) → JSON.stringify({title, description, coverUrl,
    //                          status, authors, tags, chapters})
    //
    // status : "ongoing" | "finished" | "paused" | "abandoned" | "unknown"
    // authors : tableau de strings
    // tags    : tableau de strings
    // chapters : [{id, title, number, url}]  — triés du plus vieux au plus récent
    // -------------------------------------------------------------------------
    getDetails: function(mangaUrl) {
        var url = SOURCE.baseUrl + mangaUrl;
        var html = http.get(url);

        // --- Titre ---
        var titreMatch = /<h1 class="manga-title">([^<]+)<\/h1>/.exec(html);
        var titre = titreMatch ? titreMatch[1].trim() : "Inconnu";

        // --- Description ---
        var descMatch = /<div class="synopsis">([^<]+)<\/div>/.exec(html);
        var description = descMatch ? descMatch[1].trim() : "";

        // --- Couverture ---
        var coverMatch = /<img class="cover" src="([^"]+)"/.exec(html);
        var coverUrl = coverMatch ? coverMatch[1] : "";

        // --- Statut ---
        var statutMatch = /<span class="status">([^<]+)<\/span>/.exec(html);
        var statut = "unknown";
        if (statutMatch) {
            var s = statutMatch[1].toLowerCase();
            if (s.indexOf("cours") >= 0) statut = "ongoing";
            else if (s.indexOf("terminé") >= 0) statut = "finished";
            else if (s.indexOf("pause") >= 0) statut = "paused";
        }

        // --- Chapitres ---
        var chapitres = [];
        var reChap = /<a class="chapter-link" href="([^"]+)"[^>]*>([^<]+)<\/a>/g;
        var num = 1;
        var c;
        while ((c = reChap.exec(html)) !== null) {
            var chapHref = c[1];
            chapitres.push({
                id:     chapHref,
                title:  c[2].trim(),
                number: num++,
                url:    chapHref,
            });
        }

        return JSON.stringify({
            title:       titre,
            description: description,
            coverUrl:    coverUrl,
            status:      statut,
            authors:     [],
            tags:        [],
            chapters:    chapitres,
        });
    },

    // -------------------------------------------------------------------------
    // getPages(chapterUrl) → JSON.stringify([{id, url}, ...])
    //
    // url : URL directe de l'image (doit commencer par https://)
    // -------------------------------------------------------------------------
    getPages: function(chapterUrl) {
        var url = SOURCE.baseUrl + chapterUrl;
        var html = http.get(url);

        // Beaucoup de sites embarquent la liste des pages dans un tableau JS.
        // Exemple : var pages = ["001.jpg", "002.jpg", ...];
        var pagesMatch = /var pages\s*=\s*(\[[^\]]+\])/.exec(html);
        if (pagesMatch) {
            var liste = JSON.parse(pagesMatch[1]);
            var cdn = SOURCE.baseUrl + "/img" + chapterUrl + "/";
            return JSON.stringify(liste.map(function(nom, i) {
                return { id: nom, url: cdn + nom };
            }));
        }

        // Fallback : images directement dans le HTML.
        var pages = [];
        var re = /<img class="page-img" src="(https:\/\/[^"]+)"/g;
        var m;
        while ((m = re.exec(html)) !== null) {
            pages.push({ id: m[1], url: m[1] });
        }
        return JSON.stringify(pages);
    },
};
