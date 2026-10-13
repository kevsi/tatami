/**
 * Source scriptée Tatami — Scan-Manga (FR)
 * =========================================
 *
 * ## ⚠ À LIRE AVANT — ce qui est vérifié et ce qui ne l'est pas
 *
 * **Le site est derrière Cloudflare.** Mesuré le 2026-10-09 depuis cette
 * machine :
 *
 *   https://www.scan-manga.com/            → 403, `cf-mitigated: challenge`
 *   https://www.scan-manga.com/manga        → 403 idem
 *   https://www.scan-manga.com/liste-mangas → 403 idem
 *   ... 5 autres chemins, tous identiques
 *
 * Donc : **aucune page du site n'a pu être lue**, et la structure HTML est
 * inconnue. Les sélecteurs ci-dessous sont des **emplacements balisés**, pas
 * des expressions vérifiées. C'est une limite de mesure, pas un oubli : je n'ai
 * pas inventé de sélecteurs « plausibles » pour faire semblance.
 *
 * Pour vérifier la structure, ouvrez un chapitre dans un navigateur qui a
 * résolu le challenge, puis : clic droit → « Inspecter », et relevez :
 *   - la classe des cartes de série dans la liste de recherche ;
 *   - le lien du bouton « Chapitre X » sur une fiche ;
 *   - le `src` des `<img>` d'un chapitre.
 *
 * ## Comment le.cloudflare se résout
 *
 * Tatami gère ce cas : le moteur lève `CHALLENGE_REQUIRED`, et Rust ouvre une
 * **fenêtre de résolution** (`commands.rs`). Résolvez le challenge dans cette
 * fenêtre, une fois : les cookies sont conservés pour la session, et les
 * requêtes suivantes passent. C'est ce qui rend cette source utilisable — sans
 * cela, aucune requête ne aboutirait.
 *
 * ## Identité : c'est le NOM DU FICHIER qui fait foi
 *
 * Mesuré le 2026-10-07 : le moteur prend l'identifiant dans **le nom de
 * fichier** (`scan-manga.js` → `custom:scan-manga`), et Rust enregistre la
 * ligne sous ce même nom. `SOURCE.id` est lu et validé, mais ne décide plus de
 * l'identité — une valeur divergente faisait disparaître la source des
 * réglages tout en la laissant fantôme dans le catalogue. **Gardez-les
 * identiques.**
 *
 * ## API disponible dans le script
 *
 *   http.get(url)         → String (corps de la réponse)
 *   http.post(url, body)  → String
 *
 * Contraintes mesurées :
 *   - **URLs `https://` uniquement** (le réseau local est refusé) ;
 *   - **aucun en-tête personnalisable** — `Accept` n'est pas modifiable. Donc
 *     toute API exigeant un `Accept` particulier est hors de portée ;
 *   - corps limité à 5 Mo.
 */

var SOURCE = {
    // =========================================================================
    // Métadonnées — `id` doit être le nom du fichier SANS `.js`.
    // =========================================================================
    id: "scan-manga",
    name: "Scan-Manga",
    lang: "fr",
    baseUrl: "https://www.scan-manga.com",

    // =========================================================================
    // getList(page, query) → JSON.stringify([{id, title, coverUrl, url}, …])
    //
    // `page` commence à 1. Renvoyer [] en fin de liste.
    //
    // ⚠ URL À VÉRIFIER : les 7 chemins testés renvoient tous 403. Celui-ci est
    // une hypothèse ; remplacez-le par celui de la barre d'adresse quand vous
    // aurez résolu le challenge.
    // =========================================================================
    getList: function (page, query) {
        var url = query && query.length > 0
            ? SOURCE.baseUrl + "/recherche?q=" + encodeURIComponent(query)
            : SOURCE.baseUrl + "/liste-mangas?page=" + page;

        var html = http.get(url);
        var resultats = [];

        // ⚠ SÉLECTEUR À VÉRIFIER — regex à adapter sur le HTML réel.
        // Attendu : un lien vers une fiche, et une image de couverture.
        var reSerie = /<a[^>]+href="([^"]+)"[^>]*>\s*<img[^>]+src="([^"]+)"[^>]*>([\s\S]{0,400}?)<\/a>/g;
        var m;
        while ((m = reSerie.exec(html)) !== null) {
            var href = m[1];
            // On ignore ce qui n'est pas une fiche de série.
            if (href.indexOf("http") !== 0 && href.charAt(0) !== "/") continue;
            resultats.push({
                id: href,
                title: SOURCE.titreDe(m[3]) || href,
                coverUrl: m[2],
                url: href,
            });
        }
        return JSON.stringify(resultats);
    },

    // =========================================================================
    // getDetails(mangaUrl) → JSON.stringify({title, description, coverUrl,
    //                          status, authors, tags, chapters})
    //
    // ⚠ TOUS LES SÉLECTEURS CI-DESSOUS SONT À VÉRIFIER : aucun HTML du site
    // n'a pu être lu (voir l'avertissement en tête de fichier).
    // =========================================================================
    getDetails: function (mangaUrl) {
        var html = http.get(SOURCE.baseUrl + mangaUrl);

        var titre = SOURCE.texteDe(html, /<h1[^>]*>([\s\S]{0,120}?)<\/h1>/i);
        var description = SOURCE.texteDe(html, /<div[^>]+class="[^"]*synopsis[^"]*"[^>]*>([\s\S]{0,2000}?)<\/div>/i);
        var coverUrl = (/<img[^>]+class="[^"]*cover[^"]*"[^>]+src="([^"]+)"/i.exec(html) || [])[1] || "";

        // `status` : "ongoing" | "finished" | "paused" | "abandoned" | "unknown"
        var statut = "unknown";
        var brut = SOURCE.texteDe(html, /<span[^>]+class="[^"]*status[^"]*"[^>]*>([\s\S]{0,40}?)<\/span>/i).toLowerCase();
        if (brut.indexOf("en cours") >= 0 || brut.indexOf("ongoing") >= 0) statut = "ongoing";
        else if (brut.indexOf("termin") >= 0 || brut.indexOf("finished") >= 0) statut = "finished";
        else if (brut.indexOf("pause") >= 0) statut = "paused";

        // Chapitres, du plus ancien au plus récent. `number` sert au tri ; si
        // le site ne donne pas de numéro, on utilise l'ordre du tableau.
        var chapitres = [];
        var reChap = /<a[^>]+href="([^"]+)"[^>]*>([\s\S]{0,120}?)<\/a>/g;
        var c;
        var vus = {};
        while ((c = reChap.exec(html)) !== null) {
            var href = c[1];
            if (vus[href]) continue; // le même chapitre apparaît souvent deux fois
            var libelle = SOURCE.titreDe(c[2]);
            if (!libelle || libelle.indexOf("chapitre") < 0) continue;
            vus[href] = true;
            chapitres.push({
                id: href,
                title: libelle,
                number: SOURCE.numeroDe(libelle),
                url: href,
            });
        }
        chapitres.sort(function (a, b) { return a.number - b.number; });

        return JSON.stringify({
            title: titre || "Sans titre",
            description: description,
            coverUrl: coverUrl,
            status: statut,
            authors: [],
            tags: [],
            chapters: chapitres,
        });
    },

    // =========================================================================
    // getPages(chapterUrl) → JSON.stringify([{id, url}, …])
    //
    // `url` doit être une URL **absolue** en https:// : le moteur refuse les
    // URLs relatives pour les images (mesuré le 2026-10-08 : sans ce contrôle,
    // un script pouvait faire sortir une requête vers le réseau local).
    // =====================================================================
    getPages: function (chapterUrl) {
        var html = http.get(SOURCE.baseUrl + chapterUrl);
        var pages = [];
        var vues = {};

        // 1) `<img>` réels — la forme la plus courante sur un lecteur.
        var reImg = /<img[^>]+(?:data-src|src)="(https:\/\/[^"]+\.(?:jpe?g|png|webp))"/gi;
        var m;
        while ((m = reImg.exec(html)) !== null) {
            if (vues[m[1]]) continue;
            vues[m[1]] = true;
            pages.push({ id: m[1], url: m[1] });
        }
        if (pages.length > 0) return JSON.stringify(pages);

        // 2) Repli : un tableau JavaScript dans la page.
        //    Mesuré sur d'autres sites : `var pages = ["001.jpg", …]`.
        var tab = /var\s+pages\s*=\s*(\[[^\]]+\])/.exec(html);
        if (tab) {
            var noms;
            try { noms = JSON.parse(tab[1]); } catch (e) { noms = null; }
            if (noms) {
                for (var i = 0; i < noms.length; i++) {
                    var u = SOURCE.absolu(String(noms[i]), chapterUrl);
                    pages.push({ id: String(noms[i]), url: u });
                }
                return JSON.stringify(pages);
            }
        }
        // Aucun repli : une liste vide vaut mieux qu'une liste fausse — le
        // lecteur affichera « aucune page », pas des images cassées.
        return JSON.stringify(pages);
    },

    // =========================================================================
    // Utilitaires — sans état, testables dans la console du moteur.
    // =========================================================================

    /** Retire les balises et_entities d'un fragment, puis normalise les espaces. */
    titreDe: function (html) {
        return String(html)
            .replace(/<[^>]*>/g, " ")
            .replace(/&nbsp;/g, " ")
            .replace(/&amp;/g, "&")
            .replace(/&quot;/g, '"')
            .replace(/&#39;/g, "'")
            .replace(/&lt;/g, "<")
            .replace(/&gt;/g, ">")
            .replace(/\s+/g, " ")
            .trim();
    },

    /** Premier groupe d'une regex, ou une chaîne vide. */
    texteDe: function (html, re) {
        var m = re.exec(html);
        return m ? SOURCE.titreDe(m[1]) : "";
    },

    /** Extrait le numéro d'un libellé du type « Chapitre 12.5 ». */
    numeroDe: function (libelle) {
        var m = /(\d+(?:[.,]\d+)?)/.exec(libelle);
        return m ? parseFloat(m[1].replace(",", ".")) : 0;
    },

    /** Rend une URL absolue à partir d'un chemin relatif de la page. */
    absolu: function (href, chapterUrl) {
        if (!href) return "";
        if (href.indexOf("https://") === 0) return href;
        if (href.indexOf("//") === 0) return "https:" + href;
        if (href.charAt(0) === "/") return SOURCE.baseUrl + href;
        // Relatif au dossier du chapitre.
        var dossier = chapterUrl.substring(0, chapterUrl.lastIndexOf("/") + 1);
        return SOURCE.baseUrl + dossier + href;
    },
};