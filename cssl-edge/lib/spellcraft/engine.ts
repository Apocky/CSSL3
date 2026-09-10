import { compileSpell } from './compiler';
import { sha256Hex, stableStringify } from './hash';
import { interpretSpell } from './interpreter';
import { canonicalizeProgram, parseSpell } from './parser';
import {
  ENGINE_VERSION,
  VOCABULARY_ID,
  VOCAB_VERSION,
  type EngineProvenance,
  type QuarantinedSpellAnalysis,
  type RejectedSpellAnalysis,
  type SpellAnalysis,
  type SpellLimits,
  type SpellReceipt,
  type ValidSpellAnalysis,
} from './types';
import { validateSpellProgram } from './validator';
import { VOCABULARY_HASH } from './vocabulary';

export interface AnalyzeSpellOptions {
  readonly limits?: Partial<SpellLimits>;
}

export const ENGINE_PROVENANCE: EngineProvenance = Object.freeze({
  engineVersion: ENGINE_VERSION,
  vocabularyId: VOCABULARY_ID,
  vocabularyVersion: VOCAB_VERSION,
  vocabularyHash: VOCABULARY_HASH,
  source: 'independently implemented, owner-authorized Haloic-derived symbolic model',
  efficacy: 'reflective-only; no metaphysical or real-world effect claim',
});

function rejected(
  parsed: ReturnType<typeof parseSpell>,
  additionalIssues: RejectedSpellAnalysis['issues'] = [],
): RejectedSpellAnalysis {
  return Object.freeze({
    status: 'rejected',
    input: parsed.normalizedInput,
    tokens: parsed.tokens,
    issues: Object.freeze([...parsed.issues, ...additionalIssues]),
    warnings: Object.freeze(parsed.issues.filter((issue) => issue.severity === 'warning')),
    confidence: 0,
    provenance: ENGINE_PROVENANCE,
    ...(parsed.program ? { program: parsed.program } : {}),
  });
}

/** Full fail-closed pipeline: tokenize -> parse -> validate -> compile -> interpret -> receipt. */
export function analyzeSpell(input: string, options: AnalyzeSpellOptions = {}): SpellAnalysis {
  const parsed = parseSpell(input, options.limits);
  if (parsed.issues.some((issue) => issue.severity === 'error')) return rejected(parsed);

  if (parsed.issues.some((issue) => issue.severity === 'quarantine')) {
    const result: QuarantinedSpellAnalysis = Object.freeze({
      status: 'quarantined',
      input: parsed.normalizedInput,
      tokens: parsed.tokens,
      issues: parsed.issues,
      warnings: Object.freeze(parsed.issues.filter((issue) => issue.severity === 'warning')),
      confidence: 0,
      provenance: ENGINE_PROVENANCE,
    });
    return result;
  }

  if (!parsed.program) {
    return rejected(parsed, [{
      code: 'INVALID_MORPHOLOGY',
      severity: 'error',
      message: 'No complete symbolic program could be formed.',
    }]);
  }

  const validation = validateSpellProgram(parsed.program);
  if (!validation.valid) return rejected(parsed, validation.issues);

  const compiled = compileSpell(parsed.program);
  const interpretation = interpretSpell(parsed.program);
  const receipt: SpellReceipt = Object.freeze({
    kind: 'apocky.spellcraft.receipt/v1',
    engineVersion: ENGINE_VERSION,
    vocabularyId: VOCABULARY_ID,
    vocabularyVersion: VOCAB_VERSION,
    vocabularyHashAlgorithm: 'sha256',
    vocabularyHash: VOCABULARY_HASH,
    sourceHash: sha256Hex(parsed.normalizedInput),
    programHash: sha256Hex(canonicalizeProgram(parsed.program)),
    compiledHash: sha256Hex(stableStringify(compiled)),
    verdict: 'symbolic-valid',
    authority: 'none',
  });

  const result: ValidSpellAnalysis = Object.freeze({
    status: 'valid',
    input: parsed.normalizedInput,
    tokens: parsed.tokens,
    program: parsed.program,
    issues: parsed.issues,
    warnings: Object.freeze(parsed.issues.filter((issue) => issue.severity === 'warning')),
    confidence: parsed.confidence,
    provenance: ENGINE_PROVENANCE,
    compiled,
    interpretation,
    receipt,
  });
  return result;
}
