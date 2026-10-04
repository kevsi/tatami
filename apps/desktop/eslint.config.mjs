import { defineConfig, globalIgnores } from 'eslint/config';
import nextVitals from 'eslint-config-next/core-web-vitals';

/**
 * ESLint en configuration plate (flat config).
 * Depuis Next 16, `next lint` n'existe plus et `next build` ne lance plus le
 * linter : on appelle donc le binaire eslint directement, comme le prescrit la
 * documentation officielle (docs/01-app/03-api-reference/05-config/03-eslint).
 */
const eslintConfig = defineConfig([
  ...nextVitals,
  {
    rules: {
      /*
       * Tatami est une webview Tauri en export statique, avec `images.unoptimized`
       * déjà actif. Les images viennent soit du protocole `tatami://` (pages mises
       * en cache par Rust), soit de CDN distants. `next/image` n'apporterait donc
       * aucune optimisation : il lui faudrait un chargeur pour `tatami://` et un
       * domaine autorisé, et il casse le protocole au passage.
       */
      '@next/next/no-img-element': 'off',
    },
  },
  globalIgnores([
    '.next/**',
    'out/**',
    'build/**',
    'next-env.d.ts',
    // Généré par Next, jamais édité à la main.
    'src-tauri/**',
  ]),
]);

export default eslintConfig;