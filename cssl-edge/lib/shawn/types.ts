export type EvidenceMode =
  | 'formal'
  | 'computational'
  | 'empirical'
  | 'textual'
  | 'phenomenological'
  | 'philosophical'
  | 'interpretive'
  | 'normative';

export type CitationRelation =
  | 'proves'
  | 'verifies'
  | 'supports'
  | 'refutes'
  | 'defines'
  | 'attests'
  | 'contextualizes'
  | 'motivates'
  | 'analogizes';

export type ReferenceRole = 'R0' | 'R1' | 'R2' | 'R3' | 'R4';
export type EvidenceLane =
  | 'observed'
  | 'self-reported'
  | 'inferred'
  | 'proposed'
  | 'disputed'
  | 'unknown';
export type RelationshipClass =
  | 'IDENTITY'
  | 'FORMAL_HOMOLOGY'
  | 'STRUCTURAL_ANALOGY'
  | 'METAPHOR'
  | 'FICTIONAL_MODEL'
  | 'OPEN_HYPOTHESIS';
export type QuantumLane = 'QL0' | 'QL1' | 'QL2';
export type TruthState = 'TRUE' | 'OPEN' | 'FALSE';
export type Confidence = 'high' | 'medium' | 'low' | 'unknown';
export type ClaimKind =
  | 'formal'
  | 'physical-mechanism'
  | 'computational'
  | 'empirical'
  | 'phenomenological'
  | 'textual'
  | 'philosophical'
  | 'interpretive'
  | 'normative'
  | 'artifact-observation';
export type PrivacyClass = 'public' | 'personal' | 'private' | 'restricted';
export type AuthorClass =
  | 'shawn'
  | 'assistant'
  | 'coauthored'
  | 'third-party'
  | 'system'
  | 'mixed'
  | 'unknown';

export interface SourceRef {
  readonly id: string;
  readonly label: string;
  readonly sourceKind:
    | 'authored-text'
    | 'coauthored-artifact'
    | 'conversation'
    | 'repository-artifact'
    | 'canonical-model'
    | 'current-directive';
  readonly authorClass: AuthorClass;
  readonly privacy: PrivacyClass;
  readonly evidenceLane: EvidenceLane;
  readonly locator: string;
  readonly recordedAt?: string;
  readonly contentHash?: string;
  readonly fullRead: boolean;
  readonly publicationApproved: boolean;
  readonly limitations: readonly string[];
}

export interface VoiceFragment {
  readonly id: string;
  readonly text: string;
  readonly sourceId: string;
  readonly status: 'exact-approved-directive' | 'public-safe-paraphrase';
  readonly analysis: string;
  readonly boundary: string;
}

export interface ClaimRecord {
  readonly id: string;
  readonly title: string;
  readonly wording: string;
  readonly kind: ClaimKind;
  readonly quantumLane?: QuantumLane;
  readonly lane: EvidenceLane;
  readonly truthState: TruthState;
  readonly confidence: Confidence;
  readonly consequential: boolean;
  readonly sourceIds: readonly string[];
  readonly counterevidenceSourceIds?: readonly string[];
  readonly supportingCitationIds: readonly string[];
  readonly contradictingCitationIds: readonly string[];
  readonly countercase: string;
  readonly falsifier: string;
  readonly supersedes: readonly string[];
  readonly topicSlugs: readonly string[];
}

export type VariableRole =
  | 'manipulated'
  | 'held-constant'
  | 'measured'
  | 'covariate'
  | 'suspected-confound'
  | 'unmeasured'
  | 'unknown';

export interface VariableRecord {
  readonly id: string;
  readonly label: string;
  readonly role: VariableRole;
  readonly description: string;
  readonly certainty: Confidence;
  readonly sourceIds: readonly string[];
}

export interface EpisodeRecord {
  readonly id: string;
  readonly title: string;
  readonly period: string;
  readonly summary: string;
  readonly context: string;
  readonly variableIds: readonly string[];
  readonly observation: string;
  readonly interpretation: string;
  readonly rivalExplanations: readonly string[];
  readonly method: string;
  readonly result: string;
  readonly externalCheck: string;
  readonly counterevidence: readonly string[];
  readonly truthState: TruthState;
  readonly claimIds: readonly string[];
  readonly topicSlugs: readonly string[];
  readonly sourceIds: readonly string[];
  readonly privacy: 'public';
}

export interface BridgeRecord {
  readonly id: string;
  readonly from: string;
  readonly to: string;
  readonly relationship: RelationshipClass;
  readonly statement: string;
  readonly invariant: string;
  readonly differences: readonly string[];
  readonly lane: EvidenceLane;
  readonly truthState: TruthState;
  readonly prediction: string;
  readonly negativeTransferTest: string;
  readonly sourceIds: readonly string[];
  readonly topicSlugs: readonly string[];
  readonly quantumLane?: QuantumLane;
}

export interface CitationRecord {
  readonly id: string;
  readonly referenceSlug: string;
  readonly relation: CitationRelation;
  readonly claimIds: readonly string[];
  readonly supports: string;
  readonly doesNotSupport: string;
  readonly locator?: string;
}

export interface ReferenceIdentifier {
  readonly scheme:
    | 'DOI'
    | 'arXiv'
    | 'PMID'
    | 'ISBN'
    | 'RFC'
    | 'W3C'
    | 'SWHID'
    | 'standard'
    | 'catalog';
  readonly value: string;
}

export interface ReferenceBacklink {
  readonly kind: 'claim' | 'episode' | 'bridge' | 'artifact' | 'chronology';
  readonly id: string;
  readonly label: string;
}

export interface EvidenceAccount {
  readonly label:
    | 'Proof'
    | 'Formal treatment'
    | 'Computational verification'
    | 'Computational method'
    | 'Empirical evidence'
    | 'Primary-text attestation'
    | 'Philosophical argument'
    | 'Interpretive lineage'
    | 'Normative framework';
  readonly summary: string;
  readonly steps: readonly string[];
}

export interface ReferenceRecord {
  readonly slug: string;
  readonly aliases: readonly string[];
  readonly title: string;
  readonly domain:
    | 'mathematics'
    | 'physics'
    | 'geometry-topology'
    | 'computation-cognition'
    | 'modeling-methods'
    | 'games-simulation'
    | 'spirituality-esotericism'
    | 'philosophy-epistemology'
    | 'psychology-inquiry'
    | 'language-symbolism'
    | 'myth-theology-fiction';
  readonly creators: readonly string[];
  readonly edition: string;
  readonly version: string;
  readonly translation?: string;
  readonly date: string;
  readonly publisher: string;
  readonly language: string;
  readonly exactLocator: string;
  readonly identifiers: readonly ReferenceIdentifier[];
  readonly urls: {
    readonly canonical: string;
    readonly openAccess?: string;
    readonly archive?: string;
  };
  readonly accessed: string;
  readonly lastVerified: string;
  readonly license?: string;
  readonly contentHash?: string;
  readonly fullRead: boolean;
  readonly displayCitation: string;
  readonly evidenceMode: EvidenceMode;
  readonly role: ReferenceRole;
  readonly authorityScope: string;
  readonly limitations: readonly string[];
  readonly privacy: 'public';
  readonly orientation: string;
  readonly prerequisites: readonly string[];
  readonly technical: string;
  readonly mathExpressions: readonly {
    readonly tex: string;
    readonly label: string;
  }[];
  readonly evidence: EvidenceAccount;
  readonly shawnUse: string;
  readonly supports: readonly string[];
  readonly doesNotSupport: readonly string[];
  readonly counterpositions: readonly string[];
  readonly revisionConditions: readonly string[];
  readonly citationIds: readonly string[];
  readonly backlinks: readonly ReferenceBacklink[];
}

export interface ChronologyEvent {
  readonly id: string;
  readonly period: string;
  readonly precision: 'day' | 'month' | 'year' | 'range' | 'unknown';
  readonly title: string;
  readonly track: 'life-context' | 'state-phenomenology' | 'intellectual-artifact';
  readonly summary: string;
  readonly lane: EvidenceLane;
  readonly truthState: TruthState;
  readonly sourceIds: readonly string[];
  readonly claimIds: readonly string[];
  readonly topicSlugs: readonly string[];
}

export interface ReasoningStep {
  readonly id: string;
  readonly label: string;
  readonly description: string;
  readonly lane: EvidenceLane;
  readonly topicSlugs: readonly string[];
}

export interface ReasoningChain {
  readonly id: string;
  readonly title: string;
  readonly summary: string;
  readonly steps: readonly ReasoningStep[];
  readonly sourceIds: readonly string[];
  readonly claimIds: readonly string[];
}

export interface ArtifactCase {
  readonly id: string;
  readonly title: string;
  readonly kind: 'paper' | 'document' | 'software' | 'formal-language' | 'game' | 'novel' | 'symbolic-system';
  readonly status: 'observed' | 'working' | 'proposed' | 'fictional';
  readonly period: string;
  readonly thesis: string;
  readonly method: readonly string[];
  readonly evidence: readonly string[];
  readonly negativeResults: readonly string[];
  readonly openQuestions: readonly string[];
  readonly collaborationNote: string;
  readonly claimIds: readonly string[];
  readonly topicSlugs: readonly string[];
  readonly sourceIds: readonly string[];
}

export interface ArtifactLineageEdge {
  readonly id: string;
  readonly fromArtifactId: string;
  readonly toArtifactId: string;
  readonly relation: 'questions' | 'formalizes' | 'implements' | 'tests' | 'constrains' | 'supersedes' | 'projects';
  readonly description: string;
  readonly lane: EvidenceLane;
  readonly sourceIds: readonly string[];
}

export interface Lens {
  readonly id: string;
  readonly label: string;
  readonly question: string;
  readonly strength: string;
  readonly limitation: string;
  readonly topicSlugs: readonly string[];
}

export interface AtlasData {
  readonly version: string;
  readonly updatedAt: string;
  readonly status: 'candidate' | 'ratified';
  readonly thesis: string;
  readonly interpretiveContract: readonly string[];
  readonly sourceRefs: readonly SourceRef[];
  readonly voiceFragments: readonly VoiceFragment[];
  readonly claims: readonly ClaimRecord[];
  readonly citations: readonly CitationRecord[];
  readonly chronology: readonly ChronologyEvent[];
  readonly reasoningChains: readonly ReasoningChain[];
  readonly episodes: readonly EpisodeRecord[];
  readonly variables: readonly VariableRecord[];
  readonly artifacts: readonly ArtifactCase[];
  readonly artifactLineage: readonly ArtifactLineageEdge[];
  readonly bridges: readonly BridgeRecord[];
  readonly lenses: readonly Lens[];
  readonly topicSlugs: readonly string[];
}
