// cssl-edge · /api/transparency/sovereign-cap
// GET handler — surfaces sovereign-cap audit rows for a caller.
// Stage-0 stub-friendly : when Supabase env vars are absent, returns a
// deterministic stub array so the dashboard renders against a fresh deploy
// without backend wiring. Real impl reads from `public.sovereign_cap_audit`.
//
// Query :
//   - GET ?player_id=<string>&limit=<int>
//   - 200 : envelope({ rows: SovereignCapAuditRow[], total })
//   - 405 : non-GET method
//
// Cap-bit : public read of caller-scoped rows, no cap required (caller must
// supply own player_id; future RLS will lock cross-player reads).
//
// NOTE : tests run framework-agnostic via `npx tsx`. See bottom of file.
import type { NextApiRequest, NextApiResponse } from 'next';
import { envelope, logHit } from '@/lib/response';
import { logEvent, auditEvent } from '@/lib/audit';
import { getSupabase } from '@/lib/supabase';

export interface SovereignCapAuditRow {
  id: number;
  player_id: string;
  ts_iso: string;
  action_kind: string;
  cap_bypassed_kind: string;
  reason: string;
  caller_origin: string;
}

interface SovCapOk {
  served_by: string;
  ts: string;
  rows: SovereignCapAuditRow[];
  total: number;
  player_id: string;
  source: 'supabase' | 'stub';
}

interface SovCapError {
  error: string;
  served_by: string;
  ts: string;
}

const DEFAULT_LIMIT = 50;
const MAX_LIMIT = 256;

// Deterministic stub catalog — used when Supabase env-vars are absent. Six
// rows spanning typical action_kinds + cap_bypassed_kinds so the dashboard
// renders a meaningful demo against a fresh deploy.
function buildStubRows(player_id: string): SovereignCapAuditRow[] {
  const base = '2026-04-30T12:00:00.000Z';
  const ts = (offsetMin: number): string =>
    new Date(new Date(base).getTime() - offsetMin * 60_000).toISOString();
  return [
    {
      id: 1006,
      player_id,
      ts_iso: ts(2),
      action_kind: 'companion.relay',
      cap_bypassed_kind: 'COMPANION_REMOTE_RELAY',
      reason: 'sovereign-bypass present · header verified',
      caller_origin: 'cssl-host',
    },
    {
      id: 1005,
      player_id,
      ts_iso: ts(7),
      action_kind: 'signaling.host_room',
      cap_bypassed_kind: 'MP_CAP_HOST_ROOM',
      reason: 'sovereign-bypass present · header verified',
      caller_origin: 'cssl-host',
    },
    {
      id: 1004,
      player_id,
      ts_iso: ts(15),
      action_kind: 'asset.fetch_glb',
      cap_bypassed_kind: 'ASSET_FETCH',
      reason: 'sovereign-bypass present · header verified',
      caller_origin: 'cssl-edge',
    },
    {
      id: 1003,
      player_id,
      ts_iso: ts(34),
      action_kind: 'intent.translate',
      cap_bypassed_kind: 'LLM_DELEGATE',
      reason: 'sovereign-bypass present · header verified',
      caller_origin: 'cssl-edge',
    },
    {
      id: 1002,
      player_id,
      ts_iso: ts(58),
      action_kind: 'signaling.relay',
      cap_bypassed_kind: 'MP_CAP_RELAY_DATA',
      reason: 'sovereign-bypass present · header verified',
      caller_origin: 'cssl-host',
    },
    {
      id: 1001,
      player_id,
      ts_iso: ts(122),
      action_kind: 'generate.3d',
      cap_bypassed_kind: 'GENERATE_DELEGATE',
      reason: 'sovereign-bypass present · header verified',
      caller_origin: 'cssl-edge',
    },
  ];
}

export default async function handler(
  req: NextApiRequest,
  res: NextApiResponse<SovCapOk | SovCapError>
): Promise<void> {
  logHit('transparency.sovereign-cap', { method: req.method ?? 'GET' });

  if (req.method !== 'GET') {
    const env = envelope();
    res.setHeader('Allow', 'GET');
    res.status(405).json({
      error: 'Method Not Allowed — GET ?player_id=&limit=',
      served_by: env.served_by,
      ts: env.ts,
    });
    return;
  }

  const q = req.query as Record<string, string | string[] | undefined>;
  const playerRaw = q['player_id'];
  const player_id =
    (Array.isArray(playerRaw) ? playerRaw[0] : playerRaw) ?? 'anonymous';

  const limitRaw = q['limit'];
  const limitStr = Array.isArray(limitRaw) ? limitRaw[0] : limitRaw;
  const limitParsed =
    limitStr !== undefined ? parseInt(limitStr, 10) : DEFAULT_LIMIT;
  const limit = Math.max(
    1,
    Math.min(
      Number.isFinite(limitParsed) ? limitParsed : DEFAULT_LIMIT,
      MAX_LIMIT
    )
  );

  const sb = getSupabase();
  let rows: SovereignCapAuditRow[];
  let source: 'supabase' | 'stub';

  if (sb === null) {
    rows = buildStubRows(player_id).slice(0, limit);
    source = 'stub';
  } else {
    const { data, error } = await sb
      .from('sovereign_cap_audit')
      .select('*')
      .eq('player_id', player_id)
      .order('ts_iso', { ascending: false })
      .limit(limit);
    if (error || data === null) {
      rows = [];
    } else {
      rows = data as SovereignCapAuditRow[];
    }
    source = 'supabase';
  }

  logEvent(
    auditEvent('transparency.sovereign-cap', 0, false, 'ok', {
      player_id,
      returned: rows.length,
      source,
    })
  );

  const env = envelope();
  res.status(200).json({
    served_by: env.served_by,
    ts: env.ts,
    rows,
    total: rows.length,
    player_id,
    source,
  });
}

// ─── Inline tests · framework-agnostic ─────────────────────────────────────
// Run via `npx tsx pages/api/transparency/sovereign-cap.ts`.

interface MockedResponse {
  statusCode: number;
  body: unknown;
  headers: Record<string, string>;
}

function mockReqRes(
  method: string,
  query: Record<string, string | string[]> = {}
): { req: NextApiRequest; res: NextApiResponse; out: MockedResponse } {
  const out: MockedResponse = { statusCode: 0, body: null, headers: {} };
  const req = { method, query, headers: {}, body: undefined } as unknown as NextApiRequest;
  const res = {
    status(code: number) { out.statusCode = code; return this; },
    json(payload: unknown) { out.body = payload; return this; },
    setHeader(key: string, val: string) { out.headers[key] = val; return this; },
  } as unknown as NextApiResponse;
  return { req, res, out };
}

function assert(cond: boolean, msg: string): void {
  if (!cond) throw new Error(`assert failed : ${msg}`);
}

// 1. env-missing → returns stub rows for the supplied player_id.
export async function testEnvMissingStubFallback(): Promise<void> {
  delete process.env['NEXT_PUBLIC_SUPABASE_URL'];
  delete process.env['SUPABASE_ANON_KEY'];
  const { _resetSupabaseForTests } = await import('@/lib/supabase');
  _resetSupabaseForTests();
  const { req, res, out } = mockReqRes('GET', { player_id: 'alice' });
  await handler(req, res);
  assert(out.statusCode === 200, `expected 200, got ${out.statusCode}`);
  const body = out.body as SovCapOk;
  assert(body.source === 'stub', `expected stub source, got ${body.source}`);
  assert(Array.isArray(body.rows), 'rows must be Array');
  assert(body.rows.length === 6, `expected 6 stub rows, got ${body.rows.length}`);
  assert(body.player_id === 'alice', 'player_id must echo');
}

// 2. Returns a list (typeof array) on every code path.
export async function testReturnsArray(): Promise<void> {
  const { _resetSupabaseForTests } = await import('@/lib/supabase');
  _resetSupabaseForTests();
  const { req, res, out } = mockReqRes('GET', { player_id: 'bob', limit: '3' });
  await handler(req, res);
  const body = out.body as SovCapOk;
  assert(Array.isArray(body.rows), 'rows must be Array on every path');
  assert(body.rows.length === 3, `expected limit=3 rows, got ${body.rows.length}`);
  // Each row must include the required audit columns.
  for (const row of body.rows) {
    assert(typeof row.ts_iso === 'string', 'row.ts_iso must be string');
    assert(typeof row.action_kind === 'string', 'row.action_kind must be string');
    assert(typeof row.cap_bypassed_kind === 'string', 'row.cap_bypassed_kind must be string');
    assert(typeof row.caller_origin === 'string', 'row.caller_origin must be string');
  }
}

declare const require: { main?: unknown } | undefined;
declare const module: { id?: string } | undefined;
const isMain =
  typeof require !== 'undefined' &&
  typeof module !== 'undefined' &&
  require.main === module;
if (isMain) {
  Promise.resolve()
    .then(testEnvMissingStubFallback)
    .then(testReturnsArray)
    .then(() => {
      // eslint-disable-next-line no-console
      console.log('transparency/sovereign-cap.ts : OK · 2 inline tests passed');
    })
    .catch((err) => {
      // eslint-disable-next-line no-console
      console.error(err);
      process.exit(1);
    });
}
