// cssl-edge · /api/signaling/create-room
// POST handler · creates a new multiplayer-signaling room.
//
// Auth :
//   - Cap-bit MP_CAP_HOST_ROOM = 1 REQUIRED · DEFAULT-DENY when caps=0
//   - Sovereign bypass : sovereign:true + x-loa-sovereign-cap header → allowed
//
// Body  : { host_player_id: string, max_peers?: number, cap?: number, sovereign?: boolean }
// 200   : { served_by, ts, room_id, code, host_player_id, expires_at, stub? }
// 403   : { error, served_by, ts }
// Audit : { kind: 'mp.create_room', cap, sovereign, status, room_id?, host? }

import type { NextApiRequest, NextApiResponse } from 'next';
import { auditEvent, deny, logEvent } from '@/lib/audit';
import { isSovereignFromIncoming } from '@/lib/sovereign';
import { envelope, logHit } from '@/lib/response';
import { MP_CAP_HOST_ROOM, checkCap } from '@/lib/cap';
import { createRoom } from '@/lib/supabase';

interface CreateRoomRequest {
  host_player_id?: unknown;
  max_peers?: unknown;
  cap?: unknown;
  sovereign?: unknown;
}

interface CreateRoomOk {
  served_by: string;
  ts: string;
  room_id: string;
  code: string;
  host_player_id: string;
  expires_at: string;
  stub?: true;
}

interface CreateRoomError {
  error: string;
  served_by: string;
  ts: string;
}

function isObject(b: unknown): b is Record<string, unknown> {
  return typeof b === 'object' && b !== null;
}

// Stub room synthesizer — used when Supabase env-vars are missing so the route
// remains demo-able / testable without a live DB.
function stubRoom(host: string): { id: string; code: string; expires_at: string } {
  const r = Math.floor(Math.random() * 0xffffffff).toString(16).padStart(8, '0');
  const code = ('STUB' + r.slice(0, 2)).toUpperCase();
  const id = `00000000-0000-0000-0000-${r.padStart(12, '0')}`;
  const expires_at = new Date(Date.now() + 4 * 60 * 60 * 1000).toISOString();
  return { id, code, expires_at };
}

export default async function handler(
  req: NextApiRequest,
  res: NextApiResponse<CreateRoomOk | CreateRoomError>
): Promise<void> {
  logHit('signaling.create-room', { method: req.method ?? 'GET' });

  if (req.method !== 'POST') {
    const env = envelope();
    res.setHeader('Allow', 'POST');
    res.status(405).json({
      error: 'Method Not Allowed — POST {host_player_id, max_peers?, cap, sovereign?}',
      served_by: env.served_by,
      ts: env.ts,
    });
    return;
  }

  const body: unknown = req.body;
  if (!isObject(body)) {
    const env = envelope();
    res.status(400).json({
      error: 'Bad Request — body must be JSON {host_player_id, max_peers?, cap, sovereign?}',
      served_by: env.served_by,
      ts: env.ts,
    });
    return;
  }
  const reqBody = body as CreateRoomRequest;

  if (typeof reqBody.host_player_id !== 'string' || reqBody.host_player_id.length === 0) {
    const env = envelope();
    res.status(400).json({
      error: 'Bad Request — host_player_id must be non-empty string',
      served_by: env.served_by,
      ts: env.ts,
    });
    return;
  }
  const host_player_id = reqBody.host_player_id;

  // max_peers default 8 · validated to fit DDL CHECK 2..32
  let max_peers = 8;
  if (typeof reqBody.max_peers === 'number') {
    if (!Number.isInteger(reqBody.max_peers) || reqBody.max_peers < 2 || reqBody.max_peers > 32) {
      const env = envelope();
      res.status(400).json({
        error: 'Bad Request — max_peers must be integer in [2, 32]',
        served_by: env.served_by,
        ts: env.ts,
      });
      return;
    }
    max_peers = reqBody.max_peers;
  }

  const cap = typeof reqBody.cap === 'number' ? reqBody.cap : 0;
  const sovereignFlag = reqBody.sovereign === true;
  const sovereignAllowed = isSovereignFromIncoming(req.headers, sovereignFlag);

  // Cap-gate.
  const decision = checkCap(cap, MP_CAP_HOST_ROOM, sovereignAllowed);
  if (!decision.ok) {
    const reason = decision.reason ?? 'cap MP_CAP_HOST_ROOM=0x1 required';
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

  // Persist via Supabase OR fall through to stub when env-vars missing.
  const created = await createRoom(host_player_id, max_peers);
  const env = envelope();

  if (created === null) {
    const stub = stubRoom(host_player_id);
    logEvent(
      auditEvent('mp.create_room', cap, sovereignAllowed, 'ok', {
        host: host_player_id,
        room_id: stub.id,
        stub: true,
      })
    );
    res.status(200).json({
      served_by: env.served_by,
      ts: env.ts,
      room_id: stub.id,
      code: stub.code,
      host_player_id,
      expires_at: stub.expires_at,
      stub: true,
    });
    return;
  }

  logEvent(
    auditEvent('mp.create_room', cap, sovereignAllowed, 'ok', {
      host: host_player_id,
      room_id: created.id,
    })
  );
  res.status(200).json({
    served_by: env.served_by,
    ts: env.ts,
    room_id: created.id,
    code: created.code,
    host_player_id: created.host_player_id,
    expires_at: created.expires_at,
  });
}
