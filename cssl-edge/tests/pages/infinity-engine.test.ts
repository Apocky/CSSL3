// cssl-edge · tests/pages/infinity-engine.test.ts
// W14-H · smoke + render-shape tests for /infinity-engine page.
// Pattern matches press.test.ts + docs.test.ts.
// I> we cannot invoke the React fn here (no React runtime in node:test env)
// I> we DO assert : default-export shape + name + non-zero LOC + canonical strings

import { readFileSync } from 'node:fs';
import { resolve, dirname } from 'node:path';
import { fileURLToPath } from 'node:url';
import InfinityEngine from '@/pages/infinity-engine';

function assert(cond: boolean, msg: string): void {
  if (!cond) throw new Error(`assert failed : ${msg}`);
}

// 1. Page module exposes a renderable component.
export function testInfinityEngineDefaultExport(): void {
  assert(
    typeof InfinityEngine === 'function',
    `infinity-engine default export must be a component (function), got ${typeof InfinityEngine}`,
  );
}

// 2. Component carries an identifier — protects against accidental rename
//    or default-export-shadowing during refactor.
export function testInfinityEngineComponentName(): void {
  const fn = InfinityEngine as unknown as { name?: string };
  const name = fn.name;
  assert(typeof name === 'string', 'component must carry a .name');
  assert(name !== undefined && name.length > 0, 'component .name must be non-empty');
}

// 3. The page source must contain the canonical brand string + key
//    sovereignty-respecting copy. Static-string scan is a renderer-free
//    way to validate the rebrand actually landed.
export function testInfinityEngineSourceHasCanonicalCopy(): void {
  // Resolve the .tsx source relative to this test file. ESM-style.
  const here =
    typeof __dirname === 'string'
      ? __dirname
      : dirname(fileURLToPath(import.meta.url));
  const src = readFileSync(
    resolve(here, '..', '..', 'pages', 'infinity-engine.tsx'),
    'utf8',
  );
  assert(src.includes('The Infinity Engine'), 'canonical brand string must appear');
  assert(src.includes('sovereign by default'), 'sovereignty-respecting copy must appear');
  assert(src.includes('§'), 'CSL3 § glyph must appear in source');
  assert(
    src.includes('/engine'),
    'must link to W14-M sibling /engine live-status page',
  );
  assert(src.length > 4000, `page must be substantive (>4kB), got ${src.length}`);
}

declare const require: { main?: unknown } | undefined;
declare const module: { id?: string } | undefined;
const isMain =
  typeof require !== 'undefined' &&
  typeof module !== 'undefined' &&
  require.main === module;

if (isMain) {
  try {
    testInfinityEngineDefaultExport();
    testInfinityEngineComponentName();
    testInfinityEngineSourceHasCanonicalCopy();
    // eslint-disable-next-line no-console
    console.log('infinity-engine.test : OK · 3 tests passed');
  } catch (err) {
    // eslint-disable-next-line no-console
    console.error(err);
    process.exit(1);
  }
}
