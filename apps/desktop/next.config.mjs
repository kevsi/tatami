const isProd = process.env.NODE_ENV === 'production';
const internalHost = process.env.TAURI_DEV_HOST || 'localhost';

/** @type {import('next').NextConfig} */
const nextConfig = {
  // Export statique : Tauri ne fait pas de SSR (cf. docs Tauri v2 Next.js).
  output: 'export',
  images: {
    unoptimized: true,
  },
  // En dev, les assets passent par le serveur Next local.
  assetPrefix: isProd ? undefined : `http://${internalHost}:3000`,
  // **Pas d'indicateur de développement.** Next pose un bouton flottant dans le
  // coin de l'écran, en bas à gauche — donc **sur la page de lecture**, par-dessus
  // le contenu. Signalé par l'utilisateur comme « un footer inutile ».
  //
  // Il n'existe pas dans l'application livrée (il n'apparaît qu'en `next dev`) :
  // c'est du bruit qui masque le lecteur pendant le développement, et il masque
  // aussi ce qu'on mesure — une capture du lecteur prise en dev le montre, donc
  // elle ne décrit pas l'application.
  devIndicators: false,
};

export default nextConfig;
