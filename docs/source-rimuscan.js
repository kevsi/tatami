/**
 * Source scriptée : Rimu Scans (https://rimuscan.fr)
 * =================================================
 *
 * Le parseur natif `RIMUSCANS` vise **déjà** le bon domaine (`rimuscan.fr`,
 * lu dans le bytecode) et le site répond 200 — mais il ne trouve rien. Ce
 * n'est donc pas une migration d'adresse : c'est un parseur inadapté. Le site
 * est une application **Next.js**, sans une seule classe Madara. D'où « 200 OK
 * sans résultat » dans `contract/sources-etat.json`.
 *
 * Tout ce qui suit est mesuré le 2026-10-13 sur le HTML réel.
 *
 * ## Pourquoi aucune regex ne cherche les `\"`
 *
 * Le payload est du JSON **échappé**, donc chaque clé s'écrit `\"slug\":\"…`.
 * Or **dans une RegExp, `\"` est un échappement de `"`** : un motif construit
 * avec `\"slug\"` cherche en réalité `"slug"` et ne trouve rien, sans lever la
 * moindre erreur. C'est exactement ce qui est arrivé au premier jet — 0 série
 * sur 238, silencieusement. D'où une extraction **par chaînes** (`indexOf`),
 * où `\"` est bien deux caractères.
 *
 * ## Formes mesurées
 *
 *  - Catalogue : un **seul** objet `\"series\":{…}` dont les entrées sont
 *    séparées par `},"` — 238 entrées sur l'accueil. Ici `slug` vient
 *    **avant** `title`, l'inverse d'ortegascans.fr.
 *  - Fiche : `\"mangaCover\":\"…\"`, `\"mangaSlug\":\"…\"`, et le tableau
 *    `\"chapters\":[…]`.
 *  - Dans ce tableau, `\"number\":5` est **non guillemeté** — une extraction
 *    qui cherche une chaîne échoue.
 *  - Séries : `/manga/<slug>`. Chapitres : **`/read/<slug>/<n>`** — pas
 *    `/chapter/`, forme propre à ce site.
 *  - Pages : `\"url\":\"\/uploads\/mangas/<slug>/chapters/<n>/<page>.jpg?v=…\"`.
 *    Le HTML n'en rend qu'une partie ; le payload en a toutes.
 *  - **Les images exigent un `Referer`** : la même URL répond 403 sans, et
 *    200 (786 715 octets) avec `Referer: https://rimuscan.fr/`. L'origine
 *    seule suffit. Le chemin **natif** du moteur envoie déjà un `Referer`
 *    (`fetch_octets`) ; le chemin scripté non — aligné le 2026-10-13.
 *
 * Limite réelle : pas de pagination ni de recherche mesurables — comme
 * ortegascans.fr, la recherche filtre la liste de l'accueil localement.
 */

var SOURCE = {
    id: "rimuscan",
    name: "Rimu Scans",
    lang: "fr",
    baseUrl: "https://rimuscan.fr",

    // La séquence Literally « backslash + guillemet » du payload. Écrite par
    // codes de caractères pour qu'aucun lecteur ne la prenne pour un échappement.
    B: String.fromCharCode(92),
    Q: String.fromCharCode(34),

    // -----------------------------------------------------------------------
    // Utilitaires d'extraction — par chaînes, jamais par regex (voir l'en-tête)
    // -----------------------------------------------------------------------

    /** La séquence `\"` — **tout** guillemet du payload en est précédé, y
     *  compris celui qui ouvre une valeur. C'est la règle qui rendait la
     *  première version silencieusement vide : le motif cherchait
     *  `\"slug\":"` au lieu de `\"slug\":\"`. */
    q: function () {
        return "\\" + '"';
    },

    /** La marque `\"<cle>\":` telle qu'elle apparaît dans le payload. */
    marque: function (cle) {
        return this.q() + cle + this.q() + ":";
    },

    /** Valeur **guillemetée** d'une clé, ou null. */
    lireTexte: function (texte, cle, depuis) {
        var q = SOURCE.q();
        var m = SOURCE.marque(cle) + q;
        var debut = texte.indexOf(m, depuis || 0);
        if (debut < 0) return null;
        var from = debut + m.length;
        var fin = texte.indexOf(q, from);
        if (fin < 0) return null;
        return { valeur: texte.substring(from, fin), fin: fin };
    },

    /** Valeur **numérique** d'une clé (`\"number\":5`, sans guillemets). */
    lireNombre: function (texte, cle, depuis) {
        var m = SOURCE.marque(cle);
        var debut = texte.indexOf(m, depuis || 0);
        if (debut < 0) return null;
        var from = debut + m.length;
        var fin = from;
        while (fin < texte.length && /[0-9.\-]/.test(texte.charAt(fin))) fin += 1;
        if (fin === from) return null;
        return { valeur: texte.substring(from, fin), fin: fin };
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
        return out.replace(/\\u([0-9a-fA-F]{4})/g, function (m, h) {
            return String.fromCharCode(parseInt(h, 16));
        });
    },

    sansAccents: function (s) {
        var table = {
            "à": "a", "â": "a", "ä": "a", "á": "a", "ã": "a", "å": "a",
            "ç": "c", "è": "e", "é": "e", "ê": "e", "ë": "e",
            "î": "i", "ï": "i", "í": "i", "ô": "o", "ö": "o", "ó": "o", "õ": "o",
            "ù": "u", "û": "u", "ü": "u", "ú": "u", "ý": "y", "ÿ": "y", "ñ": "n"
        };
        return String(s).toLowerCase()
            .replace(/[àâäáãåçèéêëîïíôöóõùûüúýÿñ]/g, function (c) { return table[c]; });
    },

    /** Une origine absolue : le payload donne `/uploads/…`, sans base. */
    absolute: function (chemin) {
        if (!chemin) return "";
        if (chemin.indexOf("/") === 0) return SOURCE.baseUrl + chemin;
        return chemin;
    },

    // -----------------------------------------------------------------------
    // getList(page, query)
    // -----------------------------------------------------------------------

    parseListe: function (html) {
        var conteneur = html.indexOf(SOURCE.marque("series") + "{");
        if (conteneur < 0) return [];

        var corps = html.substring(conteneur);

        // Le catalogue est UN objet dont les entrées sont séparées par `},"`
        // **avec le backslash** — mesuré, couper sur `},"` seul ne trouve rien.
        // Découper sur la clé n'aurait donné qu'une entrée : 1 sur 238.
        var morceaux = corps.split("}," + SOURCE.q());
        var out = [];
        var vus = {};
        for (var i = 0; i < morceaux.length; i++) {
            var s = SOURCE.lireTexte(morceaux[i], "slug");
            if (!s) continue;
            var slug = s.valeur;
            if (vus[slug]) continue;
            var t = SOURCE.lireTexte(morceaux[i], "title", s.fin);
            if (!t) continue;
            vus[slug] = true;
            var c = SOURCE.lireTexte(morceaux[i], "cover", t.fin);
            var st = SOURCE.lireTexte(morceaux[i], "status", c ? c.fin : t.fin);
            out.push({
                id: SOURCE.baseUrl + "/manga/" + slug,
                title: SOURCE.decodeEntites(t.valeur),
                coverUrl: SOURCE.absolute(c ? SOURCE.decodeEntites(c.valeur) : ""),
                url: SOURCE.baseUrl + "/manga/" + slug,
                // Conservé pour le tri ultérieur, pas renvoyé tel quel.
                _statut: st ? st.valeur : ""
            });
        }
        return out;
    },

    getList: function (page, query) {
        var items = SOURCE.parseListe(http.get(SOURCE.baseUrl + "/"));
        if (query && SOURCE.trim(query).length > 0) {
            var cible = SOURCE.sansAccents(query);
            var trouves = [];
            for (var i = 0; i < items.length; i++) {
                if (SOURCE.sansAccents(items[i].title).indexOf(cible) >= 0) {
                    trouves.push(items[i]);
                }
            }
            items = trouves;
        } else if (page > 1) {
            return JSON.stringify([]);
        }
        var propres = [];
        for (var k = 0; k < items.length; k++) {
            propres.push({
                id: items[k].id,
                title: items[k].title,
                coverUrl: items[k].coverUrl,
                url: items[k].url
            });
        }
        return JSON.stringify(propres);
    },

    // -----------------------------------------------------------------------
    // getDetails(mangaUrl)
    // -----------------------------------------------------------------------

    getDetails: function (mangaUrl) {
        var url = mangaUrl.indexOf("http") === 0 ? mangaUrl : SOURCE.baseUrl + mangaUrl;
        var html = http.get(url);

        var slug = SOURCE.lireTexte(html, "mangaSlug");
        if (!slug) {
            slug = { valeur: url.replace(/^https?:\/\/[^/]+\/manga\//, "").replace(/\/$/, "") };
        }
        var slugPropre = slug.valeur;

        var titre = "";
        var mT = html.match(/<title>([\s\S]*?)<\/title>/);
        if (mT) titre = SOURCE.trim(SOURCE.decodeEntites(mT[1])).split("|")[0];

        var mCov = SOURCE.lireTexte(html, "mangaCover");
        var mDesc = SOURCE.lireTexte(html, "desc");

        // Statut de série : `\"status\":\"ongoing\"` dans l'objet `series`.
        // Sur la fiche, `\"status\":\"PUBLISHED\"` est celui d'un **chapitre** ;
        // le chercher globalement donnait `PUBLISHED` — mesuré.
        var statut = "unknown";
        var conteneur = html.indexOf(SOURCE.marque("series") + "{");
        if (conteneur >= 0) {
            var s0 = SOURCE.lireTexte(html, "status", conteneur);
            if (s0) {
                var v = s0.valeur;
                if (v === "ongoing") statut = "ongoing";
                else if (v === "completed" || v === "finished") statut = "finished";
                else if (v === "hiatus") statut = "paused";
            }
        }

        // Chapitres : le HTML ne monte que les dernières ancres (mesuré le
        // 2026-10-20 sur « Slime » : 21 ancres pour 154 chapitres annoncés),
        // mais le **JSON-LD** de la même page porte la liste complète en
        // `ListItem` (154 entrées `url` + `name`). Lire les ancres seules
        // faisait donc disparaître tous les chapitres anciens, pas seulement
        // les plus récents. On lit le JSON-LD d'abord, les ancres en repli.
        var vus = {};
        var chapitres = [];
        var blocsLd = html.split('"@type":"ListItem"');
        for (var i = 1; i < blocsLd.length; i++) {
            var blocLd = blocsLd[i].substring(0, 400);
            var mUrl = blocLd.match(/"url":"(https?:\/\/[^"]*\/read\/[^"\/]+\/(\d+))"/);
            if (!mUrl) continue;
            var num = parseInt(mUrl[2], 10);
            if (isNaN(num) || vus[num]) continue;
            vus[num] = true;
            var mNom = blocLd.match(/"name":"([^"]*)"/);
            chapitres.push({
                id: mUrl[1],
                title: (mNom ? SOURCE.decodeEntites(mNom[1]) : "") || ("Chapitre " + num),
                number: num,
                url: mUrl[1]
            });
        }
        if (chapitres.length === 0) {
            var reCh = /href="\/read\/([^"\\/]+)\/([0-9]+)"/g;
            var m;
            while ((m = reCh.exec(html)) !== null) {
                var numAncre = parseInt(m[2], 10);
                if (isNaN(numAncre) || vus[numAncre]) continue;
                vus[numAncre] = true;
                var chUrl = SOURCE.baseUrl + "/read/" + m[1] + "/" + numAncre;
                chapitres.push({
                    id: chUrl,
                    title: "Chapitre " + numAncre,
                    number: numAncre,
                    url: chUrl
                });
            }
        }
        chapitres.sort(function (a, b) { return a.number - b.number; });

        return JSON.stringify({
            title: SOURCE.trim(titre),
            description: mDesc ? SOURCE.decodeEntites(mDesc.valeur) : "",
            coverUrl: SOURCE.absolute(mCov ? SOURCE.decodeEntites(mCov.valeur) : ""),
            status: statut,
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

        // Le payload, pas les `<img>` : le lecteur n'en rend qu'une partie.
        var pages = [];
        var vus = {};
        // On s'ancre sur la clé `\"url\":\"\/uploads\/mangas\/…` du **payload**.
        // Balayer toutes les occurrences de `/uploads/mangas/` paraît plus
        // simple mais casse : dans le HTML rendu les attributs ont des `"`
        // **non échappés**, donc la coupure `\"` part parfois de plusieurs
        // kilo-octets plus loin et renvoie un blob HTML entier — mesuré.
        var prefixe = SOURCE.marque("url") + SOURCE.q() + "/uploads/mangas/";
        var p = 0;
        while (true) {
            var i = html.indexOf(prefixe, p);
            if (i < 0) break;
            var from = i + prefixe.length;
            var fin = html.indexOf(SOURCE.q(), from);
            if (fin < 0) break;
            p = fin;
            var brut = html.substring(from, fin);
            // Les URL se terminent par `?v=<cache-buster>` : sans le couper,
            // le test d'extension échouait et la liste restait vide — mesuré,
            // une seule page trouvée (la couverture).
            if (!/\.(jpg|jpeg|png|webp)$/i.test(brut.split("?")[0])) continue;
            // La couverture de la série est dans le même dossier d'URL : ce
            // n'est pas une page de chapitre.
            if (/\/cover/.test(brut)) continue;
            if (vus[brut]) continue;
            vus[brut] = true;
            // `brut` est extrait **après** `/uploads/mangas/` : le préfixe a été
            // consommé par la recherche, il faut donc le remettre, sinon les
            // URL sortent amputées — mesuré, `https://rimuscan.fr/<slug>/…`.
            pages.push({
                id: String(pages.length + 1),
                url: SOURCE.baseUrl + "/uploads/mangas/" + brut
            });
        }
        return JSON.stringify(pages);
    },

    /** Utilitaire de trim, absent de la version Nashorn ciblée ici. */
    trim: function (s) {
        return String(s).replace(/^[\s ]+/, "").replace(/[\s ]+$/, "");
    }
};