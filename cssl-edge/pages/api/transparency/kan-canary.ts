// cssl-edge · /api/transparency/kan-canary
// PUBLIC-READ canary-disagreement-stats per swap-point. NO cap-gate
// (per spec/11_KAN_RIDE § ATTESTATION : KAN-canary stats are publicly
// observable so external auditors can verify substrate behavior).
//
// Stage-0 stub-friendly : when Supabase env vars are absent, returns a
// deterministic stub array spanning 6 swap-points. Real impl reads from
// `public.kan_canary_disagreements` (cssl-supabase migration 0014/0015).
//
// Query :
//   - GET ?swap_point=<string>&window_min=<int>&limit=<int>
//   - 200 : envelope({ stats: KanCanaryStat[], total })
//   - 405 : non-GET method

import type { NextApiRequest, NextApiResponse } from 'next';
import { auditEvent, logEvent } from '@/lib/audit';
import { envelope, logHit } from '@/lib/response';
import { getSupabase } from '@/lib/supabase';

export interface KanCanaryStat {
  swap_point: string;
  total_invocations: number;
  disagreements: number;
  disagreement_rate: number;
  last_disagreement_at: string;
  canary_kind: string;
  notes: string;
}

interface CanaryOk {
  served_by: string;
  ts: string;
  stats: KanCanaryStat[];
  total: number;
  swap_point_filter: string;
  window_min: number;
  source: 'supabase' | 'stub';
}

interface CanaryError {
  error: string;
  served_by: string;
  ts: string;
}

const DEFAULT_LIMIT = 50;
const MAX_LIMIT = 256;
const DEFAULT_WINDOW_MIN = 60 * 24; // 24 hours

// 6 swap-points · matches the canonical KAN-vs-MLP disagreement surfaces
// surfaced by cssl-substrate-kan-substrate-runtime + cssl-host-kan-bench.
function buildStubStats(): KanCanaryStat[] {
  const base = '2026-04-30T12:00:00.000Z';
  const ago = (offsetMin: number): string =>
    new Date(new Date(base).getTime() - offsetMin * 60_000).toISOString();
  return [
    {
      swap_point: 'sigma-mask.gather',
      total_invocations: 12_481,
      disagreements: 6,
      disagreement_rate: 6 / 12481,
      last_disagreement_at: ago(3),
      canary_kind: 'numeric-mismatch',
      notes: 'KAN edge-spline diverged on edge-of-mask · within tolerance',
    },
    {
      swap_point: 'omega-field.read',
      total_invocations: 38_204,
      disagreements: 1,
      disagreement_rate: 1 / 38204,
      last_disagreement_at: ago(58),
      canary_kind: 'numeric-mismatch',
      notes: 'one-off flush race · re-canary clean',
    },
    {
      swap_point: 'sig-render.h-band',
      total_invocations: 7_602,
      disagreements: 0,
      disagreement_rate: 0,
      last_disagreement_at: ago(1440),
      canary_kind: 'never-disagreed',
      notes: 'rock-solid since v0.4',
    },
    {
      swap_point: 'sig-render.s-band',
      total_invocations: 7_602,
      disagreements: 4,
      disagreement_rate: 4 / 7602,
      last_disagreement_at: ago(11),
      canary_kind: 'precision-cliff',
      notes: 'KAN spline at S<0.04 disagrees · investigated · within tolerance',
    },
    {
      swap_point: 'novelty-path.compose',
      total_invocations: 3_104,
      disagreements: 2,
      disagreement_rate: 2 / 3104,
      last_disagreement_at: ago(36),
      canary_kind: 'numeric-mismatch',
      notes: 'late-stage 6-path multiplicative · canary green',
    },
    {
      swap_point: 'attestation.verify',
      total_invocations: 1_829,
      disagreements: 0,
      disagreement_rate: 0,
      last_disagreement_at: ago(2880),
      canary_kind: 'never-disagreed',
      notes: 'identity attestation rock-solid',
    },
  ];
}

function readQuery(
  q: Record<string, string | string[] | undefined>,
  key: string
): string | undefined {
  const v = q[key];
  if (Array.isArray(v)) return v[0];
  return v;
}

export default async function handler(
  req: NextApiRequest,
  res: NextApiResponse<CanaryOk | CanaryError>
): Promise<void> {
  logHit('transparency.kan-canary', { method: req.method ?? 'GET' });

  if (req.method !== 'GET') {
    const env = envelope();
    res.setHeader('Allow', 'GET');
    res.status(405).json({
      error: 'Method Not Allowed — GET ?swap_point=&window_min=&limit=',
      served_by: env.served_by,
      ts: env.ts,
    });
    return;
  }

  const q = req.query as Record<string, string | string[] | undefined>;
  const swap_point_filter = readQuery(q, 'swap_point') ?? '';
  const limitRaw = readQuery(q, 'limit');
  const limitParsed = limitRaw !== undefined ? parseInt(limitRaw, 10) : DEFAULT_LIMIT;
  const limit = Math.max(
    1,
    Math.min(Number.isFinite(limitParsed) ? limitParsed : DEFAULT_LIMIT, MAX_LIMIT)
  );
  const windowRaw = readQuery(q, 'window_min');
  const windowParsed = windowRaw !== undefined ? parseInt(windowRaw, 10) : DEFAULT_WINDOW_MIN;
  const window_min = Math.max(
    1,
    Number.isFinite(windowParsed) ? windowParsed : DEFAULT_WINDOW_MIN
  );

  const sb = getSupabase();
  let stats: KanCanaryStat[];
  let source: 'supabase' | 'stub';

  if (sb === null) {
    let stubs = buildStubStats();
    if (swap_point_filter.length > 0) {
      stubs = stubs.filter((s) => s.swap_point === swap_point_filter);
    }
    stats = stubs.slice(0, limit);
    source = 'stub';
  } else {
    let qb = sb
      .from('kan_canary_disagreements')
      .select('*')
      .order('last_disagreement_at', { ascending: false })
      .limit(limit);
    if (swap_point_filter.length > 0) {
      qb = qb.eq('swap_point', swap_point_filter);
    }
    const { data, error } = await qb;
    if (error || data === null) {
      stats = [];
    } else {
      stats = data as KanCanaryStat[];
    }
    source = 'supabase';
  }

  // PUBLIC observability — log emit per spec/11 ATTESTATION (sovereign=false,
  // cap=0). Caller-anonymous · this is a transparency surface.
  logEvent(
    auditEvent('transparency.kan-canary', 0, false, 'ok', {
      swap_point_filter,
      window_min,
      returned: stats.length,
      source,
    })
  );

  const env = envelope();
  res.status(200).json({
    served_by: env.served_by,
    ts: env.ts,
    stats,
    total: stats.length,
    swap_point_filter,
    window_min,
    source,
  });
}

// ─── Inline tests · framework-agnostic ─────────────────────────────────────

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

// 1. env-missing → returns 6 stub rows (no cap-gate · public read).
export async function testEnvMissingStubFallback(): Promise<void> {
  delete process.env['NEXT_PUBLIC_SUPABASE_URL'];
  delete process.env['SUPABASE_ANON_KEY'];
  const { _resetSupabaseForTests } = await import('@/lib/supabase');
  _resetSupabaseForTests();
  const { req, res, out } = mockReqRes('GET', {});
  await handler(req, res);
  assert(out.statusCode === 200, `expected 200, got ${out.statusCode}`);
  const body = out.body as CanaryOk;
  assert(body.source === 'stub', `expected stub source, got ${body.source}`);
  assert(Array.isArray(body.stats), 'stats must be array');
  assert(body.stats.length === 6, `expected 6 stub stats, got ${body.stats.length}`);
}

// 2. swap_point filter narrows down to single row.
export async function testSwapPointFilter(): Promise<void> {
  const { _resetSupabaseForTests } = await import('@/lib/supabase');
  _resetSupabaseForTests();
  const { req, res, out } = mockReqRes('GET', { swap_point: 'sigma-mask.gather' });
  await handler(req, res);
  assert(out.statusCode === 200, `expected 200, got ${out.statusCode}`);
  const body = out.body as CanaryOk;
  assert(body.stats.length === 1, `expected 1 filtered stat, got ${body.stats.length}`);
  assert(
    body.stats[0]?.swap_point === 'sigma-mask.gather',
    'filtered stat must match swap_point'
  );
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
    .then(testSwapPointFilter)
    .then(() => {
      // eslint-disable-next-line no-console
      console.log('transparency/kan-canary.ts : OK · 2 inline tests passed');
    })
    .catch((err) => {
      // eslint-disable-next-line no-console
      console.error(err);
      process.exit(1);
    });
}
