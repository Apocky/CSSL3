// cssl-edge · GET /api/mneme/[profile]/list
// MNEME — list active memories with optional type filter + cursor pagination.
//
// Spec : ../../../../specs/43_MNEME.csl § OPS.list
//
// REQUEST  GET ?type=fact|event|instruction|task &limit=N &cursor=ISO
// RESPONSE 200 { ok, memories, next_cursor, served_by, ts }

import type { NextApiRequest, NextApiResponse } from 'next';
import { envelope, logHit } from '@/lib/response';
import { getMnemeClient, listMemories, memoryToPublic } from '@/lib/mneme/store';
import type { ListResponse, MemoryType } from '@/lib/mneme/types';

interface ErrorResponse {
    error:     string;
    served_by: string;
    ts:        string;
}

const PROFILE_RE = /^[a-z0-9-]{1,64}$/;
const TYPES: MemoryType[] = ['fact', 'event', 'instruction', 'task'];

export default async function handler(
    req: NextApiRequest,
    res: NextApiResponse<ListResponse | ErrorResponse>,
): Promise<void> {
    logHit('mneme.list', { method: req.method ?? 'GET' });

    if (req.method !== 'GET') {
        const env = envelope();
        res.setHeader('Allow', 'GET');
        res.status(405).json({
            error: 'Method Not Allowed — GET',
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
    const typeRaw = typeof req.query['type'] === 'string' ? req.query['type'] : undefined;
    const type = typeRaw && TYPES.includes(typeRaw as MemoryType) ? typeRaw as MemoryType : undefined;
    const limitRaw = typeof req.query['limit'] === 'string' ? parseInt(req.query['limit'], 10) : NaN;
    const limit = Number.isFinite(limitRaw) ? limitRaw : 50;
    const cursor = typeof req.query['cursor'] === 'string' ? req.query['cursor'] : undefined;

    const sb = getMnemeClient();
    try {
        const env = envelope();
        if (!sb) {
            // Mock mode — empty list with stable shape.
            res.status(200).json({
                ok: true,
                memories: [],
                next_cursor: null,
                served_by: env.served_by, ts: env.ts,
            });
            return;
        }
        const out = await listMemories(sb, profile_id, { type, limit, cursor });
        res.status(200).json({
            ok: true,
            memories: out.memories.map(memoryToPublic),
            next_cursor: out.next_cursor,
            served_by: env.served_by, ts: env.ts,
        });
    } catch (e) {
        const env = envelope();
        const msg = e instanceof Error ? e.message : String(e);
        // eslint-disable-next-line no-console
        console.error(JSON.stringify({ evt: 'mneme.list.fail', err: msg }));
        res.status(502).json({ error: msg, served_by: env.served_by, ts: env.ts });
    }
}
