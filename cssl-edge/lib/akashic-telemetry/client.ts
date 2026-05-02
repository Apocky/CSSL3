// § Akashic-Webpage-Records · client.ts
// Main client API. Singleton-pattern · idempotent-init · sovereign-revocable.
//
// Public surface :
//   - init(opts) : install all observers + wire up
//   - capture(kind, payload) : stamp a cell · pure
//   - flush(reason) : force-flush ring-buffer to /api/akashic/batch
//   - withConsent(tier) : update tier · emits consent.granted/.revoked
//   - attestVersion() : poll /api/akashic/version · emit deploy.detected on drift
//   - purgeAllMine() : DELETE /api/akashic/purge with cap-witness
//   - currentTier() : read consent-tier
//
// Bit-pack philosophy : ring-buffer pre-allocated · 256-event capacity ·
// flush at 32 events OR 10s OR unload OR manual. Backend owns retention.

import {
  CONSENT_TIERS,
  type AkashicEvent,
  type AkashicKind,
  type ConsentTier,
  type ConsentPolicy,
  type AkashicBatch,
} from './event-types';
import { applyGate } from './sigma-mask';

// ─── Tunables · keep low for cost · raise post-launch if needed ────────────
const RING_CAP = 256;
const FLUSH_AT = 32;
const FLUSH_INTERVAL_MS = 10_000;
const VERSION_PROBE_MS = 60_000; // poll /api/akashic/version every 60s
const STORAGE_KEY = 'akashic.consent.tier.v1';
const SESSION_KEY = 'akashic.session.id.v1';

// ─── Module-state · singleton ──────────────────────────────────────────────
interface AkashicState {
  initialized: boolean;
  consent_tier: ConsentTier;
  session_id: string;
  page_load_dpl_id: string;     // baked-in dpl_id at page load
  current_dpl_id: string;       // most-recent server-reported dpl_id
  commit_sha: string;
  build_time: string;
  ring: AkashicEvent[];
  ring_idx: number;             // write head
  flush_timer: ReturnType<typeof setTimeout> | null;
  version_timer: ReturnType<typeof setInterval> | null;
  endpoint_batch: string;
  endpoint_event: string;
  endpoint_version: string;
  endpoint_purge: string;
  user_cap_hash?: string;
  cap_witness?: string;
  beforeunload_attached: boolean;
}

const state: AkashicState = {
  initialized: false,
  consent_tier: 'spore', // default-tier ¬ zero ; spore = aggregate-only
  session_id: '',
  page_load_dpl_id: 'unknown',
  current_dpl_id: 'unknown',
  commit_sha: 'unknown',
  build_time: 'unknown',
  ring: [],
  ring_idx: 0,
  flush_timer: null,
  version_timer: null,
  endpoint_batch: '/api/akashic/batch',
  endpoint_event: '/api/akashic/event',
  endpoint_version: '/api/akashic/version',
  endpoint_purge: '/api/akashic/purge',
  beforeunload_attached: false,
};

// ─── Tiny crypto-ish helpers (BLAKE3 unavailable in browser-stage-0) ───────
// 16-char fnv-1a-ish hex hash. Deterministic · NOT cryptographic. Server runs
// a real BLAKE3 pass when retaining ; this is just for cell_id / batch_id /
// cluster_signature.
export function hash16(s: string): string {
  let h1 = 0x811c9dc5 >>> 0;
  let h2 = 0xcbf29ce4 >>> 0;
  for (let i = 0; i < s.length; i++) {
    const c = s.charCodeAt(i);
    h1 = Math.imul(h1 ^ c, 0x01000193) >>> 0;
    h2 = Math.imul(h2 ^ c, 0x100000001b3 >>> 0) >>> 0;
  }
  return (h1.toString(16).padStart(8, '0') + h2.toString(16).padStart(8, '0')).slice(0, 16);
}

// 16-char random session-id ; uses crypto.getRandomValues if available, else Math.random.
function randomSessionId(): string {
  try {
    if (typeof crypto !== 'undefined' && crypto.getRandomValues !== undefined) {
      const buf = new Uint8Array(8);
      crypto.getRandomValues(buf);
      return Array.from(buf).map((b) => b.toString(16).padStart(2, '0')).join('');
    }
  } catch {
    // fall through
  }
  let s = '';
  for (let i = 0; i < 16; i++) s += Math.floor(Math.random() * 16).toString(16);
  return s;
}

function nowIso(): string {
  return new Date().toISOString();
}

// Read-or-create ephemeral session-id. Lives in sessionStorage so refresh
// keeps it ; tab-close clears it. NO localStorage for session-id (cross-tab
// linkage = sovereignty-violation).
function loadOrMakeSessionId(): string {
  try {
    if (typeof sessionStorage !== 'undefined') {
      const existing = sessionStorage.getItem(SESSION_KEY);
      if (existing !== null && existing.length === 16) return existing;
      const fresh = randomSessionId();
      sessionStorage.setItem(SESSION_KEY, fresh);
      return fresh;
    }
  } catch {
    // SecurityError in some sandboxed iframes ; degrade gracefully
  }
  return randomSessionId();
}

// Persist consent across sessions in localStorage ; revoke clears.
function loadStoredTier(): ConsentTier {
  try {
    if (typeof localStorage !== 'undefined') {
      const v = localStorage.getItem(STORAGE_KEY);
      if (v === 'none' || v === 'spore' || v === 'mycelium' || v === 'akashic') {
        return v;
      }
    }
  } catch {
    // ignore
  }
  return 'spore';
}

function persistTier(tier: ConsentTier): void {
  try {
    if (typeof localStorage !== 'undefined') {
      localStorage.setItem(STORAGE_KEY, tier);
    }
  } catch {
    // ignore
  }
}

// ─── init opts ─────────────────────────────────────────────────────────────
export interface InitOpts {
  dpl_id?: string;
  commit_sha?: string;
  build_time?: string;
  // Caller may override · for self-hosted forks.
  endpoints?: Partial<{
    batch: string;
    event: string;
    version: string;
    purge: string;
  }>;
  // Optional logged-in user-cap (hashed) for sovereign-purge linkage.
  user_cap_hash?: string;
  cap_witness?: string;
  // If false, init does NOT install observers (test mode).
  install_observers?: boolean;
}

// Idempotent · safe to call repeatedly. Returns false if already initialized.
export function init(opts: InitOpts = {}): boolean {
  if (state.initialized) return false;
  state.initialized = true;
  state.consent_tier = loadStoredTier();
  state.session_id = loadOrMakeSessionId();
  state.page_load_dpl_id = opts.dpl_id ?? 'unknown';
  state.current_dpl_id = state.page_load_dpl_id;
  state.commit_sha = opts.commit_sha ?? 'unknown';
  state.build_time = opts.build_time ?? 'unknown';
  if (opts.user_cap_hash !== undefined) state.user_cap_hash = opts.user_cap_hash;
  if (opts.cap_witness !== undefined) state.cap_witness = opts.cap_witness;
  if (opts.endpoints !== undefined) {
    if (opts.endpoints.batch !== undefined) state.endpoint_batch = opts.endpoints.batch;
    if (opts.endpoints.event !== undefined) state.endpoint_event = opts.endpoints.event;
    if (opts.endpoints.version !== undefined) state.endpoint_version = opts.endpoints.version;
    if (opts.endpoints.purge !== undefined) state.endpoint_purge = opts.endpoints.purge;
  }
  // Pre-allocate ring (Sawyer/Pokémon-OG style) ; fixed-size · no growth.
  state.ring = new Array(RING_CAP);
  state.ring_idx = 0;

  // Stamp the page-view cell immediately (consent-checked).
  capture('page.view', {
    url: typeof location !== 'undefined' ? location.href : 'about:blank',
    referrer: typeof document !== 'undefined' ? document.referrer : '',
    viewport: viewport(),
  });

  // Wire up flush-interval + beforeunload + version-probe (skip in test mode).
  if (opts.install_observers !== false) {
    armFlushTimer();
    armVersionProbe();
    attachUnloadFlush();
  }

  return true;
}

// ─── capture · the only entry-point for stamping a cell ────────────────────
// Pure function (modulo ring-buffer push). Σ-mask gate first ; if denied,
// no-op. Returns the cell_id (or '' if denied).
export function capture(
  kind: AkashicKind,
  payload: Record<string, unknown> = {}
): string {
  if (!state.initialized) return '';
  const ts = nowIso();
  const cell_id = hash16(`${ts}|${state.session_id}|${kind}|${JSON.stringify(payload)}`);
  const candidate: AkashicEvent = {
    cell_id,
    ts_iso: ts,
    sigma_mask: 0, // gate sets the real value
    dpl_id: state.page_load_dpl_id,
    commit_sha: state.commit_sha,
    build_time: state.build_time,
    kind,
    payload,
    session_id: state.session_id,
  };
  if (state.cap_witness !== undefined) candidate.cap_witness = state.cap_witness;
  if (state.user_cap_hash !== undefined) candidate.user_cap_hash = state.user_cap_hash;

  const gated = applyGate(candidate, state.consent_tier);
  if (gated === null) return ''; // Σ-mask denied
  pushRing(gated);
  if (effectiveCount() >= FLUSH_AT) {
    void flush('size');
  }
  return cell_id;
}

// ─── ring-buffer ops ───────────────────────────────────────────────────────
function pushRing(ev: AkashicEvent): void {
  state.ring[state.ring_idx % RING_CAP] = ev;
  state.ring_idx = (state.ring_idx + 1) % (RING_CAP * 2); // avoid overflow ; mod-RING_CAP on read
}

function effectiveCount(): number {
  return Math.min(state.ring_idx, RING_CAP);
}

// Drain returns at-most-RING_CAP events in-FIFO-order, oldest first.
function drainRing(): AkashicEvent[] {
  const n = effectiveCount();
  if (n === 0) return [];
  const out: AkashicEvent[] = new Array(n);
  if (state.ring_idx < RING_CAP) {
    for (let i = 0; i < n; i++) out[i] = state.ring[i] as AkashicEvent;
  } else {
    // wrapped · oldest-first starts at ring_idx % RING_CAP
    const start = state.ring_idx % RING_CAP;
    for (let i = 0; i < n; i++) {
      out[i] = state.ring[(start + i) % RING_CAP] as AkashicEvent;
    }
  }
  state.ring_idx = 0;
  return out;
}

// ─── flush · POST batch to /api/akashic/batch ──────────────────────────────
export async function flush(reason: AkashicBatch['flush_reason'] = 'manual'): Promise<boolean> {
  if (!state.initialized) return false;
  const events = drainRing();
  if (events.length === 0) return true;
  const first = events[0];
  const last = events[events.length - 1];
  const batch_id = hash16(
    `${state.session_id}|${first?.cell_id ?? '0'}|${last?.cell_id ?? '0'}`
  );
  const batch: AkashicBatch = {
    batch_id,
    session_id: state.session_id,
    events,
    flush_reason: reason,
  };
  // Use sendBeacon when available (survives unload) ; fall through to fetch.
  if (
    reason === 'unload' &&
    typeof navigator !== 'undefined' &&
    navigator.sendBeacon !== undefined
  ) {
    try {
      const blob = new Blob([JSON.stringify(batch)], { type: 'application/json' });
      const ok = navigator.sendBeacon(state.endpoint_batch, blob);
      return ok;
    } catch {
      // fall through to fetch
    }
  }
  try {
    const r = await fetch(state.endpoint_batch, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify(batch),
      keepalive: reason === 'unload',
    });
    return r.ok;
  } catch {
    // network-fail · cells are lost (we already drained) ; better than backpressure.
    // TODO[mycelium] : optional offline-queue in IndexedDB for high-fidelity tier.
    return false;
  }
}

// ─── consent updates ───────────────────────────────────────────────────────
export function withConsent(tier: ConsentTier): void {
  if (!state.initialized) return;
  const prev = state.consent_tier;
  if (prev === tier) return;
  state.consent_tier = tier;
  persistTier(tier);
  // Revocation = downgrade ; granted = upgrade. consent.granted/revoked always
  // emit (gate-table allows them at all tiers).
  capture(
    tierRank(tier) > tierRank(prev) ? 'consent.granted' : 'consent.revoked',
    { from: prev, to: tier }
  );
  // Force-flush so the consent transition itself is durable even if user
  // closes the tab seconds after granting.
  void flush('manual');
}

function tierRank(t: ConsentTier): number {
  return t === 'akashic' ? 3 : t === 'mycelium' ? 2 : t === 'spore' ? 1 : 0;
}

export function currentTier(): ConsentTier {
  return state.consent_tier;
}

export function currentPolicy(): ConsentPolicy {
  return CONSENT_TIERS[state.consent_tier];
}

// ─── deploy-version drift detection ────────────────────────────────────────
// Polls /api/akashic/version every VERSION_PROBE_MS · if the server-reported
// dpl_id ≠ page_load_dpl_id, emit deploy.detected. This is the canary-pattern
// for stuck-deploys (the Vercel-stuck-deploy issue Apocky just hit).
export async function attestVersion(): Promise<boolean> {
  if (!state.initialized) return false;
  try {
    const r = await fetch(state.endpoint_version, { method: 'GET' });
    if (!r.ok) return false;
    const body = (await r.json()) as { dpl_id?: string; commit_sha?: string };
    const observed = body.dpl_id ?? 'unknown';
    if (observed !== 'unknown' && observed !== state.page_load_dpl_id) {
      capture('deploy.detected', {
        observed_dpl_id: observed,
        page_load_dpl_id: state.page_load_dpl_id,
        observed_commit_sha: body.commit_sha ?? 'unknown',
      });
    }
    state.current_dpl_id = observed;
    return true;
  } catch {
    return false;
  }
}

// ─── sovereign-purge ───────────────────────────────────────────────────────
// User invokes from /admin/telemetry "purge all my events" UI. Deletes every
// row in akashic_events whose user_cap_hash matches the supplied cap.
export async function purgeAllMine(cap_witness?: string): Promise<boolean> {
  if (!state.initialized) return false;
  const witness = cap_witness ?? state.cap_witness;
  if (witness === undefined) return false;
  capture('consent.purge_request', { witness_hash: hash16(witness) });
  await flush('manual'); // ensure the purge-request event lands BEFORE the purge
  try {
    const r = await fetch(state.endpoint_purge, {
      method: 'DELETE',
      headers: {
        'content-type': 'application/json',
        'x-akashic-cap-witness': witness,
      },
      body: JSON.stringify({
        session_id: state.session_id,
        user_cap_hash: state.user_cap_hash ?? hash16(witness),
      }),
    });
    return r.ok;
  } catch {
    return false;
  }
}

// ─── timers + unload ───────────────────────────────────────────────────────
function armFlushTimer(): void {
  if (state.flush_timer !== null) return;
  if (typeof window === 'undefined') return;
  const tick = (): void => {
    void flush('interval');
    state.flush_timer = setTimeout(tick, FLUSH_INTERVAL_MS);
  };
  state.flush_timer = setTimeout(tick, FLUSH_INTERVAL_MS);
}

function armVersionProbe(): void {
  if (state.version_timer !== null) return;
  if (typeof window === 'undefined') return;
  state.version_timer = setInterval(() => {
    void attestVersion();
  }, VERSION_PROBE_MS);
}

function attachUnloadFlush(): void {
  if (state.beforeunload_attached) return;
  if (typeof window === 'undefined') return;
  state.beforeunload_attached = true;
  // Use 'pagehide' over 'beforeunload' for mobile Safari reliability.
  window.addEventListener('pagehide', () => {
    capture('page.unload', { url: location.href });
    void flush('unload');
  });
}

function viewport(): { w: number; h: number } {
  if (typeof window === 'undefined') return { w: 0, h: 0 };
  return { w: window.innerWidth ?? 0, h: window.innerHeight ?? 0 };
}

// ─── test-only escape hatch ────────────────────────────────────────────────
export function _resetForTests(): void {
  if (state.flush_timer !== null) {
    clearTimeout(state.flush_timer);
    state.flush_timer = null;
  }
  if (state.version_timer !== null) {
    clearInterval(state.version_timer);
    state.version_timer = null;
  }
  state.initialized = false;
  state.consent_tier = 'spore';
  state.session_id = '';
  state.ring = [];
  state.ring_idx = 0;
  state.beforeunload_attached = false;
}

// ─── inspectors (used by AkashicConsent overlay + admin/telemetry page) ───
export function _peekRing(): AkashicEvent[] {
  return drainRing();
}

export function _ringSize(): number {
  return effectiveCount();
}

export function _sessionId(): string {
  return state.session_id;
}

export function _isInit(): boolean {
  return state.initialized;
}
