// cssl-edge · /api/signaling/poll
// GET handler · long-poll undelivered signals addressed to a peer.
//
// Auth :
//   - Cap-bit MP_CAP_RELAY_DATA = 4 REQUIRED · DEFAULT-DENY when caps=0
//   - Sovereign bypass : sovereign:true + x-loa-sovereign-cap header → allowed
//
// Query : ?room_id=<uuid>&peer_id=<uuid>&since=<id>&cap=<int>&sovereign=<bool>
// 200   : { served_by, ts, signals: [...], next_since: number, stub? }
// 400   : missing query params
// 403   : cap-denied
// Audit : { kind: 'mp.poll', cap, sovereign, status, room_id?, peer_id?, count? }

import type { NextApiRequest, NextApiResponse } from 'next';
import { auditEvent, deny, logEvent } from '@/lib/audit';
import { isSovereignFromIncoming } from '@/lib/sovereign';
import { envelope, logHit } from '@/lib/response';
import { MP_CAP_RELAY_DATA, checkCap } from '@/lib/cap';
import { pollSignals, type SignalingMessageRow } from '@/lib/supabase';

interface PollOk {
  served_by: string;
  ts: string;
  signals: SignalingMessageRow[];
  next_since: number;
  stub?: true;
}

interface PollError {
  error: string;
  served_by: string;
  ts: string;
}

function readStringParam(
  q: Record<string, string | string[] | undefined>,
  key: string
): string | undefined {
  const v = q[key];
  if (Array.isArray(v)) return v[0];
  return v;
}

function readNumberParam(
  q: Record<string, string | string[] | undefined>,
  key: string,
  fallback: number
): number {
  const raw = readStringParam(q, key);
  if (raw === undefined) return fallback;
  const n = Number(raw);
  if (!Number.isFinite(n)) return fallback;
  return n;
}

function readBoolParam(
  q: Record<string, string | string[] | undefined>,
  key: string
): boolean {
  const raw = readStringParam(q, key);
  return raw === 'true' || raw === '1';
}

export default async function handler(
  req: NextApiRequest,
  res: NextApiResponse<PollOk | PollError>
): Promise<void> {
  logHit('signaling.poll', { method: req.method ?? 'GET' });

  if (req.method !== 'GET') {
    const env = envelope();
    res.setHeader('Allow', 'GET');
    res.status(405).json({
      error: 'Method Not Allowed — GET ?room_id=&peer_id=&since=&cap=&sovereign=',
      served_by: env.served_by,
      ts: env.ts,
    });
    return;
  }

  const q = req.query as Record<string, string | string[] | undefined>;
  const room_id = readStringParam(q, 'room_id');
  const peer_id = readStringParam(q, 'peer_id');
  if (typeof room_id !== 'string' || room_id.length === 0) {
    const env = envelope();
    res.status(400).json({
      error: 'Bad Request — room_id query param required',
      served_by: env.served_by,
      ts: env.ts,
    });
    return;
  }
  if (typeof peer_id !== 'string' || peer_id.length === 0) {
    const env = envelope();
    res.status(400).json({
      error: 'Bad Request — peer_id query param required',
      served_by: env.served_by,
      ts: env.ts,
    });
    return;
  }

  const since = readNumberParam(q, 'since', 0);
  const cap = readNumberParam(q, 'cap', 0);
  const sovereignFlag = readBoolParam(q, 'sovereign');
  const sovereignAllowed = isSovereignFromIncoming(req.headers, sovereignFlag);

  // Cap-gate.
  const decision = checkCap(cap, MP_CAP_RELAY_DATA, sovereignAllowed);
  if (!decision.ok) {
    const reason = decision.reason ?? 'cap MP_CAP_RELAY_DATA=0x4 required';
    const d = deny(reason, cap);
    logEvent(d.body);
    const env = envelope();
    res.status(d.status).json({
      error: reason,
      served_by: env.served_by,
      ts: env.ts,
    });
    return;
  }

  const supabaseConfigured = Boolean(
    process.env['NEXT_PUBLIC_SUPABASE_URL'] && process.env['SUPABASE_ANON_KEY']
  );
  const result = await pollSignals(room_id, peer_id, since);
  const env = envelope();

  if (!supabaseConfigured) {
    logEvent(
      auditEvent('mp.poll', cap, sovereignAllowed, 'ok', {
        room_id,
        peer_id,
        count: 0,
        stub: true,
      })
    );
    res.status(200).json({
      served_by: env.served_by,
      ts: env.ts,
      signals: [],
      next_since: since,
      stub: true,
    });
    return;
  }

  logEvent(
    auditEvent('mp.poll', cap, sovereignAllowed, 'ok', {
      room_id,
      peer_id,
      count: result.signals.length,
      next_since: result.next_since,
    })
  );
  res.status(200).json({
    served_by: env.served_by,
    ts: env.ts,
    signals: result.signals,
    next_since: result.next_since,
  });
}
