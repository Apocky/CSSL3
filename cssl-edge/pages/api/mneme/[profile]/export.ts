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
import type { ExportResponse, Profile } from '@/lib/mneme/types';

interface ErrorResponse {
    error:     string;
    served_by: string;
    ts:        string;
}

const PROFILE_RE = /^[a-z0-9-]{1,64}$/;

export default async function handler(
    req: NextApiRequest,
    res: NextApiResponse<ExportResponse | ErrorResponse>,
): Promise<void> {
    logHit('mneme.export', { method: req.method ?? 'GET' });

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

    const sb = getMnemeClient();
    try {
        const env = envelope();
        if (!sb) {
            // Mock mode → empty bundle so client tools can drive flows.
            const stubProfile: Profile = {
                profile_id,
                sovereign_pk: new Uint8Array(32),
                sigma_mask:   new Uint8Array(32),
                created_at:   env.ts,
                memory_count: 0, message_count: 0,
                meta: { stub: true },
            };
            res.status(200).json({
                ok: true,
                profile:  stubProfile,
                memories: [],
                messages: [],
                served_by: env.served_by, ts: env.ts,
            });
            return;
        }
        const out = await exportProfile(sb, profile_id);
        // Render the profile + messages with hex masks (avoid raw Uint8Array on the wire).
        const profile: Profile = {
            ...out.profile,
            // Already typed Uint8Array — JSON.stringify will emit objects unless we coerce.
            sovereign_pk: out.profile.sovereign_pk,
            sigma_mask:   out.profile.sigma_mask,
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
            evt: 'mneme.export', profile_id,
            memories: out.memories.length, messages: out.messages.length,
            mask_hex: maskToHex(out.profile.sigma_mask),
        }));
    } catch (e) {
        const env = envelope();
        const msg = e instanceof Error ? e.message : String(e);
        const status = /not found/i.test(msg) ? 404 : 502;
        // eslint-disable-next-line no-console
        console.error(JSON.stringify({ evt: 'mneme.export.fail', err: msg }));
        res.status(status).json({ error: msg, served_by: env.served_by, ts: env.ts });
    }
}
