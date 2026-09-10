export const ENGINE_VERSION = '1.0.0' as const;
export const VOCAB_VERSION = '1.0.0' as const;
export const VOCABULARY_ID = 'apocky.haloic-derived.symbolic-core' as const;
export const SIGIL_GEOMETRY_VERSION = '1.0.0' as const;

export const DEFAULT_SPELL_LIMITS = Object.freeze({
  maxInputChars: 512,
  maxLexicalTokens: 64,
  maxTerms: 24,
  maxMorphemesPerTerm: 6,
  maxAstNodes: 64,
  maxDepth: 4,
});

export interface SpellLimits {
  readonly maxInputChars: number;
  readonly maxLexicalTokens: number;
  readonly maxTerms: number;
  readonly maxMorphemesPerTerm: number;
  readonly maxAstNodes: number;
  readonly maxDepth: number;
}

export type VocabularyNamespace = 'prefix' | 'root' | 'suffix' | 'verb' | 'particle' | 'power';

export type VocabularyOperation =
  | 'intent.cause'
  | 'intent.negate'
  | 'intent.aspire'
  | 'intent.amplify'
  | 'intent.origin'
  | 'modifier.transform'
  | 'modifier.state'
  | 'modifier.agent'
  | 'modifier.collective'
  | 'modifier.abstract'
  | 'modifier.place'
  | 'modifier.continuous'
  | 'flow.if'
  | 'flow.then'
  | 'flow.else'
  | 'action.invoke'
  | 'action.release'
  | 'action.move'
  | 'action.protect'
  | 'action.harmonize'
  | 'action.shield'
  | 'action.open'
  | 'action.banish';

export interface VocabularyEntry {
  readonly id: `${typeof VOCABULARY_ID}:${VocabularyNamespace}:${string}`;
  readonly lexeme: string;
  readonly namespace: VocabularyNamespace;
  readonly meaning: string;
  readonly category: string;
  readonly tags: readonly string[];
  readonly operation?: VocabularyOperation;
}

export type SpellIssueSeverity = 'warning' | 'quarantine' | 'error';

export type SpellIssueCode =
  | 'EMPTY_INPUT'
  | 'INPUT_LIMIT'
  | 'TOKEN_LIMIT'
  | 'TERM_LIMIT'
  | 'MORPHEME_LIMIT'
  | 'AST_NODE_LIMIT'
  | 'DEPTH_LIMIT'
  | 'UNSUPPORTED_CHARACTER'
  | 'INVALID_PUNCTUATION'
  | 'INVALID_CONDITIONAL'
  | 'INVALID_QUALIFIER'
  | 'UNKNOWN_LEXEME'
  | 'AMBIGUOUS_LEXEME'
  | 'INVALID_MORPHOLOGY'
  | 'RESERVED_PARTICLE'
  | 'DISALLOWED_INTENT';

export interface SpellIssue {
  readonly code: SpellIssueCode;
  readonly severity: SpellIssueSeverity;
  readonly message: string;
  readonly tokenIndex?: number;
  readonly lexeme?: string;
}

export type SpellLexicalTokenKind = 'word' | 'hyphen' | 'comma' | 'semicolon' | 'terminal';

export interface SpellLexicalToken {
  readonly kind: SpellLexicalTokenKind;
  readonly raw: string;
  readonly normalized: string;
  readonly start: number;
  readonly end: number;
}

export interface ResolvedMorpheme {
  readonly id: VocabularyEntry['id'];
  readonly lexeme: string;
  readonly namespace: VocabularyNamespace;
  readonly meaning: string;
  readonly category: string;
  readonly tags: readonly string[];
  readonly operation?: VocabularyOperation;
  readonly confidence: number;
  readonly qualified: boolean;
}

export interface SpellTerm {
  readonly kind: 'term';
  readonly source: string;
  readonly canonical: string;
  readonly morphemes: readonly ResolvedMorpheme[];
}

export interface SpellExpression {
  readonly kind: 'expression';
  readonly terms: readonly SpellTerm[];
}

export interface WordSpellProgram {
  readonly kind: 'program';
  readonly form: 'word';
  readonly body: SpellExpression;
}

export interface PhraseSpellProgram {
  readonly kind: 'program';
  readonly form: 'phrase';
  readonly body: SpellExpression;
}

export interface ConditionalSpellProgram {
  readonly kind: 'program';
  readonly form: 'conditional';
  readonly condition: SpellExpression;
  readonly then: SpellExpression;
  readonly else?: SpellExpression;
}

export type SpellProgram = WordSpellProgram | PhraseSpellProgram | ConditionalSpellProgram;

export type AllowedSymbolicOperation =
  | 'symbolic.reference'
  | 'symbolic.modify'
  | 'symbolic.compose'
  | 'symbolic.call'
  | 'symbolic.branch';

export interface CompiledNode {
  readonly id: `node-${number}`;
  readonly operation: AllowedSymbolicOperation;
  readonly inputs: readonly CompiledNode['id'][];
  readonly vocabularyRefs: readonly VocabularyEntry['id'][];
  readonly tags: readonly string[];
  readonly label: string;
}

export interface CompiledSpell {
  readonly kind: 'apocky.symbolic-plan/v1';
  readonly executable: false;
  readonly operations: readonly CompiledNode[];
  readonly output: CompiledNode['id'];
  readonly canonicalProgram: string;
}

export interface InterpretationTraceItem {
  readonly term: string;
  readonly gloss: string;
  readonly vocabularyRefs: readonly VocabularyEntry['id'][];
  readonly confidence: number;
}

export interface SpellInterpretation {
  readonly kind: 'reflective-symbolic-interpretation';
  readonly text: string;
  readonly disclaimer: string;
  readonly trace: readonly InterpretationTraceItem[];
}

export interface EngineProvenance {
  readonly engineVersion: typeof ENGINE_VERSION;
  readonly vocabularyId: typeof VOCABULARY_ID;
  readonly vocabularyVersion: typeof VOCAB_VERSION;
  readonly vocabularyHash: string;
  readonly source: 'independently implemented, owner-authorized Haloic-derived symbolic model';
  readonly efficacy: 'reflective-only; no metaphysical or real-world effect claim';
}

export interface SpellReceipt {
  readonly kind: 'apocky.spellcraft.receipt/v1';
  readonly engineVersion: typeof ENGINE_VERSION;
  readonly vocabularyId: typeof VOCABULARY_ID;
  readonly vocabularyVersion: typeof VOCAB_VERSION;
  readonly vocabularyHashAlgorithm: 'sha256';
  readonly vocabularyHash: string;
  readonly sourceHash: string;
  readonly programHash: string;
  readonly compiledHash: string;
  readonly verdict: 'symbolic-valid';
  readonly authority: 'none';
}

interface SpellAnalysisBase {
  readonly input: string;
  readonly tokens: readonly SpellLexicalToken[];
  readonly issues: readonly SpellIssue[];
  readonly warnings: readonly SpellIssue[];
  readonly confidence: number;
  readonly provenance: EngineProvenance;
}

export interface ValidSpellAnalysis extends SpellAnalysisBase {
  readonly status: 'valid';
  readonly program: SpellProgram;
  readonly compiled: CompiledSpell;
  readonly interpretation: SpellInterpretation;
  readonly receipt: SpellReceipt;
}

export interface QuarantinedSpellAnalysis extends SpellAnalysisBase {
  readonly status: 'quarantined';
  readonly program?: SpellProgram;
}

export interface RejectedSpellAnalysis extends SpellAnalysisBase {
  readonly status: 'rejected';
  readonly program?: SpellProgram;
}

export type SpellAnalysis = ValidSpellAnalysis | QuarantinedSpellAnalysis | RejectedSpellAnalysis;

export interface SigilArtifact {
  readonly kind: 'apocky.symbolic-sigil/v1';
  readonly svg: string;
  readonly seedHash: string;
  readonly geometryVersion: typeof SIGIL_GEOMETRY_VERSION;
  readonly variant: number;
  readonly viewBox: '0 0 512 512';
  readonly title: string;
  readonly semantics: Readonly<{
    disclosure: string;
    path: string;
    rings: string;
    nodes: string;
  }>;
  readonly receipt: Readonly<{
    engineVersion: typeof ENGINE_VERSION;
    vocabularyVersion: typeof VOCAB_VERSION;
    vocabularyHash: string;
    programHash: string;
  }>;
}

export interface SpellbookEntry {
  readonly kind: 'apocky.spellbook.entry/v1';
  readonly entryId: `spell-${string}`;
  readonly label: string;
  readonly savedAt?: string;
  readonly input: string;
  readonly program: SpellProgram;
  readonly compiled: CompiledSpell;
  readonly interpretation: SpellInterpretation;
  readonly sigilSeedHash: string;
  readonly receipt: SpellReceipt;
  readonly contentHash: string;
}

export interface SpellbookPayload {
  readonly kind: 'apocky.spellbook.local-export/v1';
  readonly storage: 'caller-controlled-local-first';
  readonly entries: readonly SpellbookEntry[];
}

export interface StorageValidationResult {
  readonly valid: boolean;
  readonly issues: readonly string[];
  readonly payload?: SpellbookPayload;
}
