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

export type MemberChatEngineLane = 'local' | 'flagship';
export const MEMBER_CHAT_ENGINE_LANES: ReadonlySet<string> = new Set(['local', 'flagship']);
export const APOCRYPHA_PREMIUM_PRODUCT_ID = 'apocrypha-premium';

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
  thread_id?: string | null;
  engine_lane?: MemberChatEngineLane;
}

export interface MemberChatThread {
  thread_id: string;
  title: string;
  pinned: boolean;
  archived: boolean;
  created_at: string;
  last_active_at: string;
  turn_count: number;
  preview: string | null;
}

export interface MemberChatAttachment {
  id: string;
  thread_id: string;
  file_name: string;
  mime_type: string;
  byte_size: number;
  has_text: boolean;
  created_at: string;
}

export interface MemberPlan {
  flagship: boolean;
  default_lane: MemberChatEngineLane;
  product_id: string;
  /** Set when the plan could not be read because migration 0057 is not applied; visible, not silent. */
  degraded?: 'migration_0057_pending';
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
  thread_id?: string | null;
  engine_lane?: MemberChatEngineLane;
  attachment_ids?: string[];
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
    ...optionalLaneFields(item),
  };
}

function optionalLaneFields(item: Record<string, unknown>): {
  thread_id?: string | null;
  engine_lane?: MemberChatEngineLane;
  attachment_ids?: string[];
} {
  const out: { thread_id?: string | null; engine_lane?: MemberChatEngineLane; attachment_ids?: string[] } = {};
  if (typeof item.thread_id === 'string' && isMemberChatUuid(item.thread_id)) out.thread_id = item.thread_id.toLowerCase();
  else if (item.thread_id === null) out.thread_id = null;
  if (typeof item.engine_lane === 'string' && MEMBER_CHAT_ENGINE_LANES.has(item.engine_lane)) {
    out.engine_lane = item.engine_lane as MemberChatEngineLane;
  }
  if (Array.isArray(item.attachment_ids) && item.attachment_ids.every((id) => typeof id === 'string' && isMemberChatUuid(id))) {
    out.attachment_ids = (item.attachment_ids as string[]).map((id) => id.toLowerCase());
  }
  return out;
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
    ...optionalLaneFields(item),
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

// PostgREST answers PGRST202 when a function is not in the schema: the 0057 functions are not
// applied yet. The site then serves the v2 surface (one conversation, local lane, no
// attachments) instead of a 503, and says so in the response where a caller can see it.
export function migration0057Pending(error: RpcError | null): boolean {
  const message = error?.message?.toLowerCase() ?? '';
  return error?.code === 'PGRST202' || (message.includes('could not find the function') && message.includes('apocrypha_'));
}

function storeFailure(operation: string, error: RpcError | null): never {
  const message = error?.message?.toLowerCase() ?? '';
  // Live 2026-09-25: a 503 with no cause in the logs cost hours. Code and message only; no ids.
  console.error(JSON.stringify({
    at: new Date().toISOString(), level: 'error', event: 'apocrypha.member_chat.store_failure',
    operation, database_code: error?.code ?? null, database_message: (error?.message ?? '').slice(0, 300),
  }));
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
  if (error?.code === 'P4020') {
    throw new MemberChatStoreError(402, 'MEMBER_CHAT_PREMIUM_REQUIRED', 'The flagship lane needs an active Apocrypha Premium plan.');
  }
  if (error?.code === 'P4022') {
    throw new MemberChatStoreError(409, 'MEMBER_CHAT_THREAD_ARCHIVED', 'This thread is archived. Restore it to continue.');
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

/**
 * Check a presented conversation id and return it.
 *
 * This required `presented === verifiedAuthUserId` until migration 0057. That
 * equality made "this conversation belongs to this member" true by
 * construction - and also made a member own exactly one conversation forever.
 *
 * The guarantee has NOT been dropped, it has moved to where it is stronger.
 * `apocrypha_open_member_conversation` looks the id up across the tenant and
 * refuses it unless the row's principal is this member's. That check is TOTAL,
 * because `UNIQUE (tenant_id, conversation_id)` means an id has at most one
 * owner - and it runs inside the same transaction as the write, which a check
 * here never could.
 *
 * What is genuinely given up: this used to refuse a foreign id without ever
 * reaching the database. It now forwards it and the database refuses it. That
 * is one fewer layer, traded knowingly for a member being able to have more
 * than one conversation.
 */
export function requireMemberChatConversationBinding(
  verifiedAuthUserId: string,
  presentedConversationId: string,
): string {
  // Still validated: an unusable session must not reach the database at all.
  canonicalMemberChatVerifiedIdentity(verifiedAuthUserId);
  if (!isMemberChatUuid(presentedConversationId)) {
    throw new MemberChatStoreError(
      403,
      'MEMBER_CHAT_CONVERSATION_MISMATCH',
      'This conversation is not bound to the verified member session.',
    );
  }
  return presentedConversationId.toLowerCase();
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
    threadId?: string | null;
    engineLane?: MemberChatEngineLane;
    attachmentIds?: string[];
  },
  client: MemberChatRpcClient = configuredClient(),
): Promise<MemberChatJobReceipt> {
  // Kept apart deliberately. Until 0057 these were one value, because a
  // conversation id had to EQUAL the auth user id; collapsing them again would
  // hand the database two copies of the same identity and destroy its ability
  // to refuse a foreign conversation.
  const verifiedAuthUserId = canonicalMemberChatVerifiedIdentity(input.verifiedAuthUserId);
  const conversationId = requireMemberChatConversationBinding(
    input.verifiedAuthUserId,
    input.conversationId,
  );
  const requestId = input.requestId.toLowerCase();
  if (!isMemberChatUuid(requestId) || canonicalMemberChatMessage(input.message) !== input.message) {
    throw new MemberChatStoreError(500, 'MEMBER_CHAT_SERVER_BINDING_INVALID', 'The verified member binding is invalid.');
  }
  const threadId = input.threadId ? input.threadId.toLowerCase() : null;
  const engineLane: MemberChatEngineLane = input.engineLane ?? 'local';
  const attachmentIds = (input.attachmentIds ?? []).map((id) => id.toLowerCase());
  if (
    (threadId !== null && !isMemberChatUuid(threadId))
    || !MEMBER_CHAT_ENGINE_LANES.has(engineLane)
    || attachmentIds.length > 8
    || attachmentIds.some((id) => !isMemberChatUuid(id))
  ) {
    throw new MemberChatStoreError(400, 'MEMBER_CHAT_INVALID_REQUEST', 'The member chat request is invalid.');
  }
  // Two 0057s meet here. 0057_apocrypha_member_conversations (live since 2026-09-10) lets a member
  // hold several conversations and is spoken through v2. 0057_apocrypha_member_threads_lane_attachments
  // keeps ONE conversation per member (its id is the auth user id) with threads inside it, and is
  // spoken through v3, whose database gate refuses any other conversation id. So a turn that needs a
  // thread, the flagship lane or attachments must present the primary conversation; every other turn
  // stays on v2, which keeps secondary conversations working.
  const needsV3 = threadId !== null || engineLane !== 'local' || attachmentIds.length > 0;
  if (needsV3 && conversationId !== verifiedAuthUserId) {
    throw new MemberChatStoreError(
      400,
      'MEMBER_CHAT_THREADS_NEED_PRIMARY',
      'Threads, the flagship lane and attachments live in your primary conversation.',
    );
  }
  const common = {
    p_verified_auth_user_id: verifiedAuthUserId,
    p_presented_conversation_id: conversationId,
    p_request_id: requestId,
    p_message: input.message,
    p_model_alias: APOCRYPHA_MODEL_ALIAS,
    p_profile_hash: APOCRYPHA_PROFILE_HASH,
    p_tool_registry_version: APOCRYPHA_TOOL_REGISTRY_VERSION,
    p_memory_manifest_hash: APOCRYPHA_MEMORY_MANIFEST_HASH,
  };
  const result = needsV3
    ? await client.rpc('apocrypha_enqueue_member_chat_v3', {
      ...common,
      p_thread_id: threadId,
      p_engine_lane: engineLane,
      p_attachment_ids: attachmentIds,
    })
    : await client.rpc('apocrypha_enqueue_member_chat_v2', common);
  if (needsV3 && result.error && migration0057Pending(result.error)) {
    throw new MemberChatStoreError(503, 'MEMBER_CHAT_MIGRATION_PENDING', 'Threads, the flagship lane and attachments need database migration 0057, which is not applied yet.');
  }
  if (result.error) storeFailure('submission', result.error);
  const row = rows(result.data)[0];
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
  // The conversation id is no longer compared to the auth user id: a job now
  // legitimately belongs to any conversation the member owns, and that
  // comparison would reject every one except their oldest. The boundary is
  // held by apocrypha_get_member_chat_job, which is passed the verified auth
  // user id and selects only that principal's rows - a foreign job is not
  // returned at all, rather than returned and then caught here.
  if (job.job_id.toLowerCase() !== jobId || !isMemberChatUuid(job.conversation_id)) {
    invalidProjection('The job projection escaped its verified member binding.');
  }
  return job;
}

export async function listMemberChatHistory(
  input: {
    verifiedAuthUserId: string;
    conversationId: string;
    beforeCursor?: string | null;
    threadId?: string | null;
  },
  client: MemberChatRpcClient = configuredClient(),
): Promise<MemberChatHistoryPage> {
  // Kept apart deliberately. Until 0057 these were one value, because a
  // conversation id had to EQUAL the auth user id; collapsing them again would
  // hand the database two copies of the same identity and destroy its ability
  // to refuse a foreign conversation.
  const verifiedAuthUserId = canonicalMemberChatVerifiedIdentity(input.verifiedAuthUserId);
  const conversationId = requireMemberChatConversationBinding(
    input.verifiedAuthUserId,
    input.conversationId,
  );
  const beforeCursor = input.beforeCursor ?? null;
  if (beforeCursor !== null && canonicalMemberChatCursor(beforeCursor) !== beforeCursor) {
    throw new MemberChatStoreError(400, 'MEMBER_CHAT_HISTORY_CURSOR_INVALID', 'The member chat history cursor is invalid.');
  }
  const threadId = input.threadId ? input.threadId.toLowerCase() : null;
  if (threadId !== null && !isMemberChatUuid(threadId)) {
    throw new MemberChatStoreError(400, 'MEMBER_CHAT_THREAD_ID_INVALID', 'The member chat thread id is invalid.');
  }
  // A thread is a v3 concept inside the primary conversation (see enqueueMemberChat); without one,
  // v2 serves any conversation the member owns.
  if (threadId !== null && conversationId !== verifiedAuthUserId) {
    throw new MemberChatStoreError(400, 'MEMBER_CHAT_THREADS_NEED_PRIMARY', 'Threads live in your primary conversation.');
  }
  const common = {
    p_verified_auth_user_id: verifiedAuthUserId,
    p_presented_conversation_id: conversationId,
    p_before_turn_sequence: beforeCursor,
    p_limit: MEMBER_CHAT_HISTORY_LIMIT,
  };
  let result = threadId !== null
    ? await client.rpc('apocrypha_list_member_chat_history_v3', { ...common, p_thread_id: threadId })
    : await client.rpc('apocrypha_list_member_chat_history_v2', common);
  if (threadId !== null && result.error && migration0057Pending(result.error)) {
    result = await client.rpc('apocrypha_list_member_chat_history_v2', common);
  }
  if (result.error) storeFailure('history lookup', result.error);
  const projected = rows(result.data).map(normalizeHistoryRpcRow);
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

// ─── threads (migration 0057) ────────────────────────────────────────────────

function normalizeThread(value: unknown): MemberChatThread {
  const item = record(value);
  if (!item) invalidProjection('A thread row was invalid.');
  const id = typeof item.thread_id === 'string' ? item.thread_id : typeof item.id === 'string' ? item.id : '';
  return {
    thread_id: requiredString(id, 'thread_id', isMemberChatUuid).toLowerCase(),
    title: requiredString(item.title, 'title', (candidate) => candidate.length > 0 && candidate.length <= 120),
    pinned: typeof item.pinned === 'boolean' ? item.pinned : item.pinned_at !== null && item.pinned_at !== undefined,
    archived: typeof item.archived === 'boolean' ? item.archived : item.archived_at !== null && item.archived_at !== undefined,
    created_at: requiredString(item.created_at, 'created_at'),
    last_active_at: requiredString(item.last_active_at, 'last_active_at'),
    turn_count: typeof item.turn_count === 'number' ? item.turn_count : Number(item.turn_count ?? 0),
    preview: typeof item.preview === 'string' ? item.preview : null,
  };
}

export async function listMemberThreads(
  input: { verifiedAuthUserId: string; includeArchived?: boolean },
  client: MemberChatRpcClient = configuredClient(),
): Promise<MemberChatThread[]> {
  const userId = canonicalMemberChatVerifiedIdentity(input.verifiedAuthUserId);
  const { data, error } = await client.rpc('apocrypha_list_member_threads', {
    p_verified_auth_user_id: userId,
    p_include_archived: input.includeArchived === true,
  });
  if (error && migration0057Pending(error)) return [];
  if (error) storeFailure('thread listing', error);
  return rows(data).map(normalizeThread);
}

export async function createMemberThread(
  input: { verifiedAuthUserId: string; title?: string | null },
  client: MemberChatRpcClient = configuredClient(),
): Promise<MemberChatThread> {
  const userId = canonicalMemberChatVerifiedIdentity(input.verifiedAuthUserId);
  const title = typeof input.title === 'string' ? input.title.trim().slice(0, 120) : null;
  const { data, error } = await client.rpc('apocrypha_create_member_thread', {
    p_verified_auth_user_id: userId,
    p_title: title && title.length > 0 ? title : null,
  });
  if (error) storeFailure('thread creation', error);
  return normalizeThread(rows(data)[0] ?? data);
}

export async function updateMemberThread(
  input: { verifiedAuthUserId: string; threadId: string; title?: string | null; pinned?: boolean | null; archived?: boolean | null },
  client: MemberChatRpcClient = configuredClient(),
): Promise<MemberChatThread> {
  const userId = canonicalMemberChatVerifiedIdentity(input.verifiedAuthUserId);
  const threadId = input.threadId.toLowerCase();
  if (!isMemberChatUuid(threadId)) {
    throw new MemberChatStoreError(400, 'MEMBER_CHAT_THREAD_ID_INVALID', 'The member chat thread id is invalid.');
  }
  const title = typeof input.title === 'string' ? input.title.trim().slice(0, 120) : null;
  const { data, error } = await client.rpc('apocrypha_update_member_thread', {
    p_verified_auth_user_id: userId,
    p_thread_id: threadId,
    p_title: title && title.length > 0 ? title : null,
    p_pinned: typeof input.pinned === 'boolean' ? input.pinned : null,
    p_archived: typeof input.archived === 'boolean' ? input.archived : null,
  });
  if (error) storeFailure('thread update', error);
  return normalizeThread(rows(data)[0] ?? data);
}

// ─── the premium plan (Stripe entitlement) ──────────────────────────────────

export async function getMemberPlan(
  input: { verifiedAuthUserId: string },
  client: MemberChatRpcClient = configuredClient(),
): Promise<MemberPlan> {
  const userId = canonicalMemberChatVerifiedIdentity(input.verifiedAuthUserId);
  const { data, error } = await client.rpc('apocrypha_member_has_flagship', { p_verified_auth_user_id: userId });
  if (error && migration0057Pending(error)) {
    return { flagship: false, default_lane: 'local', product_id: APOCRYPHA_PREMIUM_PRODUCT_ID, degraded: 'migration_0057_pending' };
  }
  if (error) storeFailure('plan lookup', error);
  const flagship = data === true;
  // Owner decision 2026-09-25: the flagship is the main lane; free members run local.
  return { flagship, default_lane: flagship ? 'flagship' : 'local', product_id: APOCRYPHA_PREMIUM_PRODUCT_ID };
}

// ─── consent-gated analytics ────────────────────────────────────────────────

export async function setMemberConsent(
  input: { verifiedAuthUserId: string; analytics: boolean },
  client: MemberChatRpcClient = configuredClient(),
): Promise<{ analytics: boolean; updated_at: string }> {
  const userId = canonicalMemberChatVerifiedIdentity(input.verifiedAuthUserId);
  const { data, error } = await client.rpc('apocrypha_set_member_consent', {
    p_verified_auth_user_id: userId,
    p_analytics: input.analytics === true,
  });
  if (error) storeFailure('consent update', error);
  const row = record(rows(data)[0] ?? data);
  return { analytics: row?.analytics === true, updated_at: typeof row?.updated_at === 'string' ? row.updated_at : new Date().toISOString() };
}

export async function recordAnalyticsEvent(
  input: { authUserId: string | null; kind: string; props?: Record<string, unknown> },
  client: MemberChatRpcClient = configuredClient(),
): Promise<boolean> {
  if (!/^[a-z][a-z0-9_.]{1,63}$/.test(input.kind)) return false;
  const { data, error } = await client.rpc('apocrypha_record_analytics_event', {
    p_auth_user_id: input.authUserId ? canonicalMemberChatVerifiedIdentity(input.authUserId) : null,
    p_kind: input.kind,
    p_props: input.props ?? {},
  });
  if (error) return false;
  return data === true;
}

// ─── attachments (the plus icon) ─────────────────────────────────────────────

export const MEMBER_CHAT_ATTACHMENT_MAX_BYTES = 25 * 1024 * 1024;
export const MEMBER_CHAT_ATTACHMENT_TEXT_MAX_BYTES = 65_536;
export const MEMBER_CHAT_ATTACHMENT_BUCKET = 'apocrypha-attachments';

export function attachmentTextFrom(mime: string, bytes: Buffer): string | null {
  const textual = mime.startsWith('text/') || /^application\/(json|xml|x-yaml|yaml|csv|toml|javascript|typescript|x-python|sql|markdown)$/.test(mime);
  if (!textual) return null;
  const text = bytes.subarray(0, MEMBER_CHAT_ATTACHMENT_TEXT_MAX_BYTES).toString('utf8').replace(/\uFFFD/g, '');
  return text.length ? text : null;
}

export async function registerMemberAttachment(
  input: { verifiedAuthUserId: string; threadId: string | null; fileName: string; mimeType: string; byteSize: number; storagePath: string; extractedText: string | null },
  client: MemberChatRpcClient = configuredClient(),
): Promise<MemberChatAttachment> {
  const userId = canonicalMemberChatVerifiedIdentity(input.verifiedAuthUserId);
  const { data, error } = await client.rpc('apocrypha_register_member_attachment', {
    p_verified_auth_user_id: userId,
    p_thread_id: input.threadId ? input.threadId.toLowerCase() : null,
    p_file_name: input.fileName.slice(0, 255),
    p_mime_type: input.mimeType.slice(0, 120),
    p_byte_size: input.byteSize,
    p_storage_path: input.storagePath,
    p_extracted_text: input.extractedText,
  });
  if (error) storeFailure('attachment registration', error);
  const row = record(rows(data)[0] ?? data);
  if (!row) invalidProjection('The attachment row was missing.');
  return {
    id: requiredString(row.id, 'id', isMemberChatUuid).toLowerCase(),
    thread_id: requiredString(row.thread_id, 'thread_id', isMemberChatUuid).toLowerCase(),
    file_name: requiredString(row.file_name, 'file_name'),
    mime_type: requiredString(row.mime_type, 'mime_type'),
    byte_size: Number(row.byte_size),
    has_text: typeof row.extracted_text === 'string' && row.extracted_text.length > 0,
    created_at: requiredString(row.created_at, 'created_at'),
  };
}
