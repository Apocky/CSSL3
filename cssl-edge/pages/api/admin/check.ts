// /api/admin/check · verifies signed-in user is on admin allowlist
// Allowlist via APOCKY_ADMIN_EMAILS env-var (comma-separated) · falls-back to apocky13@gmail.com

import type { NextApiRequest, NextApiResponse } from 'next';
import { getAuthClient } from '../../../lib/auth';

const DEFAULT_ALLOWLIST = ['apocky13@gmail.com'];

function getAllowlist(): string[] {
  const env = process.env.APOCKY_ADMIN_EMAILS;
  if (!env) return DEFAULT_ALLOWLIST;
  return env
    .split(',')
    .map((s) => s.trim().toLowerCase())
    .filter((s) => s.length > 0 && s.includes('@'));
}

export default async function handler(req: NextApiRequest, res: NextApiResponse) {
  if (req.method !== 'GET') {
    res.setHeader('Allow', 'GET');
    return res.status(405).json({ authorized: false, reason: 'Method not allowed' });
  }

  const allowlist = getAllowlist();
  const client = getAuthClient();

  if (!client) {
    return res.status(200).json({
      authorized: false,
      stub: true,
      reason:
        'Stub mode · APOCKY_HUB_SUPABASE_URL not set. Admin pages render UI-only · server-side endpoints will validate once Apocky-Hub Supabase is configured.',
    });
  }

  const cookies = req.headers.cookie ?? '';
  const tokenMatch = cookies.match(/sb-access-token=([^;]+)/);
  const captured = tokenMatch?.[1];
  const accessToken = captured ? decodeURIComponent(captured) : undefined;

  if (!accessToken) {
    return res.status(200).json({
      authorized: false,
      reason: 'Not signed in · sign in at /login with admin email.',
    });
  }

  const { data, error } = await client.auth.getUser(accessToken);
  if (error || !data?.user?.email) {
    return res.status(200).json({
      authorized: false,
      reason: 'Session invalid or expired · sign in again.',
    });
  }

  const userEmail = data.user.email.toLowerCase();
  const authorized = allowlist.includes(userEmail);

  return res.status(200).json({
    authorized,
    email: data.user.email,
    reason: authorized ? undefined : 'Email not on admin allowlist.',
  });
}
