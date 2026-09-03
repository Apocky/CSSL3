import { compileSpell } from './compiler';
import { analyzeSpell } from './engine';
import { sha256Hex, stableStringify } from './hash';
import { interpretSpell } from './interpreter';
import { canonicalizeProgram } from './parser';
import { sigilSeedHashForReceipt } from './sigil';
import {
  ENGINE_VERSION,
  VOCABULARY_ID,
  VOCAB_VERSION,
  type SpellbookEntry,
  type SpellbookPayload,
  type StorageValidationResult,
  type ValidSpellAnalysis,
} from './types';
import { validateSpellProgram } from './validator';
import { VOCABULARY_HASH } from './vocabulary';

const MAX_EXPORT_CHARS = 512 * 1024;
const MAX_ENTRIES = 100;

export interface CreateSpellbookEntryOptions {
  readonly label?: string;
  readonly savedAt?: string;
}

function normalizeLabel(value: string | undefined): string {
  const normalized = (value ?? 'Untitled symbolic spell')
    .normalize('NFC')
    .replace(/[\u0000-\u001f\u007f]/g, ' ')
    .replace(/\s+/g, ' ')
    .trim();
  return (normalized || 'Untitled symbolic spell').slice(0, 80);
}

function validateSavedAt(value: string | undefined): void {
  if (value === undefined) return;
  const parsed = new Date(value);
  if (!Number.isFinite(parsed.valueOf()) || parsed.toISOString() !== value) {
    throw new TypeError('savedAt must be an exact ISO-8601 UTC timestamp.');
  }
}

function entrySnapshot(entry: Omit<SpellbookEntry, 'entryId' | 'contentHash'>): unknown {
  return {
    kind: entry.kind,
    label: entry.label,
    ...(entry.savedAt ? { savedAt: entry.savedAt } : {}),
    input: entry.input,
    program: entry.program,
    compiled: entry.compiled,
    interpretation: entry.interpretation,
    sigilSeedHash: entry.sigilSeedHash,
    receipt: entry.receipt,
  };
}

export function createSpellbookEntry(
  analysis: ValidSpellAnalysis,
  options: CreateSpellbookEntryOptions = {},
): SpellbookEntry {
  if (analysis.status !== 'valid') throw new TypeError('Only a valid symbolic analysis can be saved.');
  validateSavedAt(options.savedAt);
  const base: Omit<SpellbookEntry, 'entryId' | 'contentHash'> = {
    kind: 'apocky.spellbook.entry/v1',
    label: normalizeLabel(options.label),
    ...(options.savedAt ? { savedAt: options.savedAt } : {}),
    input: analysis.input,
    program: analysis.program,
    compiled: analysis.compiled,
    interpretation: analysis.interpretation,
    sigilSeedHash: sigilSeedHashForReceipt(analysis.receipt),
    receipt: analysis.receipt,
  };
  const contentHash = sha256Hex(stableStringify(entrySnapshot(base)));
  return Object.freeze({
    ...base,
    entryId: `spell-${contentHash.slice(0, 24)}`,
    contentHash,
  });
}

function record(value: unknown): Record<string, unknown> | undefined {
  return value !== null && typeof value === 'object' && !Array.isArray(value)
    ? value as Record<string, unknown>
    : undefined;
}

function hasOnlyKeys(value: Record<string, unknown>, allowed: readonly string[]): boolean {
  const allowedSet = new Set(allowed);
  return Object.keys(value).every((key) => allowedSet.has(key));
}

export function verifySpellbookEntry(value: unknown): StorageValidationResult {
  const issues: string[] = [];
  const candidate = record(value);
  if (!candidate) return { valid: false, issues: ['Entry must be a JSON object.'] };
  if (!hasOnlyKeys(candidate, [
    'kind', 'entryId', 'label', 'savedAt', 'input', 'program', 'compiled',
    'interpretation', 'sigilSeedHash', 'receipt', 'contentHash',
  ])) issues.push('Entry contains undeclared fields.');
  if (candidate.kind !== 'apocky.spellbook.entry/v1') issues.push('Unsupported spellbook entry kind.');
  if (typeof candidate.input !== 'string' || candidate.input.length > 512) issues.push('Entry input is missing or exceeds its limit.');
  if (typeof candidate.label !== 'string' || candidate.label.length === 0 || candidate.label.length > 80) issues.push('Entry label is invalid.');
  if (typeof candidate.input === 'string' && candidate.input.normalize('NFC').trim() !== candidate.input) issues.push('Entry input is not canonical.');
  if (typeof candidate.label === 'string' && normalizeLabel(candidate.label) !== candidate.label) issues.push('Entry label is not canonical.');
  if (candidate.savedAt !== undefined) {
    try {
      validateSavedAt(typeof candidate.savedAt === 'string' ? candidate.savedAt : 'invalid');
    } catch {
      issues.push('Entry savedAt timestamp is invalid.');
    }
  }
  if (typeof candidate.entryId !== 'string' || !/^spell-[a-f0-9]{24}$/.test(candidate.entryId)) issues.push('Entry ID is invalid.');
  if (typeof candidate.contentHash !== 'string' || !/^[a-f0-9]{64}$/.test(candidate.contentHash)) issues.push('Entry content hash is invalid.');
  const receipt = record(candidate.receipt);
  if (
    !receipt
    || receipt.kind !== 'apocky.spellcraft.receipt/v1'
    || receipt.engineVersion !== ENGINE_VERSION
    || receipt.vocabularyId !== VOCABULARY_ID
    || receipt.vocabularyVersion !== VOCAB_VERSION
    || receipt.vocabularyHashAlgorithm !== 'sha256'
    || receipt.vocabularyHash !== VOCABULARY_HASH
    || receipt.authority !== 'none'
  ) {
    issues.push('Entry receipt does not match the active engine and vocabulary seal.');
  }
  if (!candidate.program || typeof candidate.program !== 'object') issues.push('Entry program is missing.');
  if (!candidate.compiled || typeof candidate.compiled !== 'object') issues.push('Entry compiled plan is missing.');
  if (!candidate.interpretation || typeof candidate.interpretation !== 'object') issues.push('Entry interpretation is missing.');
  if (issues.length > 0) return { valid: false, issues: Object.freeze(issues) };

  try {
    const entry = candidate as unknown as SpellbookEntry;
    const validation = validateSpellProgram(entry.program);
    if (!validation.valid) issues.push(...validation.issues.map((issue) => issue.message));
    const canonicalProgram = canonicalizeProgram(entry.program);
    const recompiled = compileSpell(entry.program);
    const reinterpreted = interpretSpell(entry.program);
    const reanalysis = analyzeSpell(entry.input);
    if (reanalysis.status !== 'valid') {
      issues.push('Stored input no longer produces a valid program under the sealed engine.');
    } else {
      if (stableStringify(entry.program) !== stableStringify(reanalysis.program)) issues.push('Program does not match the stored input.');
      if (stableStringify(entry.receipt) !== stableStringify(reanalysis.receipt)) issues.push('Receipt does not match the stored input.');
    }
    if (entry.receipt.sourceHash !== sha256Hex(entry.input.normalize('NFC').trim())) issues.push('Source hash mismatch.');
    if (entry.receipt.programHash !== sha256Hex(canonicalProgram)) issues.push('Program hash mismatch.');
    if (entry.receipt.compiledHash !== sha256Hex(stableStringify(recompiled))) issues.push('Compiled hash mismatch.');
    if (stableStringify(entry.compiled) !== stableStringify(recompiled)) issues.push('Compiled plan was changed after receipt creation.');
    if (stableStringify(entry.interpretation) !== stableStringify(reinterpreted)) issues.push('Interpretation was changed after receipt creation.');
    if (entry.sigilSeedHash !== sigilSeedHashForReceipt(entry.receipt)) issues.push('Sigil seed hash mismatch.');
    const { entryId: _entryId, contentHash: _contentHash, ...base } = entry;
    const recomputedContentHash = sha256Hex(stableStringify(entrySnapshot(base)));
    if (entry.contentHash !== recomputedContentHash) issues.push('Entry content hash mismatch.');
    if (entry.entryId !== `spell-${recomputedContentHash.slice(0, 24)}`) issues.push('Entry ID does not match its content.');
  } catch {
    issues.push('Entry contains malformed program data.');
  }

  return Object.freeze({ valid: issues.length === 0, issues: Object.freeze(issues) });
}

export function serializeSpellbook(entries: readonly SpellbookEntry[]): string {
  if (entries.length > MAX_ENTRIES) throw new RangeError(`A local export may contain at most ${MAX_ENTRIES} entries.`);
  for (const entry of entries) {
    const validation = verifySpellbookEntry(entry);
    if (!validation.valid) throw new TypeError(`Cannot export an invalid entry: ${validation.issues.join(' ')}`);
  }
  const payload: SpellbookPayload = Object.freeze({
    kind: 'apocky.spellbook.local-export/v1',
    storage: 'caller-controlled-local-first',
    entries: Object.freeze([...entries]),
  });
  return stableStringify(payload);
}

export function parseSpellbook(serialized: string): StorageValidationResult {
  if (serialized.length > MAX_EXPORT_CHARS) {
    return { valid: false, issues: [`Export exceeds the ${MAX_EXPORT_CHARS}-character safety limit.`] };
  }
  let raw: unknown;
  try {
    raw = JSON.parse(serialized);
  } catch {
    return { valid: false, issues: ['Export is not valid JSON.'] };
  }
  const candidate = record(raw);
  if (!candidate || candidate.kind !== 'apocky.spellbook.local-export/v1' || candidate.storage !== 'caller-controlled-local-first') {
    return { valid: false, issues: ['Unsupported local spellbook export.'] };
  }
  if (!hasOnlyKeys(candidate, ['kind', 'storage', 'entries'])) {
    return { valid: false, issues: ['Local export contains undeclared fields.'] };
  }
  if (!Array.isArray(candidate.entries) || candidate.entries.length > MAX_ENTRIES) {
    return { valid: false, issues: [`Export must contain no more than ${MAX_ENTRIES} entries.`] };
  }
  const issues = candidate.entries.flatMap((entry, index) =>
    verifySpellbookEntry(entry).issues.map((issue) => `Entry ${index + 1}: ${issue}`),
  );
  if (issues.length > 0) return { valid: false, issues: Object.freeze(issues) };
  const payload: SpellbookPayload = Object.freeze({
    kind: 'apocky.spellbook.local-export/v1',
    storage: 'caller-controlled-local-first',
    entries: Object.freeze(candidate.entries as SpellbookEntry[]),
  });
  return Object.freeze({ valid: true, issues: Object.freeze([]), payload });
}
