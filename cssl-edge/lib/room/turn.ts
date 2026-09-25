// A room message is a job (migration 0061).
//
// apocrypha_room_say / apocrypha_room_say_guest write the human row AND enqueue its job in one
// database transaction, so a message either has a job or was never posted. This module decides
// the three things the database cannot: WHO is speaking (owner, signed-in member, guest), which
// LANE they may use (flagship = Opus on the Vercel runner, local = the PC worker), and what the
// job CARRIES (the room persona, the room's recent conversation, any attachments).

import { createHash, randomUUID } from 'node:crypto';
import type { NextApiRequest } from 'next';
import type { SupabaseClient } from '@supabase/supabase-js';

import { getAdminAuthorization } from '@/lib/admin-auth';
import { presentable } from '@/lib/apocrypha/deliberation';
import { guestCookie, guestSubjectHash, newGuestId, readGuestCookie } from '@/lib/apocrypha/guest-chat';
import {
  APOCRYPHA_MEMORY_MANIFEST_HASH,
  APOCRYPHA_MODEL_ALIAS,
  APOCRYPHA_PROFILE_HASH,
  APOCRYPHA_TOOL_REGISTRY_VERSION,
  ensureOwnerIdentity,
} from '@/lib/apocrypha/job-control';
import { gatewayToken } from '@/lib/apocrypha/vercel-runner';
import { roomPersona } from './persona';
import { RoomError, guestAuthor, listEvents, roomClient, toEvent, type Room, type RoomEvent } from './store';

export type EngineLane = 'local' | 'flagship';
export const ROOM_TOOLS = ['image', 'web'] as const;
export type RoomTool = typeof ROOM_TOOLS[number];

const HISTORY_ROWS = 24;
const GUEST_HISTORY_TURNS = 12;
const MAX_ATTACHMENTS = 8;

export interface Speaker {
  readonly kind: 'owner' | 'member' | 'guest';
  /** The author label on the row. Never an email or a raw id: every lobby reader can see it. */
  readonly author: string;
  readonly authUserId: string | null;
  readonly guestId: string | null;
  /** A Set-Cookie header to send when a guest cookie was minted for this request. */
  readonly setCookie: string | null;
}

export function memberAuthor(authUserId: string): string {
  const digest = createHash('sha256').update('APOCRYPHA-ROOM-MEMBER-v1\0', 'utf8').update(authUserId, 'utf8').digest('hex');
  return `member:${digest.slice(0, 16)}`;
}

/** Owner, signed-in member, or guest -- decided from the session, never from the request body. */
export async function resolveSpeaker(req: NextApiRequest, options: { mintGuest: boolean }): Promise<Speaker> {
  const auth = await getAdminAuthorization(req);
  if (auth.user && auth.authorized) {
    return { kind: 'owner', author: 'apocky', authUserId: auth.user.id, guestId: null, setCookie: null };
  }
  if (auth.user) {
    return { kind: 'member', author: memberAuthor(auth.user.id), authUserId: auth.user.id, guestId: null, setCookie: null };
  }
  const existing = readGuestCookie(req.headers.cookie);
  if (existing) return { kind: 'guest', author: guestAuthor(existing), authUserId: null, guestId: existing, setCookie: null };
  if (!options.mintGuest) return { kind: 'guest', author: '', authUserId: null, guestId: null, setCookie: null };
  const minted = newGuestId();
  return {
    kind: 'guest',
    author: guestAuthor(minted),
    authUserId: null,
    guestId: minted,
    setCookie: guestCookie(minted, process.env.NODE_ENV === 'production'),
  };
}

/** Whether this speaker's plan includes the flagship lane. The database checks the answer again. */
export async function flagshipAllowed(speaker: Speaker, client: SupabaseClient = roomClient()): Promise<boolean> {
  if (speaker.kind === 'owner') return true;
  if (speaker.kind !== 'member' || !speaker.authUserId) return false;
  const { data, error } = await client.rpc('apocrypha_member_has_flagship', { p_verified_auth_user_id: speaker.authUserId });
  if (error) return false;
  return data === true;
}

/** The OIDC token Vercel hands a function, when the project's federation is on. */
export function oidcFromRequest(req: NextApiRequest): string | undefined {
  const header = req.headers['x-vercel-oidc-token'];
  const value = Array.isArray(header) ? header[0] : header;
  return typeof value === 'string' && value.trim() !== '' ? value.trim() : undefined;
}

/** Whether a flagship turn could be answered right now: the runner needs gateway credentials. */
export function flagshipReady(req: NextApiRequest): boolean {
  return gatewayToken({ ...process.env, VERCEL_OIDC_TOKEN: process.env.VERCEL_OIDC_TOKEN || oidcFromRequest(req) }) !== null;
}

function speakerLabel(author: string): string {
  if (author === 'apocky') return 'apocky';
  if (author.startsWith('guest:')) return `guest-${author.slice(6, 10)}`;
  if (author.startsWith('member:')) return `member-${author.slice(7, 11)}`;
  return author;
}

interface Turn { role: 'user' | 'assistant'; content: string }

/** The room's recent conversation as chat turns. In the lobby every human turn carries its speaker. */
export function roomHistory(rows: readonly RoomEvent[], room: Room): Turn[] {
  const turns: Turn[] = [];
  for (const row of rows) {
    if (row.kind !== 'utterance' || row.body.trim() === '') continue;
    if (row.author === 'apocrypha') {
      const shown = presentable(row.body);
      if (shown.withheld === null) turns.push({ role: 'assistant', content: shown.text.slice(0, 8_000) });
      continue;
    }
    const content = room === 'lobby' ? `${speakerLabel(row.author)}: ${row.body}` : row.body;
    turns.push({ role: 'user', content: content.slice(0, 8_000) });
  }
  return turns;
}

interface AttachmentForJob { id: string; name: string; mime: string; bytes: number; text: string }

/** A signed-in speaker's own attachments, by id, with the text the model may read. */
async function loadAttachments(speaker: Speaker, ids: readonly string[], client: SupabaseClient): Promise<AttachmentForJob[]> {
  if (ids.length === 0) return [];
  if (!speaker.authUserId) throw new RoomError(403, 'ATTACHMENTS_NEED_SIGN_IN', 'Sign in to attach files.');
  const { data: principal, error: principalError } = await client.rpc('apocrypha_ensure_member_principal', {
    p_verified_auth_user_id: speaker.authUserId,
  });
  const owner = (Array.isArray(principal) ? principal[0] : principal) as { tenant_id?: string; principal_id?: string } | null;
  if (principalError || !owner?.tenant_id || !owner.principal_id) {
    throw new RoomError(503, 'ATTACHMENTS_UNAVAILABLE', 'Attachments could not be read right now.');
  }
  const { data, error } = await client
    .from('apocrypha_member_chat_attachment')
    .select('id,file_name,mime_type,byte_size,extracted_text')
    .eq('tenant_id', owner.tenant_id)
    .eq('principal_id', owner.principal_id)
    .in('id', [...ids]);
  if (error) throw new RoomError(503, 'ATTACHMENTS_UNAVAILABLE', 'Attachments could not be read right now.');
  const rows = (Array.isArray(data) ? data : []) as Array<Record<string, unknown>>;
  if (rows.length !== ids.length) throw new RoomError(403, 'ATTACHMENT_NOT_YOURS', 'An attachment does not belong to you.');
  return rows.map((row) => ({
    id: String(row.id),
    name: String(row.file_name ?? 'file'),
    mime: String(row.mime_type ?? 'application/octet-stream'),
    bytes: Number(row.byte_size ?? 0),
    text: typeof row.extracted_text === 'string' ? row.extracted_text.slice(0, 32_768) : '',
  }));
}

export interface SayInput {
  readonly room: Room;
  readonly body: string;
  readonly lane: EngineLane;
  readonly attachmentIds: readonly string[];
  readonly tools: readonly RoomTool[];
}

export interface SayReceipt {
  readonly event: RoomEvent;
  readonly job: { readonly id: string; readonly status: string; readonly lane: EngineLane };
}

function sayFailure(error: { code?: string | null; message?: string | null } | null): never {
  const code = error?.code ?? '';
  if (code === 'P4091') throw new RoomError(409, 'BUSY', 'Apocrypha is still answering your last message.');
  if (code === 'P4290') throw new RoomError(429, 'QUOTA', 'You have reached the hourly limit for this room. Try again later.');
  if (code === 'P4020') throw new RoomError(402, 'PREMIUM_REQUIRED', 'Premium (Opus 5.5) needs an Apocrypha Premium plan.');
  if (code === 'P4031') throw new RoomError(403, 'OWNER_REQUIRED', 'That room is private.');
  if (code === '22023' || code === '23502') throw new RoomError(400, 'MESSAGE_INVALID', 'That message could not be accepted.');
  throw new RoomError(503, 'ROOM_UNAVAILABLE', 'The room could not take that message right now.');
}

function receipt(data: unknown): SayReceipt & { raw: Record<string, unknown> } {
  const row = (Array.isArray(data) ? data[0] : data) as Record<string, unknown> | null;
  if (!row || typeof row.job_id !== 'string' || !Number.isSafeInteger(Number(row.event_id))) {
    throw new RoomError(503, 'ROOM_UNAVAILABLE', 'The queue did not return a job.');
  }
  return {
    raw: row,
    event: null as unknown as RoomEvent,
    job: { id: row.job_id, status: String(row.job_status ?? 'queued'), lane: row.engine_lane === 'flagship' ? 'flagship' : 'local' },
  };
}

/** Post the message and enqueue its job, in one transaction. */
export async function say(speaker: Speaker, input: SayInput, client: SupabaseClient = roomClient()): Promise<SayReceipt> {
  if (input.room === 'owner' && speaker.kind !== 'owner') throw new RoomError(403, 'OWNER_REQUIRED', 'That room is private.');
  if (input.attachmentIds.length > MAX_ATTACHMENTS) throw new RoomError(400, 'TOO_MANY_ATTACHMENTS', `At most ${MAX_ATTACHMENTS} attachments per message.`);
  const tail = await listEvents(input.room, 0, HISTORY_ROWS, client);
  const history = roomHistory(tail, input.room);

  let result: { data: unknown; error: { code?: string | null; message?: string | null } | null };
  if (speaker.kind === 'guest') {
    if (!speaker.guestId) throw new RoomError(401, 'GUEST_REQUIRED', 'Reload the page and try again.');
    if (input.lane !== 'local') throw new RoomError(402, 'PREMIUM_REQUIRED', 'Premium (Opus 5.5) needs a signed-in Premium plan.');
    if (input.attachmentIds.length > 0) throw new RoomError(403, 'ATTACHMENTS_NEED_SIGN_IN', 'Sign in to attach files.');
    result = await client.rpc('apocrypha_room_say_guest', {
      p_author: speaker.author,
      p_body: input.body,
      p_subject_hash: guestSubjectHash(speaker.guestId),
      p_request_id: randomUUID(),
      p_history: history.slice(-GUEST_HISTORY_TURNS),
      p_model_alias: APOCRYPHA_MODEL_ALIAS,
      p_profile_hash: APOCRYPHA_PROFILE_HASH,
      p_tool_registry_version: APOCRYPHA_TOOL_REGISTRY_VERSION,
      p_memory_manifest_hash: APOCRYPHA_MEMORY_MANIFEST_HASH,
    });
  } else {
    if (!speaker.authUserId) throw new RoomError(401, 'SIGN_IN_REQUIRED', 'Sign in again.');
    const identity = speaker.kind === 'owner'
      ? await ensureOwnerIdentity(speaker.authUserId)
      : await memberIdentity(speaker.authUserId, client);
    const allowed = input.lane === 'flagship' ? await flagshipAllowed(speaker, client) : false;
    const attachments = await loadAttachments(speaker, input.attachmentIds, client);
    const finalContent = input.room === 'lobby' ? `${speakerLabel(speaker.author)}: ${input.body}` : input.body;
    const request = {
      prompt: finalContent,
      messages: [
        { role: 'system', content: roomPersona(input.room) },
        ...history,
        { role: 'user', content: finalContent },
      ],
      conversation_history: history,
      retrieval_query: input.body,
      output_budget: 1536,
      response_mode: 'standard',
      source: 'apocky.com/room',
      privacy_class: input.room === 'owner' ? 'restricted' : 'room-lobby',
      memory_scope: speaker.kind === 'owner' ? 'owner-authorized' : 'principal-scoped',
      speaker: speaker.kind,
      ...(attachments.length > 0 ? { attachments } : {}),
      ...(input.tools.length > 0 ? { tools: [...input.tools] } : {}),
    };
    result = await client.rpc('apocrypha_room_say', {
      p_room: input.room,
      p_author: speaker.author,
      p_body: input.body,
      p_tenant_id: identity.tenantId,
      p_principal_id: identity.principalId,
      p_capability: speaker.kind === 'owner' ? 'apocky_owner_chat' : 'apocky_member_chat',
      p_request: request,
      p_engine_lane: input.lane,
      p_flagship_allowed: allowed,
      p_model_alias: APOCRYPHA_MODEL_ALIAS,
      p_profile_hash: APOCRYPHA_PROFILE_HASH,
      p_tool_registry_version: APOCRYPHA_TOOL_REGISTRY_VERSION,
      p_memory_manifest_hash: APOCRYPHA_MEMORY_MANIFEST_HASH,
    });
  }
  if (result.error) sayFailure(result.error);
  const made = receipt(result.data);
  const event = toEvent({
    id: made.raw.event_id,
    room: input.room,
    author: speaker.author,
    kind: 'utterance',
    body: input.body,
    meta: { engine_lane: made.job.lane },
    created_at: made.raw.created_at,
  });
  if (event === null) throw new RoomError(503, 'ROOM_UNAVAILABLE', 'The room returned an unreadable row.');
  return { event, job: made.job };
}

async function memberIdentity(authUserId: string, client: SupabaseClient): Promise<{ tenantId: string; principalId: string }> {
  const { data, error } = await client.rpc('apocrypha_ensure_member_principal', { p_verified_auth_user_id: authUserId });
  const row = (Array.isArray(data) ? data[0] : data) as { tenant_id?: string; principal_id?: string } | null;
  if (error || !row?.tenant_id || !row.principal_id) throw new RoomError(503, 'MEMBER_IDENTITY_UNAVAILABLE', 'Your account could not be read right now.');
  return { tenantId: String(row.tenant_id), principalId: String(row.principal_id) };
}

export interface LiveTurn {
  readonly job_id: string;
  readonly reply_to: number;
  readonly lane: EngineLane;
  readonly status: string;
  /** The answer streamed so far, already filtered: a leaked thought never reaches a reader. */
  readonly text: string;
  readonly thinking: boolean;
}

/** The room's in-flight turns, with what has streamed so far. */
export async function liveTurns(room: Room, client: SupabaseClient = roomClient()): Promise<LiveTurn[]> {
  const { data, error } = await client.rpc('apocrypha_room_live', { p_room: room });
  if (error) return [];
  const rows = (Array.isArray(data) ? data : []) as Array<Record<string, unknown>>;
  return rows.flatMap((row): LiveTurn[] => {
    if (typeof row.job_id !== 'string') return [];
    const raw = typeof row.text === 'string' ? row.text : '';
    const shown = raw.trim() === '' ? null : presentable(raw);
    return [{
      job_id: row.job_id,
      reply_to: Number(row.event_id),
      lane: row.engine_lane === 'flagship' ? 'flagship' : 'local',
      status: String(row.status ?? 'queued'),
      text: shown && shown.withheld === null ? shown.text : '',
      thinking: shown === null || shown.withheld !== null,
    }];
  });
}
