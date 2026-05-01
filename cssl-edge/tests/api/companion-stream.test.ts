// cssl-edge · tests/api/companion-stream.test.ts
// Lightweight self-tests for /api/companion/stream. Framework-agnostic — runs
// via `npx tsx tests/api/companion-stream.test.ts`. The route normally sleeps
// 100ms between chunks ; we override via _setStreamDelayForTests(0) so the
// suite finishes in <50ms total.

import handler, { _setStreamDelayForTests } from '@/pages/api/companion/stream';
import { COMPANION_REMOTE_RELAY } from '@/lib/cap';
import { SOVEREIGN_CAP_HEX, SOVEREIGN_HEADER_NAME } from '@/lib/sovereign';
import type { NextApiRequest, NextApiResponse } from 'next';

interface MockedResponse {
  statusCode: number;
  body: unknown;
  headers: Record<string, string>;
  writes: string[];
  ended: boolean;
}

function mockReqRes(
  method: string,
  query: Record<string, string | string[]> = {},
  headers: Record<string, string> = {}
): { req: NextApiRequest; res: NextApiResponse; out: MockedResponse } {
  const out: MockedResponse = {
    statusCode: 0,
    body: null,
    headers: {},
    writes: [],
    ended: false,
  };

  const req = {
    method,
    query,
    headers,
    body: undefined,
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
    write(s: string) {
      out.writes.push(s);
      return true;
    },
    end() {
      out.ended = true;
    },
  } as unknown as NextApiResponse;

  return { req, res, out };
}

function assert(cond: boolean, msg: string): void {
  if (!cond) throw new Error(`assert failed : ${msg}`);
}

function makeMessagesB64(): string {
  const messages = [{ role: 'user' as const, content: 'hello stream' }];
  return Buffer.from(JSON.stringify(messages), 'utf-8').toString('base64');
}

// 1. caps=0 + no sovereign header → 403.
export async function testCapsZeroDenied(): Promise<void> {
  _setStreamDelayForTests(0);
  const { req, res, out } = mockReqRes('GET', {
    messages: makeMessagesB64(),
    cap: '0',
  });
  await handler(req, res);
  assert(
    out.statusCode === 403,
    `caps=0 must yield 403, got ${out.statusCode}`
  );
  // Should NOT have written any SSE bytes when denied.
  assert(out.writes.length === 0, 'no SSE writes expected on deny');
}

// 2. caps with COMPANION_REMOTE_RELAY bit set → SSE stream.
export async function testCapsSetStreams(): Promise<void> {
  _setStreamDelayForTests(0);
  const { req, res, out } = mockReqRes('GET', {
    messages: makeMessagesB64(),
    cap: String(COMPANION_REMOTE_RELAY),
  });
  await handler(req, res);
  assert(
    out.statusCode === 200,
    `caps=COMPANION_REMOTE_RELAY must yield 200, got ${out.statusCode}`
  );
  assert(
    out.headers['Content-Type'] === 'text/event-stream',
    `expected SSE Content-Type, got ${out.headers['Content-Type'] ?? '(none)'}`
  );
  // Expect : 1 message_start + 5 content_block_delta + 1 [DONE] = 7 writes.
  assert(
    out.writes.length === 7,
    `expected 7 writes (1 start + 5 delta + 1 DONE), got ${out.writes.length}`
  );
  // First write must be message_start envelope.
  const first = out.writes[0] ?? '';
  assert(
    first.startsWith('data: ') && first.includes('"message_start"'),
    `expected first write to be message_start, got ${first.slice(0, 80)}`
  );
  // Last write must be the [DONE] sentinel literal.
  const last = out.writes[out.writes.length - 1] ?? '';
  assert(
    last === 'data: [DONE]\n\n',
    `expected final [DONE] sentinel, got ${JSON.stringify(last)}`
  );
  assert(out.ended === true, 'expected res.end() call after stream-close');
}

// 3. sovereign:true + correct header → 200 streams even with caps=0.
export async function testSovereignBypass(): Promise<void> {
  _setStreamDelayForTests(0);
  const { req, res, out } = mockReqRes(
    'GET',
    {
      messages: makeMessagesB64(),
      cap: '0',
      sovereign: 'true',
    },
    { [SOVEREIGN_HEADER_NAME]: SOVEREIGN_CAP_HEX }
  );
  await handler(req, res);
  assert(
    out.statusCode === 200,
    `sovereign-bypass must yield 200, got ${out.statusCode}`
  );
  assert(out.writes.length >= 2, 'sovereign bypass should still emit chunks');
}

declare const require: { main?: unknown } | undefined;
declare const module: { id?: string } | undefined;
const isMain =
  typeof require !== 'undefined' &&
  typeof module !== 'undefined' &&
  require.main === module;

async function runAll(): Promise<void> {
  await testCapsZeroDenied();
  await testCapsSetStreams();
  await testSovereignBypass();
  // eslint-disable-next-line no-console
  console.log('companion-stream.test : OK · 3 tests passed');
}

if (isMain) {
  runAll().catch((err) => {
    // eslint-disable-next-line no-console
    console.error(err);
    process.exit(1);
  });
}
