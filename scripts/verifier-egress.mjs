// Verdict en une commande : ce chemin reseau est-il contexte par Cloudflare ?
//
// Pourquoi cette sonde existe. Mesure du 2026-10-16 : notre sortie est
// `154.72.113.241`, AS36924 GVA Cote d'Ivoire SAS — un reseau d'hebergement, pas
// une connexion residentielle. Sur ce profil, Cloudflare rejoue ses defis sans
// les resoudre : 18 sources en 523 et 14 en 403 + marqueur sur les 35 concernees,
// et le widget Turnstile tourne en boucle sur les quatre clients mesures.
//
// Donc la question n'est plus « quel code ecrire » mais « par quel reseau sort-on
// ». Cette sonde repond en une passe, sur n'importe quelle connexion.
//
// **Ce qu'elle ne peut pas mesurer, et qu'elle doit donc dire.** Elle utilise le
// **resolveur du systeme**, alors que le moteur resout en DNS-over-HTTPS
// (`DoHManager`, `cloudflare-dns.com`). Consequence mesuree le meme jour :
// `mangasoriginesunofficial.com` ne resout pas ici (« le nom DNS n'existe
// pas ») et la sonde l'annonce `INJOIGNABLE`, alors que la sonde du moteur
// (`sonde-sources.mjs`) le lit parfaitement (`OK`). Un site `INJOIGNABLE` ici
// peut donc être un site que l'application lit tres bien. Pour la reponse du
// serveur — la seule chose qui tranche ici — `sonde-sources.mjs` reste
// l'instrument de reference.
//
// Les URL par defaut sont reprises **telles quelles** du releve
// `docs/mesures/cloudflare-depuis-cette-machine-2026-10-16.json` : inventer un
// nom de domaine a partir du nom d'une source est une faute deja commise dans
// cette session (`www.mangasorigines.fr` n'existe pas, la source s'appelle
// MANGASORIGINES et vit ailleurs).
//
// Sortie : une ligne par site, puis un resume et un code de sortie.
//   0 = au moins un site derriere Cloudflare repond normalement
//   2 = aucun ne repond : ce chemin reseau est contexte
//   3 = aucun n'a meme pu etre interroge : verdict impossible
//
// Usage : node scripts/verifier-egress.mjs [url ...]

const UA =
  'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 ' +
  '(KHTML, like Gecko) Chrome/140.0.0.0 Safari/537.36 Edg/140.0.0.0';

const SITES_PAR_DEFAUT = [
  'https://hentaizone.xyz/manga/',
  'https://toonfr.com/webtoon/',
  'https://astral-manga.fr/manga/',
  'https://www.manga-mana.com/liste-mangas',
  'https://onisaga.com/browse',
];

const lireTexte = async (url) => {
  const r = await fetch(url, {
    headers: { 'user-agent': UA, accept: 'text/html,application/xhtml+xml' },
    redirect: 'follow',
    signal: AbortSignal.timeout(25000),
  });
  return { http: r.status, corps: await r.text() };
};

// Le gabarit de l'interstitielle fait ~5 a 6 ko et contient ces marqueurs ;
// un catalogue sert des liens de manga. La taille seule ne suffit pas : deux
// pages Cloudflare differentes peuvent avoir la meme taille (lecon du
// 2026-10-16, 28 722 o contre 28 783 o). On exige donc le marqueur **et** la
// lecture de la reponse.
const classer = (http, corps) => {
  const marqueur = /Just a moment|Un instant|challenges\.cloudflare\.com|cf-mitigated|Enable JavaScript and cookies/i.test(
    corps,
  );
  if (http === 200 && !marqueur) {
    const liens = (corps.match(/href="[^"]*\/manga\/[^"]*"/gi) || []).length;
    return liens > 0 ? 'ACCESSIBLE' : 'SANS_CONTENU';
  }
  if (http === 200 || http === 403 || http === 503) return 'DEFI_CLOUDFLARE';
  if (http === 404 || http === 410) return 'MORTE';
  if ([521, 522, 523, 525].includes(http)) return 'ORIGINE_ETEINTE';
  return `AUTRE_${http}`;
};

(async () => {
  const sites = process.argv.slice(2);
  const cibles = sites.length ? sites : SITES_PAR_DEFAUT;

  // 1. Identite de sortie. `ipinfo.io/json` sans jeton repond, mais avec un
  //    budget : c'est une requete de diagnostic, pas une dependance du projet.
  let sortie = null;
  try {
    const r = await fetch('https://ipinfo.io/json', { signal: AbortSignal.timeout(12000) });
    const j = await r.json();
    sortie = { ip: j.ip, org: j.org, pays: j.country, ville: j.city };
  } catch (e) {
    console.log(`identite de sortie : INCONNUE (${String(e.message).slice(0, 60)})`);
  }
  if (sortie) {
    console.log(`sortie  : ${sortie.ip}  ${sortie.org || '?'}`);
    console.log(`situee  : ${sortie.ville || '?'}, ${sortie.pays || '?'}\n`);
  }

  // 2. Ce que les serveurs repondent reellement.
  const resume = {};
  for (const url of cibles) {
    try {
      const { http, corps } = await lireTexte(url);
      const verdict = classer(http, corps);
      resume[verdict] = (resume[verdict] ?? 0) + 1;
      console.log(`${verdict.padEnd(16)} http=${String(http).padEnd(4)} ${corps.length.toString().padStart(7)} o  ${url}`);
    } catch (e) {
      resume.INJOIGNABLE = (resume.INJOIGNABLE ?? 0) + 1;
      console.log(`${'INJOIGNABLE'.padEnd(16)} ${String(e.message).slice(0, 50).padEnd(52)} ${url}`);
    }
  }

  console.log('\n--- resume ---');
  for (const [v, n] of Object.entries(resume).sort((a, b) => b[1] - a[1])) {
    console.log(`${String(n).padStart(3)}  ${v}`);
  }
  console.log(
    '\nRappel : cette sonde utilise le resolveur du systeme, le moteur utilise ' +
      'le DNS-over-HTTPS.\nUn site INJOIGNABLE ici peut etre un site que ' +
      'l\'application lit tres bien\n(`node scripts/sonde-sources.mjs --sources ' +
      '<SOURCE>` tranche sur notre propre pile).',
  );

  const passe = (resume.ACCESSIBLE ?? 0) > 0;
  const interroge = (resume.ACCESSIBLE ?? 0) + (resume.DEFI_CLOUDFLARE ?? 0);
  if (passe) {
    console.log('\nVERDICT : ce chemin reseau lit au moins un site derriere Cloudflare.');
    process.exit(0);
  }
  if (interroge === 0) {
    // Aucun site n'a repondu : dire « contexte » serait accuser le reseau alors
    // que la sonde n'a rien reussi a demander. Un echec de mesure ne se lit pas
    // comme une mesure d'echec.
    console.log('\nVERDICT : INDETERMINE — aucun site n\'a pu etre interroge.');
    process.exit(3);
  }
  console.log('\nVERDICT : ce chemin reseau est contexte. Changer de reseau, pas de code.');
  process.exit(2);
})();