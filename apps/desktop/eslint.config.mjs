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

      /*
       * Une liste rendue sans `key` ne casse pas l'affichage : React la
       * signale en console, et l'overlay Next la remonte comme « 1 Issue ».
       * Mesuré le 2026-10-10 : c'est ce que signalait cet overlay, et
       * l'avertissement est **intermittent** — il n'est pas apparu sur une
       * seconde passe identique, donc le chercher à la main revenait à
       *guesser. Cette règle le nomme, ligne par ligne, à chaque `pnpm lint`.
       *
       * Elle est activée explicitement alors que `core-web-vitals` ne la pose
       * pas : une règle absente ne mord jamais, et une porte muette est
       * indiscernable d'une porte verte (leçon AGENTS.md §14).
       */
      'react/jsx-key': 'error',
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