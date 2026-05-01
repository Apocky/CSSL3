// cssl-edge · tests/api/companion.test.ts
// Lightweight self-test for /api/companion. Framework-agnostic — runs via
// `npx tsx tests/api/companion.test.ts`. Mirrors tests/api/health.test.ts style.

import handler, { CAP_COMPANION_REMOTE_RELAY } from '@/pages/api/companion';
import { SOVEREIGN_CAP_HEX, SOVEREIGN_HEADER_NAME } from '@/lib/sovereign';
import type { NextApiRequest, NextApiResponse } from 'next';

interface MockedResponse {
  statusCode: number;
  body: unknown;
  headers: Record<string, string>;
}

function mockReqRes(
  method: string,
  body?: unknown,
  headers: Record<string, string> = {}
): { req: NextApiRequest; res: NextApiResponse; out: MockedResponse } {
  const out: MockedResponse = { statusCode: 0, body: null, headers: {} };

  const req = {
    method,
    query: {},
    headers,
    body,
  } as unknown as NextApiRequest;

  const res = {
    status(code: number) {
      out.statusCode = code;
      return this;
    },
    json(payload: unknown) {
      out.body = payload;
      return this;
    },
    setHeader(key: string, val: string) {
      out.headers[key] = val;
      return this;
    },
  } as unknown as NextApiResponse;

  return { req, res, out };
}

const SAMPLE_MESSAGES = [
  { role: 'user' as const, content: 'hello companion' },
];

function assert(cond: boolean, msg: string): void {
  if (!cond) throw new Error(`assert failed : ${msg}`);
}

// 1. caps=0 + no sovereign header → 403 deny.
export function testCapsZeroDenies(): void {
  const { req, res, out } = mockReqRes('POST', {
    messages: SAMPLE_MESSAGES,
    cap: 0,
  });
  handler(req, res);
  assert(
    out.statusCode === 403,
    `caps=0 must yield 403, got ${out.statusCode}`
  );
}

// 2. caps with COMPANION_REMOTE_RELAY bit set → 200 stub response.
export function testCapsSetAllows(): void {
  const { req, res, out } = mockReqRes('POST', {
    messages: SAMPLE_MESSAGES,
    cap: CAP_COMPANION_REMOTE_RELAY,
  });
  handler(req, res);
  assert(
    out.statusCode === 200,
    `caps=COMPANION_REMOTE_RELAY must yield 200, got ${out.statusCode}`
  );
  const body = out.body as {
    type?: unknown;
    role?: unknown;
    content?: unknown;
    stub?: unknown;
  };
  assert(body.type === 'message', `expected type:message, got ${String(body.type)}`);
  assert(body.role === 'assistant', `expected role:assistant, got ${String(body.role)}`);
  assert(Array.isArray(body.content), 'expected content:Array');
  assert(body.stub === true, 'expected stub:true');
}

// 3. sovereign:true + correct header → 200 even with caps=0.
export function testSovereignBypassWithCorrectHeader(): void {
  const { req, res, out } = mockReqRes(
    'POST',
    {
      messages: SAMPLE_MESSAGES,
      cap: 0,
      sovereign: true,
    },
    { [SOVEREIGN_HEADER_NAME]: SOVEREIGN_CAP_HEX }
  );
  handler(req, res);
  assert(
    out.statusCode === 200,
    `sovereign + header must yield 200, got ${out.statusCode}`
  );
}

// 4. sovereign:true WITHOUT header → flag ignored → 403.
export function testSovereignRejectedWithoutHeader(): void {
  const { req, res, out } = mockReqRes('POST', {
    messages: SAMPLE_MESSAGES,
    cap: 0,
    sovereign: true,
  });
  handler(req, res);
  assert(
    out.statusCode === 403,
    `sovereign:true without header must yield 403, got ${out.statusCode}`
  );
}

declare const require: { main?: unknown } | undefined;
declare const module: { id?: string } | undefined;
const isMain =
  typeof require !== 'undefined' &&
  typeof module !== 'undefined' &&
  require.main === module;
if (isMain) {
  testCapsZeroDenies();
  testCapsSetAllows();
  testSovereignBypassWithCorrectHeader();
  testSovereignRejectedWithoutHeader();
  // eslint-disable-next-line no-console
  console.log('companion.test : OK · 4 tests passed');
}
