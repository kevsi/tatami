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
};

export default nextConfig;
