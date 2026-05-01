// cssl-edge · /api/run-share/submit
// POST a roguelike run-share-receipt. Cap-gated RUN_SHARE_CAP_SUBMIT · 0x40.
// Sovereign-bypass supported via x-loa-sovereign-cap header.
//
// Gift-economy framing : friend can attempt your seed (¬ scored-against-you ;
// gift-replay) and you get echo-back bonus when they complete it.
// NO leaderboards · NO PvP · NO rank (per ROGUELIKE_LOOP § RUN-SHARING).
//
// Body :
//   {
//     player_id: string · required
//     seed: string · required (run seed)
//     scoring: { runtime_s: number, depth: number, completed: boolean } · required
//     screenshot_handle?: string · opaque CDN handle to thumbnail
//     note?: string · optional creator note
//     cap: number · cap-bit mask
//     sovereign?: boolean
//   }

import type { NextApiRequest, NextApiResponse } from 'next';
import { auditEvent, deny, logEvent } from '@/lib/audit';
import { RUN_SHARE_CAP_SUBMIT } from '@/lib/cap';
import { isSovereignFromIncoming } from '@/lib/sovereign';
import { envelope, logHit } from '@/lib/response';

interface SubmitRequest {
  player_id?: unknown;
  seed?: unknown;
  scoring?: unknown;
  screenshot_handle?: unknown;
  note?: unknown;
  cap?: unknown;
  sovereign?: unknown;
}

export interface RunShareScoring {
  runtime_s: number;
  depth: number;
  completed: boolean;
}

export interface RunShareReceipt {
  receipt_id: string;
  player_id: string;
  seed: string;
  scoring: RunShareScoring;
  screenshot_handle: string;
  note: string;
  posted_at: string;
  echoes_received: number;
}

interface SubmitOk {
  served_by: string;
  ts: string;
  receipt: RunShareReceipt;
  accepted: true;
  framing: 'gift-economy';
}

interface SubmitError {
  error: string;
  served_by: string;
  ts: string;
}

function isObject(b: unknown): b is Record<string, unknown> {
  return typeof b === 'object' && b !== null;
}

function isScoring(v: unknown): v is RunShareScoring {
  if (!isObject(v)) return false;
  return (
    typeof v['runtime_s'] === 'number' &&
    typeof v['depth'] === 'number' &&
    typeof v['completed'] === 'boolean'
  );
}

function genReceiptId(): string {
  const r = Math.floor(Math.random() * 0xffffffffffff);
  return `rs-${r.toString(16).padStart(12, '0')}`;
}

export default function handler(
  req: NextApiRequest,
  res: NextApiResponse<SubmitOk | SubmitError>
): void {
  logHit('run-share.submit', { method: req.method ?? 'GET' });

  if (req.method !== 'POST') {
    const env = envelope();
    res.setHeader('Allow', 'POST');
    res.status(405).json({
      error: 'Method Not Allowed — POST {player_id, seed, scoring, cap, ...}',
      served_by: env.served_by,
      ts: env.ts,
    });
    return;
  }

  const body: unknown = req.body;
  if (!isObject(body)) {
    const env = envelope();
    res.status(400).json({
      error: 'Bad Request — body must be JSON object',
      served_by: env.served_by,
      ts: env.ts,
    });
    return;
  }

  const reqBody = body as SubmitRequest;
  const cap = typeof reqBody.cap === 'number' ? reqBody.cap : 0;
  const sovereignFlag = reqBody.sovereign === true;
  const sovereignAllowed = isSovereignFromIncoming(req.headers, sovereignFlag);

  // Cap-gate.
  const capAllowed = (cap & RUN_SHARE_CAP_SUBMIT) !== 0;
  if (!capAllowed && !sovereignAllowed) {
    const d = deny('cap RUN_SHARE_CAP_SUBMIT=0x40 required (or sovereign-header)', cap);
    logEvent(d.body);
    const env = envelope();
    res.status(d.status).json({
      error: d.body.extra?.['reason'] as string ?? 'denied',
      served_by: env.served_by,
      ts: env.ts,
    });
    return;
  }

  const player_id = typeof reqBody.player_id === 'string' ? reqBody.player_id : '';
  const seed = typeof reqBody.seed === 'string' ? reqBody.seed : '';
  const screenshot_handle = typeof reqBody.screenshot_handle === 'string' ? reqBody.screenshot_handle : '';
  const note = typeof reqBody.note === 'string' ? reqBody.note : '';

  if (player_id.length === 0 || seed.length === 0) {
    logEvent(
      auditEvent('run-share.submit', cap, sovereignAllowed, 'denied', {
        reason: 'missing player_id or seed',
      })
    );
    const env = envelope();
    res.status(400).json({
      error: 'Bad Request — player_id and seed are required strings',
      served_by: env.served_by,
      ts: env.ts,
    });
    return;
  }

  if (!isScoring(reqBody.scoring)) {
    logEvent(
      auditEvent('run-share.submit', cap, sovereignAllowed, 'denied', {
        reason: 'scoring must be {runtime_s:number, depth:number, completed:boolean}',
      })
    );
    const env = envelope();
    res.status(400).json({
      error: 'Bad Request — scoring must be {runtime_s:number, depth:number, completed:boolean}',
      served_by: env.served_by,
      ts: env.ts,
    });
    return;
  }

  const receipt: RunShareReceipt = {
    receipt_id: genReceiptId(),
    player_id,
    seed,
    scoring: reqBody.scoring,
    screenshot_handle,
    note,
    posted_at: new Date().toISOString(),
    echoes_received: 0,
  };

  logEvent(
    auditEvent('run-share.submit', cap, sovereignAllowed, 'ok', {
      receipt_id: receipt.receipt_id,
      depth: receipt.scoring.depth,
      completed: receipt.scoring.completed,
    })
  );

  const env = envelope();
  res.status(200).json({
    served_by: env.served_by,
    ts: env.ts,
    receipt,
    accepted: true,
    framing: 'gift-economy',
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
  body?: unknown
): { req: NextApiRequest; res: NextApiResponse; out: MockedResponse } {
  const out: MockedResponse = { statusCode: 0, body: null, headers: {} };
  const req = { method, query: {}, headers: {}, body } as unknown as NextApiRequest;
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

const SAMPLE_BODY = {
  player_id: 'alice',
  seed: 'seed-run-001',
  scoring: { runtime_s: 423, depth: 7, completed: true },
  cap: RUN_SHARE_CAP_SUBMIT,
};

// 1. cap=0 → 403.
export function testCapsZeroDenies(): void {
  const { req, res, out } = mockReqRes('POST', { ...SAMPLE_BODY, cap: 0 });
  handler(req, res);
  assert(out.statusCode === 403, `cap=0 must yield 403, got ${out.statusCode}`);
}

// 2. cap-bit set + valid scoring → 200 with receipt + gift-economy framing.
export function testCapsSetAcceptsReceipt(): void {
  const { req, res, out } = mockReqRes('POST', SAMPLE_BODY);
  handler(req, res);
  assert(out.statusCode === 200, `cap-set must yield 200, got ${out.statusCode}`);
  const b = out.body as SubmitOk;
  assert(b.accepted === true, 'accepted must be true');
  assert(b.framing === 'gift-economy', 'framing must be gift-economy');
  assert(b.receipt.scoring.depth === 7, 'scoring.depth echoed');
  assert(b.receipt.echoes_received === 0, 'echoes_received starts at 0');
}

declare const require: { main?: unknown } | undefined;
declare const module: { id?: string } | undefined;
const isMain =
  typeof require !== 'undefined' &&
  typeof module !== 'undefined' &&
  require.main === module;
if (isMain) {
  testCapsZeroDenies();
  testCapsSetAcceptsReceipt();
  // eslint-disable-next-line no-console
  console.log('run-share/submit.ts : OK · 2 inline tests passed');
}
