import assert from 'node:assert/strict';

import type { NextApiRequest, NextApiResponse } from 'next';

import handler from '../../pages/api/admin/logs';

interface Output {
  statusCode: number;
  body: unknown;
  headers: Record<string, string>;
}

function reqRes(method: string, admin: boolean, query: Record<string, string> = {}) {
  const out: Output = { statusCode: 0, body: null, headers: {} };
  const req = {
    method,
    query,
    headers: admin ? { 'x-apocky-test-admin-email': 'apocky13@gmail.com' } : {},
  } as unknown as NextApiRequest;
  const res = {
    status(code: number) { out.statusCode = code; return this; },
    json(value: unknown) { out.body = value; return this; },
    setHeader(name: string, value: string | number | readonly string[]) {
      out.headers[name.toLowerCase()] = Array.isArray(value) ? value.join(', ') : String(value);
      return this;
    },
  } as unknown as NextApiResponse;
  return { req, res, out };
}

async function main(): Promise<void> {
  process.env.LAZARUS_TEST_AUTH_BYPASS = '1';
  Object.assign(process.env, { NODE_ENV: 'test' });
  delete process.env.APOCKY_HUB_SUPABASE_URL;
  delete process.env.NEXT_PUBLIC_SUPABASE_URL;
  delete process.env.APOCKY_HUB_SUPABASE_SERVICE_ROLE_KEY;
  delete process.env.SUPABASE_SERVICE_ROLE_KEY;

  const denied = reqRes('GET', false, { limit: '25' });
  await handler(denied.req, denied.res);
  assert.equal(denied.out.statusCode, 401, 'anonymous readers are denied');

  const invalid = reqRes('GET', true, { limit: '501' });
  await handler(invalid.req, invalid.res);
  assert.equal(invalid.out.statusCode, 400, 'out-of-range limits are denied');

  const unconfigured = reqRes('GET', true, { limit: '25' });
  await handler(unconfigured.req, unconfigured.res);
  assert.equal(unconfigured.out.statusCode, 503, 'missing first-party store is an explicit degraded state');
  assert.equal(
    (unconfigured.out.body as Record<string, unknown>).schema_version,
    'apocky.admin-telemetry-log.v1',
  );
  assert.match(unconfigured.out.headers['cache-control'] ?? '', /no-store/i);

  const method = reqRes('POST', true);
  await handler(method.req, method.res);
  assert.equal(method.out.statusCode, 405, 'non-read methods are denied');

  console.log('admin-logs-api.test : OK · owner authorization, bounds, degradation, and cache policy passed');
}

void main();
