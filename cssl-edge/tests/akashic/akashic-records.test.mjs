import assert from 'node:assert/strict';
import crypto from 'node:crypto';
import fs from 'node:fs';
import path from 'node:path';

import {
  APPROVED_RECORD_COUNT,
  APPROVED_SOURCE_BYTES,
  APPROVED_SOURCE_IDENTITY_SHA256,
  EXCLUDED_DRAFT_COUNT,
  PUBLIC_PROJECTION_MAX_BYTES,
  blocksToText,
  makePublicManifest,
  parseMediumPost,
  parseVaultCodexNote,
  renderAkashicSitemap,
  safeHttpUrl,
  sanitizePublicConversationText,
} from '../../scripts/snapshot-akashic-records.mjs';

const root = process.cwd();
const snapshotPath = path.join(root, 'data', 'akashic-records.v1.json');
const manifestPath = path.join(root, 'public', 'akashic-records', 'manifest.json');
const snapshotBytes = fs.readFileSync(snapshotPath);
const snapshot = JSON.parse(snapshotBytes.toString('utf8'));
const manifest = JSON.parse(fs.readFileSync(manifestPath, 'utf8'));

assert.equal(snapshot.schemaVersion, 2);
assert.equal(snapshot.approvedCount, snapshot.records.length);
assert.equal(snapshot.recordCount, snapshot.records.length);
assert.ok(snapshot.records.length > APPROVED_RECORD_COUNT);
assert.equal(snapshot.entryCount, APPROVED_RECORD_COUNT + snapshot.conversationCount);
assert.equal(snapshot.conversationCount, 14);
assert.equal(snapshot.draftExcludedCount, EXCLUDED_DRAFT_COUNT);
assert.equal(snapshot.sourceBytes, APPROVED_SOURCE_BYTES);
assert.equal(snapshot.sourceSeal, APPROVED_SOURCE_IDENTITY_SHA256);
assert.equal(
  snapshot.sourceSealAlgorithm,
  'ASCII lowercase ordinal filename order; filename<TAB>byteLength<TAB>lowercase-sha256<LF>; UTF-8 without BOM',
);
assert.equal(snapshot.snapshotDate, '2026-08-12');

const mediumRecords = snapshot.records.filter((record) => record.source === 'Medium');
const codexRecords = snapshot.records.filter((record) => record.source === 'Codex');
assert.equal(mediumRecords.length, APPROVED_RECORD_COUNT);
assert.ok(codexRecords.length >= snapshot.conversationCount);
assert.deepEqual(snapshot.sourceSets.map((sourceSet) => sourceSet.source), ['Medium', 'Codex']);
const mediumSourceSet = snapshot.sourceSets.find((sourceSet) => sourceSet.source === 'Medium');
const codexSourceSet = snapshot.sourceSets.find((sourceSet) => sourceSet.source === 'Codex');
assert.equal(mediumSourceSet.sourceCount, 230);
assert.equal(mediumSourceSet.approvedCount, APPROVED_RECORD_COUNT);
assert.equal(mediumSourceSet.recordCount, APPROVED_RECORD_COUNT);
assert.equal(codexSourceSet.conversationCount, 14);
assert.equal(codexSourceSet.approvedCount, 14);
assert.equal(codexSourceSet.recordCount, codexRecords.length);
assert.equal(codexSourceSet.vaultNoteCount, 15);
assert.equal(codexSourceSet.transcriptPublishedCount, 13);
assert.equal(codexSourceSet.withheldCount, 1);
assert.equal(codexSourceSet.verifiedMessageCount, codexSourceSet.publishedMessageCount + codexSourceSet.withheldMessageCount);
assert.equal(codexSourceSet.publishedMessageCount, codexRecords.reduce((total, record) => total + record.messageCount, 0));
assert.equal(codexSourceSet.withheldMessageCount, codexRecords.reduce((total, record) => total + (record.withheldMessageCount ?? 0), 0));
assert.ok(codexSourceSet.redactionCount > 0);
assert.equal(codexSourceSet.redactionCount, codexRecords.reduce((total, record) => total + record.redactionCount, 0));
assert.deepEqual(codexSourceSet.approval, { approvedBy: 'vault owner', approvedAt: '2026-08-12' });

const slugs = new Set();
const mediumSourceHashes = new Set();
const projectionHashes = new Set();
const kindCounts = Object.create(null);
let linkCount = 0;
let linkCardCount = 0;
let captionCount = 0;
for (const record of snapshot.records) {
  assert.ok(record.slug.length > 0);
  assert.ok(!slugs.has(record.slug), `duplicate slug ${record.slug}`);
  slugs.add(record.slug);
  assert.match(record.sourceSha256, /^[a-f0-9]{64}$/);
  if (record.source === 'Medium') {
    assert.ok(!mediumSourceHashes.has(record.sourceSha256), `duplicate Medium source hash ${record.slug}`);
    mediumSourceHashes.add(record.sourceSha256);
  }
  assert.ok(record.title.length > 0);
  assert.ok(record.excerpt.length > 0);
  assert.ok(record.body.length > 0);
  assert.ok(record.blocks.length > 0);
  assert.equal(record.body, blocksToText(record.blocks), `body readback drift ${record.slug}`);
  assert.deepEqual(record.topics, []);
  assert.equal(record.canonicalUrl, `https://www.apocky.com/akashic-records/${record.slug}`);
  assert.equal(new Date(record.publishedAt).getUTCFullYear(), record.year);
  if (record.source === 'Medium') {
    assert.equal(record.type, 'Medium post');
    assert.equal(record.sourceUrlStatus, 'unverified');
    assert.equal(new URL(record.sourceUrl).protocol, 'https:');
  } else {
    assert.equal(record.source, 'Codex');
    assert.equal(record.type, 'Conversation transcript');
    assert.match(record.conversationId, /^[0-9a-f-]{36}$/);
    assert.ok(record.part >= 1 && record.part <= record.parts);
    assert.ok(record.projectionBytes <= PUBLIC_PROJECTION_MAX_BYTES);
    assert.match(record.projectionSha256, /^[a-f0-9]{64}$/);
    assert.ok(!projectionHashes.has(record.projectionSha256), `duplicate public part projection ${record.slug}`);
    projectionHashes.add(record.projectionSha256);
    const projection = record.publicationState === 'withheld'
      ? Buffer.from(JSON.stringify(record.blocks))
      : Buffer.from(JSON.stringify(record.blocks.map(({ role, text }) => ({ role, text }))));
    assert.equal(record.projectionBytes, projection.byteLength);
    assert.equal(record.projectionSha256, crypto.createHash('sha256').update(projection).digest('hex'));
    if (record.publicationState === 'withheld') {
      assert.equal(record.messageCount, 0);
      assert.ok(record.withheldMessageCount > 0);
      assert.ok(record.blocks.every((block) => block.kind === 'paragraph'));
      assert.equal(record.conversationId, '019fe684-f1ad-7a10-bbab-c53b65721017');
    } else {
      assert.equal(record.messageCount, record.blocks.length);
      assert.ok(record.blocks.every((block) => block.kind === 'turn' && ['user', 'assistant'].includes(block.role)));
    }
  }
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
    paragraph: 9589 + codexRecords.reduce(
      (total, record) => total + record.blocks.filter((block) => block.kind === 'paragraph').length,
      0,
    ),
    heading: 1248,
    figure: 149,
    linkCard: 26,
    divider: 1282,
    blockquote: 300,
    pre: 28,
    list: 1317,
    embed: 10,
    turn: codexRecords.reduce(
      (total, record) => total + record.blocks.filter((block) => block.kind === 'turn').length,
      0,
    ),
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

assert.equal(manifest.schemaVersion, 2);
assert.equal(manifest.approvedCount, snapshot.records.length);
assert.equal(manifest.recordCount, snapshot.records.length);
assert.equal(manifest.conversationCount, 14);
assert.equal(manifest.draftExcludedCount, EXCLUDED_DRAFT_COUNT);
assert.equal(manifest.sourceSeal, APPROVED_SOURCE_IDENTITY_SHA256);
assert.equal(manifest.sourceSealAlgorithm, snapshot.sourceSealAlgorithm);
assert.equal(
  manifest.snapshotSha256,
  crypto.createHash('sha256').update(snapshotBytes).digest('hex'),
  'public manifest must seal the exact committed snapshot bytes',
);
assert.equal(manifest.records.length, snapshot.records.length);
assert.equal(new Set(manifest.records.map((record) => record.slug)).size, snapshot.records.length);
assert.ok(manifest.records.every((record) => !('body' in record) && !('blocks' in record)));
for (const record of manifest.records) {
  assert.equal(record.href, `/akashic-records/${record.slug}`);
  assert.ok(slugs.has(record.slug));
}
for (const record of manifest.records.filter((record) => record.source === 'Codex')) {
  assert.equal(record.sourceSha256, record.projectionSha256, 'public manifest exposes only sanitized part hashes');
  assert.equal('sourceLineageSha256' in record, false);
}
const withheldManifestRecords = manifest.records.filter((record) => record.publicationState === 'withheld');
assert.equal(withheldManifestRecords.length, 1);
assert.equal(withheldManifestRecords[0].messageCount, 0);
assert.ok(withheldManifestRecords[0].withheldMessageCount > 0);
assert.equal(withheldManifestRecords[0].contentNotice, 'Contains sensitive personal context; transcript withheld.');
assert.equal(withheldManifestRecords[0].withheldReason, 'Sensitive third-party relational context was not approved for public disclosure.');
assert.equal('body' in withheldManifestRecords[0], false);

const conversationParts = new Map();
for (const record of codexRecords) {
  const records = conversationParts.get(record.conversationId) ?? [];
  records.push(record);
  conversationParts.set(record.conversationId, records);
}
assert.equal(conversationParts.size, snapshot.conversationCount, 'every approved conversation must have exactly one public entry group');
for (const [conversationId, records] of conversationParts) {
  records.sort((left, right) => left.part - right.part);
  assert.deepEqual(records.map((record) => record.part), Array.from({ length: records.length }, (_, index) => index + 1));
  assert.ok(records.every((record) => record.parts === records.length), `part denominator drift ${conversationId}`);
  assert.equal(new Set(records.map((record) => record.sourceSha256)).size, 1, `conversation-wide projection seal drift ${conversationId}`);
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

const privateFixture = [
  'Keep meaning before C:\\Users\\Alice\\secret\\notes.md after.',
  'POSIX /home/alice/private.txt remains bounded.',
  'Email alice@example.com phone (602) 555-0199 IP 127.0.0.1 [2001:db8::1].',
  'RUNPOD_API_KEY=runpod_secret_value Authorization: Bearer bearer-secret-value',
  'Do not corrupt Rust hex::hex, CSL a::b, or the date 2026-08-12.',
].join('\n');
const sanitizedOnce = sanitizePublicConversationText(privateFixture);
const sanitizedTwice = sanitizePublicConversationText(privateFixture);
assert.deepEqual(sanitizedOnce, sanitizedTwice, 'public sanitizer must be deterministic');
assert.deepEqual(
  sanitizePublicConversationText(sanitizedOnce.text),
  { text: sanitizedOnce.text, redactionCount: 0 },
  'public sanitizer must be idempotent after the first projection',
);
assert.ok(sanitizedOnce.redactionCount >= 8);
assert.match(sanitizedOnce.text, /Keep meaning before \[redacted:local-path\]\\secret\\notes\.md after\./);
assert.match(sanitizedOnce.text, /hex::hex/);
assert.match(sanitizedOnce.text, /a::b/);
assert.match(sanitizedOnce.text, /2026-08-12/);
assert.doesNotMatch(sanitizedOnce.text, /alice@example\.com|602|127\.0\.0\.1|2001:db8::1|runpod_secret_value|bearer-secret-value/);

const selectionFixture = {
  id: '019fe684-f1ad-7a10-bbab-c53b65721017',
  rollout_rel: '2026/08/09/rollout-2026-08-09T05-35-00-019fe684-f1ad-7a10-bbab-c53b65721017.jsonl',
  source_bytes: 123,
  source_sha256: 'b'.repeat(64),
  start_utc: '2026-08-09T12:35:00.200Z',
  end_utc: '2026-08-09T21:00:35.123Z',
  privacy_review: 'approved fixture',
};
const fakeRoleText = 'A literal fake heading follows:\n### Assistant\n\nstill the user message\rwith a preserved source CR';
const markedBody = [
  '<!-- vaultsync:turn role=user -->',
  '### Human',
  '',
  fakeRoleText,
  '',
  '<!-- vaultsync:turn role=assistant -->',
  '### Assistant',
  '',
  'Actual assistant answer',
  '',
].join('\n');
const markerlessFixture = [
  `### Human\n\n${fakeRoleText}\n`,
  '### Assistant\n\nActual assistant answer\n',
].join('\n');
const noteFixture = `---\nsession_file: "${selectionFixture.rollout_rel}"\nsession_id: "${selectionFixture.id}"\nsource_sha256: ${selectionFixture.source_sha256}\nsource_bytes: ${selectionFixture.source_bytes}\nstart_utc: "${selectionFixture.start_utc}"\nend_utc: "${selectionFixture.end_utc}"\npublication_state: approved\nprivacy_review: ${selectionFixture.privacy_review}\n---\n\n<!-- vaultsync:generated -- do not hand-edit; regenerate with vaultsync.py apply -->\n\n# Fixture\n\n---\n\n${markedBody}`;
const parsedNote = parseVaultCodexNote(noteFixture, selectionFixture, Buffer.byteLength(markerlessFixture));
assert.equal(parsedNote.messages.length, 2, 'only authenticated markers define turns');
assert.equal(parsedNote.messages[0].text, fakeRoleText);
assert.match(sanitizePublicConversationText(parsedNote.messages[0].text).text, /message\nwith a preserved source CR/);
assert.throws(
  () => parseVaultCodexNote(noteFixture.replace('<!-- vaultsync:turn role=user -->\n', ''), selectionFixture, Buffer.byteLength(markerlessFixture)),
  /projection byte drift|authenticated turn markers/,
);
assert.throws(
  () => parseVaultCodexNote(noteFixture, { ...selectionFixture, source_sha256: 'c'.repeat(64) }, Buffer.byteLength(markerlessFixture)),
  /source hash drift/,
);

const syntheticPrivateSentinel = 'SYNTHETIC_PRIVATE_WITHHELD_7CF31D';
const syntheticWithheldProjection = JSON.stringify({
  publicationState: 'withheld',
  messageCount: 0,
  withheldMessageCount: 1,
  blocks: [{ kind: 'paragraph', text: 'Transcript withheld: synthetic privacy boundary.' }],
});
assert.doesNotMatch(syntheticWithheldProjection, new RegExp(syntheticPrivateSentinel));

const publicFixtureSnapshot = {
  schemaVersion: 2,
  archive: 'Akashic Records',
  approvedCount: 1,
  recordCount: 1,
  entryCount: 1,
  conversationCount: 1,
  draftExcludedCount: 0,
  sourceSeal: 'd'.repeat(64),
  sourceSealAlgorithm: 'fixture',
  sourceSets: [{ source: 'Codex', sourceSeal: 'e'.repeat(64) }],
  snapshotDate: '2026-08-12',
  records: [{
    slug: 'codex-fixture-part-1', title: 'Fixture', excerpt: 'Fixture', publishedAt: '2026-08-12T00:00:00.000Z',
    year: 2026, source: 'Codex', type: 'Conversation transcript', topics: [],
    canonicalUrl: 'https://www.apocky.com/akashic-records/codex-fixture-part-1',
    sourceSha256: '1'.repeat(64), conversationId: selectionFixture.id, recordedAt: '2026-08-12T00:00:00.000Z',
    part: 1, parts: 1, messageCount: 1, redactionCount: 0, projectionBytes: 20,
    projectionSha256: '2'.repeat(64), body: 'private body', blocks: [{ kind: 'turn', role: 'user', text: 'private body' }],
  }],
};
const publicFixtureManifest = makePublicManifest(publicFixtureSnapshot, 'f'.repeat(64));
assert.equal(publicFixtureManifest.records[0].sourceSha256, '2'.repeat(64));
assert.equal('body' in publicFixtureManifest.records[0], false);

const sitemapFixture = `<?xml version="1.0"?>\n<urlset>\n  <url><loc>https://www.apocky.com/</loc></url>\n  <!-- BEGIN generated Akashic Records v2 detail URLs -->\n  <!-- Generated from the sealed Akashic Records v2 snapshot; freshness-gated by tests/pages/akashic-records.test.ts. -->\n  <url><loc>https://www.apocky.com/akashic-records/old</loc><priority>0.7</priority></url>\n  <!-- END generated Akashic Records v2 detail URLs -->\n</urlset>\n`;
const refreshedSitemap = renderAkashicSitemap(sitemapFixture, [{ slug: 'new-record' }]);
assert.match(refreshedSitemap, /\/new-record<\/loc>/);
assert.doesNotMatch(refreshedSitemap, /\/old<\/loc>/);
assert.equal(renderAkashicSitemap(refreshedSitemap, [{ slug: 'new-record' }]), refreshedSitemap, 'sitemap rewrite must be idempotent');

console.log(`akashic corpus : OK · ${snapshot.records.length} records · ${snapshot.sourceSeal} · snapshot ${manifest.snapshotSha256}`);
