// /api/auth/oauth · Google / Apple / GitHub / Discord · OAuth init endpoint
// Returns provider-redirect URL · client navigates · provider redirects-back to /api/auth/callback

import type { NextApiRequest, NextApiResponse } from 'next';
import { getAuthClient } from '../../../lib/auth';

const ALLOWED_PROVIDERS = ['google', 'apple', 'github', 'discord'] as const;
type AllowedProvider = (typeof ALLOWED_PROVIDERS)[number];

export default async function handler(req: NextApiRequest, res: NextApiResponse) {
  if (req.method !== 'POST') {
    res.setHeader('Allow', 'POST');
    return res.status(405).json({ ok: false, message: 'Method not allowed' });
  }

  const { provider, redirectTo } = req.body ?? {};
  if (typeof provider !== 'string' || !ALLOWED_PROVIDERS.includes(provider as AllowedProvider)) {
    return res.status(400).json({
      ok: false,
      message: `✗ Provider must be one of : ${ALLOWED_PROVIDERS.join(', ')}`,
    });
  }

  const client = getAuthClient();
  if (!client) {
    return res.status(200).json({
      ok: false,
      stub: true,
      message:
        '⚠ stub-mode · APOCKY_HUB_SUPABASE_URL not set. OAuth providers activate once Apocky configures hub Supabase + provider OAuth-app credentials in Supabase dashboard.',
    });
  }

  const safeRedirect = typeof redirectTo === 'string' && redirectTo.startsWith('http')
    ? redirectTo
    : 'https://apocky.com/account';

  const { data, error } = await client.auth.signInWithOAuth({
    provider: provider as AllowedProvider,
    options: { redirectTo: safeRedirect, skipBrowserRedirect: true },
  });

  if (error || !data?.url) {
    return res.status(500).json({ ok: false, message: `✗ ${error?.message ?? 'OAuth init failed'}` });
  }

  return res.status(200).json({ ok: true, url: data.url });
}
