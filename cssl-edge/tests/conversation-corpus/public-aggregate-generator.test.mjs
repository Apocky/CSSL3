import assert from 'node:assert/strict';
import { execFileSync, spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, symlinkSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

import {
  LEGACY_AGGREGATE_SHA256,
  BROWSE_SHA256,
  PUBLIC_AGGREGATE_FACTS_SHA256,
  PUBLIC_INDEX_SHA256,
  buildPublicAggregate,
  findPublicAggregateOutputDrift,
  loadCommittedPublicAggregateFacts,
  validatePublicAggregateFacts,
} from '../../scripts/generate-public-conversation-aggregate.mjs';
import {
  assertExternalReviewOutput,
  assertCanonicalExternalReviewOutput,
  parseReviewArgs,
} from '../../scripts/snapshot-conversation-corpus.mjs';

const root = resolve(dirname(fileURLToPath(import.meta.url)), '..', '..');
const read = (relative) => readFileSync(join(root, relative));
const text = (relative) => read(relative).toString('utf8');
const hash = (value) => createHash('sha256').update(value).digest('hex');
const clone = (value) => JSON.parse(JSON.stringify(value));

const factsBytes = read('data/public-conversation-aggregate.v1.json');
const facts = JSON.parse(factsBytes.toString('utf8'));
const generated = buildPublicAggregate(facts);
const current = {
  publicIndexBytes: text('public/conversation-corpus/public-index.v1.json'),
  browseBytes: text('public/conversation-corpus/browse.v1.json'),
};

assert.equal(hash(factsBytes), PUBLIC_AGGREGATE_FACTS_SHA256, 'committed public facts retain their independent byte seal');
assert.equal(facts.sourceSeal.aggregateSha256, LEGACY_AGGREGATE_SHA256, 'safe facts retain the reviewed aggregate source seal');
assert.deepEqual(findPublicAggregateOutputDrift(generated, current), [], 'both public projections are byte-exact products of committed safe facts');
assert.equal(hash(generated.publicIndexBytes), PUBLIC_INDEX_SHA256);
assert.equal(hash(generated.browseBytes), BROWSE_SHA256);
assert.deepEqual(generated.publicIndex.records, [], 'aggregate generator cannot emit a public record body or summary');
assert.deepEqual(generated.browse.records, [], 'browse generator cannot emit an unreviewed browse record');

const committed = await loadCommittedPublicAggregateFacts();
assert.equal(committed.counts.uniqueConversations, 1_386);
assert.equal(committed.counts.reviewHeldConversations, 1_386);
assert.equal(committed.counts.publishedMessages, 0);

assert.deepEqual(
  findPublicAggregateOutputDrift(generated, { ...current, publicIndexBytes: `${current.publicIndexBytes} ` }),
  ['public/conversation-corpus/public-index.v1.json'],
  'a one-byte output change fails the drift oracle',
);

const sourceSealDrift = clone(facts);
sourceSealDrift.sourceSeal.aggregateSha256 = '0'.repeat(64);
assert.throws(() => validatePublicAggregateFacts(sourceSealDrift), /PUBLIC_AGGREGATE_SOURCE_SEAL_DRIFT/u);

const providerDrift = clone(facts);
providerDrift.counts.chatgptConversations -= 1;
assert.throws(() => validatePublicAggregateFacts(providerDrift), /PUBLIC_AGGREGATE_PROVIDER_DENOMINATOR_DRIFT/u);

const messageDrift = clone(facts);
messageDrift.counts.messages += 1;
assert.throws(() => validatePublicAggregateFacts(messageDrift), /PUBLIC_AGGREGATE_MESSAGE_DENOMINATOR_DRIFT/u);

const unauthorizedApproval = clone(facts);
unauthorizedApproval.counts.publiclyApprovedConversations = 1;
unauthorizedApproval.counts.reviewHeldConversations -= 1;
assert.throws(() => validatePublicAggregateFacts(unauthorizedApproval), /PUBLIC_AGGREGATE_APPROVAL_REQUIRES_PROMOTION_PIPELINE/u);

const privateField = clone(facts);
privateField.publication.body = 'not admissible even when redacted';
assert.throws(() => validatePublicAggregateFacts(privateField), /PUBLIC_AGGREGATE_PRIVATE_KEY/u);

const privatePath = clone(facts);
privatePath.publication.authority = ['local source at C:', 'Users', 'Example', 'export.json'].join('\\');
assert.throws(() => validatePublicAggregateFacts(privatePath), /PUBLIC_AGGREGATE_PRIVATE_VALUE/u);

const publicGeneratorSource = text('scripts/generate-public-conversation-aggregate.mjs');
assert.doesNotMatch(publicGeneratorSource, /APOCKY_CHATGPT_EXPORT_DIR|APOCKY_CLAUDE_CONVERSATIONS_JSON|APOCKY_CHATGPT_CATEGORIES_CSV/u, 'public aggregate generation has no raw-export input rail');

const checked = execFileSync(process.execPath, ['scripts/generate-public-conversation-aggregate.mjs', '--check'], { cwd: root, encoding: 'utf8' });
assert.match(checked, /CURRENT/u);
assert.match(checked, /bodies=0/u);
const missingMode = spawnSync(process.execPath, ['scripts/generate-public-conversation-aggregate.mjs'], { cwd: root, encoding: 'utf8' });
assert.equal(missingMode.status, 1, 'public writer requires an explicit --check or --write mode');
assert.match(missingMode.stderr, /PUBLIC_AGGREGATE_MODE_REQUIRED/u);

const retiredEntry = spawnSync(process.execPath, ['scripts/snapshot-conversation-corpus.mjs'], { cwd: root, encoding: 'utf8' });
assert.equal(retiredEntry.status, 1, 'ambiguous legacy entry point is fail-closed');
assert.match(retiredEntry.stderr, /implementation-only/u);

assert.throws(() => assertExternalReviewOutput(root), /outside the repository/u);
assert.throws(() => assertExternalReviewOutput(join(root, 'public', 'conversation-corpus')), /outside the repository/u);
const externalReviewRoot = join(tmpdir(), 'apocky-conversation-review-test');
assert.equal(assertExternalReviewOutput(externalReviewRoot), resolve(externalReviewRoot));
assert.throws(() => parseReviewArgs([
  '--chatgpt-dir', 'chatgpt', '--claude-json', 'claude.json', '--categories', 'categories.csv',
  '--review-output', join(root, 'review-output'),
]), /outside the repository/u, 'raw review generation cannot target a repository or deployment path');

const reviewSandbox = mkdtempSync(join(tmpdir(), 'apocky-conversation-review-boundary-'));
try {
  const safeExternalRoot = join(reviewSandbox, 'safe-output');
  mkdirSync(safeExternalRoot);
  assert.equal(await assertCanonicalExternalReviewOutput(safeExternalRoot), resolve(safeExternalRoot));

  const repositoryAlias = join(reviewSandbox, 'repository-alias');
  symlinkSync(root, repositoryAlias, process.platform === 'win32' ? 'junction' : 'dir');
  await assert.rejects(
    assertCanonicalExternalReviewOutput(repositoryAlias),
    /symbolic link|junction|resolves inside the repository/u,
    'an external-looking alias cannot route review output back into the repository',
  );
} finally {
  rmSync(reviewSandbox, { recursive: true, force: true });
}

const trackedPrivate = execFileSync('git', [
  'ls-files', '--',
  'cssl-edge/public/conversation-corpus/index.v1.json',
  'cssl-edge/public/conversation-corpus/records',
], { cwd: resolve(root, '..'), encoding: 'utf8' });
assert.equal(trackedPrivate.trim(), '', 'legacy index and record bodies remain outside Git');

console.log('public-conversation-aggregate-generator.test : OK · exact bytes + sealed facts + fail-closed review/output boundaries');
