/**
 * Hentai-Scantrad — source personnalisée Tatami (ADR 0010)
 * ========================================================
 * Reprend `HENTAISCANTRAD` (hôte `hentai-scantrad.org`), classé
 * `PARSER_OUTDATED` le 2026-10-16. Le site répond `200` **sans** Cloudflare —
 * c'est la deuxième des trois sources récupérables du tri (après Niadd FR, dont
 * le tri a montré que les 7 autres « parseurs périmés » sont des domaines morts).
 *
 * Toutes les formes ci-dessous sont **relevées** le 2026-10-16, aucune devinée.
 * Les mesures sont dans `.mesures/session-2026-10-16/50…65-scantrad-*.cjs`.
 *
 * Le site est un WordPress + thème **Madara** (`post_type=wp-manga`).
 *
 *   catalogue  /                 puis /page/<N>/        (24 cartes par page,
 *                                                        35 pages mesurées)
 *              <a href="…/manga/<slug>/" title="TITRE"> … <img … data-src="COUV">
 *   recherche  /?s=<requete>&post_type=wp-manga&paged=<N>
 *   fiche      <h1>TITRE</h1>
 *              <div class="summary_image"><a …><img … data-src="COUVERTURE">
 *              genres  : <a href="…/manga-genre/<slug>/">
 *              auteurs : <a href="…/manga-author/<slug>/"> et `manga-artist`
 *              résumé  : <div class="description-summary"> … <p>…</p>
 *              statut  : le bloc info intitulé « État » (« En Cours », …)
 *   chapitres  POST <fiche>ajax/chapters/  (corps vide)
 *              <li class="wp-manga-chapter"><a href="…/chapitre-315/">Chapitre 315</a>
 *   chapitre   <div class="page-break no-gaps"><img id="image-0" data-src="URL">
 *
 * Six pièges mesurés, chacun payé dans la séance :
 *
 *   1. **`manga_get_chapters` n'existe pas.** L'action Ajax « évidente » du thème
 *      WP-Manga répond `400` avec un corps de 1 octet (« 0 ») — c'est le refus de
 *      WordPress pour une action non enregistrée. Le vrai chargeur a été **lu**
 *      dans `madara-core/assets/js/script.js` :
 *      `POST {manga.base_url}ajax/chapters/`, et `manga.base_url` **contient déjà
 *      le slug** (`https://hentai-scantrad.org/manga/secret-class/`). La fiche,
 *      elle, ne contient aucune ancre de chapitre : seulement
 *      `<div id="manga-chapters-holder" data-id="4478">` et un spinner.
 *   2. **Deux formes d'URL de chapitre.** `chapitre-316/` pour les numéros ≥ 83,
 *      et `ch-N/` (« Ch.1 », ancien nommage) pour les ≤ 82. Un motif qui exige
 *      `chapitre-` ne voit que 235 des 316 chapitres — et « 235 » est un nombre
 *      plausible, donc l'erreur ne se voit pas. Le numéro est donc lu avec les
 *      deux préfixes, et `getDetails` compte les deux formes.
 *   3. **L'URL d'image est précédée d'une espace** : `data-src=" https://…"`.
 *      Déjà vu sur hhentai-fr et bananascans ; un motif qui exige `"https` ne
 *      mord rien, et le chapitre rend 0 page. D'où le `trim()` systématique.
 *   4. **`&page=<N>` est ignoré par la recherche** : il rend la page 1 à
 *      l'identique. C'est `&paged=<N>` qui pagine (mesuré : `&page=2` rend les
 *      mêmes 10 séries que la page 1, `&paged=2` en rend 10 autres) — le défaut
 *      « page 2 = page 1 » déjà vu sur sushiscan et sur Niadd.
 *   5. **`og:image` n'est pas la couverture affichée.** Mesuré sur
 *      `secret-class` : `og:image` est une jpg de **2020**, alors que
 *      `.summary_image` sert une webp de **2026** (`hentai-hd-4478-…-master`).
 *      On prend donc l'image du conteneur que le site affiche, `og:image`
 *      seulement en repli.
 *   6. **Un groupe de capture dans un lookahead disparaît sous Nashorn.** La
 *      première version extrayait titre et URL d'un seul motif à
 *      pré-vérification `(?=…)`. V8 (le harnais hors ligne) la validait — 24
 *      cartes — et le moteur refusait net :
 *      `TypeError: Cannot read property "indexOf" from undefined`. Le lookahead
 *      est donc banni ici, remplacé par une lecture de l'ancre puis un filtre
 *      sur le `title`. `pnpm moteur:check` ne voit pas ce cas : seule la sonde
 *      profonde (le vrai Nashorn) le voit.
 *
 * Le choix de la liste est mesuré, pas préféré : l'**accueil** `/page/<N>/` rend
 * 24 cartes par page sur 35 pages (≈ 840 séries), l'**archive** `/manga/page/<N>/`
 * en rend 10 sur 83 pages (≈ 830 séries) et sa page 1 est un sous-ensemble de
 * l'accueil. Même catalogue des deux côtés, donc l'accueil gagne : il demande
 * 2,4 fois moins de requêtes. C'est aussi la sémantique de la source de
 * référence Niadd FR, qui liste les dernières mises à jour.
 *
 * ES5 uniquement : le moteur évalue ce fichier avec **Nashorn 15.6**, pas avec
 * V8. Une virgule finale dans un appel d'argument y est refusée (et acceptée par
 * Node) — `pnpm moteur:check` garde ce point.
 */

var SOURCE = {
    id: "hentai-scantrad",
    name: "Hentai-Scantrad",
    lang: "fr",
    baseUrl: "https://hentai-scantrad.org",

    /** Nombre de chapitres attendus au maximum (garde-fou, pas un compte). */
    CHAPITRES_MAX: 5000,

    /**
     * Le catalogue, ou la recherche.
     *
     * La page 1 du catalogue est la racine (`/`) ; les suivantes sont
     * `/page/<N>/` (forme du thème, mesurée jusqu'à la page 9 : 24 cartes
     * chaque fois). La recherche, elle, pagine par `paged` — pas par `page`,
     * qui est silencieusement ignoré (piège 4).
     */
    getList: function (page, query) {
        var url;
        if (query && query.length > 0) {
            url = SOURCE.baseUrl + "/?s=" + encodeURIComponent(query) +
                "&post_type=wp-manga&paged=" + page;
        } else if (page > 1) {
            url = SOURCE.baseUrl + "/page/" + page + "/";
        } else {
            url = SOURCE.baseUrl + "/";
        }

        return SOURCE.extraireCartes(http.get(url));
    },

    /**
     * Les cartes du catalogue. L'ancre des cartes porte `href` **puis** `title`,
     * et la couverture est dans un `<img data-src>` **à l'intérieur** de l'ancre
     * (mesuré).
     *
     * Écrit en trois lectures séparées plutôt qu'en une seule expression à
     * pré-vérification : un **groupe de capture dans un lookahead** est rempli
     * par V8 et laissé `undefined` par Nashorn. Mesuré le 2026-10-17 — le
     * harnais hors ligne (V8) rendait 24 cartes vertes, et le moteur refusait :
     * `TypeError: Cannot read property "indexOf" from undefined`. Le `title`
     * n'est donc pas une pré-condition mais un **filtre** : les ancres des
     * widgets voisins (le curseur « Les plus Populaires », les lectures liées)
     * n'en portent pas, et c'est ce qui les écarte sans les nommer.
     */
    extraireCartes: function (html) {
        var resultats = [];
        var vus = {};
        var re = /<a\b[^>]*href="([^"]*\/manga\/[^"\/]+\/)"[^>]*>/gi;
        var m;
        while ((m = re.exec(html)) !== null) {
            var ancre = m[0];
            var tm = /title="([^"]{1,200})"/.exec(ancre);
            if (!tm) {
                continue;
            }
            var href = m[1];
            if (href.indexOf("http") !== 0) {
                href = SOURCE.baseUrl + href;
            }
            if (vus[href]) {
                continue;
            }
            // La couverture est dans l'ancre : fenêtre bornée à 700 caractères
            // après l'ouverture, soit la carte elle-même.
            var suite = html.substring(m.index + ancre.length, m.index + ancre.length + 700);
            var cm = /data-src="([^"]+)"/.exec(suite);
            if (!cm) {
                continue;
            }
            vus[href] = true;
            resultats.push({
                id: href,
                title: SOURCE.decoderEntites(tm[1].trim()),
                coverUrl: SOURCE.absolute(cm[1]),
                url: href
            });
        }
        return JSON.stringify(resultats);
    },

    /**
     * La fiche : titre, couverture, résumé, statut, auteurs, genres, et **la
     * liste complète des chapitres** — qui ne vit pas dans la fiche.
     */
    getDetails: function (mangaUrl) {
        var url = mangaUrl.indexOf("http") === 0 ? mangaUrl : SOURCE.baseUrl + mangaUrl;
        var html = http.get(url);

        var titre = "Inconnu";
        var tm = /<h1[^>]*>([\s\S]{0,160}?)<\/h1>/.exec(html);
        if (tm) {
            titre = SOURCE.decoderEntites(SOURCE.sansBalises(tm[1]).trim());
        }

        // La couverture du conteneur affiché, `og:image` en repli (piège 5).
        var cm = /<div class="summary_image">[\s\S]{0,800}?data-src="([^"]+)"/.exec(html);
        var coverUrl = cm ? SOURCE.absolute(cm[1]) : "";
        if (!coverUrl) {
            var om = /<meta[^>]*property=["']og:image["'][^>]*content="([^"]+)"/i.exec(html);
            coverUrl = om ? om[1] : "";
        }

        var description = "";
        var dm = /<div class="description-summary">([\s\S]{0,4000}?)<div class="c-content-readmore/.exec(html);
        if (!dm) {
            dm = /<div class="description-summary">([\s\S]{0,4000}?)<\/div>\s*<\/div>/.exec(html);
        }
        if (dm) {
            description = SOURCE.decoderEntites(SOURCE.sansBalises(dm[1])).replace(/\s+/g, " ").trim();
        }

        var statut = SOURCE.statut(html);
        var auteurs = SOURCE.liensTexte(html, "manga-(?:author|artist)");
        var tags = SOURCE.liensTexte(html, "manga-genre");

        // Les chapitres : POST sur `ajax/chapters/` du manga (piège 1). `http.post`
        // envoie `application/json` sans en-tête XHR — mesuré : l'endpoint répond
        // `200` et 316 chapitres dans cette forme exacte.
        var chapitres = [];
        try {
            var corps = http.post(url.replace(/\/?$/, "/") + "ajax/chapters/", "");
            chapitres = SOURCE.extraireChapitres(corps);
        } catch (e) {
            chapitres = [];
        }

        return JSON.stringify({
            title: titre,
            description: description,
            coverUrl: coverUrl,
            status: statut,
            authors: auteurs,
            tags: tags,
            chapters: chapitres
        });
    },

    /**
     * Le statut, lu dans le bloc info intitulé « État ». Le libellé est celui du
     * site (« En Cours »), traduit vers le vocabulaire du contrat.
     */
    statut: function (html) {
        var m = /<h5>\s*(?:État|Etat)\s*<\/h5>[\s\S]{0,200}?<div class="summary-content">([\s\S]{0,80}?)<\/div>/.exec(html);
        if (!m) {
            return "unknown";
        }
        var s = SOURCE.sansBalises(m[1]).toLowerCase();
        if (s.indexOf("cours") >= 0) {
            return "ongoing";
        }
        if (s.indexOf("termin") >= 0) {
            return "finished";
        }
        if (s.indexOf("pause") >= 0) {
            return "paused";
        }
        if (s.indexOf("abandon") >= 0) {
            return "abandoned";
        }
        return "unknown";
    },

    /**
     * Les libellés des ancres dont le `href` contient `motif` (genres, auteurs),
     * dédoublonnés et décodés.
     */
    liensTexte: function (html, motif) {
        var valeurs = [];
        var re = new RegExp('<a[^>]*href="[^"]*\\/' + motif + '\\/[^"]*"[^>]*>([^<]{1,80})<\\/a>', "gi");
        var m;
        while ((m = re.exec(html)) !== null) {
            var texte = SOURCE.decoderEntites(m[1]).trim();
            if (texte.length > 0 && valeurs.indexOf(texte) < 0) {
                valeurs.push(texte);
            }
        }
        return valeurs;
    },

    /**
     * Les chapitres, dédoublonnés puis **triés du plus ancien au plus récent**
     * (le contrat l'exige, et le site liste l'inverse).
     *
     * Le numéro est lu dans l'**URL**, avec ses deux préfixes (`chapitre-` et
     * `ch-`, piège 2), et non dans le texte : le texte est « Chapitre 315 » ou
     * « Ch.1 », donc il faudrait deux lectures et une heuristique.
     */
    extraireChapitres: function (html) {
        var bruts = [];
        var vus = {};
        var re = /<li[^>]*class="wp-manga-chapter[^"]*"[^>]*>[\s\S]{0,200}?<a[^>]*href="([^"]+)"[^>]*>([\s\S]{0,80}?)<\/a>/gi;
        var m;
        while ((m = re.exec(html)) !== null) {
            var href = m[1];
            if (href.indexOf("http") !== 0) {
                href = SOURCE.baseUrl + href;
            }
            if (vus[href]) {
                continue;
            }
            vus[href] = true;

            var numero = null;
            var nm = /(?:chapitre|ch)-(\d+)/.exec(href);
            if (nm) {
                numero = parseInt(nm[1], 10);
            } else {
                var chiffres = SOURCE.decoderEntites(SOURCE.sansBalises(m[2])).match(/\d+/g);
                numero = chiffres ? parseInt(chiffres[chiffres.length - 1], 10) : null;
            }
            if (numero === null || isNaN(numero)) {
                continue;
            }

            bruts.push({
                id: href,
                title: SOURCE.decoderEntites(SOURCE.sansBalises(m[2])).trim(),
                number: numero,
                url: href,
                ordre: bruts.length
            });
            if (bruts.length >= SOURCE.CHAPITRES_MAX) {
                break;
            }
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
     * Les pages d'un chapitre.
     *
     * Le lecteur numérote ses pages (`id="image-0"`, `image-1`, …) : c'est un
     * marqueur sémantique, pas une position, et il évite de compter la bannière
     * du site. Le `trim()` est obligatoire : le HTML écrit
     * `data-src=" https://…"` (piège 3).
     */
    getPages: function (chapterUrl) {
        var url = chapterUrl.indexOf("http") === 0 ? chapterUrl : SOURCE.baseUrl + chapterUrl;
        var html = http.get(url);

        var pages = [];
        var vues = {};
        var re = /<img[^>]*id="image-\d+"[^>]*data-src="([^"]+)"/gi;
        var m;
        while ((m = re.exec(html)) !== null) {
            var img = SOURCE.absolute(m[1]);
            if (vues[img]) {
                continue;
            }
            vues[img] = true;
            pages.push({ id: String(pages.length), url: img });
        }
        return JSON.stringify(pages);
    },

    /**
     * Une URL d'image telle que le site l'écrit : espaces compris, et **en
     * `https://`**.
     *
     * Le `https` n'est pas une préférence, c'est une obligation mesurée deux
     * fois. (a) Les chapitres anciens (`ch-N`, numéros ≤ 82) servent leurs
     * images en `http://stockage.hentai-scantrad.org/…`, et le moteur **refuse**
     * ce schéma (`validerUrlImage` : « seul https:// est accepte ») — sans cette
     * réécriture, ces chapitres rendraient **0 page**. (b) Le même chemin en
     * `https` a été mesuré : `200, 121 866 octets, image/jpeg`, exactement les
     * mêmes octets qu'en `http` — la réécriture ne change donc pas l'image.
     */
    absolute: function (candidate) {
        var u = candidate.trim();
        if (u.indexOf("http") !== 0) {
            u = SOURCE.baseUrl + u;
        }
        if (u.indexOf("http://") === 0) {
            u = "https://" + u.substring("http://".length);
        }
        return u;
    },

    sansBalises: function (texte) {
        return texte.replace(/<[^>]*>/g, " ");
    },

    /** Les entités que ce site écrit vraiment (mesurées dans les fiches). */
    decoderEntites: function (texte) {
        return texte
            .replace(/&nbsp;/g, " ")
            .replace(/&rsquo;|&#8217;/g, "\u2019")
            .replace(/&lsquo;|&#8216;/g, "\u2018")
            .replace(/&laquo;/g, "\u00ab")
            .replace(/&raquo;/g, "\u00bb")
            .replace(/&hellip;/g, "\u2026")
            .replace(/&amp;/g, "&")
            .replace(/&quot;/g, "\"")
            .replace(/&#0?39;|&apos;/g, "'")
            .replace(/&lt;/g, "<")
            .replace(/&gt;/g, ">");
    }
};
