// cssl-edge · /api/payments/stripe/checkout
// W9-A1 · Stripe Checkout-Session create wrapper.
//
// POST { product_id · success_url · cancel_url · player_id? · idempotency_key? }
//   → { url · session_id }   (200) when STRIPE_SECRET_KEY + price-id configured
//   → { stub: true · todo }  (200) when stub-mode (no Stripe key)
//   → { evt:'audit', status:'denied', reason }  (403) when cap-gate fails
//   → { error }              (400) when bad-input
//
// All routes audit-emit · sovereign-bypass-RECORDED.

import type { NextApiRequest, NextApiResponse } from 'next';
import type Stripe from 'stripe';
import { logHit, envelope, stubEnvelope } from '@/lib/response';
import { auditEvent, deny, logEvent } from '@/lib/audit';
import { checkCap, STRIPE_CHECKOUT_INIT } from '@/lib/cap';
import { isSovereignFromIncoming } from '@/lib/sovereign';
import { findProduct, getStripe, resolvePriceId } from '@/lib/stripe';

interface CheckoutRequest {
  product_id?: string;
  success_url?: string;
  cancel_url?: string;
  player_id?: string;
  idempotency_key?: string;
  cap?: number;
  sovereign?: boolean;
}

interface CheckoutSuccess {
  ok: true;
  url: string;
  session_id: string;
  product_id: string;
  served_by: string;
  ts: string;
}

interface CheckoutStub {
  stub: true;
  todo: string;
  product_id: string;
  served_by: string;
  ts: string;
}

interface CheckoutError {
  ok: false;
  error: string;
  served_by: string;
  ts: string;
}

type Resp = CheckoutSuccess | CheckoutStub | CheckoutError;

// HTTP-allowlist + URL-shape guard. Rejects javascript:/data: schemes.
function isSafeUrl(u: string): boolean {
  if (typeof u !== 'string' || u.length < 8 || u.length > 2048) return false;
  try {
    const parsed = new URL(u);
    return parsed.protocol === 'https:' || parsed.protocol === 'http:';
  } catch {
    return false;
  }
}

// uuid-v7-ish fallback when client supplies no idempotency-key. Not strictly
// monotonic but Stripe only requires per-key-stability over 24h.
function genIdempotencyKey(): string {
  const ts = Date.now().toString(16);
  const rand = Math.floor(Math.random() * 0xffffffff).toString(16).padStart(8, '0');
  return `cssl-edge-${ts}-${rand}`;
}

export default async function handler(req: NextApiRequest, res: NextApiResponse<Resp>): Promise<void> {
  logHit('payments.stripe.checkout', { method: req.method ?? 'POST' });
  const env = envelope();

  if (req.method !== 'POST') {
    res.status(405).json({ ok: false, error: 'POST only', ...env });
    return;
  }

  const body = (req.body ?? {}) as CheckoutRequest;
  const productId = typeof body.product_id === 'string' ? body.product_id : '';
  const successUrl = typeof body.success_url === 'string' ? body.success_url : '';
  const cancelUrl = typeof body.cancel_url === 'string' ? body.cancel_url : '';

  if (productId.length === 0) {
    res.status(400).json({ ok: false, error: 'product_id required', ...env });
    return;
  }
  const product = findProduct(productId);
  if (product === null) {
    res.status(400).json({ ok: false, error: `unknown product_id : ${productId}`, ...env });
    return;
  }
  if (!isSafeUrl(successUrl) || !isSafeUrl(cancelUrl)) {
    res.status(400).json({ ok: false, error: 'success_url + cancel_url must be valid http(s) URLs', ...env });
    return;
  }

  // ── cap-gate · default-DENY ────────────────────────────────────────────
  const sovereign = isSovereignFromIncoming(req.headers, body.sovereign);
  const callerCap = typeof body.cap === 'number' ? body.cap : 0;
  const decision = checkCap(callerCap, STRIPE_CHECKOUT_INIT, sovereign);
  if (!decision.ok) {
    const d = deny(decision.reason ?? 'cap denied', STRIPE_CHECKOUT_INIT);
    logEvent(d.body);
    res.status(d.status).json({ ok: false, error: 'cap denied · STRIPE_CHECKOUT_INIT (0x200) required', ...env });
    return;
  }

  // ── stub-mode fallback when STRIPE_SECRET_KEY missing ──────────────────
  const stripe = getStripe();
  const priceId = resolvePriceId(product);
  if (stripe === null || priceId === null) {
    logEvent(
      auditEvent('payments.checkout.stub', STRIPE_CHECKOUT_INIT, sovereign, 'ok', {
        product_id: productId,
        reason: stripe === null ? 'STRIPE_SECRET_KEY-missing' : 'price-env-missing',
      })
    );
    res.status(200).json({
      ...stubEnvelope(`set STRIPE_SECRET_KEY + ${product.stripe_price_env} on Vercel`),
      product_id: productId,
    });
    return;
  }

  // ── live Stripe call ───────────────────────────────────────────────────
  const idempotencyKey =
    typeof body.idempotency_key === 'string' && body.idempotency_key.length > 0
      ? body.idempotency_key
      : genIdempotencyKey();

  try {
    const mode = product.tier === 'subscription' ? 'subscription' : 'payment';
    const sessionParams: Stripe.Checkout.SessionCreateParams = {
      mode: mode as 'subscription' | 'payment',
      line_items: [{ price: priceId, quantity: 1 }],
      success_url: successUrl,
      cancel_url: cancelUrl,
      ...(typeof body.player_id === 'string' && body.player_id.length > 0
        ? { client_reference_id: body.player_id }
        : {}),
      metadata: {
        product_id: productId,
        cssl_edge_version: '0.1.0',
        ...(typeof body.player_id === 'string' ? { player_id: body.player_id } : {}),
      },
    };
    const session = await stripe.checkout.sessions.create(sessionParams, { idempotencyKey });

    logEvent(
      auditEvent('payments.checkout.created', STRIPE_CHECKOUT_INIT, sovereign, 'ok', {
        product_id: productId,
        session_id: session.id,
      })
    );
    res.status(200).json({
      ok: true,
      url: session.url ?? '',
      session_id: session.id,
      product_id: productId,
      ...env,
    });
  } catch (err) {
    const msg = err instanceof Error ? err.message : 'unknown error';
    logEvent(
      auditEvent('payments.checkout.error', STRIPE_CHECKOUT_INIT, sovereign, 'error', {
        product_id: productId,
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

// 1. missing product_id → 400.
export async function testCheckoutMissingProductId(): Promise<void> {
  const { req, res, out } = mockReqRes('POST', { cap: STRIPE_CHECKOUT_INIT });
  await handler(req, res);
  assert(out.statusCode === 400, `expected 400, got ${out.statusCode}`);
}

// 2. valid input + STRIPE_SECRET_KEY missing → stub-mode 200.
export async function testCheckoutStubModeShape(): Promise<void> {
  const prevKey = process.env['STRIPE_SECRET_KEY'];
  delete process.env['STRIPE_SECRET_KEY'];
  // Force fresh getStripe() resolution — singleton must be reset.
  const { _resetStripeForTests } = await import('@/lib/stripe');
  _resetStripeForTests();
  const { req, res, out } = mockReqRes('POST', {
    product_id: 'loa-cosmetic-mycelial-bloom',
    success_url: 'https://apocky.com/account?paid=1',
    cancel_url: 'https://apocky.com/buy?cancelled=1',
    cap: STRIPE_CHECKOUT_INIT,
  });
  await handler(req, res);
  if (prevKey !== undefined) process.env['STRIPE_SECRET_KEY'] = prevKey;
  assert(out.statusCode === 200, `expected 200 stub, got ${out.statusCode}`);
  const body = out.body as { stub?: boolean; product_id?: string };
  assert(body.stub === true, `expected stub:true, got ${JSON.stringify(body.stub)}`);
  assert(body.product_id === 'loa-cosmetic-mycelial-bloom', 'product_id roundtrip');
}

// 3. cap=0 + sovereign=false → 403.
export async function testCheckoutCapDenied(): Promise<void> {
  const { req, res, out } = mockReqRes('POST', {
    product_id: 'loa-cosmetic-mycelial-bloom',
    success_url: 'https://apocky.com/account',
    cancel_url: 'https://apocky.com/buy',
    cap: 0,
  });
  await handler(req, res);
  assert(out.statusCode === 403, `expected 403, got ${out.statusCode}`);
}

declare const require: { main?: unknown } | undefined;
declare const module: { id?: string } | undefined;
const isMain =
  typeof require !== 'undefined' &&
  typeof module !== 'undefined' &&
  require.main === module;
if (isMain) {
  void (async () => {
    await testCheckoutMissingProductId();
    await testCheckoutStubModeShape();
    await testCheckoutCapDenied();
    // eslint-disable-next-line no-console
    console.log('payments/stripe/checkout.ts : OK · 3 inline tests passed');
  })();
}
