import { createHash, timingSafeEqual } from 'node:crypto';

import type { NextApiRequest, NextApiResponse } from 'next';

import { getAdminAuthorization } from '@/lib/admin-auth';
import { envelope } from '@/lib/response';

export const MNEME_CAPABILITIES = {
    health:   'mneme.health',
    smoke:    'mneme.smoke',
    ingest:   'mneme.ingest',
    remember: 'mneme.remember',
    recall:   'mneme.recall',
    list:     'mneme.list',
    export:   'mneme.export',
    forget:   'mneme.forget',
} as const;

export type MnemeCapability = typeof MNEME_CAPABILITIES[keyof typeof MNEME_CAPABILITIES];

export interface MnemeAccess {
    actor: 'owner' | 'service';
    capability: MnemeCapability;
    profileId: string;
}

interface MnemeAuthError {
    error: string;
    code: 'MNEME_AUTH_REQUIRED' | 'MNEME_AUTH_FORBIDDEN' | 'MNEME_AUTH_UNCONFIGURED' | 'MNEME_PROFILE_INVALID';
    served_by: string;
    ts: string;
}

const PROFILE_RE = /^[a-z0-9-]{1,64}$/;
const SERVICE_TOKEN_MIN_BYTES = 32;
const SERVICE_TOKEN_MAX_BYTES = 512;

function firstHeader(value: string | string[] | undefined): string | null {
    const first = Array.isArray(value) ? value[0] : value;
    return first?.trim() || null;
}

function bearerToken(value: string | string[] | undefined): string | null {
    const header = firstHeader(value);
    if (!header?.startsWith('Bearer ')) return null;
    const token = header.slice('Bearer '.length).trim();
    return token || null;
}

function secretMatches(presented: string | null, expected: string | undefined): boolean {
    if (!presented || !expected) return false;
    const observed = createHash('sha256').update(presented).digest();
    const wanted = createHash('sha256').update(expected).digest();
    return timingSafeEqual(observed, wanted);
}

function validConfiguredProfile(name: 'MNEME_OWNER_PROFILE_ID' | 'MNEME_SERVICE_PROFILE_ID'): string | null {
    const value = process.env[name]?.trim() ?? '';
    return PROFILE_RE.test(value) ? value : null;
}

function configuredServiceToken(): string | null {
    const token = process.env.MNEME_SERVICE_TOKEN?.trim() ?? '';
    const bytes = Buffer.byteLength(token, 'utf8');
    return bytes >= SERVICE_TOKEN_MIN_BYTES && bytes <= SERVICE_TOKEN_MAX_BYTES ? token : null;
}

function configuredServiceCapabilities(): ReadonlySet<string> | null {
    const raw = process.env.MNEME_SERVICE_CAPABILITIES?.trim();
    if (!raw) return null;
    const capabilities = raw.split(',').map((value) => value.trim()).filter(Boolean);
    if (capabilities.length === 0 || capabilities.includes('*')) return null;
    if (capabilities.some((value) => !Object.values(MNEME_CAPABILITIES).includes(value as MnemeCapability))) {
        return null;
    }
    return new Set(capabilities);
}

function reject<T>(
    res: NextApiResponse<T>,
    status: number,
    code: MnemeAuthError['code'],
    error: string,
): null {
    const env = envelope();
    res.status(status).json({ error, code, served_by: env.served_by, ts: env.ts } as T);
    return null;
}

/**
 * Default-deny authorization for every MNEME profile route.
 *
 * Owner requests require a valid Apocky owner session and may access only the
 * exact `MNEME_OWNER_PROFILE_ID`. Service requests require the exact bearer,
 * route profile, declared header capability, and closed capability allowlist.
 * This function performs no MNEME store or model-provider construction.
 */
export async function requireMnemeProfileAccess<T>(
    req: NextApiRequest,
    res: NextApiResponse<T>,
    capability: MnemeCapability,
): Promise<MnemeAccess | null> {
    res.setHeader('Cache-Control', 'private, no-store, max-age=0');

    const profileId = String(req.query['profile'] ?? '');
    if (!PROFILE_RE.test(profileId)) {
        return reject(res, 422, 'MNEME_PROFILE_INVALID', 'Invalid profile_id');
    }

    const presentedToken = bearerToken(req.headers.authorization);
    const rawServiceToken = process.env.MNEME_SERVICE_TOKEN?.trim();
    const serviceToken = configuredServiceToken();
    const presentedCapability = firstHeader(req.headers['x-mneme-capability']);
    const presentedProfile = firstHeader(req.headers['x-mneme-profile']);
    const serviceIntent = Boolean(presentedCapability || presentedProfile)
        || secretMatches(presentedToken, rawServiceToken);

    if (serviceIntent) {
        const serviceProfile = validConfiguredProfile('MNEME_SERVICE_PROFILE_ID');
        const allowedCapabilities = configuredServiceCapabilities();
        if (!serviceToken || !serviceProfile || !allowedCapabilities) {
            return reject(res, 503, 'MNEME_AUTH_UNCONFIGURED', 'MNEME service authorization is not configured.');
        }
        if (!secretMatches(presentedToken, serviceToken)) {
            return reject(res, 401, 'MNEME_AUTH_REQUIRED', 'MNEME service authentication failed.');
        }
        if (
            profileId !== serviceProfile
            || presentedProfile !== profileId
            || presentedCapability !== capability
            || !allowedCapabilities.has(capability)
        ) {
            return reject(res, 403, 'MNEME_AUTH_FORBIDDEN', 'MNEME service profile or capability is not authorized.');
        }
        return { actor: 'service', capability, profileId };
    }

    const owner = await getAdminAuthorization(req);
    if (!owner.authorized || !owner.user) {
        const status = owner.user ? 403 : 401;
        return reject(res, status, status === 401 ? 'MNEME_AUTH_REQUIRED' : 'MNEME_AUTH_FORBIDDEN',
            owner.reason ?? 'Apocky owner sign-in required.');
    }

    const ownerProfile = validConfiguredProfile('MNEME_OWNER_PROFILE_ID');
    if (!ownerProfile) {
        return reject(res, 503, 'MNEME_AUTH_UNCONFIGURED', 'MNEME owner profile authorization is not configured.');
    }
    if (profileId !== ownerProfile) {
        return reject(res, 403, 'MNEME_AUTH_FORBIDDEN', 'This owner session is not authorized for the requested MNEME profile.');
    }

    return { actor: 'owner', capability, profileId };
}
