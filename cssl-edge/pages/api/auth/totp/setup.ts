// Set up an authenticator from inside the app.
//
// Requires a signed-in session: enrolment adds a credential to an account, so proving you are
// already that account is the whole gate. There is deliberately no "enrol without signing in" path
// — that would let anyone claim an account that has no authenticator yet.
//
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

import { generateSecret, provisioningUri, verifyTotp } from '@/lib/auth-totp';

export const config = { api: { bodyParser: { sizeLimit: '8kb' } } };

export default async function handler(req: NextApiRequest, res: NextApiResponse): Promise<void> {
  res.setHeader('Cache-Control', 'private, no-store, max-age=0');
  res.setHeader('Vary', 'Cookie');
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
      res.status(401).json({
        ok: false,
        code: 'CODE_REJECTED',
        error: 'That code did not match. Check your authenticator shows this account, then try the current code.',
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
