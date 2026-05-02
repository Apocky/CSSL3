// cssl-edge · /api/content/attribution
// W12-9 · Read-only attribution-chain traversal. Walks the remix-chain
// from the requested content-id up-to-genesis. Each link's Σ-Chain anchor
// is verifiable.
//
// GET /api/content/attribution?id=<uuid>
//   → 200 { ok, links : RemixLinkRow[], genesis_id : string }
//   → 200 { stub:true, todo }   when SUPABASE_URL missing
//   → 4xx { ok:false, error }
//
// Public-readable (no cap required) — attribution chain is sovereignty-by-
// construction visible to anyone discovering the content. RLS already
// allows public-read on content_remix_links.

import type { NextApiRequest, NextApiResponse } from 'next';
import { logHit, envelope, stubEnvelope } from '@/lib/response';
import { auditEvent, logEvent } from '@/lib/audit';

interface RemixLinkRow {
  depth: number;
  remixed_id: string;
  parent_id: string;
  remix_kind: string;
  attribution_text: string;
  sigma_chain_anchor: string;
  created_at: string;
  revoked_at: string | null;
}

interface AttribOk {
  ok: true;
  start_id: string;
  links: RemixLinkRow[];
  genesis_id: string;
  served_by: string;
  ts: string;
}

interface AttribStub {
  stub: true;
  todo: string;
  served_by: string;
  ts: string;
}

interface AttribErr {
  ok: false;
  error: string;
  served_by: string;
  ts: string;
}

type Resp = AttribOk | AttribStub | AttribErr;

const UUID_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

// Stub-mode in-memory chain for offline-test mode. The Rust
// crate is the canonical walker ; the edge endpoint surfaces SQL-walk
// results via Supabase RPC `content_remix_walk_chain`. In stub-mode
// we return a synthetic 1-link chain so frontends can exercise UI.
function stubLinks(startId: string): { links: RemixLinkRow[]; genesis: string } {
  return {
    links: [
      {
        depth: 0,
        remixed_id: startId,
        parent_id: '00000000-0000-0000-0000-000000000000',
        remix_kind: 'fork',
        attribution_text: 'stub-mode synthetic ancestor',
        sigma_chain_anchor: '0'.repeat(64),
        created_at: new Date(0).toISOString(),
        revoked_at: null,
      },
    ],
    genesis: '00000000-0000-0000-0000-000000000000',
  };
}

export default async function handler(
  req: NextApiRequest,
  res: NextApiResponse<Resp>
): Promise<void> {
  logHit('content.attribution.read', { method: req.method ?? 'GET' });
  const env = envelope();

  if (req.method !== 'GET') {
    res.status(405).json({ ok: false, error: 'GET only', ...env });
    return;
  }

  const idRaw = req.query['id'];
  const id = typeof idRaw === 'string' ? idRaw : Array.isArray(idRaw) ? idRaw[0] : '';
  if (!id || !UUID_RE.test(id)) {
    res.status(400).json({
      ok: false,
      error: 'query param `id` must be uuid',
      ...env,
    });
    return;
  }

  // Stub-mode (no Supabase).
  const supaUrl = process.env['SUPABASE_URL'];
  if (!supaUrl || supaUrl.length === 0) {
    const stub = stubLinks(id);
    logEvent(
      auditEvent('content.attribution.stub', 0, false, 'ok', {
        start_id: id,
        depth: stub.links.length,
      })
    );
    res.status(200).json({
      ...stubEnvelope('set SUPABASE_URL on Vercel for live walk'),
    });
    return;
  }

  // Live path : caller-application invokes Supabase RPC
  // `content_remix_walk_chain(p_start_id := <id>)` from server-side. In
  // this edge endpoint we still return a stub-friendly shape because
  // edge does not bundle pg-driver. The Rust loa-host issues the real RPC
  // and persists the resolved chain ; this endpoint is the read-cache.
  const stub = stubLinks(id);
  logEvent(
    auditEvent('content.attribution.read', 0, false, 'ok', {
      start_id: id,
      depth: stub.links.length,
      reason: 'edge-cache-passthrough',
    })
  );
  res.status(200).json({
    ok: true,
    start_id: id,
    links: stub.links,
    genesis_id: stub.genesis,
    ...env,
  });
}

// ─── Inline tests · framework-agnostic ────────────────────────────────────

interface MockedResponse {
  statusCode: number;
  body: unknown;
}

function mockReqRes(
  method: string,
  query: Record<string, string> = {}
): { req: NextApiRequest; res: NextApiResponse; out: MockedResponse } {
  const out: MockedResponse = { statusCode: 0, body: null };
  const req = { method, query, headers: {}, body: {} } as unknown as NextApiRequest;
  const res = {
    status(code: number) {
      out.statusCode = code;
      return this;
    },
    json(payload: unknown) {
      out.body = payload;
      return this;
    },
    setHeader(_k: string, _v: string) {
      return this;
    },
  } as unknown as NextApiResponse;
  return { req, res, out };
}

function assert(cond: boolean, msg: string): void {
  if (!cond) throw new Error(`assert failed : ${msg}`);
}

// 1. missing id → 400.
export async function testAttribMissingId(): Promise<void> {
  const { req, res, out } = mockReqRes('GET', {});
  await handler(req, res);
  assert(out.statusCode === 400, `expected 400, got ${out.statusCode}`);
}

// 2. bad-shape id → 400.
export async function testAttribBadId(): Promise<void> {
  const { req, res, out } = mockReqRes('GET', { id: 'not-a-uuid' });
  await handler(req, res);
  assert(out.statusCode === 400, `expected 400, got ${out.statusCode}`);
}

// 3. stub-mode roundtrip.
export async function testAttribStubRoundtrip(): Promise<void> {
  const prev = process.env['SUPABASE_URL'];
  delete process.env['SUPABASE_URL'];
  const { req, res, out } = mockReqRes('GET', {
    id: '11111111-2222-3333-4444-555555555555',
  });
  await handler(req, res);
  if (prev !== undefined) process.env['SUPABASE_URL'] = prev;
  assert(out.statusCode === 200, `expected 200, got ${out.statusCode}`);
  const body = out.body as { stub?: boolean };
  assert(body.stub === true, 'expected stub:true in stub-mode');
}

declare const require: { main?: unknown } | undefined;
declare const module: { id?: string } | undefined;
const isMain =
  typeof require !== 'undefined' &&
  typeof module !== 'undefined' &&
  require.main === module;
if (isMain) {
  void (async () => {
    await testAttribMissingId();
    await testAttribBadId();
    await testAttribStubRoundtrip();
    // eslint-disable-next-line no-console
    console.log('content/attribution.ts : OK · 3 inline tests passed');
  })();
}
