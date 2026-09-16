// One model, many temperaments.
//
// Apocrypha runs ONE engine for everything. Two models meant the arbiter took chat down every time
// work ran, because only one engine fits the A770 at a time -- that is measured hardware, not
// preference. So the thing that used to separate "the chat model" from "the coder" has to move out
// of the weights and into the sampler. This module is that separation.
//
// WHAT THE ENGINE ACTUALLY OFFERS, measured against the running llama.cpp b9743 on 2026-09-16 by
// POSTing each parameter and reading the status code. All 24 were accepted; the sampler chain it
// reports at /props is:
//     penalties -> dry -> top_n_sigma -> top_k -> typ_p -> top_p -> min_p -> xtc -> temperature
// Before this file, we drove five of those and left the rest at whatever the GGUF metadata carried.
// Acceptance is not effect, so the load-bearing ones were checked for effect separately:
//   - seed BITES: temp 1.0 with seed 7 gave byte-identical output twice, seed 999 differed.
//   - temp 0 alone is deterministic here: 3/3 identical with NO seed, while the unseeded temp 0.25
//     control gave 3 different outputs. That control is what makes the claim falsifiable.
//     N! the T98 transcript warns temp 0 is only "best effort" because a from-scratch PyTorch
//     implementation divides by max(temp, epsilon). That warning is PyTorch-scoped and does NOT
//     transfer to llama.cpp, which takes a real greedy path. Measured, not assumed.
//   - grammar BITES: a one-word GBNF root forced exactly that word.
//
// Ranges come from Apocrypha/specs/sources/transcripts/T94_LLM_GENERATION.txt:
//   "writing code temperature 0.2 to 0.4. You want precision. General tasks temperature 0.7 to 1.0.
//    Balanced. creative writing temperature 1.0 or higher. Embrace variation."
//   "temperature doesn't make models more creative. It makes them more likely to select lower
//    probability tokens."
// Sampler ORDER is load-bearing, per T98: top_k/top_p mask to -inf BEFORE the final softmax, so a
// masked token has probability exactly zero and temperature never gets to act on it.

export type PresetId = 'exact' | 'precise' | 'balanced' | 'open';

/**
 * Every per-request dial this engine honours.
 *
 * Only the first five are required: they predate this file and three call sites already destructure
 * them. Everything else is optional so that a profile says only what it means to change, and an
 * omitted dial is left at the engine's own default rather than being pinned to a number nobody
 * chose.
 */
export interface SamplingProfile {
  readonly temperature: number;
  readonly topP: number;
  readonly topK: number;
  readonly minP: number;
  readonly repeatPenalty: number;
  /** Absent means "let the caller decide" -- the lanes carry different output budgets. */
  readonly seed?: number;

  // --- anti-repetition -------------------------------------------------------------------------
  /** How far back the penalties sampler looks. Engine default is 64, which is ~2 lines of code. */
  readonly repeatLastN?: number;
  readonly presencePenalty?: number;
  readonly frequencyPenalty?: number;
  /**
   * DRY penalises repeated SEQUENCES rather than repeated tokens, which is the distinction that
   * matters for code: an indent, a brace and a variable name recur constantly and legitimately,
   * and repeat_penalty cannot tell those apart from a model stuck in a loop.
   * Honest note: a side-by-side on a repetition-heavy generation did NOT separate the two at 220
   * tokens. The argument for DRY on code is structural, not an observed win here.
   */
  readonly dryMultiplier?: number;
  readonly dryBase?: number;
  readonly dryAllowedLength?: number;
  readonly drySequenceBreakers?: readonly string[];

  // --- tail shaping ----------------------------------------------------------------------------
  readonly typicalP?: number;
  /** Keeps tokens within N standard deviations of the top logit. Newer than top_p, cheaper to tune. */
  readonly topNSigma?: number;
  readonly xtcProbability?: number;
  readonly xtcThreshold?: number;

  // --- determinism -----------------------------------------------------------------------------
  /** Force the greedy path. The only setting that makes a run genuinely repeatable. */
  readonly greedy?: boolean;
}

export interface PresetDefinition {
  readonly id: PresetId;
  readonly label: string;
  /** Said in terms of what the reader gets, not in terms of the number. */
  readonly effect: string;
  readonly profile: SamplingProfile;
}

/**
 * Sequence breakers stop DRY from treating ordinary code punctuation as a repeated phrase. Newlines
 * and indentation are the whole shape of Python; without these DRY would fight the language.
 */
const CODE_SEQUENCE_BREAKERS = ['\n', ':', '"', '`', '```', '\t', '    '] as const;

export const SAMPLING_PRESETS: readonly PresetDefinition[] = [
  {
    id: 'exact',
    label: 'Exact',
    effect: 'The same answer every time, for the same question. Slowest to surprise you, best for edits you intend to re-run.',
    // Greedy, not "temperature nearly zero". Proven deterministic on this engine across 3 runs with
    // no seed at all; the seed is pinned anyway so the run stays reproducible if greedy is overridden.
    profile: {
      temperature: 0, topP: 1, topK: 1, minP: 0, repeatPenalty: 1,
      greedy: true, seed: 7,
    },
  },
  {
    id: 'precise',
    label: 'Precise',
    effect: 'Near enough the same answer twice. For code, edits and anything you will paste somewhere.',
    // T94: 0.2-0.4 for code. 0.3 sits mid-range.
    // repeat_penalty is deliberately 1.0 (off) with DRY carrying anti-repetition instead -- see the
    // note on dryMultiplier above for why that split is the right one for code.
    profile: {
      temperature: 0.3, topP: 0.9, topK: 40, minP: 0.05, repeatPenalty: 1,
      dryMultiplier: 0.8, dryBase: 1.75, dryAllowedLength: 2,
      drySequenceBreakers: CODE_SEQUENCE_BREAKERS,
      repeatLastN: 256,
    },
  },
  {
    id: 'balanced',
    label: 'Balanced',
    effect: 'Room to think. The default for conversation.',
    // T94: 0.7-1.0 general. These five numbers are UNCHANGED and must stay unchanged: this is the
    // profile the public chat room runs on, and altering it changes what apocky.com says to people.
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
  repeatLastN: { min: -1, max: 4096 },
  presencePenalty: { min: -2, max: 2 },
  frequencyPenalty: { min: -2, max: 2 },
  dryMultiplier: { min: 0, max: 5 },
  dryBase: { min: 1, max: 4 },
  dryAllowedLength: { min: 1, max: 64 },
  typicalP: { min: 0, max: 1 },
  topNSigma: { min: 0, max: 10 },
  xtcProbability: { min: 0, max: 1 },
  xtcThreshold: { min: 0, max: 1 },
} as const;

type BoundedKey = keyof typeof BOUNDS;

function clampNumber(value: unknown, fallback: number, min: number, max: number): number {
  if (typeof value !== 'number' || !Number.isFinite(value)) return fallback;
  return Math.min(max, Math.max(min, value));
}

/** Clamp an optional dial, preserving "absent" rather than inventing a default for it. */
function clampOptional(raw: Record<string, unknown>, base: SamplingProfile, key: BoundedKey): number | undefined {
  const supplied = raw[key];
  const fallback = base[key as keyof SamplingProfile] as number | undefined;
  if (typeof supplied !== 'number' || !Number.isFinite(supplied)) return fallback;
  const bound = BOUNDS[key];
  return Math.min(bound.max, Math.max(bound.min, supplied));
}

/** Coerce anything that arrived over the wire into a profile that cannot break a turn. */
export function clampSampling(input: unknown, base: SamplingProfile): SamplingProfile {
  const raw = (input && typeof input === 'object') ? input as Record<string, unknown> : {};
  const seed = raw.seed;
  const breakers = raw.drySequenceBreakers;

  const optional: Record<string, unknown> = {};
  for (const key of ['repeatLastN', 'presencePenalty', 'frequencyPenalty', 'dryMultiplier', 'dryBase',
    'dryAllowedLength', 'typicalP', 'topNSigma', 'xtcProbability', 'xtcThreshold'] as BoundedKey[]) {
    const value = clampOptional(raw, base, key);
    if (value !== undefined) optional[key] = key === 'repeatLastN' || key === 'dryAllowedLength' ? Math.round(value) : value;
  }

  return {
    temperature: clampNumber(raw.temperature, base.temperature, BOUNDS.temperature.min, BOUNDS.temperature.max),
    topP: clampNumber(raw.topP, base.topP, BOUNDS.topP.min, BOUNDS.topP.max),
    topK: Math.round(clampNumber(raw.topK, base.topK, BOUNDS.topK.min, BOUNDS.topK.max)),
    minP: clampNumber(raw.minP, base.minP, BOUNDS.minP.min, BOUNDS.minP.max),
    repeatPenalty: clampNumber(raw.repeatPenalty, base.repeatPenalty, BOUNDS.repeatPenalty.min, BOUNDS.repeatPenalty.max),
    ...optional,
    // Strings only, and capped, so a caller cannot hand the engine an unbounded breaker list.
    ...(Array.isArray(breakers)
      ? { drySequenceBreakers: breakers.filter((b): b is string => typeof b === 'string').slice(0, 16) }
      : base.drySequenceBreakers ? { drySequenceBreakers: base.drySequenceBreakers } : {}),
    ...(typeof raw.greedy === 'boolean' ? { greedy: raw.greedy } : base.greedy ? { greedy: base.greedy } : {}),
    // A seed is either a real integer or absent. There is no sensible clamp for "reproducible".
    ...(typeof seed === 'number' && Number.isInteger(seed) ? { seed } : base.seed !== undefined ? { seed: base.seed } : {}),
  };
}

/** The profile a turn should run with, given a preset and any overrides the reader supplied. */
export function resolveSampling(presetId: string | null | undefined, overrides?: unknown): SamplingProfile {
  return clampSampling(overrides, presetById(presetId).profile);
}

/**
 * Render a profile as llama.cpp's own request fields.
 *
 * One place converts camelCase to the engine's snake_case, because the alternative is every caller
 * spelling `dry_sequence_breakers` correctly forever. Absent dials are OMITTED rather than sent as
 * null: sending a null where the engine wants a number is how a working turn starts 400ing.
 */
export function toEngineParams(profile: SamplingProfile): Record<string, unknown> {
  const out: Record<string, unknown> = {
    temperature: profile.temperature,
    top_p: profile.topP,
    top_k: profile.topK,
    min_p: profile.minP,
    repeat_penalty: profile.repeatPenalty,
  };
  const map: ReadonlyArray<readonly [keyof SamplingProfile, string]> = [
    ['seed', 'seed'],
    ['repeatLastN', 'repeat_last_n'],
    ['presencePenalty', 'presence_penalty'],
    ['frequencyPenalty', 'frequency_penalty'],
    ['dryMultiplier', 'dry_multiplier'],
    ['dryBase', 'dry_base'],
    ['dryAllowedLength', 'dry_allowed_length'],
    ['drySequenceBreakers', 'dry_sequence_breakers'],
    ['typicalP', 'typical_p'],
    ['topNSigma', 'top_n_sigma'],
    ['xtcProbability', 'xtc_probability'],
    ['xtcThreshold', 'xtc_threshold'],
  ];
  for (const [key, wire] of map) {
    const value = profile[key];
    if (value !== undefined) out[wire] = value;
  }
  // Greedy is expressed as the chain, not as a flag: llama.cpp has no `greedy: true` field, and
  // top_k 1 is what actually collapses the distribution to its argmax.
  if (profile.greedy) {
    out.top_k = 1;
    out.temperature = 0;
  }
  return out;
}
