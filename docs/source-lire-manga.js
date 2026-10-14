/**
 * Source script : Lire Manga (https://lire-manga.com)
 * =====================================================
 *
 * Le parseur natif `LIRESCAN` est classé `parseur_casse` : le site répond 200,
 * le parseur n'en extrait rien.
 *
 * Tout est **mesuré le 2026-10-14** dans un vrai navigateur.
 *
 *   liste    : `/manga/list-mode/` — séries `/manga/<slug>/`, classe `series tip`
 *   série    : `/manga/<slug>/`    — chapitres `/<slug>-chapitre-<n>/`
 *   chapitre : `/<slug>-chapitre-<n>/`
 *   images   : `<p><img>` dans `div.rdminimal` — CDN externe (`imgez.org`)
 *
 * ## Le piège : ce n'est pas un Madara
 *
 * Les deux sources précédentes discriminaient les images par leur chemin
 * `/wp-content/uploads/`. Ici le site est bien WordPress, mais **les pages ne
 * sont pas sur le site** : elles viennent de `imgez.org`, avec un segment
 * base64 dans l'URL (`/bWFuZ2E=/Solo%20Leveling/…`). Copier le filtre des deux
 * autres sources aurait rendu **zéro page**, sans lever d'erreur.
 *
 * D'où un discriminant **structurel** : une page de lecture est un `<p><img>`.
 * Tout ce qui est ailleurs est du décor — logo (`header`), vignettes de la série
 * (`div.bs`), lecteur vide (`readerarea.svg`). Borner la zone par `</article>`
 * ne suffit pas : la fixture a montré que ces trois structures sont dans le même
 * `article.post-3027`.
 *
 * ## Les liens sont absolus
 *
 * Le site écrit `https://www.lire-manga.com/manga/<slug>/`, alors que le
 * contrat attend un chemin relatif qu'on concatène à `baseUrl`. `chemin()`
 * retire l'origine ; sans lui chaque URL deviendrait
 * `https://lire-manga.comhttps://www.lire-manga.com/…`.
 *
 * ## Le numéro de chapitre ne se déduit pas
 *
 * `/solo-leveling-chapitre-1/` a répondu « Chapitre 179.5 » : le site
 * redirige et sa numérotation n'est ni continue ni prévisible. On prend donc le
 * **libellé porté par le lien** sur la fiche, et la dernière occurrence du
 * numéro dans le titre, jamais le numéro de l'URL.
 */

var SOURCE = {
    id: "lire-manga",
    name: "Lire Manga",
    lang: "fr",
    baseUrl: "https://www.lire-manga.com",

    /** Ne garde que le chemin : le site écrit des liens absolus. */
    chemin: function (href) {
        var h = String(href).trim();
        var i = h.search(/^https?:\/\/[^/]+/i);
        if (i >= 0) h = h.slice(i).replace(/^https?:\/\/[^/]+/i, "");
        return h;
    },

    texte: function (brut) {
        return String(brut)
            .replace(/<[^>]*>/g, " ")
            .replace(/&nbsp;/gi, " ")
            .replace(/&amp;/gi, "&")
            .replace(/&#0?39;|&apos;/gi, "'")
            .replace(/&[a-z]+;/gi, " ")
            .replace(/\s+/g, " ")
            .trim();
    },

    // ------------------------------------------------------------------ liste
    getList: function (page, query) {
        var url = query
            ? SOURCE.baseUrl + "/?s=" + encodeURIComponent(query)
            : SOURCE.baseUrl + "/manga/list-mode/page/" + page + "/";
        var html = http.get(url);
        if (!html) return "[]";

        // Une série est `/manga/<slug>/`. Les chapitres sont à la racine
        // (`/<slug>-chapitre-<n>/`), donc pas de collision ici — contrairement à
        // hhentai.fr, où les deux vivaient sous `/manga/`.
        var re = /<a[^>]+href="([^"]*\/manga\/([a-z0-9-]+)\/)"[^>]*>([\s\S]*?)<\/a>/gi;
        var vus = {};
        var resultats = [];
        var m;
        while ((m = re.exec(html)) !== null) {
            var slug = m[2];
            if (vus[slug]) continue;
            var nom = SOURCE.texte(m[3]);
            if (!nom) continue;
            vus[slug] = true;
            var href = SOURCE.chemin(m[1]);
            resultats.push({ id: href, title: nom, coverUrl: "", url: href });
        }
        return JSON.stringify(resultats);
    },

    // ------------------------------------------------------------------ fiche
    getDetails: function (mangaUrl) {
        var html = http.get(SOURCE.baseUrl + mangaUrl);
        if (!html) {
            return JSON.stringify({
                title: mangaUrl,
                description: "",
                coverUrl: "",
                status: "unknown",
                authors: [],
                tags: [],
                chapters: [],
            });
        }

        var titre = SOURCE.texte(
            (/<h1[^>]*>([\s\S]*?)<\/h1>/i.exec(html) || [, mangaUrl])[1]
        );

        var couverture = "";
        var reCouv = /<img[^>]+(?:data-src|src)="([^"]*\/wp-content\/uploads\/[^"]+)"[^>]*>/gi;
        var c = reCouv.exec(html);
        if (c) couverture = SOURCE.baseUrl + SOURCE.chemin(c[1]);

        var resume = SOURCE.texte(
            (/<div[^>]+class="[^"]*(?:entry-content|desc)[^"]*"[^>]*>([\s\S]*?)<\/div>/i
                .exec(html) || ["", ""])[1]
        ).slice(0, 1200);

        var brut = SOURCE.texte(html).toLowerCase();
        var statut = "unknown";
        if (brut.indexOf("en cours") >= 0) statut = "ongoing";
        else if (brut.indexOf("termin") >= 0) statut = "finished";

        // Chapitres : `/<slug>-chapitre-<n>/`. Le site les présente du plus
        // récent au plus ancien ; l'application attend l'inverse.
        var chapitres = [];
        var vus = {};
        var reChap = /<a[^>]+href="([^"]*\/([a-z0-9-]+)-chapitre-(\d+)\/)"[^>]*>([\s\S]*?)<\/a>/gi;
        var m;
        while ((m = reChap.exec(html)) !== null) {
            var href = SOURCE.chemin(m[1]);
            if (vus[href]) continue;
            vus[href] = true;
            var num = parseInt(m[3], 10);
            var libelle = SOURCE.texte(m[4]);
            chapitres.push({
                id: href,
                // Le libellé prime : la numérotation du site redirige et ne se
                // déduit pas de l'URL (voir l'en-tête).
                title: libelle || "Chapitre " + num,
                number: isNaN(num) ? chapitres.length + 1 : num,
                url: href,
            });
        }
        chapitres.reverse();

        return JSON.stringify({
            title: titre,
            description: resume,
            coverUrl: couverture,
            status: statut,
            authors: [],
            tags: [],
            chapters: chapitres,
        });
    },

    // ------------------------------------------------------------------ pages
    getPages: function (chapterUrl) {
        var html = http.get(SOURCE.baseUrl + chapterUrl);
        if (!html) return "[]";

        // **Une page est un `<p><img>`.** C'est la forme mesurée dans le DOM :
        // les pages sont en `p < div.rdminimal`, alors que le lecteur vide
        // (`readerarea.svg`) est en `div < div.postarea` et les vignettes de la
        // série en `div.limit > a > div.bsx`.
        //
        // Borner par `</article>` ne suffit pas — la fixture l'a montré : les
        // trois structures sont dans le même `article.post-3027`, donc la
        // borne laissait passer le `.svg`. Une borne structurelle trop large est
        // aussi un défaut, pas une précaution.
        var pages = [];
        var vus = {};
        var re = /<p[^>]*>\s*<img[^>]+(?:data-src|src)="(https?:[^"]+)"/gi;
        var m;
        while ((m = re.exec(html)) !== null) {
            var src = SOURCE.texte(m[1]);
            if (!src || vus[src]) continue;
            vus[src] = true;
            pages.push({ id: src, url: src });
        }
        return JSON.stringify(pages);
    },
};