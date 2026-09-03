// cssl-edge · GET /api/mneme/me/health
// MNEME — member-bound readiness probe. Reports config and profile readiness.

import type { NextApiRequest, NextApiResponse } from 'next';
import { envelope, logHit, commitSha } from '@/lib/response';
import { getMnemeClient, getProfile } from '@/lib/mneme/store';
import {
    requireMnemeMemberProfile,
    respondMnemeMemberFailure,
    setMnemePrivateHeaders,
    type MnemePrivateErrorResponse,
} from '@/lib/mneme/member-profile';

export interface MnemeHealthResponse {
    ok: true;
    sha: string;
    profile_id: string;
    served_by: string;
    ts: string;
    anthropic_configured: boolean;
    voyage_configured:    boolean;
    supabase_connected:   boolean;
    profile_ready:        boolean;
    storage_ready:        boolean;
    semantic_ready:       boolean;
    mneme_ready:          boolean;
}

interface MnemeHealthMethodError {
    error: string;
    code: 'MNEME_METHOD_NOT_ALLOWED';
    served_by: string;
    ts: string;
}

function isSet(name: string): boolean {
    const v = process.env[name];
    return typeof v === 'string' && v.length > 0;
}

export default async function handler(
    req: NextApiRequest,
    res: NextApiResponse<MnemeHealthResponse | MnemePrivateErrorResponse | MnemeHealthMethodError>,
): Promise<void> {
    logHit('mneme.health', { method: req.method ?? 'GET' });
    setMnemePrivateHeaders(res);

    if (req.method !== 'GET') {
        const env = envelope();
        res.setHeader('Allow', 'GET');
        res.status(405).json({
            error: 'Method Not Allowed — GET',
            code: 'MNEME_METHOD_NOT_ALLOWED',
            served_by: env.served_by,
            ts: env.ts,
        });
        return;
    }

    const binding = await requireMnemeMemberProfile(req);
    if (!binding.ok) {
        respondMnemeMemberFailure(res, binding);
        return;
    }

    const env = envelope();
    const anth = isSet('ANTHROPIC_API_KEY');
    const voy  = isSet('VOYAGE_API_KEY');
    const sup  = isSet('NEXT_PUBLIC_SUPABASE_URL') && isSet('SUPABASE_SERVICE_ROLE_KEY');
    const sb = getMnemeClient();
    let profileReady = false;
    if (sb) {
        try {
            profileReady = Boolean(await getProfile(sb, binding.profileId));
        } catch {
            profileReady = false;
        }
    }
    const storageReady = sup && profileReady;
    const semanticReady = storageReady && anth && voy;
    res.status(200).json({
        ok: true,
        sha: commitSha(),
        profile_id: binding.profileId,
        served_by: env.served_by, ts: env.ts,
        anthropic_configured: anth,
        voyage_configured:    voy,
        supabase_connected:   sup,
        profile_ready:        profileReady,
        storage_ready:        storageReady,
        semantic_ready:       semanticReady,
        mneme_ready:          semanticReady,
    });
}
