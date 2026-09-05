import type { NextApiRequest, NextApiResponse } from 'next';

import {
  AUTH_FENCE_PROTOCOL,
  authSessionBindingValid,
  issueAuthAttempt,
  jwtSessionClaims,
} from '@/lib/auth-fence';
import { clearedSessionCookies } from '@/lib/auth-session';
import { createSessionHandler } from '@/pages/api/auth/session';
import logoutHandler from '@/pages/api/auth/logout';

function assert(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(`assertion failed: ${message}`);
}

function jwt(input: {
  readonly exp: number;
  readonly iat: number;
  readonly sub?: string;
  readonly sessionId?: string;
  readonly method?: string | null;
  readonly authenticationTimestamp?: number;
}): string {
  const header = Buffer.from(JSON.stringify({ alg: 'none' })).toString('base64url');
  const payload = Buffer.from(JSON.stringify({
    exp: input.exp,
    iat: input.iat,
    sub: input.sub ?? 'owner-test',
    session_id: input.sessionId ?? 'provider-session-test',
    amr: input.method === null ? [] : [{
      method: input.method ?? 'otp',
      timestamp: input.authenticationTimestamp ?? input.iat,
    }],
  })).toString('base64url');
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
const nowSeconds = Math.floor(now / 1000);
const validToken = jwt({ exp: nowSeconds + 900, iat: nowSeconds });

function issueFreshAttempt(cookie?: string, issuedAtMs = now - 1_000): string {
  const req = request({ headers: {
    host: 'apocky.com',
    origin: 'https://apocky.com',
    'x-forwarded-proto': 'https',
    ...(cookie ? { cookie } : {}),
  } });
  const attempt = issueAuthAttempt({ req, mode: 'fresh', nowMs: issuedAtMs, production: true });
  assert(attempt, 'fresh attempt issued at current logout fence');
  return attempt.ticket;
}

function fencedSessionRequest(input: {
  readonly token?: string;
  readonly ticket?: string;
  readonly mode?: 'fresh' | 'refresh';
  readonly cookie?: string;
} = {}): NextApiRequest {
  return request({
    body: { mode: input.mode ?? 'fresh' },
    headers: {
      host: 'apocky.com',
      origin: 'https://apocky.com',
      'x-forwarded-proto': 'https',
      authorization: `Bearer ${input.token ?? validToken}`,
      'x-apocky-auth-protocol': AUTH_FENCE_PROTOCOL,
      'x-apocky-auth-attempt': input.ticket ?? issueFreshAttempt(input.cookie),
      ...(input.cookie ? { cookie: input.cookie } : {}),
    },
  });
}

async function testValidSession(): Promise<void> {
  let verified: string | null = null;
  const handler = createSessionHandler(async (token) => {
    verified = token;
    return 'valid';
  }, { production: true, now: () => now });
  const req = fencedSessionRequest();
  const res = response();
  await handler(req, res);
  assert(res.statusCodeValue === 200, 'valid bearer creates session');
  assert(verified === validToken, 'server verifies exact bearer');
  const cookies = res.headers['set-cookie'];
  assert(Array.isArray(cookies), 'session response emits cookie set');
  assert(cookies[0]?.startsWith('__Host-apocky-access-token='), 'production uses __Host cookie');
  assert(cookies[0]?.includes('HttpOnly; Secure; SameSite=Strict'), 'cookie has strict security attributes');
  assert(cookies.some((cookie) => cookie.startsWith('__Host-apocky-session-v2=')), 'session is bound to the logout fence');
  assert(!cookies.some((cookie) => cookie.startsWith('sb-refresh-token=') && !cookie.includes('Max-Age=0')), 'no refresh token is persisted');
}

async function testRejections(): Promise<void> {
  const handler = createSessionHandler(async () => 'valid', { production: true, now: () => now });

  const crossOrigin = response();
  await handler(request({ headers: { host: 'apocky.com', origin: 'https://evil.example', authorization: `Bearer ${validToken}` } }), crossOrigin);
  assert(crossOrigin.statusCodeValue === 403, 'cross-origin request denied');

  const oldClient = response();
  await handler(request({ headers: {
    host: 'apocky.com', origin: 'https://apocky.com', 'x-forwarded-proto': 'https', authorization: `Bearer ${validToken}`,
  } }), oldClient);
  assert(oldClient.statusCodeValue === 428, 'a client without the response-order fence must upgrade');

  const absent = response();
  const absentRequest = fencedSessionRequest();
  delete absentRequest.headers.authorization;
  await handler(absentRequest, absent);
  assert(absent.statusCodeValue === 401, 'missing bearer denied');

  const expired = response();
  await handler(fencedSessionRequest({ token: jwt({ exp: nowSeconds - 1, iat: nowSeconds - 900 }) }), expired);
  assert(expired.statusCodeValue === 401, 'expired bearer denied before provider call');

  const provider = createSessionHandler(async () => 'unavailable', { now: () => now });
  const unavailable = response();
  await provider(fencedSessionRequest(), unavailable);
  assert(unavailable.statusCodeValue === 503, 'provider outage remains distinct from invalid session');

  const futureAttempt = issueFreshAttempt();
  const predatingToken = jwt({ exp: nowSeconds + 900, iat: nowSeconds - 60 });
  const predating = response();
  await handler(fencedSessionRequest({ ticket: futureAttempt, token: predatingToken }), predating);
  assert(predating.statusCodeValue === 409, 'a provider token predating the fresh attempt is denied');
  assert((predating.body as { code?: string }).code === 'AUTH_INTERACTIVE_REAUTH_REQUIRED', 'predating denial is typed');

  const refreshOnly = response();
  const refreshOnlyToken = jwt({
    exp: nowSeconds + 900,
    iat: nowSeconds,
    method: 'token_refresh',
    authenticationTimestamp: nowSeconds,
  });
  await handler(fencedSessionRequest({ ticket: issueFreshAttempt(), token: refreshOnlyToken }), refreshOnly);
  assert(refreshOnly.statusCodeValue === 409, 'a freshly issued token-refresh JWT cannot impersonate interactive reauthentication');
  assert((refreshOnly.body as { code?: string }).code === 'AUTH_INTERACTIVE_REAUTH_REQUIRED', 'refresh-only denial is typed');

  const absentAmr = response();
  await handler(fencedSessionRequest({ token: jwt({ exp: nowSeconds + 900, iat: nowSeconds, method: null }) }), absentAmr);
  assert(absentAmr.statusCodeValue === 409, 'a token without an interactive AMR entry cannot authorize fresh reauthentication');

  const unknownAmr = response();
  await handler(fencedSessionRequest({ token: jwt({ exp: nowSeconds + 900, iat: nowSeconds, method: 'email' }) }), unknownAmr);
  assert(unknownAmr.statusCodeValue === 409, 'an undocumented AMR method remains fail-closed');

  const amrAfterIssue = response();
  const olderIssueAttempt = issueFreshAttempt(undefined, now - 2_000);
  await handler(fencedSessionRequest({
    ticket: olderIssueAttempt,
    token: jwt({
      exp: nowSeconds + 900,
      iat: nowSeconds - 1,
      method: 'otp',
      authenticationTimestamp: nowSeconds,
    }),
  }), amrAfterIssue);
  assert(amrAfterIssue.statusCodeValue === 409, 'an AMR event after its JWT issue time is rejected');

  const subsecondNow = now + 900;
  const ambiguousAttempt = issueFreshAttempt(undefined, subsecondNow);
  const earlierSameSecondToken = jwt({
    exp: nowSeconds + 900,
    iat: nowSeconds,
    method: 'otp',
    authenticationTimestamp: nowSeconds,
  });
  const ambiguous = response();
  const strictHandler = createSessionHandler(async () => 'valid', { production: true, now: () => subsecondNow });
  await strictHandler(fencedSessionRequest({ ticket: ambiguousAttempt, token: earlierSameSecondToken }), ambiguous);
  assert(ambiguous.statusCodeValue === 409, 'a same-second credential that can predate the attempt is denied');
  assert((ambiguous.body as { code?: string }).code === 'AUTH_INTERACTIVE_REAUTH_REQUIRED', 'ambiguous timestamp denial is typed');

  const futureDated = response();
  const futureDatedToken = jwt({
    exp: nowSeconds + 900,
    iat: nowSeconds + 5,
    method: 'otp',
    authenticationTimestamp: nowSeconds + 5,
  });
  await strictHandler(fencedSessionRequest({ ticket: ambiguousAttempt, token: futureDatedToken }), futureDated);
  assert(futureDated.statusCodeValue === 409, 'a future-dated provider credential cannot defeat attempt ordering');
  assert((futureDated.body as { code?: string }).code === 'AUTH_INTERACTIVE_REAUTH_REQUIRED', 'future-dated denial is typed');
}

async function testLogoutSessionResponseOrdering(): Promise<void> {
  const attemptTicket = issueFreshAttempt();
  const logoutCookies = clearedSessionCookies(true);
  const fenceCookie = logoutCookies.find((cookie) => cookie.startsWith('__Host-apocky-logout-v1='));
  assert(fenceCookie, 'logout rotates the production fence');
  const fencePair = fenceCookie.split(';', 1)[0];

  const superseded = response();
  const handler = createSessionHandler(async () => 'valid', { production: true, now: () => now });
  await handler(fencedSessionRequest({ ticket: attemptTicket, cookie: fencePair }), superseded);
  assert(superseded.statusCodeValue === 409, 'logout applied before session request supersedes the old attempt');
  assert((superseded.body as { code?: string }).code === 'AUTH_ATTEMPT_SUPERSEDED', 'superseded attempt is typed');

  const late = response();
  await handler(fencedSessionRequest({ ticket: attemptTicket }), late);
  assert(late.statusCodeValue === 200, 'the already-admitted request can finish after a concurrent logout');
  const lateCookies = late.headers['set-cookie'];
  assert(Array.isArray(lateCookies), 'late response emits its old-fence binding');
  const lateBinding = lateCookies.find((cookie) => cookie.startsWith('__Host-apocky-session-v2='))?.split(';', 1)[0];
  assert(lateBinding, 'late response contains a session binding');
  const claims = jwtSessionClaims(validToken, now);
  assert(claims, 'fixture JWT has complete session claims');
  const combined = request({ headers: {
    host: 'apocky.com',
    origin: 'https://apocky.com',
    'x-forwarded-proto': 'https',
    cookie: `${fencePair}; ${lateBinding}`,
  } });
  assert(!authSessionBindingValid({ req: combined, claims, userId: claims.subject, nowMs: now, production: true }),
    'a late session response cannot cross the newer logout fence');
}

async function testLogoutClearsAllSessionSurfaces(): Promise<void> {
  const res = response();
  await logoutHandler(request(), res);
  assert(res.statusCodeValue === 200, 'same-origin logout accepted');
  const cookies = res.headers['set-cookie'];
  assert(Array.isArray(cookies) && cookies.length === 8, 'all session surfaces clear and the logout fence rotates');
  const fence = cookies.find((cookie) => cookie.includes('apocky-logout-v1=') && !cookie.includes('Max-Age=0'));
  assert(fence && /Max-Age=15552000/u.test(fence), 'logout creates a durable production fence');
  assert(cookies.filter((cookie) => cookie !== fence).every((cookie) => cookie.includes('Max-Age=0')),
    'every non-fence authentication cookie is expired');
}

async function run(): Promise<void> {
  await testValidSession();
  await testRejections();
  await testLogoutSessionResponseOrdering();
  await testLogoutClearsAllSessionSurfaces();
  // eslint-disable-next-line no-console
  console.log('shawn/auth-session.test : OK · same-origin verified HttpOnly session + complete logout');
}

run().catch((error) => {
  // eslint-disable-next-line no-console
  console.error(error);
  process.exit(1);
});
