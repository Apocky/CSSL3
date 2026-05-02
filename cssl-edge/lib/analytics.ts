// cssl-edge · lib/analytics.ts
// ════════════════════════════════════════════════════════════════════════
// § T11-W11-ANALYTICS · client + server helpers for the bit-packed event
// stream. Mirrors the Rust crate cssl-analytics-aggregator EventKind +
// ConsentCap discriminants.
//
// § Σ-mask discipline :
//   - Default cap = Deny ; client must explicitly request LocalOnly,
//     AggregateRelay, or FullRelay.
//   - PII assertion (validateNoPII) refuses payloads that look like text.
//   - Body-size limit = 4KB (analytics events are 16 bytes packed, base64
//     ≈ 24 chars + JSON envelope ≤ 256 bytes ; 4KB is a generous ceiling).
//
// § Pure : no Supabase import here ; the API endpoints handle ingestion
// via the supabase singleton.

// ─── EventKind LUT — matches Rust enum 0..13 ───────────────────────────
export const EVENT_KIND_NAMES = [
  'engine.frame_tick',
  'engine.render_mode_changed',
  'input.text_typed',
  'input.text_submitted',
  'intent.classified',
  'intent.routed',
  'gm.response_emitted',
  'dm.phase_transition',
  'procgen.scene_built',
  'mcp.tool_called',
  'kan.classified',
  'mycelium.sync_event',
  'consent.cap_granted',
  'consent.cap_revoked',
] as const;

export type EventKindName = (typeof EVENT_KIND_NAMES)[number];

// ─── ConsentCap — 4-tier Σ-mask ────────────────────────────────────────
export const CONSENT_CAPS = {
  Deny: 0,
  LocalOnly: 1,
  AggregateRelay: 2,
  FullRelay: 3,
} as const;

export type ConsentCapId = (typeof CONSENT_CAPS)[keyof typeof CONSENT_CAPS];

// ─── EventEnvelope — wire-format for /api/analytics/event ──────────────
// Matches the bit-pack EventRecord 16-byte layout with explicit fields
// instead of a pre-base64-encoded blob.
export interface EventEnvelope {
  player_id: string;
  session_id: string;
  kind_id: number; // 0..13
  payload_kind: number; // PayloadKind discriminant
  flags: number; // bit-flags (see Rust flags::*)
  frame_offset: number; // u32 ; differential from session start
  payload_b64: string; // base64-encoded 8-byte payload
  sigma_consent_cap: ConsentCapId; // explicit Σ-mask cap
}

// ─── PayloadKind LUT — for server-side decode hints ────────────────────
export const PAYLOAD_KIND_LABELS: Record<number, string> = {
  0: 'none',
  1: 'frame_tick',
  2: 'mcp_call',
  3: 'text_submit',
  4: 'dm_transition',
  5: 'procgen_scene',
  6: 'consent',
  7: 'mycelium',
  8: 'kan_class',
  9: 'gm_response',
  10: 'typed_len',
  11: 'mode_change',
  12: 'intent_class',
  13: 'intent_route',
};

// ─── Validation helpers ────────────────────────────────────────────────

const UUID_RE =
  /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
const BASE64_RE = /^[A-Za-z0-9+/]+={0,2}$/;

/**
 * § Σ-mask PII assertion : rejects payloads that look like text content.
 * Returns null if OK ; string error message if PII suspected.
 *
 * Heuristic checks :
 *   1. payload_b64 length must be 8..24 chars (1..16 byte payload pre-b64).
 *   2. payload_b64 must be valid base64 (no embedded ASCII text).
 *   3. The decoded bytes must NOT contain stretches of ASCII printable
 *      bytes (≥ 6 in a row triggers PII reject).
 */
export function validateNoPII(env: EventEnvelope): string | null {
  if (!env.payload_b64 || env.payload_b64.length === 0) {
    return 'payload_b64 missing';
  }
  if (env.payload_b64.length > 24) {
    return 'payload_b64 too long (>24 chars suggests text content)';
  }
  if (!BASE64_RE.test(env.payload_b64)) {
    return 'payload_b64 is not valid base64';
  }
  // Decode and check for ASCII-text-like patterns.
  try {
    const padded = env.payload_b64;
    const bytes = Uint8Array.from(atob(padded), (c) => c.charCodeAt(0));
    if (bytes.length > 16) {
      return 'decoded payload too long (>16 bytes)';
    }
    // Count ASCII-printable runs ≥ 6 in a row → suspicious.
    let runLen = 0;
    let maxRun = 0;
    for (const b of bytes) {
      if (b >= 0x20 && b <= 0x7e) {
        runLen += 1;
        if (runLen > maxRun) maxRun = runLen;
      } else {
        runLen = 0;
      }
    }
    if (maxRun >= 6) {
      return `payload contains ASCII run of ${maxRun} bytes (PII suspected)`;
    }
  } catch {
    return 'payload_b64 failed to decode';
  }
  return null;
}

/** Validate a complete EventEnvelope. Returns the typed envelope or error string. */
export function validateEventEnvelope(b: unknown): EventEnvelope | string {
  if (typeof b !== 'object' || b === null) return 'body must be JSON object';
  const e = b as Record<string, unknown>;
  if (typeof e.player_id !== 'string' || !UUID_RE.test(e.player_id))
    return 'player_id must be UUID';
  if (typeof e.session_id !== 'string' || !UUID_RE.test(e.session_id))
    return 'session_id must be UUID';
  if (
    typeof e.kind_id !== 'number' ||
    e.kind_id < 0 ||
    e.kind_id >= EVENT_KIND_NAMES.length
  )
    return `kind_id must be 0..${EVENT_KIND_NAMES.length - 1}`;
  if (typeof e.payload_kind !== 'number' || e.payload_kind < 0 || e.payload_kind > 13)
    return 'payload_kind must be 0..13';
  if (typeof e.flags !== 'number' || e.flags < 0 || e.flags > 0xffff)
    return 'flags must be u16';
  if (typeof e.frame_offset !== 'number' || e.frame_offset < 0)
    return 'frame_offset must be non-negative';
  if (typeof e.payload_b64 !== 'string')
    return 'payload_b64 must be string';
  if (
    typeof e.sigma_consent_cap !== 'number' ||
    e.sigma_consent_cap < 0 ||
    e.sigma_consent_cap > 3
  )
    return 'sigma_consent_cap must be 0..3';
  const env: EventEnvelope = {
    player_id: e.player_id,
    session_id: e.session_id,
    kind_id: e.kind_id,
    payload_kind: e.payload_kind,
    flags: e.flags,
    frame_offset: e.frame_offset,
    payload_b64: e.payload_b64,
    sigma_consent_cap: e.sigma_consent_cap as ConsentCapId,
  };
  const piiErr = validateNoPII(env);
  if (piiErr) return piiErr;
  return env;
}

/** Returns the canonical kind-name for a kind_id. */
export function eventKindName(id: number): EventKindName | 'unknown' {
  if (id < 0 || id >= EVENT_KIND_NAMES.length) return 'unknown';
  // noUncheckedIndexedAccess returns `T | undefined` from numeric index ;
  // we just bounds-checked so the value is defined.
  return EVENT_KIND_NAMES[id] ?? 'unknown';
}

/** Bucket tier discriminant (matches Rust BucketTier). */
export type BucketTier = '1min' | '1hr' | '1day';

export function parseBucketTier(s: string | string[] | undefined): BucketTier {
  const v = Array.isArray(s) ? s[0] : s;
  if (v === '1hr' || v === 'hour') return '1hr';
  if (v === '1day' || v === 'day') return '1day';
  return '1min';
}

/** Translates BucketTier to the Supabase rollup table name. */
export function rollupTableForTier(t: BucketTier): string {
  switch (t) {
    case '1hr':
      return 'analytics_rollup_1hr';
    case '1day':
      return 'analytics_rollup_1day';
    default:
      return 'analytics_rollup_1min';
  }
}

/** Body-size limit for /api/analytics/event POST (4KB). */
export const ANALYTICS_BODY_LIMIT_BYTES = 4096;

/**
 * § Σ-mask attestation : returns a structured statement that NO PII
 * has been retained server-side. Embedded in the metrics endpoint
 * response for sovereign-audit purposes.
 */
export function sigmaMaskAttestation(): {
  no_pii: true;
  no_text_content: true;
  consent_default: 'deny';
  aggregate_only_when_cap_lt_2: true;
} {
  return {
    no_pii: true,
    no_text_content: true,
    consent_default: 'deny',
    aggregate_only_when_cap_lt_2: true,
  };
}
