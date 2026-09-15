// Sign in with an authenticator code. No email, no link, no template.
//
// The flow: the browser sends an address and a six-digit code; this verifies the code against the
// stored TOTP secret, and only then asks Supabase for a one-time token for that account and hands
// the token back. The browser exchanges it for a real session. Nothing is emailed, so none of the
// things that were broken — the mail template deciding whether a code appears, the link opening
// the system browser and stranding the session there — can happen.
//
// The token returned here IS a credential for the account, so every path that reaches the line
// which produces it has to have proved possession of the authenticator first.

import type { NextApiRequest, NextApiResponse } from 'next';

import { getApocryphaServiceClient } from '@/lib/apocrypha/job-control';
import { hasSameOrigin } from '@/lib/auth-session';
import { verifyTotp } from '@/lib/auth-totp';

const RATE_WINDOW_MS = 60_000;
const RATE_MAX_ATTEMPTS = 8;
const MAX_BUCKETS = 5_000;
const buckets = new Map<string, { count: number; resetAt: number }>();

export const config = { api: { bodyParser: { sizeLimit: '8kb' } } };

/**
 * One answer for every failure.
 *
 * Distinguishing "no such account" from "wrong code" turns this endpoint into a way to discover
 * who has an account here, and distinguishing "not enrolled" from "wrong code" tells an attacker
 * which accounts are worth attacking. The lockout is the only state worth revealing, because a
 * person who is locked out needs to know to stop trying.
 */
function deny(res: NextApiResponse, locked = false): void {
  res.status(locked ? 429 : 401).json({
    ok: false,
    code: locked ? 'TOTP_LOCKED' : 'TOTP_REJECTED',
    error: locked
      ? 'Too many incorrect codes. Try again in about 15 minutes.'
      : 'That code was not accepted. Check your authenticator and try the current code.',
  });
}

function throttled(key: string, now = Date.now()): boolean {
  const bucket = buckets.get(key);
  if (!bucket || bucket.resetAt <= now) {
    if (buckets.size >= MAX_BUCKETS) {
      for (const [id, value] of buckets) if (value.resetAt <= now) buckets.delete(id);
      if (buckets.size >= MAX_BUCKETS) {
        const oldest = buckets.keys().next().value as string | undefined;
        if (oldest) buckets.delete(oldest);
      }
    }
    buckets.set(key, { count: 1, resetAt: now + RATE_WINDOW_MS });
    return false;
  }
  bucket.count += 1;
  return bucket.count > RATE_MAX_ATTEMPTS;
}

function clientKey(req: NextApiRequest): string {
  const forwarded = req.headers['x-forwarded-for'];
  const first = Array.isArray(forwarded) ? forwarded[0] : forwarded;
  return (first ?? req.socket.remoteAddress ?? 'unknown').split(',')[0]!.trim();
}

export default async function handler(req: NextApiRequest, res: NextApiResponse): Promise<void> {
  res.setHeader('Cache-Control', 'private, no-store, max-age=0');
  res.setHeader('Vary', 'Origin, Cookie');
  res.setHeader('X-Content-Type-Options', 'nosniff');

  if (req.method !== 'POST') {
    res.setHeader('Allow', 'POST');
    res.status(405).json({ ok: false, code: 'METHOD_NOT_ALLOWED' });
    return;
  }
  if (!hasSameOrigin(req)) {
    res.status(403).json({ ok: false, code: 'ORIGIN_REQUIRED' });
    return;
  }

  const body = (req.body ?? {}) as Record<string, unknown>;
  const email = typeof body.email === 'string' ? body.email.trim().toLowerCase() : '';
  const code = typeof body.code === 'string' ? body.code.replace(/\D/gu, '') : '';
  if (email === '' || email.length > 320 || code.length !== 6) {
    deny(res);
    return;
  }

  // Per-IP throttle on top of the per-account lockout. The account lockout alone lets one attacker
  // spread attempts across many accounts; this bounds the total rate from one source.
  if (throttled(`${clientKey(req)}`)) {
    deny(res, true);
    return;
  }

  let service;
  try {
    service = getApocryphaServiceClient();
  } catch {
    res.status(503).json({ ok: false, code: 'AUTH_UNCONFIGURED', error: 'Authenticator sign-in is not available here.' });
    return;
  }

  try {
    const { data: userRows, error: lookupError } = await service
      .schema('public')
      .rpc('apocky_totp_user_by_email', { p_email: email });
    if (lookupError) { deny(res); return; }
    const userId = Array.isArray(userRows) && userRows.length > 0
      ? (userRows[0] as { user_id?: string }).user_id
      : null;
    if (!userId) { deny(res); return; }

    const { data: beginRows, error: beginError } = await service.rpc('apocky_totp_begin', { p_user_id: userId });
    if (beginError) {
      // P4291 is the lockout; anything else is "no confirmed authenticator", which must look
      // exactly like a wrong code.
      deny(res, beginError.code === 'P4291');
      return;
    }
    const factor = Array.isArray(beginRows) ? beginRows[0] as { secret: string; last_used_step: string | number | null } : null;
    if (!factor?.secret) { deny(res); return; }

    const lastUsedStep = factor.last_used_step === null || factor.last_used_step === undefined
      ? null
      : Number(factor.last_used_step);
    const verdict = verifyTotp(factor.secret, code, { lastUsedStep });
    if (!verdict.ok || verdict.step === null) {
      const { data: failRows } = await service.rpc('apocky_totp_fail', { p_user_id: userId });
      const locked = Array.isArray(failRows) && failRows[0]
        ? Boolean((failRows[0] as { locked_until: string | null }).locked_until)
        : false;
      deny(res, locked);
      return;
    }

    // Spend the step BEFORE minting anything. If this write fails the request fails, because a
    // token handed out against a code that was never marked used is a replayable credential.
    const { error: spendError } = await service.rpc('apocky_totp_succeed', { p_user_id: userId, p_step: verdict.step });
    if (spendError) { deny(res); return; }

    const { data: link, error: linkError } = await service.auth.admin.generateLink({
      type: 'magiclink',
      email,
    });
    const hashedToken = link?.properties?.hashed_token;
    if (linkError || !hashedToken) {
      res.status(502).json({ ok: false, code: 'SESSION_UNAVAILABLE', error: 'The sign-in service could not issue a session.' });
      return;
    }

    res.status(200).json({ ok: true, token_hash: hashedToken });
  } catch {
    res.status(502).json({ ok: false, code: 'TOTP_UNAVAILABLE', error: 'Authenticator sign-in could not be completed.' });
  }
}
