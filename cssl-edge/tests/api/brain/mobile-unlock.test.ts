import assert from 'node:assert/strict';
import type { NextApiRequest, NextApiResponse } from 'next';

import { issueAuthAttempt } from '@/lib/auth-fence';
import { createMiniBrainUnlockHandler } from '@/pages/api/brain/mobile/unlock';

interface Output {
  statusCode: number;
  body: Record<string, unknown>;
  headers: Record<string, string>;
}

const now = Date.UTC(2026, 8, 4, 12, 0, 0);
const nowSeconds = Math.floor(now / 1_000);
const lockGeneration = '11111111-1111-4111-8111-111111111111';

function jwt(input: { readonly iat: number; readonly method?: string | null; readonly authenticationTimestamp?: number }): string {
  const header = Buffer.from(JSON.stringify({ alg: 'none' })).toString('base64url');
  const payload = Buffer.from(JSON.stringify({
    sub: 'owner-test',
    session_id: 'provider-session-test',
    iat: input.iat,
    exp: nowSeconds + 900,
    amr: input.method === null ? [] : [{
      method: input.method ?? 'otp',
      timestamp: input.authenticationTimestamp ?? input.iat,
    }],
  })).toString('base64url');
  return `${header}.${payload}.signature`;
}

function request(): NextApiRequest {
  return {
    method: 'POST',
    headers: { host: 'localhost:3000', origin: 'http://localhost:3000' },
    query: {},
    cookies: {},
    body: {},
  } as NextApiRequest;
}

function response(): { readonly res: NextApiResponse; readonly out: Output } {
  const out: Output = { statusCode: 200, body: {}, headers: {} };
  const res = {
    setHeader(name: string, value: number | string | readonly string[]) {
      out.headers[name.toLowerCase()] = Array.isArray(value) ? value.join(', ') : String(value);
      return this;
    },
    status(statusCode: number) { out.statusCode = statusCode; return this; },
    json(value: Record<string, unknown>) { out.body = value; return this; },
  } as unknown as NextApiResponse;
  return { res, out };
}

function attempt(req: NextApiRequest, issuedAtMs = now - 1_000): string {
  const issued = issueAuthAttempt({ req, mode: 'fresh', nowMs: issuedAtMs, production: false });
  assert(issued, 'fixture auth attempt issued');
  return issued.ticket;
}

async function run(token: string, issuedAtMs = now - 1_000): Promise<Output> {
  const req = request();
  req.body = { lock_generation: lockGeneration, auth_attempt: attempt(req, issuedAtMs) };
  const { res, out } = response();
  const handler = createMiniBrainUnlockHandler({
    now: () => now,
    production: false,
    accessToken: () => token,
    requireOwner: async () => ({
      ok: true,
      user: { id: 'owner-test', email: 'owner@example.com', provider: 'fixture', createdAt: new Date(0).toISOString() },
    }),
  });
  await handler(req, res);
  return out;
}

async function main(): Promise<void> {
  const bypass = process.env.LAZARUS_TEST_AUTH_BYPASS;
  delete process.env.LAZARUS_TEST_AUTH_BYPASS;
  try {
    const valid = await run(jwt({ iat: nowSeconds, method: 'otp' }));
    assert.equal(valid.statusCode, 200, 'fresh interactive OTP authorizes the owner-bound epoch');
    assert.equal(valid.body.lock_generation, lockGeneration, 'unlock returns only the requested epoch');

    for (const [name, token, issuedAtMs] of [
      ['absent AMR', jwt({ iat: nowSeconds, method: null }), now - 1_000],
      ['unknown AMR', jwt({ iat: nowSeconds, method: 'email' }), now - 1_000],
      ['refresh-only AMR', jwt({ iat: nowSeconds, method: 'token_refresh' }), now - 1_000],
      ['same-second pre-attempt AMR', jwt({ iat: nowSeconds, method: 'otp' }), now + 900],
      ['future JWT issue', jwt({ iat: nowSeconds + 5, method: 'otp' }), now - 1_000],
      ['AMR after JWT issue', jwt({ iat: nowSeconds - 1, method: 'otp', authenticationTimestamp: nowSeconds }), now - 2_000],
    ] as const) {
      const rejected = await run(token, issuedAtMs);
      assert.equal(rejected.statusCode, 403, `${name} is denied`);
      assert.equal(rejected.body.code, 'BRAIN_FRESH_REAUTH_REQUIRED', `${name} uses the stable freshness code`);
    }
  } finally {
    if (bypass === undefined) delete process.env.LAZARUS_TEST_AUTH_BYPASS;
    else process.env.LAZARUS_TEST_AUTH_BYPASS = bypass;
  }
  // eslint-disable-next-line no-console
  console.log('mobile-unlock.test : OK · owner route enforces exact interactive freshness');
}

void main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
