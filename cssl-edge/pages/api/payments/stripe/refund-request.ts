// cssl-edge · /api/payments/stripe/refund-request
// W9-A4 · 14-day no-questions-asked refund initiator.
//
// Honors CA-Bus-Prof-§17602(b) auto-renewal-notice + 14-day-refund discipline
// from spec/grand-vision/22 + /legal/terms.
//
// POST { stripe_session_id · player_id · reason? · cap? · sovereign? }
//   → { ok · refund_id · estimated_arrival }   (200) when configured
//   → { stub · todo }                          (200) stub-mode
//   → 403/400 cap-deny / bad-input

import type { NextApiRequest, NextApiResponse } from 'next';
import { logHit, envelope, stubEnvelope } from '@/lib/response';
import { auditEvent, deny, logEvent } from '@/lib/audit';
import { checkCap, STRIPE_REFUND_REQUEST } from '@/lib/cap';
import { isSovereignFromIncoming } from '@/lib/sovereign';
import { getStripe } from '@/lib/stripe';
import { getSupabase } from '@/lib/supabase';

interface RefundRequest {
  stripe_session_id?: string;
  player_id?: string;
  reason?: string;
  cap?: number;
  sovereign?: boolean;
}

interface RefundOk {
  ok: true;
  refund_id: string;
  estimated_arrival_iso: string;
  served_by: string;
  ts: string;
}
interface RefundStub {
  stub: true;
  todo: string;
  served_by: string;
  ts: string;
}
interface RefundErr {
  ok: false;
  error: string;
  served_by: string;
  ts: string;
}
type Resp = RefundOk | RefundStub | RefundErr;

// 5–10 business-days ACH return is Stripe's published guidance — pick 7d.
function estimatedArrival(): string {
  const now = Date.now();
  const sevenDaysMs = 7 * 24 * 60 * 60 * 1000;
  return new Date(now + sevenDaysMs).toISOString();
}

export default async function handler(req: NextApiRequest, res: NextApiResponse<Resp>): Promise<void> {
  logHit('payments.stripe.refund-request', { method: req.method ?? 'POST' });
  const env = envelope();

  if (req.method !== 'POST') {
    res.status(405).json({ ok: false, error: 'POST only', ...env });
    return;
  }

  const body = (req.body ?? {}) as RefundRequest;
  const sessionId = typeof body.stripe_session_id === 'string' ? body.stripe_session_id : '';
  const playerId = typeof body.player_id === 'string' ? body.player_id : '';
  if (sessionId.length === 0 || playerId.length === 0) {
    res.status(400).json({ ok: false, error: 'stripe_session_id + player_id required', ...env });
    return;
  }

  const sovereign = isSovereignFromIncoming(req.headers, body.sovereign);
  const callerCap = typeof body.cap === 'number' ? body.cap : 0;
  const decision = checkCap(callerCap, STRIPE_REFUND_REQUEST, sovereign);
  if (!decision.ok) {
    const d = deny(decision.reason ?? 'cap denied', STRIPE_REFUND_REQUEST);
    logEvent(d.body);
    res.status(d.status).json({ ok: false, error: 'cap denied · STRIPE_REFUND_REQUEST (0x400) required', ...env });
    return;
  }

  const stripe = getStripe();
  if (stripe === null) {
    logEvent(
      auditEvent('payments.refund.stub', STRIPE_REFUND_REQUEST, sovereign, 'ok', {
        session_id: sessionId,
        player_id: playerId,
        reason: 'STRIPE_SECRET_KEY-missing',
      })
    );
    res.status(200).json({ ...stubEnvelope('set STRIPE_SECRET_KEY on Vercel') });
    return;
  }

  try {
    // Stripe wants the payment_intent (not the checkout-session) for refunds.
    const session = await stripe.checkout.sessions.retrieve(sessionId);
    const piId = typeof session.payment_intent === 'string'
      ? session.payment_intent
      : session.payment_intent?.id;
    if (!piId) {
      res.status(400).json({ ok: false, error: 'session has no payment_intent (subscription? not yet charged?)', ...env });
      return;
    }
    const refund = await stripe.refunds.create(
      {
        payment_intent: piId,
        reason: 'requested_by_customer',
        metadata: {
          player_id: playerId,
          stripe_session_id: sessionId,
          ...(typeof body.reason === 'string' ? { customer_reason: body.reason.slice(0, 200) } : {}),
        },
      },
      { idempotencyKey: `refund-${sessionId}` }
    );

    // Best-effort persist · sb may be null in stub-Supabase mode
    const sb = getSupabase();
    if (sb !== null) {
      await sb.from('stripe_refunds').upsert(
        {
          refund_id: refund.id,
          player_id: playerId,
          stripe_session_id: sessionId,
          amount: refund.amount,
          status: refund.status,
          processed_at: new Date().toISOString(),
        },
        { onConflict: 'refund_id' }
      );
    }

    logEvent(
      auditEvent('payments.refund.created', STRIPE_REFUND_REQUEST, sovereign, 'ok', {
        refund_id: refund.id,
        session_id: sessionId,
        player_id: playerId,
      })
    );

    res.status(200).json({
      ok: true,
      refund_id: refund.id,
      estimated_arrival_iso: estimatedArrival(),
      ...env,
    });
  } catch (err) {
    const msg = err instanceof Error ? err.message : 'unknown';
    logEvent(
      auditEvent('payments.refund.error', STRIPE_REFUND_REQUEST, sovereign, 'error', {
        session_id: sessionId,
        player_id: playerId,
        err: msg,
      })
    );
    res.status(502).json({ ok: false, error: `stripe error : ${msg}`, ...env });
  }
}

// ─── Inline tests · framework-agnostic ─────────────────────────────────────

interface MockedResponse {
  statusCode: number;
  body: unknown;
}

function mockReqRes(method: string, body: unknown = {}): { req: NextApiRequest; res: NextApiResponse; out: MockedResponse } {
  const out: MockedResponse = { statusCode: 0, body: null };
  const req = { method, query: {}, headers: {}, body } as unknown as NextApiRequest;
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

// 1. missing fields → 400.
export async function testRefundMissingFields(): Promise<void> {
  const { req, res, out } = mockReqRes('POST', { cap: STRIPE_REFUND_REQUEST });
  await handler(req, res);
  assert(out.statusCode === 400, `expected 400, got ${out.statusCode}`);
}

// 2. valid + STRIPE_SECRET_KEY missing → stub.
export async function testRefundStubMode(): Promise<void> {
  const prev = process.env['STRIPE_SECRET_KEY'];
  delete process.env['STRIPE_SECRET_KEY'];
  const { _resetStripeForTests } = await import('@/lib/stripe');
  _resetStripeForTests();
  const { req, res, out } = mockReqRes('POST', {
    stripe_session_id: 'cs_test_1',
    player_id: 'p_1',
    cap: STRIPE_REFUND_REQUEST,
  });
  await handler(req, res);
  if (prev !== undefined) process.env['STRIPE_SECRET_KEY'] = prev;
  assert(out.statusCode === 200, `expected 200 stub, got ${out.statusCode}`);
  const body = out.body as { stub?: boolean };
  assert(body.stub === true, 'stub-mode shape');
}

declare const require: { main?: unknown } | undefined;
declare const module: { id?: string } | undefined;
const isMain =
  typeof require !== 'undefined' &&
  typeof module !== 'undefined' &&
  require.main === module;
if (isMain) {
  void (async () => {
    await testRefundMissingFields();
    await testRefundStubMode();
    // eslint-disable-next-line no-console
    console.log('payments/stripe/refund-request.ts : OK · 2 inline tests passed');
  })();
}
