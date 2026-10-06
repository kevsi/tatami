import { readFileSync } from 'node:fs';
import Ajv from 'ajv';
import type { PageTranslation } from './traduction';

const schema = JSON.parse(readFileSync('../../contract/translation/schema.json', 'utf8')) as Record<string, unknown>;
const ajv = new Ajv({ allErrors: true });
const validate = ajv.compile(schema);

const fixture = {
  pageId: 'page-1',
  chapterUrl: 'chapter-1',
  source: 'WEBTOONS_EN',
  width: 800,
  height: 1200,
  blocks: [
    {
      id: 'b1',
      type: 'dialogue',
      bbox: { x: 10, y: 20, w: 200, h: 40 },
      originalText: 'HELLO WORLD',
      translatedText: 'Bonjour le monde',
      readingOrder: 0,
      status: 'ok',
      confidence: 0.92,
    },
  ],
} as PageTranslation;

const valid = validate(fixture);
if (!valid) {
  console.error('Fixture invalide :', validate.errors);
  process.exit(1);
}
console.log('contrat traduction: fixture TS ok');
