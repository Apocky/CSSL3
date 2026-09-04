import { createHash } from 'node:crypto';
import { readFile, readdir } from 'node:fs/promises';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const EXPECTED_CONVERSATIONS = 1_386;
const scriptRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const corpusRoot = resolve(process.argv[2] ?? join(scriptRoot, 'public', 'conversation-corpus'));
const approvedRecordRoot = join(corpusRoot, 'approved-records');
const repositoryRoot = resolve(corpusRoot, '..', '..');

const FORBIDDEN = Object.freeze([
  ['windows-path', /(?<![A-Za-z])[A-Za-z]:[\\/]/i],
  ['unc-path', /\\\\[A-Za-z0-9._-]{2,}\\[A-Za-z0-9._$ -]{2,}/i],
  ['unix-private-path', /(?:\/Users\/|\/home\/|\/root\/|\/mnt\/[a-z]\/|\/tmp\/)/i],
  ['file-uri', /\bfile:(?:\/\/)?/i],
  ['email', /\b[A-Z0-9.!#$%&'*+/=?^_`{|}~-]+@[A-Z0-9](?:[A-Z0-9.-]*[A-Z0-9])?\b/i],
  ['north-american-phone', /(?<!\d)(?:\+?1[ .-]?)?(?:\(\d{3}\)|\d{3})[ .-]\d{3}[ .-]\d{4}(?!\d)/],
  ['government-id', /\b\d{3}-\d{2}-\d{4}\b/],
  ['payment-number', /\b(?:\d[ -]*?){13,19}\b/],
  ['aws-key', /\bAKIA[0-9A-Z]{16}\b/],
  ['github-token', /\b(?:ghp|gho|ghu|ghs|ghr|github_pat)_[A-Za-z0-9_]{20,}\b/],
  ['openai-key', /\bsk-(?:live-)?[A-Za-z0-9_-]{20,}\b/],
  ['jwt', /\beyJ[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\b/],
  ['private-key', /-----BEGIN (?:RSA |EC |OPENSSH |DSA )?PRIVATE KEY-----/i],
]);

const sha256 = (value) => createHash('sha256').update(value).digest('hex');

function allStrings(value, path = '$', found = []) {
  if (typeof value === 'string') found.push([path, value]);
  else if (Array.isArray(value)) value.forEach((item, index) => allStrings(item, `${path}[${index}]`, found));
  else if (value !== null && typeof value === 'object') {
    Object.entries(value).forEach(([key, item]) => allStrings(item, `${path}.${key}`, found));
  }
  return found;
}

function projection(record) {
  const { projectionSha256: _sha, projectionBytes: _bytes, ...source } = record;
  const serialized = JSON.stringify(source);
  return { sha256: sha256(serialized), bytes: Buffer.byteLength(serialized, 'utf8') };
}

function duplicates(values) {
  const seen = new Set();
  const repeated = new Set();
  for (const value of values) (seen.has(value) ? repeated : seen).add(value);
  return [...repeated];
}

async function json(path) {
  return JSON.parse(await readFile(path, 'utf8'));
}

async function namesIfPresent(path) {
  try { return (await readdir(path)).filter((name) => name.endsWith('.json')).sort(); }
  catch (error) {
    if (error?.code === 'ENOENT') return [];
    throw error;
  }
}

const [manifest, browse, sitemap, readerSource, middlewareSource, vercelIgnore, names, legacyNames] = await Promise.all([
  json(join(corpusRoot, 'public-index.v1.json')),
  json(join(corpusRoot, 'browse.v1.json')),
  readFile(join(repositoryRoot, 'public', 'sitemap.xml'), 'utf8'),
  readFile(join(repositoryRoot, 'pages', 'conversations', '[slug].tsx'), 'utf8'),
  readFile(join(repositoryRoot, 'middleware.ts'), 'utf8'),
  readFile(join(repositoryRoot, '.vercelignore'), 'utf8'),
  namesIfPresent(approvedRecordRoot),
  namesIfPresent(join(corpusRoot, 'records')),
]);

const failures = [];
const residuals = [];
const summaries = manifest.records ?? [];
const browseRecords = browse.records ?? [];
const summaryById = new Map(summaries.map((record) => [record.id, record]));

if (manifest.schema !== 'apocky.public-conversation-corpus.v1') failures.push('public manifest schema mismatch');
if (browse.schema !== 'apocky.public-conversation-corpus.browse.v1') failures.push('browse manifest schema mismatch');
if (manifest.publicationState !== 'aggregate-public-bodies-review-held') failures.push('public manifest is not review-held');
if (browse.publicationState !== manifest.publicationState) failures.push('browse publication state mismatch');
if (manifest.counts?.uniqueConversations !== EXPECTED_CONVERSATIONS) failures.push(`local denominator ${manifest.counts?.uniqueConversations ?? 0}`);
if ((manifest.counts?.chatgptConversations ?? 0) + (manifest.counts?.claudeConversations ?? 0) !== EXPECTED_CONVERSATIONS) failures.push('provider denominator mismatch');
if ((manifest.counts?.publiclyApprovedConversations ?? -1) !== summaries.length) failures.push('approved summary denominator mismatch');
if ((manifest.counts?.reviewHeldConversations ?? 0) + (manifest.counts?.rejectedConversations ?? 0) + summaries.length !== EXPECTED_CONVERSATIONS) failures.push('review-state denominator mismatch');
if (manifest.counts?.publishedMessages > manifest.counts?.messages) failures.push('published messages exceed local denominator');
if (summaries.length !== names.length) failures.push('approved body-file denominator mismatch');
if (browseRecords.length !== summaries.length) failures.push('browse/manifest denominator mismatch');
if (summaries.some((record) => Object.hasOwn(record, 'messages'))) failures.push('public manifest embeds message bodies');
if (browseRecords.some((record) => Object.hasOwn(record, 'messages'))) failures.push('browse manifest embeds message bodies');
if (duplicates(summaries.map((record) => record.id)).length > 0) failures.push('duplicate approved IDs');
if (duplicates(summaries.map((record) => record.slug)).length > 0) failures.push('duplicate approved slugs');
if (duplicates(browseRecords.map((record) => record.id)).length > 0) failures.push('duplicate browse IDs');
if (duplicates(browseRecords.map((record) => record.slug)).length > 0) failures.push('duplicate browse slugs');

let publishedMessages = 0;
for (const name of names) {
  const record = await json(join(approvedRecordRoot, name));
  const summary = summaryById.get(record.id);
  if (name !== `${record.id}.json`) failures.push(`${record.id}: filename mismatch`);
  if (!summary) failures.push(`${record.id}: body lacks approved summary`);
  if (record.editorialReviewState !== 'approved') failures.push(`${record.id}: body is not editorially approved`);
  if (record.publication?.state !== 'owner-approved-public-projection') failures.push(`${record.id}: publication approval absent`);
  if (record.rightsHoldCount !== 0 || record.privacyHoldCount !== 0) failures.push(`${record.id}: held content entered approved store`);
  if (!Array.isArray(record.messages)) failures.push(`${record.id}: messages is not an array`);
  if (record.messages?.some((message) => message.rights?.reviewState !== 'owner-approved' || message.privacy?.reviewState !== 'owner-approved')) failures.push(`${record.id}: message review attestation absent`);
  if (record.messageCount !== record.messages?.length) failures.push(`${record.id}: message count mismatch`);
  const measured = projection(record);
  if (record.projectionSha256 !== measured.sha256 || record.projectionBytes !== measured.bytes) failures.push(`${record.id}: projection seal mismatch`);
  if (summary && summary.projectionSha256 !== record.projectionSha256) failures.push(`${record.id}: summary/body seal mismatch`);
  publishedMessages += record.messages?.length ?? 0;
  for (const [path, value] of allStrings(record)) {
    for (const [kind, pattern] of FORBIDDEN) if (pattern.test(value)) residuals.push({ id: record.id, path, kind });
  }
}

if (publishedMessages !== manifest.counts?.publishedMessages) failures.push('published message denominator mismatch');
for (const record of summaries) {
  if (record.editorialReviewState !== 'approved') failures.push(`${record.id}: unapproved summary is public`);
  if (!record.bodyHref?.startsWith('/conversation-corpus/approved-records/')) failures.push(`${record.id}: unsafe body route`);
  const browseRecord = browseRecords.find((candidate) => candidate.id === record.id);
  if (!browseRecord || browseRecord.slug !== record.slug || browseRecord.editorialReviewState !== 'approved') failures.push(`${record.id}: browse parity mismatch`);
}
for (const [path, value] of [...allStrings(manifest, '$manifest'), ...allStrings(browse, '$browse')]) {
  for (const [kind, pattern] of FORBIDDEN) if (pattern.test(value)) residuals.push({ id: 'aggregate', path, kind });
}
if (JSON.stringify(manifest).includes('/conversation-corpus/records/')) failures.push('manifest references legacy body store');
if (JSON.stringify(browse).includes('/conversation-corpus/records/')) failures.push('browse references legacy body store');
if (!/summary\.indexable\s*\?\s*<script type="application\/ld\+json"/u.test(readerSource)) failures.push('detail JSON-LD is not indexability-gated');
if (!readerSource.includes("'public-index.v1.json'")) failures.push('reader does not use approved-only public index');
if (!middlewareSource.includes("'/conversation-corpus/index.v1.json'")) failures.push('legacy index static route is not blocked');
if (!middlewareSource.includes("'/conversation-corpus/records/'")) failures.push('legacy body static route is not blocked');
if (!vercelIgnore.includes('public/conversation-corpus/index.v1.json')) failures.push('legacy index missing from .vercelignore');
if (!vercelIgnore.includes('public/conversation-corpus/records/')) failures.push('legacy bodies missing from .vercelignore');

const conversationIndexUrls = [...sitemap.matchAll(/<loc>https:\/\/www\.apocky\.com\/conversations<\/loc>/gu)];
if (conversationIndexUrls.length !== 1) failures.push('sitemap conversation index parity mismatch');
const sitemapSlugs = [...sitemap.matchAll(/<loc>https:\/\/www\.apocky\.com\/conversations\/([^<]+)<\/loc>/gu)].map((match) => match[1]);
const expectedSlugs = summaries.filter((record) => record.indexable).map((record) => record.slug).sort();
if (duplicates(sitemapSlugs).length > 0 || JSON.stringify([...sitemapSlugs].sort()) !== JSON.stringify(expectedSlugs)) failures.push('sitemap approved/indexable parity mismatch');
if (manifest.counts?.indexable !== expectedSlugs.length) failures.push('indexable denominator mismatch');
if (residuals.length > 0) failures.push(`${residuals.length} residual private patterns in public projection`);

const report = {
  state: failures.length === 0 ? 'PASS' : 'FAIL',
  localConversations: manifest.counts?.uniqueConversations ?? 0,
  localMessages: manifest.counts?.messages ?? 0,
  publicBodies: names.length,
  publicMessages: publishedMessages,
  heldForReview: manifest.counts?.reviewHeldConversations ?? 0,
  indexable: expectedSlugs.length,
  preservedLegacyLocalBodies: legacyNames.length,
  residuals: residuals.slice(0, 20),
  failures: failures.slice(0, 100),
};
console.log(JSON.stringify(report, null, 2));
if (failures.length > 0) process.exitCode = 1;
