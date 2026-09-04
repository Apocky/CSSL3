import assert from 'node:assert/strict';
import type { NextApiRequest, NextApiResponse } from 'next';
import { NextRequest } from 'next/server';
import { mobileConfigFromEnvironment, MOBILE_SUPABASE_URL } from '@/lib/mobile/config';
import handler from '@/pages/api/mobile/config';
import { middleware } from '@/middleware';

const PUBLIC_KEY = `sb_publishable_${'public_test_fixture_'.repeat(2)}`;
const SECRET_SENTINEL = 'sb_secret_do_not_return_this_fixture';
const env = { NEXT_PUBLIC_SUPABASE_URL: MOBILE_SUPABASE_URL, NEXT_PUBLIC_SUPABASE_PUBLISHABLE_KEY: PUBLIC_KEY };
function jwt(role: string, ref = 'pzirbmyfmrbtkllrtcmx'): string {
  return `${Buffer.from(JSON.stringify({ alg: 'HS256', typ: 'JWT' })).toString('base64url')}.${Buffer.from(JSON.stringify({ role, ref })).toString('base64url')}.test_signature`;
}

function request(method: string) {
  const out = { status: 200, body: null as unknown, headers: {} as Record<string, string> };
  const req = { method, query: {}, headers: {} } as NextApiRequest;
  const res = {
    setHeader(name: string, value: string) { out.headers[name.toLowerCase()] = value; return this; },
    status(code: number) { out.status = code; return this; },
    json(body: unknown) { out.body = body; return this; },
  } as unknown as NextApiResponse;
  handler(req, res);
  return out;
}

assert.deepEqual(mobileConfigFromEnvironment(env), {
  schema_version: 'apocky.mobile-config.v1',
  site_url: 'https://www.apocky.com',
  supabase_url: MOBILE_SUPABASE_URL,
  supabase_publishable_key: PUBLIC_KEY,
  api_base: '/api/mobile',
  access: 'account',
});
assert.equal(mobileConfigFromEnvironment({ NEXT_PUBLIC_SUPABASE_URL: MOBILE_SUPABASE_URL, NEXT_PUBLIC_SUPABASE_ANON_KEY: jwt('anon') })?.supabase_publishable_key, jwt('anon'));
for (const key of [SECRET_SENTINEL, jwt('service_role'), jwt('authenticated'), jwt('anon', 'other-project'), '', 'anon', 'sb_publishable_short', ` ${PUBLIC_KEY}`, `${PUBLIC_KEY}\n`, 'a.invalid-base64.c', 'a.WyJhbm9uIl0.c']) {
  assert.equal(mobileConfigFromEnvironment({ ...env, NEXT_PUBLIC_SUPABASE_PUBLISHABLE_KEY: key }), null, 'invalid or privileged keys must fail closed');
}
for (const url of ['http://pzirbmyfmrbtkllrtcmx.supabase.co', 'https://other.supabase.co', `${MOBILE_SUPABASE_URL}.evil.test`, `${MOBILE_SUPABASE_URL}/auth`, `${MOBILE_SUPABASE_URL}/a/..`, `${MOBILE_SUPABASE_URL}?key=${SECRET_SENTINEL}`, `${MOBILE_SUPABASE_URL}#fragment`, 'https://user:pass@pzirbmyfmrbtkllrtcmx.supabase.co', `${MOBILE_SUPABASE_URL}:443`, ` ${MOBILE_SUPABASE_URL}`]) {
  assert.equal(mobileConfigFromEnvironment({ ...env, NEXT_PUBLIC_SUPABASE_URL: url }), null, 'only the pinned HTTPS project origin may be published');
}
assert.equal(mobileConfigFromEnvironment({ ...env, NEXT_PUBLIC_SUPABASE_URL: `${MOBILE_SUPABASE_URL}/` })?.supabase_url, MOBILE_SUPABASE_URL);
assert.equal(mobileConfigFromEnvironment({ ...env, NEXT_PUBLIC_SUPABASE_PUBLISHABLE_KEY: SECRET_SENTINEL, NEXT_PUBLIC_SUPABASE_ANON_KEY: jwt('anon') }), null, 'invalid primary config cannot silently fall back');

const keys = ['NEXT_PUBLIC_SUPABASE_URL', 'NEXT_PUBLIC_SUPABASE_ANON_KEY', 'NEXT_PUBLIC_SUPABASE_PUBLISHABLE_KEY', 'SUPABASE_SERVICE_ROLE_KEY', 'APOCKY_HUB_SUPABASE_ANON_KEY'] as const;
const previous = Object.fromEntries(keys.map(key => [key, process.env[key]]));
try {
  for (const key of keys) delete process.env[key];
  process.env.NEXT_PUBLIC_SUPABASE_URL = MOBILE_SUPABASE_URL;
  process.env.SUPABASE_SERVICE_ROLE_KEY = SECRET_SENTINEL;
  process.env.APOCKY_HUB_SUPABASE_ANON_KEY = SECRET_SENTINEL;
  const absent = request('GET');
  assert.equal(absent.status, 503);
  assert.deepEqual(absent.body, { error: 'Mobile sign-in is not configured.' });
  assert(!JSON.stringify(absent).includes(SECRET_SENTINEL), 'misconfiguration must never leak a value');
  process.env.NEXT_PUBLIC_SUPABASE_ANON_KEY = jwt('service_role');
  assert.equal(request('GET').status, 503, 'misnamed privileged key must never be published');
  process.env.NEXT_PUBLIC_SUPABASE_PUBLISHABLE_KEY = PUBLIC_KEY;
  const good = request('GET');
  assert.equal(good.status, 200);
  assert.equal((good.body as Record<string, unknown>).supabase_publishable_key, PUBLIC_KEY);
  assert(!JSON.stringify(good).includes(SECRET_SENTINEL));
  assert.equal(good.headers.allow, 'GET');
  assert.match(good.headers['cache-control'] ?? '', /no-store/);
  assert.equal(good.headers['x-content-type-options'], 'nosniff');
  for (const method of ['POST', 'PUT', 'DELETE', 'OPTIONS', 'HEAD']) {
    const denied = request(method);
    assert.equal(denied.status, 405);
    assert(!JSON.stringify(denied.body).includes(PUBLIC_KEY), 'unsupported methods must not return configuration');
  }
} finally {
  for (const key of keys) {
    if (previous[key] === undefined) delete process.env[key];
    else process.env[key] = previous[key];
  }
}

assert.equal(middleware(new NextRequest('https://apocky.com/api/mobile/config')).status, 200);
assert.equal(middleware(new NextRequest('https://apocky.com/download/apocrypha')).status, 200);
const accountCsp = middleware(new NextRequest('https://www.apocky.com/apocrypha')).headers.get('content-security-policy') ?? '';
assert.match(accountCsp, /connect-src 'self' https:\/\/pzirbmyfmrbtkllrtcmx\.supabase\.co;/, 'account auth refresh may reach only the pinned Supabase HTTPS project');
assert.match(accountCsp, /script-src 'self' 'nonce-[^']+' 'strict-dynamic'/, 'public account shell retains nonce-bound script execution');
const brainCsp = middleware(new NextRequest('https://www.apocky.com/brain')).headers.get('content-security-policy') ?? '';
assert.match(brainCsp, /connect-src 'self';/, 'private Brain CSP remains unchanged');
assert.equal(middleware(new NextRequest('https://apocky.com/api/apocrypha/chat')).status, 404, 'native config cannot reopen retired public chat');
console.log('mobile config: public-key allowlist, secret non-disclosure, method/security headers, unchanged retirement passed');
