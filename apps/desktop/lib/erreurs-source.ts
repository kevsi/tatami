/**
 * Traduit une erreur brute du moteur en un message court, sans l'URL technique.
 *
 * ## Pourquoi ce fichier existe
 *
 * La fonction vivait dans `app/page.tsx`, non exportée : aucune porte ne pouvait
 * la vérifier, et une erreur de classement y était invisible. C'est la même
 * famille que « une porte qui ne lit qu'un seul fichier » — ici, une fonction
 * qu'aucune porte ne lit. Extraire ici, c'est la rendre testable, rien d'autre.
 *
 * Les codes viennent de `contract/protocol.json` ; le reste est heuristique sur
 * les messages d'exception observés (sources mortes : 404, hôte inconnu, 403).
 */

/**
 * Les clés qu'`describeError` est susceptible de demander.
 *
 * **Pourquoi une union et non `string`.** La fonction de traduction du
 * composant accepte une union de ~350 clés. La contravariance des paramètres
 * de fonction veut qu'il reçoive un **surensemble** de ce qu'on lui passe :
 * `(key: string) => string` est donc **incompatible** avec elle (`tsc` : TS2345,
 * mesuré le 2026-10-11). Nommer exactement les clés utilisées ici donne un
 * sous-ensemble — donc un sur-ensemble pour `t` : la compatibilité passe, et la
 * garantie reste entière, une clé inexistante ne compile toujours pas.
 */
export type CleErreur =
  | "erreur.delai"
  | "erreur.verification"
  | "erreur.verification.non.rejouable"
  | "erreur.sourcebloquee"
  | "erreur.compte.requis"
  | "erreur.surcharge"
  | "erreur.source.morte"
  | "erreur.origine.injoignable"
  | "erreur.parseur.perime"
  | "erreur.domaine"
  | "erreur.404"
  | "erreur.403"
  | "erreur.serveur";

/**
 * Signature du traducteur, pour les fonctions de module qui doivent parler.
 *
 * La clé est typée : c'est ce qui fait échouer la compilation si on passe une
 * clé inexistante. Une `string` large accepterait n'importe quelle faute de
 * frappe en silence.
 */
export type Traduire = (
  key: CleErreur,
  vars?: Record<string, string | number>,
) => string;

/**
 * `true` quand l'erreur dit qu'un challenge a été résolu et que le **rejeu**
 * est refusé quand même — donc qu'aucune nouvelle tentative n'est utile.
 *
 * Exporté séparément pour que le site puisse décider **avant** de traduire :
 * ouvrir une fenêtre pour un rejeu refusé ne peut qu'échouer (cycle mesuré le
 * 2026-10-11 sur HENTAIZONE).
 *
 * **L'ordre des deux tests est une garantie.** Les deux codes partagent leur
 * début ; si `CHALLENGE_REQUIRED` était testé d'abord, un rejeu refusé
 * produirait « demande une vérification » et la fenêtre se rouvrirait.
 */
export function estRejeuRefuse(raw: string): boolean {
  return /CHALLENGE_UNREPLAYABLE/i.test(raw.replace(/\s+/g, " ").trim());
}

/**
 * Classe une erreur brute du moteur.
 *
 * `t` est passé en paramètre parce que cette fonction vit hors du composant :
 * elle n'a pas accès au contexte i18n. C'est aussi ce qui lui permet de
 * supprimer 20 chaines en dur au profit de l'i18n.
 */
export function describeError(
  raw: string,
  sourceName: string,
  t: Traduire,
): string {
  const s = raw.replace(/\s+/g, " ").trim();
  // Le rejeu refusé passe **avant** `TIMEOUT`, et pas seulement avant
  // `CHALLENGE_REQUIRED`. Depuis le 2026-10-15 le moteur rend la chaîne de
  // causes dans le message, donc un `SocketTimeoutException: timeout` y figure
  // pour un refus que rien ne peut résoudre : classé « délai », l'écran
  // promettait un réessai et l'utilisateur l'attendait indéfiniment.
  // Côté Rust, `is_transient` classe déjà `CHALLENGE_UNREPLAYABLE` comme jamais
  // transitoire — les deux côtés doivent dire la même chose.
  if (estRejeuRefuse(s)) {
    // Le message ne promet aucun réessai : il n'y en a pas d'utile. Le message
    // « pas encore gérée » de `erreur.verification` conviendrait ici et serait
    // un mensonge — il promet une action que rien ne peut réussir.
    return t("erreur.verification.non.rejouable", { source: sourceName });
  }
  if (/TIMEOUT|timeout/i.test(s)) {
    return t("erreur.delai", { source: sourceName });
  }
  if (/SOURCE_BLOCKED/i.test(s)) {
    // Un blocage n'est pas un captcha : il n'y a **rien à résoudre**. Ce test
    // doit précéder `CHALLENGE_REQUIRED` car Cloudflare joint parfois les deux
    // dans un même message, et promettre une vérification ici serait un
    // mensonge — c'est une règle qui refuse, pas un défi à passer.
    return t("erreur.sourcebloquee", { source: sourceName });
  }
  if (/CHALLENGE_REQUIRED/i.test(s)) {
    return t("erreur.verification", { source: sourceName });
  }
  // 401 : le site exige un compte. Ni un challenge ni une panne réseau — demander
  // une vérification anti-robot ici serait futile, c'est un compte qu'il faut.
  if (/AUTH_REQUIRED/i.test(s)) {
    return t("erreur.compte.requis", { source: sourceName });
  }
  if (/RATE_LIMITED/i.test(s)) {
    return t("erreur.surcharge", { source: sourceName });
  }
  // Codes du moteur, **avant** les heuristiques de message. Mesuré le
  // 2026-10-12 : 83 des 98 sources FR échouaient en `INTERNAL` avec une
  // exception Java brute. Le moteur classe désormais par type et par code
  // HTTP ; ce tri doit passer en premier, sinon `Status=523` serait encore
  // rangé dans les 5xx génériques et l'utilisateur n'y gagnerait rien.
  //
  // `SOURCE_DEAD` et `ORIGIN_DOWN` sont volontairement distincts : le premier
  // ne reviendra pas (le domaine a disparu), le second peut revenir (le serveur
  // a été rallumé). Dire « hors service » pour une panne d'origine serait un
  // mensonge qui décourage l'utilisateur de réessayer.
  if (/SOURCE_DEAD/i.test(s)) {
    return t("erreur.source.morte", { source: sourceName });
  }
  if (/ORIGIN_DOWN/i.test(s)) {
    return t("erreur.origine.injoignable", { source: sourceName });
  }
  if (/PARSER_OUTDATED/i.test(s)) {
    return t("erreur.parseur.perime", { source: sourceName });
  }
  if (/UnknownHostException/i.test(s)) {
    return t("erreur.domaine", { source: sourceName });
  }
  if (/404|NotFoundException/i.test(s)) {
    return t("erreur.404", { source: sourceName });
  }
  if (/403/.test(s)) {
    return t("erreur.403", { source: sourceName });
  }
  if (/500|502|503/.test(s)) {
    return t("erreur.serveur", { source: sourceName });
  }
  const short = s.length > 140 ? `${s.slice(0, 140)}…` : s;
  return `${sourceName} : ${short}`;
}
