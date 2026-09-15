// Set up an authenticator from inside the app.
//
// Requires a signed-in session: enrolment adds a credential to an account, so proving you are
// already that account is the whole gate. There is deliberately no "enrol without signing in" path
// — that would let anyone claim an account that has no authenticator yet.
//
// GET             -> status: whether this account already has a confirmed authenticator
// POST            -> start: mints a pending secret and returns the otpauth URI
// POST {code}     -> confirm: a correct code promotes the pending secret to the real one
//
// The new secret stays PENDING until a code proves the authenticator actually received it. Setting
// up and walking away must never replace a working authenticator with one nobody holds.

import type { NextApiRequest, NextApiResponse } from 'next';

import { getRequestUser } from '@/lib/admin-auth';
import { getApocryphaServiceClient } from '@/lib/apocrypha/job-control';
import { hasSameOrigin } from '@/lib/auth-session';
import QRCode from 'qrcode';

import { clockDriftSteps, describeDrift, generateSecret, provisioningUri, verifyTotp } from '@/lib/auth-totp';

export const config = { api: { bodyParser: { sizeLimit: '8kb' } } };

export default async function handler(req: NextApiRequest, res: NextApiResponse): Promise<void> {
  res.setHeader('Cache-Control', 'private, no-store, max-age=0');
  res.setHeader('Vary', 'Cookie');
  res.setHeader('X-Content-Type-Options', 'nosniff');

  if (req.method !== 'POST' && req.method !== 'GET') {
    res.setHeader('Allow', 'GET, POST');
    res.status(405).json({ ok: false, code: 'METHOD_NOT_ALLOWED' });
    return;
  }
  if (req.method === 'POST' && !hasSameOrigin(req)) {
    res.status(403).json({ ok: false, code: 'ORIGIN_REQUIRED' });
    return;
  }

  const session = await getRequestUser(req);
  if (!session.user) {
    res.status(401).json({ ok: false, code: 'SIGN_IN_REQUIRED', error: 'Sign in before adding an authenticator.' });
    return;
  }

  let service;
  try {
    service = getApocryphaServiceClient();
  } catch {
    res.status(503).json({ ok: false, code: 'AUTH_UNCONFIGURED' });
    return;
  }

  // ---- status ---------------------------------------------------------------------------------
  //
  // Read-only: does this account already have an authenticator? Asked by the sign-in flow, which
  // offers enrolment to people who have none rather than leaving them to find it later.
  //
  // This leans on apocky_totp_begin, which already answers the question by its error code —
  // P4041 for "no confirmed authenticator", P4291 for "enrolled but temporarily locked". It does
  // hand back the secret to do so, which is more than a boolean needs; the secret is discarded
  // here and never leaves the function. A dedicated boolean RPC would be tidier, and would cost a
  // migration to a database this process has no credentials for. The exposure is unchanged either
  // way: sign-in already reads the same secret through the same path on every attempt.
  if (req.method === 'GET') {
    try {
      const { error } = await service.rpc('apocky_totp_begin', { p_user_id: session.user.id });
      const code = (error as { code?: string } | null)?.code;
      if (!error) { res.status(200).json({ ok: true, enrolled: true }); return; }
      if (code === 'P4041') { res.status(200).json({ ok: true, enrolled: false }); return; }
      if (code === 'P4291') { res.status(200).json({ ok: true, enrolled: true, locked: true }); return; }
      // Unknown failure: say so rather than guessing "not enrolled" and inviting someone to
      // replace a working authenticator they still have.
      res.status(502).json({ ok: false, code: 'ENROLMENT_STATUS_UNAVAILABLE' });
    } catch {
      res.status(502).json({ ok: false, code: 'ENROLMENT_STATUS_UNAVAILABLE' });
    }
    return;
  }

  const body = (req.body ?? {}) as Record<string, unknown>;
  const code = typeof body.code === 'string' ? body.code.replace(/\D/gu, '') : '';

  try {
    // ---- start -------------------------------------------------------------------------------
    if (code === '') {
      const secret = generateSecret();
      const { error } = await service.rpc('apocky_totp_start_enrolment', {
        p_user_id: session.user.id,
        p_secret: secret,
      });
      if (error) {
        res.status(502).json({ ok: false, code: 'ENROLMENT_FAILED' });
        return;
      }
      const uri = provisioningUri(secret, session.user.email);
      // Three ways in, because which one is usable depends on where you are standing: a QR when a
      // second device is doing the scanning, the link when the authenticator is on THIS device and
      // cannot photograph its own screen, and the key when neither works.
      //
      // Rendered server-side and inlined as a data URI: the alternative is a URL holding the
      // provisioning secret, and a credential in a URL ends up in history, logs and referrers.
      let qr: string | null = null;
      try {
        qr = await QRCode.toDataURL(uri, { width: 320, margin: 1, errorCorrectionLevel: 'M' });
      } catch {
        qr = null; // The link and the key still work; a missing QR is not a failed enrolment.
      }
      res.status(200).json({
        ok: true,
        uri,
        qr,
        secret,
        account: session.user.email,
      });
      return;
    }

    // ---- confirm -----------------------------------------------------------------------------
    if (code.length !== 6) {
      res.status(400).json({ ok: false, code: 'CODE_INVALID', error: 'Enter the 6-digit code from your authenticator.' });
      return;
    }

    const { data: pendingRows, error: pendingError } = await service.rpc('apocky_totp_pending', {
      p_user_id: session.user.id,
    });
    const pending = Array.isArray(pendingRows) && pendingRows.length > 0
      ? (pendingRows[0] as { pending_secret: string }).pending_secret
      : null;
    if (pendingError || !pending) {
      res.status(409).json({
        ok: false,
        code: 'NO_ENROLMENT',
        error: 'That setup has expired. Start again to get a new key.',
      });
      return;
    }

    // No replay guard needed here: this secret has never been usable, so there is no earlier code
    // of its own to replay. The confirm call records this step as the watermark.
    const verdict = verifyTotp(pending, code);
    if (!verdict.ok || verdict.step === null) {
      // "That code did not match" was true and useless: a correct code fails here for two opposite
      // reasons, and the message named neither. Widening the search does not accept anything — it
      // only tells the two apart, and this path has already proved it is the account.
      const drift = clockDriftSteps(pending, code);
      console.log(JSON.stringify({
        at: new Date().toISOString(),
        level: 'info',
        event: 'auth.totp.confirm_rejected',
        reason: drift === null ? 'code_not_from_this_secret' : 'device_clock_drift',
        drift_steps: drift,
      }));
      res.status(401).json({
        ok: false,
        code: drift === null ? 'CODE_FOREIGN' : 'CODE_CLOCK_DRIFT',
        drift_seconds: drift === null ? null : drift * 30,
        error: drift === null
          // No clock within ten minutes of here produces this code from the pending secret, so it
          // came from a different secret: almost always a leftover Apocky entry from an earlier
          // setup, since starting setup again silently replaces the pending code.
          ? 'That code belongs to a different setup. If your authenticator lists more than one '
            + 'Apocky entry, delete every one of them and scan the code on this page again — '
            + 'starting setup replaces the previous code, so an older entry can no longer work.'
          // The code is right. The clock it was generated against is not.
          : `Your device's clock is ${describeDrift(drift)}. Codes last 30 seconds, so this will `
            + 'keep failing — and would keep failing at sign-in too. Turn on automatic date and '
            + 'time on the device running your authenticator, then try the new code.',
      });
      return;
    }

    const { error: confirmError } = await service.rpc('apocky_totp_confirm', {
      p_user_id: session.user.id,
      p_step: verdict.step,
    });
    if (confirmError) {
      res.status(502).json({ ok: false, code: 'CONFIRM_FAILED' });
      return;
    }

    res.status(200).json({ ok: true, confirmed: true });
  } catch {
    res.status(502).json({ ok: false, code: 'ENROLMENT_UNAVAILABLE' });
  }
}
