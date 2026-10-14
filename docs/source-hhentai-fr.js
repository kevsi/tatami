/**
 * Source script : Histoire d'Hentai (https://hhentai.fr)
 * =====================================================
 *
 * Le parseur natif `HHENTAIFR` est classé `parseur_casse` dans
 * `docs/mesures/sites-etat-normalise.json` : le site répond, le parseur n'en
 * extrait rien.
 *
 * Tout ce qui suit est **mesuré le 2026-10-14** dans un vrai navigateur, sur les
 * trois pages du site. Aucun sélecteur n'est deviné.
 *
 *   série    : `/manga/<slug>/`
 *   chapitre : `/manga/<slug>/chapitre-<n>/`
 *   `<h1>` de la fiche   : "MILF Exchange Plan"
 *   `<h1>` du chapitre   : "Chapitre 87"
 *   chapitres listés sous la classe `wp-manga-chapter` (thème WP-Manga)
 *   images de lecture    : `.reading-content .page-break img`
 *   logo                  : `div.logo`, **hors** de `.reading-content`
 *
 * ## Le discriminant des images
 *
 * Les pages vivent sur un R2 (`pub-….r2.dev/wp-content/uploads/…`), pas sur le
 * site lui-même. Retenir ces URLs suffit à écarter le logo, les vignettes de la
 * série et les icônes — qui sont sur le même domaine. Sans ce filtre, la
 * couverture du site deviendrait la première « page » du chapitre.
 *
 * ## `data-src` compte autant que `src`
 *
 * WP-Manga charge les pages en différé, et un `src` vide donnerait une image
 * cassée **sans aucun message** — exactement le défaut que le projet a déjà
 * encountering chez les parsers natifs.
 *
 * ## Ordre des chapitres
 *
 * Le site présente du plus récent au plus ancien ; l'application attend
 * l'inverse. `getDetails` inverse donc la liste avant de la rendre.
 */

var SOURCE = {
    id: "hhentai-fr",
    name: "Histoire d'Hentai",
    lang: "fr",
    baseUrl: "https://hhentai.fr",

    // Le chemin des uploads sur le R2 : c'est lui qui distingue une image de
    // contenu d'une image de décor, toutes deux sur le même domaine.
    UPLOADS: "/wp-content/uploads/",

    /** Texte d'un fragment HTML : entités, balises, espaces superflus. */
    texte: function (brut) {
        return String(brut)
            .replace(/<[^>]*>/g, " ")
            .replace(/&nbsp;/gi, " ")
            .replace(/&amp;/gi, "&")
            .replace(/&(?:#0?39|apos);/gi, "'")
            .replace(/&quot;/gi, '"')
            .replace(/&[a-z]+;/gi, " ")
            .replace(/\s+/g, " ")
            .trim();
    },

    // ------------------------------------------------------------------ liste
    getList: function (page, query) {
        var url = query
            ? SOURCE.baseUrl + "/recherche/?q=" + encodeURIComponent(query) + "/page/" + page + "/"
            : SOURCE.baseUrl + "/manga/?page=" + page + "/";
        var html = http.get(url);
        if (!html) return "[]";

        // `href="https://hhentai.fr/manga/<slug>/"` : le site sert des URL
        // **absolues**. Une regex qui exige `href="/manga/…"` ne trouve rien,
        // sans lever d'erreur. `[^"]*` tolère les deux formes ; le chemin est
        // reconstruit, parce que le contrat concatène `baseUrl`.
        var re = new RegExp(
            '<a[^>]+href="[^"]*\\/manga\\/([a-z0-9-]+)\\/"[^>]*>([\\s\\S]*?)</a>',
            "gi"
        );
        var vus = {};
        var resultats = [];
        var m;
        while ((m = re.exec(html)) !== null) {
            var slug = m[1];
            // Un chapitre `/manga/<slug>/chapitre-3/` ne matche pas ci-dessus :
            // `[a-z0-9-]+` s'arrête à la barre et le motif exige la fin du
            // guillemet. L'exclusion reste, par sécurité.
            if (slug.indexOf("chapitre") >= 0) continue;
            if (vus[slug]) continue;
            var nom = SOURCE.texte(m[2]);
            if (!nom) continue;
            vus[slug] = true;
            var href = "/manga/" + slug + "/";
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
        var reImg = new RegExp(
            '<img[^>]+(?:src|data-src)="([^"]+' + SOURCE.UPLOADS + '[^"]+)"',
            "gi"
        );
        var i = reImg.exec(html);
        if (i) couverture = i[1];

        var resume = SOURCE.texte(
            (/<div[^>]+class="[^"]*summary[^"]*"[^>]*>([\s\S]*?)<\/div>/i.exec(html) || [
                "",
                "",
            ])[1]
        ).slice(0, 1200);

        var brut = SOURCE.texte(html).toLowerCase();
        var statut = "unknown";
        if (brut.indexOf("en cours") >= 0 || brut.indexOf("ongoing") >= 0) statut = "ongoing";
        else if (brut.indexOf("termin") >= 0) statut = "finished";
        else if (brut.indexOf("en pause") >= 0) statut = "paused";

        var chapitres = [];
        var vus = {};
        var reChap = new RegExp(
            // URL absolue tolérée : le site n'émet que des liens préfixés du
            // domaine. Le groupe 1 reste le **chemin**, que le contrat attend.
            '<a[^>]+href="[^"]*(\\/manga\\/[a-z0-9-]+\\/chapitre-\\d+\\/)[^"]*"[^>]*>([\\s\\S]*?)</a>',
            "gi"
        );
        var c;
        while ((c = reChap.exec(html)) !== null) {
            if (vus[c[1]]) continue;
            vus[c[1]] = true;
            var num = parseInt(/chapitre-(\d+)/.exec(c[1])[1], 10);
            var libelle = SOURCE.texte(c[2]);
            chapitres.push({
                id: c[1],
                title: libelle || "Chapitre " + num,
                number: num,
                url: c[1],
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

        // On ne garde que le bloc de lecture : ailleurs la page porte le logo
        // et les vignettes de la série, qui ne sont pas des pages.
        var bloc = new RegExp(
            '<div[^>]+class="[^"]*reading-content[^"]*"[\\s\\S]*?(?=<div[^>]+class="[^"]*(?:comment|related|share))',
            "i"
        ).exec(html);
        var zone = bloc ? bloc[0] : html;

        var pages = [];
        var vus = {};
        var re = /<img[^>]+(?:data-src|src)="(https:[^"]+)"/gi;
        var m;
        while ((m = re.exec(zone)) !== null) {
            var src = m[1];
            if (src.indexOf(SOURCE.UPLOADS) < 0) continue;
            if (vus[src]) continue;
            vus[src] = true;
            pages.push({ id: src, url: src });
        }
        return JSON.stringify(pages);
    },
};
