// cssl-edge · /api/hotfix/revoke
// § T11-W11-HOTFIX-INFRA — admin-only revocation endpoint.
//
// POST /api/hotfix/revoke
//   headers : authorization: Bearer <admin-token>  (matched against env)
//   body    : { channel, version, reason }
//   → { ok: true, marked: bool }
//   Marks (channel, version) as revoked in `hotfix_manifest_versions`.
//   Subsequent /api/hotfix/manifest responses include the entry under
//   `revocations` ; clients pull → uninstall any matching local bundle.
import type { NextApiRequest, NextApiResponse } from 'next';
import { logHit, envelope, stubEnvelope } from '@/lib/response';

const VALID_CHANNELS = new Set([
  'loa.binary', 'cssl.bundle', 'kan.weights', 'balance.config',
  'recipe.book', 'nemesis.bestiary', 'security.patch',
  'storylet.content', 'render.pipeline',
]);

interface RevokeBody {
  channel?: string;
  version?: string;
  reason?: string;
}

export default async function handler(req: NextApiRequest, res: NextApiResponse): Promise<void> {
  logHit('hotfix.revoke', { method: req.method ?? 'POST' });

  if (req.method !== 'POST') {
    res.status(405).json({ ok: false, error: 'POST only', ...envelope() });
    return;
  }

  // Admin gate. The admin-token is generated on bootstrap and stored in
  // ~/.loa-secrets/apocky-hub-admin-token ; the cssl-edge env carries the
  // sha256 only (HOTFIX_ADMIN_TOKEN_SHA256), the request carries the raw.
  const auth = req.headers.authorization;
  const adminSha = process.env.HOTFIX_ADMIN_TOKEN_SHA256;
  if (!auth || !auth.startsWith('Bearer ') || !adminSha) {
    res.status(401).json({ ok: false, error: 'admin-required', ...envelope() });
    return;
  }
  const tok = auth.slice('Bearer '.length).trim();
  // Constant-time equality via fixed-length sha256 digest comparison.
  const enc = new TextEncoder();
  const buf = await crypto.subtle.digest('SHA-256', enc.encode(tok));
  const hex = Array.from(new Uint8Array(buf))
    .map((b) => b.toString(16).padStart(2, '0'))
    .join('');
  if (hex.length !== adminSha.length || !timingSafeEqual(hex, adminSha)) {
    res.status(403).json({ ok: false, error: 'admin-denied', ...envelope() });
    return;
  }

  const body: RevokeBody = (req.body ?? {}) as RevokeBody;
  if (!body.channel || !VALID_CHANNELS.has(body.channel)) {
    res.status(400).json({ ok: false, error: 'bad channel', ...envelope() });
    return;
  }
  if (!body.version || !/^\d+\.\d+\.\d+$/.test(body.version)) {
    res.status(400).json({ ok: false, error: 'bad version', ...envelope() });
    return;
  }
  if (!body.reason || body.reason.length < 4 || body.reason.length > 200) {
    res.status(400).json({ ok: false, error: 'reason-required (4..200 chars)', ...envelope() });
    return;
  }

  const supabaseUrl = process.env.NEXT_PUBLIC_SUPABASE_URL;
  const sbServiceKey = process.env.SUPABASE_SERVICE_ROLE_KEY;
  if (!supabaseUrl || !sbServiceKey) {
    res.status(200).json(stubEnvelope('wire SUPABASE_SERVICE_ROLE_KEY ; revoke patches hotfix_manifest_versions.revoked_at + appends to hotfix_revocations'));
    return;
  }

  try {
    // PATCH : set revoked_at + revoked_reason.
    const r = await fetch(
      `${supabaseUrl}/rest/v1/hotfix_manifest_versions?channel=eq.${encodeURIComponent(body.channel)}&version=eq.${encodeURIComponent(body.version)}`,
      {
        method: 'PATCH',
        headers: {
          apikey: sbServiceKey,
          authorization: `Bearer ${sbServiceKey}`,
          'content-type': 'application/json',
          prefer: 'return=minimal',
        },
        body: JSON.stringify({
          revoked_at: new Date().toISOString(),
          revoked_reason: body.reason,
        }),
      },
    );
    if (!r.ok) {
      res.status(502).json({ ok: false, error: `supabase ${r.status}`, ...envelope() });
      return;
    }
    res.status(200).json({ ok: true, marked: true, ...envelope() });
  } catch (e: unknown) {
    res.status(500).json({
      ok: false,
      error: e instanceof Error ? e.message : 'internal-error',
      ...envelope(),
    });
  }
}

function timingSafeEqual(a: string, b: string): boolean {
  if (a.length !== b.length) return false;
  let diff = 0;
  for (let i = 0; i < a.length; i++) {
    diff |= a.charCodeAt(i) ^ b.charCodeAt(i);
  }
  return diff === 0;
}
