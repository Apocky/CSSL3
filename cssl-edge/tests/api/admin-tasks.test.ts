import assert from 'node:assert/strict';

import type { NextApiRequest, NextApiResponse } from 'next';

import handler from '../../pages/api/admin/tasks';

interface Output {
  statusCode: number;
  body: unknown;
  headers: Record<string, string>;
}

function reqRes(method: string, admin: boolean) {
  const out: Output = { statusCode: 0, body: null, headers: {} };
  const req = {
    method,
    query: {},
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

  const anonymous = reqRes('GET', false);
  await handler(anonymous.req, anonymous.res);
  assert.equal(anonymous.out.statusCode, 401, 'task metadata must reject anonymous readers');
  assert.match(anonymous.out.headers['cache-control'] ?? '', /no-store/i);

  const owner = reqRes('GET', true);
  await handler(owner.req, owner.res);
  assert.equal(owner.out.statusCode, 200, 'allowlisted owner can inspect the bounded task projection');
  assert.equal((owner.out.body as { stub?: boolean }).stub, true);
  assert.match(owner.out.headers['cache-control'] ?? '', /no-store/i);

  const method = reqRes('POST', true);
  await handler(method.req, method.res);
  assert.equal(method.out.statusCode, 405, 'task projection remains read-only');

  console.log('admin-tasks.test : OK · owner-only, read-only, and private-cache contract passed');
}

void main().catch((error: unknown) => {
  console.error(error);
  process.exitCode = 1;
});
