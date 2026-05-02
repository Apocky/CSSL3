// § Akashic-Webpage-Records · /api/akashic/event
// POST · ingest a SINGLE event. Σ-mask validated · k-anon enforced via
// downstream views ¬ inline. Stub-friendly : when Supabase env-vars absent,
// returns OK without persisting (preserves stage-0 deploy semantics).
//
// Bit-pack philosophy : tight schema · server-side redact-pass on payload.
// Prefer /api/akashic/batch for high-volume sessions ; this is the
// individual-event escape-hatch for early-error scripts that fire pre-init.

import type { NextApiRequest, NextApiResponse } from 'next';
import { envelope, logHit } from '@/lib/response';
import { getSupabase } from '@/lib/supabase';

// Server-side mirror of client-side redact (defense-in-depth).
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

interface AkashicEventBody {
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

interface OkResp { served_by: string; ts: string; ok: true; persisted: boolean; cell_id: string; }
interface ErrResp { served_by: string; ts: string; error: string; }

const KIND_RX = /^[a-z][a-z0-9._-]{2,63}$/;

function validate(body: AkashicEventBody): { ok: true; cell_id: string } | { ok: false; reason: string } {
  if (typeof body.cell_id !== 'string' || body.cell_id.length < 8 || body.cell_id.length > 64) {
    return { ok: false, reason: 'cell_id required (8-64 chars)' };
  }
  if (typeof body.kind !== 'string' || !KIND_RX.test(body.kind)) {
    return { ok: false, reason: 'kind required (matches ^[a-z][a-z0-9._-]+$)' };
  }
  if (typeof body.session_id !== 'string' || body.session_id.length < 8 || body.session_id.length > 64) {
    return { ok: false, reason: 'session_id required (8-64 chars)' };
  }
  const mask = body.sigma_mask ?? 0;
  if (typeof mask !== 'number' || mask < 0 || mask > 4095) {
    return { ok: false, reason: 'sigma_mask out of range' };
  }
  // Σ-mask = 0 ⇒ client should not have sent. Reject ; log nothing.
  if (mask === 0) return { ok: false, reason: 'sigma_mask=0 ; refusing to persist' };
  return { ok: true, cell_id: body.cell_id };
}

export default async function handler(
  req: NextApiRequest,
  res: NextApiResponse<OkResp | ErrResp>
): Promise<void> {
  logHit('akashic.event', { method: req.method ?? 'POST' });
  if (req.method !== 'POST') {
    const env = envelope();
    res.setHeader('Allow', 'POST');
    res.status(405).json({ served_by: env.served_by, ts: env.ts, error: 'POST only' });
    return;
  }
  const body = (req.body ?? {}) as AkashicEventBody;
  const v = validate(body);
  if (!v.ok) {
    const env = envelope();
    res.status(400).json({ served_by: env.served_by, ts: env.ts, error: v.reason });
    return;
  }

  const sb = getSupabase();
  let persisted = false;
  if (sb !== null) {
    const { error } = await sb.from('akashic_events').insert({
      cell_id: body.cell_id,
      ts_iso: body.ts_iso ?? new Date().toISOString(),
      sigma_mask: body.sigma_mask ?? 0,
      cap_witness_hash: body.cap_witness ?? null,
      dpl_id: body.dpl_id ?? 'unknown',
      commit_sha: body.commit_sha ?? 'unknown',
      build_time: body.build_time ?? 'unknown',
      kind: body.kind,
      payload: redactPayload(body.payload ?? {}),
      session_id: body.session_id,
      user_cap_hash: body.user_cap_hash ?? null,
      cluster_signature: ((body.payload ?? {}) as Record<string, unknown>)['cluster_signature'] ?? null,
    });
    persisted = error === null;
  }

  const env = envelope();
  res.status(200).json({
    served_by: env.served_by,
    ts: env.ts,
    ok: true,
    persisted,
    cell_id: v.cell_id,
  });
}
