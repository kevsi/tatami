/**
 * Client CDP minimal pour les E2E de Tatami.
 *
 * Pas de dépendance : le binaire debug lance la webview avec
 * `--remote-debugging-port`, et Node 22+ fournit `fetch` et `WebSocket`.
 * `playwright-core` n'apporterait ici que le lancement du navigateur — or le
 * navigateur, c'est déjà l'application.
 *
 * Prérequis : `pnpm tauri dev` lancé avec
 * `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=9225` (et un
 * `WEBVIEW2_USER_DATA_FOLDER` temporaire pour ne pas perturber les instances
 * installées, cf. AGENTS.md §14). `E2E_PORT` permet de viser un autre port.
 */

/** Ouvre une session sur la page servie par `localhost:3001`. */
export async function connecter(port = Number(process.env.E2E_PORT ?? 9225)) {
  const reponse = await fetch(`http://localhost:${port}/json/list`);
  const cibles = await reponse.json();
  const page = cibles.find((t) => t.type === 'page' && t.url.includes('3001'));
  if (!page) {
    throw new Error(
      `aucune page Tatami sur le port ${port}. Lancez « pnpm tauri dev » avec ` +
        `WEBVIEW2_ADDITIONAL_BROWSER_ARGUMENTS=--remote-debugging-port=${port}.`,
    );
  }

  const ws = new WebSocket(page.webSocketDebuggerUrl);
  await new Promise((resoudre, rejeter) => {
    ws.onopen = resoudre;
    ws.onerror = () => rejeter(new Error('connexion CDP refusée'));
  });

  let suivant = 0;
  const attentes = new Map();
  ws.onmessage = (evenement) => {
    const message = JSON.parse(evenement.data);
    const attente = attentes.get(message.id);
    if (!attente) return;
    attentes.delete(message.id);
    if (message.error) attente.rejeter(new Error(JSON.stringify(message.error)));
    else attente.resoudre(message.result);
  };

  const envoyer = (method, params = {}) =>
    new Promise((resoudre, rejeter) => {
      const id = ++suivant;
      attentes.set(id, { resoudre, rejeter });
      ws.send(JSON.stringify({ id, method, params }));
    });

  const evaluer = async (expression) => {
    const r = await envoyer('Runtime.evaluate', {
      expression,
      returnByValue: true,
      awaitPromise: true,
    });
    if (r.exceptionDetails) {
      const ex = r.exceptionDetails.exception;
      const detail =
        ex?.description ??
        ex?.value ??
        r.exceptionDetails.text ??
        JSON.stringify(r.exceptionDetails);
      throw new Error(String(detail));
    }
    return r.result?.value;
  };

  return {
    ws,
    envoyer,
    evaluer,
    fermer: () => ws.close(),
  };
}

/** Attente simple, réservée aux transitions d'interface. */
export const pause = (ms) => new Promise((r) => setTimeout(r, ms));

/** Attend qu'une expression devienne vraie, sinon échoue en nommant l'attente. */
export async function jusqua(evaluer, expression, { delai = 20000, pas = 250, quoi } = {}) {
  const fin = Date.now() + delai;
  let derniere;
  while (Date.now() < fin) {
    derniere = await evaluer(expression);
    if (derniere) return derniere;
    await pause(pas);
  }
  throw new Error(`délai dépassé : ${quoi ?? expression}`);
}

/** Remet l'application sur l'accueil, sans état résiduel du scénario précédent. */
export async function reinitialiser({ envoyer, evaluer }) {
  await envoyer('Page.reload');
  await pause(1500);
  await jusqua(evaluer, `document.querySelectorAll('[role=tab]').length === 4`, {
    delai: 30000,
    quoi: 'la barre d’onglets après rechargement',
  });
}

export async function allerAOnglet(evaluer, nom) {
  const clique = await evaluer(`(function(){
    const t = [...document.querySelectorAll('[role=tab]')].find((x) => x.innerText.includes(${JSON.stringify(nom)}));
    if (!t) return false;
    t.click();
    return true;
  })()`);
  if (!clique) throw new Error(`onglet introuvable : ${nom}`);
  await pause(1200);
}

/**
 * Ouvre l'onglet Catalogues et attend que `listSources` soit arrivé.
 *
 * Le piège mesuré : `.lang-tile` existe **dès** le premier rendu, mais vide —
 * et la tuile d'import contient le mot « source », donc une attente trop large
 * (`.lang-tile` présent) passe avant que les langues soient là. On attend le
 * **compte** de tuiles, qui n'atteint sa valeur qu'une fois les langues
 * rendues.
 */
export async function ouvrirCatalogue(session) {
  await allerAOnglet(session.evaluer, 'Catalogues');
  await jusqua(session.evaluer, `document.querySelectorAll('.lang-tile').length > 5`, {
    delai: 60000,
    quoi: 'les tuiles de langue (listSources)',
  });
}

/** Choisit une langue (par défaut FR) et attend la liste des sources. */
export async function choisirLangue(session, code = 'FR') {
  const ok = await session.evaluer(`(function(){
    const t = [...document.querySelectorAll('.lang-tile')].find((x) => new RegExp('^' + ${JSON.stringify(code)} + '\\\\b').test(x.innerText.trim()));
    if (!t) return false;
    t.click();
    return true;
  })()`);
  if (!ok) throw new Error(`langue introuvable : ${code}`);
  await jusqua(session.evaluer, `document.querySelectorAll('.slot button.card').length > 0`, {
    delai: 60000,
    quoi: `les sources ${code}`,
  });
}

/** Clique une source par son nom (première ligne de la tuile) et attend la grille. */
export async function ouvrirSource(session, nom) {
  const ok = await session.evaluer(`(function(){
    const b = [...document.querySelectorAll('.slot button.card')].find(
      (x) => x.innerText.trim().split('\\n')[0] === ${JSON.stringify(nom)} && !x.classList.contains('dead'),
    );
    if (!b) return false;
    b.click();
    return true;
  })()`);
  if (!ok) return false;
  await jusqua(session.evaluer, `document.querySelectorAll('.manga').length > 0`, {
    delai: 60000,
    quoi: `la grille de ${nom}`,
  });
  return true;
}

/** Ouvre la fiche du n-ième manga et attend ses **vrais** chapitres. */
export async function ouvrirFiche(session, index = 0) {
  const ok = await session.evaluer(`(function(){
    const m = [...document.querySelectorAll('.manga .manga-open')][${index}];
    if (!m) return false;
    m.click();
    return true;
  })()`);
  if (!ok) throw new Error(`pas de manga à l'index ${index}`);
  // `.chapter-cell` matche aussi le **squelette** : il faut exclure
  // `.squelette-ligne`, sinon l'attente se satisfait d'une liste vide.
  await jusqua(session.evaluer, `document.querySelectorAll('.chapter-cell:not(.squelette-ligne)').length > 0`, {
    delai: 60000,
    quoi: 'les vrais chapitres de la fiche',
  });
  return session.evaluer(`(document.querySelector('.page-head')||{}).innerText || ''`);
}

/**
 * Ouvre la première fiche dont les chapitres arrivent, en essayant des sources
 * dans l'ordre. Une source peut répondre sa grille puis échouer ses chapitres
 * (504 côté site, mesuré) : les deux étapes comptent.
 *
 * Rend `{ source, titre }` avec la fiche ouverte, ou lève si aucune source ne
 * répond — auquel cas c'est le réseau/la source qu'il faut regarder, pas le code.
 */
export async function ouvrirUneFicheAvecChapitres(session, sources) {
  const echecs = [];
  for (const nom of sources) {
    try {
      await reinitialiser(session);
      await ouvrirCatalogue(session);
      await choisirLangue(session, 'FR');
      if (!(await ouvrirSource(session, nom))) {
        echecs.push(`${nom} : tuile absente ou hors service`);
        continue;
      }
      const entete = await ouvrirFiche(session, 0);
      const titre = entete.split('\n')[0].trim();
      return { source: nom, titre };
    } catch (e) {
      echecs.push(`${nom} : ${String(e.message).slice(0, 60)}`);
    }
  }
  throw new Error(`aucune source utilisable :\n  - ${echecs.join('\n  - ')}`);
}

/** Lit la position de lecture dans la barre du lecteur. */
export const lireProgressionLecteur = (evaluer) =>
  evaluer(`(function(){
    const p = document.querySelector('.reader-progress');
    return p ? { now: Number(p.getAttribute('aria-valuenow')), max: Number(p.getAttribute('aria-valuemax')) } : null;
  })()`);

/** Nombre d'images de page effectivement décodées (une `<img>` cassée est « visible »). */
export const pagesDecodees = (evaluer) =>
  evaluer(`[...document.querySelectorAll('img[data-index]')].filter((i) => i.complete && i.naturalWidth > 0).length`);

/** Petite assertion partagée : nomme ce qui est faux, sans framework. */
export function verifier(condition, message) {
  if (!condition) throw new Error(`assertion : ${message}`);
}
