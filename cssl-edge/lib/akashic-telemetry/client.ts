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
export const CONSENT_STORAGE_KEY = 'akashic.consent.tier.v1';
const SESSION_KEY = 'akashic.session.id.v1';
export const CONSENT_CHANGE_EVENT = 'akashic:consent-change';

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
  lifecycle_generation: number;
  request_controllers: Set<AbortController>;
}

const state: AkashicState = {
  initialized: false,
  consent_tier: 'none',
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
  lifecycle_generation: 0,
  request_controllers: new Set(),
};

let lifecycle_reconciler: (() => void) | null = null;
let runtime_forced_none = false;

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

function beginRequest(): AbortController | null {
  if (typeof AbortController === 'undefined') return null;
  const controller = new AbortController();
  state.request_controllers.add(controller);
  return controller;
}

function finishRequest(controller: AbortController | null): void {
  if (controller !== null) state.request_controllers.delete(controller);
}

// Hard privacy boundaries. Capture can remain installed briefly across a
// client-side transition, so every effectful entry point fails closed here.
export function isTelemetryBlackoutPath(rawPath?: string): boolean {
  let path = rawPath;
  if (path === undefined) {
    path =
      typeof location !== 'undefined' && typeof location.pathname === 'string'
        ? location.pathname
        : '';
  }
  try {
    if (/^[a-z][a-z\d+.-]*:\/\//i.test(path)) {
      path = new URL(path).pathname;
    }
  } catch {
    return true;
  }
  const pathname = (path.split(/[?#]/, 1)[0] ?? '').replace(/\/+$/, '') || '/';
  return (
    pathname === '/login' ||
    pathname === '/register' ||
    pathname === '/auth' ||
    pathname.startsWith('/auth/') ||
    pathname === '/brain' ||
    pathname.startsWith('/brain/') ||
    pathname === '/shawn/clinical' ||
    pathname.startsWith('/shawn/clinical/')
  );
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

// A missing/invalid value means no explicit choice. Reading this key does not
// initialize telemetry or create storage, a session, observers, or events.
export function storedConsentTier(): ConsentTier | null {
  try {
    if (typeof localStorage !== 'undefined') {
      const v = localStorage.getItem(CONSENT_STORAGE_KEY);
      if (v === 'none' || v === 'spore' || v === 'mycelium' || v === 'akashic') {
        return v;
      }
    }
  } catch {
    // ignore
  }
  return null;
}

function persistTier(tier: ConsentTier): void {
  try {
    if (typeof localStorage !== 'undefined') {
      localStorage.setItem(CONSENT_STORAGE_KEY, tier);
    }
  } catch {
    // ignore
  }
}

function clearSessionId(): void {
  try {
    if (typeof sessionStorage !== 'undefined') sessionStorage.removeItem(SESSION_KEY);
  } catch {
    // ignore
  }
}

function notifyLifecycleChange(): void {
  lifecycle_reconciler?.();
  try {
    if (typeof window !== 'undefined') {
      window.dispatchEvent(new Event(CONSENT_CHANGE_EVENT));
    }
  } catch {
    // A telemetry preference must never break the page.
  }
}

// Internal barrel hook: keeps observer installation/removal synchronous with
// preference changes without creating a client -> installer import cycle.
export function _registerLifecycleReconciler(reconciler: (() => void) | null): void {
  lifecycle_reconciler = reconciler;
}

// Reconcile a consent-key mutation made by another tab. This never writes the
// key or emits a consent event in the observing tab.
export function syncConsentFromStorage(): void {
  const stored = storedConsentTier();
  runtime_forced_none = stored === null || stored === 'none';
  state.consent_tier = stored ?? 'none';
  if (stored === null || stored === 'none') shutdown();
  notifyLifecycleChange();
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

function pageRoute(rawRoute?: string): string {
  const candidate = rawRoute
    ?? (typeof location !== 'undefined' ? location.pathname : '/');
  try {
    const pathname = new URL(
      candidate,
      typeof location !== 'undefined' && typeof location.href === 'string'
        ? location.href
        : 'https://apocky.com',
    ).pathname;
    return pathname.slice(0, 256) || '/';
  } catch {
    return '/';
  }
}

export function capturePageView(
  rawRoute?: string,
  navigation: 'document' | 'client' = 'document',
): string {
  return capture('page.view', {
    route: pageRoute(rawRoute),
    navigation,
    viewport: viewport(),
  });
}

// Idempotent · safe to call repeatedly. Returns false if already initialized.
export function init(opts: InitOpts = {}): boolean {
  if (isTelemetryBlackoutPath()) {
    shutdown();
    return false;
  }
  if (state.initialized) return false;
  if (runtime_forced_none) {
    state.consent_tier = 'none';
    return false;
  }
  const storedTier = storedConsentTier();
  if (storedTier === null || storedTier === 'none') {
    state.consent_tier = 'none';
    return false;
  }
  state.initialized = true;
  state.consent_tier = storedTier;
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
  capturePageView(undefined, 'document');

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
  if (isTelemetryBlackoutPath()) return '';
  if (!state.initialized) return '';
  if (state.consent_tier === 'none') return '';
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
  if (isTelemetryBlackoutPath()) return false;
  if (!state.initialized) return false;
  if (state.consent_tier === 'none') return false;
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
  const generation = state.lifecycle_generation;
  const controller = beginRequest();
  try {
    const r = await fetch(state.endpoint_batch, {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify(batch),
      keepalive: reason === 'unload',
      ...(controller !== null ? { signal: controller.signal } : {}),
    });
    return generation === state.lifecycle_generation && r.ok;
  } catch {
    // network-fail · cells are lost (we already drained) ; better than backpressure.
    // TODO[mycelium] : optional offline-queue in IndexedDB for high-fidelity tier.
    return false;
  } finally {
    finishRequest(controller);
  }
}

// ─── consent updates ───────────────────────────────────────────────────────
export function withConsent(tier: ConsentTier): boolean {
  const prev = currentTier();
  const wasInitialized = state.initialized;
  persistTier(tier);
  const persisted = storedConsentTier() === tier;

  if (!persisted) {
    if (tier === 'none') {
      runtime_forced_none = true;
      state.consent_tier = 'none';
      shutdown();
      state.consent_tier = 'none';
      notifyLifecycleChange();
    } else {
      state.consent_tier = prev;
    }
    return false;
  }

  runtime_forced_none = tier === 'none';
  state.consent_tier = tier;

  // None is an immediate local stop: discard buffered telemetry, remove the
  // tab identifier, and cancel client-owned timers/listeners before notifying
  // observer installers. Revocation itself is not emitted.
  if (tier === 'none') {
    shutdown();
    notifyLifecycleChange();
    return persisted;
  }

  notifyLifecycleChange();
  if (prev !== tier && state.initialized) {
    capture(
      tierRank(tier) > tierRank(prev) ? 'consent.granted' : 'consent.revoked',
      { from: prev, to: tier, resumed: !wasInitialized }
    );
    void flush('manual');
  }
  return persisted;
}

function tierRank(t: ConsentTier): number {
  return t === 'akashic' ? 3 : t === 'mycelium' ? 2 : t === 'spore' ? 1 : 0;
}

export function currentTier(): ConsentTier {
  if (runtime_forced_none) return 'none';
  return state.initialized ? state.consent_tier : (storedConsentTier() ?? 'none');
}

export function currentPolicy(): ConsentPolicy {
  return CONSENT_TIERS[state.consent_tier];
}

// ─── deploy-version drift detection ────────────────────────────────────────
// Polls /api/akashic/version every VERSION_PROBE_MS · if the server-reported
// dpl_id ≠ page_load_dpl_id, emit deploy.detected. This is the canary-pattern
// for stuck-deploys (the Vercel-stuck-deploy issue Apocky just hit).
export async function attestVersion(): Promise<boolean> {
  if (isTelemetryBlackoutPath()) return false;
  if (!state.initialized) return false;
  const generation = state.lifecycle_generation;
  const controller = beginRequest();
  try {
    const r = await fetch(state.endpoint_version, {
      method: 'GET',
      ...(controller !== null ? { signal: controller.signal } : {}),
    });
    if (generation !== state.lifecycle_generation) return false;
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
  } finally {
    finishRequest(controller);
  }
}

// ─── sovereign-purge ───────────────────────────────────────────────────────
// Deletes rows tied to a validated user-cap witness when an authorized caller
// explicitly invokes this API.
export async function purgeAllMine(cap_witness?: string): Promise<boolean> {
  if (isTelemetryBlackoutPath()) return false;
  if (!state.initialized || state.consent_tier === 'none') return false;
  const witness = cap_witness ?? state.cap_witness;
  if (witness === undefined) return false;
  const generation = state.lifecycle_generation;
  capture('consent.purge_request', { witness_hash: hash16(witness) });
  await flush('manual'); // ensure the purge-request event lands BEFORE the purge
  if (
    generation !== state.lifecycle_generation ||
    !state.initialized ||
    currentTier() === 'none' ||
    isTelemetryBlackoutPath()
  ) return false;
  const controller = beginRequest();
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
      ...(controller !== null ? { signal: controller.signal } : {}),
    });
    return generation === state.lifecycle_generation && r.ok;
  } catch {
    return false;
  } finally {
    finishRequest(controller);
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
  window.addEventListener('pagehide', handlePageHide);
}

function handlePageHide(): void {
  capture('page.unload', { url: location.href });
  void flush('unload');
}

function detachUnloadFlush(): void {
  if (!state.beforeunload_attached) return;
  if (typeof window !== 'undefined') window.removeEventListener('pagehide', handlePageHide);
  state.beforeunload_attached = false;
}

function viewport(): { w: number; h: number } {
  if (typeof window === 'undefined') return { w: 0, h: 0 };
  return { w: window.innerWidth ?? 0, h: window.innerHeight ?? 0 };
}

// Stop all client-owned effects without changing the user's stored choice.
// Used for auth/clinical blackout transitions as well as explicit revocation.
export function shutdown(): void {
  state.lifecycle_generation += 1;
  for (const controller of state.request_controllers) controller.abort();
  state.request_controllers.clear();
  if (state.flush_timer !== null) {
    clearTimeout(state.flush_timer);
    state.flush_timer = null;
  }
  if (state.version_timer !== null) {
    clearInterval(state.version_timer);
    state.version_timer = null;
  }
  detachUnloadFlush();
  state.initialized = false;
  state.consent_tier = runtime_forced_none ? 'none' : (storedConsentTier() ?? 'none');
  state.session_id = '';
  clearSessionId();
  state.ring = [];
  state.ring_idx = 0;
  state.user_cap_hash = undefined;
  state.cap_witness = undefined;
}

// ─── test-only escape hatch ────────────────────────────────────────────────
export function _resetForTests(): void {
  shutdown();
  runtime_forced_none = false;
  state.consent_tier = 'none';
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
