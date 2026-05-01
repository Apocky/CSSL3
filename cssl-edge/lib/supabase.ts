// cssl-edge · lib/supabase.ts
// Server-only Supabase client wrapper for multiplayer-signaling tables.
// Reads NEXT_PUBLIC_SUPABASE_URL + SUPABASE_ANON_KEY (server-only) at first
// call. Returns null when env-vars missing — routes fall back to mocked
// behavior (preserves stage-0 stub-friendly deploy semantics).
//
// Tables (from cssl-supabase/migrations/0004_signaling.sql) :
//   - public.multiplayer_rooms     (room descriptor)
//   - public.room_peers            (membership)
//   - public.signaling_messages    (offer/answer/ICE relay)
//   - public.room_state_snapshots  (out-of-scope here · wave-5b)

import type { SupabaseClient } from '@supabase/supabase-js';
import { createClient } from '@supabase/supabase-js';

// Row shapes matching the SQL DDL (see migrations/0004_signaling.sql).
export type SignalingMessageRow = {
  id: number;
  room_id: string;
  from_peer: string;
  to_peer: string;
  kind: string;
  payload: unknown;
  created_at: string;
  delivered: boolean;
};

export type RoomRow = {
  id: string;
  code: string;
  host_player_id: string;
  created_at: string;
  expires_at: string;
  max_peers: number;
  is_open: boolean;
  meta: Record<string, unknown>;
};

export type RoomPeerRow = {
  id: string;
  room_id: string;
  player_id: string;
  display_name: string | null;
  joined_at: string;
  last_seen_at: string;
  is_host: boolean;
};

// Singleton state — lazily initialized on first getSupabase() call.
let _client: SupabaseClient | null | undefined;

// Returns a configured client OR null when env-vars missing. Routes test for
// null and fall back to mocked behavior (no Supabase round-trip).
export function getSupabase(): SupabaseClient | null {
  if (_client !== undefined) return _client;
  const url = process.env['NEXT_PUBLIC_SUPABASE_URL'];
  const key = process.env['SUPABASE_ANON_KEY'];
  if (!url || !key) {
    _client = null;
    return null;
  }
  _client = createClient(url, key, {
    auth: { persistSession: false },
  });
  return _client;
}

// Test-only escape hatch : reset the singleton so per-test env-var changes take
// effect. Not exported in production paths.
export function _resetSupabaseForTests(): void {
  _client = undefined;
}

// ─── Room CRUD helpers ─────────────────────────────────────────────────────

// Create a new multiplayer room. Returns null when Supabase is unconfigured
// (caller may synthesize a stub row). `code` is generated server-side via the
// gen_room_code() pgsql function.
export async function createRoom(
  host_player_id: string,
  max_peers = 8
): Promise<RoomRow | null> {
  const sb = getSupabase();
  if (sb === null) return null;
  // gen_room_code() is the SECURITY-DEFINER function from 0004_signaling.sql.
  const { data: codeData, error: codeErr } = await sb.rpc('gen_room_code');
  if (codeErr || typeof codeData !== 'string') return null;
  const { data, error } = await sb
    .from('multiplayer_rooms')
    .insert({
      code: codeData,
      host_player_id,
      max_peers,
    })
    .select('*')
    .single();
  if (error || data === null) return null;
  return data as RoomRow;
}

// Look up a room by code AND record the joining peer. Returns the room +
// the synthesized peer-id (uuid) so the caller can address future signals.
export async function joinRoomByCode(
  code: string,
  player_id: string,
  display_name?: string
): Promise<{ room: RoomRow | null; peer_id: string }> {
  const sb = getSupabase();
  if (sb === null) return { room: null, peer_id: '' };
  // 1. resolve room by code
  const { data: roomData, error: roomErr } = await sb
    .from('multiplayer_rooms')
    .select('*')
    .eq('code', code)
    .eq('is_open', true)
    .maybeSingle();
  if (roomErr || roomData === null) return { room: null, peer_id: '' };
  const room = roomData as RoomRow;
  // 2. upsert peer ; UNIQUE(room_id, player_id) makes this idempotent
  const upsertPayload: Record<string, unknown> = {
    room_id: room.id,
    player_id,
    is_host: room.host_player_id === player_id,
  };
  if (typeof display_name === 'string') {
    upsertPayload['display_name'] = display_name;
  }
  const { data: peerData, error: peerErr } = await sb
    .from('room_peers')
    .upsert(upsertPayload, { onConflict: 'room_id,player_id' })
    .select('id')
    .single();
  if (peerErr || peerData === null) return { room, peer_id: '' };
  const peerRow = peerData as { id: string };
  return { room, peer_id: peerRow.id };
}

// List current peers in a room (used by /api/signaling/join-room response).
export async function listRoomPeers(room_id: string): Promise<RoomPeerRow[]> {
  const sb = getSupabase();
  if (sb === null) return [];
  const { data, error } = await sb
    .from('room_peers')
    .select('*')
    .eq('room_id', room_id);
  if (error || data === null) return [];
  return data as RoomPeerRow[];
}

// Insert a signaling envelope. Returns the new row's id (bigserial) or null
// if the insert failed (or Supabase is unconfigured).
export async function postSignal(
  room_id: string,
  from_peer: string,
  to_peer: string,
  kind: string,
  payload: unknown
): Promise<{ id: number } | null> {
  const sb = getSupabase();
  if (sb === null) return null;
  const { data, error } = await sb
    .from('signaling_messages')
    .insert({ room_id, from_peer, to_peer, kind, payload })
    .select('id')
    .single();
  if (error || data === null) return null;
  return data as { id: number };
}

// Poll undelivered signals addressed to peer_id (or `*` broadcast) since the
// supplied id-watermark. Marks fetched rows as delivered. Returns the rows
// + the new watermark (max id seen).
export async function pollSignals(
  room_id: string,
  peer_id: string,
  since: number
): Promise<{ signals: SignalingMessageRow[]; next_since: number }> {
  const sb = getSupabase();
  if (sb === null) return { signals: [], next_since: since };
  // Use OR(...) so we get both directly-addressed AND broadcast messages.
  const { data, error } = await sb
    .from('signaling_messages')
    .select('*')
    .eq('room_id', room_id)
    .or(`to_peer.eq.${peer_id},to_peer.eq.*`)
    .gt('id', since)
    .order('id', { ascending: true })
    .limit(256);
  if (error || data === null) return { signals: [], next_since: since };
  const rows = data as SignalingMessageRow[];
  if (rows.length === 0) return { signals: [], next_since: since };
  const ids = rows.map((r) => r.id);
  const next = ids.reduce((acc, n) => (n > acc ? n : acc), since);
  // Best-effort delivered=true update — don't gate the response on it.
  await sb
    .from('signaling_messages')
    .update({ delivered: true })
    .in('id', ids);
  return { signals: rows, next_since: next };
}
