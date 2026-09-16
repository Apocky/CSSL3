// One model, many temperaments.
//
// Apocrypha now runs ONE engine for everything. Two models meant the arbiter took chat down every
// time work ran, because only one engine fits the A770 at a time -- that is measured hardware, not
// preference. So the thing that used to separate "the chat model" from "the coder" has to move out
// of the weights and into the sampler.
//
// The ranges below are not invented. From the corpus at
// Apocrypha/specs/sources/transcripts/T94_LLM_GENERATION.txt:
//   "writing code temperature 0.2 to 0.4. You want precision. General tasks temperature 0.7 to 1.0.
//    Balanced. creative writing temperature 1.0 or higher. Embrace variation."
//   and the correction worth keeping in mind:
//   "temperature doesn't make models more creative. It makes them more likely to select lower
//    probability tokens."
// scripts/apocrypha-work/config.ts had already applied this: APOCRYPHA_WORK_TEMPERATURE defaults to
// 0.2, clamped [0,2]. The chat worker had bare literals (0.65/0.9/40/0.05/1.08) with no clamp and no
// way to change them. This module is the single place both now read.

export type PresetId = 'precise' | 'balanced' | 'open';

export interface SamplingProfile {
  readonly temperature: number;
  readonly topP: number;
  readonly topK: number;
  readonly minP: number;
  readonly repeatPenalty: number;
  /** Absent means "let the caller decide" -- the lanes carry different output budgets. */
  readonly seed?: number;
}

export interface PresetDefinition {
  readonly id: PresetId;
  readonly label: string;
  /** Said in terms of what the reader gets, not in terms of the number. */
  readonly effect: string;
  readonly profile: SamplingProfile;
}

export const SAMPLING_PRESETS: readonly PresetDefinition[] = [
  {
    id: 'precise',
    label: 'Precise',
    effect: 'Same question, near enough the same answer. For code, edits and anything you will paste somewhere.',
    // T94: 0.2-0.4 for code. 0.3 sits mid-range; work already shipped 0.2 and is unchanged by this.
    profile: { temperature: 0.3, topP: 0.9, topK: 40, minP: 0.05, repeatPenalty: 1.05 },
  },
  {
    id: 'balanced',
    label: 'Balanced',
    effect: 'Room to think. The default for conversation.',
    // T94: 0.7-1.0 general. Chat was 0.65 -- just under the band it wanted to be in.
    profile: { temperature: 0.8, topP: 0.9, topK: 40, minP: 0.05, repeatPenalty: 1.08 },
  },
  {
    id: 'open',
    label: 'Open',
    effect: 'Unlikelier words get a real chance. Looser, stranger, and more likely to wander.',
    // T94: 1.0+ for variation, with its own warning that past ~1.5 output "become[s] incoherent".
    profile: { temperature: 1.1, topP: 0.95, topK: 80, minP: 0.02, repeatPenalty: 1.1 },
  },
];

export const DEFAULT_PRESET: PresetId = 'balanced';

export function presetById(id: string | null | undefined): PresetDefinition {
  return SAMPLING_PRESETS.find((preset) => preset.id === id)
    ?? SAMPLING_PRESETS.find((preset) => preset.id === DEFAULT_PRESET)!;
}

// Which temperament a starter button implies. The button sets it; the reader can still override.
export const PRESET_FOR_TOOL: Readonly<Record<string, PresetId>> = {
  write: 'precise',
  explain: 'precise',
  debug: 'precise',
  review: 'precise',
  plan: 'balanced',
};

// Bounds. Same shape as the env clamps in scripts/apocrypha-work/config.ts, but these CLAMP where
// those THROW -- an env typo should stop a service at startup, a phone sending temperature 9 should
// not kill a turn. Never trust the client; never punish it either.
const BOUNDS = {
  temperature: { min: 0, max: 2 },
  topP: { min: 0, max: 1 },
  topK: { min: 0, max: 200 },
  minP: { min: 0, max: 1 },
  repeatPenalty: { min: 0.5, max: 2 },
} as const;

function clampNumber(value: unknown, fallback: number, min: number, max: number): number {
  if (typeof value !== 'number' || !Number.isFinite(value)) return fallback;
  return Math.min(max, Math.max(min, value));
}

/** Coerce anything that arrived over the wire into a profile that cannot break a turn. */
export function clampSampling(input: unknown, base: SamplingProfile): SamplingProfile {
  const raw = (input && typeof input === 'object') ? input as Record<string, unknown> : {};
  const seed = raw.seed;
  return {
    temperature: clampNumber(raw.temperature, base.temperature, BOUNDS.temperature.min, BOUNDS.temperature.max),
    topP: clampNumber(raw.topP, base.topP, BOUNDS.topP.min, BOUNDS.topP.max),
    topK: Math.round(clampNumber(raw.topK, base.topK, BOUNDS.topK.min, BOUNDS.topK.max)),
    minP: clampNumber(raw.minP, base.minP, BOUNDS.minP.min, BOUNDS.minP.max),
    repeatPenalty: clampNumber(raw.repeatPenalty, base.repeatPenalty, BOUNDS.repeatPenalty.min, BOUNDS.repeatPenalty.max),
    // A seed is either a real integer or absent. There is no sensible clamp for "reproducible".
    ...(typeof seed === 'number' && Number.isInteger(seed) ? { seed } : {}),
  };
}

/** The profile a turn should run with, given a preset and any overrides the reader supplied. */
export function resolveSampling(presetId: string | null | undefined, overrides?: unknown): SamplingProfile {
  return clampSampling(overrides, presetById(presetId).profile);
}
