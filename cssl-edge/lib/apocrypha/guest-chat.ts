// Guest chat: the durable queue, for someone who has not signed in.
//
// Guests reach the SAME worker as members. What differs is the tenant they reach it in --
// 'apocky-guests' rather than 'apocky-members' -- and memory retrieval in the worker is
// tenant-scoped, so that tenancy is what structurally keeps a stranger's turn away from member and
// owner memory. Verified 2026-09-14: a guest turn came back saying the subject "does not appear in
// any of the admitted memory records."
//
// Nothing about a guest is retained beyond the job itself. The thread lives in their browser, which
// is why history is passed up with each turn and bounded in SQL rather than trusted.

import { createHash, randomUUID } from 'node:crypto';
import {
  getApocryphaServiceClient,
  APOCRYPHA_MODEL_ALIAS,
  APOCRYPHA_PROFILE_HASH,
  APOCRYPHA_TOOL_REGISTRY_VERSION,
  APOCRYPHA_MEMORY_MANIFEST_HASH,
} from '@/lib/apocrypha/job-control';

export const GUEST_COOKIE = 'apx_guest';
export const GUEST_COOKIE_MAX_AGE = 60 * 60 * 24 * 30;
const MAX_HISTORY_TURNS = 12;

export interface GuestTurn { readonly role: 'user' | 'assistant'; readonly content: string }

/**
 * The largest message the guest queue accepts, in BYTES.
 *
 * Exported because the browser was capping the composer at 8000 UTF-16 units while this rejected
 * at 8192 bytes — so ~4,100 characters of any non-Latin script passed the client check and was
 * refused by the server, after the composer had already been cleared.
 */
export const GUEST_MESSAGE_MAX_BYTES = 8_192;

export class GuestChatError extends Error {
  constructor(readonly status: number, readonly code: string, message: string) {
    super(message);
    this.name = 'GuestChatError';
  }
}

interface RpcError { code?: string | null; message?: string | null }
interface RpcClient {
  rpc(fn: string, args: Record<string, unknown>): PromiseLike<{ data: unknown; error: RpcError | null }>;
}

/**
 * Server-derived identity for a signed-out visitor.
 *
 * The cookie holds a random id and nothing about the person; the DIGEST of it is what reaches the
 * database, so even a leaked row cannot be correlated back to the cookie. Domain-separated so a
 * guest digest can never coincide with any other hashed identity in the system.
 */
export function guestSubjectHash(guestId: string): string {
  if (!/^[0-9a-f-]{8,128}$/i.test(guestId)) throw new GuestChatError(400, 'GUEST_ID_INVALID', 'Invalid guest identity.');
  return createHash('sha256')
    .update('APOCRYPHA-GUEST-SUBJECT-v1\0', 'utf8')
    .update(guestId, 'utf8')
    .digest('hex');
}

export function readGuestCookie(cookieHeader: string | undefined): string | null {
  for (const part of (cookieHeader ?? '').split(';')) {
    const [name, ...rest] = part.trim().split('=');
    if (name !== GUEST_COOKIE) continue;
    const value = decodeURIComponent(rest.join('='));
    if (/^[0-9a-f-]{8,128}$/i.test(value)) return value;
  }
  return null;
}

export function newGuestId(): string {
  return randomUUID();
}

export function guestCookie(guestId: string, production: boolean): string {
  const secure = production ? '; Secure' : '';
  return `${GUEST_COOKIE}=${encodeURIComponent(guestId)}; Path=/; Max-Age=${GUEST_COOKIE_MAX_AGE}`
    + `; HttpOnly; SameSite=Lax${secure}`;
}

function fail(error: RpcError | null): never {
  const code = error?.code ?? '';
  if (code === 'P4091') {
    throw new GuestChatError(409, 'GUEST_CHAT_BUSY', 'Apocrypha is still answering your last message.');
  }
  if (code === 'P4290') {
    throw new GuestChatError(429, 'GUEST_CHAT_QUOTA', 'You have reached the hourly limit for signed-out questions. Sign in for more room.');
  }
  if (code === 'P4031') {
    throw new GuestChatError(403, 'GUEST_CHAT_FORBIDDEN', 'That conversation does not belong to this browser.');
  }
  if (code === '22023' || code === '23502') {
    throw new GuestChatError(400, 'GUEST_CHAT_INVALID', 'That message could not be accepted.');
  }
  throw new GuestChatError(502, 'GUEST_CHAT_UNAVAILABLE', 'Apocrypha could not take that message right now.');
}

function rows(data: unknown): Record<string, unknown>[] {
  return Array.isArray(data) ? data as Record<string, unknown>[] : [];
}

export interface GuestJobReceipt {
  readonly job_id: string;
  readonly status: string;
  readonly replayed: boolean;
}

export async function enqueueGuestChat(
  input: { guestId: string; requestId: string; message: string; history: readonly GuestTurn[] },
  client: RpcClient = getApocryphaServiceClient() as unknown as RpcClient,
): Promise<GuestJobReceipt> {
  const history = input.history
    .filter((turn) => typeof turn?.content === 'string' && turn.content.trim() !== '')
    .slice(-MAX_HISTORY_TURNS)
    .map((turn) => ({ role: turn.role === 'assistant' ? 'assistant' : 'user', content: turn.content.slice(0, 4_000) }));

  const { data, error } = await client.rpc('apocrypha_enqueue_guest_chat_v1', {
    p_subject_hash: guestSubjectHash(input.guestId),
    p_request_id: input.requestId.toLowerCase(),
    p_message: input.message,
    p_history: history,
    p_model_alias: APOCRYPHA_MODEL_ALIAS,
    p_profile_hash: APOCRYPHA_PROFILE_HASH,
    p_tool_registry_version: APOCRYPHA_TOOL_REGISTRY_VERSION,
    p_memory_manifest_hash: APOCRYPHA_MEMORY_MANIFEST_HASH,
  });
  if (error) fail(error);
  const row = rows(data)[0];
  if (!row || typeof row.job_id !== 'string') {
    throw new GuestChatError(502, 'GUEST_CHAT_UNAVAILABLE', 'The queue did not return a job.');
  }
  return { job_id: row.job_id, status: String(row.status ?? 'queued'), replayed: row.replayed === true };
}

export interface GuestJobState {
  readonly job_id: string;
  readonly status: string;
  readonly answer: string | null;
  readonly error_code: string | null;
}

export async function readGuestChatJob(
  input: { guestId: string; jobId: string },
  client: RpcClient = getApocryphaServiceClient() as unknown as RpcClient,
): Promise<GuestJobState> {
  const { data, error } = await client.rpc('apocrypha_get_guest_chat_job', {
    p_subject_hash: guestSubjectHash(input.guestId),
    p_job_id: input.jobId.toLowerCase(),
  });
  if (error) fail(error);
  const row = rows(data)[0];
  if (!row || typeof row.job_id !== 'string') {
    throw new GuestChatError(404, 'GUEST_CHAT_NOT_FOUND', 'That answer is no longer available.');
  }
  return {
    job_id: row.job_id,
    status: String(row.status ?? 'queued'),
    answer: typeof row.answer === 'string' ? row.answer : null,
    error_code: typeof row.error_code === 'string' ? row.error_code : null,
  };
}
