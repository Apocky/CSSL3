// How hard the flagship thinks before it answers (the AI Gateway's `reasoning.effort`).
//
// Owner steering 2026-09-25, twice in one day: first "maximum reasoning", then "Use the least
// effort thinking" after max effort spent the whole 64,000-token budget thinking and returned no
// answer on 3/3 thesis attempts (measured: finish_reason "length", reasoning_tokens equal to
// completion_tokens, on both Opus 5.5 and Sonnet 5).
//
// Least for Opus 5.5 is `low`: the gateway lists its supported efforts as low, medium, high,
// xhigh, max (GET /v1/models/anthropic/claude-opus-5.5/endpoints, 2026-09-25) -- `none` is not
// among them, although Opus 5 and Sonnet 5 accept it. The fallback models accept `low` too.

export const LEAST_EFFORT = 'low';

export function hostedEffort(env: Record<string, string | undefined> = process.env): string {
  return env.APOCRYPHA_HOSTED_EFFORT?.trim() || LEAST_EFFORT;
}
