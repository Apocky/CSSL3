// § Akashic-Webpage-Records · sigma-mask.ts
// Σ-mask · client-side cell-gating. Default-deny ; cap-grant via consent-arch.
// Server re-checks ; this is the FIRST gate ¬ the only gate.
//
// Mathematical-notion : every event has σ_mask ∈ N⁴ ; route requires σ_route ;
// emission ⇔ (σ_mask AND σ_route) ≠ 0 AND consent.tier ⊑ event.required_tier.

import {
  SIGMA_NONE,
  SIGMA_SELF,
  SIGMA_AGGREGATE,
  SIGMA_PATTERN,
  SIGMA_FEDERATED,
  CONSENT_TIERS,
  type AkashicEvent,
  type AkashicKind,
  type ConsentTier,
  type SigmaMask,
} from './event-types';

// ─── Per-kind required-tier (which consent-tier unlocks this kind) ─────────
// Spore = aggregate-only · Mycelium = + stack-traces · Akashic = + console + flow.
const KIND_REQUIRED_TIER: Record<AkashicKind, ConsentTier> = {
  // Always-on (even at None) for sovereignty-of-the-user themselves :
  'consent.granted': 'none',
  'consent.revoked': 'none',
  'consent.purge_request': 'none',

  // Spore-tier kinds (counts/aggregate)
  'page.view': 'spore',
  'page.unload': 'spore',
  'perf.lcp': 'spore',
  'perf.fid': 'spore',
  'perf.cls': 'spore',
  'perf.inp': 'spore',
  'perf.ttfb': 'spore',
  'perf.fcp': 'spore',
  'perf.long_task': 'spore',
  'perf.resource_slow': 'spore',
  'perf.resource_fail': 'spore',
  'page.error': 'spore',
  'deploy.detected': 'spore',
  'net.fail': 'spore',
  'net.slow': 'spore',

  // Mycelium-tier kinds (stack traces · cluster sigs)
  'react.error': 'mycelium',
  'promise.unhandled': 'mycelium',

  // Akashic-tier kinds (console · user-flow)
  'console.error': 'akashic',
  'console.warn': 'akashic',
  'user.flow': 'akashic',
};

// Tier-ordering · monotonic upgrade-only.
const TIER_RANK: Record<ConsentTier, number> = {
  none: 0,
  spore: 1,
  mycelium: 2,
  akashic: 3,
};

// ─── Σ-mask gate · "should this event leave the client?" ───────────────────
// Returns the effective sigma_mask (or SIGMA_NONE if denied). Caller writes
// the returned mask into the event ; if NONE, drop the event entirely.
//
// Special case : the consent-bookkeeping kinds (consent.*) always emit, even
// at none-tier ; these self-witness the user's sovereignty-actions. They get
// SIGMA_AGGREGATE so the server retains the count without anything else.
export function gateEvent(
  kind: AkashicKind,
  consent_tier: ConsentTier
): SigmaMask {
  const required = KIND_REQUIRED_TIER[kind];
  if (TIER_RANK[consent_tier] < TIER_RANK[required]) return SIGMA_NONE;
  const policy = CONSENT_TIERS[consent_tier];
  // none-tier kinds (consent.*) always self-witness · force aggregate-mask.
  if (required === 'none') return SIGMA_AGGREGATE;
  return policy.default_mask;
}

// ─── Σ-mask redact · scrub PII-shaped payload-fields ───────────────────────
// Defense-in-depth ¬ catch-all : redacts obvious patterns (email · jwt-ish ·
// query-string secrets · long-numerics that look like credit-cards). Server
// re-runs a stricter pass.
const EMAIL_RX = /[a-z0-9._%+-]+@[a-z0-9.-]+\.[a-z]{2,}/gi;
const JWT_RX = /eyJ[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}/g;
const LONG_DIGITS_RX = /\b\d{12,19}\b/g;
const QUERY_SECRET_RX = /([?&](?:token|key|secret|api[_-]?key|password|auth)=)([^&\s]+)/gi;

export function redactString(s: string): string {
  return s
    .replace(EMAIL_RX, '«email»')
    .replace(JWT_RX, '«jwt»')
    .replace(LONG_DIGITS_RX, '«num»')
    .replace(QUERY_SECRET_RX, '$1«redacted»');
}

// Deep-redact a payload-bag (in-place-safe : returns a new object).
export function redactPayload(p: Record<string, unknown>): Record<string, unknown> {
  const out: Record<string, unknown> = {};
  for (const k of Object.keys(p)) {
    const v = p[k];
    if (typeof v === 'string') {
      out[k] = redactString(v);
    } else if (v !== null && typeof v === 'object' && !Array.isArray(v)) {
      out[k] = redactPayload(v as Record<string, unknown>);
    } else if (Array.isArray(v)) {
      out[k] = v.map((x) =>
        typeof x === 'string'
          ? redactString(x)
          : x !== null && typeof x === 'object'
            ? redactPayload(x as Record<string, unknown>)
            : x
      );
    } else {
      out[k] = v;
    }
  }
  return out;
}

// ─── Apply the gate + redact pipeline · pure function ──────────────────────
// Returns the final event ready for ring-buffer push ; OR null if denied.
export function applyGate(
  ev: AkashicEvent,
  consent_tier: ConsentTier
): AkashicEvent | null {
  const mask = gateEvent(ev.kind, consent_tier);
  if (mask === SIGMA_NONE) return null;
  return {
    ...ev,
    sigma_mask: mask,
    payload: redactPayload(ev.payload),
  };
}

// ─── Re-export Σ constants for ergonomic imports ───────────────────────────
export {
  SIGMA_NONE,
  SIGMA_SELF,
  SIGMA_AGGREGATE,
  SIGMA_PATTERN,
  SIGMA_FEDERATED,
  KIND_REQUIRED_TIER,
};
