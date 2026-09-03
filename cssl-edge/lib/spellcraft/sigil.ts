import { sha256Hex, stableStringify } from './hash';
import { canonicalizeProgram } from './parser';
import {
  ENGINE_VERSION,
  SIGIL_GEOMETRY_VERSION,
  VOCAB_VERSION,
  type SigilArtifact,
  type SpellReceipt,
  type ValidSpellAnalysis,
} from './types';
import { validateSpellProgram } from './validator';
import { VOCABULARY_HASH } from './vocabulary';

export interface CreateSigilOptions {
  readonly variant?: number;
}

function validateVariant(value: number): number {
  if (!Number.isInteger(value) || value < 0 || value > 65_535) {
    throw new RangeError('Sigil variant must be an integer from 0 through 65535.');
  }
  return value;
}

export function sigilSeedHashForReceipt(receipt: SpellReceipt, variant = 0): string {
  const boundedVariant = validateVariant(variant);
  return sha256Hex([
    'apocky.symbolic-sigil/v1',
    SIGIL_GEOMETRY_VERSION,
    receipt.engineVersion,
    receipt.vocabularyVersion,
    receipt.vocabularyHash,
    receipt.programHash,
    boundedVariant.toString(10),
  ].join('|'));
}

function gcd(left: number, right: number): number {
  let a = left;
  let b = right;
  while (b !== 0) [a, b] = [b, a % b];
  return Math.abs(a);
}

function format(value: number): string {
  return value.toFixed(2);
}

/** Generate disclosed, non-reversible symbolic geometry. No source text is embedded in the SVG. */
export function createSigil(
  analysis: ValidSpellAnalysis,
  options: CreateSigilOptions = {},
): SigilArtifact {
  if (analysis.status !== 'valid' || analysis.receipt.authority !== 'none') {
    throw new TypeError('A valid authority-none spell analysis is required to create a sigil.');
  }
  const validation = validateSpellProgram(analysis.program);
  if (
    !validation.valid
    || analysis.receipt.engineVersion !== ENGINE_VERSION
    || analysis.receipt.vocabularyVersion !== VOCAB_VERSION
    || analysis.receipt.vocabularyHash !== VOCABULARY_HASH
    || analysis.receipt.programHash !== sha256Hex(canonicalizeProgram(analysis.program))
    || analysis.receipt.compiledHash !== sha256Hex(stableStringify(analysis.compiled))
    || analysis.compiled.executable !== false
  ) {
    throw new TypeError('Spell analysis does not match the active sealed engine receipt.');
  }
  const variant = validateVariant(options.variant ?? 0);
  const seedHash = sigilSeedHashForReceipt(analysis.receipt, variant);
  const bytes = seedHash.match(/.{2}/g)?.map((pair) => Number.parseInt(pair, 16)) ?? [];
  const referenceCount = new Set(
    analysis.compiled.operations.flatMap((operation) => operation.vocabularyRefs),
  ).size;
  const count = Math.max(7, Math.min(12, 6 + referenceCount));
  const angleOffset = ((bytes[1] ?? 0) / 255) * Math.PI * 2;
  const points = Array.from({ length: count }, (_, index) => {
    const radialByte = bytes[2 + index] ?? 128;
    const angularByte = bytes[16 + index] ?? 128;
    const radius = 112 + (radialByte / 255) * 94;
    const jitter = ((angularByte / 255) - 0.5) * (Math.PI / count) * 0.6;
    const angle = angleOffset + (index / count) * Math.PI * 2 + jitter;
    return Object.freeze({
      x: 256 + Math.cos(angle) * radius,
      y: 256 + Math.sin(angle) * radius,
    });
  });
  const path = points.map((point, index) => `${index === 0 ? 'M' : 'L'} ${format(point.x)} ${format(point.y)}`).join(' ') + ' Z';

  let step = 2 + ((bytes[30] ?? 0) % Math.max(2, count - 2));
  while (step < count && gcd(step, count) !== 1) step += 1;
  if (step >= count) step = 1;
  const chordOrder: number[] = [];
  let cursor = (bytes[31] ?? 0) % count;
  for (let index = 0; index < count; index += 1) {
    chordOrder.push(cursor);
    cursor = (cursor + step) % count;
  }
  const chordPath = chordOrder
    .map((pointIndex, index) => {
      const point = points[pointIndex];
      return `${index === 0 ? 'M' : 'L'} ${format(point?.x ?? 256)} ${format(point?.y ?? 256)}`;
    })
    .join(' ') + ' Z';
  const nodeCircles = points
    .map((point, index) => {
      const radius = 3 + ((bytes[40 + index] ?? 0) % 5);
      return `<circle cx="${format(point.x)}" cy="${format(point.y)}" r="${radius}" />`;
    })
    .join('');
  const ringRotation = ((bytes[29] ?? 0) / 255) * 180;

  const svg = [
    '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 512 512" role="img" aria-labelledby="sigil-title sigil-description">',
    '<title id="sigil-title">Deterministic symbolic sigil</title>',
    '<desc id="sigil-description">A disclosed geometric reflection generated from a validated symbolic program.</desc>',
    '<rect width="512" height="512" rx="96" fill="#09061a"/>',
    '<g fill="none" stroke-linecap="round" stroke-linejoin="round">',
    '<circle cx="256" cy="256" r="210" stroke="#312e81" stroke-width="3"/>',
    `<g transform="rotate(${format(ringRotation)} 256 256)" stroke="#6366f1" opacity="0.62">`,
    '<ellipse cx="256" cy="256" rx="188" ry="92" stroke-width="3"/>',
    '<ellipse cx="256" cy="256" rx="92" ry="188" stroke-width="3"/>',
    '</g>',
    `<path d="${path}" stroke="#38bdf8" stroke-width="7" opacity="0.92"/>`,
    `<path d="${chordPath}" stroke="#a855f7" stroke-width="4" opacity="0.88"/>`,
    `<g fill="#d8b4fe" stroke="#f8faff" stroke-width="2">${nodeCircles}</g>`,
    '<circle cx="256" cy="256" r="13" fill="#09061a" stroke="#f8faff" stroke-width="5"/>',
    '</g>',
    '</svg>',
  ].join('');

  return Object.freeze({
    kind: 'apocky.symbolic-sigil/v1',
    svg,
    seedHash,
    geometryVersion: SIGIL_GEOMETRY_VERSION,
    variant,
    viewBox: '0 0 512 512',
    title: 'Deterministic symbolic sigil',
    semantics: Object.freeze({
      disclosure: 'Geometry is a deterministic visual fingerprint, not a hidden message or claim of efficacy.',
      path: 'The cyan perimeter follows a digest-seeded traversal of the canonical AST.',
      rings: 'Indigo rings distinguish the program container from its symbolic layers.',
      nodes: 'Visible violet node count is bounded by the number of resolved vocabulary references.',
    }),
    receipt: Object.freeze({
      engineVersion: ENGINE_VERSION,
      vocabularyVersion: VOCAB_VERSION,
      vocabularyHash: analysis.receipt.vocabularyHash,
      programHash: analysis.receipt.programHash,
    }),
  });
}
