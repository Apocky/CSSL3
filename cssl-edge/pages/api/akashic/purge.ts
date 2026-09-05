// § Akashic-Webpage-Records · /api/akashic/purge
// DELETE · sovereign-purge of all events tied to a user-cap. Cap-witness
// header required (x-akashic-cap-witness). Server hashes the witness +
// invokes public.akashic_purge() (SECURITY DEFINER · INSERT-only otherwise).
//
// This is the user-facing sovereign-revoke escape-hatch. Triggered from
// /admin/telemetry "purge all my events" button OR from purgeAllMine() lib.
//
// Cap-bit : sovereign-cap MAY bypass (admin purging self) ; otherwise the
// supplied witness MUST match the user_cap_hash on rows.

import type { NextApiRequest, NextApiResponse } from 'next';
import { envelope, logHit } from '@/lib/response';
import { getSupabaseService } from '@/lib/supabase';

const CAP_HEADER = 'x-akashic-cap-witness';
const MIN_WITNESS_LEN = 8;

interface PurgeBody {
  session_id?: string;
}

interface OkResp {
  served_by: string;
  ts: string;
  ok: true;
  rows_deleted: number;
  source: 'supabase' | 'stub';
}
interface ErrResp { served_by: string; ts: string; error: string; }

// 16-char fnv-1a-ish hash mirroring the client-side.
function hash16(s: string): string {
  let h1 = 0x811c9dc5 >>> 0;
  let h2 = 0xcbf29ce4 >>> 0;
  for (let i = 0; i < s.length; i++) {
    const c = s.charCodeAt(i);
    h1 = Math.imul(h1 ^ c, 0x01000193) >>> 0;
    h2 = Math.imul(h2 ^ c, 0x100000001b3 >>> 0) >>> 0;
  }
  return (h1.toString(16).padStart(8, '0') + h2.toString(16).padStart(8, '0')).slice(0, 16);
}

export default async function handler(
  req: NextApiRequest,
  res: NextApiResponse<OkResp | ErrResp>
): Promise<void> {
  logHit('akashic.purge', { method: req.method ?? 'DELETE' });
  if (req.method !== 'DELETE') {
    const env = envelope();
    res.setHeader('Allow', 'DELETE');
    res.status(405).json({ served_by: env.served_by, ts: env.ts, error: 'DELETE only' });
    return;
  }

  const witnessRaw = req.headers[CAP_HEADER];
  const witness = Array.isArray(witnessRaw) ? witnessRaw[0] : witnessRaw;
  if (typeof witness !== 'string' || witness.length < MIN_WITNESS_LEN) {
    const env = envelope();
    res.status(401).json({ served_by: env.served_by, ts: env.ts, error: 'cap-witness required' });
    return;
  }

  const body = (req.body ?? {}) as PurgeBody;
  if (body.session_id !== undefined && (typeof body.session_id !== 'string' || body.session_id.length > 64)) {
    const env = envelope();
    res.status(400).json({ served_by: env.served_by, ts: env.ts, error: 'invalid session_id' });
    return;
  }
  // The caller may never select an arbitrary deletion target. The target is
  // derived only from the presented witness, matching the client-side link.
  const target_hash = hash16(witness);

  const sb = getSupabaseService();
  if (sb === null) {
    // stub-mode · no DB to delete from. Return 0 deletions (truthful).
    const env = envelope();
    res.status(200).json({
      served_by: env.served_by,
      ts: env.ts,
      ok: true,
      rows_deleted: 0,
      source: 'stub',
    });
    return;
  }

  const { data, error } = await sb.rpc('akashic_purge', { p_user_cap_hash: target_hash });
  if (error !== null && error !== undefined) {
    const env = envelope();
    res.status(500).json({
      served_by: env.served_by,
      ts: env.ts,
      error: `purge failed : ${error.message}`,
    });
    return;
  }

  const rows_deleted = typeof data === 'number' ? data : 0;
  const env = envelope();
  res.status(200).json({
    served_by: env.served_by,
    ts: env.ts,
    ok: true,
    rows_deleted,
    source: 'supabase',
  });
}
