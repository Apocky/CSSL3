import type { NextApiRequest, NextApiResponse } from 'next';
import handler from '@/pages/api/payments/stripe/checkout';
import type { ContainmentResponse } from '@/lib/containment';

const out: { status: number; body: ContainmentResponse | null; headers: Record<string, string> } = {
  status: 0, body: null, headers: {},
};
const req = {
  method: 'POST',
  headers: { 'x-loa-cap': String(Number.MAX_SAFE_INTEGER) },
  body: { product_id: 'attacker-controlled' },
} as unknown as NextApiRequest;
const res = {
  status(code: number) { out.status = code; return this; },
  json(body: ContainmentResponse) { out.body = body; return this; },
  setHeader(name: string, value: string) { out.headers[name.toLowerCase()] = value; return this; },
} as unknown as NextApiResponse<ContainmentResponse>;

handler(req, res);
if (out.status !== 410 || out.body?.reason_code !== 'temporary_security_containment') {
  throw new Error(`checkout must fail closed with typed 410; got ${out.status}`);
}
if (!out.headers['cache-control']?.includes('no-store')) throw new Error('checkout 410 must be no-store');
console.log('stripe-checkout.test : OK · checkout is retired');
