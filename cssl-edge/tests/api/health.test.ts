// cssl-edge · tests/api/health.test.ts
// Lightweight self-test for /api/health. NO test framework required —
// runs via `npx tsx tests/api/health.test.ts` if a runner is wired later.
// Lives OUTSIDE pages/api so Next.js does not register it as a route.

import handler from '@/pages/api/health';
import type { NextApiRequest, NextApiResponse } from 'next';

interface MockedResponse {
  statusCode: number;
  body: unknown;
  headers: Record<string, string>;
}

function mockReqRes(method: string): { req: NextApiRequest; res: NextApiResponse; out: MockedResponse } {
  const out: MockedResponse = { statusCode: 0, body: null, headers: {} };

  const req = { method, query: {}, headers: {}, body: undefined } as unknown as NextApiRequest;

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

export function testHealthOk(): void {
  const { req, res, out } = mockReqRes('GET');
  handler(req, res);

  if (out.statusCode !== 200) {
    throw new Error(`expected 200, got ${out.statusCode}`);
  }
  const body = out.body as { ok?: unknown; sha?: unknown; served_by?: unknown };
  if (body.ok !== true) {
    throw new Error(`expected ok:true, got ${JSON.stringify(body.ok)}`);
  }
  if (typeof body.sha !== 'string') {
    throw new Error(`expected sha:string, got ${typeof body.sha}`);
  }
  if (typeof body.served_by !== 'string') {
    throw new Error(`expected served_by:string, got ${typeof body.served_by}`);
  }
}

export function testHealthShape(): void {
  const { req, res, out } = mockReqRes('GET');
  handler(req, res);
  const body = out.body as Record<string, unknown>;
  const requiredKeys = ['ok', 'sha', 'served_by', 'ts', 'version'];
  for (const k of requiredKeys) {
    if (!(k in body)) {
      throw new Error(`missing required key: ${k}`);
    }
  }
}

// Run directly when invoked as a script (e.g. tsx tests/api/health.test.ts).
// Guarded so importing this module under a test framework does not auto-run.
declare const require: { main?: unknown } | undefined;
declare const module: { id?: string } | undefined;
const isMain =
  typeof require !== 'undefined' &&
  typeof module !== 'undefined' &&
  require.main === module;
if (isMain) {
  testHealthOk();
  testHealthShape();
  // eslint-disable-next-line no-console
  console.log('health.test : OK');
}
