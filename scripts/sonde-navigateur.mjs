#!/usr/bin/env node
// Sonde « et ce que voit un vrai navigateur ? » : ouvre chaque URL dans le
// **WebView2 de l'application**, résout l'interstitielle Cloudflare, puis relève
// statut, poids de page et **nombre de liens internes**.
//
// ## Pourquoi une sonde, et pas un test
//
// Une source n'est pas une fonction : c'est un site qui s'allume, s'éteint et
// change d'adresse. Le seul état qui vaille est un état **mesuré à une date
// donnée**. Cette sonde produit le relevé ; `docs/mesures/` le fige.
// Cf. `scripts/sonde-sources.mjs` pour le parcours complet côté moteur.
//
// ## Pourquoi le WebView2 de l'application, et pas un Edge lancé à part
//
// Le transport de l'option B **est** le WebView2 de l'application. Une mesure
// faite dans un autre Chromium mesurerait autre chose : c'est le navigateur du
// transport qu'il faut éprouver. D'où le port 9223, ouvert par
// `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9223`.
// Cf. `challenge.rs` → `webview_lire`.
//
// ## Deux pièges déjà payés, pour qu'on ne les paie pas deux fois
//
// 1. **Il faut laisser la page finir de peindre.** La sonde s'arrêtait dès la
//    disparition de l'interstitielle, donc **avant** le rendu : TOONFR était
//    relevé à 43 Ko / 0 lien alors que la page chargée fait 177 Ko / 48 liens.
//    La sonde mesurait sa propre impatience. D'où `TEMPS_STABILISATION`.
// 2. **Un `200` sans lien ne prouve rien.** C'est *exactement* le piège de
//    TOONFR, dont l'AJAX rend 200 avec 0 octet. Le statut seul classe donc
//    « VIDE » comme un succès. On compte les liens, toujours.
//
// ## Pourquoi les verdicts sont distincts
//
// Mesuré le 2026-10-14 : parmi les 15 sources FR `CHALLENGE_REQUIRED`, **8
// étaient mortes** (Cloudflare `523 Origin is unreachable` — le serveur
// derrière est éteint). Aucun client ne peut les lire, transport ou pas : les
// compter comme « problème de transport » aurait envoyé chercher une parade
// qui n'existe pas. `VERDICTS` sépare donc ces cas, et ils ne sont pas du même
// travail.
//
// ## La leçon qui a coûté le plus cher, et elle est dans ce fichier
//
// EPSILONSOFT a été classée `PERIMEE` le 2026-10-14 : `/manga/` répond 404 sur
// `epsilonsoft.to`. Or le bytecode du parser porte
// `ldc "beta.epsilonsoft.to"` : **le mauvais domaine**. L'apex sert une fausse
// mise à jour de navigateur (adware), le `beta.` est le site réel — 200,
// 86 liens de série. Règle appliquée ici : **une URL est une valeur reçue,
// il faut interroger son producteur avant de juger l'hôte.** Ici le producteur
// c'est le bytecode du parser, pas le DNS.
//
// ## Utilisation
//
// ## Usage
//
// ```powershell
// # tout le relevé
//   $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS="--remote-debugging-port=9223"
//   node scripts\sonde-navigateur.mjs --sortie docs\mesures\navigateur-AAAAMM-JJ.json
//
// re-tester un groupe (les 8 mortes, le lendemain)
//   node scripts\sonde-navigateur.mjs --groupe morte --sortie <fichier>
// ```

import { readFileSync, writeFileSync } from "node:fs";

const PORTA_CDP = Number(process.env.CDP_PORT ?? 9223);
const CLE_CIBLES = arg("cibles", "docs/mesures/navigateur-cibles.json");
/** Plafond de l'attente de stabilité, en secondes. Garde-fou, pas la mesure. */
const ATTENTE_STABILITE_S = 25;
/**
 * Nombre de releves (500 ms) avant d'accepter une stabilite.
 * Sans ce plancher, « 0 lien deux fois de suite » passe pour une page stable
 * alors qu'elle n'est pas encore arrivee — mesure le 2026-10-14 sur HENTAIZONE.
 */
const PLANCHER_STABILITE_S = 8;

/**
 * Compte les liens internes **distincts**, sans fragment ni query string.
 * C'est le nombre qui décide du verdict, pas le statut HTTP.
 */
const exprLiens = `JSON.stringify((() => {
  const vus = new Set();
  for (const a of document.querySelectorAll('a[href]')) {
    const h = a.getAttribute('href');
    if (h && !h.startsWith('#') && h.length > 4) vus.add(h.split('?')[0]);
  }
  return vus.size;
})())`;

async function compterLiens(send, sid) {
  return Number((await evalJs(send, sid, exprLiens)) ?? 0);
}

/** Un statut seul ne suffit pas : le titre Cloudflare dit *pourquoi*. */
const VERDICTS = {
  MORTE: "Origine injoignable (Cloudflare 522 délai dépassé / 523 éteinte) — aucun client ne peut la lire",
  TLS: "TLS (525, handshake d'origine cassé) — pas un problème de challenge",
  UTILE: "SERVIE par le navigateur — cible réelle pour le transport",
  VIDE: "200 sans contenu exploitable — l'URL demandée est mauvaise ou l'AJAX est vide",
  PERIMEE:
    "404 sur CETTE URL — à elle seule elle ne dit **pas** si le site est vivant. Mesuré le 2026-10-14 : `/manga/` répond 404 sur epsilonsoft.to alors que `beta.epsilonsoft.to` (le seul hôte que le parser interroge) rend 86 liens. Le site-vivant est une **autre** mesure.",
  BLOQUE: "Aucun statut 200 et pas de code d'origine Cloudflare connu — **verdict à ne pas conclure** : réessayer ou lire la page manuellement",
};

function arg(nom, defaut) {
  const i = process.argv.indexOf(`--${nom}`);
  return i >= 0 && process.argv[i + 1] ? process.argv[i + 1] : defaut;
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

function connecter(url) {
  return new Promise((resolve, reject) => {
    const ws = new WebSocket(url);
    ws.onopen = () => resolve(ws);
    ws.onerror = (e) => reject(new Error(`ws ${e.message ?? e.type}`));
  });
}

/** Client CDP minimal : `send` (rpc) + `events` (réseau), sur une session à plat. */
function rpc(ws, idRef) {
  const pending = new Map();
  const events = [];
  ws.onmessage = (ev) => {
    let msg;
    try {
      msg = JSON.parse(ev.data);
    } catch {
      return;
    }
    if (msg.id && pending.has(msg.id)) {
      const { resolve, reject } = pending.get(msg.id);
      pending.delete(msg.id);
      if (msg.error) reject(new Error(JSON.stringify(msg.error)));
      else resolve(msg.result);
    } else if (msg.method) {
      events.push(msg);
    }
  };
  const send = (method, params = {}, sessionId) =>
    new Promise((resolve, reject) => {
      const id = ++idRef.n;
      pending.set(id, { resolve, reject });
      const payload = { id, method, params };
      if (sessionId) payload.sessionId = sessionId;
      ws.send(JSON.stringify(payload));
    });
  return { send, events };
}

async function evalJs(send, sid, expression) {
  const r = await send(
    "Runtime.evaluate",
    { expression, returnByValue: true, awaitPromise: true },
    sid,
  );
  if (r.exceptionDetails) return { erreur: r.exceptionDetails.text };
  return r.result.value;
}

/**
 * Classe le relevé. L'ordre compte : le titre Cloudflare l'emporte sur le
 * statut.
 *
 * **Le trou que le premier relevé a mis au jour.** Le 2026-10-14,
 * MYHEROACADEMIASCAN a répondu `522 Connection timed out` — un code que ce
 * tableau ne connaissait pas, donc classé `BLOQUE`, ce qui est faux : 522 est
 * la même famille que 523, l'origine ne répond pas. Cloudflare a un petit
 * nombre de codes d'origine injoignable (522 délai dépassé, 523 éteinte) et un
 * code de handshake (525) ; les traiter « génériquement » ferait compter une
 * source morte parmi les sources bloquées par notre client — exactement la
 * confusion que ce fichier existe pour empêcher.
 */
function classer(statuts, titre, liens, octets) {
  const t = String(titre ?? "");
  if (/(522|523):\s*(connection timed out|origin is unreachable)/i.test(t)) return "MORTE";
  if (/525:\s*ssl handshake failed/i.test(t)) return "TLS";
  if (statuts.length === 1 && statuts[0] === 404) return "PERIMEE";
  if (statuts.includes(200)) return liens >= 5 ? "UTILE" : "VIDE";
  return "BLOQUE";
}

/** Ouvre l'URL dans un onglet de la webview, attend le rendu, relève. */
async function mesurer(send, events, url) {
  const { targetId } = await send("Target.createTarget", { url: "about:blank" });
  const { sessionId } = await send("Target.attachToTarget", { targetId, flatten: true });
  await send("Page.enable", {}, sessionId);
  await send("Network.enable", {}, sessionId);
  await send("Runtime.enable", {}, sessionId);
  try {
    const t0 = Date.now();
    await send("Page.navigate", { url }, sessionId);

    // 1. Attendre la disparition de l'interstitielle.
    //
    //    **Un titre vide n'est pas une page chargée.** La condition ci-dessous
    //    le traite comme « encore en cours » : sinon la sonde sortait sur un
    //    `about:blank` et déclarait 0 lien. C'est ce qui a fait mesurer
    //    HENTAIZONE à 0 lien alors qu'elle en a 48.
    const statuts = [];
    let titre = "";
    for (let i = 0; i < ATTENTE_STABILITE_S * 2; i++) {
      await sleep(500);
      statuts.length = 0;
      for (const ev of events) {
        if (ev.method !== "Network.responseReceived" || ev.sessionId !== sessionId) continue;
        if (ev.params.type === "Document") statuts.push(ev.params.response.status);
      }
      titre = (await evalJs(send, sessionId, "document.title")) ?? "";
      if (String(titre).trim() !== "" && !/just a moment|un instant|attention required/i.test(String(titre))) break;
    }

    // 2. **Attendre que la page se stabilise**, pas un délai fixe.
    //
    //    Un délai fixe est une conjecture, et le relevé l'a démontré : deux runs
    //    le même jour, TOONFR est mesuré à 127 liens puis à **1** lien. Ce n'est
    //    pas le site qui change, c'est la sonde qui renonce avant le rendu.
    //
    //    Deux conditions, parce qu'une seule suffit à mentir :
    //      - **le nombre de liens ne bouge plus** ;
    //      - **après un plancher minimal**, sinon 0 lien lu deux fois de suite
    //        paraît « stable » alors que la page n'est simplement pas encore
    //        arrivée. Ce n'est pas une théorie : c'est exactement ce qui a fait
    //        relever HENTAIZONE à 0 lien.
    let liens = 0;
    let precedent = -1;
    for (let i = 0; i < ATTENTE_STABILITE_S * 2; i++) {
      liens = await compterLiens(send, sessionId);
      const Chargée = i >= PLANCHER_STABILITE_S;
      if (Chargée && liens === precedent) break;
      precedent = liens;
      await sleep(500);
    }
    const octets = Number(
      (await evalJs(send, sessionId, "document.documentElement.outerHTML.length")) ?? 0,
    );

    const stats = [...new Set(statuts)];
    return {
      statuts: stats,
      titre: String(titre).slice(0, 70),
      liensInternes: liens,
      octetsHtml: octets,
      ms: Date.now() - t0,
      verdict: classer(stats, titre, liens, octets),
    };
  } finally {
    // Refermer l'onglet : un onglet laissé ouvert s'empile et fausse le suivant.
    await send("Target.closeTarget", { targetId });
  }
}

// ── exécution ────────────────────────────────────────────────────────────

const catalogue = JSON.parse(readFileSync(CLE_CIBLES, "utf8"));
const cibles = catalogue.sources;
const groupe = arg("groupe", null);
const aTester = groupe ? cibles.filter((c) => c.groupe === groupe) : cibles;

if (!aTester.length) {
  console.error(
    `Aucune cible dans le groupe « ${groupe} ». Groupes : ${[...new Set(cibles.map((c) => c.groupe))].join(", ")}`,
  );
  process.exit(1);
}

const base = `http://127.0.0.1:${PORTA_CDP}`;
let version;
try {
  version = await fetch(`${base}/json/version`).then((r) => r.json());
} catch {
  console.error(
    `CDP absent sur ${base}.\n` +
      `Démarrez l'application avec :\n` +
      `  $env:WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS="--remote-debugging-port=${PORTA_CDP}"\n` +
      `  pnpm tauri dev`,
  );
  process.exit(2);
}

const ws = await connecter(version.webSocketDebuggerUrl);
const idRef = { n: 0 };
const { send, events } = rpc(ws, idRef);

const resultats = [];
for (const { source, url } of aTester) {
  process.stdout.write(`${source.padEnd(22)} ${url.slice(0, 42).padEnd(43)} `);
  try {
    const r = await mesurer(send, events, url);
    console.log(
      `${r.verdict.padEnd(8)} statuts=[${r.statuts}] liens=${r.liensInternes} ` +
        `html=${r.octetsHtml} ${Math.round(r.ms / 1000)}s "${r.titre}"`,
    );
    resultats.push({ source, url, ...r });
  } catch (e) {
    console.log(`ERREUR ${String(e).slice(0, 80)}`);
    resultats.push({ source, url, verdict: "ERREUR", erreur: String(e).slice(0, 160) });
  }
}
ws.close();

// ── écriture du relevé ───────────────────────────────────────────────────

const parVerdict = {};
for (const r of resultats) parVerdict[r.verdict] = (parVerdict[r.verdict] ?? 0) + 1;

const releve = {
  date: new Date().toISOString().slice(0, 10),
  groupe: groupe ?? "tous",
  navigateur: version.Browser,
  portCdp: PORTA_CDP,
  question:
    "Un vrai navigateur obtient-il du contenu sur les URLs que le client HTTP du moteur n'atteint pas ?",
  lecture: Object.fromEntries(
    Object.entries(VERDICTS).map(([k, v]) => [k, { signification: v, nombre: parVerdict[k] ?? 0 }]),
  ),
  resultats,
};

const sortie = arg("sortie", null);
if (sortie) {
  writeFileSync(sortie, `${JSON.stringify(releve, null, 2)}\n`, "utf8");
  console.log(`\nrelevé écrit : ${sortie}`);
}
console.log("\n--- résumé ---");
for (const k of Object.keys(VERDICTS)) {
  if (parVerdict[k]) console.log(`${k.padEnd(8)} ${parVerdict[k]}`);
}
const utiles = resultats.filter((r) => r.verdict === "UTILE").map((r) => r.source);
if (utiles.length) console.log(`\ncibles réelles du transport : ${utiles.join(", ")}`);