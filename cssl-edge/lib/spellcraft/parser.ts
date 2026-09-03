import {
  DEFAULT_SPELL_LIMITS,
  type ResolvedMorpheme,
  type SpellExpression,
  type SpellIssue,
  type SpellLexicalToken,
  type SpellLimits,
  type SpellProgram,
  type SpellTerm,
} from './types';
import {
  findQualifiedVocabularyEntry,
  findVocabularyEntries,
  isVocabularyNamespace,
} from './vocabulary';

export interface TokenizeSpellResult {
  readonly normalizedInput: string;
  readonly tokens: readonly SpellLexicalToken[];
  readonly issues: readonly SpellIssue[];
}

export interface ParseSpellResult extends TokenizeSpellResult {
  readonly program?: SpellProgram;
  readonly confidence: number;
}

const DISALLOWED_INTENT_PATTERNS = Object.freeze([
  /\b(?:coerce|dominate|enslave|control)\s+(?:another|someone|a person|people)\b/i,
  /\bwithout\s+(?:their\s+)?consent\b/i,
  /\b(?:harm|curse|stalk|spy on)\s+(?:another|someone|a person|people|them)\b/i,
]);

export function resolveSpellLimits(overrides: Partial<SpellLimits> = {}): SpellLimits {
  const bounded = <Key extends keyof SpellLimits>(key: Key): number => {
    const requested = overrides[key];
    if (requested === undefined || !Number.isFinite(requested)) return DEFAULT_SPELL_LIMITS[key];
    return Math.max(1, Math.min(DEFAULT_SPELL_LIMITS[key], Math.floor(requested)));
  };

  return Object.freeze({
    maxInputChars: bounded('maxInputChars'),
    maxLexicalTokens: bounded('maxLexicalTokens'),
    maxTerms: bounded('maxTerms'),
    maxMorphemesPerTerm: bounded('maxMorphemesPerTerm'),
    maxAstNodes: bounded('maxAstNodes'),
    maxDepth: bounded('maxDepth'),
  });
}

function lexicalKind(character: string): SpellLexicalToken['kind'] | undefined {
  if (character === '-') return 'hyphen';
  if (character === ',') return 'comma';
  if (character === ';') return 'semicolon';
  if (character === '.' || character === '!' || character === '?') return 'terminal';
  return undefined;
}

export function tokenizeSpell(input: string, overrides: Partial<SpellLimits> = {}): TokenizeSpellResult {
  const limits = resolveSpellLimits(overrides);
  const normalizedInput = input.normalize('NFC').trim();
  const tokens: SpellLexicalToken[] = [];
  const issues: SpellIssue[] = [];

  if (normalizedInput.length === 0) {
    return {
      normalizedInput,
      tokens,
      issues: [{ code: 'EMPTY_INPUT', severity: 'error', message: 'Enter at least one symbolic term.' }],
    };
  }

  if (normalizedInput.length > limits.maxInputChars) {
    issues.push({
      code: 'INPUT_LIMIT',
      severity: 'error',
      message: `Input exceeds the ${limits.maxInputChars}-character safety limit.`,
    });
    return { normalizedInput, tokens, issues };
  }

  if (DISALLOWED_INTENT_PATTERNS.some((pattern) => pattern.test(normalizedInput))) {
    issues.push({
      code: 'DISALLOWED_INTENT',
      severity: 'error',
      message: 'This reflective tool does not provide coercive, harmful, or non-consensual intent templates.',
    });
    return { normalizedInput, tokens, issues };
  }

  let cursor = 0;
  while (cursor < normalizedInput.length) {
    const character = normalizedInput[cursor] ?? '';
    if (/\s/.test(character)) {
      cursor += 1;
      continue;
    }

    const punctuationKind = lexicalKind(character);
    if (punctuationKind) {
      tokens.push({ kind: punctuationKind, raw: character, normalized: character, start: cursor, end: cursor + 1 });
      cursor += 1;
      continue;
    }

    if (/[A-Za-z]/.test(character)) {
      const start = cursor;
      while (cursor < normalizedInput.length && /[A-Za-z]/.test(normalizedInput[cursor] ?? '')) cursor += 1;
      if (normalizedInput[cursor] === ':') {
        cursor += 1;
        const valueStart = cursor;
        while (cursor < normalizedInput.length && /[A-Za-z]/.test(normalizedInput[cursor] ?? '')) cursor += 1;
        if (cursor === valueStart) {
          issues.push({
            code: 'INVALID_QUALIFIER',
            severity: 'error',
            message: 'A namespace qualifier must be followed by a lexeme, such as root:sol.',
            tokenIndex: tokens.length,
          });
          break;
        }
      }
      const raw = normalizedInput.slice(start, cursor);
      tokens.push({ kind: 'word', raw, normalized: raw.toLowerCase(), start, end: cursor });
      continue;
    }

    const codePoint = normalizedInput.codePointAt(cursor);
    const unsupported = codePoint === undefined ? character : String.fromCodePoint(codePoint);
    issues.push({
      code: 'UNSUPPORTED_CHARACTER',
      severity: 'error',
      message: `Unsupported character at position ${cursor + 1}. Use letters, spaces, hyphens, namespace colons, or basic punctuation.`,
      lexeme: unsupported,
    });
    cursor += unsupported.length || 1;
  }

  if (tokens.length > limits.maxLexicalTokens) {
    issues.push({
      code: 'TOKEN_LIMIT',
      severity: 'error',
      message: `Input exceeds the ${limits.maxLexicalTokens}-token safety limit.`,
    });
  }

  return { normalizedInput, tokens: Object.freeze(tokens), issues: Object.freeze(issues) };
}

function resolveMorpheme(token: SpellLexicalToken, tokenIndex: number, issues: SpellIssue[]): ResolvedMorpheme | undefined {
  const [first, second, ...rest] = token.normalized.split(':');
  if (!first) return undefined;

  if (second !== undefined) {
    if (rest.length > 0 || !second || !isVocabularyNamespace(first)) {
      issues.push({
        code: 'INVALID_QUALIFIER',
        severity: 'error',
        message: `“${token.raw}” is not a valid vocabulary reference. Use namespace:lexeme.`,
        tokenIndex,
        lexeme: token.raw,
      });
      return undefined;
    }
    const match = findQualifiedVocabularyEntry(first, second);
    if (!match) {
      issues.push({
        code: 'UNKNOWN_LEXEME',
        severity: 'quarantine',
        message: `No ${first} entry exists for “${second}”.`,
        tokenIndex,
        lexeme: token.raw,
      });
      return undefined;
    }
    return Object.freeze({ ...match, confidence: 1, qualified: true });
  }

  const matches = findVocabularyEntries(first);
  if (matches.length === 0) {
    issues.push({
      code: 'UNKNOWN_LEXEME',
      severity: 'quarantine',
      message: `“${token.raw}” is not in the active vocabulary and was not compiled.`,
      tokenIndex,
      lexeme: token.raw,
    });
    return undefined;
  }
  if (matches.length > 1) {
    const choices = matches.map((match) => `${match.namespace}:${match.lexeme}`).join(' or ');
    issues.push({
      code: 'AMBIGUOUS_LEXEME',
      severity: 'quarantine',
      message: `“${token.raw}” has multiple meanings. Choose ${choices}.`,
      tokenIndex,
      lexeme: token.raw,
    });
    return undefined;
  }

  const match = matches[0];
  return match ? Object.freeze({ ...match, confidence: 0.98, qualified: false }) : undefined;
}

function validateMorphology(morphemes: readonly ResolvedMorpheme[], source: string, issues: SpellIssue[]): boolean {
  if (morphemes.some((morpheme) => morpheme.namespace === 'particle')) {
    issues.push({
      code: 'RESERVED_PARTICLE',
      severity: 'error',
      message: `Logic particle in “${source}” may only delimit a conditional.`,
      lexeme: source,
    });
    return false;
  }

  const heads = morphemes.filter((morpheme) => ['root', 'verb', 'power'].includes(morpheme.namespace));
  if (heads.length === 0) {
    issues.push({
      code: 'INVALID_MORPHOLOGY',
      severity: 'error',
      message: `“${source}” needs at least one root, verb, or power-word head.`,
      lexeme: source,
    });
    return false;
  }

  let phase: 'prefix' | 'head' | 'suffix' = 'prefix';
  for (const morpheme of morphemes) {
    if (morpheme.namespace === 'prefix') {
      if (phase !== 'prefix') {
        issues.push({
          code: 'INVALID_MORPHOLOGY',
          severity: 'error',
          message: `Prefix “${morpheme.lexeme}” must precede every head in “${source}”.`,
          lexeme: source,
        });
        return false;
      }
      continue;
    }
    if (morpheme.namespace === 'suffix') {
      if (phase === 'prefix') {
        issues.push({
          code: 'INVALID_MORPHOLOGY',
          severity: 'error',
          message: `Suffix “${morpheme.lexeme}” cannot precede the head in “${source}”.`,
          lexeme: source,
        });
        return false;
      }
      phase = 'suffix';
      continue;
    }
    if (phase === 'suffix') {
      issues.push({
        code: 'INVALID_MORPHOLOGY',
        severity: 'error',
        message: `Head “${morpheme.lexeme}” cannot follow a suffix in “${source}”.`,
        lexeme: source,
      });
      return false;
    }
    phase = 'head';
  }
  return true;
}

function parseExpression(
  tokens: readonly SpellLexicalToken[],
  limits: SpellLimits,
  issues: SpellIssue[],
): SpellExpression | undefined {
  const terms: SpellTerm[] = [];
  let cursor = 0;

  while (cursor < tokens.length) {
    const current = tokens[cursor];
    if (!current || current.kind !== 'word') {
      issues.push({
        code: 'INVALID_PUNCTUATION',
        severity: 'error',
        message: `Unexpected “${current?.raw ?? ''}” inside a symbolic expression.`,
        tokenIndex: cursor,
        lexeme: current?.raw,
      });
      return undefined;
    }

    const wordTokens: Array<{ token: SpellLexicalToken; index: number }> = [{ token: current, index: cursor }];
    cursor += 1;
    while (tokens[cursor]?.kind === 'hyphen') {
      const next = tokens[cursor + 1];
      if (!next || next.kind !== 'word') {
        issues.push({
          code: 'INVALID_MORPHOLOGY',
          severity: 'error',
          message: 'Every hyphen must join two vocabulary terms.',
          tokenIndex: cursor,
          lexeme: '-',
        });
        return undefined;
      }
      wordTokens.push({ token: next, index: cursor + 1 });
      cursor += 2;
    }

    if (wordTokens.length > limits.maxMorphemesPerTerm) {
      issues.push({
        code: 'MORPHEME_LIMIT',
        severity: 'error',
        message: `A compound may contain at most ${limits.maxMorphemesPerTerm} morphemes.`,
        tokenIndex: wordTokens[0]?.index,
      });
      return undefined;
    }

    const morphemes = wordTokens
      .map(({ token, index }) => resolveMorpheme(token, index, issues))
      .filter((value): value is ResolvedMorpheme => Boolean(value));
    if (morphemes.length !== wordTokens.length) continue;

    const source = wordTokens.map(({ token }) => token.raw).join('-');
    if (!validateMorphology(morphemes, source, issues)) continue;
    terms.push(Object.freeze({
      kind: 'term',
      source,
      canonical: morphemes.map((morpheme) => morpheme.id).join('+'),
      morphemes: Object.freeze(morphemes),
    }));
  }

  if (terms.length > limits.maxTerms) {
    issues.push({
      code: 'TERM_LIMIT',
      severity: 'error',
      message: `An expression may contain at most ${limits.maxTerms} terms.`,
    });
    return undefined;
  }

  return terms.length > 0 ? Object.freeze({ kind: 'expression', terms: Object.freeze(terms) }) : undefined;
}

function trimExpressionPunctuation(tokens: readonly SpellLexicalToken[]): readonly SpellLexicalToken[] {
  let end = tokens.length;
  while (end > 0 && (tokens[end - 1]?.kind === 'comma' || tokens[end - 1]?.kind === 'semicolon')) end -= 1;
  return tokens.slice(0, end);
}

function isMarker(token: SpellLexicalToken | undefined, values: readonly string[]): boolean {
  return token?.kind === 'word' && !token.normalized.includes(':') && values.includes(token.normalized);
}

function countAstNodes(program: SpellProgram): number {
  const expressions = program.form === 'conditional'
    ? [program.condition, program.then, ...(program.else ? [program.else] : [])]
    : [program.body];
  return 1 + expressions.reduce(
    (total, expression) => total + 1 + expression.terms.reduce((sum, term) => sum + 1 + term.morphemes.length, 0),
    0,
  );
}

export function parseSpell(input: string, overrides: Partial<SpellLimits> = {}): ParseSpellResult {
  const limits = resolveSpellLimits(overrides);
  const tokenized = tokenizeSpell(input, limits);
  const issues = [...tokenized.issues];
  if (issues.some((issue) => issue.severity === 'error')) return { ...tokenized, confidence: 0 };

  let tokens = [...tokenized.tokens];
  const terminalIndices = tokens
    .map((token, index) => ({ token, index }))
    .filter(({ token }) => token.kind === 'terminal')
    .map(({ index }) => index);
  if (terminalIndices.length > 1 || (terminalIndices.length === 1 && terminalIndices[0] !== tokens.length - 1)) {
    issues.push({
      code: 'INVALID_PUNCTUATION',
      severity: 'error',
      message: 'Sentence punctuation is allowed only once, at the end.',
    });
  }
  if (tokens.at(-1)?.kind === 'terminal') tokens = tokens.slice(0, -1);

  let program: SpellProgram | undefined;
  const startsConditional = isMarker(tokens[0], ['ki', 'if']);
  if (startsConditional) {
    if (limits.maxDepth < 3) {
      issues.push({ code: 'DEPTH_LIMIT', severity: 'error', message: 'Conditional form exceeds the configured depth limit.' });
    } else {
      const thenMarkers = tokens
        .map((token, index) => ({ token, index }))
        .filter(({ token }) => isMarker(token, ['ya', 'then']));
      if (thenMarkers.length !== 1) {
        issues.push({
          code: 'INVALID_CONDITIONAL',
          severity: 'error',
          message: 'A conditional needs exactly one then marker: ya or then.',
        });
      } else {
        const thenIndex = thenMarkers[0]?.index ?? -1;
        const elseMarkers = tokens
          .map((token, index) => ({ token, index }))
          .filter(({ token, index }) => index > thenIndex && isMarker(token, ['al', 'else']));
        if (elseMarkers.length > 1) {
          issues.push({ code: 'INVALID_CONDITIONAL', severity: 'error', message: 'A conditional may contain only one else branch.' });
        } else {
          const elseIndex = elseMarkers[0]?.index ?? tokens.length;
          const conditionTokens = trimExpressionPunctuation(tokens.slice(1, thenIndex));
          const thenTokens = trimExpressionPunctuation(tokens.slice(thenIndex + 1, elseIndex));
          const elseTokens = elseIndex < tokens.length ? trimExpressionPunctuation(tokens.slice(elseIndex + 1)) : [];
          const condition = parseExpression(conditionTokens, limits, issues);
          const thenExpression = parseExpression(thenTokens, limits, issues);
          const elseExpression = elseIndex < tokens.length ? parseExpression(elseTokens, limits, issues) : undefined;
          if (!condition || !thenExpression || (elseIndex < tokens.length && !elseExpression)) {
            issues.push({
              code: 'INVALID_CONDITIONAL',
              severity: 'error',
              message: 'Condition, then branch, and any declared else branch must be non-empty and valid.',
            });
          } else {
            program = Object.freeze({
              kind: 'program',
              form: 'conditional',
              condition,
              then: thenExpression,
              ...(elseExpression ? { else: elseExpression } : {}),
            });
          }
        }
      }
    }
  } else {
    const expression = parseExpression(tokens, limits, issues);
    if (expression) {
      program = Object.freeze({
        kind: 'program',
        form: expression.terms.length === 1 ? 'word' : 'phrase',
        body: expression,
      });
    }
  }

  if (program && countAstNodes(program) > limits.maxAstNodes) {
    issues.push({
      code: 'AST_NODE_LIMIT',
      severity: 'error',
      message: `Parsed form exceeds the ${limits.maxAstNodes}-node safety limit.`,
    });
    program = undefined;
  }

  const resolved = program
    ? (program.form === 'conditional'
      ? [program.condition, program.then, ...(program.else ? [program.else] : [])]
      : [program.body]).flatMap((expression) => expression.terms.flatMap((term) => term.morphemes))
    : [];
  const confidence = resolved.length > 0
    ? Number((resolved.reduce((sum, morpheme) => sum + morpheme.confidence, 0) / resolved.length).toFixed(3))
    : 0;

  return {
    ...tokenized,
    issues: Object.freeze(issues),
    ...(program ? { program } : {}),
    confidence,
  };
}

export function canonicalizeProgram(program: SpellProgram): string {
  const expression = (value: SpellExpression): readonly (readonly string[])[] =>
    value.terms.map((term) => term.morphemes.map((morpheme) => morpheme.id));
  if (program.form === 'conditional') {
    return JSON.stringify({
      form: program.form,
      condition: expression(program.condition),
      then: expression(program.then),
      ...(program.else ? { else: expression(program.else) } : {}),
    });
  }
  return JSON.stringify({ form: program.form, body: expression(program.body) });
}
