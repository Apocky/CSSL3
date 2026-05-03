// cssl-edge · POST /api/mneme/[profile]/recall
// MNEME — synthesis-driven recall over 6-channel retrieval.
//
// Spec : ../../../../specs/43_MNEME.csl § OPS.recall + 44_MNEME_PIPELINES.csl § RETRIEVE
//
// REQUEST  POST { query, k?, types?, audience_bits?, debug? }
// RESPONSE 200  { ok, result_nl, result_csl, citations, confidence, debug?, served_by, ts }

import type { NextApiRequest, NextApiResponse } from 'next';
import { envelope, logHit } from '@/lib/response';
import { getMnemeClient } from '@/lib/mneme/store';
import { retrievePipeline } from '@/lib/mneme/pipeline-retrieve';
import type {
    RecallRequest,
    RecallResponse,
    MemoryType,
} from '@/lib/mneme/types';

interface ErrorResponse {
    error:     string;
    served_by: string;
    ts:        string;
}

const PROFILE_RE = /^[a-z0-9-]{1,64}$/;
const TYPES: MemoryType[] = ['fact', 'event', 'instruction', 'task'];

function isObject(b: unknown): b is Record<string, unknown> {
    return typeof b === 'object' && b !== null;
}

export default async function handler(
    req: NextApiRequest,
    res: NextApiResponse<RecallResponse | ErrorResponse>,
): Promise<void> {
    logHit('mneme.recall', { method: req.method ?? 'GET' });

    if (req.method !== 'POST') {
        const env = envelope();
        res.setHeader('Allow', 'POST');
        res.status(405).json({
            error: 'Method Not Allowed — POST { query, k?, types?, debug? }',
            served_by: env.served_by, ts: env.ts,
        });
        return;
    }

    const profile_id = String(req.query['profile'] ?? '');
    if (!PROFILE_RE.test(profile_id)) {
        const env = envelope();
        res.status(422).json({
            error: 'Invalid profile_id',
            served_by: env.served_by, ts: env.ts,
        });
        return;
    }

    const body: unknown = req.body;
    if (!isObject(body)) {
        const env = envelope();
        res.status(400).json({
            error: 'Bad Request — body must be JSON',
            served_by: env.served_by, ts: env.ts,
        });
        return;
    }
    const reqBody = body as Partial<RecallRequest>;
    if (typeof reqBody.query !== 'string' || reqBody.query.trim().length === 0) {
        const env = envelope();
        res.status(400).json({
            error: 'query is required',
            served_by: env.served_by, ts: env.ts,
        });
        return;
    }
    if (reqBody.query.length > 512) {
        const env = envelope();
        res.status(400).json({
            error: 'query exceeds 512 chars',
            served_by: env.served_by, ts: env.ts,
        });
        return;
    }

    const k = typeof reqBody.k === 'number' ? Math.max(1, Math.min(20, reqBody.k)) : 5;
    const types = Array.isArray(reqBody.types)
        ? reqBody.types.filter((t): t is MemoryType => TYPES.includes(t as MemoryType))
        : undefined;
    const debug = reqBody.debug === true;

    const sb = getMnemeClient();
    try {
        const result = await retrievePipeline(sb, {
            profile_id,
            query: reqBody.query,
            k,
            types,
            debug,
        });
        const env = envelope();
        const responseBody: RecallResponse = {
            ok:         true,
            result_nl:  result.result_nl,
            result_csl: result.result_csl,
            citations:  result.citations,
            confidence: result.confidence,
            served_by:  env.served_by,
            ts:         env.ts,
        };
        if (result.debug) {
            responseBody.debug = result.debug;
        }
        res.status(200).json(responseBody);
    } catch (e) {
        const env = envelope();
        const msg = e instanceof Error ? e.message : String(e);
        // eslint-disable-next-line no-console
        console.error(JSON.stringify({ evt: 'mneme.recall.fail', err: msg }));
        res.status(502).json({ error: msg, served_by: env.served_by, ts: env.ts });
    }
}
