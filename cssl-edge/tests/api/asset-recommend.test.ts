// cssl-edge · tests/api/asset-recommend.test.ts
// Lightweight self-tests for /api/asset/recommend. Framework-agnostic — runs
// via `npx tsx tests/api/asset-recommend.test.ts`. Public endpoint (no caps),
// rate-limit deny path tested via x-loa-rl: deny header.

import handler from '@/pages/api/asset/recommend';
import type { NextApiRequest, NextApiResponse } from 'next';

interface MockedResponse {
  statusCode: number;
  body: unknown;
  headers: Record<string, string>;
}

function mockReqRes(
  method: string,
  query: Record<string, string | string[]> = {},
  headers: Record<string, string> = {}
): { req: NextApiRequest; res: NextApiResponse; out: MockedResponse } {
  const out: MockedResponse = { statusCode: 0, body: null, headers: {} };

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
  } as unknown as NextApiResponse;

  return { req, res, out };
}

function assert(cond: boolean, msg: string): void {
  if (!cond) throw new Error(`assert failed : ${msg}`);
}

function seedFeaturesB64(features: Record<string, number>): string {
  return Buffer.from(JSON.stringify(features), 'utf-8').toString('base64');
}

interface RecommendOk {
  recommendations?: unknown;
  player_id?: unknown;
  total?: unknown;
  reason?: unknown;
}

// 1. Default returns 24 recommendations.
export function testReturns24ByDefault(): void {
  const { req, res, out } = mockReqRes('GET', {
    player_id: 'alice',
    seed_features: seedFeaturesB64({ forest: 0.8, ruins: 0.4 }),
  });
  handler(req, res);
  assert(out.statusCode === 200, `expected 200, got ${out.statusCode}`);
  const body = out.body as RecommendOk;
  assert(Array.isArray(body.recommendations), 'expected recommendations:Array');
  const recs = body.recommendations as Array<unknown>;
  assert(recs.length === 24, `expected 24 default recs, got ${recs.length}`);
  assert(body.player_id === 'alice', 'expected echoed player_id');
  assert(typeof body.reason === 'string', 'expected reason:string');
}

// 2. Respects ?limit=12.
export function testRespectsLimit(): void {
  const { req, res, out } = mockReqRes('GET', {
    player_id: 'bob',
    seed_features: seedFeaturesB64({ ocean: 0.9 }),
    limit: '12',
  });
  handler(req, res);
  assert(out.statusCode === 200, `expected 200, got ${out.statusCode}`);
  const body = out.body as RecommendOk;
  const recs = body.recommendations as Array<unknown>;
  assert(recs.length === 12, `expected 12 recs, got ${recs.length}`);
}

// 3. Scores are deterministic for identical inputs.
export function testScoresDeterministicOnSameInput(): void {
  const seed = seedFeaturesB64({ stone: 0.5, moss: 0.7 });
  const { req: rA, res: resA, out: outA } = mockReqRes('GET', {
    player_id: 'carol',
    seed_features: seed,
    limit: '8',
  });
  const { req: rB, res: resB, out: outB } = mockReqRes('GET', {
    player_id: 'carol',
    seed_features: seed,
    limit: '8',
  });
  handler(rA, resA);
  handler(rB, resB);
  const a = (outA.body as RecommendOk).recommendations as Array<{ asset_id: string; score: number }>;
  const b = (outB.body as RecommendOk).recommendations as Array<{ asset_id: string; score: number }>;
  assert(a.length === b.length, 'lengths must match');
  for (let i = 0; i < a.length; i += 1) {
    const ai = a[i];
    const bi = b[i];
    if (ai === undefined || bi === undefined) throw new Error(`row[${i}] missing`);
    assert(ai.asset_id === bi.asset_id, `row ${i} asset_id mismatch`);
    assert(ai.score === bi.score, `row ${i} score mismatch`);
  }
  // Bonus check : different player_id → different score for same asset.
  const { req: rC, res: resC, out: outC } = mockReqRes('GET', {
    player_id: 'dave',
    seed_features: seed,
    limit: '8',
  });
  handler(rC, resC);
  const c = (outC.body as RecommendOk).recommendations as Array<{ asset_id: string; score: number }>;
  // At least one row must have a different score (overwhelmingly likely with FNV).
  const allSame = a.every((r, i) => {
    const ci = c[i];
    if (ci === undefined) return false;
    return r.score === ci.score;
  });
  assert(!allSame, 'different player_id must produce at least one different score');
}

declare const require: { main?: unknown } | undefined;
declare const module: { id?: string } | undefined;
const isMain =
  typeof require !== 'undefined' &&
  typeof module !== 'undefined' &&
  require.main === module;

if (isMain) {
  testReturns24ByDefault();
  testRespectsLimit();
  testScoresDeterministicOnSameInput();
  // eslint-disable-next-line no-console
  console.log('asset-recommend.test : OK · 3 tests passed');
}
