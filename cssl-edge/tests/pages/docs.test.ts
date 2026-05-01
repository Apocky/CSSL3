// cssl-edge · tests/pages/docs.test.ts
// Page-level smoke: /docs index + /docs/[slug] resolve via getStaticProps.

import { SPECS, findSpec } from '@/lib/specs-snapshot';

function assert(cond: boolean, msg: string): void {
  if (!cond) throw new Error(`assert failed : ${msg}`);
}

export function testDocsIndexHasEntries(): void {
  assert(SPECS.length > 0, 'snapshot must contain at least one spec');
  for (const s of SPECS) {
    assert(typeof s.slug === 'string' && s.slug.length > 0, `slug shape : ${JSON.stringify(s)}`);
    assert(typeof s.title === 'string' && s.title.length > 0, `title shape : ${s.slug}`);
    assert(typeof s.body === 'string' && s.body.length > 0, `body shape : ${s.slug}`);
  }
}

export function testDocsSlugLookup(): void {
  const known = SPECS[0]?.slug ?? '';
  const found = findSpec(known);
  assert(found !== null, `findSpec(${known}) must resolve`);
  assert(findSpec('non-existent-slug-xyz') === null, 'unknown slug → null');
}

export function testDocsCslGlyphPresent(): void {
  // grand-vision specs are CSL3-glyph-native — at least ONE § should appear.
  const concat = SPECS.map((s) => s.body).join('\n');
  assert(concat.includes('§'), 'CSL3 § glyph must appear in spec snapshot');
}

declare const require: { main?: unknown } | undefined;
declare const module: { id?: string } | undefined;
const isMain =
  typeof require !== 'undefined' &&
  typeof module !== 'undefined' &&
  require.main === module;
if (isMain) {
  try {
    testDocsIndexHasEntries();
    testDocsSlugLookup();
    testDocsCslGlyphPresent();
    // eslint-disable-next-line no-console
    console.log('docs.test : OK · 3 tests passed');
  } catch (err) {
    // eslint-disable-next-line no-console
    console.error(err);
    process.exit(1);
  }
}
