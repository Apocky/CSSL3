// cssl-edge · /api/content/aggregate
// k-anonymized aggregate read for a content_id. Public-readable BUT only
// returns mean_stars + tag_counts when distinct_rater_count >= 5 (single)
// or marks `trending: true` when >= 10. Below k-floor the response holds
// only `{ visibility: 'hidden', distinct_rater_count }`.
//
// Method : GET ?content_id=<u32>
// Response 200 : AggregateView (always — even Hidden returns shape with NULLs)
// Response 4xx : { error, served_by, ts }

import type { NextApiRequest, NextApiResponse } from 'next';
import { auditEvent, logEvent } from '@/lib/audit';
import { envelope, logHit } from '@/lib/response';

// k-anon floors mirror the Rust crate constants.
const K_FLOOR_SINGLE = 5;
const K_FLOOR_TRENDING = 10;

export interface AggregateOk {
  served_by: string;
  ts: string;
  content_id: number;
  visibility: 'hidden' | 'visible' | 'trending';
  distinct_rater_count: number;
  mean_stars: number | null;
  tag_top: { tag: string; count: number }[];
  k_floor_single: number;
  k_floor_trending: number;
  framing: 'k-anon-aggregate';
}

interface AggregateError {
  error: string;
  served_by: string;
  ts: string;
}

// Stage-0 stub : returns an empty aggregate with `hidden` visibility ; a
// follow-up wires this to the supabase content_rating_aggregates VIEW.
// CRITICAL : even the stub MUST return only {distinct_rater_count, visibility}
// when below k-floor — defense-in-depth so the substrate-invariant is
// preserved even when persistence is mocked.
//
// When STAGE-0=stub, the in-memory store is empty → distinct_rater_count=0.
function stubAggregate(contentId: number): AggregateOk {
  const env = envelope();
  return {
    served_by: env.served_by,
    ts: env.ts,
    content_id: contentId,
    visibility: 'hidden',
    distinct_rater_count: 0,
    mean_stars: null,
    tag_top: [],
    k_floor_single: K_FLOOR_SINGLE,
    k_floor_trending: K_FLOOR_TRENDING,
    framing: 'k-anon-aggregate',
  };
}

export default function handler(
  req: NextApiRequest,
  res: NextApiResponse<AggregateOk | AggregateError>
): void {
  logHit('content.aggregate', { method: req.method ?? 'GET' });

  if (req.method !== 'GET') {
    const env = envelope();
    res.setHeader('Allow', 'GET');
    res.status(405).json({
      error: 'Method Not Allowed — GET ?content_id=<u32>',
      served_by: env.served_by,
      ts: env.ts,
    });
    return;
  }

  const raw = req.query['content_id'];
  const cidStr = Array.isArray(raw) ? raw[0] : raw;
  const contentId = typeof cidStr === 'string' ? parseInt(cidStr, 10) : NaN;
  if (!Number.isFinite(contentId) || contentId < 0 || contentId > 0xffffffff) {
    const env = envelope();
    res.status(400).json({
      error: 'Bad Request — content_id query-param must be u32',
      served_by: env.served_by,
      ts: env.ts,
    });
    return;
  }

  // Aggregate reads are NOT cap-gated (public visibility tier-1) ; the
  // visibility itself is the gate (Hidden returns no per-rater detail).
  const agg = stubAggregate(contentId);

  logEvent(
    auditEvent('content.aggregate', 0, false, 'ok', {
      content_id: contentId,
      visibility: agg.visibility,
      distinct_rater_count: agg.distinct_rater_count,
    })
  );

  res.status(200).json(agg);
}

// ─── Inline tests ─────────────────────────────────────────────────────────

interface MockedResponse {
  statusCode: number;
  body: unknown;
  headers: Record<string, string>;
}

function mockReqRes(method: string, query: Record<string, string> = {}, headers: Record<string, string> = {}) {
  const out: MockedResponse = { statusCode: 0, body: null, headers: {} };
  const req = { method, query, headers, body: undefined } as unknown as NextApiRequest;
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

export function testAggregateGetReturnsHiddenStub(): void {
  const { req, res, out } = mockReqRes('GET', { content_id: '7' });
  handler(req, res);
  assert(out.statusCode === 200, `200 expected, got ${out.statusCode}`);
  const b = out.body as AggregateOk;
  assert(b.visibility === 'hidden', 'stub returns hidden');
  assert(b.distinct_rater_count === 0, 'stub k-count=0');
  assert(b.mean_stars === null, 'mean null when hidden');
  assert(b.tag_top.length === 0, 'tag_top empty when hidden');
  assert(b.k_floor_single === 5, 'k_floor_single = 5');
  assert(b.k_floor_trending === 10, 'k_floor_trending = 10');
}

export function testAggregateMissingContentIdRejects(): void {
  const { req, res, out } = mockReqRes('GET', {});
  handler(req, res);
  assert(out.statusCode === 400, 'missing content_id must 400');
}

export function testAggregateBadContentIdRejects(): void {
  const { req, res, out } = mockReqRes('GET', { content_id: 'abc' });
  handler(req, res);
  assert(out.statusCode === 400, 'non-numeric content_id must 400');
}

export function testAggregateRejectsNonGet(): void {
  const { req, res, out } = mockReqRes('POST', { content_id: '7' });
  handler(req, res);
  assert(out.statusCode === 405, 'POST must 405');
  assert(out.headers['Allow'] === 'GET', 'Allow: GET header');
}

declare const require: { main?: unknown } | undefined;
declare const module: { id?: string } | undefined;
const isMain =
  typeof require !== 'undefined' &&
  typeof module !== 'undefined' &&
  require.main === module;
if (isMain) {
  testAggregateGetReturnsHiddenStub();
  testAggregateMissingContentIdRejects();
  testAggregateBadContentIdRejects();
  testAggregateRejectsNonGet();
  // eslint-disable-next-line no-console
  console.log('content/aggregate.ts : OK · 4 inline tests passed');
}
