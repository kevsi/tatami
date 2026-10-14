#!/usr/bin/env node
// Croise les deux mesures et tranche : **site mort, ou parseur cassé ?**
//
// ## Pourquoi croiser
//
// Chaque mesure seule ment dans un sens, et c'est ce qui a coûté des journées :
//
//   - le **moteur** seul dit « challenge » — sans dire si le site est mort, ni si
//     le parseur demande la bonne page ;
//   - le **navigateur** seul dit « le site répond » — sans dire si Tatami le
//     demande correctement, ni si nos images sechargent.
//
// La grille appliquée est celle de `docs/audit-flux-sources.md` :
//
//   navigateur mort + moteur mort ................ site_mort
//   navigateur sert + moteur refusé (même URL) ... client_refuse
//   navigateur sert + moteur refusé (autre URL) ... parseur_casse
//   navigateur muet + moteur refusé ............... parseur_casse
//   le parcours complet aboutit ................... ok
//   pas de mesure navigateur ..................... a_verifier  (jamais deviné)
//
// ## La règle qui ne se négocie pas
//
// **Pas de mesure navigateur → `a_verifier`, jamais un verdict.** Conclure
// « morte » sur la seule parole du moteur a déjà coûté une fausse entrée dans
// le catalogue ; l'inverse a coûté une source vivante déclarée morte.
//
// ## Usage
//
//   node scripts/normaliser-sites.mjs \
//     --moteur docs/mesures/audit-parcours-fr-2026-10-14.json \
//     --navigateur docs/mesures/navigateur-2026-10-14.json \
//     --sortie docs/mesures/sites-etat-normalise.json

import { readFileSync, writeFileSync } from "node:fs";

function arg(nom, defaut) {
  const i = process.argv.indexOf(`--${nom}`);
  return i >= 0 && process.argv[i + 1] ? process.argv[i + 1] : defaut;
}

const REPERE_MOTEUR = arg(
  "moteur",
  "docs/mesures/audit-parcours-fr-2026-10-14.json",
);
const REPERE_NAVIGATEUR = arg(
  "navigateur",
  "docs/mesures/navigateur-2026-10-14.json",
);
const SORTIE = arg("sortie", "docs/mesures/sites-etat-normalise.json");

/** Comparaison d'URL : le `/` final ne change pas la page, l'hôte en revanche. */
function memeUrl(a, b) {
  if (!a || !b) return false;
  const n = (u) => {
    try {
      const x = new URL(u);
      return `${x.hostname.toLowerCase()}${x.pathname.replace(/\/+$/, "")}${x.search}`;
    } catch {
      return String(u).replace(/\/+$/, "");
    }
  };
  return n(a) === n(b);
}

/** Décrit une étape, qu'elle soit un objet, une chaîne, ou absente. */
function decrireEtape(v) {
  // `typeof null === "object"` en JavaScript : sans ce test, une étape jamais
  // exécutée — que la sonde écrit `null` — fait échouer le rapport sur
  // `v.code`. C'est arrivé, sur les relevé où l'audit s'arrête à `latest`.
  if (v === null || v === undefined) return "non exécutée";
  if (typeof v === "object") return `${v.code ?? "?"} n=${v.n ?? 0}`;
  return String(v);
}

/** Le parcours complet a-t-il abouti ? Une image décodée, pas un statut. */
function abouti(releve) {
  const etapes = releve.etapes ?? {};
  const image = etapes.fetchImage ?? etapes.pages;
  if (!image || typeof image !== "object") {
    return { ok: false, detail: "aucune étape image enregistrée" };
  }
  const n = image.n ?? 0;
  const code = image.code ?? "";
  return {
    ok: n > 0 && !/ERROR|INTERNAL|CHALLENGE|TIMEOUT/i.test(code),
    detail: `${code} n=${n}`,
  };
}

/**
 * La conclusion. Une fonction, et non une cascade dispersée dans le fichier :
 * c'est elle qui définit ce que « dead » veut dire, donc elle doit se lire d'un
 * seul endroit.
 */
/**
 * Les verdicts que le moteur tranche **seul**, sans mesure navigateur.
 *
 * Ce n'est pas se fier à lui : chacun est la définition même du mot.
 * - `ORIGIN_DOWN` n'est émis que sur un **52x de Cloudflare** (521/522/523/525) :
 *   c'est Cloudflare lui-même qui dit que l'origine ne répond plus. Un
 *   navigateur dirait exactement la même chose — on l'a vérifié sur 8 sources.
 * - `SOURCE_DEAD` : le domaine a disparu.
 * - `PARSER_OUTDATED` : le parseur ne reconnaît plus la page qu'il reçoit.
 *   **Attention**, c'est un diagnostic de parseur, pas de site : rien ici ne
 *   dit que le site est mort, et c'est précisément la confusion à éviter.
 *
 * Tout le reste exige le croisement. Un `CHALLENGE_REQUIRED` ne dit rien du
 * site, et un `INTERNAL` ne dit rien du tout.
 */
const DECISIFS_SANS_NAVIGATEUR = {
  ORIGIN_DOWN: () => ({
    conclusion: "site_mort",
    raison: "Cloudflare répond 52x : c'est lui qui dit que l'origine ne répond plus",
  }),
  SOURCE_DEAD: () => ({
    conclusion: "site_mort",
    raison: "le domaine a disparu",
  }),
  PARSER_OUTDATED: () => ({
    conclusion: "parseur_casse",
    raison: "le parseur ne reconnaît plus la page qu'il reçoit — le site, lui, n'est pas jugé",
  }),
  // `ACTIF` est la seule preuve positive de tout le relevé : le parcours
  // complet, **image décodée comprise**, a abouti. Il n'a rien à croiser, et le
  // croiser avec un navigateur n'apporterait rien — au contraire, un navigateur
  // qui charge la page ne dit pas que le lecteur s'affiche.
  ACTIF: () => ({
    conclusion: "ok",
    raison: "parcours complet abouti jusqu'à une image décodée",
  }),
};

/**
 * Deux verdicts que le **titre** de la page révèle, et que le statut seul ne
 * disait pas. Mesurés le 2026-10-14 sur les 43 sources de l'audit :
 *
 * - `bentoscan.com for sale — memorable .com domain | ED.com`
 * - `papscan.com is for sale — Buy now for $8,300 | GoDaddy`
 *
 * Un domaine parking n'est pas « mort » au sens d'une panne : le site a été
 * **revendu**. La conclusion est la même — à personne de le lire — mais le motif
 * doit le dire, sinon la fiche rapporterait une panne alors que la vraie action est
 * de nettoyer le catalogue.
 */
function verdictParTitre(titre) {
  const t = String(titre ?? "");
  if (/is for sale|for sale —|for sale \||domain.*for sale|memorable \.com/i.test(t)) {
    return {
      conclusion: "site_mort",
      raison: `domaine parking mis en vente — « ${t.slice(0, 60)} » : le site n'existe plus, il a été revendu`,
    };
  }
  if (/erreur de confidentialité|privacy error|certificate|ssl error/i.test(t)) {
    return {
      conclusion: "site_mort",
      raison: `certificat refusé par le navigateur — « ${t.slice(0, 60)} » : aucun client TLS ne peut lire ce site`,
    };
  }
  return null;
}

function conclure(verdictMoteur, nav, urlDemandee) {
  const decisif = DECISIFS_SANS_NAVIGATEUR[verdictMoteur];
  if (decisif) return decisif();
  if (!nav) {
    return {
      conclusion: "a_verifier",
      raison: `verdict moteur « ${verdictMoteur} » sans conclusion possible : il manque la mesure navigateur`,
    };
  }
  const parTitre = verdictParTitre(nav.titre);
  if (parTitre) return parTitre;
  if (nav.verdict === "MORTE") {
    return {
      conclusion: "site_mort",
      raison: `Cloudflare ${nav.titre || "52x"} — l'origine est éteinte, aucun client ne peut la lire`,
    };
  }
  if (nav.verdict === "TLS") {
    return {
      conclusion: "site_mort",
      raison: `chaîne TLS de l'origine cassée (${nav.titre}) — pas un problème de challenge`,
    };
  }
  const refuse =
    /CHALLENGE_REQUIRED|CHALLENGE_UNREPLAYABLE|SOURCE_BLOCKED/.test(verdictMoteur ?? "");
  if (nav.verdict === "UTILE") {
    if (refuse) {
      return memeUrl(urlDemandee, nav.url)
        ? {
            conclusion: "client_refuse",
            raison:
              "le navigateur sert cette URL, le moteur se fait refuser dessus — notre client",
          }
        : {
            conclusion: "parseur_casse",
            raison: `le navigateur sert ${nav.url}, le moteur demande ${urlDemandee} — le parseur vise la mauvaise page`,
          };
    }
    // Le navigateur sert la page **et** le moteur l'a lue sans être refusé, et
    // il a quand même échoué (`INTERNAL`, `INCERTAIN`, `TIMEOUT_IMAGE`). Ce
    // n'est donc **pas** une source qui marche : c'est un parseur qui lit mal
    // une page qui existe. La conclusion « ok » serait exactement le genre de
    // raccourci que ce fichier interdit.
    return {
      conclusion: "parseur_casse",
      raison: `le navigateur sert cette page, le moteur l'a lue et a quand même échoué (${verdictMoteur}) — le parseur n'en extrait rien`,
    };
  }
  // VIDE ou PERIMEE : l'URL demandée ne rend rien, même dans un navigateur.
  return {
    conclusion: "parseur_casse",
    raison: `${urlDemandee} ne rend pas de contenu exploitable dans un vrai navigateur (${nav.titre || nav.verdict})`,
  };
}

/** Qui doit agir. Une conclusion sans action est un constat mort. */
function action(conclusion) {
  switch (conclusion) {
    case "site_mort":
      return "retirer du catalogue, avec la date du relevé dans le motif";
    case "parseur_casse":
      return "source personnalisée vers la bonne page, ou correction amont du parseur";
    case "client_refuse":
      return "transport webview — à vérifier jusqu'à l'image affichée";
    case "ok":
      return "rien";
    default:
      return "mesurer dans un navigateur avant de conclure quoi que ce soit";
  }
}

const audit = JSON.parse(readFileSync(REPERE_MOTEUR, "utf8"));
const nav = JSON.parse(readFileSync(REPERE_NAVIGATEUR, "utf8"));

const parSourceNav = new Map(nav.resultats.map((r) => [r.source, r]));

const sites = (audit.releves ?? []).map((r) => {
  const mesureNav = parSourceNav.get(r.source) ?? null;
  const urlDemandee = r.urlFinale ?? null;
  const { conclusion, raison } = conclure(r.verdict, mesureNav, urlDemandee);
  const fin = abouti(r);
  return {
    source: r.source,
    conclusion,
    raison,
    action: action(conclusion),
    url_demandee: urlDemandee,
    preuve_moteur: {
      verdict: r.verdict,
      message: r.message ?? null,
      etapes: Object.fromEntries(
        Object.entries(r.etapes ?? {}).map(([k, v]) => [k, decrireEtape(v)]),
      ),
    },
    preuve_navigateur: mesureNav
      ? {
          url: mesureNav.url,
          verdict: mesureNav.verdict,
          statuts: mesureNav.statuts,
          titre: mesureNav.titre,
          liensInternes: mesureNav.liensInternes,
          octetsHtml: mesureNav.octetsHtml,
        }
      : null,
    parcours_complet_abouti: fin.ok,
    detail_parcours: fin.detail,
  };
});

const parConclusion = {};
for (const s of sites) parConclusion[s.conclusion] = (parConclusion[s.conclusion] ?? 0) + 1;

const sortie = {
  genere_le: new Date().toISOString().slice(0, 19).replace("T", " "),
  question: "Pour chaque source : le site est-il mort, ou le parseur est-il cassé ?",
  methode: {
    moteur: {
      outil: "scripts/sonde-sources.mjs --deep",
      releve: REPERE_MOTEUR,
      mesure: "latest → details → chapters → pages → image",
    },
    navigateur: {
      outil: "scripts/sonde-navigateur.mjs",
      releve: REPERE_NAVIGATEUR,
      mesure: "l'URL réellement demandée par le moteur, dans le WebView2 de l'application",
    },
  },
  regle: "Pas de mesure navigateur → a_verifier. Jamais un verdict deviné.",
  compte: parConclusion,
  sites,
};

writeFileSync(SORTIE, `${JSON.stringify(sortie, null, 2)}\n`, "utf8");

console.log(`\n=== ${sites.length} sources, croisées ===`);
for (const [k, v] of Object.entries(parConclusion).sort((a, b) => b[1] - a[1])) {
  console.log(`${k.padEnd(14)} ${v}`);
}
console.log(`\nécrit : ${SORTIE}`);
console.log("\n--- ce qui demande une action ---");
for (const s of sites.filter((x) => x.conclusion !== "ok")) {
  console.log(`${s.source.padEnd(22)} ${s.conclusion.padEnd(14)} ${s.raison.slice(0, 78)}`);
}
