import type { NextApiRequest, NextApiResponse } from 'next';
import handler from '@/pages/api/payments/stripe/webhook';
import type { ContainmentResponse } from '@/lib/containment';

const out: { status: number; body: ContainmentResponse | null; headers: Record<string, string> } = {
  status: 0, body: null, headers: {},
};
const req = {
  method: 'POST',
  headers: { 'stripe-signature': 'attacker-controlled' },
  body: Buffer.from('{malformed'),
} as unknown as NextApiRequest;
const res = {
  status(code: number) { out.status = code; return this; },
  json(body: ContainmentResponse) { out.body = body; return this; },
  setHeader(name: string, value: string) { out.headers[name.toLowerCase()] = value; return this; },
} as unknown as NextApiResponse<ContainmentResponse>;

handler(req, res);
if (out.status !== 410 || out.body?.reason_code !== 'temporary_security_containment') {
  throw new Error(`webhook must fail closed with typed 410; got ${out.status}`);
}
if (!out.headers['cache-control']?.includes('no-store')) throw new Error('webhook 410 must be no-store');
console.log('stripe-webhook.test : OK · webhook is retired');
