// cssl-edge · /api/signaling/join-room
// POST handler · joins an existing multiplayer-signaling room by code.
//
// Auth :
//   - Cap-bit MP_CAP_JOIN_ROOM = 2 REQUIRED · DEFAULT-DENY when caps=0
//   - Sovereign bypass : sovereign:true + x-loa-sovereign-cap header → allowed
//
// Body  : { code: string, player_id: string, display_name?: string, cap?: number, sovereign?: boolean }
// 200   : { served_by, ts, room_id, peer_id, peers: [...], stub? }
// 400   : invalid code-format / missing fields
// 403   : cap-denied
// Audit : { kind: 'mp.join_room', cap, sovereign, status, room_id?, peer_id? }

import type { NextApiRequest, NextApiResponse } from 'next';
import { auditEvent, deny, logEvent } from '@/lib/audit';
import { isSovereignFromIncoming } from '@/lib/sovereign';
import { envelope, logHit } from '@/lib/response';
import { MP_CAP_JOIN_ROOM, checkCap } from '@/lib/cap';
import { joinRoomByCode, listRoomPeers } from '@/lib/supabase';

interface JoinRoomRequest {
  code?: unknown;
  player_id?: unknown;
  display_name?: unknown;
  cap?: unknown;
  sovereign?: unknown;
}

interface PeerSummary {
  player_id: string;
  display_name: string | null;
  is_host: boolean;
  joined_at: string;
}

interface JoinRoomOk {
  served_by: string;
  ts: string;
  room_id: string;
  peer_id: string;
  peers: PeerSummary[];
  stub?: true;
}

interface JoinRoomError {
  error: string;
  served_by: string;
  ts: string;
}

function isObject(b: unknown): b is Record<string, unknown> {
  return typeof b === 'object' && b !== null;
}

// Code format mirrors gen_room_code() output : 6 chars from the legibility
// alphabet `ABCDEFGHJKLMNPQRSTUVWXYZ23456789` (no I/O/0/1).
// Stub codes (`STUB**`) also pass — needed for env-less smoke tests.
const CODE_REGEX = /^[A-HJ-NP-Z2-9]{6}$/;
const STUB_CODE_REGEX = /^STUB[A-Z0-9]{2}$/;

function isValidCode(code: string): boolean {
  return CODE_REGEX.test(code) || STUB_CODE_REGEX.test(code);
}

function stubJoinResponse(code: string, player_id: string): {
  room_id: string;
  peer_id: string;
} {
  // Synth a uuid-shaped pair derived from code+player so multiple stub-runs
  // are stable per-input but distinct across inputs.
  let h = 0;
  const seed = `${code}::${player_id}`;
  for (let i = 0; i < seed.length; i++) {
    h = ((h << 5) - h + seed.charCodeAt(i)) | 0;
  }
  const hex = (h >>> 0).toString(16).padStart(8, '0');
  return {
    room_id: `00000000-0000-0000-0000-${hex.padStart(12, '0')}`,
    peer_id: `00000000-0000-0000-0000-${hex.padStart(12, '0').split('').reverse().join('')}`,
  };
}

export default async function handler(
  req: NextApiRequest,
  res: NextApiResponse<JoinRoomOk | JoinRoomError>
): Promise<void> {
  logHit('signaling.join-room', { method: req.method ?? 'GET' });

  if (req.method !== 'POST') {
    const env = envelope();
    res.setHeader('Allow', 'POST');
    res.status(405).json({
      error: 'Method Not Allowed — POST {code, player_id, display_name?, cap, sovereign?}',
      served_by: env.served_by,
      ts: env.ts,
    });
    return;
  }

  const body: unknown = req.body;
  if (!isObject(body)) {
    const env = envelope();
    res.status(400).json({
      error: 'Bad Request — body must be JSON {code, player_id, display_name?, cap, sovereign?}',
      served_by: env.served_by,
      ts: env.ts,
    });
    return;
  }
  const reqBody = body as JoinRoomRequest;

  if (typeof reqBody.code !== 'string' || !isValidCode(reqBody.code)) {
    const env = envelope();
    res.status(400).json({
      error: 'Bad Request — code must be 6-char alphanumeric (legibility alphabet)',
      served_by: env.served_by,
      ts: env.ts,
    });
    return;
  }
  if (typeof reqBody.player_id !== 'string' || reqBody.player_id.length === 0) {
    const env = envelope();
    res.status(400).json({
      error: 'Bad Request — player_id must be non-empty string',
      served_by: env.served_by,
      ts: env.ts,
    });
    return;
  }

  const code = reqBody.code;
  const player_id = reqBody.player_id;
  const display_name =
    typeof reqBody.display_name === 'string' ? reqBody.display_name : undefined;
  const cap = typeof reqBody.cap === 'number' ? reqBody.cap : 0;
  const sovereignFlag = reqBody.sovereign === true;
  const sovereignAllowed = isSovereignFromIncoming(req.headers, sovereignFlag);

  // Cap-gate.
  const decision = checkCap(cap, MP_CAP_JOIN_ROOM, sovereignAllowed);
  if (!decision.ok) {
    const reason = decision.reason ?? 'cap MP_CAP_JOIN_ROOM=0x2 required';
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

  const { room, peer_id } = await joinRoomByCode(code, player_id, display_name);
  const env = envelope();

  if (room === null) {
    // Stub path : Supabase unconfigured OR room not found. We can't tell the
    // difference cheaply ; for stage-0 we treat unconfigured-env as "stub it"
    // and configured-but-missing as "404 surface". Detect by re-querying env.
    const supabaseConfigured = Boolean(
      process.env['NEXT_PUBLIC_SUPABASE_URL'] && process.env['SUPABASE_ANON_KEY']
    );
    if (supabaseConfigured) {
      logEvent(
        auditEvent('mp.join_room', cap, sovereignAllowed, 'error', {
          code,
          player_id,
          reason: 'room not found or closed',
        })
      );
      res.status(404).json({
        error: 'room not found or closed',
        served_by: env.served_by,
        ts: env.ts,
      });
      return;
    }
    const stub = stubJoinResponse(code, player_id);
    logEvent(
      auditEvent('mp.join_room', cap, sovereignAllowed, 'ok', {
        code,
        player_id,
        room_id: stub.room_id,
        peer_id: stub.peer_id,
        stub: true,
      })
    );
    res.status(200).json({
      served_by: env.served_by,
      ts: env.ts,
      room_id: stub.room_id,
      peer_id: stub.peer_id,
      peers: [
        {
          player_id,
          display_name: display_name ?? null,
          is_host: false,
          joined_at: env.ts,
        },
      ],
      stub: true,
    });
    return;
  }

  // List current peers for the response.
  const peerRows = await listRoomPeers(room.id);
  const peers: PeerSummary[] = peerRows.map((p) => ({
    player_id: p.player_id,
    display_name: p.display_name,
    is_host: p.is_host,
    joined_at: p.joined_at,
  }));

  logEvent(
    auditEvent('mp.join_room', cap, sovereignAllowed, 'ok', {
      code,
      player_id,
      room_id: room.id,
      peer_id,
      peer_count: peers.length,
    })
  );
  res.status(200).json({
    served_by: env.served_by,
    ts: env.ts,
    room_id: room.id,
    peer_id,
    peers,
  });
}
