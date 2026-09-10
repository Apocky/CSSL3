import type { NextApiRequest, NextApiResponse } from 'next';

import { hasSameOrigin, requestOrigin } from '../auth-session';
import {
  APOCRYPHA_MEMORY_MANIFEST_HASH,
  APOCRYPHA_MODEL_ALIAS,
  APOCRYPHA_PROFILE_HASH,
  APOCRYPHA_TOOL_REGISTRY_VERSION,
  getApocryphaServiceClient,
} from './job-control';

export const APOCRYPHA_MEMBER_CHAT_CAPABILITY = 'apocky_member_chat' as const;
export const MEMBER_CHAT_MESSAGE_MAX_BYTES = 16_384;
export const MEMBER_CHAT_ASSISTANT_MAX_BYTES = 65_536;
export const MEMBER_CHAT_HISTORY_LIMIT = 50;
export const MEMBER_CHAT_HISTORY_CONTENT_MAX_BYTES = 4_096_000;
export const MEMBER_CHAT_HISTORY_WIRE_MAX_BYTES = 1_572_864;

const UUID_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;
const HISTORY_CURSOR_RE = /^[1-9][0-9]{0,18}$/;
const DISALLOWED_MESSAGE_CONTROL_RE = /[\u0000-\u0008\u000b\u000c\u000e-\u001f\u007f]/u;
const MAX_POSTGRES_BIGINT = 9_223_372_036_854_775_807n;
const JOB_STATUSES = new Set([
  'queued',
  'leased',
  'running',
  'cancel_requested',
  'succeeded',
  'failed',
  'cancelled',
]);

interface RpcError {
  code?: string;
  message?: string;
}

export interface MemberChatRpcClient {
  rpc(
    functionName: string,
    args: Record<string, unknown>,
  ): PromiseLike<{ data: unknown; error: RpcError | null }>;
}

export interface MemberChatJobReceipt {
  job_id: string;
  conversation_id: string;
  request_id: string;
  status: string;
  model_alias: string;
  memory_manifest_hash: string;
  created_at: string;
  updated_at: string;
  replayed: boolean;
}

export interface MemberChatHistoryEntry {
  job_id: string;
  conversation_id: string;
  request_id: string;
  status: string;
  user_message: string;
  assistant_message: string | null;
  assistant_truncated: boolean;
  model_alias: string;
  memory_manifest_hash: string;
  created_at: string;
  updated_at: string;
  completed_at: string | null;
  error_code: string | null;
}

export interface MemberChatHistoryPage {
  history: MemberChatHistoryEntry[];
  nextCursor: string | null;
}

export class MemberChatStoreError extends Error {
  constructor(
    readonly publicStatus: number,
    readonly publicCode: string,
    message: string,
    readonly retryAfterSeconds?: number,
  ) {
    super(message);
    this.name = 'MemberChatStoreError';
  }
}

function record(value: unknown): Record<string, unknown> | null {
  return value !== null && typeof value === 'object' && !Array.isArray(value)
    ? value as Record<string, unknown>
    : null;
}

function rows(value: unknown): Record<string, unknown>[] {
  if (value === null) return [];
  if (Array.isArray(value)) {
    const projected: Record<string, unknown>[] = [];
    for (const item of value) {
      const candidate = record(item);
      if (!candidate) {
        throw new MemberChatStoreError(502, 'MEMBER_CHAT_INVALID_PROJECTION', 'The RPC returned an invalid row.');
      }
      projected.push(candidate);
    }
    return projected;
  }
  const single = record(value);
  if (!single) {
    throw new MemberChatStoreError(502, 'MEMBER_CHAT_INVALID_PROJECTION', 'The RPC returned an invalid projection.');
  }
  return [single];
}

function requiredString(
  value: unknown,
  field: string,
  predicate: (candidate: string) => boolean = () => true,
): string {
  if (typeof value !== 'string' || !predicate(value)) {
    throw new MemberChatStoreError(502, 'MEMBER_CHAT_INVALID_PROJECTION', `Invalid ${field} projection.`);
  }
  return value;
}

function nullableString(
  value: unknown,
  field: string,
  predicate: (candidate: string) => boolean = () => true,
): string | null {
  if (value === null) return null;
  return requiredString(value, field, predicate);
}

function requiredBoolean(value: unknown, field: string): boolean {
  if (typeof value !== 'boolean') {
    throw new MemberChatStoreError(502, 'MEMBER_CHAT_INVALID_PROJECTION', `Invalid ${field} projection.`);
  }
  return value;
}

function invalidProjection(message: string): never {
  throw new MemberChatStoreError(502, 'MEMBER_CHAT_INVALID_PROJECTION', message);
}

function normalizeJobReceipt(value: unknown): MemberChatJobReceipt {
  const item = record(value);
  if (!item) {
    throw new MemberChatStoreError(502, 'MEMBER_CHAT_INVALID_PROJECTION', 'The job receipt was missing.');
  }
  if (typeof item.replayed !== 'boolean') {
    throw new MemberChatStoreError(502, 'MEMBER_CHAT_INVALID_PROJECTION', 'Invalid replay projection.');
  }
  return {
    job_id: requiredString(item.job_id, 'job_id', isMemberChatUuid).toLowerCase(),
    conversation_id: requiredString(item.conversation_id, 'conversation_id', isMemberChatUuid).toLowerCase(),
    request_id: requiredString(item.request_id, 'request_id', isMemberChatUuid).toLowerCase(),
    status: requiredString(item.status, 'status', (candidate) => JOB_STATUSES.has(candidate)),
    model_alias: requiredString(item.model_alias, 'model_alias', (candidate) => candidate.length > 0 && candidate.length <= 160),
    memory_manifest_hash: requiredString(item.memory_manifest_hash, 'memory_manifest_hash', (candidate) => /^[0-9a-f]{64}$/.test(candidate)),
    created_at: requiredString(item.created_at, 'created_at'),
    updated_at: requiredString(item.updated_at, 'updated_at'),
    replayed: item.replayed,
  };
}

function normalizeHistoryEntry(value: unknown): MemberChatHistoryEntry {
  const item = record(value);
  if (!item) {
    throw new MemberChatStoreError(502, 'MEMBER_CHAT_INVALID_PROJECTION', 'A history entry was invalid.');
  }
  return {
    job_id: requiredString(item.job_id, 'job_id', isMemberChatUuid).toLowerCase(),
    conversation_id: requiredString(item.conversation_id, 'conversation_id', isMemberChatUuid).toLowerCase(),
    request_id: requiredString(item.request_id, 'request_id', isMemberChatUuid).toLowerCase(),
    status: requiredString(item.status, 'status', (candidate) => JOB_STATUSES.has(candidate)),
    user_message: requiredString(
      item.user_message,
      'user_message',
      (candidate) => Buffer.byteLength(candidate, 'utf8') <= MEMBER_CHAT_MESSAGE_MAX_BYTES,
    ),
    assistant_message: nullableString(
      item.assistant_message,
      'assistant_message',
      (candidate) => Buffer.byteLength(candidate, 'utf8') <= MEMBER_CHAT_ASSISTANT_MAX_BYTES,
    ),
    assistant_truncated: requiredBoolean(item.assistant_truncated, 'assistant_truncated'),
    model_alias: requiredString(item.model_alias, 'model_alias', (candidate) => candidate.length > 0 && candidate.length <= 160),
    memory_manifest_hash: requiredString(item.memory_manifest_hash, 'memory_manifest_hash', (candidate) => /^[0-9a-f]{64}$/.test(candidate)),
    created_at: requiredString(item.created_at, 'created_at'),
    updated_at: requiredString(item.updated_at, 'updated_at'),
    completed_at: nullableString(item.completed_at, 'completed_at'),
    error_code: nullableString(item.error_code, 'error_code'),
  };
}

interface MemberChatHistoryRpcRow {
  entry: MemberChatHistoryEntry;
  turnCursor: string;
  hasMore: boolean;
}

function normalizeHistoryRpcRow(value: unknown): MemberChatHistoryRpcRow {
  const item = record(value);
  if (!item) invalidProjection('A history row was invalid.');
  const turnCursor = requiredString(
    item.turn_cursor,
    'turn_cursor',
    (candidate) => canonicalMemberChatCursor(candidate) === candidate,
  );
  return {
    entry: normalizeHistoryEntry(item),
    turnCursor,
    hasMore: requiredBoolean(item.has_more, 'has_more'),
  };
}

function historyWireBytes(
  conversationId: string,
  history: MemberChatHistoryEntry[],
  nextCursor: string | null,
): number {
  return Buffer.byteLength(JSON.stringify({
    ok: true,
    conversation_id: conversationId,
    history,
    count: history.length,
    next_cursor: nextCursor,
  }), 'utf8');
}

function configuredClient(): MemberChatRpcClient {
  return getApocryphaServiceClient() as unknown as MemberChatRpcClient;
}

function storeFailure(operation: string, error: RpcError | null): never {
  const message = error?.message?.toLowerCase() ?? '';
  if (error?.code === 'P4031') {
    throw new MemberChatStoreError(
      403,
      'MEMBER_CHAT_CONVERSATION_MISMATCH',
      'This conversation is not bound to the verified member session.',
    );
  }
  if (
    error?.code === 'P4091'
    || (error?.code === '55000' && message.includes('turn in progress'))
  ) {
    throw new MemberChatStoreError(
      409,
      'MEMBER_CHAT_TURN_IN_PROGRESS',
      'This conversation already has a turn in progress. Retry after it finishes.',
    );
  }
  if (error?.code === '23505' || message.includes('replay') || message.includes('idempotency')) {
    throw new MemberChatStoreError(
      409,
      'MEMBER_CHAT_REPLAY_CONFLICT',
      'This request identifier is already attached to different content.',
    );
  }
  if (error?.code === 'P4290') {
    throw new MemberChatStoreError(
      429,
      'MEMBER_CHAT_QUOTA_EXCEEDED',
      'This member has reached the rolling chat limit. Retry later.',
      3600,
    );
  }
  if (
    error?.code === '23514'
    || error?.code === '22023'
    || error?.code === '22P05'
    || error?.code === '22021'
  ) {
    throw new MemberChatStoreError(400, 'MEMBER_CHAT_INVALID_REQUEST', 'The member chat request is invalid.');
  }
  if (error?.code === '42501') {
    throw new MemberChatStoreError(403, 'MEMBER_CHAT_ACCESS_DENIED', 'Member chat is unavailable for this account.');
  }
  throw new MemberChatStoreError(
    503,
    'MEMBER_CHAT_STORAGE_UNAVAILABLE',
    `Member chat ${operation} is temporarily unavailable. It is safe to retry.`,
  );
}

export function isMemberChatUuid(value: string): boolean {
  return UUID_RE.test(value);
}

export function canonicalMemberChatMessage(value: unknown): string | null {
  if (
    typeof value !== 'string'
    || value.length === 0
    || value !== value.trim()
    || DISALLOWED_MESSAGE_CONTROL_RE.test(value)
  ) return null;
  return Buffer.byteLength(value, 'utf8') <= MEMBER_CHAT_MESSAGE_MAX_BYTES ? value : null;
}

export function canonicalMemberChatCursor(value: unknown): string | null {
  if (typeof value !== 'string' || !HISTORY_CURSOR_RE.test(value)) return null;
  try {
    return BigInt(value) <= MAX_POSTGRES_BIGINT ? value : null;
  } catch {
    return null;
  }
}

export function canonicalMemberChatVerifiedIdentity(value: string): string {
  if (!isMemberChatUuid(value)) {
    throw new MemberChatStoreError(
      500,
      'MEMBER_CHAT_SERVER_BINDING_INVALID',
      'The verified member binding is invalid.',
    );
  }
  return value.toLowerCase();
}

export function requireMemberChatConversationBinding(
  verifiedAuthUserId: string,
  presentedConversationId: string,
): string {
  const verified = canonicalMemberChatVerifiedIdentity(verifiedAuthUserId);
  if (!isMemberChatUuid(presentedConversationId) || presentedConversationId.toLowerCase() !== verified) {
    throw new MemberChatStoreError(
      403,
      'MEMBER_CHAT_CONVERSATION_MISMATCH',
      'This conversation is not bound to the verified member session.',
    );
  }
  return verified;
}

export function setMemberChatPrivateHeaders(res: NextApiResponse): void {
  res.setHeader('Cache-Control', 'private, no-store, no-cache, must-revalidate, max-age=0');
  res.setHeader('Pragma', 'no-cache');
  res.setHeader('Vary', 'Authorization, Cookie, Origin');
  res.setHeader('Referrer-Policy', 'no-referrer');
  res.setHeader('X-Content-Type-Options', 'nosniff');
  res.setHeader('X-Frame-Options', 'DENY');
  res.setHeader('X-Robots-Tag', 'noindex, nofollow, noarchive, nosnippet');
}

function firstHeader(value: string | string[] | undefined): string | null {
  const first = Array.isArray(value) ? value[0] : value;
  return first?.split(',')[0]?.trim() || null;
}

/**
 * Mutations require an Origin header. Same-origin browser reads may instead
 * carry Referer/Sec-Fetch-Site, so validate those without accepting a foreign
 * Origin or an unlabelled server-to-server request.
 */
export function hasMemberChatReadOrigin(req: NextApiRequest): boolean {
  const origin = firstHeader(req.headers.origin);
  const expected = requestOrigin(req);
  const referer = firstHeader(req.headers.referer);
  const fetchSite = firstHeader(req.headers['sec-fetch-site'])?.toLowerCase() ?? null;
  let presented = false;

  if (origin) {
    presented = true;
    if (!hasSameOrigin(req)) return false;
  }
  if (expected && referer) {
    presented = true;
    try {
      if (new URL(referer).origin !== expected) return false;
    } catch {
      return false;
    }
  } else if (referer) {
    return false;
  }
  if (fetchSite) {
    presented = true;
    if (fetchSite !== 'same-origin') return false;
  }
  return presented;
}

export async function enqueueMemberChat(
  input: {
    verifiedAuthUserId: string;
    conversationId: string;
    requestId: string;
    message: string;
  },
  client: MemberChatRpcClient = configuredClient(),
): Promise<MemberChatJobReceipt> {
  const conversationId = requireMemberChatConversationBinding(
    input.verifiedAuthUserId,
    input.conversationId,
  );
  const requestId = input.requestId.toLowerCase();
  if (!isMemberChatUuid(requestId) || canonicalMemberChatMessage(input.message) !== input.message) {
    throw new MemberChatStoreError(500, 'MEMBER_CHAT_SERVER_BINDING_INVALID', 'The verified member binding is invalid.');
  }
  const { data, error } = await client.rpc('apocrypha_enqueue_member_chat_v2', {
    p_verified_auth_user_id: conversationId,
    p_presented_conversation_id: conversationId,
    p_request_id: requestId,
    p_message: input.message,
    p_model_alias: APOCRYPHA_MODEL_ALIAS,
    p_profile_hash: APOCRYPHA_PROFILE_HASH,
    p_tool_registry_version: APOCRYPHA_TOOL_REGISTRY_VERSION,
    p_memory_manifest_hash: APOCRYPHA_MEMORY_MANIFEST_HASH,
  });
  if (error) storeFailure('submission', error);
  const row = rows(data)[0];
  const receipt = normalizeJobReceipt(row);
  if (
    receipt.conversation_id.toLowerCase() !== conversationId
    || receipt.request_id.toLowerCase() !== requestId
  ) {
    invalidProjection('The job receipt escaped its verified member binding.');
  }
  return receipt;
}

export async function getMemberChatJob(
  input: { verifiedAuthUserId: string; jobId: string },
  client: MemberChatRpcClient = configuredClient(),
): Promise<MemberChatHistoryEntry | null> {
  const verifiedAuthUserId = canonicalMemberChatVerifiedIdentity(input.verifiedAuthUserId);
  const jobId = input.jobId.toLowerCase();
  if (!isMemberChatUuid(jobId)) {
    throw new MemberChatStoreError(500, 'MEMBER_CHAT_SERVER_BINDING_INVALID', 'The verified member binding is invalid.');
  }
  const { data, error } = await client.rpc('apocrypha_get_member_chat_job', {
    p_verified_auth_user_id: verifiedAuthUserId,
    p_job_id: jobId,
  });
  if (error) storeFailure('job lookup', error);
  const row = rows(data)[0];
  if (!row) return null;
  const job = normalizeHistoryEntry(row);
  if (
    job.job_id.toLowerCase() !== jobId
    || job.conversation_id.toLowerCase() !== verifiedAuthUserId
  ) {
    invalidProjection('The job projection escaped its verified member binding.');
  }
  return job;
}

export async function listMemberChatHistory(
  input: {
    verifiedAuthUserId: string;
    conversationId: string;
    beforeCursor?: string | null;
  },
  client: MemberChatRpcClient = configuredClient(),
): Promise<MemberChatHistoryPage> {
  const conversationId = requireMemberChatConversationBinding(
    input.verifiedAuthUserId,
    input.conversationId,
  );
  const beforeCursor = input.beforeCursor ?? null;
  if (beforeCursor !== null && canonicalMemberChatCursor(beforeCursor) !== beforeCursor) {
    throw new MemberChatStoreError(400, 'MEMBER_CHAT_HISTORY_CURSOR_INVALID', 'The member chat history cursor is invalid.');
  }
  const { data, error } = await client.rpc('apocrypha_list_member_chat_history_v2', {
    p_verified_auth_user_id: conversationId,
    p_presented_conversation_id: conversationId,
    p_before_turn_sequence: beforeCursor,
    p_limit: MEMBER_CHAT_HISTORY_LIMIT,
  });
  if (error) storeFailure('history lookup', error);
  const projected = rows(data).map(normalizeHistoryRpcRow);
  if (projected.length > MEMBER_CHAT_HISTORY_LIMIT) {
    throw new MemberChatStoreError(502, 'MEMBER_CHAT_INVALID_PROJECTION', 'The history projection exceeded its bound.');
  }
  let previousCursor = 0n;
  let databaseHasMore = false;
  for (const [index, row] of projected.entries()) {
    if (row.entry.conversation_id.toLowerCase() !== conversationId) {
      invalidProjection('A history row escaped its verified member binding.');
    }
    const cursor = BigInt(row.turnCursor);
    if (cursor <= previousCursor) {
      invalidProjection('The history cursor order was invalid.');
    }
    previousCursor = cursor;
    if (index === 0) databaseHasMore = row.hasMore;
    if (row.hasMore !== databaseHasMore) {
      invalidProjection('The history continuation projection was inconsistent.');
    }
  }

  let selected = projected;
  let wirePruned = false;
  while (selected.length > 0) {
    const candidateCursor = databaseHasMore || wirePruned
      ? selected[0]!.turnCursor
      : null;
    if (
      historyWireBytes(
        conversationId,
        selected.map((row) => row.entry),
        candidateCursor,
      ) <= MEMBER_CHAT_HISTORY_WIRE_MAX_BYTES
    ) break;
    selected = selected.slice(1);
    wirePruned = true;
  }
  if (projected.length > 0 && selected.length === 0) {
    invalidProjection('A single history row exceeded the serialized response bound.');
  }

  const history = selected.map((row) => row.entry);
  const contentBytes = history.reduce(
    (total, entry) => total
      + Buffer.byteLength(entry.user_message, 'utf8')
      + (entry.assistant_message ? Buffer.byteLength(entry.assistant_message, 'utf8') : 0),
    0,
  );
  if (contentBytes > MEMBER_CHAT_HISTORY_CONTENT_MAX_BYTES) {
    throw new MemberChatStoreError(502, 'MEMBER_CHAT_INVALID_PROJECTION', 'The history content exceeded its byte bound.');
  }
  const nextCursor = history.length > 0 && (databaseHasMore || wirePruned)
    ? selected[0]!.turnCursor
    : null;
  return { history, nextCursor };
}

export function memberChatPublicError(error: unknown): {
  status: number;
  body: { ok: false; code: string; error: string };
  retryAfterSeconds?: number;
} {
  if (error instanceof MemberChatStoreError) {
    return {
      status: error.publicStatus,
      body: { ok: false, code: error.publicCode, error: error.message },
      ...(error.retryAfterSeconds === undefined
        ? {}
        : { retryAfterSeconds: error.retryAfterSeconds }),
    };
  }
  const raw = error instanceof Error ? error.message : String(error);
  if (raw.includes('APOCRYPHA_CONTROL_PLANE_UNCONFIGURED')) {
    return {
      status: 503,
      body: {
        ok: false,
        code: 'MEMBER_CHAT_STORAGE_UNAVAILABLE',
        error: 'Member chat storage is not configured for this deployment.',
      },
    };
  }
  return {
    status: 503,
    body: {
      ok: false,
      code: 'MEMBER_CHAT_UNAVAILABLE',
      error: 'Member chat is temporarily unavailable. It is safe to retry.',
    },
  };
}

/** One of a member's conversations, as a sidebar would show it. */
export interface MemberChatConversationSummary {
  conversation_id: string;
  title: string;
  turn_count: number;
  created_at: string;
  last_activity_at: string;
}

/**
 * Every conversation this member owns, most recently active first.
 *
 * Scoping happens inside the RPC, derived from the verified auth user id, so
 * there is no conversation id to present here and therefore none to get wrong.
 *
 * DORMANT until migration 0053 is applied: the RPC it calls does not exist yet,
 * so this returns a storage error rather than data. It ships ahead of the
 * migration deliberately - the route and the type are inert, and landing them
 * separately keeps the migration's own change-set small enough to read.
 */
export async function listMemberConversations(
  input: { verifiedAuthUserId: string; limit?: number },
  client: MemberChatRpcClient = configuredClient(),
): Promise<MemberChatConversationSummary[]> {
  const verifiedAuthUserId = canonicalMemberChatVerifiedIdentity(input.verifiedAuthUserId);
  const { data, error } = await client.rpc('apocrypha_list_member_conversations', {
    p_verified_auth_user_id: verifiedAuthUserId,
    p_limit: Math.min(Math.max(input.limit ?? 50, 1), 200),
  });
  if (error) storeFailure('conversation listing', error);
  return rows(data).map((row) => {
    const item = row as Record<string, unknown>;
    const conversationId = requiredString(item.conversation_id, 'conversation_id', isMemberChatUuid)
      .toLowerCase();
    return {
      conversation_id: conversationId,
      // A title is presentation, so a missing or odd one degrades to a usable
      // label rather than failing a listing the member is entitled to see.
      title: typeof item.title === 'string' && item.title.trim() ? item.title.trim() : 'New conversation',
      turn_count: Number.isFinite(Number(item.turn_count)) ? Number(item.turn_count) : 0,
      created_at: typeof item.created_at === 'string' ? item.created_at : '',
      last_activity_at: typeof item.last_activity_at === 'string' ? item.last_activity_at : '',
    };
  });
}
