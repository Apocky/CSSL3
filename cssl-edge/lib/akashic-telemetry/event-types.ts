// § Akashic-Webpage-Records · event-types.ts
// ω-field-cell flavored event-shape ; every page-event becomes a cell stamped
// in the Akashic-Records. cell_id = BLAKE3-flavored 16-char hash · sigma_mask
// gates audience · cap_witness proves consent. Substrate-native ¬ Sentry-clone.
//
// Substrate parallels :
//   ω-field cell    → AkashicEvent
//   Σ-mask          → sigma_mask bitmask
//   KAN-pattern     → server-side cluster signature (out-of-scope here)
//   mycelium-spore  → batched event-flush
//
// Every event-kind is a discriminated-union member ; payload-shape is bound
// to kind. Default-deny philosophy : sigma_mask=0 ⇒ never leaves client.

// ─── Σ-mask audience tiers (bitmask) ───────────────────────────────────────
// Each bit = one audience. AND'd at server-time against route-required-mask.
// Future audiences (admin-aggregated · mycelium-federated · self-only) layered
// on without breaking older events.
export const SIGMA_NONE = 0b0000;
export const SIGMA_SELF = 0b0001;        // client-only · never flushed
export const SIGMA_AGGREGATE = 0b0010;   // k-anon ≥ K · counts only
export const SIGMA_PATTERN = 0b0100;     // stack-trace cluster · k-anon ≥ K
export const SIGMA_FEDERATED = 0b1000;   // mycelium cross-session · opt-in
export const SIGMA_ALL = 0b1111;

export type SigmaMask = number; // any bitwise-OR of the above

// ─── Consent-tiers (user-facing · sovereign-revocable) ─────────────────────
// Each tier maps to a default sigma_mask + a k-anon threshold. User picks tier
// via AkashicConsent overlay ; revoke = downgrade to None at any time.
export type ConsentTier = 'none' | 'spore' | 'mycelium' | 'akashic';

export interface ConsentPolicy {
  tier: ConsentTier;
  default_mask: SigmaMask;
  k_anon: number;          // server retains pattern-detail iff ≥ this many users
  capture_stack: boolean;  // captures component-stacks on errors
  capture_console: boolean;
  capture_user_flow: boolean;
}

// CONSENT_TIERS · canonical policy-table. Keep as const ¬ frozen so callers
// can read but the lib never mutates.
export const CONSENT_TIERS: Record<ConsentTier, ConsentPolicy> = {
  none: {
    tier: 'none',
    default_mask: SIGMA_NONE,
    k_anon: Number.POSITIVE_INFINITY,
    capture_stack: false,
    capture_console: false,
    capture_user_flow: false,
  },
  spore: {
    tier: 'spore',
    default_mask: SIGMA_AGGREGATE,
    k_anon: 10,
    capture_stack: false,
    capture_console: false,
    capture_user_flow: false,
  },
  mycelium: {
    tier: 'mycelium',
    default_mask: SIGMA_AGGREGATE | SIGMA_PATTERN,
    k_anon: 5,
    capture_stack: true,
    capture_console: false,
    capture_user_flow: false,
  },
  akashic: {
    tier: 'akashic',
    default_mask: SIGMA_AGGREGATE | SIGMA_PATTERN | SIGMA_FEDERATED,
    k_anon: 5,
    capture_stack: true,
    capture_console: true,
    capture_user_flow: true,
  },
};

// ─── Event kinds (discriminator) ───────────────────────────────────────────
// This runtime catalog also reserves server-only namespaces from public ingest.
export const AKASHIC_KINDS = [
  'page.view',
  'page.error',
  'page.unload',
  'react.error',
  'promise.unhandled',
  'console.error',
  'console.warn',
  'perf.lcp',
  'perf.fid',
  'perf.cls',
  'perf.inp',
  'perf.ttfb',
  'perf.fcp',
  'perf.long_task',
  'perf.resource_slow',
  'perf.resource_fail',
  'net.fail',
  'net.slow',
  'consent.granted',
  'consent.revoked',
  'consent.purge_request',
  'user.flow',
  'deploy.detected',
] as const;

export type AkashicKind = typeof AKASHIC_KINDS[number];
const AKASHIC_KIND_SET: ReadonlySet<string> = new Set(AKASHIC_KINDS);

export function isAkashicKind(value: unknown): value is AkashicKind {
  return typeof value === 'string' && AKASHIC_KIND_SET.has(value);
}

// ─── Event shape (the ω-field cell) ────────────────────────────────────────
// Required-keys-only · payload-bag for kind-specific extras. cell_id uniquely
// addresses this cell ; user can purge by listing cell_ids tied to their cap.
export interface AkashicEvent {
  // — Substrate metadata —
  cell_id: string;             // 16-char BLAKE3-flavored hex hash
  ts_iso: string;              // UTC ISO-8601
  sigma_mask: SigmaMask;       // bitmask · 0 = never-flush
  cap_witness?: string;        // sovereign-cap proof if required

  // — Substrate version-attestation —
  dpl_id: string;              // Vercel deployment-id
  commit_sha: string;          // git commit-sha
  build_time: string;          // ISO ; when this bundle was built

  // — Event content —
  kind: AkashicKind;
  payload: Record<string, unknown>;

  // — Sovereignty —
  session_id: string;          // ephemeral random · NOT user-id · rotates
  user_cap_hash?: string;      // hash of user-cap iff logged-in (purge-key)
}

// ─── Payload-shapes per kind (declarative · for typing) ────────────────────

export interface PerfPayload {
  value: number;               // metric value (ms or unitless)
  url: string;
  viewport: { w: number; h: number };
  connection?: string;         // navigator.connection?.effectiveType
}

export interface ErrorPayload {
  message: string;
  // Stack only included when consent_tier ∋ {mycelium, akashic}
  stack?: string;
  source?: string;
  line?: number;
  col?: number;
  component_stack?: string;    // React-only
  cluster_signature?: string;  // 16-char hash of normalized stack-frames
}

export interface NetFailPayload {
  url: string;
  status?: number;
  duration_ms?: number;
  method?: string;
}

export interface DeployDetectedPayload {
  observed_dpl_id: string;     // server-reported dpl_id
  page_load_dpl_id: string;    // dpl_id baked into running bundle
  age_ms: number;              // ms since page load
}

// ─── Buffer-shape for batched flush ────────────────────────────────────────
// Bit-pack : ring-buffer with deterministic flush-trigger (size or interval).
export interface AkashicBatch {
  batch_id: string;            // BLAKE3 of (session_id · first.cell_id · last.cell_id)
  session_id: string;
  events: AkashicEvent[];
  flush_reason: 'size' | 'interval' | 'unload' | 'manual';
}
