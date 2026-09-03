// cssl-edge · GET /api/mneme/[profile]/export
// MNEME — full data dump (your data is yours).
//
// Spec : ../../../../specs/43_MNEME.csl § OPS.export
//
// REQUEST  GET
// RESPONSE 200 { ok, profile, memories, messages, served_by, ts }

import type { NextApiRequest, NextApiResponse } from 'next';
import { envelope, logHit } from '@/lib/response';
import { getMnemeClient, exportProfile, memoryToPublic } from '@/lib/mneme/store';
import { maskToHex } from '@/lib/mneme/sigma';
import type { ExportResponse } from '@/lib/mneme/types';
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

export default async function handler(
    req: NextApiRequest,
    res: NextApiResponse<ExportResponse | ErrorResponse>,
): Promise<void> {
    logHit('mneme.export', { method: req.method ?? 'GET' });
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

    const sb = getMnemeClient();
    const storageFailure = await requireStoredMnemeProfile(sb, profile_id);
    if (storageFailure) {
        respondMnemeMemberFailure(res, storageFailure);
        return;
    }
    const client = sb!;
    try {
        const env = envelope();
        const out = await exportProfile(client, profile_id);
        // Render byte fields as portable hex rather than JSON object-shaped Uint8Arrays.
        const { sovereign_pk, sigma_mask, ...profileFields } = out.profile;
        const profile: ExportResponse['profile'] = {
            ...profileFields,
            sovereign_pk_hex: maskToHex(sovereign_pk),
            sigma_mask_hex: maskToHex(sigma_mask),
        };
        const messages = out.messages.map(m => ({
            id: m.id, profile_id: m.profile_id, session_id: m.session_id,
            role: m.role, content: m.content, ts: m.ts,
        }));
        res.status(200).json({
            ok: true,
            profile,
            memories: out.memories.map(memoryToPublic),
            messages,
            served_by: env.served_by, ts: env.ts,
        });
        // Side-effect: not awaited because we already responded.
        // (audit log of export is performed in store.exportProfile? — emit here)
        // eslint-disable-next-line no-console
        console.log(JSON.stringify({
            evt: 'mneme.export',
            memories: out.memories.length, messages: out.messages.length,
        }));
    } catch (e) {
        const env = envelope();
        // eslint-disable-next-line no-console
        console.error(JSON.stringify({ evt: 'mneme.export.fail', code: e instanceof Error ? e.name : 'UNKNOWN' }));
        res.status(502).json({ error: 'Private memory export failed. Retry without changing your data.', code: 'MNEME_EXPORT_FAILED', served_by: env.served_by, ts: env.ts });
    }
}
