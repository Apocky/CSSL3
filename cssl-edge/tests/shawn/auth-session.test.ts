import type { NextApiRequest, NextApiResponse } from 'next';

import { createSessionHandler } from '@/pages/api/auth/session';
import logoutHandler from '@/pages/api/auth/logout';

function assert(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(`assertion failed: ${message}`);
}

function jwt(exp: number): string {
  const header = Buffer.from(JSON.stringify({ alg: 'none' })).toString('base64url');
  const payload = Buffer.from(JSON.stringify({ exp })).toString('base64url');
  return `${header}.${payload}.signature`;
}

function request(overrides: Partial<NextApiRequest> = {}): NextApiRequest {
  return {
    method: 'POST',
    headers: {
      host: 'apocky.com',
      origin: 'https://apocky.com',
      'x-forwarded-proto': 'https',
    },
    query: {},
    cookies: {},
    ...overrides,
  } as NextApiRequest;
}

function response(): NextApiResponse & { statusCodeValue: number; body: unknown; headers: Record<string, string | string[]> } {
  const state = {
    statusCodeValue: 200,
    body: undefined as unknown,
    headers: {} as Record<string, string | string[]>,
    setHeader(name: string, value: number | string | readonly string[]) {
      state.headers[name.toLowerCase()] = Array.isArray(value) ? [...value] : String(value);
      return state;
    },
    status(code: number) {
      state.statusCodeValue = code;
      return state;
    },
    json(body: unknown) {
      state.body = body;
      return state;
    },
  };
  return state as unknown as NextApiResponse & typeof state;
}

const now = Date.UTC(2026, 6, 15, 12, 0, 0);
const validToken = jwt(Math.floor(now / 1000) + 900);

async function testValidSession(): Promise<void> {
  let verified: string | null = null;
  const handler = createSessionHandler(async (token) => {
    verified = token;
    return 'valid';
  }, { production: true, now: () => now });
  const req = request({
    headers: {
      host: 'apocky.com',
      origin: 'https://apocky.com',
      'x-forwarded-proto': 'https',
      authorization: `Bearer ${validToken}`,
    },
  });
  const res = response();
  await handler(req, res);
  assert(res.statusCodeValue === 200, 'valid bearer creates session');
  assert(verified === validToken, 'server verifies exact bearer');
  const cookies = res.headers['set-cookie'];
  assert(Array.isArray(cookies), 'session response emits cookie set');
  assert(cookies[0]?.startsWith('__Host-apocky-access-token='), 'production uses __Host cookie');
  assert(cookies[0]?.includes('HttpOnly; Secure; SameSite=Strict'), 'cookie has strict security attributes');
  assert(!cookies.some((cookie) => cookie.startsWith('sb-refresh-token=') && !cookie.includes('Max-Age=0')), 'no refresh token is persisted');
}

async function testRejections(): Promise<void> {
  const handler = createSessionHandler(async () => 'valid', { production: true, now: () => now });

  const crossOrigin = response();
  await handler(request({ headers: { host: 'apocky.com', origin: 'https://evil.example', authorization: `Bearer ${validToken}` } }), crossOrigin);
  assert(crossOrigin.statusCodeValue === 403, 'cross-origin request denied');

  const absent = response();
  await handler(request(), absent);
  assert(absent.statusCodeValue === 401, 'missing bearer denied');

  const expired = response();
  await handler(request({ headers: { host: 'apocky.com', origin: 'https://apocky.com', authorization: `Bearer ${jwt(Math.floor(now / 1000) - 1)}` } }), expired);
  assert(expired.statusCodeValue === 401, 'expired bearer denied before provider call');

  const provider = createSessionHandler(async () => 'unavailable', { now: () => now });
  const unavailable = response();
  await provider(request({ headers: { host: 'apocky.com', origin: 'https://apocky.com', authorization: `Bearer ${validToken}` } }), unavailable);
  assert(unavailable.statusCodeValue === 503, 'provider outage remains distinct from invalid session');
}

async function testLogoutClearsAllSessionSurfaces(): Promise<void> {
  const res = response();
  await logoutHandler(request(), res);
  assert(res.statusCodeValue === 200, 'same-origin logout accepted');
  const cookies = res.headers['set-cookie'];
  assert(Array.isArray(cookies) && cookies.length === 4, 'all current and legacy cookie names cleared');
  assert(cookies.every((cookie) => cookie.includes('Max-Age=0')), 'logout only emits expirations');
}

async function run(): Promise<void> {
  await testValidSession();
  await testRejections();
  await testLogoutClearsAllSessionSurfaces();
  // eslint-disable-next-line no-console
  console.log('shawn/auth-session.test : OK · same-origin verified HttpOnly session + complete logout');
}

run().catch((error) => {
  // eslint-disable-next-line no-console
  console.error(error);
  process.exit(1);
});
