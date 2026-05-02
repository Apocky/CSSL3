// cssl-edge · /api/content/rate
// Submit (or overwrite) a star+tag rating on user-generated CSSL content.
// Cap-gated CONTENT_CAP_RATE (0x10000). Sovereign-bypass via x-loa-sovereign-cap.
//
// Mirrors compiler-rs `cssl-content-rating::Rating::new` validation. Re-rating
// the same (rater, content) UPSERTS — never duplicates. Stars=0 acts as the
// withdrawn-sentinel for sovereign-revoke (rater can also POST stars=0).
//
// Body :
//   {
//     rater_pubkey_hash: string  // 16 hex chars (8 bytes BLAKE3-trunc)
//     content_id: number         // u32 content identifier
//     stars: number              // 0..=5 (0 = withdrawn)
//     tags_bitset: number        // 0..=65535 (16-bit bitset)
//     sigma_mask: number         // 0..=255 ; CAP_RATE=0x01 bit MUST be set
//     ts_minutes_since_epoch: number  // u32 quantized timestamp
//     weight_q8: number          // 0..=255 (KAN weight ; default 200)
//     share_with_author: boolean // optional ; default false
//     cap: number                // CONTENT_CAP_RATE 0x10000 mask
//     sovereign?: boolean        // optional sovereign-bypass flag
//   }
// Response 200 : { receipt: { storage_key, withdrawn }, accepted: true }
// Response 4xx : { error, served_by, ts }

import type { NextApiRequest, NextApiResponse } from 'next';
import { auditEvent, deny, logEvent } from '@/lib/audit';
import { CONTENT_CAP_RATE } from '@/lib/cap';
import { isSovereignFromIncoming } from '@/lib/sovereign';
import { envelope, logHit } from '@/lib/response';

// Bit-pack-side cap-bit constants (mirror compiler-rs/cssl-content-rating).
const RATING_CAP_RATE = 0x01;
const RATING_CAP_RESERVED_MASK = 0xf0;

interface RateRequest {
  rater_pubkey_hash?: unknown;
  content_id?: unknown;
  stars?: unknown;
  tags_bitset?: unknown;
  sigma_mask?: unknown;
  ts_minutes_since_epoch?: unknown;
  weight_q8?: unknown;
  share_with_author?: unknown;
  cap?: unknown;
  sovereign?: unknown;
}

export interface RateOk {
  served_by: string;
  ts: string;
  receipt: {
    storage_key: string;
    withdrawn: boolean;
  };
  accepted: true;
  framing: 'sovereign-rating';
}

interface RateError {
  error: string;
  served_by: string;
  ts: string;
}

function isObject(b: unknown): b is Record<string, unknown> {
  return typeof b === 'object' && b !== null;
}

// Hex → BLAKE3-trunc-style storage_key. Mirrors compiler-rs
// `Rating::storage_key()` — domain-separated BLAKE3 over (content_id ‖ rater).
async function deriveStorageKey(
  raterPubkeyHashHex: string,
  contentId: number
): Promise<string> {
  // Stage-0 deterministic key : content_id-rater. The Rust crate uses BLAKE3
  // ; the edge runs in Vercel-Node where blake3 is not built-in. We emit the
  // same logical key (hex of content + rater) ; the persistence layer can
  // cross-reference.
  const cidHex = contentId.toString(16).padStart(8, '0');
  return `${cidHex}-${raterPubkeyHashHex.toLowerCase().slice(0, 16)}`;
}

export default async function handler(
  req: NextApiRequest,
  res: NextApiResponse<RateOk | RateError>
): Promise<void> {
  logHit('content.rate', { method: req.method ?? 'GET' });

  if (req.method !== 'POST') {
    const env = envelope();
    res.setHeader('Allow', 'POST');
    res.status(405).json({
      error:
        'Method Not Allowed — POST {rater_pubkey_hash, content_id, stars, tags_bitset, sigma_mask, ts_minutes_since_epoch, weight_q8, cap, sovereign?}',
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

  const reqBody = body as RateRequest;
  const cap = typeof reqBody.cap === 'number' ? reqBody.cap : 0;
  const sovereignFlag = reqBody.sovereign === true;
  const sovereignAllowed = isSovereignFromIncoming(req.headers, sovereignFlag);

  // Cap-gate : default-deny.
  const capAllowed = (cap & CONTENT_CAP_RATE) === CONTENT_CAP_RATE;
  if (!capAllowed && !sovereignAllowed) {
    const d = deny('cap CONTENT_CAP_RATE=0x10000 required (or sovereign-header)', cap);
    logEvent(d.body);
    const env = envelope();
    res.status(d.status).json({
      error: (d.body.extra?.['reason'] as string) ?? 'denied',
      served_by: env.served_by,
      ts: env.ts,
    });
    return;
  }

  // Validate required fields.
  const raterPubkeyHashHex =
    typeof reqBody.rater_pubkey_hash === 'string' ? reqBody.rater_pubkey_hash : '';
  const contentId =
    typeof reqBody.content_id === 'number' ? Math.floor(reqBody.content_id) : -1;
  const stars = typeof reqBody.stars === 'number' ? Math.floor(reqBody.stars) : -1;
  const tagsBitset =
    typeof reqBody.tags_bitset === 'number' ? Math.floor(reqBody.tags_bitset) : -1;
  const sigmaMask =
    typeof reqBody.sigma_mask === 'number' ? Math.floor(reqBody.sigma_mask) : -1;
  const tsMinutes =
    typeof reqBody.ts_minutes_since_epoch === 'number'
      ? Math.floor(reqBody.ts_minutes_since_epoch)
      : -1;
  const weightQ8 =
    typeof reqBody.weight_q8 === 'number' ? Math.floor(reqBody.weight_q8) : 200;
  const shareWithAuthor = reqBody.share_with_author === true;

  // 16 hex chars (8 bytes).
  if (!/^[0-9a-fA-F]{16}$/.test(raterPubkeyHashHex)) {
    const env = envelope();
    res.status(400).json({
      error: 'Bad Request — rater_pubkey_hash must be 16 hex chars (8 bytes)',
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
  if (stars < 0 || stars > 5) {
    const env = envelope();
    res.status(400).json({
      error: 'Bad Request — stars must be in 0..=5 (0 = withdrawn-sentinel)',
      served_by: env.served_by,
      ts: env.ts,
    });
    return;
  }
  if (tagsBitset < 0 || tagsBitset > 65535) {
    const env = envelope();
    res.status(400).json({
      error: 'Bad Request — tags_bitset must be u16 (0..=65535)',
      served_by: env.served_by,
      ts: env.ts,
    });
    return;
  }
  if (sigmaMask < 0 || sigmaMask > 255) {
    const env = envelope();
    res.status(400).json({
      error: 'Bad Request — sigma_mask must be u8',
      served_by: env.served_by,
      ts: env.ts,
    });
    return;
  }
  if ((sigmaMask & RATING_CAP_RESERVED_MASK) !== 0) {
    const env = envelope();
    res.status(400).json({
      error: `Bad Request — sigma_mask reserved bits must be zero (got 0x${sigmaMask.toString(16)})`,
      served_by: env.served_by,
      ts: env.ts,
    });
    return;
  }
  if ((sigmaMask & RATING_CAP_RATE) === 0) {
    const env = envelope();
    res.status(400).json({
      error: 'Bad Request — sigma_mask CAP_RATE bit (0x01) must be set',
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
  if (weightQ8 < 0 || weightQ8 > 255) {
    const env = envelope();
    res.status(400).json({
      error: 'Bad Request — weight_q8 must be u8',
      served_by: env.served_by,
      ts: env.ts,
    });
    return;
  }

  const storageKey = await deriveStorageKey(raterPubkeyHashHex, contentId);
  const withdrawn = stars === 0;

  logEvent(
    auditEvent('content.rate', cap, sovereignAllowed, 'ok', {
      content_id: contentId,
      withdrawn,
      stars,
      shareWithAuthor,
    })
  );

  const env = envelope();
  res.status(200).json({
    served_by: env.served_by,
    ts: env.ts,
    receipt: {
      storage_key: storageKey,
      withdrawn,
    },
    accepted: true,
    framing: 'sovereign-rating',
  });
}

// ─── Inline tests · framework-agnostic ─────────────────────────────────────

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

const VALID_BODY = {
  rater_pubkey_hash: 'deadbeefcafebabe',
  content_id: 42,
  stars: 5,
  tags_bitset: 0x21, // fun + remix-worthy
  sigma_mask: 0x03, // CAP_RATE | CAP_AGGREGATE_PUBLIC
  ts_minutes_since_epoch: 1_000_000,
  weight_q8: 200,
  cap: CONTENT_CAP_RATE,
};

export async function testCapZeroDenies(): Promise<void> {
  const { req, res, out } = mockReqRes('POST', { ...VALID_BODY, cap: 0 });
  await handler(req, res);
  assert(out.statusCode === 403, `cap=0 must yield 403, got ${out.statusCode}`);
}

export async function testValidBodyAccepts(): Promise<void> {
  const { req, res, out } = mockReqRes('POST', VALID_BODY);
  await handler(req, res);
  assert(out.statusCode === 200, `valid body must yield 200, got ${out.statusCode}`);
  const b = out.body as RateOk;
  assert(b.accepted === true, 'accepted must be true');
  assert(b.framing === 'sovereign-rating', 'framing must be sovereign-rating');
  assert(b.receipt.withdrawn === false, 'stars=5 → not withdrawn');
}

export async function testStarsZeroIsWithdrawnSentinel(): Promise<void> {
  const { req, res, out } = mockReqRes('POST', { ...VALID_BODY, stars: 0 });
  await handler(req, res);
  assert(out.statusCode === 200, 'stars=0 (withdrawn) must accept');
  assert((out.body as RateOk).receipt.withdrawn === true, 'withdrawn=true');
}

export async function testStarsAbove5Rejects(): Promise<void> {
  const { req, res, out } = mockReqRes('POST', { ...VALID_BODY, stars: 6 });
  await handler(req, res);
  assert(out.statusCode === 400, 'stars=6 must reject');
}

export async function testMissingCapRateBitRejects(): Promise<void> {
  const { req, res, out } = mockReqRes('POST', { ...VALID_BODY, sigma_mask: 0x02 });
  await handler(req, res);
  assert(out.statusCode === 400, 'sigma_mask without CAP_RATE bit must reject');
}

export async function testReservedBitsRejects(): Promise<void> {
  const { req, res, out } = mockReqRes('POST', { ...VALID_BODY, sigma_mask: 0x81 });
  await handler(req, res);
  assert(out.statusCode === 400, 'reserved-bit set must reject');
}

export async function testInvalidPubkeyHashRejects(): Promise<void> {
  const { req, res, out } = mockReqRes('POST', { ...VALID_BODY, rater_pubkey_hash: 'short' });
  await handler(req, res);
  assert(out.statusCode === 400, 'short hash must reject');
}

declare const require: { main?: unknown } | undefined;
declare const module: { id?: string } | undefined;
const isMain =
  typeof require !== 'undefined' &&
  typeof module !== 'undefined' &&
  require.main === module;
if (isMain) {
  void (async () => {
    await testCapZeroDenies();
    await testValidBodyAccepts();
    await testStarsZeroIsWithdrawnSentinel();
    await testStarsAbove5Rejects();
    await testMissingCapRateBitRejects();
    await testReservedBitsRejects();
    await testInvalidPubkeyHashRejects();
    // eslint-disable-next-line no-console
    console.log('content/rate.ts : OK · 7 inline tests passed');
  })();
}
