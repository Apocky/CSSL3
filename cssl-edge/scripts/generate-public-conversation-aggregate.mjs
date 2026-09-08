import { createHash } from 'node:crypto';
import { readFile, writeFile } from 'node:fs/promises';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const repositoryRoot = resolve(dirname(fileURLToPath(import.meta.url)), '..');
const factsPath = join(repositoryRoot, 'data', 'public-conversation-aggregate.v1.json');
const outputRoot = join(repositoryRoot, 'public', 'conversation-corpus');

export const PUBLIC_AGGREGATE_FACTS_SHA256 = '87c2976437d98dd3ccc6606d13a4934addae3578925fa9905d7100f0bd1c689f';
export const LEGACY_AGGREGATE_SHA256 = '8bcce56ee0d179e07150f68aaa3423805954ad734b76157365aacfaac3dfe8a2';
export const PUBLIC_INDEX_SHA256 = 'e83b71314a4c9170bd287b75f4873e58f9ba13adb12123abb32adc6732714a28';
export const BROWSE_SHA256 = '9d381b481696ed79224eeef0ffd2fe292bc61b279139e0444f896564db25a818';

const TOP_LEVEL_KEYS = Object.freeze([
  'schema', 'generatedAt', 'publication', 'counts', 'structuralExclusions', 'qualityAudit', 'sourceSeal',
]);
const PUBLICATION_KEYS = Object.freeze([
  'state', 'authority', 'publicIndexScope', 'publicIndexBoundaries', 'selectionCriteria', 'browseScope', 'browseBoundaries',
]);
const SELECTION_KEYS = Object.freeze(['localInclusion', 'publicAdmission', 'indexability', 'curatedLayer']);
const COUNT_KEYS = Object.freeze([
  'uniqueConversations', 'chatgptConversations', 'claudeConversations', 'anthropicDuplicateDelivery',
  'messages', 'emptyConversationRecords', 'userMessages', 'assistantMessages', 'alternateBranchMessages',
  'redactions', 'automatedFeatureCandidates', 'editoriallyFeatureEligible', 'indexable',
  'publiclyApprovedConversations', 'reviewHeldConversations', 'rejectedConversations', 'publishedMessages',
]);
const STRUCTURAL_KEYS = Object.freeze({
  ChatGPT: Object.freeze(['structuralRoleMessages', 'hiddenMessages', 'toolDirectedMessages', 'reasoningOrToolBodies', 'emptyVisibleBodies']),
  Claude: Object.freeze(['thinkingBlocks', 'toolUseBlocks', 'toolResultBlocks', 'flagBlocks', 'emptyVisibleBodies']),
});
const QUALITY_KEYS = Object.freeze([
  'recordsScored', 'qualityScoreMin', 'qualityScoreMax', 'qualityScoreMeanMilli',
  'recordsWithContentWarnings', 'automatedFeatureCandidates', 'indexableCandidates', 'reviewHeld',
]);
const SOURCE_SEAL_KEYS = Object.freeze(['algorithm', 'aggregateSha256', 'derivation']);
const FORBIDDEN_KEYS = new Set([
  'records', 'record', 'title', 'slug', 'excerpt', 'humanSignal', 'aiSignal', 'body', 'bodyHref',
  'sourcePath', 'sourceReference', 'sourceFingerprint', 'exportFingerprint', 'contentSha256', 'messagesByRole',
]);
const FORBIDDEN_STRINGS = Object.freeze([
  ['windows-path', /(?<![A-Za-z])[A-Za-z]:[\\/]/iu],
  ['unc-path', /\\\\[A-Za-z0-9._-]{2,}\\[A-Za-z0-9._$ -]{2,}/iu],
  ['unix-private-path', /(?:\/Users\/|\/home\/|\/root\/|\/mnt\/[a-z]\/|\/tmp\/)/iu],
  ['file-uri', /\bfile:(?:\/\/)?/iu],
  ['email', /\b[A-Z0-9.!#$%&'*+/=?^_`{|}~-]+@[A-Z0-9](?:[A-Z0-9.-]*[A-Z0-9])?\b/iu],
  ['north-american-phone', /(?<!\d)(?:\+?1[ .-]?)?(?:\(\d{3}\)|\d{3})[ .-]\d{3}[ .-]\d{4}(?!\d)/u],
  ['government-id', /\b\d{3}-\d{2}-\d{4}\b/u],
  ['payment-number', /\b(?:\d[ -]*?){13,19}\b/u],
  ['aws-key', /\bAKIA[0-9A-Z]{16}\b/u],
  ['github-token', /\b(?:ghp|gho|ghu|ghs|ghr|github_pat)_[A-Za-z0-9_]{20,}\b/u],
  ['openai-key', /\bsk-(?:live-)?[A-Za-z0-9_-]{20,}\b/u],
  ['jwt', /\beyJ[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\b/u],
  ['private-key', /-----BEGIN (?:RSA |EC |OPENSSH |DSA )?PRIVATE KEY-----/iu],
]);

const sha256 = (value) => createHash('sha256').update(value).digest('hex');

function fail(code, detail = '') {
  throw new Error(detail.length > 0 ? `${code}: ${detail}` : code);
}

function object(value, label) {
  if (value === null || typeof value !== 'object' || Array.isArray(value)) fail('PUBLIC_AGGREGATE_OBJECT_INVALID', label);
  return value;
}

function exactKeys(value, expected, label) {
  const actual = Object.keys(object(value, label)).sort();
  const wanted = [...expected].sort();
  if (JSON.stringify(actual) !== JSON.stringify(wanted)) fail('PUBLIC_AGGREGATE_KEYS_INVALID', label);
}

function string(value, label) {
  if (typeof value !== 'string' || value.length === 0) fail('PUBLIC_AGGREGATE_STRING_INVALID', label);
  return value;
}

function strings(value, label) {
  if (!Array.isArray(value) || value.length === 0 || value.some((item) => typeof item !== 'string' || item.length === 0)) {
    fail('PUBLIC_AGGREGATE_STRING_ARRAY_INVALID', label);
  }
  return [...value];
}

function integers(value, keys, label) {
  exactKeys(value, keys, label);
  const result = {};
  for (const key of keys) {
    if (!Number.isInteger(value[key]) || value[key] < 0) fail('PUBLIC_AGGREGATE_COUNT_INVALID', `${label}.${key}`);
    result[key] = value[key];
  }
  return result;
}

function scan(value, path = '$facts') {
  if (typeof value === 'string') {
    for (const [kind, pattern] of FORBIDDEN_STRINGS) if (pattern.test(value)) fail('PUBLIC_AGGREGATE_PRIVATE_VALUE', `${path}:${kind}`);
    return;
  }
  if (Array.isArray(value)) {
    value.forEach((item, index) => scan(item, `${path}[${index}]`));
    return;
  }
  if (value !== null && typeof value === 'object') {
    for (const [key, item] of Object.entries(value)) {
      if (FORBIDDEN_KEYS.has(key)) fail('PUBLIC_AGGREGATE_PRIVATE_KEY', `${path}.${key}`);
      scan(item, `${path}.${key}`);
    }
  }
}

export function validatePublicAggregateFacts(input) {
  const facts = object(input, '$facts');
  exactKeys(facts, TOP_LEVEL_KEYS, '$facts');
  scan(facts);
  if (facts.schema !== 'apocky.public-conversation-aggregate-facts.v1') fail('PUBLIC_AGGREGATE_SCHEMA_INVALID');
  if (!/^\d{4}-\d{2}-\d{2}$/u.test(facts.generatedAt)) fail('PUBLIC_AGGREGATE_DATE_INVALID');

  exactKeys(facts.publication, PUBLICATION_KEYS, '$facts.publication');
  if (facts.publication.state !== 'aggregate-public-bodies-review-held') fail('PUBLIC_AGGREGATE_STATE_INVALID');
  const publication = {
    state: facts.publication.state,
    authority: string(facts.publication.authority, '$facts.publication.authority'),
    publicIndexScope: string(facts.publication.publicIndexScope, '$facts.publication.publicIndexScope'),
    publicIndexBoundaries: strings(facts.publication.publicIndexBoundaries, '$facts.publication.publicIndexBoundaries'),
    selectionCriteria: {},
    browseScope: string(facts.publication.browseScope, '$facts.publication.browseScope'),
    browseBoundaries: strings(facts.publication.browseBoundaries, '$facts.publication.browseBoundaries'),
  };
  exactKeys(facts.publication.selectionCriteria, SELECTION_KEYS, '$facts.publication.selectionCriteria');
  for (const key of SELECTION_KEYS) publication.selectionCriteria[key] = string(facts.publication.selectionCriteria[key], `$facts.publication.selectionCriteria.${key}`);

  const counts = integers(facts.counts, COUNT_KEYS, '$facts.counts');
  if (counts.uniqueConversations !== counts.chatgptConversations + counts.claudeConversations) fail('PUBLIC_AGGREGATE_PROVIDER_DENOMINATOR_DRIFT');
  if (counts.messages !== counts.userMessages + counts.assistantMessages) fail('PUBLIC_AGGREGATE_MESSAGE_DENOMINATOR_DRIFT');
  if (counts.uniqueConversations !== counts.reviewHeldConversations + counts.rejectedConversations + counts.publiclyApprovedConversations) fail('PUBLIC_AGGREGATE_REVIEW_DENOMINATOR_DRIFT');
  if (counts.publiclyApprovedConversations !== 0 || counts.indexable !== 0 || counts.publishedMessages !== 0) fail('PUBLIC_AGGREGATE_APPROVAL_REQUIRES_PROMOTION_PIPELINE');

  exactKeys(facts.structuralExclusions, Object.keys(STRUCTURAL_KEYS), '$facts.structuralExclusions');
  const structuralExclusions = {};
  for (const provider of Object.keys(STRUCTURAL_KEYS)) structuralExclusions[provider] = integers(facts.structuralExclusions[provider], STRUCTURAL_KEYS[provider], `$facts.structuralExclusions.${provider}`);
  const qualityAudit = integers(facts.qualityAudit, QUALITY_KEYS, '$facts.qualityAudit');
  if (qualityAudit.recordsScored !== counts.uniqueConversations || qualityAudit.reviewHeld !== counts.reviewHeldConversations || qualityAudit.automatedFeatureCandidates !== counts.automatedFeatureCandidates) fail('PUBLIC_AGGREGATE_QUALITY_DENOMINATOR_DRIFT');
  if (qualityAudit.qualityScoreMin > qualityAudit.qualityScoreMax || qualityAudit.qualityScoreMeanMilli < qualityAudit.qualityScoreMin * 1_000 || qualityAudit.qualityScoreMeanMilli > qualityAudit.qualityScoreMax * 1_000) fail('PUBLIC_AGGREGATE_QUALITY_RANGE_INVALID');

  exactKeys(facts.sourceSeal, SOURCE_SEAL_KEYS, '$facts.sourceSeal');
  if (facts.sourceSeal.algorithm !== 'sha256' || facts.sourceSeal.aggregateSha256 !== LEGACY_AGGREGATE_SHA256) fail('PUBLIC_AGGREGATE_SOURCE_SEAL_DRIFT');
  const derivation = string(facts.sourceSeal.derivation, '$facts.sourceSeal.derivation');
  if (!derivation.includes('no conversation body is copied')) fail('PUBLIC_AGGREGATE_DERIVATION_BOUNDARY_INVALID');

  return {
    schema: facts.schema,
    generatedAt: facts.generatedAt,
    publication,
    counts,
    structuralExclusions,
    qualityAudit,
    sourceSeal: { algorithm: 'sha256', aggregateSha256: LEGACY_AGGREGATE_SHA256, derivation },
  };
}

export function buildPublicAggregate(factsInput) {
  const facts = validatePublicAggregateFacts(factsInput);
  const publicIndex = {
    schema: 'apocky.public-conversation-corpus.v1',
    generatedAt: facts.generatedAt,
    publicationState: facts.publication.state,
    publicationAuthority: facts.publication.authority,
    scope: facts.publication.publicIndexScope,
    boundaries: facts.publication.publicIndexBoundaries,
    selectionCriteria: facts.publication.selectionCriteria,
    counts: facts.counts,
    structuralExclusions: facts.structuralExclusions,
    qualityAudit: facts.qualityAudit,
    aggregateSourceSha256: facts.sourceSeal.aggregateSha256,
    aggregateDerivation: facts.sourceSeal.derivation,
    records: [],
  };
  const browse = {
    schema: 'apocky.public-conversation-corpus.browse.v1',
    generatedAt: facts.generatedAt,
    publicationState: facts.publication.state,
    scope: facts.publication.browseScope,
    boundaries: facts.publication.browseBoundaries,
    counts: facts.counts,
    structuralExclusions: facts.structuralExclusions,
    qualityAudit: facts.qualityAudit,
    aggregateSourceSha256: facts.sourceSeal.aggregateSha256,
    aggregateDerivation: facts.sourceSeal.derivation,
    records: [],
  };
  return {
    publicIndex,
    browse,
    publicIndexBytes: `${JSON.stringify(publicIndex, null, 2)}\n`,
    browseBytes: `${JSON.stringify(browse, null, 2)}\n`,
  };
}

export function findPublicAggregateOutputDrift(generated, current) {
  const drift = [];
  if (current.publicIndexBytes !== generated.publicIndexBytes) drift.push('public/conversation-corpus/public-index.v1.json');
  if (current.browseBytes !== generated.browseBytes) drift.push('public/conversation-corpus/browse.v1.json');
  return drift;
}

export async function loadCommittedPublicAggregateFacts() {
  const bytes = await readFile(factsPath);
  if (sha256(bytes) !== PUBLIC_AGGREGATE_FACTS_SHA256) fail('PUBLIC_AGGREGATE_FACTS_SEAL_DRIFT');
  return validatePublicAggregateFacts(JSON.parse(bytes.toString('utf8')));
}

export async function generatePublicAggregate({ check }) {
  const facts = await loadCommittedPublicAggregateFacts();
  const generated = buildPublicAggregate(facts);
  if (sha256(generated.publicIndexBytes) !== PUBLIC_INDEX_SHA256 || sha256(generated.browseBytes) !== BROWSE_SHA256) {
    fail('PUBLIC_AGGREGATE_GENERATED_SEAL_DRIFT');
  }
  const targets = [
    [join(outputRoot, 'public-index.v1.json'), generated.publicIndexBytes],
    [join(outputRoot, 'browse.v1.json'), generated.browseBytes],
  ];
  if (check) {
    const current = {};
    for (const [path] of targets) {
      let actual;
      try { actual = await readFile(path, 'utf8'); } catch { actual = undefined; }
      if (path.endsWith('public-index.v1.json')) current.publicIndexBytes = actual;
      else current.browseBytes = actual;
    }
    const drift = findPublicAggregateOutputDrift(generated, current);
    if (drift.length > 0) fail('PUBLIC_AGGREGATE_OUTPUT_DRIFT', drift.join(', '));
  } else {
    for (const [path, content] of targets) await writeFile(path, content, 'utf8');
  }
  return generated;
}

function parseMode(argv) {
  if (argv.length !== 1 || !['--check', '--write'].includes(argv[0])) fail('PUBLIC_AGGREGATE_MODE_REQUIRED', 'use exactly one of --check or --write');
  return { check: argv[0] === '--check' };
}

if (process.argv[1] !== undefined && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  generatePublicAggregate(parseMode(process.argv.slice(2))).then(({ publicIndexBytes, browseBytes }) => {
    console.log(`public conversation aggregate : ${process.argv[2] === '--check' ? 'CURRENT' : 'WROTE'} · ${Buffer.byteLength(publicIndexBytes) + Buffer.byteLength(browseBytes)} bytes · bodies=0`);
  }).catch((error) => {
    console.error(error instanceof Error ? error.message : String(error));
    process.exitCode = 1;
  });
}
