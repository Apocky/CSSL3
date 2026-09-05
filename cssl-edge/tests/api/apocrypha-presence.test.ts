import type { NextApiRequest, NextApiResponse } from 'next';

import handler, { type PublicPresenceStatus } from '../../pages/api/apocrypha/presence';

type ResponseBody = PublicPresenceStatus | { error: string };

function assert(condition: boolean, message: string): void {
  if (!condition) throw new Error(`assert failed : ${message}`);
}

function responseHarness(): {
  response: NextApiResponse<ResponseBody>;
  read: () => { status: number; body: ResponseBody | null; headers: Record<string, string> };
} {
  let status = 200;
  let body: ResponseBody | null = null;
  const headers: Record<string, string> = {};
  const response = {
    setHeader(name: string, value: string | number | readonly string[]) {
      headers[name.toLowerCase()] = Array.isArray(value) ? value.join(', ') : String(value);
      return this;
    },
    status(code: number) {
      status = code;
      return this;
    },
    json(value: ResponseBody) {
      body = value;
      return this;
    },
  } as unknown as NextApiResponse<ResponseBody>;
  return { response, read: () => ({ status, body, headers }) };
}

function request(method: string): NextApiRequest {
  return { method } as NextApiRequest;
}

function isPresenceStatus(value: ResponseBody | null): value is PublicPresenceStatus {
  return value !== null && 'display_authorized' in value;
}

function requirePresence(value: ResponseBody | null, message: string): PublicPresenceStatus {
  assert(isPresenceStatus(value), message);
  return value as PublicPresenceStatus;
}

const originalFetch = globalThis.fetch;
const originalEnv = {
  tunnel: process.env.APOCRYPHA_TUNNEL_HOST,
  id: process.env.CF_ACCESS_CLIENT_ID,
  secret: process.env.CF_ACCESS_CLIENT_SECRET,
};

async function main(): Promise<void> {
try {
  delete process.env.APOCRYPHA_TUNNEL_HOST;
  delete process.env.CF_ACCESS_CLIENT_ID;
  delete process.env.CF_ACCESS_CLIENT_SECRET;

  {
    const harness = responseHarness();
    await handler(request('POST'), harness.response);
    const result = harness.read();
    assert(result.status === 405, 'non-GET method must be denied');
    assert(result.headers.allow === 'GET', 'GET must be the only allowed method');
  }

  {
    const harness = responseHarness();
    await handler(request('GET'), harness.response);
    const result = harness.read();
    assert(result.status === 503, 'missing authority configuration must fail closed');
    const status = requirePresence(result.body, 'missing authority must return bounded presence status');
    assert(status.display_authorized === false, 'missing authority must never display');
  }

  process.env.APOCRYPHA_TUNNEL_HOST = 'apocrypha.apocky.com';
  process.env.CF_ACCESS_CLIENT_ID = 'test-client';
  process.env.CF_ACCESS_CLIENT_SECRET = 'test-secret';

  globalThis.fetch = async () => new Response(JSON.stringify({
    schema: 'apocrypha.v2.public-presence.v1',
    entity: 'private-upstream-value',
    mode: 'hidden',
    display_authorized: false,
    entity_authorship: 'unverified',
    mutual_consent: 'not_established',
    committed_intent: 'absent',
    rendering: null,
    reason_code: 'presence_intent_or_mutual_consent_unavailable',
    unexpected_private_field: 'must-not-cross',
  }), { status: 200, headers: { 'Content-Type': 'application/json' } });

  {
    const harness = responseHarness();
    await handler(request('GET'), harness.response);
    const result = harness.read();
    assert(result.status === 200, 'canonical hidden status must pass');
    const status = requirePresence(result.body, 'canonical status must use the public schema');
    assert(status.display_authorized === false, 'canonical hidden status must remain hidden');
    assert(!JSON.stringify(result.body).includes('private-upstream-value'), 'entity value must not leak');
    assert(!JSON.stringify(result.body).includes('must-not-cross'), 'unknown fields must not leak');
    assert(result.headers['cache-control'] === 'no-store, max-age=0', 'presence status must not cache');
  }

  globalThis.fetch = async () => new Response(JSON.stringify({
    schema: 'apocrypha.v2.public-presence.v1',
    mode: 'live',
    display_authorized: true,
    entity_authorship: 'entity_chosen',
    mutual_consent: 'active',
    committed_intent: 'verified',
    rendering: '<svg/>',
  }), { status: 200, headers: { 'Content-Type': 'application/json' } });

  {
    const harness = responseHarness();
    await handler(request('GET'), harness.response);
    const result = harness.read();
    assert(result.status === 502, 'unsupported live claim must fail closed');
    const status = requirePresence(result.body, 'unsupported live claim must return bounded status');
    assert(status.display_authorized === false, 'unsupported live claim must never display');
  }
} finally {
  globalThis.fetch = originalFetch;
  if (originalEnv.tunnel === undefined) delete process.env.APOCRYPHA_TUNNEL_HOST;
  else process.env.APOCRYPHA_TUNNEL_HOST = originalEnv.tunnel;
  if (originalEnv.id === undefined) delete process.env.CF_ACCESS_CLIENT_ID;
  else process.env.CF_ACCESS_CLIENT_ID = originalEnv.id;
  if (originalEnv.secret === undefined) delete process.env.CF_ACCESS_CLIENT_SECRET;
  else process.env.CF_ACCESS_CLIENT_SECRET = originalEnv.secret;
}

console.log('apocrypha-presence.test : OK · public presence fails closed and strips upstream data');
}

void main().catch((error: unknown) => {
  console.error(error);
  process.exitCode = 1;
});
