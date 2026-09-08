import assert from 'node:assert/strict';
import type { NextApiRequest, NextApiResponse } from 'next';
import { NextRequest } from 'next/server';

import { CORPUS_REVIEW_HELD_CODE } from '@/lib/conversation-corpus';
import { middleware } from '@/middleware';
import handler from '@/pages/api/conversation-corpus/[id]';

interface Output {
  statusCode: number;
  body: unknown;
  headers: Record<string, string>;
}

function request(method: string, id: string): { req: NextApiRequest; res: NextApiResponse; out: Output } {
  const out: Output = { statusCode: 200, body: null, headers: {} };
  const req = { method, query: { id }, headers: {} } as unknown as NextApiRequest;
  const res = {
    setHeader(name: string, value: string | number | readonly string[]) {
      out.headers[name.toLowerCase()] = Array.isArray(value) ? value.join(', ') : String(value);
      return this;
    },
    status(code: number) { out.statusCode = code; return this; },
    json(value: unknown) { out.body = value; return this; },
  } as unknown as NextApiResponse;
  return { req, res, out };
}

async function main(): Promise<void> {
  const method = request('POST', 'aaaaaaaaaaaaaaaaaaaa');
  await handler(method.req, method.res);
  assert.equal(method.out.statusCode, 405);

  const invalid = request('GET', '../private-record');
  await handler(invalid.req, invalid.res);
  assert.equal(invalid.out.statusCode, 400, 'invalid and traversal-like IDs fail before filesystem access');

  const held = request('GET', 'aaaaaaaaaaaaaaaaaaaa');
  await handler(held.req, held.res);
  assert.equal(held.out.statusCode, 423, 'valid but unapproved IDs remain review-held');
  assert.equal((held.out.body as { error: { code: string } }).error.code, CORPUS_REVIEW_HELD_CODE);
  assert.match(held.out.headers['cache-control'] ?? '', /no-store/u, 'held response is not cached');

  for (const pathname of ['/conversation-corpus/index.v1.json', '/conversation-corpus/records/aaaaaaaaaaaaaaaaaaaa.json']) {
    const response = middleware(new NextRequest(`https://www.apocky.com${pathname}`));
    assert.equal(response.status, 404, `${pathname} is unreachable even when legacy local bytes exist`);
    assert.match(response.headers.get('x-robots-tag') ?? '', /noindex/u);
  }

  console.log('conversation-corpus-api.test : OK · stable hold + traversal denial + legacy static blackout');
}

void main().catch((error: unknown) => {
  console.error(error);
  process.exitCode = 1;
});
