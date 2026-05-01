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
//
// NOTE : we MUST set env-vars via dynamic-key indirection here. Next.js +
// Webpack inline `process.env.NEXT_PUBLIC_*` literals at build-time, so a
// direct assignment like `process.env['NEXT_PUBLIC_SUPABASE_URL'] = 'x'`
// becomes `'<inlined-string>' = 'x'` — a syntax error in the production bundle.
// Indirection through a variable defeats the inline-substitution.
export function testHealthPaymentsReadyComposite(): void {
  const KEYS = ['STRIPE_SECRET_KEY', 'STRIPE_WEBHOOK_SIGNING_SECRET', 'NEXT_PUBLIC_SUPABASE_URL', 'SUPABASE_ANON_KEY'] as const;
  const VALS = ['sk_test_x', 'whsec_x', 'https://test.supabase.co', 'anon_test'];
  const prev: Record<string, string | undefined> = {};
  for (const k of KEYS) prev[k] = process.env[k];
  for (let i = 0; i < KEYS.length; i++) process.env[KEYS[i] as string] = VALS[i] as string;
  const { req, res, out } = mockReqRes();
  handler(req, res);
  const body = out.body as HealthResponse;
  for (const k of KEYS) {
    if (prev[k] === undefined) delete process.env[k];
    else process.env[k] = prev[k] as string;
  }
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
