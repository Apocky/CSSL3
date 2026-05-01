// cssl-edge · /api/signaling/post-signal
// POST handler · enqueues a signaling envelope (offer/answer/ICE/etc).
//
// Auth :
//   - Cap-bit MP_CAP_RELAY_DATA = 4 REQUIRED · DEFAULT-DENY when caps=0
//   - Sovereign bypass : sovereign:true + x-loa-sovereign-cap header → allowed
//
// Body  : { room_id, from_peer, to_peer, kind, payload, cap, sovereign? }
// 200   : { served_by, ts, id, stub? }
// 400   : invalid kind / payload too large / missing fields
// 403   : cap-denied
// Audit : { kind: 'mp.post_signal', cap, sovereign, status, room_id?, signal_kind? }

import type { NextApiRequest, NextApiResponse } from 'next';
import { auditEvent, deny, logEvent } from '@/lib/audit';
import { isSovereignFromIncoming } from '@/lib/sovereign';
import { envelope, logHit } from '@/lib/response';
import { MP_CAP_RELAY_DATA, checkCap } from '@/lib/cap';
import { postSignal } from '@/lib/supabase';

interface PostSignalRequest {
  room_id?: unknown;
  from_peer?: unknown;
  to_peer?: unknown;
  kind?: unknown;
  payload?: unknown;
  cap?: unknown;
  sovereign?: unknown;
}

interface PostSignalOk {
  served_by: string;
  ts: string;
  id: number;
  stub?: true;
}

interface PostSignalError {
  error: string;
  served_by: string;
  ts: string;
}

function isObject(b: unknown): b is Record<string, unknown> {
  return typeof b === 'object' && b !== null;
}

// Mirror SQL CHECK on signaling_messages.kind from 0004_signaling.sql.
const VALID_KINDS = new Set([
  'offer',
  'answer',
  'ice',
  'hello',
  'ping',
  'pong',
  'bye',
  'custom',
]);

// Mirror cssl-host-multiplayer-signaling::MAX_PAYLOAD_BYTES = 64 KiB.
const MAX_PAYLOAD_BYTES = 64 * 1024;

// Approximate the encoded byte-size of a payload. JSON.stringify gives us a
// portable upper bound — base64 round-trip via supabase-js will be at most
// ~33% larger than the raw bytes, but we cap on the JSON shape since that's
// what actually rides over the wire to /api/* + into the jsonb column.
function payloadByteSize(payload: unknown): number {
  if (payload === undefined) return 0;
  try {
    return Buffer.byteLength(JSON.stringify(payload) ?? '', 'utf8');
  } catch {
    return Number.POSITIVE_INFINITY;
  }
}

let _stubIdCounter = 0;
function nextStubId(): number {
  _stubIdCounter += 1;
  return _stubIdCounter;
}

export default async function handler(
  req: NextApiRequest,
  res: NextApiResponse<PostSignalOk | PostSignalError>
): Promise<void> {
  logHit('signaling.post-signal', { method: req.method ?? 'GET' });

  if (req.method !== 'POST') {
    const env = envelope();
    res.setHeader('Allow', 'POST');
    res.status(405).json({
      error: 'Method Not Allowed — POST {room_id, from_peer, to_peer, kind, payload, cap, sovereign?}',
      served_by: env.served_by,
      ts: env.ts,
    });
    return;
  }

  const body: unknown = req.body;
  if (!isObject(body)) {
    const env = envelope();
    res.status(400).json({
      error: 'Bad Request — body must be JSON object',
      served_by: env.served_by,
      ts: env.ts,
    });
    return;
  }
  const reqBody = body as PostSignalRequest;

  if (typeof reqBody.room_id !== 'string' || reqBody.room_id.length === 0) {
    const env = envelope();
    res.status(400).json({
      error: 'Bad Request — room_id must be non-empty string',
      served_by: env.served_by,
      ts: env.ts,
    });
    return;
  }
  if (typeof reqBody.from_peer !== 'string' || reqBody.from_peer.length === 0) {
    const env = envelope();
    res.status(400).json({
      error: 'Bad Request — from_peer must be non-empty string',
      served_by: env.served_by,
      ts: env.ts,
    });
    return;
  }
  if (typeof reqBody.to_peer !== 'string' || reqBody.to_peer.length === 0) {
    const env = envelope();
    res.status(400).json({
      error: 'Bad Request — to_peer must be non-empty string (or "*" for broadcast)',
      served_by: env.served_by,
      ts: env.ts,
    });
    return;
  }
  if (typeof reqBody.kind !== 'string' || !VALID_KINDS.has(reqBody.kind)) {
    const env = envelope();
    res.status(400).json({
      error: `Bad Request — kind must be one of [${[...VALID_KINDS].join(',')}]`,
      served_by: env.served_by,
      ts: env.ts,
    });
    return;
  }

  const room_id = reqBody.room_id;
  const from_peer = reqBody.from_peer;
  const to_peer = reqBody.to_peer;
  const kind = reqBody.kind;
  const payload = reqBody.payload;

  // Payload-size cap mirrors cssl-host-multiplayer-signaling::MsgErr::PayloadTooLarge.
  const sz = payloadByteSize(payload);
  if (sz > MAX_PAYLOAD_BYTES) {
    const env = envelope();
    res.status(400).json({
      error: `Bad Request — payload exceeds ${MAX_PAYLOAD_BYTES} bytes (got ${sz})`,
      served_by: env.served_by,
      ts: env.ts,
    });
    return;
  }

  const cap = typeof reqBody.cap === 'number' ? reqBody.cap : 0;
  const sovereignFlag = reqBody.sovereign === true;
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

  const inserted = await postSignal(room_id, from_peer, to_peer, kind, payload);
  const env = envelope();

  if (inserted === null) {
    const stubId = nextStubId();
    logEvent(
      auditEvent('mp.post_signal', cap, sovereignAllowed, 'ok', {
        room_id,
        signal_kind: kind,
        stub_id: stubId,
        stub: true,
      })
    );
    res.status(200).json({
      served_by: env.served_by,
      ts: env.ts,
      id: stubId,
      stub: true,
    });
    return;
  }

  logEvent(
    auditEvent('mp.post_signal', cap, sovereignAllowed, 'ok', {
      room_id,
      signal_kind: kind,
      id: inserted.id,
    })
  );
  res.status(200).json({
    served_by: env.served_by,
    ts: env.ts,
    id: inserted.id,
  });
}
