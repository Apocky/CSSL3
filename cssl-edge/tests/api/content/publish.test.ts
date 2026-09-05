import type { NextApiRequest, NextApiResponse } from 'next';

import type { ContainmentResponse } from '@/lib/containment';
import initHandler from '@/pages/api/content/publish/init';
import chunkHandler from '@/pages/api/content/publish/chunk';
import completeHandler from '@/pages/api/content/publish/complete';
import revokeHandler from '@/pages/api/content/publish/revoke';
import statusHandler from '@/pages/api/content/publish/status/[id]';

type Handler = (
  req: NextApiRequest,
  res: NextApiResponse<ContainmentResponse>,
) => void | Promise<void>;

function assert(condition: boolean, message: string): asserts condition {
  if (!condition) throw new Error(`assert failed: ${message}`);
}

async function assertRetired(handler: Handler, surface: string): Promise<void> {
  const out: {
    status: number;
    body: ContainmentResponse | null;
    headers: Record<string, string>;
  } = { status: 0, body: null, headers: {} };
  const req = {
    method: 'POST',
    query: { id: '00000000-0000-0000-0000-000000000000', seq: '0' },
    headers: {
      'x-loa-cap': String(Number.MAX_SAFE_INTEGER),
      'x-author-pubkey': 'a'.repeat(64),
      authorization: 'Bearer attacker-controlled',
    },
    body: {
      package_id: '00000000-0000-0000-0000-000000000000',
      author_pubkey: 'a'.repeat(64),
      signature_ed25519: 'b'.repeat(128),
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
  assert(out.status === 410, `${surface} returns 410 despite asserted cap/signature`);
  assert(out.body?.reason_code === 'temporary_security_containment', `${surface} has typed reason`);
  assert(out.body?.surface === surface, `${surface} has canonical surface`);
  assert(out.headers['cache-control']?.includes('no-store') === true, `${surface} is no-store`);
}

async function main(): Promise<void> {
  await assertRetired(initHandler, '/api/content/*');
  await assertRetired(chunkHandler, '/api/content/*');
  await assertRetired(completeHandler, '/api/content/*');
  await assertRetired(revokeHandler, '/api/content/*');
  await assertRetired(statusHandler, '/api/content/*');
  console.log('publish.test : OK · all 5 publish routes are retired');
}

main().catch((error: unknown) => {
  console.error(error);
  process.exitCode = 1;
});
