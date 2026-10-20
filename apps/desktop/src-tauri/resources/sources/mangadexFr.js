/**
 * Source personnalisée Tatami — MangaDex via son API JSON
 * =====================================================
 * Fichier à importer tel quel depuis Réglages → « Sources perso ».
 *
 * Le nom du fichier fait l'identifiant de la source : ici `mangadexFr.js`,
 * donc `custom:mangadexFr` dans le catalogue. `SOURCE.id` reste descriptif.
 *
 * Pourquoi cette API et pas un site de manga ?
 * ---------------------------------------------
 * Le pont `http` n'expose que `http.get(url)` / `http.post(url, corps)` : pas
 * de `fetch`, pas de parseur HTML. Un script ne peut donc consommer qu'une API
 * qui **renvoie du JSON**. MangaDex l'est, ne demande ni clé ni compte, et sert
 * de vraies pages d'images.
 *
 * Contraintes du moteur (Nashorn, ES5.1) — à respecter, sinon le script est
 * refusé au chargement :
 *   - `var` uniquement, pas de fléchées, pas de template littéral ;
 *   - pas de `Promise` : les appels `http.get` sont **synchrones** ;
 *   - chaque méthode doit retourner une **chaîne JSON** (`JSON.stringify`) ;
 *   - la seule API d'images exposée par un script est `http`.
 *
 * Vérifié le 2026-07 contre le moteur réel (voir `docs/smoke-test.md` §S) :
 * `listSources` → `custom:mangadex-api`, recherche, fiche, chapitres, pages.
 */

var API = "https://api.mangadex.org";
var CDN = "https://uploads.mangadex.org";
var PAR_PAGE = 20;

/** `{en: "…", ja: "…"}` → chaîne. L'API est multilingue : on garde l'anglais
 *  s'il existe, sinon la première valeur présente. */
function texte(objet, defaut) {
    if (!objet) return defaut;
    if (typeof objet === "string") return objet;
    if (objet.en) return objet.en;
    var cles = Object.keys(objet);
    for (var i = 0; i < cles.length; i++) {
        if (objet[cles[i]]) return objet[cles[i]];
    }
    return defaut;
}

/** Dernier segment d'un chemin interne (`/manga/<id>` → `<id>`). */
function dernierSegment(url) {
    var s = String(url).split("?")[0];
    var i = s.lastIndexOf("/");
    return i >= 0 ? s.substring(i + 1) : s;
}

/** URL de la couverture, ou null. L'API renvoie un nom de fichier sans
 *  extension exploitable ; le CDN veut `<hash>.256.jpg`. Une série sans
 *  couverture rend `null`, ce que l'interface sait traiter. */
function couverture(manga) {
    var rels = manga.relationships || [];
    for (var i = 0; i < rels.length; i++) {
        var r = rels[i];
        if (r.type === "cover_art" && r.attributes && r.attributes.fileName) {
            // Mesuré le 2026-07 : MangaDex sert `…/<hash>.jpg` (200, 266 663 o).
            // Les variantes `.256.jpg` et `.512.jpg` répondent **404** — j'avais
            // d'abord ajouté un suffixe de taille, et toutes les vignettes
            // échouaient ("ce n'est pas une image"). Donc : le `fileName` tel
            // quel, extension comprise ; `.jpg` seulement s'il n'en a pas.
            var f = String(r.attributes.fileName);
            if (!/\.[A-Za-z0-9]+$/.test(f)) f = f + ".jpg";
            return CDN + "/covers/" + manga.id + "/" + f;
        }
    }
    return null;
}

/** Noms d'auteurs / d'artistes, en `string[]`. */
function noms(manga, type) {
    var out = [];
    var rels = manga.relationships || [];
    for (var i = 0; i < rels.length; i++) {
        if (rels[i].type === type && rels[i].attributes && rels[i].attributes.name) {
            out.push(rels[i].attributes.name);
        }
    }
    return out;
}

/** Genres au format de l'interface : `{key, title}` (cf. `type Tag` côté TS). */
function genres(attributs) {
    var out = [];
    var tags = attributs.tags || [];
    for (var i = 0; i < tags.length; i++) {
        var nom = texte(tags[i].attributes ? tags[i].attributes.name : null, null);
        if (nom) out.push({ key: tags[i].id, title: nom });
    }
    return out;
}

var SOURCE = {
    /** Documenté, mais l'identité de la source vient du nom du fichier. */
    id: "mangadexFr",

    /** Nom affiché dans la grille du catalogue. */
    name: "MangaDex Français (API)",

    /** Place la tuile dans le catalogue « JA ». */
    lang: "fr",

    baseUrl: API,

    /**
     * getList(page, query) → chaîne JSON d'un tableau
     * `[{id, title, coverUrl, url}, …]`, `page` à partir de 1.
     * `query` vide = Dernières mises à jour (le catalogue), sinon recherche
     * par titre.
     */
    getList: function (page, query) {
        var decalage = (Math.max(1, page) - 1) * PAR_PAGE;
        var q = query ? String(query).trim() : "";
        var url;
        if (q.length > 0) {
            url = API + "/manga?title=" + encodeURIComponent(q) +
                "&limit=" + PAR_PAGE + "&offset=" + decalage +
                "&includes%5B%5D=cover_art" +
                "&availableTranslatedLanguage%5B%5D=fr" +
                "&order%5Brelevance%5D=desc";
        } else {
            url = API + "/manga?limit=" + PAR_PAGE + "&offset=" + decalage +
                "&includes%5B%5D=cover_art" +
                "&availableTranslatedLanguage%5B%5D=fr" +
                "&order%5BlatestUploadedChapter%5D=desc";
        }

        var data = JSON.parse(http.get(url)).data || [];
        var liste = [];
        for (var i = 0; i < data.length; i++) {
            liste.push({
                id: data[i].id,
                title: texte(data[i].attributes.title, "Sans titre"),
                coverUrl: couverture(data[i]),
                url: "/manga/" + data[i].id,
            });
        }
        return JSON.stringify(liste);
    },

    /**
     * getDetails(mangaUrl) → chaîne JSON
     * `{title, description, coverUrl, status, authors, tags, chapters}`.
     *
     * L'API sépare la fiche et les chapitres : deux appels. Si l'agrégat des
     * chapitres échoue, la fiche reste affichable — on préfère un manga sans
     * chapitre à une fiche qui ne s'ouvre pas.
     */
    getDetails: function (mangaUrl) {
        var id = dernierSegment(mangaUrl);
        var entite = JSON.parse(http.get(
            API + "/manga/" + id +
            "?includes%5B%5D=cover_art&includes%5B%5D=author&includes%5B%5D=artist"
        ));
        var manga = entite.data ? entite.data : entite;
        var a = manga.attributes;

        var chapitres = [];
        try {
            // On utilise le feed plutôt que /aggregate afin de demander
            // explicitement les chapitres dont la langue de traduction est le français.
            var feedUrl = API + "/manga/" + id + "/feed" +
                "?limit=500" +
                "&translatedLanguage%5B%5D=fr" +
                "&order%5Bchapter%5D=asc" +
                "&order%5Bvolume%5D=asc" +
                "&includes%5B%5D=scanlation_group";
            var feed = JSON.parse(http.get(feedUrl));
            var data = feed.data || [];

            for (var c = 0; c < data.length; c++) {
                var ch = data[c];
                var aCh = ch.attributes || {};
                if (!ch.id || aCh.externalUrl) continue;

                var numero = parseFloat(aCh.chapter);
                var volume = aCh.volume;
                var titre = aCh.title || "";

                chapitres.push({
                    id: ch.id,
                    title: titre || (volume && volume !== "none" && volume !== "0"
                        ? "vol. " + volume
                        : ""),
                    number: isNaN(numero) ? 0 : numero,
                    url: "/chapter/" + ch.id
                });
            }
        } catch (e) {
            chapitres = [];
        }

        return JSON.stringify({
            title: texte(a.title, "Sans titre"),
            description: texte(a.description, ""),
            coverUrl: couverture(manga),
            status: String(a.status || "unknown").toUpperCase(),
            authors: noms(manga, "author"),
            tags: genres(a),
            chapters: chapitres,
        });
    },

    /**
     * getPages(chapterUrl) → chaîne JSON de `[{id, url}, …]`.
     * L'API renvoie un hôte + une liste de noms de fichiers, à recomposer en
     * URL. Le CDN de MangaDex demande un `Referer` : c'est Rust qui l'ajoute
     * en téléchargeant (l'interface ne voit jamais l'image directement).
     */
    getPages: function (chapterUrl) {
        var id = dernierSegment(chapterUrl);
        var json = JSON.parse(http.get(API + "/at-home/server/" + id));
        var chapitre = json.chapter || {};
        var fichiers = chapitre.data || [];
        var pages = [];
        for (var i = 0; i < fichiers.length; i++) {
            pages.push({
                id: fichiers[i],
                url: json.baseUrl + "/data/" + chapitre.hash + "/" + fichiers[i],
            });
        }
        return JSON.stringify(pages);
    },
};