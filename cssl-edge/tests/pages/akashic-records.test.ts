import assert from 'node:assert/strict';
import fs from 'node:fs';
import path from 'node:path';

import {
  findAkashicRecord,
  getAkashicRecordSummaries,
} from '@/lib/akashic-records';

const root = process.cwd();
const read = (relative: string): string => fs.readFileSync(path.join(root, relative), 'utf8');

const indexSource = read('pages/akashic-records/index.tsx');
const detailSource = read('pages/akashic-records/[slug].tsx');
const shellSource = read('components/SiteShell.tsx');
const homeSource = read('pages/index.tsx');
const docsDetailSource = read('pages/docs/[slug].tsx');
const sitemap = read('public/sitemap.xml');

async function run(): Promise<void> {
  const summaries = getAkashicRecordSummaries();
  assert.equal(summaries.length, 204, 'the approved v1 denominator is exactly 204 non-draft Medium works');
  assert.ok(summaries.every((record) => !('body' in record)), 'index summaries must never carry full bodies');

  const sitemapSlugs = Array.from(
    sitemap.matchAll(/<loc>https:\/\/www\.apocky\.com\/akashic-records\/([^<]+)<\/loc>/g),
    (match) => match[1] as string,
  );
  assert.equal(sitemapSlugs.length, summaries.length, 'sitemap must expose one detail URL per approved work');
  assert.equal(new Set(sitemapSlugs).size, summaries.length, 'sitemap detail URLs must be unique');
  for (const summary of summaries) {
    assert.ok(sitemapSlugs.includes(summary.slug), `sitemap missing approved slug ${summary.slug}`);
  }

  const first = summaries[0];
  assert.ok(first !== undefined, 'fixture archive must have a first record');
  const record = findAkashicRecord(first.slug);
  assert.ok(record !== undefined, 'a published summary must resolve to a full record');
  assert.ok(record.blocks.length > 0, 'a published record must expose safe semantic blocks');
  assert.equal(findAkashicRecord('not-a-public-record'), undefined, 'unknown slugs must fail closed');

  for (const source of [indexSource, detailSource]) {
    assert.doesNotMatch(source, /dangerouslySetInnerHTML/, 'archive pages must render escaped React text');
    assert.doesNotMatch(source, /fetch\s*\(/, 'archive pages must not fetch the live vault or Medium');
    assert.doesNotMatch(source, /\/api\/akashic/, 'works archive must remain separate from Akashic telemetry APIs');
    assert.doesNotMatch(source, /Obsidian Vault|[A-Z]:\\Users\\/, 'archive pages must not expose local paths');
  }

  assert.match(indexSource, /type="search"/, 'archive must provide a named search control');
  assert.match(indexSource, /props: \{ records: \[\.\.\.getAkashicRecordSummaries\(\)\] \}/, 'index static props must expose summaries only');
  assert.match(indexSource, /sources\.length > 1/, 'source facets must appear only when the corpus has multiple sources');
  assert.match(indexSource, /topics\.length > 0/, 'topic facets must appear only when the corpus has topic data');
  assert.match(indexSource, /Year/, 'archive must provide a year filter');
  assert.match(indexSource, /types\.length > 1/, 'type facets must appear only when the corpus has multiple work types');
  assert.match(indexSource, /Titles and descriptions…/, 'search copy must describe the fields v1 actually searches');
  const searchableProjection = indexSource.match(/const searchable = \[([\s\S]*?)\]\.join/);
  assert.ok(searchableProjection !== null, 'searchable projection must remain inspectable');
  assert.doesNotMatch(
    searchableProjection[1] ?? '',
    /record\.(?:source|type|topics|year)/,
    'v1 search must stay scoped to title and description',
  );
  assert.match(indexSource, /aria-live="polite"/, 'result count must be announced accessibly');
  assert.match(indexSource, /href="\/akashic-records\/manifest\.json"/, 'archive must expose its hash-sealed public catalog');
  assert.match(detailSource, /Source fingerprint/, 'detail pages must expose provenance');
  assert.match(detailSource, /Archive\/import date/, 'detail pages must expose the archive date when available');
  assert.match(detailSource, /record\.blocks\.map\(renderRecordBlock\)/, 'detail pages must render semantic blocks');
  assert.match(detailSource, /<pre key=\{key\} tabIndex=\{0\}>/, 'scrollable preformatted blocks must remain keyboard reachable');
  assert.match(detailSource, /getStaticPaths/, 'detail pages must materialize static paths');
  assert.match(detailSource, /findAkashicRecord\(slug\)/, 'detail static props must resolve one approved record');
  assert.match(detailSource, /Omit<AkashicRecord, 'body'>/, 'detail props must exclude the unused readback body');
  assert.match(detailSource, /body: _readbackBody, \.\.\.pageRecord/, 'detail static props must strip the readback body before serialization');
  assert.match(detailSource, /\['http:', 'https:'\]/, 'detail page must enforce safe external-link protocols');
  assert.doesNotMatch(detailSource, /record\.body\.split/, 'detail pages must not flatten the readback body into paragraphs');
  for (const kind of ['paragraph', 'heading', 'blockquote', 'list', 'pre', 'figure', 'embed', 'linkCard', 'divider']) {
    assert.match(detailSource, new RegExp(`case '${kind}'`), `semantic renderer must handle ${kind} blocks`);
  }
  assert.match(detailSource, /renderLinkedText\(block\.text, block\.links\)/, 'authored heading and quotation links must remain interactive');
  assert.match(detailSource, /const cardHref = safeExternalHref\(block\.href\)/, 'link cards must pass through the safe external-link policy');
  assert.match(detailSource, /availability unverified|may be unavailable/, 'original links must disclose uncertainty');
  assert.match(shellSource, /href: '\/akashic-records'/, 'global navigation must expose the archive');
  assert.match(homeSource, /title: 'Akashic Records'/, 'homepage must expose the archive');
  assert.match(docsDetailSource, /spec\.slug === '18_AKASHIC_RECORDS'/, 'legacy qualifier must target only the existing technical document');
  assert.match(
    docsDetailSource,
    /Akashic Records — legacy Labyrinth technical specification/,
    'legacy document title must remain visibly disambiguated from the works archive',
  );
  assert.match(docsDetailSource, /href="\/akashic-records"/, 'legacy document must point readers to the works archive');
  assert.match(
    docsDetailSource,
    /https:\/\/www\.apocky\.com\/docs\/\$\{spec\.slug\}/,
    'legacy document canonical must use the public www host',
  );

  console.log(`akashic-records.test : OK · ${summaries.length} approved static records`);
}

void run().catch((error: unknown) => {
  console.error(error);
  process.exit(1);
});
