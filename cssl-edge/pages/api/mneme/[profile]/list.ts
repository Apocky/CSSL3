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

const TYPES: MemoryType[] = ['fact', 'event', 'instruction', 'task'];

export default async function handler(
    req: NextApiRequest,
    res: NextApiResponse<ListResponse | ErrorResponse>,
): Promise<void> {
    logHit('mneme.list', { method: req.method ?? 'GET' });
    setMnemePrivateHeaders(res);

    if (req.method !== 'GET') {
        const env = envelope();
        res.setHeader('Allow', 'GET');
        res.status(405).json({
            error: 'Method Not Allowed — GET',
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
    const typeRaw = typeof req.query['type'] === 'string' ? req.query['type'] : undefined;
    const type = typeRaw && TYPES.includes(typeRaw as MemoryType) ? typeRaw as MemoryType : undefined;
    const limitRaw = typeof req.query['limit'] === 'string' ? parseInt(req.query['limit'], 10) : NaN;
    const limit = Number.isFinite(limitRaw) ? limitRaw : 50;
    const cursor = typeof req.query['cursor'] === 'string' ? req.query['cursor'] : undefined;

    const sb = getMnemeClient();
    const storageFailure = await requireStoredMnemeProfile(sb, profile_id);
    if (storageFailure) {
        respondMnemeMemberFailure(res, storageFailure);
        return;
    }
    const client = sb!;
    try {
        const env = envelope();
        const out = await listMemories(client, profile_id, { type, limit, cursor });
        res.status(200).json({
            ok: true,
            memories: out.memories.map(memoryToPublic),
            next_cursor: out.next_cursor,
            served_by: env.served_by, ts: env.ts,
        });
    } catch (e) {
        const env = envelope();
        // eslint-disable-next-line no-console
        console.error(JSON.stringify({ evt: 'mneme.list.fail', code: e instanceof Error ? e.name : 'UNKNOWN' }));
        res.status(502).json({ error: 'Private memory could not be listed. Retry before making changes.', code: 'MNEME_LIST_FAILED', served_by: env.served_by, ts: env.ts });
    }
}
