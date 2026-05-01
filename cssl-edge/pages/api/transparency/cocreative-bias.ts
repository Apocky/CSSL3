// cssl-edge · /api/transparency/cocreative-bias
// GET handler — surfaces current cocreative bias-vector + recent feedback rows
// for a caller. Stage-0 stub-friendly when Supabase env vars absent.
//
// Query :
//   - GET ?player_id=<string>&limit=<int>
//   - 200 : envelope({ bias_vector, feedback: BiasFeedbackRow[], total })
//   - 405 : non-GET method
//
// Real impl reads `public.cocreative_bias_vector` (1 row per player) +
// `public.cocreative_feedback` (N rows per player) ; aggregates into a
// transparency-friendly summary.
import type { NextApiRequest, NextApiResponse } from 'next';
import { envelope, logHit } from '@/lib/response';
import { logEvent, auditEvent } from '@/lib/audit';
import { getSupabase } from '@/lib/supabase';

export interface BiasVector {
  player_id: string;
  // Six axes the substrate currently tracks. Values in [-1, 1] where 0=neutral.
  // Negative = lean-toward-A · positive = lean-toward-B.
  exploration_vs_safety: number;
  novelty_vs_familiarity: number;
  pace_vs_atmosphere: number;
  challenge_vs_flow: number;
  detail_vs_summary: number;
  collab_vs_solo: number;
  updated_at: string;
}

export interface BiasFeedbackRow {
  id: number;
  player_id: string;
  ts_iso: string;
  axis: string;
  delta: number;
  source_event: string;
  caller_origin: string;
}

interface BiasOk {
  served_by: string;
  ts: string;
  bias_vector: BiasVector;
  feedback: BiasFeedbackRow[];
  total: number;
  player_id: string;
  source: 'supabase' | 'stub';
}

interface BiasError {
  error: string;
  served_by: string;
  ts: string;
}

const DEFAULT_LIMIT = 20;
const MAX_LIMIT = 200;

function buildStubBiasVector(player_id: string): BiasVector {
  return {
    player_id,
    exploration_vs_safety: 0.34,
    novelty_vs_familiarity: -0.12,
    pace_vs_atmosphere: -0.41,
    challenge_vs_flow: 0.07,
    detail_vs_summary: 0.55,
    collab_vs_solo: -0.23,
    updated_at: '2026-04-30T11:58:00.000Z',
  };
}

function buildStubFeedback(player_id: string): BiasFeedbackRow[] {
  const base = '2026-04-30T12:00:00.000Z';
  const ts = (offsetMin: number): string =>
    new Date(new Date(base).getTime() - offsetMin * 60_000).toISOString();
  return [
    { id: 240, player_id, ts_iso: ts(2), axis: 'detail_vs_summary', delta: 0.05, source_event: 'scene.companion-asked-for-detail', caller_origin: 'cssl-host' },
    { id: 239, player_id, ts_iso: ts(11), axis: 'pace_vs_atmosphere', delta: -0.04, source_event: 'scene.player-lingered-in-atmosphere', caller_origin: 'cssl-host' },
    { id: 238, player_id, ts_iso: ts(28), axis: 'exploration_vs_safety', delta: 0.03, source_event: 'scene.chose-unmarked-path', caller_origin: 'cssl-host' },
    { id: 237, player_id, ts_iso: ts(45), axis: 'collab_vs_solo', delta: -0.02, source_event: 'companion.player-declined-collab-suggestion', caller_origin: 'cssl-host' },
    { id: 236, player_id, ts_iso: ts(73), axis: 'novelty_vs_familiarity', delta: -0.03, source_event: 'scene.requested-revisit-prior-room', caller_origin: 'cssl-host' },
    { id: 235, player_id, ts_iso: ts(101), axis: 'challenge_vs_flow', delta: 0.02, source_event: 'combat.opted-into-harder-tier', caller_origin: 'cssl-host' },
  ];
}

export default async function handler(
  req: NextApiRequest,
  res: NextApiResponse<BiasOk | BiasError>
): Promise<void> {
  logHit('transparency.cocreative-bias', { method: req.method ?? 'GET' });

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
  let bias_vector: BiasVector;
  let feedback: BiasFeedbackRow[];
  let source: 'supabase' | 'stub';

  if (sb === null) {
    bias_vector = buildStubBiasVector(player_id);
    feedback = buildStubFeedback(player_id).slice(0, limit);
    source = 'stub';
  } else {
    // Bias vector — single-row lookup.
    const { data: biasData, error: biasErr } = await sb
      .from('cocreative_bias_vector')
      .select('*')
      .eq('player_id', player_id)
      .maybeSingle();
    if (biasErr || biasData === null) {
      bias_vector = buildStubBiasVector(player_id);
    } else {
      bias_vector = biasData as BiasVector;
    }
    // Feedback rows — most recent first.
    const { data: fbData, error: fbErr } = await sb
      .from('cocreative_feedback')
      .select('*')
      .eq('player_id', player_id)
      .order('ts_iso', { ascending: false })
      .limit(limit);
    if (fbErr || fbData === null) {
      feedback = [];
    } else {
      feedback = fbData as BiasFeedbackRow[];
    }
    source = 'supabase';
  }

  logEvent(
    auditEvent('transparency.cocreative-bias', 0, false, 'ok', {
      player_id,
      feedback_returned: feedback.length,
      source,
    })
  );

  const env = envelope();
  res.status(200).json({
    served_by: env.served_by,
    ts: env.ts,
    bias_vector,
    feedback,
    total: feedback.length,
    player_id,
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

export async function testEnvMissingStubFallback(): Promise<void> {
  delete process.env['NEXT_PUBLIC_SUPABASE_URL'];
  delete process.env['SUPABASE_ANON_KEY'];
  const { _resetSupabaseForTests } = await import('@/lib/supabase');
  _resetSupabaseForTests();
  const { req, res, out } = mockReqRes('GET', { player_id: 'alice' });
  await handler(req, res);
  assert(out.statusCode === 200, `expected 200, got ${out.statusCode}`);
  const body = out.body as BiasOk;
  assert(body.source === 'stub', `expected stub, got ${body.source}`);
  assert(typeof body.bias_vector === 'object', 'bias_vector must be object');
  assert(body.bias_vector.player_id === 'alice', 'bias_vector.player_id must echo');
  assert(Array.isArray(body.feedback), 'feedback must be Array');
  assert(body.feedback.length === 6, `expected 6 stub feedback rows, got ${body.feedback.length}`);
}

export async function testReturnsArrayWithLimit(): Promise<void> {
  const { _resetSupabaseForTests } = await import('@/lib/supabase');
  _resetSupabaseForTests();
  const { req, res, out } = mockReqRes('GET', { player_id: 'bob', limit: '3' });
  await handler(req, res);
  const body = out.body as BiasOk;
  assert(Array.isArray(body.feedback), 'feedback must be Array');
  assert(body.feedback.length === 3, `expected 3 feedback rows, got ${body.feedback.length}`);
  // Each axis value must be a number.
  const v = body.bias_vector;
  assert(typeof v.exploration_vs_safety === 'number', 'axis value must be number');
  assert(typeof v.novelty_vs_familiarity === 'number', 'axis value must be number');
  assert(typeof v.pace_vs_atmosphere === 'number', 'axis value must be number');
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
    .then(testReturnsArrayWithLimit)
    .then(() => {
      // eslint-disable-next-line no-console
      console.log('transparency/cocreative-bias.ts : OK · 2 inline tests passed');
    })
    .catch((err) => {
      // eslint-disable-next-line no-console
      console.error(err);
      process.exit(1);
    });
}
