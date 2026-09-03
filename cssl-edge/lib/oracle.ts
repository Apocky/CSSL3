export const ORACLE_VERSION = 'apocky-oracle/1.0.0';
export const ORACLE_MAX_QUESTION_LENGTH = 280;

export type OracleSignal = 'yes' | 'no';

export interface OracleReading {
  readonly version: typeof ORACLE_VERSION;
  readonly question: string;
  readonly seed: string;
  readonly signal: OracleSignal;
  readonly clarity: number;
  readonly counterweight: string;
  readonly nextQuestion: string;
  readonly receipt: string;
}

const COUNTERWEIGHTS = {
  yes: [
    'Name the smallest reversible first move.',
    'Decide what evidence would make you stop.',
    'Keep one boundary intact while you begin.',
    'Tell one trusted person what you are testing.',
  ],
  no: [
    'Ask what condition would need to change.',
    'Separate “not now” from “not ever.”',
    'Find the need hiding beneath the proposed move.',
    'Try the smallest safe alternative instead.',
  ],
} as const;

const NEXT_QUESTIONS = [
  'What am I hoping this answer gives me permission to do?',
  'What observable result would change my mind?',
  'Which part of this choice is actually mine to make?',
  'What happens if I wait one day?',
  'What is the least costly way to learn more?',
  'What boundary must remain non-negotiable?',
] as const;

const HIGH_STAKES_PATTERNS = [
  /\b(?:medical|medicine|medication|meds|diagnos\w*|dose|insulin|prescription|treatment|surgery|doctor|hospital|symptom\w*|pregnan\w*)\b/i,
  /\b(?:legal|lawsuit|court|sue|plead|guilty|lawyer|attorney|police|arrest\w*|contract|custody)\b/i,
  /\b(?:invest\w*|trading|stock|crypto|life savings|retirement (?:fund|money|savings)|mortgage|bankrupt\w*|wire (?:money|funds)|take out (?:a )?loan)\b/i,
  /\b(?:suicid\w*|self[- ]?harm|emergency|overdose|kill|hurt|attack|threaten|weapon|gun|stalk|spy|surveil\w*)\b/i,
  /\b(?:driv(?:e|ing)|operate (?:a )?vehicle).{0,32}\b(?:drink\w*|drunk|alcohol|high|intoxicated)\b/i,
  /\b(?:control|coerce|force|manipulate|deceive|blackmail) (?:them|him|her|someone|a person)\b/i,
] as const;

export function normalizeOracleQuestion(question: string): string {
  return question.trim().replace(/\s+/g, ' ');
}

export function createOracleSeed(): string {
  if (typeof globalThis.crypto?.getRandomValues === 'function') {
    const bytes = new Uint32Array(4);
    globalThis.crypto.getRandomValues(bytes);
    return [...bytes].map((part) => part.toString(36).padStart(7, '0')).join('-');
  }
  throw new Error('APX-ORACLE-CRYPTO-UNAVAILABLE');
}

export function isHighStakesOracleQuestion(question: string): boolean {
  const normalized = normalizeOracleQuestion(question);
  return HIGH_STAKES_PATTERNS.some((pattern) => pattern.test(normalized));
}

export function drawOracle(question: string, seed: string): OracleReading {
  const normalized = normalizeOracleQuestion(question);
  if (!normalized) throw new Error('APX-ORACLE-QUESTION-REQUIRED');
  if (normalized.length > ORACLE_MAX_QUESTION_LENGTH) throw new Error('APX-ORACLE-QUESTION-TOO-LONG');
  if (!seed.trim()) throw new Error('APX-ORACLE-SEED-REQUIRED');
  if (isHighStakesOracleQuestion(normalized)) throw new Error('APX-ORACLE-HIGH-STAKES-BLOCKED');

  const digest = sha256Hex(`${ORACLE_VERSION}\u241f${normalized.toLocaleLowerCase()}\u241f${seed.trim()}`);
  const byte = (offset: number): number => Number.parseInt(digest.slice(offset, offset + 2), 16);
  const signal: OracleSignal = (byte(0) & 1) === 0 ? 'yes' : 'no';
  const clarity = 55 + (byte(2) % 41);
  const counterweights = COUNTERWEIGHTS[signal];
  const counterweight = counterweights[byte(4) % counterweights.length]!;
  const nextQuestion = NEXT_QUESTIONS[byte(6) % NEXT_QUESTIONS.length]!;
  const receiptHash = digest.slice(0, 16);
  const receipt = `${ORACLE_VERSION}:${receiptHash}`;

  return {
    version: ORACLE_VERSION,
    question: normalized,
    seed: seed.trim(),
    signal,
    clarity,
    counterweight,
    nextQuestion,
    receipt,
  };
}
import { sha256Hex } from './spellcraft/hash';
