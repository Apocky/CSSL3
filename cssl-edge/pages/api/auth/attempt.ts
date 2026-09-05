import type { NextApiRequest, NextApiResponse } from 'next';

import { AUTH_FENCE_PROTOCOL, issueAuthAttempt, type AuthAttemptMode } from '@/lib/auth-fence';
import { hasSameOrigin } from '@/lib/auth-session';

function exactBody(value: unknown): value is { mode: AuthAttemptMode } {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return false;
  const row = value as Record<string, unknown>;
  return Object.keys(row).length === 1 && (row.mode === 'fresh' || row.mode === 'refresh');
}

export default async function handler(req: NextApiRequest, res: NextApiResponse): Promise<void> {
  res.setHeader('Cache-Control', 'private, no-store, max-age=0');
  res.setHeader('Pragma', 'no-cache');
  res.setHeader('Vary', 'Origin, Cookie');
  res.setHeader('Allow', 'POST');
  if (req.method !== 'POST') {
    res.status(405).json({ ok: false, code: 'AUTH_ATTEMPT_METHOD_NOT_ALLOWED' });
    return;
  }
  if (!hasSameOrigin(req)) {
    res.status(403).json({ ok: false, code: 'AUTH_ATTEMPT_ORIGIN_DENIED' });
    return;
  }
  if (!exactBody(req.body)) {
    res.status(400).json({ ok: false, code: 'AUTH_ATTEMPT_BODY_INVALID' });
    return;
  }
  try {
    const issued = issueAuthAttempt({ req, mode: req.body.mode });
    if (!issued) {
      res.status(req.body.mode === 'refresh' ? 401 : 400).json({ ok: false, code: 'AUTH_ATTEMPT_FENCE_INVALID' });
      return;
    }
    res.status(200).json({
      schema_version: AUTH_FENCE_PROTOCOL,
      status: 'ready',
      mode: req.body.mode,
      ticket: issued.ticket,
      expires_at: new Date(issued.expiresAtMs).toISOString(),
      provider_start_delay_ms: Math.max(0, issued.providerStartAfterMs - Date.now()),
    });
  } catch {
    res.status(503).json({ ok: false, code: 'AUTH_FENCE_UNAVAILABLE' });
  }
}
