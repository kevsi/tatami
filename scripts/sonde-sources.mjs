#!/usr/bin/env node
// Sonde d'état des sources : rejoue le **parcours complet** du lecteur sur un
// ensemble de sources et note, pour chacune, à quelle étape elle s'arrête.
//
// ## Pourquoi une sonde, et pas un test
//
// Une source n'est pas une fonction : c'est un site web qui change sans prévenir.
// Le seul état qui vaille est un état **mesuré à une date donnée**. Cette sonde
// produit ce relevé ; `contract/sources-etat.json` le fige ; l'interface s'en
// sert pour distinguer « hors service » de « à vérifier ».
//
// ## Pourquoi le parcours complet, et pas seulement `latest`
//
// Une source peut lister des titres et ne rien servir derrière. Mesuré le
// 2026-10-12 : sur les 98 sources FR, `latest` renvoyait 5 résultats pour
// SODSAIME, ASURASCANS, HENTAIORIGINES et DEATHTOLLSCANS — les quatre ont ensuite
// été validés jusqu'à l'octet d'image. Mais une source qui passe `latest` et
// casse sur `chapters` est **inutilisable**, et `latest` seul ne le voit pas.
// « Marche vraiment » = l'image est décodable.
//
// ## Pourquoi Node
//
// Le protocole est du NDJSON : une ligne JSON par requête, une réponse par ligne.
// Le faire en PowerShell a coûté une session entière de regex fragiles sur du
// JSON d'une seule ligne (mesuré : un simple `listSources` a demandé quatre
// passes de correction). `JSON.parse` n'a pas ce problème. Node est déjà la
// langue des scripts de ce dépôt (`moteur-sign.mjs`).
//
// ## Commandes
//
//   node scripts/sonde-sources.mjs                       # tout le catalogue
//   node scripts/sonde-sources.mjs --lang fr             # une langue
//   node scripts/sonde-sources.mjs --sources A,B,C       # une liste
//   node scripts/sonde-sources.mjs --deep                # jusqu'à fetchImage
//   node scripts/sonde-sources.mjs --workers 3            # moteurs en parallèle
//   node scripts/sonde-sources.mjs --out docs/mesures/… # écrit un JSON
//
// ## Ce qu'elle ne fait pas
//
// Elle ne **corrige** rien. Elle mesure. Le classement qui transforme un relevé
// en décision vit dans `contract/sources-etat.json`, versionné et relu à l'œil.

import { spawn } from "node:child_process";
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const RACINE = join(dirname(fileURLToPath(import.meta.url)), "..");
const DIST = join(RACINE, "engine", "dist", "tatami-engine");
const JAVA = join(DIST, "runtime", "bin", "java.exe");

function arg(flag, defaut) {
  const i = process.argv.indexOf(flag);
  return i >= 0 && process.argv[i + 1] ? process.argv[i + 1] : defaut;
}
const aOption = (flag) => process.argv.includes(flag);

/**
 * Traduit une étape ratée en verdict de catalogue.
 *
 * ## Pourquoi une couche de plus, alors que le moteur a déjà un code
 *
 * Parce que le MÊME code ne veut pas dire la même chose selon l'étape. Mesuré
 * le 2026-10-12 : REMANGA répond 20 titres, une fiche, 5 chapitres et 11 pages
 * sans problème, puis `fetchImage` renvoie `CHALLENGE_REQUIRED`. Classé
 * tel quel, il rejoint les 15 sources « derrière Cloudflare » — qui, elles, ne
 * servent **rien** du tout sans vérification. Lui affiche son catalogue
 * parfaitement ; c'est la lecture qui est bloquée. Les confondre ferait
 * jeter une source qui marche à 4 étapes sur 5.
 *
 * C'est le groupe que la lecture naïve ne voit pas : un `200 OK` sur toutes les
 * étapes navigables, et un mur sur les images.
 *
 * `HANG` n'est pas une panne du site : c'est la sonde qui n'a pas eu de réponse
 * dans le délai. On le nomme pour qu'il ne soit jamais compté comme un verdict
 * de source.
 */
function classer(res, etape) {
  if (res.code === "HANG") return `TIMEOUT_${etape.toUpperCase()}`;
  if (
    etape === "image" &&
    (res.code === "CHALLENGE_REQUIRED" || res.code === "CHALLENGE_UNREPLAYABLE")
  ) {
    return "IMAGE_BLOQUEE";
  }
  return res.code;
}

/**
 * Un moteur NDJSON.
 *
 * Le protocole est **séquentiel** : la ligne N+1 n'est traitée qu'après la N.
 * C'est mesuré le 2026-10-12 — envoyer cinq requêtes d'un bloc et les lire après
 * produit des `InterruptedException` sur les dernières, parce que le moteur les
 * tue en changeant de ligne. D'où `appel()`, qui écrit **puis attend** sa
 * réponse avant d'écrire la suivante.
 */
function creerMoteur() {
  const proc = spawn(
    JAVA,
    [
      "-Xmx220m",
      "-XX:+UseSerialGC",
      "-cp",
      `${join(DIST, "lib", "*")}`,
      "org.tatami.engine.MainKt",
      "serve",
    ],
    { cwd: DIST, stdio: ["pipe", "pipe", "pipe"] },
  );
  const lignes = [];
  let tampon = "";
  proc.stdout.setEncoding("utf8");
  proc.stdout.on("data", (morceau) => {
    tampon += morceau;
    const bouts = tampon.split("\n");
    tampon = bouts.pop() ?? "";
    for (const l of bouts) if (l.trim()) lignes.push(l);
  });
  let compteur = 0;
  let attendu = null;
  let resolveur = null;
  // Le moteur écrit ses avertissements sur stderr : les lire évite de remplir
  // le pipe et de bloquer le moteur (cf. `ListSources`, ScriptedSourceLoader).
  proc.stderr.resume();

  return {
    proc,
    async appel(method, params, delaiMs) {
      compteur += 1;
      const id = `s${compteur}`;
      const avant = lignes.length;
      attendu = id;
      return new Promise((resolve) => {
        resolveur = resolve;
        proc.stdin.write(`${JSON.stringify({ id, method, params })}\n`);
        const chrono = setTimeout(() => {
          if (attendu === id) {
            attendu = null;
            resolve(null);
          }
        }, delaiMs);
        const verif = setInterval(() => {
          if (attendu !== id) {
            clearInterval(verif);
            clearTimeout(chrono);
            return;
          }
          for (let i = avant; i < lignes.length; i += 1) {
            if (lignes[i].includes(`"${id}"`)) {
              clearInterval(verif);
              clearTimeout(chrono);
              attendu = null;
              resolve(lignes[i]);
              return;
            }
          }
        }, 60);
      });
    },
    arreter() {
      proc.kill();
    },
  };
}

/** Réduit une ligne NDJSON à ce que la sonde retient. */
function verdict(bloc) {
  if (bloc === null)
    return { ok: false, code: "HANG", message: "aucune reponse" };
  let o;
  try {
    o = JSON.parse(bloc);
  } catch {
    return { ok: false, code: "ILLISIBLE", message: bloc.slice(0, 120) };
  }
  if (o.ok === true) return { ok: true, code: "OK", result: o.result ?? {} };
  const e = o.error ?? {};
  return {
    ok: false,
    code: e.code ?? "INTERNAL",
    message: String(e.message ?? "").slice(0, 200),
  };
}

/** L'hôte final d'une URL, pour comparer au domaine déclaré. */
function hote(url) {
  try {
    return new URL(url).hostname.toLowerCase();
  } catch {
    return null;
  }
}

const DELAI_LISTE = Number(arg("--delai-liste", 90000));
const DELAI_ETAPE = Number(arg("--delai-etape", 30000));
const PROFOND = aOption("--deep");
const NB_WORKERS = Number(arg("--workers", 2));

async function principal() {
  // Reclasser un relevé déjà mesuré, sans sonder à nouveau. Mesuré le
  // 2026-10-12 : le balayage profond des 98 sources FR prend ~18 minutes, et
  // un simple ajustement de règle de classement ne doit pas coûter ça.
  const depuis = arg("--depuis", "");
  if (depuis) {
    const brut = JSON.parse(
      readFileSync(join(RACINE, depuis), "utf8").replace(/^\uFEFF/, ""),
    );
    const sources = classerTous(brut.releves);
    const chemin = join(RACINE, arg("--vers", "contract/sources-etat.json"));
    mkdirSync(dirname(chemin), { recursive: true });
    writeFileSync(
      chemin,
      `${JSON.stringify({ schema: 1, mesure: brut.date, sources }, null, 2)}\n`,
      "utf8",
    );
    const parEtat = {};
    for (const s of Object.values(sources))
      parEtat[s.etat] = (parEtat[s.etat] ?? 0) + 1;
    console.log(
      `reclassement de ${depuis} (mesure du ${brut.date}) — ${Object.keys(sources).length} sources`,
    );
    console.log("\n--- etats de catalogue ---");
    for (const [k, v] of Object.entries(parEtat).sort((a, b) => b[1] - a[1])) {
      console.log(`${k.padEnd(24)} ${v}`);
    }
    console.log(`\nclassification ecrite : ${chemin}`);
    return;
  }

  const premier = creerMoteur();
  await premier.appel("hello", {}, DELAI_LISTE);

  let catalogue;
  if (aOption("--sources")) {
    const noms = arg("--sources")
      .split(",")
      .map((s) => s.trim())
      .filter(Boolean);
    catalogue = noms.map((name) => ({ name, lang: "?", domains: [] }));
  } else {
    const r = verdict(await premier.appel("listSources", {}, DELAI_LISTE));
    if (!r.ok) {
      premier.arreter();
      throw new Error(`listSources a echoue : ${r.code} ${r.message}`);
    }
    catalogue = r.result.sources ?? [];
  }
  premier.arreter();

  const filtre = arg("--lang", "");
  const cibles = catalogue.filter(
    (s) => !s.name.startsWith("custom:") && (!filtre || s.lang === filtre),
  );
  console.log(
    `${cibles.length} source(s) a sonder${filtre ? ` (langue ${filtre})` : ""}`,
  );

  const releves = [];
  const date = new Date().toISOString().slice(0, 10);

  for (let i = 0; i < cibles.length; i += NB_WORKERS) {
    const lot = cibles.slice(i, i + NB_WORKERS);
    await Promise.all(
      lot.map(async (src) => {
        const m = creerMoteur();
        await m.appel("hello", {}, DELAI_LISTE);
        const releve = {
          source: src.name,
          lang: src.lang,
          domaines: src.domains ?? [],
          date,
          etapes: {},
          verdict: "INCONNU",
        };
        try {
          const liste = verdict(
            await m.appel(
              "latest",
              {
                source: src.name,
                offset: 0,
                limit: 24,
                query: "",
                tags: [],
                safeOnly: false,
              },
              DELAI_ETAPE,
            ),
          );
          releve.etapes.latest = {
            code: liste.code,
            n: (liste.result?.list ?? []).length,
          };
          if (!liste.ok) {
            releve.verdict = liste.code;
            releve.message = liste.message;
            if (liste.message) {
              const u = liste.message.match(/https?:\/\/[^\s,\]]+/)?.[0];
              if (u) releve.urlFinale = u;
            }
          } else if ((liste.result?.list ?? []).length === 0) {
            // 200 sans le moindre titre : ni panne, ni contenu. Ni `dead` ni
            // `ok` — c'est `INCERTAIN`, et c'est le groupe qui demande le plus
            // de soin parce qu'il n'y a rien à corriger.
            releve.verdict = "INCERTAIN";
            releve.message = "200 OK sans resultat";
          } else {
            // `url` et NON `publicUrl` : ce sont deux URL différentes et le
            // contrat fait la différence. `url` est l'identifiant que le parseur
            // attend (un UUID pour MangaDex), `publicUrl` est l'adresse lisible
            // par un humain. La sonde utilisait `publicUrl` et obtenait, mesuré
            // le 2026-10-12 sur MangaDex, `api.mangadex.org/manga/
            // https://mangadex.org/title/<uuid>` : un 404 fabriqué par la sonde
            // elle-même, qui concluait « source morte » sur une source saine.
            const url = liste.result.list[0].url;
            releve.urlListe = liste.result.list[0].publicUrl ?? null;
            const detail = verdict(
              await m.appel(
                "details",
                { source: src.name, url, mangaTitle: "" },
                DELAI_ETAPE,
              ),
            );
            releve.etapes.details = { code: detail.code };
            if (!detail.ok) {
              releve.verdict = detail.code;
              releve.message = detail.message;
            } else {
              const chap = verdict(
                await m.appel(
                  "chapters",
                  { source: src.name, mangaUrl: url, mangaTitle: "" },
                  DELAI_ETAPE,
                ),
              );
              const nbChap = (chap.result?.chapters ?? chap.result?.list ?? [])
                .length;
              releve.etapes.chapters = { code: chap.code, n: nbChap };
              if (!chap.ok) {
                releve.verdict = chap.code;
                releve.message = chap.message;
              } else if (nbChap === 0) {
                releve.verdict = "INCERTAIN";
                releve.message = "aucun chapitre";
              } else if (PROFOND) {
                const chUrl = (chap.result?.chapters ??
                  chap.result?.list ??
                  [])[0].url;
                const pages = verdict(
                  await m.appel(
                    "pages",
                    { source: src.name, chapterUrl: chUrl },
                    DELAI_ETAPE,
                  ),
                );
                releve.etapes.pages = {
                  code: pages.code,
                  n: (pages.result?.pages ?? []).length,
                };
                if (!pages.ok) {
                  releve.verdict = pages.code;
                  releve.message = pages.message;
                } else if ((pages.result?.pages ?? []).length === 0) {
                  // Un chapitre sans image n'est pas un «ok» : le lecteur
                  // n'affichera rien. Sans ce cas, la sonde demandait une
                  // `pageUrl` vide et le moteur répondait `missing param:
                  // pageUrl` — une faute de la sonde notée comme une panne du
                  // site (mesuré le 2026-10-12 sur MANGAHUB).
                  releve.verdict = "INCERTAIN";
                  releve.message = "chapitre sans aucune image";
                } else {
                  const pageUrl = (pages.result?.pages ?? [])[0]?.url;
                  const img = verdict(
                    await m.appel(
                      "fetchImage",
                      { source: src.name, pageUrl },
                      DELAI_ETAPE,
                    ),
                  );
                  releve.etapes.image = {
                    code: img.code,
                    type: img.result?.contentType ?? null,
                  };
                  releve.etapeEchec = "image";
                  // Le parcours s'arrête sur l'octet décodable : c'est la seule
                  // preuve qu'un lecteur peut vraiment afficher la source.
                  releve.verdict = img.ok ? "ACTIF" : classer(img, "image");
                  if (!img.ok) releve.message = img.message;
                }
              } else {
                releve.verdict = "ACTIF_LIGHT";
              }
            }
          }
        } catch (e) {
          releve.verdict = "CRASH";
          releve.message = String(e.message ?? e).slice(0, 200);
        } finally {
          m.arreter();
        }
        releves.push(releve);
        const etiquette = {
          ACTIF: "OK",
          ACTIF_LIGHT: "OK*",
          INCERTAIN: "?",
          IMAGE_BLOQUEE: "IMG",
        };
        process.stdout.write(
          `${String(i + lot.indexOf(src) + 1).padStart(4)}/${cibles.length}  ${src.name.padEnd(28)} ${etiquette[releve.verdict] ?? releve.verdict}\n`,
        );
      }),
    );
  }

  releves.sort((a, b) => a.source.localeCompare(b.source));
  const parVerdict = {};
  for (const r of releves)
    parVerdict[r.verdict] = (parVerdict[r.verdict] ?? 0) + 1;
  console.log("\n--- repartition ---");
  for (const [k, v] of Object.entries(parVerdict).sort((a, b) => b[1] - a[1])) {
    console.log(`${k.padEnd(18)} ${v}`);
  }

  const sortie = arg("--out", "");
  if (sortie) {
    const chemin =
      sortie.includes("/") || sortie.includes("\\")
        ? join(RACINE, sortie)
        : join(RACINE, sortie);
    mkdirSync(dirname(chemin), { recursive: true });
    writeFileSync(
      chemin,
      `${JSON.stringify({ date, releves }, null, 2)}\n`,
      "utf8",
    );
    console.log(`\nreleve ecrit : ${chemin}`);
  } else {
    console.log("\n(passe --out <chemin> pour ecrire le releve)");
  }

  if (aOption("--classer")) {
    const chemin = join(RACINE, arg("--vers", "contract/sources-etat.json"));
    mkdirSync(dirname(chemin), { recursive: true });
    const sources = classerTous(releves);
    writeFileSync(
      chemin,
      `${JSON.stringify({ schema: 1, mesure: date, sources }, null, 2)}\n`,
      "utf8",
    );
    const parEtat = {};
    for (const s of Object.values(sources))
      parEtat[s.etat] = (parEtat[s.etat] ?? 0) + 1;
    console.log("\n--- etats de catalogue ---");
    for (const [k, v] of Object.entries(parEtat).sort((a, b) => b[1] - a[1])) {
      console.log(`${k.padEnd(24)} ${v}`);
    }
    console.log(`\nclassification ecrite : ${chemin}`);
  }
}

/**
 * Traduit un relevé brut en décision de catalogue.
 *
 * ## Ce que la sonde ne peut pas décider seule
 *
 * Un relevé dit *ce qui s'est passé*, pas *ce qu'il faut faire*. Trois cas
 * demandent un arbitrage humain, et la sonde les laisse donc visibles plutôt
 * que de les trancher :
 *
 * - `incertain` — 200 sans résultat, ou délai dépassé. Ni panne, ni contenu.
 * - `certificat_invalide` — le site présente une chaîne que **deux** piles Java
 *   indépendantes refusent (Conscrypt et JSSE, mesuré le 2026-10-12 sur 8
 *   sources). Ce n'est donc pas notre TLS. On le distingue de `hors_service`
 *   parce qu'un navigateur, lui, sait passer outre l'avertissement.
 * - `reponse_incompatible` — le site a changé son format (JSON, champ typé).
 *   Le site vit, c'est le parseur qui est daté : c'est réparable.
 */
function classerTous(releves) {
  const out = {};
  for (const r of releves) {
    const v = r.verdict;
    const m = r.message ?? "";
    let etat;
    let motif;
    if (v === "ACTIF" || v === "ACTIF_LIGHT") {
      etat = "actif";
      motif = null;
    } else if (v === "ORIGIN_DOWN") {
      etat = "indisponible";
      motif = "origine du site injoignable (Cloudflare 52x) — peut revenir";
    } else if (v === "CHALLENGE_REQUIRED") {
      etat = "verification";
      motif = "derriere un challenge Cloudflare";
    } else if (v === "IMAGE_BLOQUEE") {
      etat = "images_bloquees";
      motif = "catalogue lisible, CDN d'images protege";
    } else if (v === "PARSER_OUTDATED") {
      etat = "parseur_perime";
      motif = "le site a change de page";
    } else if (v === "SOURCE_DEAD") {
      etat = "hors_service";
      motif =
        "domaine disparu, 404/410, ou domaine repris par une regie publicitaire";
    } else if (v === "INCERTAIN" || v === "HANG" || v === "PARSE_ERROR") {
      etat = "incertain";
      motif = m || "sonde non concluante";
    } else if (/SSLHandshake|SSLPeerUnverified/.test(m)) {
      etat = "certificat_invalide";
      motif = "chaine de certificats refusee par Conscrypt ET par JSSE";
    } else if (/JSONException|ClassCastException|NumberFormat/.test(m)) {
      etat = "reponse_incompatible";
      motif = "le site a change le format de sa reponse";
    } else {
      etat = "incertain";
      motif = m || v;
    }
    out[r.source] = {
      etat,
      langue: r.lang === "?" ? null : r.lang,
      mesure: r.date,
      etape: r.etapeEchec ?? null,
      motif,
    };
  }
  return out;
}

principal().catch((e) => {
  console.error(e.message);
  process.exit(1);
});
