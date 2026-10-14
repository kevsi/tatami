/**
 * Source script : Banana Scans (https://bananascans.com)
 * =====================================================
 *
 * Le parseur natif `BANANASCAN` est classé `parseur_casse` : le site répond 200,
 * le parseur n'en extrait rien.
 *
 * Tout est **mesuré le 2026-10-14** dans un vrai navigateur.
 *
 *   liste    : `/manga-list`      — séries `/manga/<slug>`
 *   série    : `/manga/<slug>`    — chapitres `/manga/<slug>/vol<N>`
 *   chapitre : `/manga/<slug>/vol<N>/<n>`
 *   images   : attribut `data-src` (et **jamais** `src`)
 *
 * ## Le piège des images : des GIF transparents
 *
 * Le lecteur ne met **aucune image réelle** dans `src` : il y met un GIF
 * 1×1 transparent en base64 (`data:image/gif;base64,R0lGODlh…`) et laisse la
 * vraie adresse dans `data-src`. Lire `src` — le réflexe — donne huit images
 * identiques de 1 pixel, sans lever la moindre erreur. C'est le défaut que le
 * projet connaît déjà sous le nom « une image cassée sans message », ici pire :
 * une image *valide* et *vide*.
 *
 * ## Les espaces
 *
 * Les valeurs de `data-src` sont **entourées d'espaces** :
 * `data-src="  http://… 00002.jpeg "`. Sans `trim()`, chaque URL commence par
 * une espace et l'image ne se charge pas — un échec silencieux de plus.
 *
 * ## http → https
 *
 * Le site annonce ses images en `http://`, mais le contrat exige une URL
 * **`https://`**. On réécrit donc le schéma. À vérifier : si le CDN ne sert pas
 * en TLS, il faudra passer par le chemin d'image de Rust plutôt que par cette
 * source. Signalé plutôt que supposé.
 */

var SOURCE = {
    id: "bananascans",
    name: "Banana Scans",
    lang: "en",
    baseUrl: "https://bananascans.com",

    /** Le dossier des images, seul discriminant possible sur ce site. */
    UPLOADS: "/uploads/manga/",

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

    /** Nettoie une adresse d'image : espaces, et `http` vers `https`. */
    image: function (brute) {
        var u = String(brute).trim();
        if (u.indexOf("http://") === 0) u = "https://" + u.slice(7);
        return u;
    },

    // ------------------------------------------------------------------ liste
    getList: function (page, query) {
        var url = query
            ? SOURCE.baseUrl + "/?s=" + encodeURIComponent(query) + "/page/" + page + "/"
            : SOURCE.baseUrl + "/manga-list/page/" + page + "/";
        var html = http.get(url);
        if (!html) return "[]";

        // Une série est un lien `/manga/<slug>` **sans** `/vol`. Les chapitres
        // partagent le même préfixe : sans cette distinction, la liste est un
        // sommaire de chapitres.
        // Le site sert des URL **absolues** (`https://bananascans.com/manga/…`).
        // Une regex exigeant `href="/manga/…"` ne trouve rien, sans lever
        // d'erreur. `[^"]*` tolère les deux formes ; c'est le slug qui est
        // capturé, le chemin est reconstruit pour le contrat.
        //
        // Le guillemet juste après le slug exclut de fait les chapitres
        // (`/manga/harumichi/vol3` a une barre, pas un guillemet), sans avoir à
        // l'exclure explicitement.
        var re = /<a[^>]+href="[^"]*\/manga\/([a-z0-9-]+)"[^>]*>([\s\S]*?)<\/a>/gi;
        var vus = {};
        var resultats = [];
        var m;
        while ((m = re.exec(html)) !== null) {
            var slug = m[1];
            if (vus[slug]) continue;
            // Le titre est dans la cellule voisine ; on prend le texte du lien
            // et, à défaut, le slug rendu lisible.
            var nom = SOURCE.texte(m[2]);
            if (!nom || nom.length < 2) continue;
            vus[slug] = true;
            resultats.push({
                id: "/manga/" + slug,
                title: nom,
                coverUrl: "",
                url: "/manga/" + slug,
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
            (/<h1[^>]*>([\s\S]*?)<\/h1>/i.exec(html) || [, mangaUrl.replace(/\//g, " ").replace(/^\s+/, "")])[1]
        );

        var couverture = "";
        var reCouv = /<img[^>]+(?:data-src|src)="([^"]+\/uploads\/manga\/[^"]+)"[^>]*>/gi;
        var c = reCouv.exec(html);
        if (c) couverture = SOURCE.image(c[1]);

        var resume = SOURCE.texte(
            (/<div[^>]+class="[^"]*description[^"]*"[^>]*>([\s\S]*?)<\/div>/i.exec(html) || [
                "",
                "",
            ])[1]
        ).slice(0, 1200);

        var brut = SOURCE.texte(html).toLowerCase();
        var statut = "unknown";
        if (brut.indexOf("ongoing") >= 0 || brut.indexOf("en cours") >= 0) statut = "ongoing";
        else if (brut.indexOf("completed") >= 0 || brut.indexOf("termin") >= 0) statut = "finished";

        // Chapitres : `/manga/<slug>/vol<N>`. Le site les présente du plus
        // récent au plus ancien ; l'application attend l'inverse.
        var chapitres = [];
        var vus = {};
        // URL absolue tolérée, comme dans `getList`. Le groupe 1 reste le **chemin**,
        // que le contrat attend.
        var reChap = /<a[^>]+href="[^"]*(\/manga\/[a-z0-9-]+\/vol(\d+))"[^>]*>([\s\S]*?)<\/a>/gi;
        var m;
        while ((m = reChap.exec(html)) !== null) {
            if (vus[m[1]]) continue;
            vus[m[1]] = true;
            var vol = parseInt(m[2], 10);
            var libelle = SOURCE.texte(m[3]);
            chapitres.push({
                id: m[1] + "/1",
                title: libelle || "Vol. " + vol,
                number: vol,
                url: m[1] + "/1",
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

        // **Jamais `src`** : c'est un GIF 1×1 transparent. Voir l'en-tête.
        var pages = [];
        var vus = {};
        var re = /<img[^>]+data-src="([^"]+)"[^>]*>/gi;
        var m;
        while ((m = re.exec(html)) !== null) {
            var src = SOURCE.image(m[1]);
            if (src.indexOf(SOURCE.UPLOADS) < 0) continue;
            if (vus[src]) continue;
            vus[src] = true;
            pages.push({ id: src, url: src });
        }
        return JSON.stringify(pages);
    },
};
