// cssl-edge · POST /api/mneme/[profile]/forget
// MNEME — sovereign-revoke a memory (sigma-mask flip + cascade).
//
// Spec : ../../../../specs/43_MNEME.csl § OPS.forget
//
// REQUEST  POST { memory_id, reason }
// RESPONSE 200 { ok, revoked, cascade, served_by, ts }

import type { NextApiRequest, NextApiResponse } from 'next';
import { envelope, logHit } from '@/lib/response';
import { getMnemeClient, forgetMemory } from '@/lib/mneme/store';
import type { ForgetResponse } from '@/lib/mneme/types';
import {
    requireMnemeMemberProfile,
    requireStoredMnemeProfile,
    respondMnemeMemberFailure,
    setMnemePrivateHeaders,
} from '@/lib/mneme/member-profile';

interface ErrorResponse {
    error:     string;
    code?:     string;
    served_by: string;
    ts:        string;
}

const UUID_RE    = /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

function isObject(b: unknown): b is Record<string, unknown> {
    return typeof b === 'object' && b !== null;
}

export default async function handler(
    req: NextApiRequest,
    res: NextApiResponse<ForgetResponse | ErrorResponse>,
): Promise<void> {
    logHit('mneme.forget', { method: req.method ?? 'GET' });
    setMnemePrivateHeaders(res);

    if (req.method !== 'POST') {
        const env = envelope();
        res.setHeader('Allow', 'POST');
        res.status(405).json({
            error: 'Method Not Allowed — POST { memory_id, reason }',
            served_by: env.served_by, ts: env.ts,
        });
        return;
    }

    const binding = await requireMnemeMemberProfile(req);
    if (!binding.ok) {
        respondMnemeMemberFailure(res, binding);
        return;
    }
    const profile_id = binding.profileId;

    const body: unknown = req.body;
    if (!isObject(body)) {
        const env = envelope();
        res.status(400).json({
            error: 'Bad Request — body must be JSON',
            served_by: env.served_by, ts: env.ts,
        });
        return;
    }
    const memory_id = body['memory_id'];
    const reason    = body['reason'];
    if (typeof memory_id !== 'string' || !UUID_RE.test(memory_id)) {
        const env = envelope();
        res.status(400).json({
            error: 'memory_id must be a UUID',
            served_by: env.served_by, ts: env.ts,
        });
        return;
    }
    if (typeof reason !== 'string' || reason.length === 0 || reason.length > 256) {
        const env = envelope();
        res.status(400).json({
            error: 'reason is required (1..256 chars)',
            served_by: env.served_by, ts: env.ts,
        });
        return;
    }

    const sb = getMnemeClient();
    const storageFailure = await requireStoredMnemeProfile(sb, profile_id);
    if (storageFailure) {
        respondMnemeMemberFailure(res, storageFailure);
        return;
    }
    const client = sb!;
    try {
        const env = envelope();
        const r = await forgetMemory(client, profile_id, memory_id, reason);
        res.status(200).json({
            ok: true,
            revoked: r.revoked,
            cascade: r.cascade,
            served_by: env.served_by, ts: env.ts,
        });
    } catch (e) {
        const env = envelope();
        // eslint-disable-next-line no-console
        console.error(JSON.stringify({ evt: 'mneme.forget.fail', code: e instanceof Error ? e.name : 'UNKNOWN' }));
        res.status(502).json({ error: 'Private forgetting could not complete. Retry before assuming the memory is inactive.', code: 'MNEME_FORGET_FAILED', served_by: env.served_by, ts: env.ts });
    }
}
