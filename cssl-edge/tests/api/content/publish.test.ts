// cssl-edge · tests/api/content/publish.test.ts
// § T11-W12-UGC-PUBLISH — integration tests for publish-pipeline.
// Stub-mode (no Supabase env vars) ; exercises shape + cap-gate +
// sig-verify + revoke-cascade-broadcast + happy-path init/chunk/complete.

import initHandler from '@/pages/api/content/publish/init';
import chunkHandler from '@/pages/api/content/publish/chunk';
import completeHandler from '@/pages/api/content/publish/complete';
import revokeHandler from '@/pages/api/content/publish/revoke';
import statusHandler from '@/pages/api/content/publish/status/[id]';
import {
  CONTENT_CAP_PUBLISH,
  CONTENT_CAP_REVOKE_ANY,
} from '@/lib/cap';
import {
  testValidateInitOk,
  testValidateInitRejectsBadKind,
  testValidateInitRejectsBadVersion,
  testValidateInitRejectsCommercialLicense,
  testValidateInitChunkCountCeiling,
  testValidateCompleteOk,
  testValidateCompleteRejectsBadSig,
  testValidateRevokeOk,
  testCanonicalSignMessageDeterministic,
  testCanonicalSignMessageChangesWithInputs,
  testHexRoundtrip,
  testBuildRevokeBroadcastShape,
  testUploadChunkedHappy,
  testUploadChunkedResumesOn409,
  testUploadChunkedFailsOn4xx,
} from '@/lib/content-publish';
import type { NextApiRequest, NextApiResponse } from 'next';

interface Mocked {
  statusCode: number;
  body: unknown;
  headers: Record<string, string>;
}

function mockReqRes(method: string, opts: {
  body?: unknown;
  query?: Record<string, string>;
  headers?: Record<string, string>;
  rawBody?: Uint8Array;
} = {}): { req: NextApiRequest; res: NextApiResponse; out: Mocked } {
  const out: Mocked = { statusCode: 0, body: null, headers: {} };
  const baseReq = {
    method,
    query: opts.query ?? {},
    headers: opts.headers ?? {},
    body: opts.body,
  } as unknown as Record<string, unknown>;
  // Add a tiny EventEmitter shim for chunk endpoint raw-body reader.
  if (opts.rawBody !== undefined) {
    const listeners: Record<string, Array<(...a: unknown[]) => void>> = {};
    baseReq['on'] = function(ev: string, cb: (...a: unknown[]) => void): unknown {
      (listeners[ev] ??= []).push(cb);
      return baseReq;
    };
    baseReq['destroy'] = function(): unknown { return baseReq; };
    // Schedule data + end after handler subscribes.
    queueMicrotask(() => {
      (listeners['data'] ?? []).forEach((cb) => cb(Buffer.from(opts.rawBody as Uint8Array)));
      (listeners['end'] ?? []).forEach((cb) => cb());
    });
  }
  const req = baseReq as unknown as NextApiRequest;
  const res: Partial<NextApiResponse> = {
    status(code: number) { out.statusCode = code; return this as NextApiResponse; },
    json(payload: unknown) { out.body = payload; return this as NextApiResponse; },
    setHeader(k: string, v: string) { out.headers[k] = String(v); return this as NextApiResponse; },
    send(payload: unknown) { out.body = payload; return this as NextApiResponse; },
  };
  return {
    req,
    res: res as NextApiResponse,
    out,
  };
}

function assert(cond: boolean, msg: string): void {
  if (!cond) throw new Error(`assert failed : ${msg}`);
}

// ─── lib/content-publish.ts inline tests · re-exercised here ─────────────────

export async function testLibSuite(): Promise<void> {
  testValidateInitOk();
  testValidateInitRejectsBadKind();
  testValidateInitRejectsBadVersion();
  testValidateInitRejectsCommercialLicense();
  testValidateInitChunkCountCeiling();
  testValidateCompleteOk();
  testValidateCompleteRejectsBadSig();
  testValidateRevokeOk();
  testCanonicalSignMessageDeterministic();
  testCanonicalSignMessageChangesWithInputs();
  testHexRoundtrip();
  testBuildRevokeBroadcastShape();
  await testUploadChunkedHappy();
  await testUploadChunkedResumesOn409();
  await testUploadChunkedFailsOn4xx();
}

// ─── /api/content/publish/init ────────────────────────────────────────────

export async function testInitRejectsNonPost(): Promise<void> {
  const { req, res, out } = mockReqRes('GET');
  await initHandler(req, res);
  assert(out.statusCode === 405, `expected 405, got ${out.statusCode}`);
}

export async function testInitRejectsNoCap(): Promise<void> {
  const { req, res, out } = mockReqRes('POST', {
    body: {
      author_pubkey: 'a'.repeat(64),
      kind: 'scene',
      version: '1.0.0',
      license: 'CC-BY-SA-4.0',
      size_bytes_estimate: 1024,
      chunk_count: 1,
    },
  });
  await initHandler(req, res);
  assert(out.statusCode === 403, `expected 403 cap-denied, got ${out.statusCode}`);
}

export async function testInitAcceptsWithCap(): Promise<void> {
  const { req, res, out } = mockReqRes('POST', {
    body: {
      author_pubkey: 'a'.repeat(64),
      kind: 'scene',
      version: '1.0.0',
      license: 'CC-BY-SA-4.0',
      size_bytes_estimate: 1024,
      chunk_count: 2,
    },
    headers: { 'x-loa-cap': String(CONTENT_CAP_PUBLISH) },
  });
  await initHandler(req, res);
  // Stub-mode (no Supabase env) → 200 stub.
  assert(out.statusCode === 200, `expected 200 (stub), got ${out.statusCode}`);
  const body = out.body as Record<string, unknown>;
  assert(body['ok'] === true, 'ok:true expected');
  assert(typeof body['package_id'] === 'string', 'package_id present');
  assert(typeof body['upload_url_template'] === 'string', 'upload_url_template present');
}

export async function testInitRejectsBadVersion(): Promise<void> {
  const { req, res, out } = mockReqRes('POST', {
    body: {
      author_pubkey: 'a'.repeat(64),
      kind: 'scene',
      version: 'bogus',
      license: 'CC-BY-SA-4.0',
      size_bytes_estimate: 1024,
      chunk_count: 1,
    },
    headers: { 'x-loa-cap': String(CONTENT_CAP_PUBLISH) },
  });
  await initHandler(req, res);
  assert(out.statusCode === 400, `expected 400, got ${out.statusCode}`);
}

export async function testInitRejectsBadLicense(): Promise<void> {
  const { req, res, out } = mockReqRes('POST', {
    body: {
      author_pubkey: 'a'.repeat(64),
      kind: 'scene',
      version: '1.0.0',
      license: 'PROPRIETARY-PAID',
      size_bytes_estimate: 1024,
      chunk_count: 1,
    },
    headers: { 'x-loa-cap': String(CONTENT_CAP_PUBLISH) },
  });
  await initHandler(req, res);
  assert(out.statusCode === 400, `expected 400 license-denied, got ${out.statusCode}`);
  const body = out.body as { error?: string };
  assert(typeof body.error === 'string' && body.error.includes('license'), 'error mentions license');
}

// ─── /api/content/publish/chunk ────────────────────────────────────────────

export async function testChunkRejectsBadId(): Promise<void> {
  const { req, res, out } = mockReqRes('POST', {
    query: { id: 'not-a-uuid', seq: '0' },
    headers: { 'x-loa-cap': String(CONTENT_CAP_PUBLISH), 'x-author-pubkey': 'a'.repeat(64) },
    rawBody: new Uint8Array(8),
  });
  await chunkHandler(req, res);
  assert(out.statusCode === 400, `expected 400, got ${out.statusCode}`);
}

export async function testChunkRejectsNoCap(): Promise<void> {
  const { req, res, out } = mockReqRes('POST', {
    query: { id: '00000000-0000-0000-0000-000000000000', seq: '0' },
    headers: { 'x-author-pubkey': 'a'.repeat(64) },
    rawBody: new Uint8Array(8),
  });
  await chunkHandler(req, res);
  assert(out.statusCode === 403, `expected 403, got ${out.statusCode}`);
}

export async function testChunkAcceptsHappyStub(): Promise<void> {
  const { req, res, out } = mockReqRes('POST', {
    query: { id: '00000000-0000-0000-0000-000000000000', seq: '0' },
    headers: {
      'x-loa-cap': String(CONTENT_CAP_PUBLISH),
      'x-author-pubkey': 'a'.repeat(64),
    },
    rawBody: new Uint8Array([1, 2, 3, 4, 5, 6, 7, 8]),
  });
  await chunkHandler(req, res);
  assert(out.statusCode === 200, `expected 200, got ${out.statusCode}`);
  const body = out.body as Record<string, unknown>;
  assert(body['ok'] === true, 'ok:true');
  assert(typeof body['sha256'] === 'string', 'sha256 present');
}

// ─── /api/content/publish/complete ────────────────────────────────────────

export async function testCompleteRejectsBadSig(): Promise<void> {
  const { req, res, out } = mockReqRes('POST', {
    body: {
      package_id: '00000000-0000-0000-0000-000000000000',
      sha256: 'a'.repeat(64),
      signature_ed25519: 'short',
      size_bytes: 1024,
      chunk_count: 1,
    },
    headers: { 'x-loa-cap': String(CONTENT_CAP_PUBLISH), 'x-author-pubkey': 'a'.repeat(64) },
  });
  await completeHandler(req, res);
  assert(out.statusCode === 400, `expected 400, got ${out.statusCode}`);
}

export async function testCompleteAcceptsHappyStub(): Promise<void> {
  const { req, res, out } = mockReqRes('POST', {
    body: {
      package_id: '00000000-0000-0000-0000-000000000000',
      sha256: 'a'.repeat(64),
      signature_ed25519: 'b'.repeat(128),
      size_bytes: 1024,
      chunk_count: 1,
    },
    headers: { 'x-loa-cap': String(CONTENT_CAP_PUBLISH), 'x-author-pubkey': 'a'.repeat(64) },
  });
  await completeHandler(req, res);
  assert(out.statusCode === 200, `expected 200 (stub), got ${out.statusCode}`);
  const body = out.body as Record<string, unknown>;
  assert(body['ok'] === true, 'ok:true');
  assert(typeof body['sigma_chain_anchor'] === 'string', 'anchor present');
  assert((body['sigma_chain_anchor'] as string).length === 64, 'anchor 64-hex');
  assert(body['state'] === 'published', 'state=published');
}

export async function testCompleteRejectsCapMissing(): Promise<void> {
  const { req, res, out } = mockReqRes('POST', {
    body: {
      package_id: '00000000-0000-0000-0000-000000000000',
      sha256: 'a'.repeat(64),
      signature_ed25519: 'b'.repeat(128),
      size_bytes: 1024,
      chunk_count: 1,
    },
    headers: { 'x-author-pubkey': 'a'.repeat(64) },
  });
  await completeHandler(req, res);
  assert(out.statusCode === 403, `expected 403 cap-denied, got ${out.statusCode}`);
}

// ─── /api/content/publish/revoke ───────────────────────────────────────────

export async function testRevokeSelfHappyStub(): Promise<void> {
  const { req, res, out } = mockReqRes('POST', {
    body: {
      package_id: '00000000-0000-0000-0000-000000000000',
      reason: 'creator-revoked',
      who_pubkey: 'a'.repeat(64),
    },
    headers: { 'x-loa-cap': String(CONTENT_CAP_PUBLISH), 'x-author-pubkey': 'a'.repeat(64) },
  });
  await revokeHandler(req, res);
  assert(out.statusCode === 200, `expected 200, got ${out.statusCode}`);
  const body = out.body as Record<string, unknown>;
  assert(body['ok'] === true, 'ok:true');
  const broadcast = body['mycelium_broadcast'] as Record<string, unknown>;
  assert(broadcast['kind'] === 'content.revoke', 'broadcast.kind');
  assert(typeof broadcast['ts_ns'] === 'number', 'broadcast.ts_ns');
  assert(broadcast['by_pubkey'] === 'a'.repeat(64), 'broadcast.by_pubkey');
}

export async function testRevokeRejectsNoCap(): Promise<void> {
  const { req, res, out } = mockReqRes('POST', {
    body: {
      package_id: '00000000-0000-0000-0000-000000000000',
      reason: 'creator-revoked',
      who_pubkey: 'a'.repeat(64),
    },
    headers: { 'x-author-pubkey': 'a'.repeat(64) },
  });
  await revokeHandler(req, res);
  assert(out.statusCode === 403, `expected 403 cap-denied, got ${out.statusCode}`);
}

export async function testRevokeRejectsHeaderMismatch(): Promise<void> {
  const { req, res, out } = mockReqRes('POST', {
    body: {
      package_id: '00000000-0000-0000-0000-000000000000',
      reason: 'fake-revoke-attempt',
      who_pubkey: 'a'.repeat(64),
    },
    headers: { 'x-loa-cap': String(CONTENT_CAP_PUBLISH), 'x-author-pubkey': 'b'.repeat(64) },
  });
  await revokeHandler(req, res);
  assert(out.statusCode === 403, `expected 403 mismatch, got ${out.statusCode}`);
}

export async function testRevokeModeratorWithRevokeAnyCap(): Promise<void> {
  const { req, res, out } = mockReqRes('POST', {
    body: {
      package_id: '00000000-0000-0000-0000-000000000000',
      reason: 'moderator-revoked',
      who_pubkey: 'fe'.repeat(32),
    },
    headers: { 'x-loa-cap': String(CONTENT_CAP_REVOKE_ANY), 'x-author-pubkey': 'fe'.repeat(32) },
  });
  await revokeHandler(req, res);
  assert(out.statusCode === 200, `expected 200 (stub), got ${out.statusCode}`);
}

// ─── /api/content/publish/status/:id ───────────────────────────────────────

export async function testStatusRejectsBadId(): Promise<void> {
  const { req, res, out } = mockReqRes('GET', { query: { id: 'bogus' } });
  await statusHandler(req, res);
  assert(out.statusCode === 400, `expected 400, got ${out.statusCode}`);
}

export async function testStatusAcceptsStubMode(): Promise<void> {
  const { req, res, out } = mockReqRes('GET', { query: { id: '00000000-0000-0000-0000-000000000000' } });
  await statusHandler(req, res);
  assert(out.statusCode === 200, `expected 200 (stub), got ${out.statusCode}`);
  const body = out.body as Record<string, unknown>;
  assert(body['ok'] === true, 'ok:true');
  assert(body['package_id'] === '00000000-0000-0000-0000-000000000000', 'id passthrough');
}

// ─── runner ───────────────────────────────────────────────────────────────

declare const require: { main?: unknown } | undefined;
declare const module: { id?: string } | undefined;
const isMain =
  typeof require !== 'undefined' &&
  typeof module !== 'undefined' &&
  require.main === module;
if (isMain) {
  (async (): Promise<void> => {
    await testLibSuite();
    await testInitRejectsNonPost();
    await testInitRejectsNoCap();
    await testInitAcceptsWithCap();
    await testInitRejectsBadVersion();
    await testInitRejectsBadLicense();
    await testChunkRejectsBadId();
    await testChunkRejectsNoCap();
    await testChunkAcceptsHappyStub();
    await testCompleteRejectsBadSig();
    await testCompleteAcceptsHappyStub();
    await testCompleteRejectsCapMissing();
    await testRevokeSelfHappyStub();
    await testRevokeRejectsNoCap();
    await testRevokeRejectsHeaderMismatch();
    await testRevokeModeratorWithRevokeAnyCap();
    await testStatusRejectsBadId();
    await testStatusAcceptsStubMode();
    // eslint-disable-next-line no-console
    console.log('publish.test : OK · 17 endpoint + 15 lib tests passed');
  })().catch((e: unknown) => {
    // eslint-disable-next-line no-console
    console.error('publish.test : FAIL ·', e);
    process.exit(1);
  });
}
