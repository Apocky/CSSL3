// cssl-edge · GET /api/mneme/[profile]/health
// MNEME — per-profile liveness ping. Always 200; reports config flags.

import type { NextApiRequest, NextApiResponse } from 'next';
import { envelope, logHit, commitSha } from '@/lib/response';

const PROFILE_RE = /^[a-z0-9-]{1,64}$/;

export interface MnemeHealthResponse {
    ok: true;
    sha: string;
    profile_id: string;
    served_by: string;
    ts: string;
    anthropic_configured: boolean;
    voyage_configured:    boolean;
    supabase_connected:   boolean;
    mneme_ready:          boolean;
}

interface ErrorResponse {
    error:     string;
    served_by: string;
    ts:        string;
}

function isSet(name: string): boolean {
    const v = process.env[name];
    return typeof v === 'string' && v.length > 0;
}

export default function handler(
    req: NextApiRequest,
    res: NextApiResponse<MnemeHealthResponse | ErrorResponse>,
): void {
    logHit('mneme.health', { method: req.method ?? 'GET' });

    const profile_id = String(req.query['profile'] ?? '');
    if (!PROFILE_RE.test(profile_id)) {
        const env = envelope();
        res.status(422).json({
            error: 'Invalid profile_id',
            served_by: env.served_by, ts: env.ts,
        });
        return;
    }
    const env = envelope();
    const anth = isSet('ANTHROPIC_API_KEY');
    const voy  = isSet('VOYAGE_API_KEY');
    const sup  = isSet('NEXT_PUBLIC_SUPABASE_URL') && isSet('SUPABASE_SERVICE_ROLE_KEY');
    res.status(200).json({
        ok: true,
        sha: commitSha(),
        profile_id,
        served_by: env.served_by, ts: env.ts,
        anthropic_configured: anth,
        voyage_configured:    voy,
        supabase_connected:   sup,
        mneme_ready:          anth && voy && sup,
    });
}
