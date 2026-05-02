// cssl-edge · /api/gacha/history
// W13-10 · Gacha pull-history listing for a player.
//
// POST { player_pubkey }
//   → { ok:true · pulls[] · refunds[] · disclosed_drop_rates · pity_threshold }
//   → { evt:'audit', status:'denied', reason }  (403) when cap-gate fails
//
// Stage-0 stub-mode : returns an empty history. Live mode queries Supabase
// `gacha_pulls` + `gacha_refunds` tables (RLS-scoped to the caller's pubkey).
//
// Transparency : every history-response carries the disclosed_drop_rates
// AND pity_threshold so the client can re-display them on the history view.

import type { NextApiRequest, NextApiResponse } from 'next';
import { logHit, envelope } from '@/lib/response';
import { auditEvent, deny, logEvent } from '@/lib/audit';
import { checkCap, GACHA_CAP_HISTORY } from '@/lib/cap';
import { isSovereignFromIncoming } from '@/lib/sovereign';

const DROP_RATES_BPS: Record<string, number> = {
  common: 60_000,
  uncommon: 25_000,
  rare: 10_000,
  epic: 4_000,
  legendary: 900,
  mythic: 100,
};
const PITY_THRESHOLD = 90;
const REFUND_WINDOW_SECS = 7 * 24 * 60 * 60;

interface HistoryRequestBody {
  player_pubkey?: string;
  cap?: number;
  sovereign?: boolean;
  limit?: number;
}

interface PullRow {
  pull_id: string;
  banner_id: string;
  rarity: string;
  cosmetic_handle: string;
  forced_by_pity: boolean;
  pulled_at: string;
  refunded_at: string | null;
  sigma_anchor_id: string;
  refundable: boolean;
  refund_seconds_remaining: number;
}

interface RefundRow {
  pull_id: string;
  refunded_at: string;
  refund_amount: number;
  sigma_refund_id: string;
  original_was_mythic: boolean;
}

interface HistorySuccess {
  ok: true;
  player_pubkey: string;
  pulls: PullRow[];
  refunds: RefundRow[];
  disclosed_drop_rates: Record<string, string>;
  pity_threshold: number;
  refund_window_secs: number;
  served_by: string;
  ts: string;
  stub: boolean;
}

interface HistoryError {
  ok: false;
  error: string;
  served_by: string;
  ts: string;
}

type Resp = HistorySuccess | HistoryError;

function disclosedDropRatePct(): Record<string, string> {
  const out: Record<string, string> = {};
  for (const [k, bps] of Object.entries(DROP_RATES_BPS)) {
    const whole = Math.floor(bps / 1000);
    const frac = bps % 1000;
    out[k] = `${whole}.${String(frac).padStart(3, '0')}%`;
  }
  return out;
}

export default async function handler(req: NextApiRequest, res: NextApiResponse<Resp>): Promise<void> {
  logHit('gacha.history', { method: req.method ?? 'POST' });
  const env = envelope();

  if (req.method !== 'POST' && req.method !== 'GET') {
    res.status(405).json({ ok: false, error: 'POST or GET only', ...env });
    return;
  }

  const body = (req.body ?? {}) as HistoryRequestBody;
  const queryPubkey = typeof req.query?.player_pubkey === 'string' ? req.query.player_pubkey : '';
  const pubkey = typeof body.player_pubkey === 'string' && body.player_pubkey.length > 0
    ? body.player_pubkey
    : queryPubkey;

  if (pubkey.length === 0) {
    res.status(400).json({ ok: false, error: 'player_pubkey required', ...env });
    return;
  }

  // ── cap-gate · default-DENY ────────────────────────────────────────────
  const sovereign = isSovereignFromIncoming(req.headers, body.sovereign);
  const callerCap = typeof body.cap === 'number' ? body.cap : 0;
  const decision = checkCap(callerCap, GACHA_CAP_HISTORY, sovereign);
  if (!decision.ok) {
    const d = deny(decision.reason ?? 'cap denied', GACHA_CAP_HISTORY);
    logEvent(d.body);
    res.status(d.status).json({
      ok: false,
      error: 'cap denied · GACHA_CAP_HISTORY (0x20000000) required',
      ...env,
    });
    return;
  }

  // ── stub-mode response (live mode queries Supabase) ────────────────────
  // Live mode would read from `gacha_pulls` filtered by player_pubkey · RLS
  // enforces self-read · then for each pull compute `refundable` predicate
  // from (now - pulled_at) ≤ REFUND_WINDOW_SECS.

  logEvent(
    auditEvent('gacha.history.queried', GACHA_CAP_HISTORY, sovereign, 'ok', {
      player_pubkey_prefix: pubkey.slice(0, 8),
    })
  );

  res.status(200).json({
    ok: true,
    player_pubkey: pubkey,
    pulls: [],
    refunds: [],
    disclosed_drop_rates: disclosedDropRatePct(),
    pity_threshold: PITY_THRESHOLD,
    refund_window_secs: REFUND_WINDOW_SECS,
    stub: true,
    ...env,
  });
}

// ─── inline tests · framework-agnostic ─────────────────────────────────────

interface MockedResponse {
  statusCode: number;
  body: unknown;
}

function mockReqRes(
  method: string,
  body: unknown = {},
  headers: Record<string, string> = {}
): { req: NextApiRequest; res: NextApiResponse; out: MockedResponse } {
  const out: MockedResponse = { statusCode: 0, body: null };
  const req = { method, query: {}, headers, body } as unknown as NextApiRequest;
  const res = {
    status(code: number) { out.statusCode = code; return this; },
    json(payload: unknown) { out.body = payload; return this; },
    setHeader(_k: string, _v: string) { return this; },
  } as unknown as NextApiResponse;
  return { req, res, out };
}

function assert(cond: boolean, msg: string): void {
  if (!cond) throw new Error(`assert failed : ${msg}`);
}

export async function testHistoryRequiresPubkey(): Promise<void> {
  const { req, res, out } = mockReqRes('POST', { cap: GACHA_CAP_HISTORY });
  await handler(req, res);
  assert(out.statusCode === 400, `expected 400, got ${out.statusCode}`);
}

export async function testHistoryCapDenied(): Promise<void> {
  const { req, res, out } = mockReqRes('POST', {
    player_pubkey: 'aabbccdd'.repeat(8),
    cap: 0,
  });
  await handler(req, res);
  assert(out.statusCode === 403, `expected 403, got ${out.statusCode}`);
}

export async function testHistoryStubModeShape(): Promise<void> {
  const { req, res, out } = mockReqRes('POST', {
    player_pubkey: 'aabbccdd'.repeat(8),
    cap: GACHA_CAP_HISTORY,
  });
  await handler(req, res);
  assert(out.statusCode === 200, 'OK');
  const body = out.body as HistorySuccess;
  assert(body.ok === true, 'ok = true');
  assert(body.stub === true, 'stub = true');
  assert(body.pity_threshold === 90, 'pity threshold disclosed');
  assert(body.refund_window_secs === 604800, 'refund window 7d disclosed');
  assert(body.disclosed_drop_rates.mythic === '0.100%', 'mythic disclosed');
}

declare const require: { main?: unknown } | undefined;
declare const module: { id?: string } | undefined;
const isMain =
  typeof require !== 'undefined' &&
  typeof module !== 'undefined' &&
  require.main === module;
if (isMain) {
  void (async () => {
    await testHistoryRequiresPubkey();
    await testHistoryCapDenied();
    await testHistoryStubModeShape();
    // eslint-disable-next-line no-console
    console.log('gacha/history.ts : OK · 3 inline tests passed');
  })();
}
