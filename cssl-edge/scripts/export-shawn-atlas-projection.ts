import { createHash } from 'node:crypto';
import { mkdirSync, renameSync, writeFileSync } from 'node:fs';
import { basename, dirname, resolve } from 'node:path';

import { atlasData } from '../lib/shawn/atlas';
import { referenceCatalog } from '../lib/shawn/catalog';

const SCHEMA_VERSION = 'shawn-atlas-public-projection.v0.1';
const AUTHORITY = 'candidate-projection-only';

type Identified = Readonly<{ id: string }>;

function requireUniqueIds(label: string, records: readonly Identified[]): void {
  const ids = records.map((record) => record.id);
  const duplicates = [...new Set(ids.filter((id, index) => ids.indexOf(id) !== index))];
  if (duplicates.length > 0) {
    throw new Error(`${label} contains duplicate ids: ${duplicates.join(', ')}`);
  }
}

function requireKnown(label: string, ids: readonly string[], known: ReadonlySet<string>): void {
  const unknown = [...new Set(ids.filter((id) => !known.has(id)))];
  if (unknown.length > 0) throw new Error(`${label} references unknown ids: ${unknown.join(', ')}`);
}

function allSourceIds(): readonly string[] {
  return [
    ...atlasData.voiceFragments.flatMap((record) => [record.sourceId]),
    ...atlasData.claims.flatMap((record) => [
      ...record.sourceIds,
      ...(record.counterevidenceSourceIds ?? []),
    ]),
    ...atlasData.chronology.flatMap((record) => record.sourceIds),
    ...atlasData.reasoningChains.flatMap((record) => record.sourceIds),
    ...atlasData.variables.flatMap((record) => record.sourceIds),
    ...atlasData.episodes.flatMap((record) => record.sourceIds),
    ...atlasData.artifacts.flatMap((record) => record.sourceIds),
    ...atlasData.artifactLineage.flatMap((record) => record.sourceIds),
    ...atlasData.bridges.flatMap((record) => record.sourceIds),
  ];
}

function allClaimIds(): readonly string[] {
  return [
    ...atlasData.citations.flatMap((record) => record.claimIds),
    ...atlasData.chronology.flatMap((record) => record.claimIds),
    ...atlasData.reasoningChains.flatMap((record) => record.claimIds),
    ...atlasData.episodes.flatMap((record) => record.claimIds),
    ...atlasData.artifacts.flatMap((record) => record.claimIds),
    ...atlasData.claims.flatMap((record) => record.supersedes),
  ];
}

function allTopicSlugs(): readonly string[] {
  return [
    ...atlasData.claims.flatMap((record) => record.topicSlugs),
    ...atlasData.chronology.flatMap((record) => record.topicSlugs),
    ...atlasData.reasoningChains.flatMap((record) =>
      record.steps.flatMap((step) => step.topicSlugs),
    ),
    ...atlasData.episodes.flatMap((record) => record.topicSlugs),
    ...atlasData.artifacts.flatMap((record) => record.topicSlugs),
    ...atlasData.bridges.flatMap((record) => record.topicSlugs),
    ...atlasData.lenses.flatMap((record) => record.topicSlugs),
  ];
}

function validateProjection(): void {
  if (atlasData.status !== 'candidate') {
    throw new Error(`Exporter only accepts the candidate atlas; received ${atlasData.status}`);
  }

  requireUniqueIds('sourceRefs', atlasData.sourceRefs);
  requireUniqueIds('voiceFragments', atlasData.voiceFragments);
  requireUniqueIds('claims', atlasData.claims);
  requireUniqueIds('citations', atlasData.citations);
  requireUniqueIds('chronology', atlasData.chronology);
  requireUniqueIds('reasoningChains', atlasData.reasoningChains);
  requireUniqueIds('variables', atlasData.variables);
  requireUniqueIds('episodes', atlasData.episodes);
  requireUniqueIds('artifacts', atlasData.artifacts);
  requireUniqueIds('artifactLineage', atlasData.artifactLineage);
  requireUniqueIds('bridges', atlasData.bridges);
  requireUniqueIds('lenses', atlasData.lenses);

  const referenceSlugs = referenceCatalog.map((record) => record.slug);
  const duplicateReferences = [...new Set(referenceSlugs.filter(
    (slug, index) => referenceSlugs.indexOf(slug) !== index,
  ))];
  if (duplicateReferences.length > 0) {
    throw new Error(`referenceCatalog contains duplicate slugs: ${duplicateReferences.join(', ')}`);
  }

  const sourceIds = new Set(atlasData.sourceRefs.map((record) => record.id));
  const claimIds = new Set(atlasData.claims.map((record) => record.id));
  const variableIds = new Set(atlasData.variables.map((record) => record.id));
  const artifactIds = new Set(atlasData.artifacts.map((record) => record.id));
  const citationIds = new Set(atlasData.citations.map((record) => record.id));
  const topics = new Set(atlasData.topicSlugs);
  const references = new Set(referenceSlugs);

  requireKnown('source linkage', allSourceIds(), sourceIds);
  requireKnown('claim linkage', allClaimIds(), claimIds);
  requireKnown(
    'citation linkage',
    atlasData.claims.flatMap((record) => [
      ...record.supportingCitationIds,
      ...record.contradictingCitationIds,
    ]),
    citationIds,
  );
  requireKnown(
    'episode variable linkage',
    atlasData.episodes.flatMap((record) => record.variableIds),
    variableIds,
  );
  requireKnown(
    'artifact lineage linkage',
    atlasData.artifactLineage.flatMap((record) => [record.fromArtifactId, record.toArtifactId]),
    artifactIds,
  );
  requireKnown('topic linkage', allTopicSlugs(), topics);
  requireKnown(
    'citation reference linkage',
    atlasData.citations.map((record) => record.referenceSlug),
    references,
  );

  const missingTopicRecords = atlasData.topicSlugs.filter((slug) => !references.has(slug));
  const extraTopicRecords = referenceSlugs.filter((slug) => !topics.has(slug));
  if (missingTopicRecords.length > 0 || extraTopicRecords.length > 0) {
    throw new Error(
      `topic/reference denominator mismatch: missing=${missingTopicRecords.join(',') || 'none'}; `
      + `extra=${extraTopicRecords.join(',') || 'none'}`,
    );
  }

  const disallowedSources = atlasData.sourceRefs.filter(
    (record) => record.privacy !== 'public' || !record.publicationApproved,
  );
  if (disallowedSources.length > 0) {
    throw new Error(
      `public projection contains non-public or unapproved source refs: ${disallowedSources.map((record) => record.id).join(', ')}`,
    );
  }
  if (referenceCatalog.some((record) => record.privacy !== 'public')) {
    throw new Error('public projection contains a non-public reference record');
  }
}

function findLocalPath(value: unknown, pointer = '$'): string | undefined {
  if (typeof value === 'string') {
    const match = /(?:^|[\s"'(])(?:[A-Za-z]:[\\/]|\\\\[^\\])/u.exec(value);
    return match ? `${pointer}: ${match[0].trim()}` : undefined;
  }
  if (Array.isArray(value)) {
    for (let index = 0; index < value.length; index += 1) {
      const found = findLocalPath(value[index], `${pointer}[${index}]`);
      if (found) return found;
    }
    return undefined;
  }
  if (value && typeof value === 'object') {
    for (const [key, nested] of Object.entries(value)) {
      const found = findLocalPath(nested, `${pointer}.${key}`);
      if (found) return found;
    }
  }
  return undefined;
}

validateProjection();

const collections = {
  sourceRefs: atlasData.sourceRefs.length,
  voiceFragments: atlasData.voiceFragments.length,
  claims: atlasData.claims.length,
  citations: atlasData.citations.length,
  chronology: atlasData.chronology.length,
  reasoningChains: atlasData.reasoningChains.length,
  reasoningSteps: atlasData.reasoningChains.reduce((sum, record) => sum + record.steps.length, 0),
  variables: atlasData.variables.length,
  episodes: atlasData.episodes.length,
  artifacts: atlasData.artifacts.length,
  artifactLineage: atlasData.artifactLineage.length,
  bridges: atlasData.bridges.length,
  lenses: atlasData.lenses.length,
  topics: atlasData.topicSlugs.length,
  references: referenceCatalog.length,
} as const;

const projection = {
  schemaVersion: SCHEMA_VERSION,
  atlasVersion: atlasData.version,
  dataUpdatedAt: atlasData.updatedAt,
  status: atlasData.status,
  authority: AUTHORITY,
  privacy: 'public',
  provenance: {
    canonicalSpine: 'SHAWN_APOCKY_MODEL',
    typedSourceModules: ['lib/shawn/atlas.ts', 'lib/shawn/catalog.ts'],
    graphRole: 'lead-and-retrieval-projection',
    truthBoundary: 'Graph connectivity does not ratify, prove, or independently verify any modeled claim.',
  },
  atlas: atlasData,
  references: referenceCatalog,
  coverage: {
    included: collections,
    excluded: [
      {
        surface: 'private-source-registry',
        reason: 'Exact raw locators and hashes remain in the canonical private ContextFrame, outside the public graph.',
      },
      {
        surface: 'clinical-records',
        reason: 'Restricted clinical narrative is never copied into this public projection.',
      },
      {
        surface: 'raw-archives',
        reason: 'The graph contains typed public records and pointers, not duplicated source archives.',
      },
    ],
  },
} as const;

const forbiddenPath = findLocalPath(projection);
if (forbiddenPath) throw new Error(`projection leaked a local path at ${forbiddenPath}`);
const serialized = `${JSON.stringify(projection, null, 2)}\n`;

const outputArg = process.argv[2];
if (!outputArg) {
  throw new Error('Usage: node --import tsx scripts/export-shawn-atlas-projection.ts <output.json>');
}

const outputPath = resolve(outputArg);
mkdirSync(dirname(outputPath), { recursive: true });
const temporaryPath = `${outputPath}.tmp-${process.pid}`;
writeFileSync(temporaryPath, serialized, 'utf8');
renameSync(temporaryPath, outputPath);

const digest = createHash('sha256').update(serialized, 'utf8').digest('hex');
writeFileSync(`${outputPath}.sha256`, `${digest}  ${basename(outputPath)}\n`, 'utf8');

process.stdout.write(`${JSON.stringify({
  outputPath,
  sha256: digest,
  bytes: Buffer.byteLength(serialized, 'utf8'),
  collections,
}, null, 2)}\n`);
