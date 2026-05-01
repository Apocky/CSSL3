// cssl-edge · /api/health
// Liveness ping. Always returns 200. Carries commit SHA + integration-config
// booleans so deploys are auditable AND admins can verify Stripe/Supabase env
// is wired without leaking secrets.
//
// W9-bumped : added stripe_configured · supabase_connected · payments_ready.

import type { NextApiRequest, NextApiResponse } from 'next';
import { commitSha, envelope, logHit } from '@/lib/response';

export interface HealthResponse {
  ok: true;
  sha: string;
  served_by: string;
  ts: string;
  version: string;
  // Integration config — booleans only · NEVER leak the actual env-values.
  stripe_configured: boolean;
  stripe_webhook_configured: boolean;
  supabase_connected: boolean;
  // Composite readiness flag — convenience for status-page polls.
  payments_ready: boolean;
}

function isSet(name: string): boolean {
  const v = process.env[name];
  return typeof v === 'string' && v.length > 0;
}

export default function handler(
  req: NextApiRequest,
  res: NextApiResponse<HealthResponse>
): void {
  logHit('health', { method: req.method ?? 'GET' });

  const env = envelope();
  const stripeConfigured = isSet('STRIPE_SECRET_KEY');
  const webhookConfigured = isSet('STRIPE_WEBHOOK_SIGNING_SECRET');
  const supabaseConnected = isSet('NEXT_PUBLIC_SUPABASE_URL') && isSet('SUPABASE_ANON_KEY');

  const body: HealthResponse = {
    ok: true,
    sha: commitSha(),
    served_by: env.served_by,
    ts: env.ts,
    version: process.env['CSSL_EDGE_VERSION'] ?? '0.1.0',
    stripe_configured: stripeConfigured,
    stripe_webhook_configured: webhookConfigured,
    supabase_connected: supabaseConnected,
    payments_ready: stripeConfigured && webhookConfigured && supabaseConnected,
  };

  res.status(200).json(body);
}

// ─── Inline tests for W9-bump · framework-agnostic ────────────────────────

interface MockedResponse {
  statusCode: number;
  body: unknown;
}

function mockReqRes(): { req: NextApiRequest; res: NextApiResponse<HealthResponse>; out: MockedResponse } {
  const out: MockedResponse = { statusCode: 0, body: null };
  const req = { method: 'GET', query: {}, headers: {}, body: undefined } as unknown as NextApiRequest;
  const res = {
    status(code: number) { out.statusCode = code; return this; },
    json(payload: unknown) { out.body = payload; return this; },
    setHeader(_k: string, _v: string) { return this; },
  } as unknown as NextApiResponse<HealthResponse>;
  return { req, res, out };
}

function assert(cond: boolean, msg: string): void {
  if (!cond) throw new Error(`assert failed : ${msg}`);
}

// 1. Health response carries new W9 keys.
export function testHealthCarriesW9Keys(): void {
  const { req, res, out } = mockReqRes();
  handler(req, res);
  const body = out.body as Record<string, unknown>;
  for (const k of ['stripe_configured', 'stripe_webhook_configured', 'supabase_connected', 'payments_ready']) {
    assert(typeof body[k] === 'boolean', `${k} must be boolean`);
  }
}

// 2. payments_ready is true iff all three integration env-vars are set.
export function testHealthPaymentsReadyComposite(): void {
  const prev = {
    sk: process.env['STRIPE_SECRET_KEY'],
    ws: process.env['STRIPE_WEBHOOK_SIGNING_SECRET'],
    su: process.env['NEXT_PUBLIC_SUPABASE_URL'],
    sa: process.env['SUPABASE_ANON_KEY'],
  };
  process.env['STRIPE_SECRET_KEY'] = 'sk_test_x';
  process.env['STRIPE_WEBHOOK_SIGNING_SECRET'] = 'whsec_x';
  process.env['NEXT_PUBLIC_SUPABASE_URL'] = 'https://test.supabase.co';
  process.env['SUPABASE_ANON_KEY'] = 'anon_test';
  const { req, res, out } = mockReqRes();
  handler(req, res);
  const body = out.body as HealthResponse;
  // restore
  if (prev.sk === undefined) delete process.env['STRIPE_SECRET_KEY']; else process.env['STRIPE_SECRET_KEY'] = prev.sk;
  if (prev.ws === undefined) delete process.env['STRIPE_WEBHOOK_SIGNING_SECRET']; else process.env['STRIPE_WEBHOOK_SIGNING_SECRET'] = prev.ws;
  if (prev.su === undefined) delete process.env['NEXT_PUBLIC_SUPABASE_URL']; else process.env['NEXT_PUBLIC_SUPABASE_URL'] = prev.su;
  if (prev.sa === undefined) delete process.env['SUPABASE_ANON_KEY']; else process.env['SUPABASE_ANON_KEY'] = prev.sa;
  assert(body.payments_ready === true, 'all-env-set → payments_ready true');
}

declare const require: { main?: unknown } | undefined;
declare const module: { id?: string } | undefined;
const isMain =
  typeof require !== 'undefined' &&
  typeof module !== 'undefined' &&
  require.main === module;
if (isMain) {
  testHealthCarriesW9Keys();
  testHealthPaymentsReadyComposite();
  // eslint-disable-next-line no-console
  console.log('health.ts : OK · 2 W9-inline tests passed');
}
