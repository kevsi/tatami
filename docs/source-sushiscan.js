// Source personnalisée Sushiscan — https://sushiscan.net
//
// ## Ce que le site sert réellement (mesuré le 2026-10-14)
//
// Catalogue : `/manga/` répond 200 avec 770 856 o et **65** ancres de série,
// toutes en URL **absolue** : `https://sushiscan.net/catalogue/<slug>/`.
// Le titre est dans l'attribut `title`, la couverture dans l'<img> **imbriqué**
// dans l'ancre (le texte interne ne porte que le badge de type).
//
// Fiche : une couverture `itemprop="image"` — la première image
// `/wp-content/uploads/` de la page est le logo du site (`sushiscanlogo.webp`,
// 195x50), comme sur lire-manga. Les chapitres sont dans `#chapterlist`, en
// `<li data-num="…">` avec un `<span class="chapternum">`, et leurs URL sont
// `/<slug>-volume-<n>/`.
//
// ## Le piège de ce site : le chapitre ne contient AUCUNE image
//
// `https://sushiscan.net/sawako-volume-30/` (758 060 o) ne contient que deux
// `<img>` : le logo et un SVG de décor. Le `<div id="readerarea">` est **vide**,
// et un `#readerarea-loading` attend. Mesuré : 0 `data-src`, 0 tableau
// JavaScript d'images, la seule `<iframe>` étant une publicité.
// Le lecteur est donc construit en JavaScript… mais **le HTML porte quand même
// tout** : un `ts_reader.run({…})` en fin de page, dont le champ `sources` porte
// les URL d'images, échappées (`https:\/\/c1.sushiscan.net\/…`).
//
// C'est ce que cette source lit. Une source qui ne voyait que les `<img>`
// rendrait 0 page — silencieusement, comme toujours.
//
// ## Ce qui n'est PAS mesuré
//
// La pagination au-delà de la page 1 : `/manga/?page=2/` suit la convention
// WordPress du thème, mais seule la page 1 a été ouverte (65 séries d'un bloc).
// De même, `sources` peut contenir plusieurs serveurs : on les concatène dans
// l'ordre servi, sans deduplication inter-serveurs.
var SOURCE = {
    id: "sushiscan",
    name: "Sushiscan",
    lang: "fr",
    baseUrl: "https://sushiscan.net",

    /** Retire les balises et les entités usuelles. */
    texte: function (brut) {
        return String(brut)
            .replace(/<[^>]*>/g, " ")
            .replace(/&nbsp;/gi, " ")
            .replace(/&amp;/gi, "&")
            .replace(/&(?:#0?39|apos|rsquo|lsquo);/gi, "'")
            .replace(/&[a-z]+;/gi, " ")
            .replace(/\s+/g, " ")
            .trim();
    },

    /** Le contrat concatène `baseUrl` : on ne stocke que le chemin. */
    chemin: function (absolu) {
        var u = String(absolu);
        var i = u.indexOf("://");
        if (i >= 0) {
            var j = u.indexOf("/", i + 3);
            return j >= 0 ? u.slice(j) : "/";
        }
        return u.charAt(0) === "/" ? u : "/" + u;
    },

    // ------------------------------------------------------------------ liste
    getList: function (page, query) {
        var url = query
            ? SOURCE.baseUrl + "/?s=" + encodeURIComponent(query)
            : SOURCE.baseUrl + "/manga/?page=" + page + "/";
        var html = http.get(url);
        if (!html) return "[]";

        // `[^"]*` devant `/catalogue/` tolère l'URL absolue et la relative ;
        // les liens de genre sont `/genres/…`, donc ils ne tombent pas ici.
        var re = /<a([^>]*href="[^"]*\/catalogue\/([^"]+)\/")([^>]*)>([\s\S]*?)<\/a>/gi;
        var vus = {};
        var resultats = [];
        var m;
        while ((m = re.exec(html)) !== null) {
            var slug = SOURCE.texte(m[2]);
            if (!slug || vus[slug]) continue;
            var attrs = m[1] + " " + m[3];
            var attrTitre = /title="([^"]*)"/i.exec(attrs);
            // Mesuré le 2026-10-14 : la grille `/manga/` porte **19 séries
            // distinctes**, mais seulement **5** ont une `<img>` imbriquée —
            // les 14 autres n'ont qu'un titre. On ne les jette donc pas : une
            // série sans couverture vaut mieux qu'une série absente, et le
            // journal dira `URL vide` pour elle, ce qui est exact.
            var img = /<img[^>]+src="([^"]+)"/i.exec(m[4]);
            var nom = SOURCE.texte(attrTitre ? attrTitre[1] : m[4]);
            if (!nom) continue;
            vus[slug] = true;
            var href = "/catalogue/" + slug + "/";
            // La couverture est l'image imbriquée dans l'ancre. Le `?ver=` de
            // WordPress est conservé tel quel : c'est un cache-buster du site.
            resultats.push({
                id: href,
                title: nom,
                coverUrl: img ? SOURCE.texte(img[1]) : "",
                url: href,
            });
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
        var imgCouv = /<img[^>]*itemprop="image"[^>]*>/i.exec(html);
        if (imgCouv) {
            var u = /src=["']([^"']+)["']/i.exec(imgCouv[0]);
            if (u) couverture = u[1].trim();
        }

        var resume = SOURCE.texte(
            (/<div[^>]+class="[^"]*(?:entry-content|chdesc)[^"]*"[^>]*>([\s\S]*?)<\/div>/i
                .exec(html) || ["", ""])[1]
        ).slice(0, 1200);

        var brut = SOURCE.texte(html).toLowerCase();
        var statut = "unknown";
        if (brut.indexOf("en cours") >= 0 || brut.indexOf("ongoing") >= 0) statut = "ongoing";
        else if (brut.indexOf("termin") >= 0) statut = "finished";

        // `#chapterlist` borne la liste : ailleurs la page propose des
        // « chapitres » d'autres séries dans ses widgets.
        var bloc = /<div[^>]*id="chapterlist"[\s\S]*?<\/ul>/i.exec(html);
        var zone = bloc ? bloc[0] : "";
        var chapitres = [];
        var vus = {};
        var reChap =
            /<li[^>]*data-num="([^"]*)"[\s\S]*?<a[^>]*href="([^"]*)"[\s\S]*?<span[^>]*class="chapternum"[^>]*>([\s\S]*?)<\/span>/gi;
        var m;
        while ((m = reChap.exec(zone)) !== null) {
            var href = SOURCE.chemin(m[2]);
            if (vus[href]) continue;
            vus[href] = true;
            var libelle = SOURCE.texte(m[3]);
            chapitres.push({
                id: href,
                // Le libellé prime : « Volume 30 » ne se déduit pas de l'URL.
                title: libelle || SOURCE.texte(m[1]),
                number: 0,
                url: href,
            });
        }
        // Le site présente du plus récent au plus ancien ; l'application attend
        // l'inverse. **La numérotation est attribuée APRÈS l'inversion** :
        // mesuré le 2026-10-14, la numéroter avant donnait « Volume 1 » en
        // position 1 mais portant `number: 30` — une série dont l'ordre et le
        // numéro se contredisent, ce que rien ne signale.
        chapitres.reverse();
        for (var i = 0; i < chapitres.length; i++) {
            var n = parseInt(chapitres[i].title, 10);
            chapitres[i].number = isNaN(n) ? i + 1 : n;
        }

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

        // **Les pages ne sont pas dans les `<img>`** : le chapitre n'en compte
        // que deux (logo + SVG). Elles sont dans le `ts_reader.run({…})` que le
        // lecteur s'injecte, sous `sources[].images`, avec les barrets
        // échappés (`https:\/\/…`). On lit donc le tableau, pas le DOM.
        var pages = [];
        var vus = {};
        var reImg = /"images"\s*:\s*\[([^\]]*)\]/g;
        var bloc;
        while ((bloc = reImg.exec(html)) !== null) {
            var liste = bloc[1].split(/","/);
            for (var i = 0; i < liste.length; i++) {
                var src = SOURCE.texte(liste[i].replace(/^"|"$/g, "").replace(/\\\//g, "/"));
                if (!/^https?:\/\/\S+\.(?:jpe?g|png|webp)$/i.test(src)) continue;
                if (vus[src]) continue;
                vus[src] = true;
                pages.push({ id: src, url: src });
            }
        }
        return JSON.stringify(pages);
    },
};
