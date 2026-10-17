/**
 * Niadd FR — source personnalisée Tatami (ADR 0010)
 * ==================================================
 * Remplace le parseur natif `NINEMANGA_FR` (hôte `fr.niadd.com`), classé
 * `PARSER_OUTDATED` le 2026-10-16 alors que le site répond `200` **sans**
 * Cloudflare. Les 7 autres « parseurs périmés » du relevé sont des domaines
 * revendus à un service de parking : eux ne se récupèrent pas.
 *
 * Toutes les formes ci-dessous sont **relevées** le 2026-10-16, aucune devinée.
 * Les mesures sont dans `.mesures/session-2026-10-16/25…44-niadd-*.cjs`.
 *
 *   catalogue  /list/New-Update.html
 *              <a title="TITRE" href="…/manga/<Slug>.html"><div class="manga-img"><img src="COUV">
 *   recherche  /search/?name=<requete>        (formulaire : action /search/, champ `name`)
 *   fiche      <h1 itemprop="name" class="book-headline-name">TITRE</h1>
 *              <span class="book-status"> (En cours) </span>
 *              <img itemprop="image" … src="COUVERTURE">
 *              genres : <span itemprop="genre">
 *              résumé : la fiche n'en affiche AUCUN — il n'existe que dans
 *                       <meta name="description" content="TITRE Manga details, …">
 *   chapitres  /manga/<Slug>/chapters.html  ← la fiche, elle, n'affiche que les
 *              4 derniers chapitres (mesuré : 4 sur la fiche, 425 sur cette
 *              page, pour « The Ghostly Doctor »)
 *              <a href="/chapter/…" title="The Ghostly Doctor 421">
 *   chapitre   /chapter/<Slug>_<n>/<id>/ puis les pages suivantes, données par
 *              `next_page = "/chapter/…-2.html"` (le site **donne** son
 *              pointeur : on le suit au lieu de fabriquer `-N.html`)
 *              images : https://wpimg.movietop.cc/…<hash>.webp?acc=…
 *
 * Trois pièges du site, chacun mesuré :
 *   1. les couvertures sont sous `/fr_manga/logo/…` — un dossier nommé `logo`
 *      qui contient les couvertures, pas la bannière. Sur une page de
 *      chapitre il faut donc les exclure (`/pic5/` est le dossier des pages) ;
 *   2. chaque page de chapitre répète l'image courante **6 fois** et annonce la
 *      suivante : sans dédoublonnage par chemin, chaque image sortirait deux
 *      fois, et avec un dédoublonnage par URL entière le jeton `?acc=` les
 *      ferait passer pour différentes ;
 *   3. `?page=2` renvoie **la même page** sur le catalogue, sur `chapters.html`
 *      et sur la recherche (mesuré : 304 730 o identiques). Rendre deux fois la
 *      même liste est un défaut déjà vu (sushiscan, 2026-10-14) : on rend donc
 *      `[]` au-delà de la page 1 plutôt que de dupliquer.
 *
 * ES5 uniquement : le moteur évalue ce fichier avec **Nashorn 15.6**, pas avec
 * V8. Une virgule finale dans un appel d'argument y est refusée (et acceptée par
 * Node) — `pnpm moteur:check` garde ce point.
 */

var SOURCE = {
    id: "niadd-fr",
    name: "Niadd FR",
    lang: "fr",
    baseUrl: "https://fr.niadd.com",

    /** Hôte des pages de chapitre. Les noms de fichiers y sont hachés. */
    HOTE_PAGES: "wpimg.movietop.cc",

    /** Nombre de pages de chapitre suivies au maximum (garde-fou, pas un compte). */
    PAGES_MAX: 300,

    /**
     * Le catalogue. Le site n'expose aucune pagination vérifiée : `page > 1`
     * rend une liste vide, jamais la page 1 recopiée.
     */
    getList: function (page, query) {
        var url;
        if (query && query.length > 0) {
            url = SOURCE.baseUrl + "/search/?name=" + encodeURIComponent(query);
        } else {
            if (page > 1) {
                return JSON.stringify([]);
            }
            url = SOURCE.baseUrl + "/list/New-Update.html";
        }

        var html = http.get(url);
        return SOURCE.extraireCartes(html);
    },

    /**
     * Les cartes du catalogue. La couverture est **dans la même ancre** que le
     * titre (mesuré), donc la lecture se fait en un seul motif ; la fenêtre de
     * 400 caractères évite d'apparier un titre avec une image lointaine.
     */
    extraireCartes: function (html) {
        var resultats = [];
        var vus = {};
        var re = /<a[^>]*title="([^"]{1,160})"[^>]*href="([^"]*\/manga\/[^"\/]+\.html)"[^>]*>[\s\S]{0,400}?<img[^>]*src="([^"]+)"/g;
        var m;
        while ((m = re.exec(html)) !== null) {
            var href = m[2];
            if (href.indexOf("http") !== 0) {
                href = SOURCE.baseUrl + href;
            }
            if (vus[href]) {
                continue;
            }
            vus[href] = true;
            resultats.push({
                id: href,
                title: m[1].trim(),
                coverUrl: m[3],
                url: href
            });
        }
        return JSON.stringify(resultats);
    },

    /**
     * La fiche : titre, statut, couverture, genres, résumé, et **la liste
     * complète des chapitres** lue sur `chapters.html`.
     */
    getDetails: function (mangaUrl) {
        var url = mangaUrl.indexOf("http") === 0 ? mangaUrl : SOURCE.baseUrl + mangaUrl;
        var html = http.get(url);

        var titre = "Inconnu";
        var tm = /<h1[^>]*itemprop="name"[^>]*>([^<]{1,200})<\/h1>/.exec(html);
        if (tm) {
            titre = tm[1].trim();
        }

        var cm = /<img[^>]*itemprop="image"[^>]*src="([^"]+)"/.exec(html);
        var coverUrl = cm ? cm[1] : "";

        var description = "";
        var dm = /<meta[^>]*name=["']?description["']?[^>]*content="([^"]*)"/i.exec(html);
        if (dm) {
            description = dm[1];
            // Le site prefixe le resume par « <TITRE> Manga details, ». On retire
            // exactement ce prefixe, et rien d'autre.
            var prefixe = titre + " Manga details, ";
            if (description.indexOf(prefixe) === 0) {
                description = description.substring(prefixe.length);
            }
        }

        var tags = [];
        var reTag = /<span[^>]*itemprop="genre"[^>]*>([^<]{1,60})<\/span>/g;
        var g;
        while ((g = reTag.exec(html)) !== null) {
            var tag = g[1].trim();
            if (tag.length > 0 && tags.indexOf(tag) < 0) {
                tags.push(tag);
            }
        }

        var sm = /<span[^>]*class="book-status"[^>]*>([\s\S]{0,60}?)<\/span>/.exec(html);
        var statut = "unknown";
        if (sm) {
            var s = sm[1].toLowerCase();
            if (s.indexOf("cours") >= 0) {
                statut = "ongoing";
            } else if (s.indexOf("termin") >= 0) {
                statut = "finished";
            } else if (s.indexOf("pause") >= 0) {
                statut = "paused";
            } else if (s.indexOf("abandon") >= 0) {
                statut = "abandoned";
            }
        }

        // La liste des chapitres vit sur une autre page que la fiche : celle-ci
        // n'affiche que les derniers. Le suffixe vient du site, il n'est pas
        // fabrique — il est mesure.
        var urlChapitres = url.replace(/\.html$/, "") + "/chapters.html";
        var htmlChapitres = "";
        try {
            htmlChapitres = http.get(urlChapitres);
        } catch (e) {
            htmlChapitres = "";
        }

        var chapitres = SOURCE.extraireChapitres(htmlChapitres);
        // Si la page des chapitres manquait, on se rabat sur ce que porte la
        // fiche : peu de chapitres, mais reels.
        if (chapitres.length === 0) {
            chapitres = SOURCE.extraireChapitres(html);
        }

        return JSON.stringify({
            title: titre,
            description: description,
            coverUrl: coverUrl,
            status: statut,
            authors: [],
            tags: tags,
            chapters: chapitres
        });
    },

    /**
     * Les ancres de chapitre, dédoublonnées par URL puis **triées du plus
     * ancien au plus récent** — le contrat l'exige, et le site liste l'inverse.
     *
     * Le numéro est le **dernier entier du `title`** : le site écrit aussi bien
     * « The Ghostly Doctor Ch. 0 » que « The Ghostly Doctor 421 ». Trier par ce
     * numéro, et non par position, protège d'une liste que le site réordonnerait.
     */
    extraireChapitres: function (html) {
        var bruts = [];
        var vus = {};
        var re = /<a[^>]*href="(\/chapter\/[^"]+)"[^>]*title="([^"]{0,160})"/g;
        var m;
        while ((m = re.exec(html)) !== null) {
            var href = m[1];
            if (vus[href]) {
                continue;
            }
            vus[href] = true;
            var titre = m[2].trim();
            var nombres = titre.match(/\d+/g);
            var numero = nombres ? parseInt(nombres[nombres.length - 1], 10) : bruts.length + 1;
            bruts.push({ id: href, title: titre, number: numero, url: href, ordre: bruts.length });
        }

        bruts.sort(function (a, b) {
            if (a.number !== b.number) {
                return a.number - b.number;
            }
            return a.ordre - b.ordre;
        });

        var chapitres = [];
        for (var i = 0; i < bruts.length; i++) {
            chapitres.push({
                id: bruts[i].id,
                title: bruts[i].title,
                number: bruts[i].number,
                url: bruts[i].url
            });
        }
        return chapitres;
    },

    /**
     * Les pages d'un chapitre. On suit `next_page`, que le site écrit dans la
     * page : c'est lui qui sait où s'arrêter, pas une formule `-N.html`.
     */
    getPages: function (chapterUrl) {
        var url = chapterUrl.indexOf("http") === 0 ? chapterUrl : SOURCE.baseUrl + chapterUrl;

        var pages = [];
        var vues = {};
        var vuesHtml = {};
        var suivante = url;

        for (var i = 0; i < SOURCE.PAGES_MAX && suivante; i++) {
            if (vuesHtml[suivante]) {
                break;
            }
            vuesHtml[suivante] = true;

            var html;
            try {
                html = http.get(suivante);
            } catch (e) {
                break;
            }
            if (!html) {
                break;
            }

            var trouvees = 0;
            var re = new RegExp("https?://" + SOURCE.HOTE_PAGES.replace(/\./g, "\\.") + "/[^\"'\\s>]*?\\.webp[^\"'\\s>]*", "g");
            var m;
            while ((m = re.exec(html)) !== null) {
                var img = m[0];
                // `/logo/` porte les couvertures de series, servies par le meme
                // hote : sans cette exclusion, la couverture devient la 1re page.
                if (img.indexOf("/logo/") >= 0) {
                    continue;
                }
                // Dedoublonnage par **chemin nu** : le jeton `?acc=` change
                // d'une occurrence a l'autre, l'image non.
                var chemin = img.split("?")[0];
                if (vues[chemin]) {
                    continue;
                }
                vues[chemin] = true;
                trouvees++;
                pages.push({ id: String(pages.length), url: img });
            }

            var np = /next_page\s*=\s*["']([^"']+)["']/.exec(html);
            var lienSuivant = np ? np[1] : "";
            // Une page sans image nouvelle est la derniere : c'est le seul signal
            // de fin verifie, et il ne depend d'aucun nombre devine.
            if (trouvees === 0) {
                break;
            }
            suivante = lienSuivant ? (lienSuivant.indexOf("http") === 0 ? lienSuivant : SOURCE.baseUrl + lienSuivant) : "";
        }

        return JSON.stringify(pages);
    }
};
