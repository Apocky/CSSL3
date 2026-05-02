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
    getItem: (k: string) => string | null;
    setItem: (k: string, v: string) => void;
    removeItem: (k: string) => void;
    clear: () => void;
  } => {
    const items = new Map<string, string>();
    return {
      items,
      getItem: (k) => items.get(k) ?? null,
      setItem: (k, v) => { items.set(k, String(v)); },
      removeItem: (k) => { items.delete(k); },
      clear: () => items.clear(),
    };
  };

  const ls = storage();
  const ss = storage();

  const listeners: Map<string, Array<(ev: unknown) => void>> = new Map();
  const win = {
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
  define('document', { addEventListener: win.addEventListener, visibilityState: 'visible', referrer: '' });
  define('localStorage', ls);
  define('sessionStorage', ss);
  define('location', { href: 'https://apocky.com/test' });
  define('navigator', { sendBeacon: undefined, connection: { effectiveType: '4g' } });
  // silence the unused-state warning
  void G;
}

installDomShim();

// ─── imports must follow shim ──────────────────────────────────────────────
import {
  init,
  capture,
  flush,
  withConsent,
  currentTier,
  hash16,
  _resetForTests,
  _ringSize,
  _isInit,
  _peekRing,
  CONSENT_TIERS,
  SIGMA_NONE,
  applyGate,
  redactString,
  redactPayload,
  gateEvent,
} from '@/lib/akashic-telemetry';
import { clusterSignature } from '@/lib/akashic-telemetry/error-boundary';

function assert(cond: boolean, msg: string): void {
  if (!cond) throw new Error(`assert failed : ${msg}`);
}

// ─── tests ─────────────────────────────────────────────────────────────────

export function test_init_idempotent(): void {
  _resetForTests();
  const first = init({ install_observers: false });
  const second = init({ install_observers: false });
  assert(first === true, 'first init returns true');
  assert(second === false, 'second init returns false (idempotent)');
  assert(_isInit() === true, 'state.initialized after init');
}

export function test_consent_default_spore(): void {
  _resetForTests();
  init({ install_observers: false });
  // page.view stamped at init
  assert(_ringSize() >= 1, 'page.view captured at init');
  assert(currentTier() === 'spore', 'default tier is spore');
}

export function test_capture_gates_via_consent(): void {
  _resetForTests();
  init({ install_observers: false });
  withConsent('none');
  // Drain ring to ignore anything from init/withConsent.
  _peekRing();
  const cell = capture('perf.lcp', { value: 1234 });
  assert(cell === '', 'perf.lcp denied at none-tier');
  assert(_ringSize() === 0, 'ring stays empty after denied capture');
}

export function test_capture_allows_consent_at_none(): void {
  // consent.granted/.revoked must always emit even at none-tier (sovereignty
  // self-witness). Verifies KIND_REQUIRED_TIER for these is 'none'.
  _resetForTests();
  init({ install_observers: false });
  withConsent('none');
  _peekRing();
  const cell = capture('consent.granted', { from: 'none', to: 'akashic' });
  assert(cell !== '', 'consent.granted always emits');
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
  // consent.granted always allowed.
  assert(gateEvent('consent.granted', 'none') !== SIGMA_NONE, 'consent.granted allowed at none');
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
  // Clear stored tier so this test runs at default spore (prior test set 'none').
  try {
    if (typeof localStorage !== 'undefined') localStorage.clear();
  } catch { /* ignore */ }
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
    .then(test_consent_default_spore)
    .then(test_capture_gates_via_consent)
    .then(test_capture_allows_consent_at_none)
    .then(test_redaction_email_jwt_digits)
    .then(test_redaction_query_secrets)
    .then(test_redact_payload_deep)
    .then(test_gate_kind_required_tier)
    .then(test_apply_gate_returns_null_on_deny)
    .then(test_consent_tiers_table)
    .then(test_hash16_deterministic)
    .then(test_cluster_signature_normalizes)
    .then(test_flush_drains_ring)
    .then(() => {
      // eslint-disable-next-line no-console
      console.log('akashic-telemetry.test : OK · 13 inline tests passed');
    })
    .catch((err) => {
      // eslint-disable-next-line no-console
      console.error(err);
      process.exit(1);
    });
}
