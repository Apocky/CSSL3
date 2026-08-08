import assert from 'node:assert/strict';
import crypto from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';

import {
  APPROVED_RECORD_COUNT,
  APPROVED_SOURCE_BYTES,
  APPROVED_SOURCE_IDENTITY_SHA256,
  EXCLUDED_DRAFT_COUNT,
  blocksToText,
  parseMediumPost,
  safeHttpUrl,
} from '../../scripts/snapshot-akashic-records.mjs';

const root = process.cwd();
const snapshotPath = path.join(root, 'data', 'akashic-records.v1.json');
const manifestPath = path.join(root, 'public', 'akashic-records', 'manifest.json');
const snapshotBytes = fs.readFileSync(snapshotPath);
const snapshot = JSON.parse(snapshotBytes.toString('utf8'));
const manifest = JSON.parse(fs.readFileSync(manifestPath, 'utf8'));

assert.equal(snapshot.schemaVersion, 1);
assert.equal(snapshot.approvedCount, APPROVED_RECORD_COUNT);
assert.equal(snapshot.records.length, APPROVED_RECORD_COUNT);
assert.equal(snapshot.draftExcludedCount, EXCLUDED_DRAFT_COUNT);
assert.equal(snapshot.sourceBytes, APPROVED_SOURCE_BYTES);
assert.equal(snapshot.sourceSeal, APPROVED_SOURCE_IDENTITY_SHA256);
assert.equal(
  snapshot.sourceSealAlgorithm,
  'ASCII lowercase ordinal filename order; filename<TAB>byteLength<TAB>lowercase-sha256<LF>; UTF-8 without BOM',
);
assert.equal(snapshot.snapshotDate, '2026-03-06');

const slugs = new Set();
const sourceHashes = new Set();
const kindCounts = Object.create(null);
let linkCount = 0;
let linkCardCount = 0;
let captionCount = 0;
for (const record of snapshot.records) {
  assert.ok(record.slug.length > 0);
  assert.ok(!slugs.has(record.slug), `duplicate slug ${record.slug}`);
  slugs.add(record.slug);
  assert.match(record.sourceSha256, /^[a-f0-9]{64}$/);
  assert.ok(!sourceHashes.has(record.sourceSha256), `duplicate source hash ${record.slug}`);
  sourceHashes.add(record.sourceSha256);
  assert.ok(record.title.length > 0);
  assert.ok(record.excerpt.length > 0);
  assert.ok(record.body.length > 0);
  assert.ok(record.blocks.length > 0);
  assert.equal(record.body, blocksToText(record.blocks), `body readback drift ${record.slug}`);
  assert.equal(record.source, 'Medium');
  assert.equal(record.type, 'Medium post');
  assert.deepEqual(record.topics, []);
  assert.equal(record.sourceUrlStatus, 'unverified');
  assert.equal(new URL(record.sourceUrl).protocol, 'https:');
  assert.equal(record.canonicalUrl, `https://www.apocky.com/akashic-records/${record.slug}`);
  assert.equal(new Date(record.publishedAt).getUTCFullYear(), record.year);
  for (const block of record.blocks) {
    kindCounts[block.kind] = (kindCounts[block.kind] ?? 0) + 1;
    if (block.kind === 'paragraph' || block.kind === 'heading' || block.kind === 'blockquote') {
      for (const link of block.links ?? []) {
        linkCount += 1;
        assert.equal(link.text, block.text.slice(link.start, link.end));
        assert.equal(safeHttpUrl(link.href), link.href);
      }
    }
    if (block.kind === 'linkCard') {
      linkCardCount += 1;
      assert.equal(safeHttpUrl(block.href), block.href);
    }
    if (block.kind === 'figure') {
      assert.equal(block.omitted, true);
      assert.equal('src' in block, false);
      if (block.caption !== undefined) captionCount += 1;
    }
    if (block.kind === 'embed') {
      assert.equal(block.omitted, true);
      assert.equal('src' in block, false);
      if (block.href !== undefined) assert.equal(safeHttpUrl(block.href), block.href);
    }
  }
}

assert.deepEqual(
  { ...kindCounts },
  {
    paragraph: 9589,
    heading: 1248,
    figure: 149,
    linkCard: 26,
    divider: 1282,
    blockquote: 300,
    pre: 28,
    list: 1317,
    embed: 10,
  },
  'the safe AST must retain the complete measured structural denominator',
);
assert.equal(captionCount, 41);
assert.equal(linkCount, 61, 'all safe paragraph, non-metadata heading, and blockquote links must remain navigable');
assert.equal(linkCardCount, 26, 'each authored text link card must retain one safe destination');

const serialized = snapshotBytes.toString('utf8');
for (const forbidden of [
  /[A-Z]:\\Users\\/i,
  /\/Users\//,
  /\/home\//,
  /Obsidian Vault/i,
  /file:\/\//i,
  /<\/?(?:script|iframe|img)\b/i,
  /javascript:/i,
  /-----BEGIN (?:RSA |EC |OPENSSH )?PRIVATE KEY-----/,
  /\bAKIA[0-9A-Z]{16}\b/,
  /\b(?:ghp|github_pat)_[A-Za-z0-9_]{20,}\b/,
  /\bsk-[A-Za-z0-9_-]{20,}\b/,
]) {
  assert.doesNotMatch(serialized, forbidden);
}

assert.equal(manifest.approvedCount, APPROVED_RECORD_COUNT);
assert.equal(manifest.draftExcludedCount, EXCLUDED_DRAFT_COUNT);
assert.equal(manifest.sourceSeal, APPROVED_SOURCE_IDENTITY_SHA256);
assert.equal(manifest.sourceSealAlgorithm, snapshot.sourceSealAlgorithm);
assert.equal(
  manifest.snapshotSha256,
  crypto.createHash('sha256').update(snapshotBytes).digest('hex'),
  'public manifest must seal the exact committed snapshot bytes',
);
assert.equal(manifest.records.length, APPROVED_RECORD_COUNT);
assert.equal(new Set(manifest.records.map((record) => record.slug)).size, APPROVED_RECORD_COUNT);
assert.ok(manifest.records.every((record) => !('body' in record) && !('blocks' in record)));
for (const record of manifest.records) {
  assert.equal(record.href, `/akashic-records/${record.slug}`);
  assert.ok(slugs.has(record.slug));
}

assert.equal(safeHttpUrl('javascript:alert(1)'), undefined);
assert.equal(safeHttpUrl('https://user:pass@example.com/'), undefined);
assert.equal(
  safeHttpUrl('https://example.com/path?utm_source=medium&keep=yes#part'),
  'https://example.com/path?keep=yes#part',
);

const fixture = `<!doctype html><html><body><article>
<header><h1 class="p-name">Fixture title</h1></header>
<section data-field="subtitle" class="p-summary">Fixture subtitle</section>
<section data-field="body" class="e-content">
<section><hr><h3>Fixture title</h3><h4>Fixture subtitle</h4>
<p>Hello <a href="https://example.com/read?utm_source=medium&amp;keep=1">🌌 reader</a> and <a href="javascript:alert(1)">plain text</a>.</p>
<h3>Heading</h3><blockquote><p>Quoted thought</p></blockquote>
<div class="graf graf--mixtapeEmbed"><a class="markup--anchor markup--mixtapeEmbed-anchor" href="https://example.net/card?utm_medium=referral"><strong>Linked card</strong><br>Example</a><a class="js-mixtapeImage" href="https://example.net/card"></a></div>
<ol><li>First</li><li>Second</li></ol><pre>&lt;safe&gt;\ncode</pre>
<figure><img src="https://remote.invalid/image.png" alt="Described image"><figcaption>Caption</figcaption></figure>
<figure><iframe src="https://www.youtube.com/embed/abc_123"></iframe></figure></section>
</section>
<footer><p><time class="dt-published" datetime="2025-01-02T03:04:05.000Z">date</time></p>
<p><a class="p-canonical" href="https://medium.com/@author/fixture-title-0123456789ab">Canonical</a></p>
<p>Exported from <a href="https://medium.com">Medium</a> on March 6, 2026.</p></footer>
</article></body></html>`;
const fixtureRecord = parseMediumPost(fixture, '2025-01-02_Fixture-title-0123456789ab.html', 'a'.repeat(64));
assert.equal(fixtureRecord.slug, 'fixture-title-0123456789ab');
assert.equal(fixtureRecord.blocks[0].kind, 'paragraph', 'duplicated leading title/subtitle must be removed');
const fixtureParagraph = fixtureRecord.blocks.find((block) => block.kind === 'paragraph');
assert.equal(fixtureParagraph.links.length, 1, 'unsafe authored URL must become plain text');
assert.equal(fixtureParagraph.links[0].text, '🌌 reader');
assert.equal(fixtureParagraph.text.slice(fixtureParagraph.links[0].start, fixtureParagraph.links[0].end), '🌌 reader');
assert.equal(fixtureParagraph.links[0].href, 'https://example.com/read?keep=1');
assert.ok(fixtureRecord.blocks.some((block) => block.kind === 'figure' && block.alt === 'Described image'));
assert.ok(fixtureRecord.blocks.some((block) => block.kind === 'embed' && block.href === 'https://www.youtube.com/watch?v=abc_123'));
assert.ok(fixtureRecord.blocks.some((block) => block.kind === 'linkCard' && block.href === 'https://example.net/card'));

console.log(`akashic corpus : OK · ${snapshot.records.length} records · ${snapshot.sourceSeal} · snapshot ${manifest.snapshotSha256}`);
