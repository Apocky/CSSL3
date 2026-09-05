import type { NextApiRequest, NextApiResponse } from 'next';

import { getAccessTokenFromRequest } from '@/lib/admin-auth';
import {
  hasFreshInteractiveAuthenticationSince,
  jwtSessionClaims,
  verifyAuthAttempt,
} from '@/lib/auth-fence';
import { hasSameOrigin } from '@/lib/auth-session';
import {
  requireBrainOwner,
  respondBrainOwnerFailure,
  setBrainPrivateHeaders,
  type BrainOwnerDecision,
} from '@/lib/brain/owner';
import { miniBrainOwnerRef } from '@/lib/brain/mobile-relay';
import { envelope } from '@/lib/response';

const LOCK_GENERATION_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/iu;

function exactBody(value: unknown): value is { lock_generation: string; auth_attempt: string } {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return false;
  const row = value as Record<string, unknown>;
  return Object.keys(row).length === 2
    && typeof row.lock_generation === 'string'
    && LOCK_GENERATION_RE.test(row.lock_generation)
    && typeof row.auth_attempt === 'string'
    && row.auth_attempt.length >= 80
    && row.auth_attempt.length <= 8_192;
}

function testAuthBypass(req: NextApiRequest): boolean {
  return process.env.NODE_ENV !== 'production'
    && process.env.LAZARUS_TEST_AUTH_BYPASS === '1'
    && Boolean(req.headers['x-apocky-test-admin-email']);
}

export function createMiniBrainUnlockHandler(dependencies: {
  readonly requireOwner?: (request: NextApiRequest) => Promise<BrainOwnerDecision>;
  readonly accessToken?: (request: NextApiRequest) => string | null;
  readonly now?: () => number;
  readonly production?: boolean;
} = {}) {
return async function handler(req: NextApiRequest, res: NextApiResponse): Promise<void> {
  setBrainPrivateHeaders(res);
  res.setHeader('Allow', 'POST');
  if (req.method !== 'POST') {
    res.status(405).json({ error: 'Method not allowed', code: 'BRAIN_METHOD_NOT_ALLOWED', ...envelope() });
    return;
  }
  if (!hasSameOrigin(req)) {
    res.status(403).json({ error: 'Same-origin request required', code: 'BRAIN_ORIGIN_DENIED', ...envelope() });
    return;
  }
  if (!exactBody(req.body)) {
    res.status(400).json({ error: 'Rebind proof body is invalid.', code: 'BRAIN_REBIND_BODY_INVALID', ...envelope() });
    return;
  }
  const owner = await (dependencies.requireOwner ?? requireBrainOwner)(req);
  if (!owner.ok) {
    respondBrainOwnerFailure(res, owner);
    return;
  }
  if (!testAuthBypass(req)) {
    const nowMs = dependencies.now?.() ?? Date.now();
    const token = (dependencies.accessToken ?? getAccessTokenFromRequest)(req);
    const claims = token ? jwtSessionClaims(token, nowMs) : null;
    let attempt = null;
    try {
      attempt = verifyAuthAttempt({
        req,
        ticket: req.body.auth_attempt,
        mode: 'fresh',
        nowMs,
        production: dependencies.production,
      });
    } catch {
      res.status(503).json({ error: 'The reauthentication boundary is unavailable.', code: 'BRAIN_REAUTH_UNAVAILABLE', ...envelope() });
      return;
    }
    if (
      !claims
      || claims.subject !== owner.user.id
      || !attempt
      || !hasFreshInteractiveAuthenticationSince(claims, attempt.issued_at_ms, nowMs)
    ) {
      res.status(403).json({ error: 'Fresh interactive owner authentication is required.', code: 'BRAIN_FRESH_REAUTH_REQUIRED', ...envelope() });
      return;
    }
  }
  res.status(200).json({
    schema_version: 'apocky.mini-brain.owner-rebind.v1',
    status: 'rebind_authorized',
    owner_ref: miniBrainOwnerRef(owner.user.id),
    lock_generation: req.body.lock_generation,
    ...envelope(),
  });
};
}

export default createMiniBrainUnlockHandler();
