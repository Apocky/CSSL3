// cssl-edge · /api/content/tip
// W12-9 · Stripe-gift-channel tip from one creator to another. 100% to-
// tipped-creator (minus Stripe-fee). ¬ platform-tax · ¬ pay-for-power.
// Sovereign-revocable per Stripe ToS (sender refund-flow) + receiver-
// creator can-revoke royalty-share-pledge any-time.
//
// POST /api/content/tip
//   body : {
//     to_creator_pubkey : string (64-hex)
//     content_id        : string (uuid)
//     amount_lamports   : integer 50..=100_000_000
//     success_url       : string (https)
//     cancel_url        : string (https)
//     player_id?        : string (caller-side player id for Stripe meta)
//     idempotency_key?  : string
//   }
//   → 200 { ok, url, session_id, gross, fee_estimate, net_to_creator }
//   → 200 { stub:true, todo }   when STRIPE_SECRET_KEY missing
//   → 4xx on validation / cap-deny

import type { NextApiRequest, NextApiResponse } from 'next';
import type Stripe from 'stripe';
import { logHit, envelope, stubEnvelope } from '@/lib/response';
import { auditEvent, deny, logEvent } from '@/lib/audit';
import { checkCap, CONTENT_CAP_TIP } from '@/lib/cap';
import { isSovereignFromIncoming } from '@/lib/sovereign';
import { getStripe } from '@/lib/stripe';

interface TipReq {
  to_creator_pubkey?: string;
  content_id?: string;
  amount_lamports?: number;
  success_url?: string;
  cancel_url?: string;
  player_id?: string;
  idempotency_key?: string;
  cap?: number;
  sovereign?: boolean;
}

const HEX64_RE = /^[0-9a-f]{64}$/;
const UUID_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

function isSafeUrl(u: string): boolean {
  if (typeof u !== 'string' || u.length < 8 || u.length > 2048) return false;
  try {
    const p = new URL(u);
    return p.protocol === 'https:' || p.protocol === 'http:';
  } catch {
    return false;
  }
}

// 2.9% + 30-lamport flat (matches cssl-content-remix Rust crate
// `stripe_fee_estimate`). Mirror function so server + client agree on
// the displayed-net amount before Stripe round-trip.
function feeEstimateLamports(gross: number): number {
  // ceiling(2.9%) using BigInt to avoid float-rounding.
  const n = BigInt(gross);
  const num = n * 29n + 999n;
  const pct = num / 1000n;
  const total = pct + 30n;
  return Number(total < BigInt(Number.MAX_SAFE_INTEGER) ? total : BigInt(Number.MAX_SAFE_INTEGER));
}

interface ValidationOk {
  ok: true;
  to_creator_pubkey: string;
  content_id: string;
  amount: number;
  success_url: string;
  cancel_url: string;
  player_id?: string;
  idempotency_key: string;
}
interface ValidationErr {
  ok: false;
  error: string;
}

function validate(body: TipReq): ValidationOk | ValidationErr {
  if (!body.to_creator_pubkey || !HEX64_RE.test(body.to_creator_pubkey)) {
    return { ok: false, error: 'to_creator_pubkey must be 64-char lower-hex' };
  }
  if (!body.content_id || !UUID_RE.test(body.content_id)) {
    return { ok: false, error: 'content_id must be uuid' };
  }
  const amt = body.amount_lamports;
  if (typeof amt !== 'number' || !Number.isInteger(amt) || amt < 50 || amt > 100_000_000) {
    return {
      ok: false,
      error: 'amount_lamports must be int 50..=100_000_000',
    };
  }
  const success = body.success_url ?? '';
  const cancel = body.cancel_url ?? '';
  if (!isSafeUrl(success) || !isSafeUrl(cancel)) {
    return { ok: false, error: 'success_url + cancel_url must be valid http(s)' };
  }
  const idempotency =
    typeof body.idempotency_key === 'string' && body.idempotency_key.length > 0
      ? body.idempotency_key
      : `cssl-tip-${Date.now().toString(16)}-${Math.floor(
          Math.random() * 0xffffffff
        )
          .toString(16)
          .padStart(8, '0')}`;
  const out: ValidationOk = {
    ok: true,
    to_creator_pubkey: body.to_creator_pubkey,
    content_id: body.content_id,
    amount: amt,
    success_url: success,
    cancel_url: cancel,
    idempotency_key: idempotency,
  };
  if (typeof body.player_id === 'string' && body.player_id.length > 0) {
    out.player_id = body.player_id;
  }
  return out;
}

interface TipOk {
  ok: true;
  url: string;
  session_id: string;
  to_creator_pubkey: string;
  content_id: string;
  gross_lamports: number;
  stripe_fee_estimate_lamports: number;
  net_lamports_to_creator: number;
  served_by: string;
  ts: string;
}
interface TipStub {
  stub: true;
  todo: string;
  served_by: string;
  ts: string;
}
interface TipErr {
  ok: false;
  error: string;
  served_by: string;
  ts: string;
}
type Resp = TipOk | TipStub | TipErr;

export default async function handler(
  req: NextApiRequest,
  res: NextApiResponse<Resp>
): Promise<void> {
  logHit('content.tip', { method: req.method ?? 'POST' });
  const env = envelope();

  if (req.method !== 'POST') {
    res.status(405).json({ ok: false, error: 'POST only', ...env });
    return;
  }

  const body = (req.body ?? {}) as TipReq;

  // ── cap-gate · default-DENY ───────────────────────────────────────────
  const sovereign = isSovereignFromIncoming(req.headers, body.sovereign);
  const callerCap = typeof body.cap === 'number' ? body.cap : 0;
  const decision = checkCap(callerCap, CONTENT_CAP_TIP, sovereign);
  if (!decision.ok) {
    const d = deny(decision.reason ?? 'cap denied', CONTENT_CAP_TIP);
    logEvent(d.body);
    res.status(d.status).json({
      ok: false,
      error: 'cap denied · CONTENT_CAP_TIP (0x4000) required',
      ...env,
    });
    return;
  }

  // ── validate ──────────────────────────────────────────────────────────
  const v = validate(body);
  if (!v.ok) {
    res.status(400).json({ ok: false, error: v.error, ...env });
    return;
  }

  const fee = feeEstimateLamports(v.amount);
  const net = Math.max(0, v.amount - fee);

  // ── stub-mode (no Stripe) ─────────────────────────────────────────────
  const stripe = getStripe();
  if (stripe === null) {
    logEvent(
      auditEvent('content.tip.stub', CONTENT_CAP_TIP, sovereign, 'ok', {
        to: v.to_creator_pubkey,
        content_id: v.content_id,
        gross: v.amount,
        fee_estimate: fee,
        net,
      })
    );
    res.status(200).json({
      ...stubEnvelope('set STRIPE_SECRET_KEY on Vercel for live tip-flow'),
    });
    return;
  }

  // ── live Stripe call : create one-shot checkout-session for tip ──────
  try {
    const sessionParams: Stripe.Checkout.SessionCreateParams = {
      mode: 'payment',
      line_items: [
        {
          price_data: {
            currency: 'usd',
            unit_amount: v.amount,
            product_data: {
              name: `Gift tip to creator ${v.to_creator_pubkey.slice(0, 12)}…`,
              description: `100% to-creator (minus Stripe-fee ~ ${fee} lamports). Cosmetic-only-axiom · gift-channel · ¬ platform-tax.`,
            },
          },
          quantity: 1,
        },
      ],
      success_url: v.success_url,
      cancel_url: v.cancel_url,
      ...(v.player_id !== undefined ? { client_reference_id: v.player_id } : {}),
      metadata: {
        flow: 'content-tip',
        to_creator_pubkey: v.to_creator_pubkey,
        content_id: v.content_id,
        gross_lamports: v.amount.toString(),
        cssl_edge_version: '0.1.0',
      },
    };
    const session = await stripe.checkout.sessions.create(sessionParams, {
      idempotencyKey: v.idempotency_key,
    });

    logEvent(
      auditEvent('content.tip.created', CONTENT_CAP_TIP, sovereign, 'ok', {
        to: v.to_creator_pubkey,
        content_id: v.content_id,
        session_id: session.id,
        gross: v.amount,
        fee_estimate: fee,
        net,
      })
    );
    res.status(200).json({
      ok: true,
      url: session.url ?? '',
      session_id: session.id,
      to_creator_pubkey: v.to_creator_pubkey,
      content_id: v.content_id,
      gross_lamports: v.amount,
      stripe_fee_estimate_lamports: fee,
      net_lamports_to_creator: net,
      ...env,
    });
  } catch (err) {
    const msg = err instanceof Error ? err.message : 'unknown error';
    logEvent(
      auditEvent('content.tip.error', CONTENT_CAP_TIP, sovereign, 'error', {
        to: v.to_creator_pubkey,
        err: msg,
      })
    );
    res.status(502).json({ ok: false, error: `stripe error : ${msg}`, ...env });
  }
}

// ─── Inline tests · framework-agnostic ────────────────────────────────────

interface MockedResponse {
  statusCode: number;
  body: unknown;
}

function mockReqRes(
  method: string,
  body: unknown = {}
): { req: NextApiRequest; res: NextApiResponse; out: MockedResponse } {
  const out: MockedResponse = { statusCode: 0, body: null };
  const req = { method, query: {}, headers: {}, body } as unknown as NextApiRequest;
  const res = {
    status(code: number) {
      out.statusCode = code;
      return this;
    },
    json(payload: unknown) {
      out.body = payload;
      return this;
    },
    setHeader(_k: string, _v: string) {
      return this;
    },
  } as unknown as NextApiResponse;
  return { req, res, out };
}

function assert(cond: boolean, msg: string): void {
  if (!cond) throw new Error(`assert failed : ${msg}`);
}

// 1. cap=0 + sovereign=false → 403.
export async function testTipCapDenied(): Promise<void> {
  const { req, res, out } = mockReqRes('POST', { cap: 0 });
  await handler(req, res);
  assert(out.statusCode === 403, `expected 403, got ${out.statusCode}`);
}

// 2. amount below 50 lamports → 400.
export async function testTipAmountTooSmall(): Promise<void> {
  const { req, res, out } = mockReqRes('POST', {
    to_creator_pubkey: 'a'.repeat(64),
    content_id: '11111111-2222-3333-4444-555555555555',
    amount_lamports: 10,
    success_url: 'https://apocky.com/ok',
    cancel_url: 'https://apocky.com/cancel',
    cap: CONTENT_CAP_TIP,
  });
  await handler(req, res);
  assert(out.statusCode === 400, `expected 400 amount-too-small, got ${out.statusCode}`);
}

// 3. valid input + STRIPE_SECRET_KEY missing → stub-mode 200.
export async function testTipStubMode(): Promise<void> {
  const prevKey = process.env['STRIPE_SECRET_KEY'];
  delete process.env['STRIPE_SECRET_KEY'];
  const { _resetStripeForTests } = await import('@/lib/stripe');
  _resetStripeForTests();
  const { req, res, out } = mockReqRes('POST', {
    to_creator_pubkey: 'a'.repeat(64),
    content_id: '11111111-2222-3333-4444-555555555555',
    amount_lamports: 1000,
    success_url: 'https://apocky.com/ok',
    cancel_url: 'https://apocky.com/cancel',
    cap: CONTENT_CAP_TIP,
  });
  await handler(req, res);
  if (prevKey !== undefined) process.env['STRIPE_SECRET_KEY'] = prevKey;
  assert(out.statusCode === 200, `expected 200 stub, got ${out.statusCode}`);
  const b = out.body as { stub?: boolean };
  assert(b.stub === true, 'expected stub:true');
}

// 4. fee_estimate-arithmetic matches Rust formula.
export function testTipFeeEstimate(): void {
  // 1000 → 29 + 30 = 59
  assert(feeEstimateLamports(1000) === 59, `expected 59, got ${feeEstimateLamports(1000)}`);
  // 100_000 → 2900 + 30 = 2930
  assert(feeEstimateLamports(100_000) === 2930, 'fee 100k mismatch');
  // 50 minimum → 2 (ceiling 1.45) + 30 = 32
  assert(feeEstimateLamports(50) === 32, 'fee 50 mismatch');
}

declare const require: { main?: unknown } | undefined;
declare const module: { id?: string } | undefined;
const isMain =
  typeof require !== 'undefined' &&
  typeof module !== 'undefined' &&
  require.main === module;
if (isMain) {
  void (async () => {
    await testTipCapDenied();
    await testTipAmountTooSmall();
    await testTipStubMode();
    testTipFeeEstimate();
    // eslint-disable-next-line no-console
    console.log('content/tip.ts : OK · 4 inline tests passed');
  })();
}
