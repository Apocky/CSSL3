// § Akashic-Webpage-Records · akashic-telemetry.test.ts
// Framework-agnostic tests · run via `npx tsx tests/akashic/akashic-telemetry.test.ts`.
// No DOM required ; we shim window/localStorage/sessionStorage for the client.

// ─── DOM shim · minimal · just enough for client.ts ────────────────────────
function installDomShim(): void {
  const G = globalThis as unknown as {
    window?: unknown;
    document?: unknown;
    localStorage?: unknown;
    sessionStorage?: unknown;
    location?: unknown;
    navigator?: unknown;
  };
  if (G.window !== undefined) return;

  const storage = (): {
    items: Map<string, string>;
    failWrites: boolean;
    getItem: (k: string) => string | null;
    setItem: (k: string, v: string) => void;
    removeItem: (k: string) => void;
    clear: () => void;
  } => {
    const items = new Map<string, string>();
    return {
      items,
      failWrites: false,
      getItem: (k) => items.get(k) ?? null,
      setItem(k, v) {
        if (this.failWrites) throw new Error('storage write denied');
        items.set(k, String(v));
      },
      removeItem(k) {
        if (this.failWrites) throw new Error('storage write denied');
        items.delete(k);
      },
      clear() {
        if (this.failWrites) throw new Error('storage write denied');
        items.clear();
      },
    };
  };

  const ls = storage();
  const ss = storage();

  const listeners: Map<string, Array<(ev: unknown) => void>> = new Map();
  const win = {
    _listeners: listeners,
    innerWidth: 1024,
    innerHeight: 768,
    addEventListener: (kind: string, fn: (ev: unknown) => void) => {
      const arr = listeners.get(kind) ?? [];
      arr.push(fn);
      listeners.set(kind, arr);
    },
    removeEventListener: (kind: string, fn: (ev: unknown) => void) => {
      const arr = listeners.get(kind);
      if (arr === undefined) return;
      const i = arr.indexOf(fn);
      if (i >= 0) arr.splice(i, 1);
    },
    dispatchEvent: (ev: { type: string; [k: string]: unknown }) => {
      const arr = listeners.get(ev.type);
      if (arr === undefined) return false;
      for (const fn of arr) fn(ev);
      return true;
    },
    fetch: async (_url: string, _init?: unknown): Promise<Response> => {
      // default-deny ; tests override per-case
      return new Response(JSON.stringify({ ok: true }), { status: 200 }) as unknown as Response;
    },
  };

  // Some Node versions install navigator as a getter-only property ; use
  // defineProperty for any name that may conflict ; plain assignment for the
  // rest. For our purposes we only need the names below to be readable.
  const define = (name: string, value: unknown): void => {
    try {
      Object.defineProperty(globalThis, name, {
        value,
        writable: true,
        configurable: true,
      });
    } catch {
      (globalThis as Record<string, unknown>)[name] = value;
    }
  };
  define('window', win);
  define('document', {
    addEventListener: win.addEventListener,
    removeEventListener: win.removeEventListener,
    visibilityState: 'visible',
    referrer: '',
  });
  define('localStorage', ls);
  define('sessionStorage', ss);
  define('location', { href: 'https://apocky.com/test', pathname: '/test' });
  define('navigator', { sendBeacon: undefined, connection: { effectiveType: '4g' } });
  const performanceObservers: Array<{ disconnected: boolean }> = [];
  class FakePerformanceObserver {
    disconnected = false;
    constructor(_callback: (list: { getEntries: () => unknown[] }) => void) {
      performanceObservers.push(this);
    }
    observe(_options: { entryTypes: string[] }): void { /* observed by instance count */ }
    disconnect(): void { this.disconnected = true; }
  }
  define('PerformanceObserver', FakePerformanceObserver);
  define('__akashicTestPerformanceObservers', performanceObservers);
  // silence the unused-state warning
  void G;
}

installDomShim();

// ─── imports must follow shim ──────────────────────────────────────────────
import {
  init,
  akashicInstall,
  akashicDisable,
  capture,
  flush,
  purgeAllMine,
  withConsent,
  currentTier,
  hash16,
  _resetForTests,
  _ringSize,
  _sessionId,
  _isInit,
  _peekRing,
  CONSENT_TIERS,
  SIGMA_NONE,
  applyGate,
  redactString,
  redactPayload,
  gateEvent,
  installPerformanceObservers,
  installNetworkTap,
  installConsoleTap,
  CONSENT_STORAGE_KEY,
  isTelemetryBlackoutPath,
} from '@/lib/akashic-telemetry';
import { clusterSignature } from '@/lib/akashic-telemetry/error-boundary';

function assert(cond: boolean, msg: string): void {
  if (!cond) throw new Error(`assert failed : ${msg}`);
}

type TestStorage = Storage & { failWrites: boolean; items: Map<string, string> };
type TestWindow = Window & {
  _listeners: Map<string, Array<(event: unknown) => void>>;
};

function listenerCount(kind: string): number {
  return ((window as unknown as TestWindow)._listeners.get(kind) ?? []).length;
}

function performanceObserverReceipts(): Array<{ disconnected: boolean }> {
  return (globalThis as unknown as {
    __akashicTestPerformanceObservers: Array<{ disconnected: boolean }>;
  }).__akashicTestPerformanceObservers;
}

function setPath(pathname: string): void {
  const loc = globalThis.location as unknown as { href: string; pathname: string };
  loc.pathname = pathname;
  loc.href = `https://apocky.com${pathname}`;
}

function resetLifecycleHarness(): void {
  (localStorage as TestStorage).failWrites = false;
  (sessionStorage as TestStorage).failWrites = false;
  akashicDisable();
  _resetForTests();
  localStorage.clear();
  sessionStorage.clear();
  setPath('/test');
}

// ─── tests ─────────────────────────────────────────────────────────────────

export function test_init_idempotent(): void {
  _resetForTests();
  localStorage.setItem('akashic.consent.tier.v1', 'spore');
  const first = init({ install_observers: false });
  const second = init({ install_observers: false });
  assert(first === true, 'first init returns true');
  assert(second === false, 'second init returns false (idempotent)');
  assert(_isInit() === true, 'state.initialized after init');
}

export async function test_clinical_path_is_telemetry_blackout(): Promise<void> {
  _resetForTests();
  localStorage.setItem('akashic.consent.tier.v1', 'spore');
  const loc = globalThis.location as unknown as { href: string; pathname: string };
  const previous = { ...loc };
  loc.href = 'https://apocky.com/shawn/clinical';
  loc.pathname = '/shawn/clinical';
  try {
    assert(init({ install_observers: false }) === false, 'clinical route refuses init');
    assert(_isInit() === false, 'clinical route leaves telemetry uninitialized');
    assert(capture('page.view', { url: loc.href }) === '', 'clinical capture is denied');
    assert(_ringSize() === 0, 'clinical capture leaves no buffered event');
    assert((await flush('manual')) === false, 'clinical route refuses network flush');
  } finally {
    loc.href = previous.href;
    loc.pathname = previous.pathname;
  }
}

export function test_first_visit_is_zero_effect_none(): void {
  _resetForTests();
  localStorage.clear();
  sessionStorage.clear();
  assert(init({ install_observers: false }) === false, 'init refuses to start before a choice');
  assert(_isInit() === false, 'telemetry remains uninitialized before a choice');
  assert(currentTier() === 'none', 'first-visit effective tier is none');
  assert(_sessionId() === '', 'no session id is created before a choice');
  assert(sessionStorage.getItem('akashic.session.id.v1') === null, 'no session storage is created');
  assert(_ringSize() === 0, 'no page-view or other event is buffered');
}

export function test_capture_gates_via_consent(): void {
  _resetForTests();
  localStorage.setItem('akashic.consent.tier.v1', 'spore');
  init({ install_observers: false });
  withConsent('none');
  assert(_isInit() === false, 'revocation stops the telemetry client');
  assert(_sessionId() === '', 'revocation clears the telemetry session');
  const cell = capture('perf.lcp', { value: 1234 });
  assert(cell === '', 'perf.lcp denied at none-tier');
  assert(_ringSize() === 0, 'ring stays empty after denied capture');
}

export function test_none_disables_all_emissions(): void {
  _resetForTests();
  withConsent('none');
  const cell = capture('consent.granted', { from: 'none', to: 'akashic' });
  assert(cell === '', 'none-tier does not emit consent bookkeeping');
  assert(_ringSize() === 0, 'none-tier buffers no bookkeeping event');
}

export function test_public_install_is_zero_effect_before_choice(): void {
  resetLifecycleHarness();
  const originalFetch = window.fetch;
  const originalError = console.error;
  const perfBefore = performanceObserverReceipts().length;
  const pagehideBefore = listenerCount('pagehide');

  installPerformanceObservers();
  installNetworkTap();
  installConsoleTap();
  assert(window.fetch === originalFetch, 'raw network installer is guarded before consent');
  assert(console.error === originalError, 'raw console installer is guarded before consent');
  assert(performanceObserverReceipts().length === perfBefore, 'raw performance installer is guarded');
  assert(akashicInstall() === false, 'public installer refuses first visit');
  assert(_isInit() === false, 'public installer leaves client stopped');
  assert(_sessionId() === '', 'public installer creates no session id');
  assert(_ringSize() === 0, 'public installer creates no event');
  assert(window.fetch === originalFetch, 'public installer creates no network observer');
  assert(listenerCount('pagehide') === pagehideBefore, 'public installer creates no page observer');
  assert(listenerCount('storage') === 0, 'public installer creates no consent listener before choice');
}

export function test_revoke_uninstalls_every_owned_effect(): void {
  resetLifecycleHarness();
  localStorage.setItem(CONSENT_STORAGE_KEY, 'akashic');
  const originalFetch = window.fetch;
  const originalError = console.error;
  const perfBefore = performanceObserverReceipts().length;
  assert(akashicInstall() === true, 'positive choice activates public installer');
  const installedObservers = performanceObserverReceipts().slice(perfBefore);
  assert(installedObservers.length > 0, 'performance observers installed after consent');
  assert(window.fetch !== originalFetch, 'network tap installed after consent');
  assert(console.error !== originalError, 'console tap installed at akashic tier');
  assert(listenerCount('storage') === 1, 'cross-tab revoke listener installed while active');
  assert(_sessionId().length === 16, 'positive choice creates tab session');

  assert(withConsent('none') === true, 'explicit none preference persisted');
  assert(_isInit() === false, 'revocation stops client synchronously');
  assert(_sessionId() === '', 'revocation clears client session');
  assert(sessionStorage.getItem('akashic.session.id.v1') === null, 'revocation clears session storage');
  assert(_ringSize() === 0, 'revocation discards buffered events without emission');
  assert(window.fetch === originalFetch, 'revocation restores fetch by identity');
  assert(console.error === originalError, 'revocation restores console by identity');
  assert(installedObservers.every((observer) => observer.disconnected), 'revocation disconnects every observer');
  assert(listenerCount('storage') === 0, 'revocation removes cross-tab listener');
}

export function test_auth_blackout_suspends_prior_consent(): void {
  resetLifecycleHarness();
  localStorage.setItem(CONSENT_STORAGE_KEY, 'akashic');
  const originalFetch = window.fetch;
  assert(akashicInstall() === true, 'prior consent activates on ordinary route');
  assert(isTelemetryBlackoutPath('/auth/callback?code=SENTINEL_CODE'), 'callback query normalizes to blackout');
  setPath('/auth/callback');
  (globalThis.location as unknown as { href: string }).href =
    'https://apocky.com/auth/callback?code=SENTINEL_CODE#access_token=SENTINEL_TOKEN';
  assert(akashicInstall() === false, 'public installer suspends on auth callback');
  assert(_isInit() === false, 'callback leaves telemetry stopped');
  assert(window.fetch === originalFetch, 'callback restores network tap');
  assert(capture('page.error', { message: 'SENTINEL_CODE SENTINEL_TOKEN' }) === '', 'callback sentinel denied');
  assert(_ringSize() === 0, 'callback sentinel never enters ring');
  assert(isTelemetryBlackoutPath('/brain?memory=PRIVATE_SENTINEL'), 'private Brain query normalizes to blackout');
  assert(isTelemetryBlackoutPath('/apocrypha?memory=PRIVATE_SENTINEL'), 'primary private Apocrypha query normalizes to blackout');
  setPath('/brain');
  assert(akashicInstall() === false, 'private Brain keeps telemetry suspended');
  assert(capture('page.view', { memory: 'PRIVATE_SENTINEL' }) === '', 'private Brain payload cannot enter telemetry');
  assert(_ringSize() === 0, 'private Brain leaves no buffered event');
}

export function test_cross_tab_none_stops_active_tab(): void {
  resetLifecycleHarness();
  localStorage.setItem(CONSENT_STORAGE_KEY, 'spore');
  assert(akashicInstall() === true, 'spore activates before cross-tab revoke');
  localStorage.setItem(CONSENT_STORAGE_KEY, 'none');
  (window as unknown as { dispatchEvent: (event: { type: string; key: string }) => boolean })
    .dispatchEvent({ type: 'storage', key: CONSENT_STORAGE_KEY });
  assert(_isInit() === false, 'cross-tab none stops client');
  assert(currentTier() === 'none', 'cross-tab none becomes effective tier');
  assert(_sessionId() === '', 'cross-tab none clears session');
}

export function test_failed_storage_write_cannot_escalate_or_reactivate(): void {
  resetLifecycleHarness();
  localStorage.setItem(CONSENT_STORAGE_KEY, 'spore');
  assert(akashicInstall({ install_observers: false }) === true, 'spore client active');
  _peekRing();
  const storage = localStorage as TestStorage;
  storage.failWrites = true;
  assert(withConsent('mycelium') === false, 'failed tier write reports failure');
  assert(currentTier() === 'spore', 'failed tier write retains prior tier');
  assert(_ringSize() === 0, 'failed tier write emits no consent event');
  assert(withConsent('none') === false, 'failed none write reports failure');
  assert(_isInit() === false, 'failed none write still stops this tab');
  assert(currentTier() === 'none', 'failed none write is fail-closed in this tab');
  assert(akashicInstall({ install_observers: false }) === false, 'stored prior consent cannot reactivate tab');
  storage.failWrites = false;
}

export async function test_revoke_aborts_inflight_purge_sequence(): Promise<void> {
  resetLifecycleHarness();
  localStorage.setItem(CONSENT_STORAGE_KEY, 'spore');
  assert(
    akashicInstall({ install_observers: false, cap_witness: 'test-witness' }) === true,
    'purge test client active',
  );
  _peekRing();
  const originalFetch = globalThis.fetch;
  let resolveFetch: ((response: Response) => void) | null = null;
  let observedSignal: AbortSignal | undefined;
  let fetchCalls = 0;
  globalThis.fetch = (async (_input: RequestInfo | URL, init?: RequestInit): Promise<Response> => {
    fetchCalls += 1;
    observedSignal = init?.signal ?? undefined;
    return await new Promise<Response>((resolve) => { resolveFetch = resolve; });
  }) as typeof fetch;
  try {
    const purge = purgeAllMine();
    await Promise.resolve();
    assert(fetchCalls === 1, 'purge sequence started only its consent flush');
    assert(observedSignal !== undefined, 'in-flight flush has an abort signal');
    withConsent('none');
    assert(observedSignal?.aborted === true, 'revocation aborts in-flight flush');
    const resolver = resolveFetch as ((response: Response) => void) | null;
    if (resolver === null) throw new Error('assert failed : deferred fetch resolver installed');
    resolver(new Response(JSON.stringify({ ok: true }), { status: 200 }));
    assert((await purge) === false, 'purge stops after lifecycle generation changes');
    assert(fetchCalls === 1, 'purge DELETE is not sent after revocation');
  } finally {
    globalThis.fetch = originalFetch;
  }
}

export function test_redaction_email_jwt_digits(): void {
  const s = 'user@example.com login token eyJabcdefgh.ijklmnop.qrstuvwxyz cc 4111111111111111';
  const r = redactString(s);
  assert(r.includes('«email»'), 'email redacted');
  assert(r.includes('«jwt»'), 'jwt redacted');
  assert(r.includes('«num»'), 'long-digits redacted');
  assert(!r.includes('user@example.com'), 'no raw email');
  assert(!r.includes('4111111111111111'), 'no raw cc');
}

export function test_redaction_query_secrets(): void {
  const s = 'GET /api/x?token=ABCXYZ&public=ok';
  const r = redactString(s);
  assert(r.includes('«redacted»'), 'query secret redacted');
  assert(!r.includes('ABCXYZ'), 'no raw secret in output');
  assert(r.includes('public=ok'), 'non-secret query preserved');
}

export function test_redact_payload_deep(): void {
  const p = { msg: 'a@b.com', nested: { x: 'eyJabcdefgh.ijklmnop.qrstuvwxyz' }, arr: ['4111111111111111'] };
  const r = redactPayload(p) as Record<string, unknown>;
  assert((r['msg'] as string).includes('«email»'), 'top-level redacted');
  assert(((r['nested'] as Record<string, unknown>)['x'] as string).includes('«jwt»'), 'nested redacted');
  assert(((r['arr'] as string[])[0] ?? '').includes('«num»'), 'array elem redacted');
}

export function test_gate_kind_required_tier(): void {
  // page.view requires spore-tier ; not emitted at none.
  assert(gateEvent('page.view', 'none') === SIGMA_NONE, 'page.view denied at none');
  assert(gateEvent('page.view', 'spore') !== SIGMA_NONE, 'page.view allowed at spore');
  // react.error requires mycelium-tier ; not emitted at spore.
  assert(gateEvent('react.error', 'spore') === SIGMA_NONE, 'react.error denied at spore');
  assert(gateEvent('react.error', 'mycelium') !== SIGMA_NONE, 'react.error allowed at mycelium');
  // console.error requires akashic-tier.
  assert(gateEvent('console.error', 'mycelium') === SIGMA_NONE, 'console.error denied at mycelium');
  assert(gateEvent('console.error', 'akashic') !== SIGMA_NONE, 'console.error allowed at akashic');
  assert(gateEvent('consent.granted', 'none') === SIGMA_NONE, 'consent.granted denied at none');
  assert(gateEvent('consent.granted', 'spore') !== SIGMA_NONE, 'consent.granted allowed after opt-in');
}

export function test_apply_gate_returns_null_on_deny(): void {
  const ev = {
    cell_id: 'abcd1234',
    ts_iso: new Date().toISOString(),
    sigma_mask: 0,
    dpl_id: 'x', commit_sha: 'y', build_time: 'z',
    kind: 'react.error' as const,
    payload: { message: 'boom' },
    session_id: '0123456789abcdef',
  };
  assert(applyGate(ev, 'none') === null, 'react.error denied at none');
  assert(applyGate(ev, 'spore') === null, 'react.error denied at spore');
  const allowed = applyGate(ev, 'mycelium');
  assert(allowed !== null, 'react.error allowed at mycelium');
  assert(allowed?.sigma_mask !== 0, 'allowed event has nonzero mask');
}

export function test_spore_strips_stack_details(): void {
  const ev = {
    cell_id: 'abcd1234',
    ts_iso: new Date().toISOString(),
    sigma_mask: 0,
    dpl_id: 'x', commit_sha: 'y', build_time: 'z',
    kind: 'page.error' as const,
    payload: { message: 'boom', stack: 'SENTINEL_STACK', component_stack: 'SENTINEL_COMPONENT' },
    session_id: '0123456789abcdef',
  };
  const spore = applyGate(ev, 'spore');
  const mycelium = applyGate(ev, 'mycelium');
  assert(spore !== null, 'page.error is eligible at spore');
  assert(spore?.payload['stack'] === undefined, 'spore strips stack');
  assert(spore?.payload['component_stack'] === undefined, 'spore strips component stack');
  assert(mycelium?.payload['stack'] === 'SENTINEL_STACK', 'mycelium retains disclosed stack');
}

export function test_consent_tiers_table(): void {
  assert(CONSENT_TIERS.spore.k_anon === 10, 'spore k-anon=10');
  assert(CONSENT_TIERS.mycelium.k_anon === 5, 'mycelium k-anon=5');
  assert(CONSENT_TIERS.akashic.k_anon === 5, 'akashic k-anon=5');
  assert(CONSENT_TIERS.none.k_anon === Number.POSITIVE_INFINITY, 'none k-anon=∞');
  assert(CONSENT_TIERS.akashic.capture_console === true, 'akashic captures console');
  assert(CONSENT_TIERS.spore.capture_console === false, 'spore does NOT capture console');
}

export function test_hash16_deterministic(): void {
  const a = hash16('hello world');
  const b = hash16('hello world');
  const c = hash16('hello worlD');
  assert(a === b, 'hash16 deterministic');
  assert(a !== c, 'hash16 sensitive to input');
  assert(a.length === 16, 'hash16 yields 16-char output');
}

export function test_cluster_signature_normalizes(): void {
  const stack1 = `Error: boom
    at Foo (https://apocky.com/_next/abc.js:42:13)
    at Bar (https://apocky.com/_next/abc.js:50:5)`;
  const stack2 = `Error: boom
    at Foo (https://apocky.com/_next/abc.js:42:99)
    at Bar (https://apocky.com/_next/abc.js:50:7)`;
  const a = clusterSignature(stack1, 'test');
  const b = clusterSignature(stack2, 'test');
  assert(a === b, 'col-numbers ignored ; same cluster');
  assert(a.length === 16, 'cluster_signature is 16-char');
}

export async function test_flush_drains_ring(): Promise<void> {
  _resetForTests();
  localStorage.setItem('akashic.consent.tier.v1', 'spore');
  init({ install_observers: false });
  capture('perf.lcp', { value: 1000, url: 'x', viewport: { w: 1, h: 1 } });
  capture('perf.fcp', { value: 500, url: 'x', viewport: { w: 1, h: 1 } });
  assert(_ringSize() >= 2, 'ring has events');
  // Stub fetch to count flushes.
  let flushed = 0;
  const G = globalThis as unknown as { window?: { fetch?: (...a: unknown[]) => Promise<Response> } };
  if (G.window !== undefined) {
    G.window.fetch = async (_url: unknown, _init: unknown): Promise<Response> => {
      flushed++;
      return new Response(JSON.stringify({ ok: true }), { status: 200 }) as unknown as Response;
    };
  }
  // Also patch global fetch (some envs).
  const Gx = globalThis as unknown as { fetch?: (...a: unknown[]) => Promise<Response> };
  Gx.fetch = async (_url: unknown, _init: unknown): Promise<Response> => {
    flushed++;
    return new Response(JSON.stringify({ ok: true }), { status: 200 }) as unknown as Response;
  };
  const ok = await flush('manual');
  assert(ok === true, 'flush returned ok');
  assert(_ringSize() === 0, 'ring empty after flush');
  assert(flushed >= 1, 'fetch called at least once');
}

// ─── runner ────────────────────────────────────────────────────────────────

declare const require: { main?: unknown } | undefined;
declare const module: { id?: string } | undefined;
const isMain =
  typeof require !== 'undefined' &&
  typeof module !== 'undefined' &&
  require.main === module;

if (isMain) {
  Promise.resolve()
    .then(test_init_idempotent)
    .then(test_clinical_path_is_telemetry_blackout)
    .then(test_first_visit_is_zero_effect_none)
    .then(test_capture_gates_via_consent)
    .then(test_none_disables_all_emissions)
    .then(test_public_install_is_zero_effect_before_choice)
    .then(test_revoke_uninstalls_every_owned_effect)
    .then(test_auth_blackout_suspends_prior_consent)
    .then(test_cross_tab_none_stops_active_tab)
    .then(test_failed_storage_write_cannot_escalate_or_reactivate)
    .then(test_revoke_aborts_inflight_purge_sequence)
    .then(test_redaction_email_jwt_digits)
    .then(test_redaction_query_secrets)
    .then(test_redact_payload_deep)
    .then(test_gate_kind_required_tier)
    .then(test_apply_gate_returns_null_on_deny)
    .then(test_spore_strips_stack_details)
    .then(test_consent_tiers_table)
    .then(test_hash16_deterministic)
    .then(test_cluster_signature_normalizes)
    .then(test_flush_drains_ring)
    .then(() => {
      // eslint-disable-next-line no-console
      console.log('akashic-telemetry.test : OK · 21 inline tests passed');
    })
    .catch((err) => {
      // eslint-disable-next-line no-console
      console.error(err);
      process.exit(1);
    });
}
