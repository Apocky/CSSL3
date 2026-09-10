export type CorpusProvider = 'ChatGPT' | 'Claude';
export type CorpusBranch = 'primary' | 'alternate';
export type CorpusEditorialReviewState = 'unreviewed' | 'approved' | 'rejected';

export const CORPUS_REVIEW_HELD_CODE = 'CORPUS_REVIEW_HELD' as const;

export interface CorpusMessage {
  readonly sequence: number;
  readonly role: 'user' | 'assistant';
  readonly branch: CorpusBranch;
  readonly createdAt?: string;
  readonly text: string;
  readonly contentSha256: string;
  readonly sourceBytes: number;
  readonly repeatOf?: { readonly recordId: string; readonly sequence: number };
  readonly rights: {
    readonly rightsStatus: string;
    readonly sourceKind: string;
    readonly sourceAttribution: string;
    readonly reviewState: string;
    readonly reviewReason: string;
  };
  readonly privacy: {
    readonly privacyStatus: string;
    readonly reviewState: string;
    readonly reviewReason: string;
  };
}

export interface CorpusDistillation {
  readonly humanSignal: string;
  readonly aiSignal: string;
  readonly correctionSignal: string;
  readonly questions: readonly string[];
  readonly arc: string;
  readonly evidenceBoundary: string;
}

export interface CorpusLore {
  readonly realm: string;
  readonly artifact: string;
  readonly fragmentTitle: string;
  readonly invocation: string;
  readonly fragment: string;
  readonly reading: string;
  readonly truthStatus: 'original-editorial-allegory';
}

export interface CorpusConnection {
  readonly id: string;
  readonly slug: string;
  readonly title: string;
  readonly provider: CorpusProvider;
  readonly sharedThemes: readonly string[];
  readonly reason: string;
  readonly href: string;
}

export interface CorpusQualityDimensions {
  readonly dialogueCompleteness: number;
  readonly substance: number;
  readonly thematicResonance: number;
  readonly interpretability: number;
  readonly dialogicDepth: number;
  readonly privacyRisk: number;
  readonly duplicatePenalty?: number;
  readonly lowEntropyPenalty?: number;
}

export interface ConversationCorpusSummary {
  readonly id: string;
  readonly slug: string;
  readonly title: string;
  readonly provider: CorpusProvider;
  readonly sourceReference: string;
  readonly sourceFingerprint: string;
  readonly exportFingerprint: string;
  readonly createdAt: string;
  readonly updatedAt: string;
  readonly category: string;
  readonly categoryProvenance: string;
  readonly themes: readonly string[];
  readonly contentWarnings: readonly string[];
  readonly messageCount: number;
  readonly bodyState: 'present' | 'absent-in-export';
  readonly userMessageCount: number;
  readonly assistantMessageCount: number;
  readonly alternateMessageCount: number;
  readonly redactionCount: number;
  readonly rightsHoldCount: number;
  readonly privacyHoldCount: number;
  readonly duplicateMessageCount: number;
  readonly duplicateByteRatio: number;
  readonly lowEntropyMessageCount: number;
  readonly qualityScore: number;
  readonly qualityDimensions: CorpusQualityDimensions;
  readonly selectionReasons: readonly string[];
  readonly automatedFeatureCandidate: boolean;
  readonly featureEligible: boolean;
  readonly editorialReviewState: CorpusEditorialReviewState;
  readonly indexable: boolean;
  readonly excerpt: string;
  readonly humanSignal: string;
  readonly aiSignal: string;
  readonly distillation: CorpusDistillation;
  readonly lore: CorpusLore;
  readonly connections: readonly CorpusConnection[];
  readonly attachmentCount?: number;
  readonly sourceShard?: string;
  readonly projectionSha256: string;
  readonly projectionBytes: number;
  readonly href: string;
  readonly bodyHref: string;
}

export interface ConversationCorpusRecord extends Omit<ConversationCorpusSummary, 'href' | 'bodyHref'> {
  readonly schema: 'apocky.public-conversation-corpus.v1';
  readonly publication: {
    readonly state: 'owner-approved-public-projection';
    readonly approvedAt: string;
    readonly approvalFingerprint: string;
    readonly policy: string;
  };
  readonly messages: readonly CorpusMessage[];
}

export interface ConversationCorpusCounts {
  readonly uniqueConversations: number;
  readonly chatgptConversations: number;
  readonly claudeConversations: number;
  readonly anthropicDuplicateDelivery: number;
  readonly messages: number;
  readonly emptyConversationRecords: number;
  readonly userMessages: number;
  readonly assistantMessages: number;
  readonly alternateBranchMessages: number;
  readonly redactions: number;
  readonly automatedFeatureCandidates: number;
  readonly editoriallyFeatureEligible: number;
  readonly indexable: number;
  readonly publiclyApprovedConversations: number;
  readonly reviewHeldConversations: number;
  readonly rejectedConversations: number;
  readonly publishedMessages: number;
}

export interface ConversationCorpusManifest {
  readonly schema: 'apocky.public-conversation-corpus.v1';
  readonly generatedAt: string;
  readonly publicationState: string;
  readonly publicationAuthority: string;
  readonly scope: string;
  readonly boundaries: readonly string[];
  readonly selectionCriteria: Readonly<Record<string, string>>;
  readonly counts: ConversationCorpusCounts;
  readonly structuralExclusions: Readonly<Record<string, Readonly<Record<string, number>>>>;
  readonly qualityAudit: Readonly<Record<string, number>>;
  readonly aggregateSourceSha256: string;
  readonly aggregateDerivation: string;
  readonly records: readonly ConversationCorpusSummary[];
}

export interface ConversationCorpusBrowseRecord {
  readonly id: string;
  readonly slug: string;
  readonly title: string;
  readonly provider: CorpusProvider;
  readonly createdAt: string;
  readonly category: string;
  readonly themes: readonly string[];
  readonly contentWarningCount: number;
  readonly messageCount: number;
  readonly bodyState: 'present' | 'absent-in-export';
  readonly redactionCount: number;
  readonly qualityScore: number;
  readonly automatedFeatureCandidate: boolean;
  readonly editorialReviewState: 'approved';
  readonly indexable: boolean;
  readonly excerpt: string;
  readonly loreRealm: string;
  readonly loreArtifact: string;
  readonly href: string;
}

export interface ConversationCorpusBrowseManifest {
  readonly schema: 'apocky.public-conversation-corpus.browse.v1';
  readonly generatedAt: string;
  readonly publicationState: string;
  readonly scope: string;
  readonly boundaries: readonly string[];
  readonly counts: ConversationCorpusCounts;
  readonly structuralExclusions: ConversationCorpusManifest['structuralExclusions'];
  readonly qualityAudit: ConversationCorpusManifest['qualityAudit'];
  readonly aggregateSourceSha256: string;
  readonly aggregateDerivation: string;
  readonly records: readonly ConversationCorpusBrowseRecord[];
}

export interface ConversationCorpusPageResponse {
  readonly schema: 'apocky.public-conversation-page.v1';
  readonly record: ConversationCorpusSummary;
  readonly messages: readonly CorpusMessage[];
  readonly page: {
    readonly offset: number;
    readonly limit: number;
    readonly total: number;
    readonly nextOffset: number | null;
  };
}

function duplicateValues(values: readonly string[]): readonly string[] {
  const seen = new Set<string>();
  const repeated = new Set<string>();
  for (const value of values) {
    if (seen.has(value)) repeated.add(value);
    else seen.add(value);
  }
  return [...repeated];
}

export function validatePublicConversationManifest(manifest: ConversationCorpusManifest): void {
  if (manifest.schema !== 'apocky.public-conversation-corpus.v1') throw new Error('CORPUS_MANIFEST_SCHEMA_INVALID');
  if (manifest.publicationState !== 'aggregate-public-bodies-review-held') throw new Error('CORPUS_MANIFEST_PUBLICATION_STATE_INVALID');
  if (manifest.records.length !== manifest.counts.publiclyApprovedConversations) throw new Error('CORPUS_MANIFEST_APPROVED_COUNT_INVALID');
  if (!Number.isInteger(manifest.counts.automatedFeatureCandidates) || manifest.counts.automatedFeatureCandidates < 0) throw new Error('CORPUS_MANIFEST_CANDIDATE_COUNT_INVALID');
  if (Object.keys(manifest.structuralExclusions).length === 0) throw new Error('CORPUS_MANIFEST_STRUCTURAL_COUNTS_MISSING');
  if (Object.keys(manifest.qualityAudit).length === 0 || Object.values(manifest.qualityAudit).some((value) => !Number.isFinite(value))) throw new Error('CORPUS_MANIFEST_QUALITY_COUNTS_MISSING');
  if (duplicateValues(manifest.records.map((record) => record.id)).length > 0) throw new Error('CORPUS_MANIFEST_DUPLICATE_ID');
  if (duplicateValues(manifest.records.map((record) => record.slug)).length > 0) throw new Error('CORPUS_MANIFEST_DUPLICATE_SLUG');
  if (manifest.records.some((record) => (
    record.editorialReviewState !== 'approved'
    || !record.bodyHref.startsWith('/conversation-corpus/approved-records/')
  ))) throw new Error('CORPUS_MANIFEST_UNAPPROVED_RECORD');
}

export function validatePublicConversationBrowseManifest(manifest: ConversationCorpusBrowseManifest): void {
  if (manifest.schema !== 'apocky.public-conversation-corpus.browse.v1') throw new Error('CORPUS_BROWSE_SCHEMA_INVALID');
  if (manifest.publicationState !== 'aggregate-public-bodies-review-held') throw new Error('CORPUS_BROWSE_PUBLICATION_STATE_INVALID');
  if (manifest.records.length !== manifest.counts.publiclyApprovedConversations) throw new Error('CORPUS_BROWSE_APPROVED_COUNT_INVALID');
  if (JSON.stringify(manifest.structuralExclusions).length <= 2) throw new Error('CORPUS_BROWSE_STRUCTURAL_COUNTS_MISSING');
  if (Object.keys(manifest.qualityAudit).length === 0 || Object.values(manifest.qualityAudit).some((value) => !Number.isFinite(value))) throw new Error('CORPUS_BROWSE_QUALITY_COUNTS_MISSING');
  if (duplicateValues(manifest.records.map((record) => record.id)).length > 0) throw new Error('CORPUS_BROWSE_DUPLICATE_ID');
  if (duplicateValues(manifest.records.map((record) => record.slug)).length > 0) throw new Error('CORPUS_BROWSE_DUPLICATE_SLUG');
  if (manifest.records.some((record) => record.editorialReviewState !== 'approved')) throw new Error('CORPUS_BROWSE_UNAPPROVED_RECORD');
}
