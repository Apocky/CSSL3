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
const bundledManifestSource = read('lib/server/conversation-corpus-manifest.ts');
const publicGeneratorSource = read('scripts/generate-public-conversation-aggregate.mjs');
const reviewBuilderSource = read('scripts/snapshot-conversation-corpus.mjs');
const reviewEntrySource = read('scripts/build-conversation-review-corpus.mjs');
const nextConfigSource = read('next.config.js');
const shellSource = read('components/SiteShell.tsx');
const middlewareSource = read('middleware.ts');
const sitemap = read('public/sitemap.xml');
const vercelIgnore = read('.vercelignore');

validatePublicConversationManifest(manifest);
validatePublicConversationBrowseManifest(browse);
assert.equal(manifest.counts.uniqueConversations, 1_386, 'aggregate retains the complete local conversation denominator');
assert.equal(manifest.counts.messages, 19_479, 'aggregate retains the complete local visible-turn denominator');
assert.equal(manifest.counts.automatedFeatureCandidates, 473, 'candidate denominator remains numeric and source-sealed');
assert.equal(manifest.counts.publiclyApprovedConversations, 0, 'no body is public before review');
assert.equal(manifest.counts.reviewHeldConversations, 1_386, 'every current record is explicitly held');
assert.deepEqual(manifest.records, [], 'public index contains no unreviewed summary');
assert.deepEqual(browse.records, [], 'browse projection contains no unreviewed title, excerpt, or signal');
assert.deepEqual(browse.counts, manifest.counts, 'browse and canonical aggregate counts remain identical');
assert.deepEqual(browse.structuralExclusions, manifest.structuralExclusions, 'browse preserves structural-exclusion aggregates');
assert.deepEqual(browse.qualityAudit, manifest.qualityAudit, 'browse preserves quality-audit aggregates');
assert.deepEqual(manifest.structuralExclusions, {
  ChatGPT: { structuralRoleMessages: 7827, hiddenMessages: 183, toolDirectedMessages: 3323, reasoningOrToolBodies: 3472, emptyVisibleBodies: 1645 },
  Claude: { thinkingBlocks: 7571, toolUseBlocks: 7721, toolResultBlocks: 7647, flagBlocks: 3, emptyVisibleBodies: 187 },
}, 'structural-exclusion counts match the sealed legacy aggregate');
assert.deepEqual(manifest.qualityAudit, {
  recordsScored: 1386,
  qualityScoreMin: 7,
  qualityScoreMax: 100,
  qualityScoreMeanMilli: 77439,
  recordsWithContentWarnings: 495,
  automatedFeatureCandidates: 473,
  indexableCandidates: 930,
  reviewHeld: 1386,
}, 'quality aggregates are numeric and explicit');
assert.equal(manifest.aggregateSourceSha256, '8bcce56ee0d179e07150f68aaa3423805954ad734b76157365aacfaac3dfe8a2');
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
assert.match(readerSource, /getBundledPublicConversationManifest/u, 'detail route resolves the traced approved-only manifest');
assert.match(bundledManifestSource, /public\/conversation-corpus\/public-index\.v1\.json/u, 'server manifest is statically bundled for serverless runtime');
assert.doesNotMatch(readerSource, /process\.cwd\(\).*public-index/u, 'detail route does not depend on runtime public filesystem layout');
assert.doesNotMatch(readerSource, /conversation-corpus', 'index\.v1\.json/u, 'detail route never reads the legacy index');
assert.match(apiSource, /approved-records/u, 'body API reads the approved-only body store');
assert.match(apiSource, /CORPUS_REVIEW_HELD_CODE/u, 'body API exposes a stable held code');
assert.match(apiSource, /getBundledPublicConversationManifest/u, 'body API resolves review holds before filesystem body access');
assert.match(nextConfigSource, /public\/conversation-corpus\/approved-records\/\*\*\/\*\.json/u, 'future approved bodies are explicitly traced into the API bundle');
assert.match(publicGeneratorSource, /public-conversation-aggregate\.v1\.json/u, 'public aggregate reads the committed privacy-safe facts only');
assert.match(publicGeneratorSource, /PUBLIC_AGGREGATE_APPROVAL_REQUIRES_PROMOTION_PIPELINE/u, 'aggregate writer cannot grant body publication authority');
assert.doesNotMatch(publicGeneratorSource, /APOCKY_CHATGPT_EXPORT_DIR|APOCKY_CLAUDE_CONVERSATIONS_JSON/u, 'public aggregate has no raw-export input rail');
assert.match(reviewBuilderSource, /must be outside the repository/u, 'raw review builder cannot target public or tracked paths');
assert.match(reviewBuilderSource, /implementation-only and cannot write public assets/u, 'ambiguous legacy entry point fails closed');
assert.match(reviewEntrySource, /buildReviewCorpus/u, 'raw review work has a separately named entry point');
assert.match(shellSource, /href: '\/conversations', label: 'Thoughts & conversations'/u, 'footer Explore navigation exposes the reading room');
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
