// cssl-edge · /api/payments/stripe/webhook
// W9-A2 · Stripe webhook receiver.
//
// Stripe POSTs raw-body events here · we verify HMAC via
// stripe.webhooks.constructEvent + STRIPE_WEBHOOK_SIGNING_SECRET, then upsert
// entitlements/refund-records into Supabase.
//
// Idempotency : stripe-event-id is UNIQUE in stripe_webhook_events table —
// duplicate replays no-op.
//
// Handled events :
//   - checkout.session.completed         → grant_entitlement(player, product)
//   - customer.subscription.updated      → upsert subscription state
//   - customer.subscription.deleted      → revoke subscription
//   - payment_intent.succeeded           → log only · entitlement granted on session-complete
//   - charge.refunded                    → revoke_entitlement(player, product)
//
// CRITICAL : Next.js default body-parser corrupts the raw bytes Stripe signed.
// We disable it via `config.api.bodyParser = false` and stream the body
// ourselves. Without this, signature-verification ALWAYS fails on prod.

import type { NextApiRequest, NextApiResponse } from 'next';
import { logHit, envelope } from '@/lib/response';
import { auditEvent, logEvent } from '@/lib/audit';
import { getStripe, getWebhookSigningSecret } from '@/lib/stripe';
import { getSupabase } from '@/lib/supabase';
import type Stripe from 'stripe';

export const config = {
  api: { bodyParser: false },
};

interface WebhookOk {
  ok: true;
  event_type: string;
  event_id: string;
  duplicate: boolean;
  served_by: string;
  ts: string;
}

interface WebhookStub {
  stub: true;
  todo: string;
  served_by: string;
  ts: string;
}

interface WebhookErr {
  ok: false;
  error: string;
  served_by: string;
  ts: string;
}

type Resp = WebhookOk | WebhookStub | WebhookErr;

// Read raw bytes from the request stream — bodyParser is disabled so we own
// the whole stream. Capped at 1 MB to bound memory in worst-case.
async function readRawBody(req: NextApiRequest): Promise<Buffer> {
  const chunks: Buffer[] = [];
  let total = 0;
  for await (const chunk of req as AsyncIterable<Buffer>) {
    const buf = chunk instanceof Buffer ? chunk : Buffer.from(chunk);
    total += buf.length;
    if (total > 1024 * 1024) {
      throw new Error('webhook body too large (>1 MB)');
    }
    chunks.push(buf);
  }
  return Buffer.concat(chunks);
}

// Persist event-id for idempotency. Returns true when this is a NEW event
// (write succeeded), false when DUPLICATE (UNIQUE-constraint trip — already
// processed). Stub-mode (no Supabase) treats everything as new.
async function recordEvent(eventId: string, eventType: string, payload: unknown): Promise<boolean> {
  const sb = getSupabase();
  if (sb === null) return true;
  const { error } = await sb.from('stripe_webhook_events').insert({
    stripe_event_id: eventId,
    event_type: eventType,
    payload: payload as Record<string, unknown>,
    processed_at: new Date().toISOString(),
  });
  // 23505 = unique-constraint violation → duplicate
  if (error && error.code === '23505') return false;
  if (error) throw new Error(`webhook persist error : ${error.message}`);
  return true;
}

interface CheckoutSessionLike {
  id: string;
  client_reference_id: string | null;
  metadata: Record<string, string> | null;
  customer: string | null;
}

async function handleCheckoutCompleted(session: CheckoutSessionLike): Promise<void> {
  const sb = getSupabase();
  if (sb === null) return;
  const playerId = session.client_reference_id ?? session.metadata?.['player_id'] ?? null;
  const productId = session.metadata?.['product_id'] ?? null;
  if (playerId === null || productId === null) return;
  await sb.rpc('grant_entitlement', {
    p_player_id: playerId,
    p_product_id: productId,
    p_session_id: session.id,
  });
  if (typeof session.customer === 'string' && session.customer.length > 0) {
    await sb.from('stripe_customers').upsert(
      {
        player_id: playerId,
        stripe_customer_id: session.customer,
      },
      { onConflict: 'player_id' }
    );
  }
}

interface SubscriptionLike {
  id: string;
  status: string;
  customer: string;
  metadata: Record<string, string> | null;
  items: { data: ReadonlyArray<{ price: { id: string } }> };
}

async function handleSubscriptionUpsert(sub: SubscriptionLike, deleted: boolean): Promise<void> {
  const sb = getSupabase();
  if (sb === null) return;
  const productId = sub.metadata?.['product_id'] ?? null;
  if (productId === null) return;
  // resolve player via stripe_customers map
  const { data: cust } = await sb
    .from('stripe_customers')
    .select('player_id')
    .eq('stripe_customer_id', sub.customer)
    .maybeSingle();
  const playerId = (cust as { player_id: string } | null)?.player_id ?? null;
  if (playerId === null) return;
  if (deleted || sub.status !== 'active') {
    await sb.rpc('revoke_entitlement', { p_player_id: playerId, p_product_id: productId });
  } else {
    await sb.rpc('grant_entitlement', {
      p_player_id: playerId,
      p_product_id: productId,
      p_session_id: sub.id,
    });
  }
}

interface ChargeLike {
  id: string;
  amount_refunded: number;
  metadata: Record<string, string> | null;
  customer: string | null;
}

async function handleChargeRefunded(charge: ChargeLike): Promise<void> {
  const sb = getSupabase();
  if (sb === null) return;
  const productId = charge.metadata?.['product_id'] ?? null;
  if (productId === null) return;
  if (charge.customer === null) return;
  const { data: cust } = await sb
    .from('stripe_customers')
    .select('player_id')
    .eq('stripe_customer_id', charge.customer)
    .maybeSingle();
  const playerId = (cust as { player_id: string } | null)?.player_id ?? null;
  if (playerId === null) return;
  await sb.rpc('revoke_entitlement', { p_player_id: playerId, p_product_id: productId });
}

export default async function handler(req: NextApiRequest, res: NextApiResponse<Resp>): Promise<void> {
  logHit('payments.stripe.webhook', { method: req.method ?? 'POST' });
  const env = envelope();

  if (req.method !== 'POST') {
    res.status(405).json({ ok: false, error: 'POST only', ...env });
    return;
  }

  const stripe = getStripe();
  const secret = getWebhookSigningSecret();
  if (stripe === null || secret === null) {
    logEvent(
      auditEvent('payments.webhook.stub', 0, false, 'ok', {
        reason: stripe === null ? 'STRIPE_SECRET_KEY-missing' : 'STRIPE_WEBHOOK_SIGNING_SECRET-missing',
      })
    );
    res.status(200).json({
      stub: true,
      todo: 'set STRIPE_SECRET_KEY + STRIPE_WEBHOOK_SIGNING_SECRET on Vercel',
      ...env,
    });
    return;
  }

  let raw: Buffer;
  try {
    raw = await readRawBody(req);
  } catch (err) {
    const msg = err instanceof Error ? err.message : 'unknown';
    res.status(413).json({ ok: false, error: `read body : ${msg}`, ...env });
    return;
  }

  const sigHdr = req.headers['stripe-signature'];
  const sig = Array.isArray(sigHdr) ? sigHdr[0] : sigHdr;
  if (typeof sig !== 'string' || sig.length === 0) {
    logEvent(auditEvent('payments.webhook.no-sig', 0, false, 'denied', {}));
    res.status(400).json({ ok: false, error: 'missing stripe-signature header', ...env });
    return;
  }

  let event: Stripe.Event;
  try {
    event = stripe.webhooks.constructEvent(raw, sig, secret);
  } catch (err) {
    const msg = err instanceof Error ? err.message : 'invalid signature';
    logEvent(auditEvent('payments.webhook.bad-sig', 0, false, 'denied', { err: msg }));
    res.status(400).json({ ok: false, error: `signature verify failed : ${msg}`, ...env });
    return;
  }

  let isNew: boolean;
  try {
    isNew = await recordEvent(event.id, event.type, event);
  } catch (err) {
    const msg = err instanceof Error ? err.message : 'unknown';
    logEvent(auditEvent('payments.webhook.persist-err', 0, false, 'error', { err: msg, event_id: event.id }));
    res.status(500).json({ ok: false, error: `persist : ${msg}`, ...env });
    return;
  }

  if (!isNew) {
    logEvent(
      auditEvent('payments.webhook.duplicate', 0, false, 'ok', {
        event_id: event.id,
        event_type: event.type,
      })
    );
    res.status(200).json({
      ok: true,
      event_type: event.type,
      event_id: event.id,
      duplicate: true,
      ...env,
    });
    return;
  }

  try {
    switch (event.type) {
      case 'checkout.session.completed':
        await handleCheckoutCompleted(event.data.object as unknown as CheckoutSessionLike);
        break;
      case 'customer.subscription.updated':
        await handleSubscriptionUpsert(event.data.object as unknown as SubscriptionLike, false);
        break;
      case 'customer.subscription.deleted':
        await handleSubscriptionUpsert(event.data.object as unknown as SubscriptionLike, true);
        break;
      case 'payment_intent.succeeded':
        // Log-only — entitlement is granted on checkout.session.completed.
        break;
      case 'charge.refunded':
        await handleChargeRefunded(event.data.object as unknown as ChargeLike);
        break;
      default:
        // Unknown event types are recorded but not actioned. Stripe expects 2xx.
        break;
    }
  } catch (err) {
    const msg = err instanceof Error ? err.message : 'unknown';
    logEvent(
      auditEvent('payments.webhook.handler-err', 0, false, 'error', {
        event_id: event.id,
        event_type: event.type,
        err: msg,
      })
    );
    res.status(500).json({ ok: false, error: `handler : ${msg}`, ...env });
    return;
  }

  logEvent(
    auditEvent('payments.webhook.processed', 0, false, 'ok', {
      event_id: event.id,
      event_type: event.type,
    })
  );
  res.status(200).json({
    ok: true,
    event_type: event.type,
    event_id: event.id,
    duplicate: false,
    ...env,
  });
}

// ─── Inline tests · framework-agnostic ─────────────────────────────────────

interface MockedResponse {
  statusCode: number;
  body: unknown;
}

function mockReqRes(
  method: string,
  body: AsyncIterable<Buffer> | Buffer | undefined,
  headers: Record<string, string> = {}
): { req: NextApiRequest; res: NextApiResponse; out: MockedResponse } {
  const out: MockedResponse = { statusCode: 0, body: null };
  // emulate raw-stream by making req an AsyncIterable<Buffer>
  const stream = body instanceof Buffer
    ? (async function* () { yield body; })()
    : body ?? (async function* () { /* empty */ })();
  const req = Object.assign(stream, { method, query: {}, headers }) as unknown as NextApiRequest;
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

// 1. STRIPE_SECRET_KEY missing → stub-mode 200.
export async function testWebhookStubMode(): Promise<void> {
  const prevKey = process.env['STRIPE_SECRET_KEY'];
  const prevSec = process.env['STRIPE_WEBHOOK_SIGNING_SECRET'];
  delete process.env['STRIPE_SECRET_KEY'];
  delete process.env['STRIPE_WEBHOOK_SIGNING_SECRET'];
  const { _resetStripeForTests } = await import('@/lib/stripe');
  _resetStripeForTests();
  const { req, res, out } = mockReqRes('POST', undefined);
  await handler(req, res);
  if (prevKey !== undefined) process.env['STRIPE_SECRET_KEY'] = prevKey;
  if (prevSec !== undefined) process.env['STRIPE_WEBHOOK_SIGNING_SECRET'] = prevSec;
  assert(out.statusCode === 200, `expected 200 stub, got ${out.statusCode}`);
  const body = out.body as { stub?: boolean };
  assert(body.stub === true, 'stub-mode body shape');
}

// 2. method other than POST → 405.
export async function testWebhookMethodNotAllowed(): Promise<void> {
  const { req, res, out } = mockReqRes('GET', undefined);
  await handler(req, res);
  assert(out.statusCode === 405, `expected 405, got ${out.statusCode}`);
}

// 3. Missing stripe-signature when configured → 400.
//    We can only exercise this if STRIPE_SECRET_KEY is set; otherwise the
//    stub-fallback shorts first. Simulate by exporting helper-fn instead.
//    Below is the structural shape-guard.
export async function testWebhookConfigShapeGuard(): Promise<void> {
  // Sanity check : module exports `config` with bodyParser disabled. This is
  // load-bearing — Next.js with default body-parser CORRUPTS the raw bytes
  // Stripe HMAC-signed.
  const mod = await import('@/pages/api/payments/stripe/webhook');
  const cfg = (mod as unknown as { config?: { api?: { bodyParser?: boolean } } }).config;
  assert(cfg?.api?.bodyParser === false, 'webhook MUST disable bodyParser');
}

// 4. Idempotency : second call with same event-id is a duplicate.
//    In stub-mode (no Supabase) we treat all as new — so we just verify the
//    handler tolerates re-entry without crashing.
export async function testWebhookIdempotencyStub(): Promise<void> {
  const { req: r1, res: s1, out: o1 } = mockReqRes('POST', undefined);
  const { req: r2, res: s2, out: o2 } = mockReqRes('POST', undefined);
  await handler(r1, s1);
  await handler(r2, s2);
  // Both should succeed (stub-mode). Different responses are fine.
  assert(o1.statusCode === 200 && o2.statusCode === 200, 'stub re-entry must remain 200');
}

declare const require: { main?: unknown } | undefined;
declare const module: { id?: string } | undefined;
const isMain =
  typeof require !== 'undefined' &&
  typeof module !== 'undefined' &&
  require.main === module;
if (isMain) {
  void (async () => {
    await testWebhookMethodNotAllowed();
    await testWebhookStubMode();
    await testWebhookConfigShapeGuard();
    await testWebhookIdempotencyStub();
    // eslint-disable-next-line no-console
    console.log('payments/stripe/webhook.ts : OK · 4 inline tests passed');
  })();
}
