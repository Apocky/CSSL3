// cssl-edge · POST /api/mneme/[profile]/remember
// MNEME — single-shot remember (skip extraction, run validate+classify+embed only).
//
// Spec : ../../../../specs/43_MNEME.csl § OPS.remember
//
// REQUEST  POST { csl, paraphrase?, type?, topic_key?, sigma_mask_hex? }
// RESPONSE 200  { ok, memory, served_by, ts }

import type { NextApiRequest, NextApiResponse } from 'next';
import { envelope, logHit } from '@/lib/response';
import { getMnemeClient, memoryToPublic } from '@/lib/mneme/store';
import { rememberPipeline } from '@/lib/mneme/pipeline-ingest';
import { maskFromHex } from '@/lib/mneme/sigma';
import type {
    RememberRequest,
    RememberResponse,
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
    res: NextApiResponse<RememberResponse | ErrorResponse>,
): Promise<void> {
    logHit('mneme.remember', { method: req.method ?? 'GET' });

    if (req.method !== 'POST') {
        const env = envelope();
        res.setHeader('Allow', 'POST');
        res.status(405).json({
            error: 'Method Not Allowed — POST { csl, paraphrase?, type?, topic_key?, sigma_mask_hex? }',
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
    const reqBody = body as Partial<RememberRequest>;
    if (typeof reqBody.csl !== 'string' || reqBody.csl.trim().length === 0) {
        const env = envelope();
        res.status(400).json({
            error: 'csl is required',
            served_by: env.served_by, ts: env.ts,
        });
        return;
    }
    let type: MemoryType | undefined;
    if (typeof reqBody.type === 'string') {
        if (!TYPES.includes(reqBody.type as MemoryType)) {
            const env = envelope();
            res.status(400).json({
                error: `type must be one of ${TYPES.join('|')}`,
                served_by: env.served_by, ts: env.ts,
            });
            return;
        }
        type = reqBody.type as MemoryType;
    }
    let sigma_mask: Uint8Array | undefined;
    if (typeof reqBody.sigma_mask_hex === 'string' && reqBody.sigma_mask_hex.length > 0) {
        try {
            sigma_mask = maskFromHex(reqBody.sigma_mask_hex);
        } catch (e) {
            const env = envelope();
            res.status(400).json({
                error: `bad sigma_mask_hex: ${e instanceof Error ? e.message : String(e)}`,
                served_by: env.served_by, ts: env.ts,
            });
            return;
        }
    }

    const sb = getMnemeClient();
    try {
        const m = await rememberPipeline(sb, {
            profile_id,
            csl:        reqBody.csl,
            paraphrase: typeof reqBody.paraphrase === 'string' ? reqBody.paraphrase : undefined,
            type,
            topic_key:  typeof reqBody.topic_key === 'string' ? reqBody.topic_key : undefined,
            sigma_mask,
        });
        const env = envelope();
        res.status(200).json({
            ok:        true,
            memory:    memoryToPublic(m),
            served_by: env.served_by,
            ts:        env.ts,
        });
    } catch (e) {
        const env = envelope();
        const msg = e instanceof Error ? e.message : String(e);
        const status = e instanceof Error && /^csl invalid/i.test(msg) ? 400 : 502;
        // eslint-disable-next-line no-console
        console.error(JSON.stringify({ evt: 'mneme.remember.fail', err: msg }));
        res.status(status).json({ error: msg, served_by: env.served_by, ts: env.ts });
    }
}
