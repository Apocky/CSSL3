// cssl-edge · POST /api/mneme/[profile]/ingest
// MNEME — bulk ingestion of conversation messages.
//
// Spec : ../../../../specs/43_MNEME.csl § OPS.ingest + 44_MNEME_PIPELINES.csl § INGEST
//
// REQUEST  POST { session_id, messages: [{role, content}], sigma_mask_hex? }
// RESPONSE 200  { ok, stored, deduped, extracted, dropped, served_by, ts }

import type { NextApiRequest, NextApiResponse } from 'next';
import { envelope, logHit } from '@/lib/response';
import { getMnemeClient } from '@/lib/mneme/store';
import { ingestPipeline } from '@/lib/mneme/pipeline-ingest';
import { maskFromHex } from '@/lib/mneme/sigma';
import type {
    IngestRequest,
    IngestResponse,
    Role,
} from '@/lib/mneme/types';

interface ErrorResponse {
    error:     string;
    served_by: string;
    ts:        string;
}

const PROFILE_RE = /^[a-z0-9-]{1,64}$/;
const ROLES: Role[] = ['user', 'assistant', 'system', 'tool'];
const MAX_MESSAGES = 200;

function isObject(b: unknown): b is Record<string, unknown> {
    return typeof b === 'object' && b !== null;
}

function isMessageInputArray(v: unknown): v is Array<{ role: Role; content: string }> {
    if (!Array.isArray(v)) return false;
    if (v.length === 0 || v.length > MAX_MESSAGES) return false;
    return v.every(m =>
        isObject(m) &&
        ROLES.includes(m['role'] as Role) &&
        typeof m['content'] === 'string' &&
        (m['content'] as string).length > 0,
    );
}

export default async function handler(
    req: NextApiRequest,
    res: NextApiResponse<IngestResponse | ErrorResponse>,
): Promise<void> {
    logHit('mneme.ingest', { method: req.method ?? 'GET' });

    if (req.method !== 'POST') {
        const env = envelope();
        res.setHeader('Allow', 'POST');
        res.status(405).json({
            error: 'Method Not Allowed — POST { session_id, messages, sigma_mask_hex? }',
            served_by: env.served_by, ts: env.ts,
        });
        return;
    }

    const profile_id = String(req.query['profile'] ?? '');
    if (!PROFILE_RE.test(profile_id)) {
        const env = envelope();
        res.status(422).json({
            error: 'Invalid profile_id (must match [a-z0-9-]{1,64})',
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
    const reqBody = body as Partial<IngestRequest>;
    if (typeof reqBody.session_id !== 'string' || reqBody.session_id.length === 0) {
        const env = envelope();
        res.status(400).json({
            error: 'session_id is required',
            served_by: env.served_by, ts: env.ts,
        });
        return;
    }
    if (!isMessageInputArray(reqBody.messages)) {
        const env = envelope();
        res.status(400).json({
            error: `messages must be Array<{role, content}> length 1..${MAX_MESSAGES}`,
            served_by: env.served_by, ts: env.ts,
        });
        return;
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
        const result = await ingestPipeline(sb, {
            profile_id,
            session_id: reqBody.session_id,
            messages:   reqBody.messages,
            sigma_mask,
        });
        const env = envelope();
        res.status(200).json({
            ok:         true,
            stored:     result.stored,
            deduped:    result.deduped,
            extracted:  result.extracted,
            dropped:    result.dropped,
            profile_id,
            session_id: reqBody.session_id,
            served_by:  env.served_by,
            ts:         env.ts,
        });
    } catch (e) {
        const env = envelope();
        const msg = e instanceof Error ? e.message : String(e);
        // eslint-disable-next-line no-console
        console.error(JSON.stringify({ evt: 'mneme.ingest.fail', err: msg }));
        res.status(502).json({ error: msg, served_by: env.served_by, ts: env.ts });
    }
}
