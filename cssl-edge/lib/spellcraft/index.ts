export {
  analyzeSpell,
  ENGINE_PROVENANCE,
  type AnalyzeSpellOptions,
} from './engine';
export { tokenizeSpell, parseSpell, canonicalizeProgram, resolveSpellLimits } from './parser';
export { validateSpellProgram, type ValidateSpellResult } from './validator';
export { compileSpell, ALLOWED_SYMBOLIC_OPERATIONS } from './compiler';
export { interpretSpell } from './interpreter';
export { createSigil, sigilSeedHashForReceipt, type CreateSigilOptions } from './sigil';
export {
  createSpellbookEntry,
  parseSpellbook,
  serializeSpellbook,
  verifySpellbookEntry,
  type CreateSpellbookEntryOptions,
} from './storage';
export {
  HALOIC_VOCAB,
  VOCABULARY_HASH,
  findQualifiedVocabularyEntry,
  findVocabularyEntries,
  isVocabularyNamespace,
} from './vocabulary';
export { sha256Hex, stableStringify } from './hash';
export {
  DEFAULT_SPELL_LIMITS,
  ENGINE_VERSION,
  SIGIL_GEOMETRY_VERSION,
  VOCABULARY_ID,
  VOCAB_VERSION,
} from './types';
export type {
  AllowedSymbolicOperation,
  CompiledNode,
  CompiledSpell,
  ConditionalSpellProgram,
  EngineProvenance,
  InterpretationTraceItem,
  PhraseSpellProgram,
  QuarantinedSpellAnalysis,
  RejectedSpellAnalysis,
  ResolvedMorpheme,
  SigilArtifact,
  SpellAnalysis,
  SpellExpression,
  SpellIssue,
  SpellIssueCode,
  SpellIssueSeverity,
  SpellLexicalToken,
  SpellLexicalTokenKind,
  SpellLimits,
  SpellProgram,
  SpellReceipt,
  SpellTerm,
  SpellbookEntry,
  SpellbookPayload,
  SpellInterpretation,
  StorageValidationResult,
  ValidSpellAnalysis,
  VocabularyEntry,
  VocabularyNamespace,
  VocabularyOperation,
  WordSpellProgram,
} from './types';
