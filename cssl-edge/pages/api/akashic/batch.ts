// § Akashic-Webpage-Records · /api/akashic/batch
// POST · ingest a BATCH of events from the client ring-buffer flush.
// Preferred ingest path (vs single /event) for cost + throughput.
//
// Shape : { batch_id, session_id, events:[...], flush_reason }
// Limits : ≤ 256 events per batch (matches client RING_CAP).

import type { NextApiRequest, NextApiResponse } from 'next';
import { envelope, logHit } from '@/lib/response';
import { getSupabase } from '@/lib/supabase';

const EMAIL_RX = /[a-z0-9._%+-]+@[a-z0-9.-]+\.[a-z]{2,}/gi;
const JWT_RX = /eyJ[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}\.[A-Za-z0-9_-]{8,}/g;
const LONG_DIGITS_RX = /\b\d{12,19}\b/g;
const QUERY_SECRET_RX = /([?&](?:token|key|secret|api[_-]?key|password|auth)=)([^&\s]+)/gi;

function redactString(s: string): string {
  return s
    .replace(EMAIL_RX, '«email»')
    .replace(JWT_RX, '«jwt»')
    .replace(LONG_DIGITS_RX, '«num»')
    .replace(QUERY_SECRET_RX, '$1«redacted»');
}

function redactPayload(p: unknown): unknown {
  if (typeof p === 'string') return redactString(p);
  if (Array.isArray(p)) return p.map(redactPayload);
  if (p !== null && typeof p === 'object') {
    const out: Record<string, unknown> = {};
    for (const k of Object.keys(p as Record<string, unknown>)) {
      out[k] = redactPayload((p as Record<string, unknown>)[k]);
    }
    return out;
  }
  return p;
}

interface BatchEvent {
  cell_id?: string;
  ts_iso?: string;
  sigma_mask?: number;
  cap_witness?: string;
  dpl_id?: string;
  commit_sha?: string;
  build_time?: string;
  kind?: string;
  payload?: Record<string, unknown>;
  session_id?: string;
  user_cap_hash?: string;
}

interface BatchBody {
  batch_id?: string;
  session_id?: string;
  events?: BatchEvent[];
  flush_reason?: 'size' | 'interval' | 'unload' | 'manual';
}

interface OkResp {
  served_by: string;
  ts: string;
  ok: true;
  accepted: number;
  rejected: number;
  persisted: boolean;
  batch_id: string;
}
interface ErrResp { served_by: string; ts: string; error: string; }

const KIND_RX = /^[a-z][a-z0-9._-]{2,63}$/;
const MAX_BATCH = 256;

function isValid(e: BatchEvent): boolean {
  if (typeof e.cell_id !== 'string' || e.cell_id.length < 8 || e.cell_id.length > 64) return false;
  if (typeof e.kind !== 'string' || !KIND_RX.test(e.kind)) return false;
  if (typeof e.session_id !== 'string' || e.session_id.length < 8 || e.session_id.length > 64) return false;
  const mask = e.sigma_mask ?? 0;
  if (typeof mask !== 'number' || mask <= 0 || mask > 4095) return false;
  return true;
}

export default async function handler(
  req: NextApiRequest,
  res: NextApiResponse<OkResp | ErrResp>
): Promise<void> {
  logHit('akashic.batch', { method: req.method ?? 'POST' });
  if (req.method !== 'POST') {
    const env = envelope();
    res.setHeader('Allow', 'POST');
    res.status(405).json({ served_by: env.served_by, ts: env.ts, error: 'POST only' });
    return;
  }

  const body = (req.body ?? {}) as BatchBody;
  if (typeof body.batch_id !== 'string' || body.batch_id.length < 8) {
    const env = envelope();
    res.status(400).json({ served_by: env.served_by, ts: env.ts, error: 'batch_id required' });
    return;
  }
  if (!Array.isArray(body.events) || body.events.length === 0) {
    const env = envelope();
    res.status(400).json({ served_by: env.served_by, ts: env.ts, error: 'events array required' });
    return;
  }
  if (body.events.length > MAX_BATCH) {
    const env = envelope();
    res.status(400).json({ served_by: env.served_by, ts: env.ts, error: `batch too large (max ${MAX_BATCH})` });
    return;
  }

  // Validate + redact each event ; rejected ones are silently dropped (don't
  // 400 the whole batch · partial-success is friendlier to flaky networks).
  const accepted: Record<string, unknown>[] = [];
  let rejected = 0;
  for (const ev of body.events) {
    if (!isValid(ev)) { rejected++; continue; }
    accepted.push({
      cell_id: ev.cell_id,
      ts_iso: ev.ts_iso ?? new Date().toISOString(),
      sigma_mask: ev.sigma_mask,
      cap_witness_hash: ev.cap_witness ?? null,
      dpl_id: ev.dpl_id ?? 'unknown',
      commit_sha: ev.commit_sha ?? 'unknown',
      build_time: ev.build_time ?? 'unknown',
      kind: ev.kind,
      payload: redactPayload(ev.payload ?? {}),
      session_id: ev.session_id,
      user_cap_hash: ev.user_cap_hash ?? null,
      cluster_signature: ((ev.payload ?? {}) as Record<string, unknown>)['cluster_signature'] ?? null,
    });
  }

  const sb = getSupabase();
  let persisted = false;
  if (sb !== null && accepted.length > 0) {
    const { error } = await sb.from('akashic_events').insert(accepted);
    persisted = error === null;
  }

  const env = envelope();
  res.status(200).json({
    served_by: env.served_by,
    ts: env.ts,
    ok: true,
    accepted: accepted.length,
    rejected,
    persisted,
    batch_id: body.batch_id,
  });
}

// Disable Next's default 1MB JSON limit only modestly · 256 events × ~2KB ≈ 512KB.
export const config = {
  api: {
    bodyParser: {
      sizeLimit: '1mb',
    },
  },
};
