// /api/auth/me · returns current-user session info OR null
// Stub-mode safe : returns { user: null, stub: true } when hub Supabase not configured

import type { NextApiRequest, NextApiResponse } from 'next';
import { getAuthClient } from '../../../lib/auth';

export default async function handler(req: NextApiRequest, res: NextApiResponse) {
  if (req.method !== 'GET') {
    res.setHeader('Allow', 'GET');
    return res.status(405).json({ user: null });
  }

  const client = getAuthClient();
  if (!client) {
    return res.status(200).json({ user: null, stub: true });
  }

  // Read JWT from cookie if present.
  const authHeader = req.headers.authorization;
  let accessToken: string | undefined;
  if (authHeader?.startsWith('Bearer ')) {
    accessToken = authHeader.slice('Bearer '.length);
  }
  if (!accessToken) {
    // Try reading from sb-access-token cookie · default Supabase cookie name varies by setup
    const cookies = req.headers.cookie ?? '';
    const match = cookies.match(/sb-access-token=([^;]+)/);
    const captured = match?.[1];
    if (captured) accessToken = decodeURIComponent(captured);
  }

  if (!accessToken) {
    return res.status(200).json({ user: null });
  }

  const { data, error } = await client.auth.getUser(accessToken);
  if (error || !data?.user) {
    return res.status(200).json({ user: null });
  }

  return res.status(200).json({
    user: {
      id: data.user.id,
      email: data.user.email ?? '(no email)',
      provider: data.user.app_metadata?.provider ?? 'email',
      createdAt: data.user.created_at ?? new Date().toISOString(),
    },
  });
}
