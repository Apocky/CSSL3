// /api/auth/oauth · Google / Apple / GitHub / Discord · OAuth init endpoint
// Returns provider-redirect URL · client navigates · provider redirects-back to /api/auth/callback

import type { NextApiRequest, NextApiResponse } from 'next';
import { createClient } from '@supabase/supabase-js';
import { resolveAuthRedirect } from '../../../lib/auth';

const ALLOWED_PROVIDERS = ['google', 'apple', 'github', 'discord'] as const;
type AllowedProvider = (typeof ALLOWED_PROVIDERS)[number];

function getLegacyOAuthClient() {
  const url = process.env.APOCKY_HUB_SUPABASE_URL ?? process.env.NEXT_PUBLIC_SUPABASE_URL;
  const anonKey = process.env.APOCKY_HUB_SUPABASE_ANON_KEY ?? process.env.NEXT_PUBLIC_SUPABASE_ANON_KEY;
  if (!url || !anonKey) return null;

  // This route exists for older cached login pages that still POST here.
  // Server-side PKCE cannot work because the verifier must stay in browser
  // storage, so this fallback uses implicit flow and lets /auth/callback set
  // the returned hash tokens as the browser session.
  return createClient(url, anonKey, {
    auth: {
      persistSession: false,
      autoRefreshToken: false,
      detectSessionInUrl: false,
      flowType: 'implicit',
    },
  });
}

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

  const client = getLegacyOAuthClient();
  if (!client) {
    return res.status(200).json({
      ok: false,
      stub: true,
      message:
        '⚠ stub-mode · APOCKY_HUB_SUPABASE_URL not set. OAuth providers activate once Apocky configures hub Supabase + provider OAuth-app credentials in Supabase dashboard.',
    });
  }

  const safeRedirect = resolveAuthRedirect(redirectTo, req.headers);

  const { data, error } = await client.auth.signInWithOAuth({
    provider: provider as AllowedProvider,
    options: { redirectTo: safeRedirect, skipBrowserRedirect: true },
  });

  if (error || !data?.url) {
    return res.status(500).json({ ok: false, message: `✗ ${error?.message ?? 'OAuth init failed'}` });
  }

  return res.status(200).json({ ok: true, url: data.url });
}
