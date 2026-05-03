// cssl-edge · tests/api/mneme/mneme.test.ts
// MNEME — single-file smoke suite covering every pipeline stage + every
// HTTP route surface. Stubs all external calls; runs without env config.
//
// Run via   npm run test:mneme

import type { NextApiRequest, NextApiResponse } from 'next';

import {
    defaultMask,
    revokeMask,
    isRevoked,
    maskRevokedAt,
    maskToHex,
    maskFromHex,
    SIGMA_WIRE_LEN,
} from '@/lib/mneme/sigma';

import { validateCsl, extractTopicKey, composeEmbeddingText, composeTsQuery } from '@/lib/mneme/csl';

import { reciprocalRankFusion, maxScoreFor } from '@/lib/mneme/store';
import type { ChannelHit, ChannelName } from '@/lib/mneme/types';

import { chunkConversation, extractFull } from '@/lib/mneme/prompts/extract-full';
import { buildWindows, shouldRunDetail, extractDetail } from '@/lib/mneme/prompts/extract-detail';
import { verifyCandidate } from '@/lib/mneme/prompts/verify';
import { classifyCandidate } from '@/lib/mneme/prompts/classify';
import { analyzeQuery } from '@/lib/mneme/prompts/query-analyze';
import { synthesize } from '@/lib/mneme/prompts/synthesize';

import {
    ingestPipeline,
    rememberPipeline,
    deterministicMsgId,
    mergeCandidates,
} from '@/lib/mneme/pipeline-ingest';
import {
    retrievePipeline,
    computeTemporalFacts,
} from '@/lib/mneme/pipeline-retrieve';

import healthHandler from '@/pages/api/mneme/[profile]/health';
import smokeHandler from '@/pages/api/mneme/[profile]/smoke';
import ingestHandler from '@/pages/api/mneme/[profile]/ingest';
import recallHandler from '@/pages/api/mneme/[profile]/recall';
import rememberHandler from '@/pages/api/mneme/[profile]/remember';
import listHandler from '@/pages/api/mneme/[profile]/list';
import forgetHandler from '@/pages/api/mneme/[profile]/forget';
import exportHandler from '@/pages/api/mneme/[profile]/export';

// ── Test harness ───────────────────────────────────────────────────────

function assert(cond: boolean, msg: string): void {
    if (!cond) throw new Error('assert failed: ' + msg);
}

interface MockedResponse {
    statusCode: number;
    body: unknown;
    headers: Record<string, string>;
}

function mockReqRes(
    method: string,
    query: Record<string, string> = {},
    body: unknown = undefined,
): { req: NextApiRequest; res: NextApiResponse; out: MockedResponse } {
    const out: MockedResponse = { statusCode: 0, body: null, headers: {} };
    const req = { method, query, headers: {}, body } as unknown as NextApiRequest;
    const res = {
        status(code: number) { out.statusCode = code; return this; },
        json(payload: unknown) { out.body = payload; return this; },
        setHeader(k: string, v: string) { out.headers[k] = v; return this; },
    } as unknown as NextApiResponse;
    return { req, res, out };
}

// ── Stub call-tool factory ─────────────────────────────────────────────

function makeStubCallTool(): <T>(opts: { tool: { name: string }; user: string }) => Promise<T> {
    return async <T>(opts: { tool: { name: string }; user: string }): Promise<T> => {
        switch (opts.tool.name) {
            case 'mneme_extract':
                return { extracted: [{
                    csl: 'user.pref.pkg-mgr ⊗ pnpm',
                    paraphrase: 'User prefers pnpm.',
                    span: [0, 0],
                }] } as unknown as T;
            case 'mneme_extract_detail':
                return { extracted: [] } as unknown as T;
            case 'mneme_verify':
                return { verdict: 'pass' } as unknown as T;
            case 'mneme_classify':
                return {
                    type: 'fact',
                    topic_key: 'user.pref.pkg-mgr',
                    search_queries: ['which package manager?', 'pnpm or npm?', 'JS package tool?'],
                } as unknown as T;
            case 'mneme_query_analyze':
                return {
                    topic_keys:      ['user.pref.pkg-mgr'],
                    fts_terms:       ['package','manager','pnpm'],
                    hyde_csl:        'user.pref.pkg-mgr ⊗ pnpm',
                    hyde_paraphrase: 'The user prefers pnpm.',
                    is_temporal:     false,
                } as unknown as T;
            case 'mneme_synthesize':
                return {
                    result_nl:  'Stub synth.',
                    result_csl: 'user.pref.pkg-mgr ⊗ pnpm',
                    citations:  [],
                    confidence: 0.0,
                } as unknown as T;
            default:
                return {} as T;
        }
    };
}

const STUB_VEC = (): Float32Array => {
    const v = new Float32Array(1024);
    for (let i = 0; i < v.length; i++) v[i] = (i % 7) / 7;
    return v;
};

// ══════════════════════════════════════════════════════════════════════
// 1. SIGMA codec tests
// ══════════════════════════════════════════════════════════════════════

export function testSigmaDefault(): void {
    const m = defaultMask();
    assert(m.length === SIGMA_WIRE_LEN, 'wire length');
    assert(!isRevoked(m), 'fresh mask not revoked');
}

export function testSigmaRevokeMonotone(): void {
    const m = defaultMask();
    const r = revokeMask(m, 1714521600);
    assert(isRevoked(r), 'revoked after revokeMask');
    assert(maskRevokedAt(r) === 1714521600, 'revoke ts preserved');
}

export function testSigmaHexRoundTrip(): void {
    const m = defaultMask();
    const hex = maskToHex(m);
    const back = maskFromHex(hex);
    assert(back.length === m.length, 'hex round-trip length');
    for (let i = 0; i < m.length; i++) assert(m[i] === back[i], `byte ${i}`);
}

// ══════════════════════════════════════════════════════════════════════
// 2. CSL validator + topic-key tests
// ══════════════════════════════════════════════════════════════════════

export function testCslValidPaths(): void {
    const cases = [
        'user.pref.pkg-mgr ⊗ pnpm',
        'flow.compact-context - { tail-summarise → ingest.mneme → drop-old }',
        "user.theme = dark",
    ];
    for (const c of cases) {
        const v = validateCsl(c);
        assert(v.ok, `valid: ${c} (diags: ${JSON.stringify(v.diags)})`);
        assert(v.topic_key !== null, `topic for: ${c}`);
    }
}

export function testCslRejectsEmpty(): void {
    const v = validateCsl('');
    assert(!v.ok, 'empty rejected');
}

export function testTopicKey(): void {
    assert(extractTopicKey('user.pref.pkg-mgr ⊗ pnpm') === 'user.pref.pkg-mgr', 'tatpurusha topic');
    assert(extractTopicKey('flow.compact - { x }') === 'flow.compact', 'flow topic');
    assert(extractTopicKey('') === null, 'empty topic');
}

export function testComposeEmbeddingText(): void {
    const text = composeEmbeddingText(
        'user.pref.pkg-mgr ⊗ pnpm',
        'User prefers pnpm.',
        ['which package manager?', '', '  '],
    );
    assert(text.split('\n').length === 3, 'embedding text has 3 lines');
}

export function testComposeTsQuery(): void {
    const q = composeTsQuery(['package', 'manager', 'pnpm']);
    assert(q.includes('package'), 'query contains package');
    assert(q.includes(' | '), 'or-separator');
}

// ══════════════════════════════════════════════════════════════════════
// 3. RRF fusion tests
// ══════════════════════════════════════════════════════════════════════

export function testRrfDeterministic(): void {
    const channels: Partial<Record<ChannelName, ChannelHit[]>> = {
        topic_exact: [{ memory_id: 'a', score: 1.0 }],
        fts_csl:     [{ memory_id: 'b', score: 0.9 }, { memory_id: 'a', score: 0.6 }],
        vec_direct:  [{ memory_id: 'c', score: 0.7 }, { memory_id: 'a', score: 0.5 }],
    };
    const fused = reciprocalRankFusion(channels, 5);
    assert(fused.length === 3, 'three unique ids');
    assert(fused[0]!.memory_id === 'a', 'a wins via topic_exact + multi-hit');
    assert(fused[1]!.memory_id === 'b' || fused[1]!.memory_id === 'c', 'b or c second');

    // Determinism : same input → same order
    const fused2 = reciprocalRankFusion(channels, 5);
    for (let i = 0; i < fused.length; i++) {
        assert(fused[i]!.memory_id === fused2[i]!.memory_id, `det ${i}`);
    }
}

export function testMaxScoreFor(): void {
    const channels: Partial<Record<ChannelName, ChannelHit[]>> = {
        fts_csl:    [{ memory_id: 'a', score: 0.6 }],
        vec_direct: [{ memory_id: 'a', score: 0.9 }],
    };
    assert(Math.abs(maxScoreFor('a', channels) - 0.9) < 1e-9, 'max chan = 0.9');
    assert(maxScoreFor('z', channels) === 0, 'unknown id score 0');
}

// ══════════════════════════════════════════════════════════════════════
// 4. Extraction PASS-A (full) tests
// ══════════════════════════════════════════════════════════════════════

export function testChunkConversation(): void {
    const msgs = Array.from({ length: 20 }, (_, i) => ({
        role: 'user' as const, content: `m${i} `.repeat(50),
    }));
    const chunks = chunkConversation(msgs, 1000, 2);
    assert(chunks.length >= 2, 'splits long convo');
}

export async function testExtractFullStubbed(): Promise<void> {
    const cands = await extractFull(
        [
            { role: 'user', content: 'I prefer pnpm.' },
            { role: 'assistant', content: 'noted' },
        ],
        { callTool: makeStubCallTool() as never },
    );
    assert(cands.length === 1, 'single candidate from stub');
    assert(cands[0]!.csl.includes('pnpm'), 'csl has pnpm');
    assert(cands[0]!.pass === 'full', 'tagged full');
}

// ══════════════════════════════════════════════════════════════════════
// 5. Extraction PASS-B (detail) tests
// ══════════════════════════════════════════════════════════════════════

export function testShouldRunDetail(): void {
    assert(!shouldRunDetail(8), 'skip <9');
    assert(shouldRunDetail(9), 'run @9');
    assert(shouldRunDetail(50), 'run @50');
}

export function testBuildWindows(): void {
    const msgs = Array.from({ length: 12 }, (_, i) => ({
        role: 'user' as const, content: `m${i}`,
    }));
    const wins = buildWindows(msgs, 5, 2);
    assert(wins.length >= 3, 'multiple windows');
}

export async function testExtractDetailSkipsShort(): Promise<void> {
    const cands = await extractDetail(
        [{ role: 'user', content: 'hi' }],
        { callTool: makeStubCallTool() as never },
    );
    assert(cands.length === 0, 'skips short conversation');
}

// ══════════════════════════════════════════════════════════════════════
// 6. Verify stage tests
// ══════════════════════════════════════════════════════════════════════

export async function testVerifyPass(): Promise<void> {
    const v = await verifyCandidate(
        { csl: 'user.pref.pkg-mgr ⊗ pnpm', paraphrase: 'p', span: [0, 0], pass: 'full' },
        'user said pnpm',
        { callTool: makeStubCallTool() as never },
    );
    assert(v.verdict === 'pass', 'pass verdict');
}

export async function testVerifyDropOnFailure(): Promise<void> {
    const failing = async () => { throw new Error('boom'); };
    const v = await verifyCandidate(
        { csl: 'x', paraphrase: 'p', span: [0, 0], pass: 'full' },
        'src',
        { callTool: failing as never },
    );
    assert(v.verdict === 'dropped', 'drop on call failure');
    assert(typeof v.drop_reason === 'string', 'drop reason set');
}

// ══════════════════════════════════════════════════════════════════════
// 7. Classify stage tests
// ══════════════════════════════════════════════════════════════════════

export async function testClassifyEnforcesTopicNullForEvent(): Promise<void> {
    const stub = async <T>(_: { tool: { name: string } }): Promise<T> => ({
        type: 'event',
        topic_key: 'should-be-removed',
        search_queries: ['q1','q2','q3'],
    } as unknown as T);
    const c = await classifyCandidate(
        { csl: 'deploy ✓', paraphrase: 'd', span: [0,0], pass: 'full', verdict: 'pass' },
        { callTool: stub as never },
    );
    assert(c !== null, 'classify produced output');
    assert(c!.topic_key === null, 'event topic null');
}

export async function testClassifySkipsDropped(): Promise<void> {
    const stub = async <T>(_: { tool: { name: string } }): Promise<T> => ({} as T);
    const c = await classifyCandidate(
        { csl: 'x', paraphrase: 'p', span: [0,0], pass: 'full', verdict: 'dropped',
          drop_reason: 'test' },
        { callTool: stub as never },
    );
    assert(c === null, 'dropped → null');
}

// ══════════════════════════════════════════════════════════════════════
// 8. Query-analyze tests
// ══════════════════════════════════════════════════════════════════════

export async function testAnalyzeQueryStub(): Promise<void> {
    const out = await analyzeQuery('which package manager?', { callTool: makeStubCallTool() as never });
    assert(out.topic_keys.length > 0, 'topic keys returned');
    assert(out.hyde_csl.includes('pnpm') || out.hyde_paraphrase.length > 0, 'HyDE present');
}

export async function testAnalyzeQueryFallback(): Promise<void> {
    const failing = async () => { throw new Error('boom'); };
    const out = await analyzeQuery('what was 2 days ago?', { callTool: failing as never });
    assert(out.is_temporal === true, 'temporal regex caught');
    assert(out.fts_terms.length > 0, 'fallback terms extracted');
}

// ══════════════════════════════════════════════════════════════════════
// 9. Synthesize tests
// ══════════════════════════════════════════════════════════════════════

export async function testSynthCorrectsInvalidCsl(): Promise<void> {
    const stub = async <T>(_: { tool: { name: string } }): Promise<T> => ({
        result_nl: 'fine',
        result_csl: 'this is plain English bad CSL',
        citations: [],
        confidence: 0.9,
    } as unknown as T);
    const out = await synthesize(
        { query: 'q', memories: [] },
        { callTool: stub as never },
    );
    // Confidence floored to 0.3 (no citations)
    assert(out.confidence <= 0.3 + 1e-9, 'confidence floored without citations');
}

export async function testSynthCallFails(): Promise<void> {
    const failing = async () => { throw new Error('rate limited'); };
    const out = await synthesize(
        { query: 'q', memories: [] },
        { callTool: failing as never },
    );
    assert(out.confidence === 0, 'fail → confidence 0');
    assert(out.result_csl.startsWith('memory ⊗ ∅'), 'fail → empty memory CSL');
}

// ══════════════════════════════════════════════════════════════════════
// 10. Pipeline orchestrators
// ══════════════════════════════════════════════════════════════════════

export async function testIngestPipelineMockMode(): Promise<void> {
    const result = await ingestPipeline(null, {
        profile_id: 'test-profile',
        session_id: 'sess-1',
        messages: [
            { role: 'user', content: 'I prefer pnpm.' },
            { role: 'assistant', content: 'noted' },
        ],
    }, {
        callTool: makeStubCallTool() as never,
        embed: async () => STUB_VEC(),
    });
    assert(result.extracted >= 1, 'extracted ≥ 1');
    assert(result.stored === 1, 'one memory stored in mock');
    assert(result.memory_ids[0]!.startsWith('mock-'), 'mock id prefix');
}

export async function testRememberPipelineMockMode(): Promise<void> {
    const m = await rememberPipeline(null, {
        profile_id: 'test-profile',
        csl: 'user.pref.pkg-mgr ⊗ pnpm',
    }, {
        callTool: makeStubCallTool() as never,
        embed: async () => STUB_VEC(),
    });
    assert(m.id === 'mock-remember', 'mock memory id');
    assert(m.topic_key === 'user.pref.pkg-mgr', 'topic key derived');
}

export async function testRememberRejectsInvalidCsl(): Promise<void> {
    let threw = false;
    try {
        await rememberPipeline(null, { profile_id: 'x', csl: '' });
    } catch {
        threw = true;
    }
    assert(threw, 'empty CSL rejected');
}

export async function testRetrievePipelineMockMode(): Promise<void> {
    const out = await retrievePipeline(null, {
        profile_id: 'test',
        query: 'package manager?',
        debug: true,
    }, {
        callTool: makeStubCallTool() as never,
        embed: async () => STUB_VEC(),
        nowMs: () => 0,
    });
    assert(typeof out.result_csl === 'string', 'result_csl string');
    assert(typeof out.confidence === 'number', 'confidence number');
    assert(out.debug !== undefined, 'debug present when requested');
}

export function testDeterministicMsgId(): void {
    const a = deterministicMsgId('s', 'user', 'hello');
    const b = deterministicMsgId('s', 'user', 'hello');
    assert(a === b, 'deterministic');
    assert(a.length === 32, '32 hex');
    const c = deterministicMsgId('s', 'user', 'world');
    assert(a !== c, 'differs by content');
}

export function testMergeCandidates(): void {
    // Threshold is < 0.15 normalised Levenshtein. Two near-identical strings
    // (case-tweak only) collapse into one; longer is kept.
    const a = 'user.pref.pkg-mgr ⊗ pnpm version 8.6.0';
    const b = 'user.pref.pkg-mgr ⊗ pnpm version 8.6.0 ✓';   // adds 2 chars
    const merged = mergeCandidates(
        [{ csl: a, paraphrase: 'p1', span: [0,0], pass: 'full' }],
        [{ csl: b, paraphrase: 'p2', span: [0,0], pass: 'detail' }],
    );
    assert(merged.length === 1, `duplicates merged (got ${merged.length})`);
    assert(merged[0]!.csl === b, 'longer kept');

    // Distinct strings stay separate
    const distinct = mergeCandidates(
        [{ csl: 'user.pref.pkg-mgr ⊗ pnpm', paraphrase: 'p', span: [0,0], pass: 'full' }],
        [{ csl: 'deploy.prod ✓', paraphrase: 'd', span: [0,0], pass: 'detail' }],
    );
    assert(distinct.length === 2, 'distinct preserved');
}

// ══════════════════════════════════════════════════════════════════════
// 11. Temporal pre-compute
// ══════════════════════════════════════════════════════════════════════

export function testTemporalFacts(): void {
    const t = Date.UTC(2026, 4, 1);  // 2026-05-01
    const facts = computeTemporalFacts('what happened 2 days ago?', t);
    assert(facts.some(f => f.iso === '2026-04-29'), '2 days ago = 2026-04-29');
}

// ══════════════════════════════════════════════════════════════════════
// 12. HTTP route tests (stub-mode, no env)
// ══════════════════════════════════════════════════════════════════════

export function testHealthRoute200(): void {
    const { req, res, out } = mockReqRes('GET', { profile: 'scratch' });
    healthHandler(req, res);
    assert(out.statusCode === 200, `health 200, got ${out.statusCode}`);
    const body = out.body as Record<string, unknown>;
    assert(body['ok'] === true, 'ok');
    assert(typeof body['anthropic_configured'] === 'boolean', 'anthropic flag');
}

export function testHealthRoute422OnBadProfile(): void {
    const { req, res, out } = mockReqRes('GET', { profile: 'BAD!' });
    healthHandler(req, res);
    assert(out.statusCode === 422, '422 on bad profile');
}

export async function testSmokeRoute(): Promise<void> {
    const { req, res, out } = mockReqRes('GET', { profile: 'scratch' });
    await smokeHandler(req, res);
    assert(out.statusCode === 200, `smoke 200, got ${out.statusCode}`);
    const body = out.body as Record<string, unknown>;
    assert(body['ok'] === true, 'ok');
    assert(typeof body['ingest'] === 'object', 'ingest block');
    assert(typeof body['retrieve'] === 'object', 'retrieve block');
}

export async function testIngestRoute405OnGet(): Promise<void> {
    const { req, res, out } = mockReqRes('GET', { profile: 'scratch' });
    await ingestHandler(req, res);
    assert(out.statusCode === 405, '405 on GET');
}

export async function testIngestRoute400OnBadBody(): Promise<void> {
    const { req, res, out } = mockReqRes('POST', { profile: 'scratch' }, 'not-json');
    await ingestHandler(req, res);
    assert(out.statusCode === 400, '400 on bad body');
}

export async function testRecallRoute400EmptyQuery(): Promise<void> {
    const { req, res, out } = mockReqRes('POST', { profile: 'scratch' }, { query: '' });
    await recallHandler(req, res);
    assert(out.statusCode === 400, '400 empty query');
}

export async function testRememberRoute400EmptyCsl(): Promise<void> {
    const { req, res, out } = mockReqRes('POST', { profile: 'scratch' }, { csl: '' });
    await rememberHandler(req, res);
    assert(out.statusCode === 400, '400 empty csl');
}

export async function testListRoute200StubMode(): Promise<void> {
    const { req, res, out } = mockReqRes('GET', { profile: 'scratch' });
    await listHandler(req, res);
    assert(out.statusCode === 200, '200 stub list');
    const body = out.body as Record<string, unknown>;
    assert(Array.isArray(body['memories']), 'memories array');
}

export async function testForgetRoute400OnBadUuid(): Promise<void> {
    const { req, res, out } = mockReqRes('POST', { profile: 'scratch' },
        { memory_id: 'not-a-uuid', reason: 'test' });
    await forgetHandler(req, res);
    assert(out.statusCode === 400, '400 bad uuid');
}

export async function testExportRoute200StubMode(): Promise<void> {
    const { req, res, out } = mockReqRes('GET', { profile: 'scratch' });
    await exportHandler(req, res);
    assert(out.statusCode === 200, '200 stub export');
    const body = out.body as Record<string, unknown>;
    assert(typeof body['profile'] === 'object', 'profile block');
    assert(Array.isArray(body['memories']), 'memories array');
    assert(Array.isArray(body['messages']), 'messages array');
}

// ── Run all (when invoked as a script) ─────────────────────────────────

declare const require: { main?: unknown } | undefined;
declare const module: { id?: string } | undefined;
const isMain = typeof require !== 'undefined'
            && typeof module !== 'undefined'
            && require.main === module;

async function runAll(): Promise<void> {
    const tests: Array<[string, () => void | Promise<void>]> = [
        ['sigma-default',                   testSigmaDefault],
        ['sigma-revoke-monotone',           testSigmaRevokeMonotone],
        ['sigma-hex-roundtrip',             testSigmaHexRoundTrip],
        ['csl-valid-paths',                 testCslValidPaths],
        ['csl-rejects-empty',               testCslRejectsEmpty],
        ['topic-key',                       testTopicKey],
        ['compose-embedding-text',          testComposeEmbeddingText],
        ['compose-tsquery',                 testComposeTsQuery],
        ['rrf-deterministic',               testRrfDeterministic],
        ['max-score-for',                   testMaxScoreFor],
        ['chunk-conversation',              testChunkConversation],
        ['extract-full-stubbed',            testExtractFullStubbed],
        ['should-run-detail',               testShouldRunDetail],
        ['build-windows',                   testBuildWindows],
        ['extract-detail-skips-short',      testExtractDetailSkipsShort],
        ['verify-pass',                     testVerifyPass],
        ['verify-drop-on-failure',          testVerifyDropOnFailure],
        ['classify-event-null-topic',       testClassifyEnforcesTopicNullForEvent],
        ['classify-skips-dropped',          testClassifySkipsDropped],
        ['analyze-query-stub',              testAnalyzeQueryStub],
        ['analyze-query-fallback',          testAnalyzeQueryFallback],
        ['synth-corrects-invalid-csl',      testSynthCorrectsInvalidCsl],
        ['synth-call-fails',                testSynthCallFails],
        ['ingest-pipeline-mock',            testIngestPipelineMockMode],
        ['remember-pipeline-mock',          testRememberPipelineMockMode],
        ['remember-rejects-invalid',        testRememberRejectsInvalidCsl],
        ['retrieve-pipeline-mock',          testRetrievePipelineMockMode],
        ['deterministic-msg-id',            testDeterministicMsgId],
        ['merge-candidates',                testMergeCandidates],
        ['temporal-facts',                  testTemporalFacts],
        ['health-route-200',                testHealthRoute200],
        ['health-route-422',                testHealthRoute422OnBadProfile],
        ['smoke-route',                     testSmokeRoute],
        ['ingest-route-405',                testIngestRoute405OnGet],
        ['ingest-route-400',                testIngestRoute400OnBadBody],
        ['recall-route-400',                testRecallRoute400EmptyQuery],
        ['remember-route-400',              testRememberRoute400EmptyCsl],
        ['list-route-200-stub',             testListRoute200StubMode],
        ['forget-route-400',                testForgetRoute400OnBadUuid],
        ['export-route-200-stub',           testExportRoute200StubMode],
    ];
    let passed = 0, failed = 0;
    for (const [name, fn] of tests) {
        try {
            await fn();
            passed++;
        } catch (e) {
            failed++;
            // eslint-disable-next-line no-console
            console.error(`  FAIL ${name}: ${e instanceof Error ? e.message : String(e)}`);
        }
    }
    // eslint-disable-next-line no-console
    console.log(`mneme.test : ${passed}/${passed + failed} passed${failed ? ` · ${failed} failed` : ''}`);
    if (failed > 0) {
        process.exit(1);
    }
}

if (isMain) {
    runAll().catch(e => {
        // eslint-disable-next-line no-console
        console.error('runAll threw:', e);
        process.exit(1);
    });
}
