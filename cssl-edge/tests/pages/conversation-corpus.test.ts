import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import fs from 'node:fs';
import path from 'node:path';

import {
  validatePublicConversationBrowseManifest,
  validatePublicConversationManifest,
  type ConversationCorpusBrowseManifest,
  type ConversationCorpusManifest,
} from '@/lib/conversation-corpus';

const root = process.cwd();
const read = (relative: string): string => fs.readFileSync(path.join(root, relative), 'utf8');
const manifestSource = read('public/conversation-corpus/public-index.v1.json');
const browseSource = read('public/conversation-corpus/browse.v1.json');
const manifest = JSON.parse(manifestSource) as ConversationCorpusManifest;
const browse = JSON.parse(browseSource) as ConversationCorpusBrowseManifest;
const pageSource = read('pages/conversations.tsx');
const readerSource = read('pages/conversations/[slug].tsx');
const apiSource = read('pages/api/conversation-corpus/[id].ts');
const generatorSource = read('scripts/snapshot-conversation-corpus.mjs');
const shellSource = read('components/SiteShell.tsx');
const middlewareSource = read('middleware.ts');
const sitemap = read('public/sitemap.xml');
const vercelIgnore = read('.vercelignore');

validatePublicConversationManifest(manifest);
validatePublicConversationBrowseManifest(browse);
assert.equal(manifest.counts.uniqueConversations, 1_386, 'aggregate retains the complete local conversation denominator');
assert.equal(manifest.counts.messages, 19_479, 'aggregate retains the complete local visible-turn denominator');
assert.equal(manifest.counts.publiclyApprovedConversations, 0, 'no body is public before review');
assert.equal(manifest.counts.reviewHeldConversations, 1_386, 'every current record is explicitly held');
assert.deepEqual(manifest.records, [], 'public index contains no unreviewed summary');
assert.deepEqual(browse.records, [], 'browse projection contains no unreviewed title, excerpt, or signal');
assert.doesNotMatch(manifestSource, /"(?:excerpt|humanSignal|aiSignal|bodyHref)"/u, 'aggregate carries no source-derived preview fields');
assert.doesNotMatch(browseSource, /"(?:excerpt|humanSignal|aiSignal|bodyHref)"/u, 'held browse carries no source-derived preview fields');

const approvedStub = {
  id: 'aaaaaaaaaaaaaaaaaaaa',
  slug: 'approved-a',
  editorialReviewState: 'approved',
  bodyHref: '/conversation-corpus/approved-records/aaaaaaaaaaaaaaaaaaaa.json',
};
const duplicateId = {
  ...manifest,
  counts: { ...manifest.counts, publiclyApprovedConversations: 2 },
  records: [approvedStub, { ...approvedStub, slug: 'approved-b' }],
} as unknown as ConversationCorpusManifest;
assert.throws(() => validatePublicConversationManifest(duplicateId), /CORPUS_MANIFEST_DUPLICATE_ID/u);
const duplicateSlug = {
  ...manifest,
  counts: { ...manifest.counts, publiclyApprovedConversations: 2 },
  records: [approvedStub, { ...approvedStub, id: 'bbbbbbbbbbbbbbbbbbbb' }],
} as unknown as ConversationCorpusManifest;
assert.throws(() => validatePublicConversationManifest(duplicateSlug), /CORPUS_MANIFEST_DUPLICATE_SLUG/u);
const unreviewed = {
  ...manifest,
  counts: { ...manifest.counts, publiclyApprovedConversations: 1 },
  records: [{ ...approvedStub, editorialReviewState: 'unreviewed' }],
} as unknown as ConversationCorpusManifest;
assert.throws(() => validatePublicConversationManifest(unreviewed), /CORPUS_MANIFEST_UNAPPROVED_RECORD/u);

assert.match(pageSource, /complete export is indexed locally/u, 'page states the local/public boundary in plain language');
assert.match(pageSource, /body library is deliberately closed today/u, 'page renders an honest current review-held state');
assert.match(readerSource, /summary\.indexable \? <script type="application\/ld\+json"/u, 'detail structured data is indexability-gated');
assert.match(readerSource, /public-index\.v1\.json/u, 'detail route resolves approved summaries only');
assert.doesNotMatch(readerSource, /conversation-corpus', 'index\.v1\.json/u, 'detail route never reads the legacy index');
assert.match(apiSource, /approved-records/u, 'body API reads the approved-only body store');
assert.match(apiSource, /CORPUS_REVIEW_HELD_CODE/u, 'body API exposes a stable held code');
assert.match(generatorSource, /approvedRecordRoot/u, 'generator writes only to the approved body store');
assert.match(generatorSource, /public-index\.v1\.json/u, 'generator writes the safe public index');
assert.match(shellSource, /href: '\/conversations', label: 'Conversations'/u, 'footer Explore navigation exposes the reading room');
assert.match(middlewareSource, /'\/conversation-corpus\/records\/'/u, 'legacy static bodies are blocked at the edge');
assert.match(vercelIgnore, /public\/conversation-corpus\/records\//u, 'legacy local bodies are excluded from dirty-root deployment');
assert.match(vercelIgnore, /public\/conversation-corpus\/index\.v1\.json/u, 'legacy local index is excluded from dirty-root deployment');
assert.equal((sitemap.match(/<loc>https:\/\/www\.apocky\.com\/conversations<\/loc>/gu) ?? []).length, 1, 'sitemap contains one public index route');
assert.equal((sitemap.match(/<loc>https:\/\/www\.apocky\.com\/conversations\//gu) ?? []).length, 0, 'sitemap contains no unapproved detail route');

const trackedLegacy = execFileSync('git', ['ls-files', '--', 'cssl-edge/public/conversation-corpus/index.v1.json', 'cssl-edge/public/conversation-corpus/records'], {
  cwd: path.resolve(root, '..'),
  encoding: 'utf8',
});
assert.equal(trackedLegacy.trim(), '', 'legacy index and bodies are not tracked release assets');

console.log('conversation-corpus-page.test : OK · aggregate truth + review hold + metadata/sitemap/nav/deploy gates');
