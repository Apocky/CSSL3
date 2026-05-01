// /api/auth/magic-link · email magic-link sign-in / register endpoint
// Falls back to stub-mode if APOCKY_HUB_SUPABASE_URL is not configured.

import type { NextApiRequest, NextApiResponse } from 'next';
import { signInWithMagicLink, getAuthClient } from '../../../lib/auth';

export default async function handler(req: NextApiRequest, res: NextApiResponse) {
  if (req.method !== 'POST') {
    res.setHeader('Allow', 'POST');
    return res.status(405).json({ ok: false, message: 'Method not allowed' });
  }

  const { email, redirectTo, isRegistration } = req.body ?? {};
  if (typeof email !== 'string' || !email.includes('@')) {
    return res.status(400).json({ ok: false, message: '✗ Valid email required' });
  }

  // Stub-mode : Apocky-Hub Supabase not yet configured.
  if (!getAuthClient()) {
    return res.status(200).json({
      ok: false,
      stub: true,
      message:
        '⚠ stub-mode · APOCKY_HUB_SUPABASE_URL not set. Once Apocky configures the hub Supabase project (per spec/22), magic-link auth activates automatically. Your email + agreement are NOT stored in stub-mode.',
    });
  }

  const safeRedirect = typeof redirectTo === 'string' && redirectTo.startsWith('http')
    ? redirectTo
    : 'https://apocky.com/account';

  const result = await signInWithMagicLink(email, safeRedirect);

  if (!result.ok) {
    return res.status(500).json({ ok: false, message: `✗ ${result.reason ?? 'unknown error'}` });
  }

  return res.status(200).json({
    ok: true,
    message: isRegistration
      ? '✓ Verification link sent · check your email to complete registration'
      : '✓ Sign-in link sent · check your email',
  });
}
