import type { NextApiRequest, NextApiResponse } from 'next';

import { getAuthClient } from '../../../lib/auth';
import {
  AUTH_FENCE_PROTOCOL,
  authSessionBindingValid,
  hasFreshInteractiveAuthenticationSince,
  issueAuthSessionBinding,
  jwtSessionClaims,
  verifyAuthAttempt,
  type AuthAttemptMode,
} from '../../../lib/auth-fence';
import { bearerFromRequest, hasSameOrigin, jwtLifetimeSeconds, sessionCookies } from '../../../lib/auth-session';

type VerifySession = (token: string) => Promise<'valid' | 'invalid' | 'unavailable'>;

function first(value: string | string[] | undefined): string | null {
  const item = Array.isArray(value) ? value[0] : value;
  return item?.trim() || null;
}

function exactBody(value: unknown): value is { mode: AuthAttemptMode } {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return false;
  const row = value as Record<string, unknown>;
  return Object.keys(row).length === 1 && (row.mode === 'fresh' || row.mode === 'refresh');
}

async function defaultVerify(token: string): Promise<'valid' | 'invalid' | 'unavailable'> {
  const client = getAuthClient();
  if (!client) return 'unavailable';
  try {
    const result = await Promise.race([
      client.auth.getUser(token),
      new Promise<never>((_, reject) => setTimeout(() => reject(new Error('timeout')), 5000)),
    ]);
    if (!result.error && result.data.user) return 'valid';
    return result.error?.status === 401 || result.error?.status === 403 ? 'invalid' : 'unavailable';
  } catch {
    return 'unavailable';
  }
}

export function createSessionHandler(
  verify: VerifySession = defaultVerify,
  options: { production?: boolean; now?: () => number } = {},
) {
  return async function sessionHandler(req: NextApiRequest, res: NextApiResponse) {
    res.setHeader('Cache-Control', 'private, no-store, max-age=0');
    res.setHeader('Pragma', 'no-cache');
    res.setHeader('Vary', 'Origin, Cookie');

    if (req.method !== 'POST') {
      res.setHeader('Allow', 'POST');
      return res.status(405).json({ ok: false });
    }
    if (!hasSameOrigin(req)) return res.status(403).json({ ok: false });
    const protocol = first(req.headers['x-apocky-auth-protocol']);
    const attemptTicket = first(req.headers['x-apocky-auth-attempt']);
    if (protocol !== AUTH_FENCE_PROTOCOL || !attemptTicket || !exactBody(req.body)) {
      return res.status(428).json({ ok: false, code: 'AUTH_PROTOCOL_UPGRADE_REQUIRED' });
    }

    const token = bearerFromRequest(req);
    if (!token) return res.status(401).json({ ok: false });
    const maxAge = jwtLifetimeSeconds(token, options.now?.() ?? Date.now());
    if (!maxAge) return res.status(401).json({ ok: false });
    const nowMs = options.now?.() ?? Date.now();
    const claims = jwtSessionClaims(token, nowMs);
    if (!claims) return res.status(401).json({ ok: false, code: 'AUTH_SESSION_CLAIMS_INVALID' });
    let attempt;
    try {
      attempt = verifyAuthAttempt({
        req,
        ticket: attemptTicket,
        mode: req.body.mode,
        nowMs,
        production: options.production,
      });
    } catch {
      return res.status(503).json({ ok: false, code: 'AUTH_FENCE_UNAVAILABLE' });
    }
    if (!attempt) return res.status(409).json({ ok: false, code: 'AUTH_ATTEMPT_SUPERSEDED' });
    if (req.body.mode === 'fresh' && (
      !hasFreshInteractiveAuthenticationSince(claims, attempt.issued_at_ms, nowMs)
    )) {
      return res.status(409).json({ ok: false, code: 'AUTH_INTERACTIVE_REAUTH_REQUIRED' });
    }
    if (req.body.mode === 'refresh' && !authSessionBindingValid({
      req,
      claims,
      userId: claims.subject,
      nowMs,
      production: options.production,
    })) return res.status(401).json({ ok: false, code: 'AUTH_REFRESH_BINDING_INVALID' });

    const verdict = await verify(token);
    if (verdict === 'unavailable') return res.status(503).json({ ok: false });
    if (verdict !== 'valid') return res.status(401).json({ ok: false });

    let sessionBinding: string | null;
    try {
      sessionBinding = issueAuthSessionBinding({ req, claims, nowMs, production: options.production });
    } catch {
      return res.status(503).json({ ok: false, code: 'AUTH_FENCE_UNAVAILABLE' });
    }
    if (!sessionBinding) return res.status(409).json({ ok: false, code: 'AUTH_ATTEMPT_SUPERSEDED' });
    res.setHeader('Set-Cookie', sessionCookies(
      token,
      maxAge,
      options.production ?? process.env.NODE_ENV === 'production',
      sessionBinding,
    ));
    return res.status(200).json({ ok: true, expiresIn: maxAge });
  };
}

export default createSessionHandler();
