import { createHash } from 'node:crypto';

import type { NextApiRequest, NextApiResponse } from 'next';
import type { SupabaseClient } from '@supabase/supabase-js';

import { getRequestUser, type RequestUser } from '../admin-auth';
import { hasSameOrigin } from '../auth-session';
import { envelope } from '../response';
import { getProfile } from './store';

export const MNEME_MEMBER_ROUTE_PROFILE = 'me';

export interface MnemeMemberBinding {
  readonly ok: true;
  readonly profileId: string;
  readonly user: RequestUser;
}

export interface MnemeMemberFailure {
  readonly ok: false;
  readonly status: 401 | 403 | 404 | 409 | 503;
  readonly code:
    | 'MNEME_PROFILE_ROUTE_DENIED'
    | 'MNEME_SESSION_REQUIRED'
    | 'MNEME_SESSION_UNAVAILABLE'
    | 'MNEME_ORIGIN_DENIED'
    | 'MNEME_STORAGE_UNAVAILABLE'
    | 'MNEME_PROFILE_NOT_PROVISIONED';
  readonly message: string;
}

export type MnemeMemberBindingResult = MnemeMemberBinding | MnemeMemberFailure;

export interface MnemePrivateErrorResponse {
  readonly error: string;
  readonly code: MnemeMemberFailure['code'];
  readonly served_by: string;
  readonly ts: string;
}

/**
 * Stable, opaque profile namespace derived only after the server validates the
 * current user. The public route never accepts this identifier from a caller.
 */
export function deriveMemberProfileId(userId: string): string {
  const digest = createHash('sha256')
    .update('apocky.mneme.member-profile.v1\0', 'utf8')
    .update(userId, 'utf8')
    .digest('hex');
  return `member-${digest.slice(0, 40)}`;
}

export function setMnemePrivateHeaders(res: NextApiResponse): void {
  res.setHeader('Cache-Control', 'private, no-store, no-cache, must-revalidate, max-age=0');
  res.setHeader('Pragma', 'no-cache');
  res.setHeader('Vary', 'Authorization, Cookie, Origin');
  res.setHeader('Referrer-Policy', 'no-referrer');
  res.setHeader('X-Content-Type-Options', 'nosniff');
  res.setHeader('X-Frame-Options', 'DENY');
  res.setHeader('X-Robots-Tag', 'noindex, nofollow, noarchive, nosnippet');
}

export async function requireMnemeMemberProfile(req: NextApiRequest): Promise<MnemeMemberBindingResult> {
  const requested = Array.isArray(req.query.profile) ? req.query.profile[0] : req.query.profile;
  if (requested !== MNEME_MEMBER_ROUTE_PROFILE) {
    return {
      ok: false,
      status: 404,
      code: 'MNEME_PROFILE_ROUTE_DENIED',
      message: 'Private memory profiles are not addressable by name. Use the signed-in /me route.',
    };
  }

  if (req.method !== 'GET' && !hasSameOrigin(req)) {
    return {
      ok: false,
      status: 403,
      code: 'MNEME_ORIGIN_DENIED',
      message: 'Private memory changes require a same-origin request from apocky.com.',
    };
  }

  const session = await getRequestUser(req);
  if (!session.user) {
    const unavailable = session.failureKind === 'unconfigured' || session.failureKind === 'upstream-unavailable';
    return {
      ok: false,
      status: unavailable ? 503 : 401,
      code: unavailable ? 'MNEME_SESSION_UNAVAILABLE' : 'MNEME_SESSION_REQUIRED',
      message: unavailable
        ? 'The sign-in service could not verify this memory profile. Retry before changing private memory.'
        : 'Sign in to open your private Mneme profile.',
    };
  }

  return {
    ok: true,
    profileId: deriveMemberProfileId(session.user.id),
    user: session.user,
  };
}

/** Never let a public member route fall through to Mneme's local mock mode. */
export async function requireStoredMnemeProfile(
  client: SupabaseClient | null,
  profileId: string,
): Promise<MnemeMemberFailure | null> {
  if (!client) {
    return {
      ok: false,
      status: 503,
      code: 'MNEME_STORAGE_UNAVAILABLE',
      message: 'Private memory storage is not connected. No memory was read or changed.',
    };
  }
  try {
    if (await getProfile(client, profileId)) return null;
    return {
      ok: false,
      status: 409,
      code: 'MNEME_PROFILE_NOT_PROVISIONED',
      message: 'This verified account does not have a provisioned Mneme profile. No profile was created automatically.',
    };
  } catch {
    return {
      ok: false,
      status: 503,
      code: 'MNEME_STORAGE_UNAVAILABLE',
      message: 'Private memory storage could not verify this profile. No memory was read or changed.',
    };
  }
}

export function respondMnemeMemberFailure(
  res: NextApiResponse,
  failure: MnemeMemberFailure,
): void {
  const env = envelope();
  res.status(failure.status).json({
    error: failure.message,
    code: failure.code,
    served_by: env.served_by,
    ts: env.ts,
  } satisfies MnemePrivateErrorResponse);
}
