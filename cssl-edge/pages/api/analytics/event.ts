// cssl-edge · /api/analytics/event
// ════════════════════════════════════════════════════════════════════════
// § T11-W11-ANALYTICS · POST handler for analytics-event ingest.
//
// § Wire-format · client POSTs an EventEnvelope (see lib/analytics.ts).
// § Σ-mask validation runs BEFORE Supabase insert :
//   1. body-size limit (4KB)
//   2. envelope shape (UUIDs · kind_id range · flags u16)
//   3. PII heuristic (no ASCII runs ≥ 6 bytes in payload)
//   4. consent.cap = Deny ⇒ silent drop with 204 No-Content
//
// § Stub-mode-aware : if Supabase env-vars missing, return 200 with
//   stubbed envelope so client-side smoke-tests pass.

import type { NextApiRequest, NextApiResponse } from 'next';
import { logHit, stubEnvelope, envelope } from '@/lib/response';
import {
  validateEventEnvelope,
  ANALYTICS_BODY_LIMIT_BYTES,
  CONSENT_CAPS,
  eventKindName,
} from '@/lib/analytics';
import { getSupabase } from '@/lib/supabase';

interface IngestOk {
  ok: true;
  event_id: string | null;
  kind: string;
  consent_cap: number;
  served_by: string;
  ts: string;
  stub?: true;
  todo?: string;
}

interface IngestErr {
  error: string;
  served_by: string;
  ts: string;
}

// § disable bodyParser default-1MB ; we cap at 4KB ourselves.
export const config = {
  api: {
    bodyParser: {
      sizeLimit: '4kb',
    },
  },
};

export default async function handler(
  req: NextApiRequest,
  res: NextApiResponse<IngestOk | IngestErr>
): Promise<void> {
  logHit('analytics.event', { method: req.method ?? 'GET' });

  if (req.method !== 'POST') {
    const env = envelope();
    res.setHeader('Allow', 'POST');
    res.status(405).json({
      error: 'Method Not Allowed — POST only',
      served_by: env.served_by,
      ts: env.ts,
    });
    return;
  }

  // § Body-size guard. Next.js bodyParser already limits via config,
  // but we verify the parsed shape too.
  const rawBodyLen = JSON.stringify(req.body ?? {}).length;
  if (rawBodyLen > ANALYTICS_BODY_LIMIT_BYTES) {
    const env = envelope();
    res.status(413).json({
      error: `Payload Too Large (>${ANALYTICS_BODY_LIMIT_BYTES} bytes)`,
      served_by: env.served_by,
      ts: env.ts,
    });
    return;
  }

  // § Shape + Σ-mask validation.
  const validated = validateEventEnvelope(req.body);
  if (typeof validated === 'string') {
    const env = envelope();
    res.status(400).json({
      error: `Bad Request — ${validated}`,
      served_by: env.served_by,
      ts: env.ts,
    });
    return;
  }
  const ev = validated;

  // § Σ-mask consent-cap = Deny ⇒ silent drop (204 No-Content equivalent
  // returned as 200 with event_id=null per envelope contract).
  if (ev.sigma_consent_cap === CONSENT_CAPS.Deny) {
    const env = envelope();
    res.status(200).json({
      ok: true,
      event_id: null,
      kind: eventKindName(ev.kind_id),
      consent_cap: ev.sigma_consent_cap,
      served_by: env.served_by,
      ts: env.ts,
    });
    return;
  }

  // § Try Supabase insert ; fall back to stub if env-vars missing.
  const supabase = getSupabase();
  if (!supabase) {
    const stub = stubEnvelope('Supabase env-vars missing · stub-mode insert');
    res.status(200).json({
      ok: true,
      event_id: null,
      kind: eventKindName(ev.kind_id),
      consent_cap: ev.sigma_consent_cap,
      served_by: stub.served_by,
      ts: stub.ts,
      stub: true,
      todo: stub.todo,
    });
    return;
  }

  // § Real insert via the SECURITY-DEFINER ingest_event RPC. The RPC
  // gates Σ-mask cap=0 and bumps the 1min rollup atomically.
  try {
    const { data, error } = await supabase.rpc('ingest_event', {
      p_player_id: ev.player_id,
      p_session_id: ev.session_id,
      p_kind_id: ev.kind_id,
      p_payload_kind: ev.payload_kind,
      p_flags: ev.flags,
      p_frame_offset: ev.frame_offset,
      p_payload_b64: ev.payload_b64,
      p_sigma_consent_cap: ev.sigma_consent_cap,
    });
    if (error) {
      const env = envelope();
      res.status(502).json({
        error: `Supabase RPC failed — ${error.message}`,
        served_by: env.served_by,
        ts: env.ts,
      });
      return;
    }
    const env = envelope();
    res.status(200).json({
      ok: true,
      event_id: typeof data === 'string' ? data : null,
      kind: eventKindName(ev.kind_id),
      consent_cap: ev.sigma_consent_cap,
      served_by: env.served_by,
      ts: env.ts,
    });
  } catch (e: unknown) {
    const env = envelope();
    const msg = e instanceof Error ? e.message : 'unknown';
    res.status(500).json({
      error: `Internal — ${msg}`,
      served_by: env.served_by,
      ts: env.ts,
    });
  }
}
