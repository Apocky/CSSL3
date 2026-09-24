// The living room's one table, and the three identities that write to it.
//
// apocrypha_room_events is append-only. The site reads it with the service role because the
// table has no RLS story for anonymous readers: what a guest may see is decided HERE (room ==
// 'lobby'), not by a policy on the row. Owner rows never leave this module for a caller that
// did not prove the owner principal.

import { createHash } from 'node:crypto';
import type { NextApiRequest } from 'next';
import type { SupabaseClient } from '@supabase/supabase-js';

import { getAdminAuthorization } from '@/lib/admin-auth';
import { assertWorkerRequest, getApocryphaServiceClient } from '@/lib/apocrypha/job-control';
import { retryOnGatewayError } from '@/lib/apocrypha/worker-http';

export const ROOM_TABLE = 'apocrypha_room_events';
export const ROOMS = ['lobby', 'owner'] as const;
export type Room = typeof ROOMS[number];
// 'thought' is Apocrypha's reasoning, posted as its own row before the utterance it led to, so the
// reader watches the mind work in order instead of receiving a finished answer from nowhere.
export const KINDS = ['utterance', 'thought', 'presence', 'system'] as const;
export type Kind = typeof KINDS[number];

export const MAX_EVENTS_PAGE = 200;
export const DEFAULT_EVENTS_PAGE = 100;
export const MAX_SAY_CHARS = 4_000;
export const MAX_WORKER_BODY_CHARS = 16_000;
export const GUEST_MIN_GAP_MS = 2_000;

export interface RoomEvent {
  readonly id: number;
  readonly room: Room;
  readonly author: string;
  readonly kind: Kind;
  readonly body: string;
  readonly meta: Record<string, unknown>;
  readonly created_at: string;
}

export interface Presence {
  readonly state: string;
  readonly at: string;
}

export class RoomError extends Error {
  constructor(readonly status: number, readonly code: string, message: string) {
    super(message);
    this.name = 'RoomError';
  }
}

export function isRoom(value: unknown): value is Room {
  return typeof value === 'string' && (ROOMS as readonly string[]).includes(value);
}

export function isKind(value: unknown): value is Kind {
  return typeof value === 'string' && (KINDS as readonly string[]).includes(value);
}

export function parseRoom(value: unknown, fallback: Room = 'lobby'): Room {
  const raw = Array.isArray(value) ? value[0] : value;
  if (raw === undefined || raw === '') return fallback;
  if (!isRoom(raw)) throw new RoomError(400, 'ROOM_INVALID', 'room must be lobby or owner');
  return raw;
}

export function parseId(value: unknown, fallback = 0): number {
  const raw = Array.isArray(value) ? value[0] : value;
  if (raw === undefined || raw === '') return fallback;
  const n = Number(raw);
  if (!Number.isSafeInteger(n) || n < 0) throw new RoomError(400, 'ID_INVALID', 'after must be a non-negative integer');
  return n;
}

export function parseLimit(value: unknown, fallback = DEFAULT_EVENTS_PAGE): number {
  const raw = Array.isArray(value) ? value[0] : value;
  if (raw === undefined || raw === '') return fallback;
  const n = Number(raw);
  if (!Number.isInteger(n) || n < 1) throw new RoomError(400, 'LIMIT_INVALID', 'limit must be a positive integer');
  return Math.min(n, MAX_EVENTS_PAGE);
}

function record(value: unknown): Record<string, unknown> {
  return value !== null && typeof value === 'object' && !Array.isArray(value)
    ? value as Record<string, unknown> : {};
}

export function toEvent(row: unknown): RoomEvent | null {
  const r = record(row);
  const id = Number(r.id);
  if (!Number.isSafeInteger(id) || !isRoom(r.room) || !isKind(r.kind)) return null;
  return {
    id,
    room: r.room,
    author: typeof r.author === 'string' ? r.author : '',
    kind: r.kind,
    body: typeof r.body === 'string' ? r.body : '',
    meta: record(r.meta),
    created_at: typeof r.created_at === 'string' ? r.created_at : new Date(0).toISOString(),
  };
}

const COLUMNS = 'id,room,author,kind,body,meta,created_at';

function dbError(error: { code?: string | null; message?: string | null } | null, op: string): never {
  throw new RoomError(503, 'ROOM_STORE_UNAVAILABLE', `${op} failed: ${error?.code ?? 'unreachable'}`);
}

export function roomClient(): SupabaseClient {
  try {
    return getApocryphaServiceClient();
  } catch {
    throw new RoomError(503, 'ROOM_STORE_UNCONFIGURED', 'The room store is not configured.');
  }
}

/**
 * Events in a room. With `after` > 0: the rows strictly after it, oldest first, bounded.
 * Without it: the newest `limit` rows, still returned oldest first, so a fresh page opens at
 * the present rather than at the beginning of time.
 */
export async function listEvents(
  room: Room,
  after: number,
  limit: number,
  client: SupabaseClient = roomClient(),
): Promise<RoomEvent[]> {
  const { data, error } = await retryOnGatewayError(() => {
    const base = client.from(ROOM_TABLE).select(COLUMNS).eq('room', room);
    return after > 0
      ? base.gt('id', after).order('id', { ascending: true }).limit(limit)
      : base.order('id', { ascending: false }).limit(limit);
  });
  if (error) dbError(error, 'list');
  const rows = (Array.isArray(data) ? data : []).map(toEvent).filter((e): e is RoomEvent => e !== null);
  return after > 0 ? rows : rows.reverse();
}

export async function newestPresence(room: Room, client: SupabaseClient = roomClient()): Promise<Presence | null> {
  const { data, error } = await retryOnGatewayError(() => client
    .from(ROOM_TABLE)
    .select('body,created_at')
    .eq('room', room)
    .eq('kind', 'presence')
    .order('id', { ascending: false })
    .limit(1));
  if (error) dbError(error, 'presence');
  const row = record(Array.isArray(data) ? data[0] : data);
  if (typeof row.body !== 'string') return null;
  return { state: row.body, at: typeof row.created_at === 'string' ? row.created_at : new Date().toISOString() };
}

/** Rows since `after` written by anyone but Apocrypha, in both rooms. The worker's inbox. */
export async function listInbox(after: number, limit: number, client: SupabaseClient = roomClient()): Promise<RoomEvent[]> {
  const { data, error } = await retryOnGatewayError(() => client
    .from(ROOM_TABLE)
    .select(COLUMNS)
    .gt('id', after)
    .neq('author', 'apocrypha')
    .order('id', { ascending: true })
    .limit(limit));
  if (error) dbError(error, 'inbox');
  return (Array.isArray(data) ? data : []).map(toEvent).filter((e): e is RoomEvent => e !== null);
}

export async function insertEvent(
  input: { room: Room; author: string; kind: Kind; body: string; meta: Record<string, unknown> },
  client: SupabaseClient = roomClient(),
): Promise<RoomEvent> {
  const { data, error } = await client
    .from(ROOM_TABLE)
    .insert({ room: input.room, author: input.author, kind: input.kind, body: input.body, meta: input.meta })
    .select(COLUMNS)
    .limit(1);
  if (error) dbError(error, 'insert');
  const event = toEvent(Array.isArray(data) ? data[0] : data);
  if (event === null) throw new RoomError(503, 'ROOM_STORE_UNAVAILABLE', 'insert returned no row');
  return event;
}

/** When this author last wrote anywhere, or null. Drives the guest gap. */
export async function lastWriteAt(author: string, client: SupabaseClient = roomClient()): Promise<number | null> {
  const { data, error } = await client
    .from(ROOM_TABLE)
    .select('created_at')
    .eq('author', author)
    .order('id', { ascending: false })
    .limit(1);
  if (error) dbError(error, 'last-write');
  const row = record(Array.isArray(data) ? data[0] : data);
  const at = typeof row.created_at === 'string' ? Date.parse(row.created_at) : Number.NaN;
  return Number.isFinite(at) ? at : null;
}

/**
 * Owner check, cached briefly per access token. The owner room polls every 1.5 s and each
 * uncached check is a round trip to Supabase auth; sixty seconds of trust in a token that was
 * good a moment ago is a smaller risk than forty auth calls a minute.
 */
const OWNER_CACHE_MS = 60_000;
const ownerCache = new Map<string, { authorized: boolean; until: number }>();

function tokenKey(req: NextApiRequest): string | null {
  const auth = Array.isArray(req.headers.authorization) ? req.headers.authorization[0] : req.headers.authorization;
  const cookie = Array.isArray(req.headers.cookie) ? req.headers.cookie[0] : req.headers.cookie;
  const source = auth?.startsWith('Bearer ') ? auth : cookie?.match(/(?:^|;\s*)(?:__Host-)?apocky-access-token=([^;]+)/)?.[1];
  return source ? createHash('sha256').update(source).digest('hex') : null;
}

export async function isOwner(req: NextApiRequest): Promise<boolean> {
  const key = tokenKey(req);
  if (key === null) return false;
  const now = Date.now();
  const hit = ownerCache.get(key);
  if (hit && hit.until > now) return hit.authorized;
  const auth = await getAdminAuthorization(req);
  const authorized = auth.authorized && auth.user !== null;
  if (ownerCache.size > 256) ownerCache.clear();
  ownerCache.set(key, { authorized, until: now + OWNER_CACHE_MS });
  return authorized;
}

export function resetOwnerCacheForTests(): void {
  ownerCache.clear();
}

/**
 * The same admission the job control plane applies: a Bearer node token of the accepted shape,
 * verified against the node row by the SECURITY DEFINER RPC. The node id travels in a header on
 * GET and in the body on POST. Anything else is 401 -- there is no anonymous worker.
 */
export async function requireRoomWorker(req: NextApiRequest, client: SupabaseClient = roomClient()): Promise<string> {
  let token: string;
  try {
    token = assertWorkerRequest(req.headers.authorization);
  } catch {
    throw new RoomError(401, 'WORKER_UNAUTHORIZED', 'Worker token required.');
  }
  const header = req.headers['x-apocrypha-node-id'];
  const fromHeader = Array.isArray(header) ? header[0] : header;
  const body = record(req.body);
  const nodeId = (fromHeader ?? (typeof body.node_id === 'string' ? body.node_id : ''))?.trim() ?? '';
  if (!nodeId || nodeId.length > 160) throw new RoomError(401, 'WORKER_UNAUTHORIZED', 'Worker node id required.');
  const { data, error } = await retryOnGatewayError(() => client.rpc('apocrypha_require_worker', {
    p_node_id: nodeId,
    p_node_token: token,
    p_require_active: true,
  }));
  if (error) throw new RoomError(401, 'WORKER_UNAUTHORIZED', 'Worker not admitted.');
  const node = record(Array.isArray(data) ? data[0] : data);
  if (typeof node.id !== 'string') throw new RoomError(401, 'WORKER_UNAUTHORIZED', 'Worker not admitted.');
  return node.id;
}

export function guestAuthor(guestId: string): string {
  // The cookie value itself never reaches a row that every lobby reader can fetch; a stranger
  // who could read it could replay it and speak as that guest.
  const digest = createHash('sha256').update('APOCRYPHA-ROOM-GUEST-v1\0', 'utf8').update(guestId, 'utf8').digest('hex');
  return `guest:${digest.slice(0, 16)}`;
}
