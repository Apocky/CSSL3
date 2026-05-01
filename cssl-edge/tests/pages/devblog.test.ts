// cssl-edge · tests/pages/devblog.test.ts
// Smoke: post catalog + markdown-render shape.

import { DEVBLOG_POSTS, findPost } from '@/lib/devblog-posts';
import { markdownToHtml, testMarkdownH1, testMarkdownList, testMarkdownCodeBlock, testMarkdownHtmlEscape } from '@/lib/markdown';

function assert(cond: boolean, msg: string): void {
  if (!cond) throw new Error(`assert failed : ${msg}`);
}

export function testDevblogHasSeedPosts(): void {
  assert(DEVBLOG_POSTS.length >= 3, `expected ≥3 seed posts, got ${DEVBLOG_POSTS.length}`);
  for (const p of DEVBLOG_POSTS) {
    assert(typeof p.slug === 'string' && p.slug.length > 0, 'slug');
    assert(typeof p.title === 'string' && p.title.length > 0, 'title');
    assert(typeof p.date_iso === 'string' && /^\d{4}-\d{2}-\d{2}$/.test(p.date_iso), `date_iso shape : ${p.slug}`);
    assert(p.body.length >= 200, `body must be ≥200 chars : ${p.slug}`);
  }
}

export function testDevblogSlugLookup(): void {
  const known = DEVBLOG_POSTS[0]?.slug ?? '';
  assert(findPost(known) !== null, `findPost(${known}) must resolve`);
  assert(findPost('non-existent-slug-xyz') === null, 'unknown slug → null');
}

export function testDevblogMarkdownRendersForAllPosts(): void {
  for (const p of DEVBLOG_POSTS) {
    const html = markdownToHtml(p.body);
    assert(html.length > p.body.length / 2, `html length sanity for ${p.slug}`);
    assert(!html.includes('<script>'), `no raw script tag : ${p.slug}`);
  }
}

declare const require: { main?: unknown } | undefined;
declare const module: { id?: string } | undefined;
const isMain =
  typeof require !== 'undefined' &&
  typeof module !== 'undefined' &&
  require.main === module;
if (isMain) {
  try {
    testDevblogHasSeedPosts();
    testDevblogSlugLookup();
    testDevblogMarkdownRendersForAllPosts();
    // also exercise markdown.ts inline tests
    testMarkdownH1();
    testMarkdownList();
    testMarkdownCodeBlock();
    testMarkdownHtmlEscape();
    // eslint-disable-next-line no-console
    console.log('devblog.test : OK · 7 tests passed');
  } catch (err) {
    // eslint-disable-next-line no-console
    console.error(err);
    process.exit(1);
  }
}
