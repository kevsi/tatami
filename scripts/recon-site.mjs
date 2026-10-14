// Reconnaissance d'un site pour une source personnalisee.
//   node recon-site.mjs <url-liste> <url-serie> <url-chapitre>
// Sort : la forme des liens, les ancetres classes, les images.
// On ne devine pas un selecteur : on le mesure.

const [urlListe, urlSerie, urlChapitre] = process.argv.slice(2);
const base = "http://127.0.0.1:9223";
const urlCdp = (await (await fetch(`${base}/json/version`).then((r) => r.json()))).webSocketDebuggerUrl;
const ws = await new Promise((res, rej) => {
  const s = new WebSocket(urlCdp);
  s.onopen = () => res(s);
  s.onerror = (e) => rej(new Error("ws " + (e.message ?? e.type)));
});
let id = 0;
const pending = new Map();
ws.onmessage = (e) => {
  let m;
  try { m = JSON.parse(e.data); } catch { return; }
  if (m.id && pending.has(m.id)) {
    const { res, rej } = pending.get(m.id);
    pending.delete(m.id);
    if (m.error) rej(new Error(JSON.stringify(m.error))); else res(m.result);
  }
};
const send = (method, params = {}, sid) =>
  new Promise((res, rej) => {
    const n = ++id;
    pending.set(n, { res, rej });
    const p = { id: n, method, params };
    if (sid) p.sessionId = sid;
    ws.send(JSON.stringify(p));
  });
const ev = async (sid, e) => {
  const r = await send("Runtime.evaluate", { expression: e, returnByValue: true, awaitPromise: true }, sid);
  if (r.exceptionDetails) return { erreur: r.exceptionDetails.text };
  return r.result.value;
};
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function look(url, expr, label) {
  if (!url) return console.log(`\n-- ${label} : pas d'URL fournie`);
  const { targetId } = await send("Target.createTarget", { url: "about:blank" });
  const { sessionId } = await send("Target.attachToTarget", { targetId, flatten: true });
  await send("Page.enable", {}, sessionId);
  await send("Runtime.enable", {}, sessionId);
  await send("Page.navigate", { url }, sessionId);
  for (let i = 0; i < 34; i++) {
    await sleep(500);
    const t = String((await ev(sessionId, "document.title")) ?? "");
    if (t.trim() !== "" && !/just a moment|un instant|attention/i.test(t)) break;
  }
  await sleep(3000);
  console.log(`\n========== ${label}\n  ${url}`);
  console.log("  titre :", await ev(sessionId, "document.title"));
  console.log("  " + label + " :", await ev(sessionId, expr));
  await send("Target.closeTarget", { targetId });
}

const LIENS = `JSON.stringify((() => {
  const vus = {}, out = [];
  for (const a of document.querySelectorAll('a[href]')) {
    const h = a.getAttribute('href');
    if (!h || h.length < 6 || vus[h]) continue;
    vus[h] = true;
    out.push({ href: h.slice(0, 84), cls: (a.className || '').slice(0, 34),
               parent: (a.parentElement && a.parentElement.className || '').slice(0, 34) });
    if (out.length >= 14) break;
  }
  return out;
})())`;

const IMAGES = `JSON.stringify([...document.querySelectorAll('img')]
  .map(i => ({ src: (i.currentSrc || i.src || i.dataset.src || '').slice(0, 96),
               chain: (() => { const c = []; let n = i; for (let k = 0; k < 4 && n; k++) { n = n.parentElement; if (n) c.push(n.tagName.toLowerCase() + (n.className ? '.' + String(n.className).trim().split(/\\s+/).join('.') : '')); } return c.join(' < '); })() }))
  .slice(0, 8))`;

await look(urlListe, LIENS, "LIENS-LISTE");
await look(urlSerie, LIENS, "LIENS-SERIE");
await look(urlChapitre, IMAGES, "IMAGES-CHAPITRE");
ws.close();