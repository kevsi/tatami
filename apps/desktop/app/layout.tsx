import type { Metadata } from 'next';
import './globals.css';

export const metadata: Metadata = {
  title: 'Tatami',
  description: 'Client desktop de lecture de manga',
};

/**
 * Applique thème et langue **avant** que React n'hydrate.
 *
 * Mesuré le 2026-10-05 : ces deux attributs étaient posés au chargement du
 * module `apparence.ts` (donc avant l'hydratation), alors que le HTML exporté
 * est produit au build et ne peut pas les contenir. Résultat : React.signalait
 * « some attributes of the server rendered HTML didn't match », et
 * `document.documentElement.lang` valait `fr` pendant que l'écran affichait
 * l'anglais — un décalage qui rendait toute la typographie du navigateur
 * incohérente.
 *
 * La correction est la solution documentée de Next.js : un script **bloquant**
 * dans le `<head>`, exécuté avant React, plus `suppressHydrationWarning` sur
 * `<html>` — cet attribut est justement le cas pour lequel Next.js demande
 * l'avertissement ignoré. Le module garde la responsabilité de l'état, il ne
 * pose plus l'attribut au chargement.
 *
 * Attention : cette correction se **vérifie sur l'export**, pas en dev. Le
 * commentaire du script, plus bas, explique pourquoi.
 *
 * Les deux clés vivent dans `localStorage` : `tatami.apparence` pour le thème
 * (`apparence.ts`) et `tatami.lang` pour la langue (`i18n.ts`).
 */
const AVANT_HYDRATATION = `(function(){try{
  var t=localStorage.getItem('tatami.apparence')==='clair'?'clair':'sombre';
  document.documentElement.setAttribute('data-apparence',t);
  var l=localStorage.getItem('tatami.lang');
  document.documentElement.setAttribute('lang',l==='en'?'en':'fr');
}catch(e){document.documentElement.setAttribute('data-apparence','sombre');}})();`;

export default function RootLayout({ children }: { children: React.ReactNode }) {
  return (
    // `suppressHydrationWarning` : le `<html>` porte deux attributs que le HTML
    // statique ne peut pas contenir (thème et langue sont sur la machine, pas
    // dans le build). Le script ci-dessus les pose avant React ; cet attribut
    // empêche React de signaler l'écart qui n'en est pas un.
    // Il faut le garder même si le script s'exécute : React compare quand même
    // `<html lang="fr">` (valeur du build) à `lang="en"` (valeur de la machine),
    // et l'écart est réel.
    <html lang="fr" suppressHydrationWarning>
      {/* Le `<script>` nu est correct pour l'application LIVRÉE — et ne pas
          « corriger » sur la foi du seul serveur de dev.

          Mesuré le 2026-10-05 : en `pnpm tauri dev`, `data-apparence` restait
          `null` et `lang` restait `fr`, alors que `localStorage` portait
          `sombre` et `en` et que l'interface affichait bien l'anglais. La
          lecture du HTML **servi par le dev** donne la cause : le script n'y
          existe que comme chaîne dans le payload RSC
          (`self.__next_f.push([0,{"children":"(function(){…`), donc comme
          balise inexécutable. Un `<Script strategy="beforeInteractive">` a le
          même sort en dev : c'était le candidat « correctif », il ne l'était pas.

          Or l'application livrée charge le HTML **exporté**, où le script est
          une vraie balise dans le `<head>`. Vérifié le même jour sur
          `out/index.html` servi en statique, avec `clair` et `en` posés :
          `data-apparence="clair"` et `lang="en"` dès le parsing, et le thème
          clair effectivement peint (`rgb(244,244,246)` sur `rgb(22,24,29)`).

          Donc : **en dev, mesurer `data-apparence` ne prouve rien sur
          l'application livrée.** La mesure à faire porte sur l'export. */}
      <head>
        <script dangerouslySetInnerHTML={{ __html: AVANT_HYDRATATION }} />
      </head>
      <body>
        <div className="app">{children}</div>
      </body>
    </html>
  );
}
