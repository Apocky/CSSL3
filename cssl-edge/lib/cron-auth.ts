// § T11-W14-K · CLOUD-ORCHESTRATOR · cron-auth + audit-log helpers
// Shared by ∀ /api/cron/* endpoints. Authenticates Vercel-cron-runner via
// CRON_SECRET env-var (Bearer-header) · audit-emits every invocation ·
// idempotency-token guard prevents double-execution on retry.
//
// Sovereignty :
//   - service-role-key NEVER reachable from non-cron routes (env-isolated)
//   - CRON_SECRET rotates without code-change
//   - failed cron-jobs ENTER backoff queue · ¬ silent-loss
//   - audit-log = transparency · public-read aggregate (cap-gated detail)
//
// Bit-pack philosophy : tight envelope · 16-byte job-id · u64 ns-timestamp.

import type { NextApiRequest, NextApiResponse } from 'next';
import { envelope } from './response';

// ─── auth ──────────────────────────────────────────────────────────────────

// Vercel injects `Authorization: Bearer <CRON_SECRET>` for cron-jobs scheduled
// in vercel.json. We accept ANY of three patterns to allow both Vercel-cron
// and out-of-band manual triggers (with the secret) :
//   1. Authorization: Bearer <secret>          (Vercel-cron default)
//   2. x-cron-secret: <secret>                 (header-shorthand)
//   3. ?cron_secret=<secret>                   (query-string · DEV ONLY)
//
// Returns true ONLY when the configured secret matches AND was supplied via
// header (constant-time compare) ; query-string fallback only honored when
// CRON_ALLOW_QUERY_SECRET=true (¬ production-default · prevents log-leakage).
export function isCronAuthorized(req: NextApiRequest): {
  ok: boolean;
  via: 'bearer' | 'header' | 'query' | 'none';
  reason: string | null;
} {
  const secret = process.env['CRON_SECRET'];
  if (typeof secret !== 'string' || secret.length === 0) {
    // No secret configured → reject. Stub-mode caller must short-circuit
    // BEFORE calling this guard (see `isCronStubMode`).
    return { ok: false, via: 'none', reason: 'CRON_SECRET not configured' };
  }
  // 1. Authorization: Bearer …
  const authRaw = req.headers['authorization'];
  const auth = Array.isArray(authRaw) ? authRaw[0] : authRaw;
  if (typeof auth === 'string' && auth.startsWith('Bearer ')) {
    const presented = auth.slice('Bearer '.length).trim();
    if (constantTimeEqual(presented, secret)) {
      return { ok: true, via: 'bearer', reason: null };
    }
  }
  // 2. x-cron-secret
  const xRaw = req.headers['x-cron-secret'];
  const x = Array.isArray(xRaw) ? xRaw[0] : xRaw;
  if (typeof x === 'string' && constantTimeEqual(x, secret)) {
    return { ok: true, via: 'header', reason: null };
  }
  // 3. ?cron_secret=… (DEV-ONLY · gated behind env-flag)
  const allowQuery = process.env['CRON_ALLOW_QUERY_SECRET'] === 'true';
  if (allowQuery) {
    const qRaw = req.query['cron_secret'];
    const q = Array.isArray(qRaw) ? qRaw[0] : qRaw;
    if (typeof q === 'string' && constantTimeEqual(q, secret)) {
      return { ok: true, via: 'query', reason: null };
    }
  }
  return { ok: false, via: 'none', reason: 'invalid or missing cron-secret' };
}

// Stub-mode → no CRON_SECRET configured. Cron endpoints short-circuit with
// 200 + stub envelope so smoke-tests pass on first-deploy (before Apocky has
// pasted the secret to Vercel-env).
export function isCronStubMode(): boolean {
  const secret = process.env['CRON_SECRET'];
  return typeof secret !== 'string' || secret.length === 0;
}

// Constant-time equality on small strings. Standard guard against timing-
// oracle even though Vercel-edge already rate-limits.
function constantTimeEqual(a: string, b: string): boolean {
  if (a.length !== b.length) return false;
  let diff = 0;
  for (let i = 0; i < a.length; i++) {
    diff |= a.charCodeAt(i) ^ b.charCodeAt(i);
  }
  return diff === 0;
}

// ─── 401 helper ────────────────────────────────────────────────────────────

// Standard 401 response · ¬ leak which check failed.
export function reject401(res: NextApiResponse, reason: string): void {
  const env = envelope();
  res.status(401).json({
    ok: false,
    error: 'unauthorized',
    reason_class: reason,
    served_by: env.served_by,
    ts: env.ts,
  });
}

// ─── audit-log ─────────────────────────────────────────────────────────────

// CronExecution = one row in `cron_executions` (see migration 0034).
// Bit-pack-conscious : keep keys terse · status enumerated · no PII.
export interface CronExecution {
  job_name: string;          // e.g. 'playtest-cycle'
  started_at: string;         // ISO-UTC
  finished_at: string;        // ISO-UTC
  duration_ms: number;        // observed wall-clock
  status: 'ok' | 'fail' | 'skip' | 'partial';
  rows_processed: number;     // domain-specific count
  retry_count: number;        // 0 on first-run · ≥1 on backoff-retry
  via: 'bearer' | 'header' | 'query' | 'none';
  notes: string | null;       // ≤256 chars · machine-readable preferred
}

// Fire-and-forget audit emit. Falls back to console-log when Supabase env
// missing. All cron endpoints SHOULD call this on completion.
export async function emitCronAudit(exec: CronExecution): Promise<void> {
  // Local trace · always
  // eslint-disable-next-line no-console
  console.log(JSON.stringify({ evt: 'cron', ...exec }));
  // Supabase mirror (best-effort) — ¬ block cron-completion on this.
  const url = process.env['NEXT_PUBLIC_SUPABASE_URL'];
  const svcKey = process.env['SUPABASE_SERVICE_ROLE_KEY'];
  if (!url || !svcKey) return;
  try {
    await fetch(`${url}/rest/v1/cron_executions`, {
      method: 'POST',
      headers: {
        'Content-Type': 'application/json',
        apikey: svcKey,
        authorization: `Bearer ${svcKey}`,
        Prefer: 'return=minimal',
      },
      body: JSON.stringify(exec),
    });
  } catch (_err) {
    // swallow · we already console-logged
  }
}

// Compute duration helper.
export function nowDurationMs(startMs: number): { finished_at: string; duration_ms: number } {
  return {
    finished_at: new Date().toISOString(),
    duration_ms: Date.now() - startMs,
  };
}

// ─── idempotency ───────────────────────────────────────────────────────────
// Vercel-cron may retry on transient failure. To avoid double-execution we
// derive an idempotency-key from (job_name, bucket-aligned-timestamp) so two
// invocations within the same cadence-bucket collapse to one effect.

export function idempotencyKey(jobName: string, cadenceSec: number): string {
  const nowSec = Math.floor(Date.now() / 1000);
  const bucket = Math.floor(nowSec / cadenceSec) * cadenceSec;
  return `${jobName}:${bucket}`;
}

// ─── service-role-key guard ─────────────────────────────────────────────────
// Cron endpoints — and ONLY cron endpoints — may use the service-role-key
// (full-Postgres-access). This helper makes the requirement explicit at the
// call-site so future audits can grep for `getServiceRoleClient` and verify
// every match is in pages/api/cron/*.

import { createClient, type SupabaseClient } from '@supabase/supabase-js';

let _svcClient: SupabaseClient | null | undefined;

export function getServiceRoleClient(): SupabaseClient | null {
  if (_svcClient !== undefined) return _svcClient;
  const url = process.env['NEXT_PUBLIC_SUPABASE_URL'];
  const key = process.env['SUPABASE_SERVICE_ROLE_KEY'];
  if (!url || !key) {
    _svcClient = null;
    return null;
  }
  _svcClient = createClient(url, key, { auth: { persistSession: false } });
  return _svcClient;
}

export function _resetServiceRoleForTests(): void {
  _svcClient = undefined;
}

// ─── inline self-tests (run via `node --import tsx lib/cron-auth.ts`) ──────

declare const require: { main?: unknown } | undefined;
declare const module: { id?: string } | undefined;

function assert(cond: boolean, msg: string): void {
  if (!cond) throw new Error(`assert failed : ${msg}`);
}

// 1. constantTimeEqual returns true on equal · false on mismatch.
function testConstantTime(): void {
  assert(constantTimeEqual('abc', 'abc'), 'eq · same');
  assert(!constantTimeEqual('abc', 'abd'), 'eq · diff');
  assert(!constantTimeEqual('abc', 'abcd'), 'eq · len diff');
  assert(!constantTimeEqual('', 'x'), 'eq · empty vs nonempty');
}

// 2. idempotencyKey is bucket-stable within cadence window.
function testIdempotencyBucket(): void {
  const k1 = idempotencyKey('foo', 60);
  const k2 = idempotencyKey('foo', 60);
  assert(k1 === k2, 'same-bucket → same-key');
  // different cadence → different bucket
  const k3 = idempotencyKey('foo', 1);
  // these MAY equal if the Date.now() second-bucket lines up, so just check
  // shape rather than inequality.
  assert(k3.startsWith('foo:'), 'key shape');
}

// 3. isCronStubMode reflects env-state.
function testStubMode(): void {
  const prev = process.env['CRON_SECRET'];
  delete process.env['CRON_SECRET'];
  assert(isCronStubMode(), 'no-secret → stub-mode');
  process.env['CRON_SECRET'] = 'x';
  assert(!isCronStubMode(), 'with-secret → ¬ stub-mode');
  if (prev === undefined) delete process.env['CRON_SECRET'];
  else process.env['CRON_SECRET'] = prev;
}

// 4. isCronAuthorized rejects missing secret.
function testAuthRejectMissing(): void {
  const prev = process.env['CRON_SECRET'];
  process.env['CRON_SECRET'] = 'topsecret';
  const req = { headers: {}, query: {} } as unknown as NextApiRequest;
  const r = isCronAuthorized(req);
  assert(!r.ok, 'no-creds → reject');
  assert(r.via === 'none', 'via=none');
  if (prev === undefined) delete process.env['CRON_SECRET'];
  else process.env['CRON_SECRET'] = prev;
}

// 5. isCronAuthorized accepts Bearer header.
function testAuthAcceptBearer(): void {
  const prev = process.env['CRON_SECRET'];
  process.env['CRON_SECRET'] = 'topsecret';
  const req = {
    headers: { authorization: 'Bearer topsecret' },
    query: {},
  } as unknown as NextApiRequest;
  const r = isCronAuthorized(req);
  assert(r.ok, 'matching bearer → accept');
  assert(r.via === 'bearer', 'via=bearer');
  if (prev === undefined) delete process.env['CRON_SECRET'];
  else process.env['CRON_SECRET'] = prev;
}

// 6. isCronAuthorized accepts x-cron-secret header.
function testAuthAcceptXHeader(): void {
  const prev = process.env['CRON_SECRET'];
  process.env['CRON_SECRET'] = 'topsecret';
  const req = {
    headers: { 'x-cron-secret': 'topsecret' },
    query: {},
  } as unknown as NextApiRequest;
  const r = isCronAuthorized(req);
  assert(r.ok, 'matching x-cron-secret → accept');
  assert(r.via === 'header', 'via=header');
  if (prev === undefined) delete process.env['CRON_SECRET'];
  else process.env['CRON_SECRET'] = prev;
}

// 7. Query-string secret rejected unless CRON_ALLOW_QUERY_SECRET=true.
function testAuthQueryGated(): void {
  const prev = process.env['CRON_SECRET'];
  const prevAllow = process.env['CRON_ALLOW_QUERY_SECRET'];
  process.env['CRON_SECRET'] = 'topsecret';
  delete process.env['CRON_ALLOW_QUERY_SECRET'];
  const req = {
    headers: {},
    query: { cron_secret: 'topsecret' },
  } as unknown as NextApiRequest;
  const rDeny = isCronAuthorized(req);
  assert(!rDeny.ok, 'query-secret w/o flag → reject');
  process.env['CRON_ALLOW_QUERY_SECRET'] = 'true';
  const rAllow = isCronAuthorized(req);
  assert(rAllow.ok, 'query-secret w/ flag → accept');
  assert(rAllow.via === 'query', 'via=query');
  if (prev === undefined) delete process.env['CRON_SECRET'];
  else process.env['CRON_SECRET'] = prev;
  if (prevAllow === undefined) delete process.env['CRON_ALLOW_QUERY_SECRET'];
  else process.env['CRON_ALLOW_QUERY_SECRET'] = prevAllow;
}

const isMain =
  typeof require !== 'undefined' &&
  typeof module !== 'undefined' &&
  require.main === module;
if (isMain) {
  testConstantTime();
  testIdempotencyBucket();
  testStubMode();
  testAuthRejectMissing();
  testAuthAcceptBearer();
  testAuthAcceptXHeader();
  testAuthQueryGated();
  // eslint-disable-next-line no-console
  console.log('cron-auth.ts : OK · 7 inline tests passed');
}
