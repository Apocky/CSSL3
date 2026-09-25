// Rooms per account (migration 0062): every signed-in account has a private room with Apocrypha,
// and any account can own lobbies that others join only by invitation. Membership, friends and
// invites live in the database; this module resolves the verified user and calls it.

import { createHash, randomBytes } from 'node:crypto';
import type { SupabaseClient } from '@supabase/supabase-js';

import { RoomError, roomClient } from './store';
import type { Speaker } from './turn';

export interface RoomSummary {
  readonly key: string;
  readonly kind: 'private' | 'lobby';
  readonly title: string;
  readonly role: 'owner' | 'member';
  readonly members: number;
}

function fail(error: { code?: string | null; message?: string | null } | null, fallback: string): never {
  const code = error?.code ?? '';
  if (code === 'P4031') throw new RoomError(403, 'NOT_A_MEMBER', 'You are not in that room.');
  if (code === 'P4040') throw new RoomError(410, 'INVITE_EXPIRED', 'That invitation has expired or was used up.');
  if (code === 'P4290') throw new RoomError(429, 'ROOM_LIMIT', error?.message ?? 'Limit reached.');
  if (code === '22023') throw new RoomError(400, 'ROOM_INVALID', error?.message ?? 'That request is invalid.');
  throw new RoomError(503, 'ROOM_UNAVAILABLE', fallback);
}

function rows(data: unknown): Array<Record<string, unknown>> {
  return Array.isArray(data) ? data as Array<Record<string, unknown>> : [];
}

export function requireUser(speaker: Speaker): string {
  if (speaker.kind === 'guest' || !speaker.authUserId) throw new RoomError(401, 'SIGN_IN_REQUIRED', 'Sign in to use the room.');
  return speaker.authUserId;
}

/** The speaker's private room key, created (with their display name) on first use. */
export async function ensurePrivate(speaker: Speaker, client: SupabaseClient = roomClient()): Promise<string> {
  const userId = requireUser(speaker);
  const { data, error } = await client.rpc('apocrypha_room_ensure_private', { p_user_id: userId, p_is_owner: speaker.kind === 'owner' });
  if (error || typeof data !== 'string') fail(error, 'Your room could not be opened.');
  if (speaker.kind !== 'owner') {
    const fallback = (speaker.email ?? '').split('@')[0]?.slice(0, 40) || 'member';
    const { data: existing } = await client.from('apocrypha_room_profile').select('user_id').eq('user_id', userId).maybeSingle();
    if (!existing) await client.rpc('apocrypha_room_set_profile', { p_user_id: userId, p_author: speaker.author, p_display_name: fallback });
  }
  return data as string;
}

/** 'me' means the speaker's own private room; anything else must be a room they belong to. */
export async function resolveRoomKey(speaker: Speaker, requested: string, client: SupabaseClient = roomClient()): Promise<{ key: string; role: string; kind: string }> {
  const userId = requireUser(speaker);
  const key = requested === 'me' || requested === '' ? await ensurePrivate(speaker, client) : requested;
  const { data, error } = await client.rpc('apocrypha_room_role', { p_room_key: key, p_user_id: userId });
  if (error) fail(error, 'The room could not be opened.');
  if (typeof data !== 'string') throw new RoomError(403, 'NOT_A_MEMBER', 'You are not in that room.');
  const { data: room } = await client.from('apocrypha_room').select('kind').eq('key', key).maybeSingle();
  return { key, role: data, kind: String(room?.kind ?? 'lobby') };
}

export async function listRooms(speaker: Speaker, client: SupabaseClient = roomClient()): Promise<RoomSummary[]> {
  const userId = requireUser(speaker);
  await ensurePrivate(speaker, client);
  const { data, error } = await client.rpc('apocrypha_room_list', { p_user_id: userId });
  if (error) fail(error, 'Your rooms could not be listed.');
  return rows(data).map((r) => ({
    key: String(r.key), kind: r.kind === 'private' ? 'private' : 'lobby', title: String(r.title),
    role: r.role === 'owner' ? 'owner' : 'member', members: Number(r.members ?? 1),
  }));
}

export async function createLobby(speaker: Speaker, title: string, client: SupabaseClient = roomClient()): Promise<string> {
  const { data, error } = await client.rpc('apocrypha_room_create_lobby', { p_user_id: requireUser(speaker), p_title: title.slice(0, 80) });
  if (error || typeof data !== 'string') fail(error, 'The lobby could not be created.');
  return data as string;
}

export async function leaveRoom(speaker: Speaker, key: string, client: SupabaseClient = roomClient()): Promise<void> {
  const { error } = await client.rpc('apocrypha_room_leave', { p_user_id: requireUser(speaker), p_room_key: key });
  if (error) fail(error, 'Could not leave that lobby.');
}

const tokenHash = (token: string) => createHash('sha256').update('APOCRYPHA-ROOM-INVITE-v1\0', 'utf8').update(token, 'utf8').digest('hex');

/** A single-use-ish link (10 uses, 7 days). Only the hash is stored. */
export async function createInvite(speaker: Speaker, key: string, client: SupabaseClient = roomClient()): Promise<{ token: string; expires_at: string }> {
  const token = randomBytes(18).toString('base64url');
  const { data, error } = await client.rpc('apocrypha_room_invite_create', { p_user_id: requireUser(speaker), p_room_key: key, p_token_hash: tokenHash(token) });
  if (error) fail(error, 'The invitation could not be created.');
  return { token, expires_at: String(data) };
}

export async function acceptInvite(speaker: Speaker, token: string, client: SupabaseClient = roomClient()): Promise<{ key: string; title: string }> {
  if (!/^[A-Za-z0-9_-]{16,64}$/.test(token)) throw new RoomError(400, 'INVITE_INVALID', 'That invitation link is not valid.');
  await ensurePrivate(speaker, client);
  const { data, error } = await client.rpc('apocrypha_room_invite_accept', { p_user_id: requireUser(speaker), p_token_hash: tokenHash(token) });
  if (error) fail(error, 'The invitation could not be accepted.');
  const row = rows(data)[0];
  if (!row) throw new RoomError(410, 'INVITE_EXPIRED', 'That invitation has expired or was used up.');
  return { key: String(row.room_key), title: String(row.title) };
}

export async function listFriends(speaker: Speaker, client: SupabaseClient = roomClient()) {
  const { data, error } = await client.rpc('apocrypha_friends', { p_user_id: requireUser(speaker) });
  if (error) fail(error, 'Your friends could not be listed.');
  return rows(data).map((r) => ({ user_id: String(r.user_id), display_name: String(r.display_name) }));
}

export async function addFriendToRoom(speaker: Speaker, key: string, friendId: string, client: SupabaseClient = roomClient()): Promise<void> {
  const { error } = await client.rpc('apocrypha_room_add_friend', { p_user_id: requireUser(speaker), p_room_key: key, p_friend: friendId });
  if (error) fail(error, 'Could not add that friend.');
}

export async function listMembers(key: string, client: SupabaseClient = roomClient()) {
  const { data, error } = await client.rpc('apocrypha_room_members', { p_room_key: key });
  if (error) fail(error, 'Members could not be listed.');
  return rows(data).map((r) => ({ user_id: String(r.user_id), role: String(r.role), author: typeof r.author === 'string' ? r.author : null, display_name: String(r.display_name) }));
}

export async function namesFor(authors: readonly string[], client: SupabaseClient = roomClient()): Promise<Record<string, string>> {
  const unique = [...new Set(authors.filter((a) => a !== 'apocrypha'))];
  if (unique.length === 0) return {};
  const { data } = await client.rpc('apocrypha_room_names', { p_authors: unique });
  return Object.fromEntries(rows(data).map((r) => [String(r.author), String(r.display_name)]));
}

export async function setDisplayName(speaker: Speaker, name: string, client: SupabaseClient = roomClient()): Promise<void> {
  const clean = name.trim().slice(0, 40);
  if (!clean) throw new RoomError(400, 'NAME_EMPTY', 'Enter a name.');
  const { error } = await client.rpc('apocrypha_room_set_profile', { p_user_id: requireUser(speaker), p_author: speaker.author, p_display_name: clean });
  if (error) fail(error, 'Your name could not be saved.');
}
