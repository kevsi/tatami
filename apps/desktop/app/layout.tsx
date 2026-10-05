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
    <html lang="fr" suppressHydrationWarning>
      <head>
        <script dangerouslySetInnerHTML={{ __html: AVANT_HYDRATATION }} />
      </head>
      <body>
        <div className="app">{children}</div>
      </body>
    </html>
  );
}
