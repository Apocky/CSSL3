import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { NextApiRequest, NextApiResponse } from 'next';

import type { ContainmentResponse } from '@/lib/containment';
import visionSession from '@/pages/api/admin/apocrypha/vision/session';
import visionState from '@/pages/api/admin/apocrypha/vision/session/[session_ref]';
import visionFrame from '@/pages/api/admin/apocrypha/vision/session/[session_ref]/frame';
import visionControl from '@/pages/api/admin/apocrypha/vision/session/[session_ref]/control';

type Handler = (
  req: NextApiRequest,
  res: NextApiResponse<ContainmentResponse>,
) => void | Promise<void>;

function assert(condition: boolean, message: string): asserts condition {
  if (!condition) throw new Error(`assert failed: ${message}`);
}

async function assertRetired(handler: Handler): Promise<void> {
  const out: {
    status: number;
    body: ContainmentResponse | null;
    headers: Record<string, string>;
  } = { status: 0, body: null, headers: {} };
  const req = {
    method: 'POST',
    query: { session_ref: crypto.randomUUID() },
    body: {
      consent_id: crypto.randomUUID(),
      content_b64: 'aGVsbG8=',
      event: 'close',
    },
    headers: {
      authorization: 'Bearer attacker-controlled',
      cookie: '__Host-apocky-access-token=attacker-controlled',
      origin: 'https://www.apocky.com',
      host: 'www.apocky.com',
    },
  } as unknown as NextApiRequest;
  const res = {
    status(code: number) { out.status = code; return this; },
    json(body: ContainmentResponse) { out.body = body; return this; },
    setHeader(name: string, value: string | number | readonly string[]) {
      out.headers[name.toLowerCase()] = Array.isArray(value) ? value.join(', ') : String(value);
      return this;
    },
  } as unknown as NextApiResponse<ContainmentResponse>;
  await handler(req, res);
  assert(out.status === 410, 'vision route returns 410 even with asserted auth and consent');
  assert(out.body?.surface === '/api/admin/apocrypha/vision/*', 'vision route uses one canonical surface');
  assert(out.headers['cache-control']?.includes('private') === true, 'vision 410 is private');
  assert(out.headers['cache-control']?.includes('no-store') === true, 'vision 410 is no-store');
}

async function main(): Promise<void> {
  let networkCalls = 0;
  const originalFetch = globalThis.fetch;
  globalThis.fetch = async () => {
    networkCalls += 1;
    throw new Error('retired vision route attempted network I/O');
  };
  try {
    for (const handler of [visionSession, visionState, visionFrame, visionControl]) {
      await assertRetired(handler);
    }
  } finally {
    globalThis.fetch = originalFetch;
  }
  assert(networkCalls === 0, 'retired vision routes perform no upstream calls');

  const chat = readFileSync(resolve(process.cwd(), 'components/apocrypha/ChatThread.tsx'), 'utf8');
  assert(!chat.includes('VisionPanel'), 'chat exposes no retired camera control');
  assert(chat.includes('Vision · temporarily retired'), 'chat declares the current boundary');
  console.log('apocrypha-v2-vision.test : OK · 4 routes fail closed and UI is truthful');
}

main().catch((error: unknown) => {
  console.error(error);
  process.exitCode = 1;
});
