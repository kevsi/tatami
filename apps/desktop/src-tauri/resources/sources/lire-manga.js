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
        // Mesuré le 2026-10-14 : `/manga/list-mode/page/1/` répond **404**
        // (225 241 o de page d'erreur), alors que `/manga/` répond 200 avec
        // 297 100 o et 122 liens distincts. La pagination est `/manga/page/<n>/`,
        // et `/manga/list-mode/` n'est qu'un lien de menu.
        var url = query
            ? SOURCE.baseUrl + "/?s=" + encodeURIComponent(query)
            : SOURCE.baseUrl + "/manga/page/" + page + "/";
        var html = http.get(url);
        if (!html) return "[]";

        // Une série est `/manga/<slug>/`. Les chapitres sont à la racine
        // (`/<slug>-chapitre-<n>/`), donc pas de collision ici — contrairement à
        // hhentai.fr, où les deux vivaient sous `/manga/`.
        //
        // Le **titre est dans l'attribut `title`**, pas dans le texte de l'ancre.
        // Mesuré le 2026-10-14 : l'ancre contient les badges de statut et de
        // couleur, donc `SOURCE.texte(interieur)` rendait « Completed Color »
        // pour **les 43 séries** — un titre identique et faux, sans lever la
        // moindre erreur. L'ancre porte bien `title="100% (Pope)"`.
        var re = /<a([^>]*href="[^"]*\/manga\/([a-z0-9-]+)\/")([^>]*)>([\s\S]*?)<\/a>/gi;
        var vus = {};
        var resultats = [];
        var m;
        while ((m = re.exec(html)) !== null) {
            var slug = m[2];
            // **Les liens de navigation vivent aussi sous `/manga/`.** Mesuré le
            // 2026-10-14 sur `/manga/page/1/` : le catalogue annonce 44 liens,
            // dont trois qui ne sont pas des séries — `/manga/list-mode/`
            // (« Manga VF »), `/manga/page/` et `/manga/feed/`. Le premier de la
            // liste était donc un menu, et il aurait été la « série » affichée
            // en tête de grille.
            if (slug === "list-mode" || slug === "page" || slug === "feed") continue;
            if (vus[slug]) continue;
            var attrs = m[1] + " " + m[3];
            var attrTitre = /title="([^"]*)"/i.exec(attrs);
            var attrHref = /href="([^"]*)"/i.exec(m[1]);
            if (!attrHref) continue;
            var nom = SOURCE.texte(attrTitre ? attrTitre[1] : m[4]);
            if (!nom) continue;
            vus[slug] = true;
            var href = SOURCE.chemin(attrHref[1]);
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
        // Mesuré le 2026-10-14 : la **première** image `/wp-content/uploads/`
        // de la fiche est le logo du site (`lire-manga-com.png`, 195x50), et la
        // fiche annonçait donc le logo comme couverture. Le marqueur est
        // sémantique : `itemprop="image"`. La couverture de la série est en
        // 400x531 ; celle du widget de recommandation est en 150x228 et n'a pas
        // `itemprop`.
        var reCouv = /<img[^>]*itemprop="image"[^>]*>/i;
        var c = reCouv.exec(html);
        if (c) {
            var u = /src=["']([^"']+)["']/i.exec(c[0]);
            if (u) couverture = SOURCE.baseUrl + SOURCE.chemin(SOURCE.texte(u[1]));
        }

        var resume = SOURCE.texte(
            (/<div[^>]+class="[^"]*(?:entry-content|desc)[^"]*"[^>]*>([\s\S]*?)<\/div>/i
                .exec(html) || ["", ""])[1]
        ).slice(0, 1200);

        var brut = SOURCE.texte(html).toLowerCase();
        var statut = "unknown";
        if (brut.indexOf("en cours") >= 0) statut = "ongoing";
        else if (brut.indexOf("termin") >= 0) statut = "finished";

        // Chapitres : mesurés le 2026-10-14 sur `/manga/100-pope/`, ils vivent
        // dans `#chapterlist`, en `<li data-num="…">` avec un
        // `<span class="chapternum">`, et leurs URL sont `/<slug>-vol-<n>/` —
        // **pas** `/<slug>-chapitre-<n>/`.
        //
        // L'ancien motif `-chapitre-(\d+)` attrapait une recommandation d'une
        // **autre série** : la fiche de « 100% (Pope) » annonçait 1 chapitre,
        // `/sex-criminals-chapitre-3/`. Un chapitre sans erreur, et pas le bon.
        var blocListe = /<div[^>]*id="chapterlist"[\s\S]*?<\/ul>/i.exec(html);
        var zoneChap = blocListe ? blocListe[0] : "";
        var chapitres = [];
        var vus = {};
        var reChap =
            /<li[^>]*data-num="([^"]*)"[\s\S]*?<a[^>]*href="([^"]*)"[\s\S]*?<span[^>]*class="chapternum"[^>]*>([\s\S]*?)<\/span>/gi;
        var m;
        while ((m = reChap.exec(zoneChap)) !== null) {
            var href = SOURCE.chemin(m[2]);
            if (vus[href]) continue;
            vus[href] = true;
            var libelle = SOURCE.texte(m[3]);
            var num = parseInt(libelle, 10);
            chapitres.push({
                id: href,
                // Le libellé prime : la numérotation du site redirige et ne se
                // déduit pas de l'URL (voir l'en-tête).
                title: libelle || m[1],
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