import {
  DEFAULT_SPELL_LIMITS,
  type ResolvedMorpheme,
  type SpellExpression,
  type SpellIssue,
  type SpellLimits,
  type SpellProgram,
} from './types';
import { findQualifiedVocabularyEntry } from './vocabulary';

export interface ValidateSpellResult {
  readonly valid: boolean;
  readonly issues: readonly SpellIssue[];
  readonly nodeCount: number;
  readonly depth: number;
}

function hasOnlyKeys(value: object, allowed: readonly string[]): boolean {
  const allowedSet = new Set(allowed);
  return Object.keys(value).every((key) => allowedSet.has(key));
}

function expressionsFor(program: SpellProgram): readonly SpellExpression[] {
  return program.form === 'conditional'
    ? [program.condition, program.then, ...(program.else ? [program.else] : [])]
    : [program.body];
}

function morphologyValid(morphemes: readonly ResolvedMorpheme[]): boolean {
  let phase: 'prefix' | 'head' | 'suffix' = 'prefix';
  let headCount = 0;
  for (const morpheme of morphemes) {
    if (morpheme.namespace === 'particle') return false;
    if (morpheme.namespace === 'prefix') {
      if (phase !== 'prefix') return false;
      continue;
    }
    if (morpheme.namespace === 'suffix') {
      if (phase === 'prefix') return false;
      phase = 'suffix';
      continue;
    }
    if (phase === 'suffix') return false;
    phase = 'head';
    headCount += 1;
  }
  return headCount > 0;
}

/** Revalidates a typed AST so imported or forged objects cannot bypass parser gates. */
export function validateSpellProgram(
  program: SpellProgram,
  limits: SpellLimits = DEFAULT_SPELL_LIMITS,
): ValidateSpellResult {
  const issues: SpellIssue[] = [];
  const expressions = expressionsFor(program);
  const depth = program.form === 'conditional' ? 3 : 2;
  let nodeCount = 1;
  let termCount = 0;

  const programKeys = program.form === 'conditional'
    ? ['kind', 'form', 'condition', 'then', 'else']
    : ['kind', 'form', 'body'];
  if (program.kind !== 'program' || !hasOnlyKeys(program, programKeys)) {
    issues.push({ code: 'INVALID_MORPHOLOGY', severity: 'error', message: 'Program contains undeclared structural fields.' });
  }

  if (depth > limits.maxDepth) {
    issues.push({ code: 'DEPTH_LIMIT', severity: 'error', message: 'Program exceeds the configured AST depth limit.' });
  }
  if (expressions.some((expression) => expression.terms.length === 0)) {
    issues.push({ code: 'INVALID_MORPHOLOGY', severity: 'error', message: 'Every expression must contain at least one term.' });
  }

  for (const expression of expressions) {
    if (expression.kind !== 'expression' || !hasOnlyKeys(expression, ['kind', 'terms'])) {
      issues.push({ code: 'INVALID_MORPHOLOGY', severity: 'error', message: 'Expression contains undeclared structural fields.' });
    }
    nodeCount += 1;
    termCount += expression.terms.length;
    for (const term of expression.terms) {
      if (term.kind !== 'term' || !hasOnlyKeys(term, ['kind', 'source', 'canonical', 'morphemes'])) {
        issues.push({ code: 'INVALID_MORPHOLOGY', severity: 'error', message: 'Term contains undeclared structural fields.' });
      }
      nodeCount += 1 + term.morphemes.length;
      if (term.morphemes.length === 0 || term.morphemes.length > limits.maxMorphemesPerTerm) {
        issues.push({
          code: 'MORPHEME_LIMIT',
          severity: 'error',
          message: `Term “${term.source}” has an invalid morpheme count.`,
          lexeme: term.source,
        });
      }
      if (!morphologyValid(term.morphemes)) {
        issues.push({
          code: 'INVALID_MORPHOLOGY',
          severity: 'error',
          message: `Term “${term.source}” does not follow prefix-head-suffix order.`,
          lexeme: term.source,
        });
      }
      const canonical = term.morphemes.map((morpheme) => morpheme.id).join('+');
      if (term.canonical !== canonical) {
        issues.push({
          code: 'INVALID_MORPHOLOGY',
          severity: 'error',
          message: `Term “${term.source}” has a non-canonical identity.`,
          lexeme: term.source,
        });
      }
      for (const morpheme of term.morphemes) {
        if (!hasOnlyKeys(morpheme, [
          'id', 'lexeme', 'namespace', 'meaning', 'category', 'tags', 'operation', 'confidence', 'qualified',
        ])) {
          issues.push({
            code: 'UNKNOWN_LEXEME',
            severity: 'error',
            message: `Vocabulary data for “${morpheme.lexeme}” contains undeclared fields.`,
            lexeme: morpheme.lexeme,
          });
        }
        const canonicalEntry = findQualifiedVocabularyEntry(morpheme.namespace, morpheme.lexeme);
        if (!canonicalEntry || canonicalEntry.id !== morpheme.id) {
          issues.push({
            code: 'UNKNOWN_LEXEME',
            severity: 'error',
            message: `Term “${term.source}” references vocabulary outside the active sealed set.`,
            lexeme: morpheme.lexeme,
          });
          continue;
        }
        if (
          canonicalEntry.meaning !== morpheme.meaning
          || canonicalEntry.category !== morpheme.category
          || canonicalEntry.operation !== morpheme.operation
          || canonicalEntry.tags.join('|') !== morpheme.tags.join('|')
        ) {
          issues.push({
            code: 'UNKNOWN_LEXEME',
            severity: 'error',
            message: `Vocabulary data for “${morpheme.lexeme}” does not match the sealed version.`,
            lexeme: morpheme.lexeme,
          });
        }
      }
    }
  }

  if (termCount > limits.maxTerms) {
    issues.push({ code: 'TERM_LIMIT', severity: 'error', message: 'Program exceeds the configured term limit.' });
  }
  if (nodeCount > limits.maxAstNodes) {
    issues.push({ code: 'AST_NODE_LIMIT', severity: 'error', message: 'Program exceeds the configured AST node limit.' });
  }

  return Object.freeze({
    valid: issues.length === 0,
    issues: Object.freeze(issues),
    nodeCount,
    depth,
  });
}
