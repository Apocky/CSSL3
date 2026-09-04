import type { NextApiRequest, NextApiResponse } from 'next';

import { getAdminAuthorization, type RequestUser } from '../admin-auth';
import { envelope } from '../response';

export interface BrainOwnerBinding {
  readonly ok: true;
  readonly user: RequestUser;
}

export interface BrainOwnerFailure {
  readonly ok: false;
  readonly status: 401 | 403 | 503;
  readonly code: 'BRAIN_SESSION_REQUIRED' | 'BRAIN_OWNER_REQUIRED' | 'BRAIN_SESSION_UNAVAILABLE';
  readonly message: string;
}

export type BrainOwnerDecision = BrainOwnerBinding | BrainOwnerFailure;

export function setBrainPrivateHeaders(res: NextApiResponse): void {
  res.setHeader('Cache-Control', 'private, no-store, no-cache, must-revalidate, max-age=0');
  res.setHeader('Pragma', 'no-cache');
  res.setHeader('Vary', 'Authorization, Cookie, Origin');
  res.setHeader('Referrer-Policy', 'no-referrer');
  res.setHeader('X-Content-Type-Options', 'nosniff');
  res.setHeader('X-Frame-Options', 'DENY');
  res.setHeader('X-Robots-Tag', 'noindex, nofollow, noarchive, nosnippet');
}

export async function requireBrainOwner(req: NextApiRequest): Promise<BrainOwnerDecision> {
  const authorization = await getAdminAuthorization(req);
  if (authorization.authorized && authorization.user) {
    return { ok: true, user: authorization.user };
  }
  if (!authorization.user) {
    const unavailable = authorization.failureKind === 'unconfigured'
      || authorization.failureKind === 'upstream-unavailable';
    return unavailable
      ? {
          ok: false,
          status: 503,
          code: 'BRAIN_SESSION_UNAVAILABLE',
          message: 'The owner session could not be verified. No private Brain data was loaded.',
        }
      : {
          ok: false,
          status: 401,
          code: 'BRAIN_SESSION_REQUIRED',
          message: 'Sign in with the owner account to open the private Brain.',
        };
  }
  return {
    ok: false,
    status: 403,
    code: 'BRAIN_OWNER_REQUIRED',
    message: 'This private Brain is available only to its owner.',
  };
}

export function respondBrainOwnerFailure(res: NextApiResponse, failure: BrainOwnerFailure): void {
  res.status(failure.status).json({
    error: failure.message,
    code: failure.code,
    ...envelope(),
  });
}
