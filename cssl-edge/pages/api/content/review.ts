// cssl-edge · /api/content/review
// Submit (or overwrite) a free-text review on user-generated CSSL content.
// Cap-gated CONTENT_CAP_REVIEW_BODY (0x20000). Sovereign-bypass via header.
//
// Mirrors compiler-rs `cssl-content-rating::Review::new`. Body ≤ 240 bytes ;
// signature ≤ 64 bytes. Re-submitting overwrites. DELETE to retract entirely.
//
// Body :
//   {
//     rater_pubkey_hash: string  // 16 hex chars
//     content_id: number         // u32
//     stars: number              // 1..=5 (reviews require active rating)
//     body: string               // ≤ 240 utf-8 chars
//     tags_bitset: number        // 0..=65535
//     sigma_mask: number         // 0..=255 ; CAP_REVIEW_BODY=0x04 must be set
//     ts_minutes_since_epoch: number
//     sig: string                // 128 hex chars (64 bytes Ed25519)
//     share_with_author: boolean // optional ; default false
//     cap: number                // CONTENT_CAP_REVIEW_BODY 0x20000
//     sovereign?: boolean
//   }

import type { NextApiRequest, NextApiResponse } from 'next';
import { auditEvent, deny, logEvent } from '@/lib/audit';
import { CONTENT_CAP_REVIEW_BODY } from '@/lib/cap';
import { isSovereignFromIncoming } from '@/lib/sovereign';
import { envelope, logHit } from '@/lib/response';

const REVIEW_CAP_BIT = 0x04;
const REVIEW_CAP_RESERVED_MASK = 0xf0;
const REVIEW_BODY_MAX_BYTES = 240;
const REVIEW_SIG_HEX_LEN = 128; // 64 bytes Ed25519

interface ReviewRequest {
  rater_pubkey_hash?: unknown;
  content_id?: unknown;
  stars?: unknown;
  body?: unknown;
  tags_bitset?: unknown;
  sigma_mask?: unknown;
  ts_minutes_since_epoch?: unknown;
  sig?: unknown;
  share_with_author?: unknown;
  cap?: unknown;
  sovereign?: unknown;
}

export interface ReviewOk {
  served_by: string;
  ts: string;
  receipt: {
    storage_key: string;
    body_len: number;
  };
  accepted: true;
  framing: 'sovereign-review';
}

interface ReviewError {
  error: string;
  served_by: string;
  ts: string;
}

function isObject(b: unknown): b is Record<string, unknown> {
  return typeof b === 'object' && b !== null;
}

function utf8ByteLen(s: string): number {
  return new TextEncoder().encode(s).length;
}

export default async function handler(
  req: NextApiRequest,
  res: NextApiResponse<ReviewOk | ReviewError>
): Promise<void> {
  logHit('content.review', { method: req.method ?? 'GET' });

  if (req.method !== 'POST') {
    const env = envelope();
    res.setHeader('Allow', 'POST');
    res.status(405).json({
      error: 'Method Not Allowed — POST {rater_pubkey_hash, content_id, stars, body, sig, cap}',
      served_by: env.served_by,
      ts: env.ts,
    });
    return;
  }

  const body: unknown = req.body;
  if (!isObject(body)) {
    const env = envelope();
    res.status(400).json({
      error: 'Bad Request — body must be JSON object',
      served_by: env.served_by,
      ts: env.ts,
    });
    return;
  }

  const reqBody = body as ReviewRequest;
  const cap = typeof reqBody.cap === 'number' ? reqBody.cap : 0;
  const sovereignFlag = reqBody.sovereign === true;
  const sovereignAllowed = isSovereignFromIncoming(req.headers, sovereignFlag);

  const capAllowed = (cap & CONTENT_CAP_REVIEW_BODY) === CONTENT_CAP_REVIEW_BODY;
  if (!capAllowed && !sovereignAllowed) {
    const d = deny('cap CONTENT_CAP_REVIEW_BODY=0x20000 required (or sovereign-header)', cap);
    logEvent(d.body);
    const env = envelope();
    res.status(d.status).json({
      error: (d.body.extra?.['reason'] as string) ?? 'denied',
      served_by: env.served_by,
      ts: env.ts,
    });
    return;
  }

  const raterPubkeyHashHex =
    typeof reqBody.rater_pubkey_hash === 'string' ? reqBody.rater_pubkey_hash : '';
  const contentId =
    typeof reqBody.content_id === 'number' ? Math.floor(reqBody.content_id) : -1;
  const stars = typeof reqBody.stars === 'number' ? Math.floor(reqBody.stars) : -1;
  const reviewBody = typeof reqBody.body === 'string' ? reqBody.body : '';
  const tagsBitset =
    typeof reqBody.tags_bitset === 'number' ? Math.floor(reqBody.tags_bitset) : -1;
  const sigmaMask =
    typeof reqBody.sigma_mask === 'number' ? Math.floor(reqBody.sigma_mask) : -1;
  const tsMinutes =
    typeof reqBody.ts_minutes_since_epoch === 'number'
      ? Math.floor(reqBody.ts_minutes_since_epoch)
      : -1;
  const sigHex = typeof reqBody.sig === 'string' ? reqBody.sig : '';

  if (!/^[0-9a-fA-F]{16}$/.test(raterPubkeyHashHex)) {
    const env = envelope();
    res.status(400).json({
      error: 'Bad Request — rater_pubkey_hash must be 16 hex chars',
      served_by: env.served_by,
      ts: env.ts,
    });
    return;
  }
  if (contentId < 0 || contentId > 0xffffffff) {
    const env = envelope();
    res.status(400).json({
      error: 'Bad Request — content_id must be u32',
      served_by: env.served_by,
      ts: env.ts,
    });
    return;
  }
  if (stars < 1 || stars > 5) {
    const env = envelope();
    res.status(400).json({
      error: 'Bad Request — review stars must be in 1..=5 (revoke via /api/content/rate stars=0)',
      served_by: env.served_by,
      ts: env.ts,
    });
    return;
  }
  const bodyBytes = utf8ByteLen(reviewBody);
  if (bodyBytes > REVIEW_BODY_MAX_BYTES) {
    const env = envelope();
    res.status(413).json({
      error: `Payload Too Large — body ${bodyBytes}B exceeds ${REVIEW_BODY_MAX_BYTES}B`,
      served_by: env.served_by,
      ts: env.ts,
    });
    return;
  }
  if (tagsBitset < 0 || tagsBitset > 65535) {
    const env = envelope();
    res.status(400).json({
      error: 'Bad Request — tags_bitset must be u16',
      served_by: env.served_by,
      ts: env.ts,
    });
    return;
  }
  if (sigmaMask < 0 || sigmaMask > 255 || (sigmaMask & REVIEW_CAP_RESERVED_MASK) !== 0) {
    const env = envelope();
    res.status(400).json({
      error: 'Bad Request — sigma_mask must be u8 with reserved bits zero',
      served_by: env.served_by,
      ts: env.ts,
    });
    return;
  }
  if ((sigmaMask & REVIEW_CAP_BIT) === 0) {
    const env = envelope();
    res.status(400).json({
      error: 'Bad Request — sigma_mask CAP_REVIEW_BODY bit (0x04) must be set',
      served_by: env.served_by,
      ts: env.ts,
    });
    return;
  }
  if (tsMinutes < 0 || tsMinutes > 0xffffffff) {
    const env = envelope();
    res.status(400).json({
      error: 'Bad Request — ts_minutes_since_epoch must be u32',
      served_by: env.served_by,
      ts: env.ts,
    });
    return;
  }
  if (sigHex.length !== REVIEW_SIG_HEX_LEN || !/^[0-9a-fA-F]+$/.test(sigHex)) {
    const env = envelope();
    res.status(400).json({
      error: `Bad Request — sig must be ${REVIEW_SIG_HEX_LEN} hex chars (Ed25519 64 bytes)`,
      served_by: env.served_by,
      ts: env.ts,
    });
    return;
  }

  const cidHex = contentId.toString(16).padStart(8, '0');
  const storageKey = `${cidHex}-${raterPubkeyHashHex.toLowerCase().slice(0, 16)}`;

  logEvent(
    auditEvent('content.review', cap, sovereignAllowed, 'ok', {
      content_id: contentId,
      body_len: bodyBytes,
      stars,
    })
  );

  const env = envelope();
  res.status(200).json({
    served_by: env.served_by,
    ts: env.ts,
    receipt: { storage_key: storageKey, body_len: bodyBytes },
    accepted: true,
    framing: 'sovereign-review',
  });
}

// ─── Inline tests ─────────────────────────────────────────────────────────

interface MockedResponse {
  statusCode: number;
  body: unknown;
  headers: Record<string, string>;
}

function mockReqRes(method: string, body?: unknown, headers: Record<string, string> = {}) {
  const out: MockedResponse = { statusCode: 0, body: null, headers: {} };
  const req = { method, query: {}, headers, body } as unknown as NextApiRequest;
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

function assert(cond: boolean, msg: string): void {
  if (!cond) throw new Error(`assert failed : ${msg}`);
}

const VALID_REVIEW = {
  rater_pubkey_hash: 'deadbeefcafebabe',
  content_id: 42,
  stars: 5,
  body: 'great pacing — looking forward to remixing this',
  tags_bitset: 0x21,
  sigma_mask: 0x05, // CAP_RATE | CAP_REVIEW_BODY (0x01 | 0x04)
  ts_minutes_since_epoch: 1_000_000,
  sig: 'a'.repeat(128),
  cap: CONTENT_CAP_REVIEW_BODY,
};

export async function testReviewCapDenies(): Promise<void> {
  const { req, res, out } = mockReqRes('POST', { ...VALID_REVIEW, cap: 0 });
  await handler(req, res);
  assert(out.statusCode === 403, `cap=0 must 403, got ${out.statusCode}`);
}

export async function testReviewValidAccepts(): Promise<void> {
  const { req, res, out } = mockReqRes('POST', VALID_REVIEW);
  await handler(req, res);
  assert(out.statusCode === 200, `valid review must 200, got ${out.statusCode}`);
  const b = out.body as ReviewOk;
  assert(b.accepted === true, 'accepted');
  assert(b.framing === 'sovereign-review', 'framing');
  assert(b.receipt.body_len > 0, 'body_len > 0');
}

export async function testReviewBodyTooLargeRejects(): Promise<void> {
  const { req, res, out } = mockReqRes('POST', { ...VALID_REVIEW, body: 'x'.repeat(241) });
  await handler(req, res);
  assert(out.statusCode === 413, '241-byte body must yield 413');
}

export async function testReviewBadSigRejects(): Promise<void> {
  const { req, res, out } = mockReqRes('POST', { ...VALID_REVIEW, sig: 'short' });
  await handler(req, res);
  assert(out.statusCode === 400, 'short sig must 400');
}

export async function testReviewStarsZeroRejects(): Promise<void> {
  const { req, res, out } = mockReqRes('POST', { ...VALID_REVIEW, stars: 0 });
  await handler(req, res);
  assert(out.statusCode === 400, 'stars=0 must 400 (use /rate to revoke)');
}

declare const require: { main?: unknown } | undefined;
declare const module: { id?: string } | undefined;
const isMain =
  typeof require !== 'undefined' &&
  typeof module !== 'undefined' &&
  require.main === module;
if (isMain) {
  void (async () => {
    await testReviewCapDenies();
    await testReviewValidAccepts();
    await testReviewBodyTooLargeRejects();
    await testReviewBadSigRejects();
    await testReviewStarsZeroRejects();
    // eslint-disable-next-line no-console
    console.log('content/review.ts : OK · 5 inline tests passed');
  })();
}
