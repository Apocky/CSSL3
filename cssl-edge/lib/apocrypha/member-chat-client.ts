export const MEMBER_CHAT_POLL_INTERVAL_MS = 2_500;
export const MEMBER_CHAT_POLL_TIMEOUT_MS = 20 * 60_000;
export const MEMBER_CHAT_MESSAGE_MAX_BYTES = 16_384;
export const MEMBER_CHAT_ASSISTANT_MAX_BYTES = 65_536;
export const MEMBER_CHAT_MAX_LOADED_HISTORY_PAGES = 4;

const MEMBER_CHAT_HISTORY_LIMIT = 50;
const MEMBER_CHAT_HISTORY_CONTENT_MAX_BYTES = 4_096_000;
const MEMBER_CHAT_REQUEST_TIMEOUT_MS = 30_000;

const UUID_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;
const HISTORY_CURSOR_RE = /^[1-9][0-9]{0,18}$/;
const DISALLOWED_MESSAGE_CONTROL_RE = /[\u0000-\u0008\u000b\u000c\u000e-\u001f\u007f]/u;
const MAX_POSTGRES_BIGINT = 9_223_372_036_854_775_807n;
const ACTIVE_STATUSES = new Set<MemberChatJobStatus>(['queued', 'leased', 'running', 'cancel_requested']);
const TERMINAL_STATUSES = new Set<MemberChatJobStatus>(['succeeded', 'failed', 'cancelled']);
const ALL_STATUSES = new Set<MemberChatJobStatus>([...ACTIVE_STATUSES, ...TERMINAL_STATUSES]);

export type MemberChatJobStatus =
  | 'queued'
  | 'leased'
  | 'running'
  | 'cancel_requested'
  | 'succeeded'
  | 'failed'
  | 'cancelled';

export interface MemberChatJobReceipt {
  job_id: string;
  conversation_id: string;
  request_id: string;
  status: MemberChatJobStatus;
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
  status: MemberChatJobStatus;
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

export interface MemberChatPendingSubmission {
  conversation_id: string;
  request_id: string;
  message: string;
  created_at: string;
  job_id?: string;
}

export interface MemberChatDisplayMessage {
  key: string;
  role: 'user' | 'assistant';
  content: string;
  request_id: string;
  recorded_at: string;
  truncated?: boolean;
}

export interface MemberChatHistoryPage {
  history: MemberChatHistoryEntry[];
  next_cursor: string | null;
}

export type MemberChatFetch = (input: RequestInfo | URL, init?: RequestInit) => Promise<Response>;
export type MemberChatStorage = Pick<Storage, 'getItem' | 'setItem' | 'removeItem'>;

export class MemberChatClientError extends Error {
  constructor(
    readonly code: string,
    message: string,
    readonly status: number,
    readonly retryable: boolean,
  ) {
    super(message);
    this.name = 'MemberChatClientError';
  }
}

interface PollOptions {
  fetcher: MemberChatFetch;
  signal?: AbortSignal;
  intervalMs?: number;
  timeoutMs?: number;
  now?: () => number;
  sleep?: (delayMs: number, signal?: AbortSignal) => Promise<void>;
  onJob?: (job: MemberChatHistoryEntry) => void;
  onRetry?: (error: MemberChatClientError) => void;
}

function record(value: unknown): Record<string, unknown> | null {
  return value !== null && typeof value === 'object' && !Array.isArray(value)
    ? value as Record<string, unknown>
    : null;
}

function requiredString(value: unknown): string | null {
  return typeof value === 'string' && value.length > 0 ? value : null;
}

function nullableString(value: unknown): string | null | undefined {
  return value === null ? null : typeof value === 'string' ? value : undefined;
}

function abortError(): Error {
  const error = new Error('Stopped waiting.');
  error.name = 'AbortError';
  return error;
}

function requestTimeoutError(): MemberChatClientError {
  return new MemberChatClientError(
    'MEMBER_CHAT_REQUEST_TIMEOUT',
    'The connection took too long. Your message is saved here; refresh and try again.',
    408,
    true,
  );
}

function networkError(): MemberChatClientError {
  return new MemberChatClientError(
    'MEMBER_CHAT_NETWORK_UNAVAILABLE',
    'The connection dropped. Your message is saved here; reconnect and try again.',
    0,
    true,
  );
}

function defaultSleep(delayMs: number, signal?: AbortSignal): Promise<void> {
  if (signal?.aborted) return Promise.reject(abortError());
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => {
      signal?.removeEventListener('abort', onAbort);
      resolve();
    }, delayMs);
    const onAbort = () => {
      clearTimeout(timer);
      signal?.removeEventListener('abort', onAbort);
      reject(abortError());
    };
    signal?.addEventListener('abort', onAbort, { once: true });
  });
}

function friendlyError(code: string, status: number): { message: string; retryable: boolean } {
  if (code === 'MEMBER_SESSION_REQUIRED' || status === 401) {
    return { message: 'Your sign-in expired. Sign in again to continue.', retryable: false };
  }
  if (code === 'MEMBER_ORIGIN_DENIED' || status === 403) {
    return { message: 'Reload this chat from Apocky before trying again.', retryable: false };
  }
  if (code === 'MEMBER_CHAT_TURN_IN_PROGRESS') {
    return { message: 'This conversation is already waiting for a reply. Refresh to follow it.', retryable: true };
  }
  if (code === 'MEMBER_CHAT_REPLAY_CONFLICT') {
    return { message: 'This message could not be retried safely. Refresh the conversation before sending again.', retryable: false };
  }
  if (code === 'MEMBER_CHAT_JOB_NOT_FOUND' || status === 404) {
    return { message: 'This saved reply could not be found. Refresh the conversation to recover its latest state.', retryable: false };
  }
  if (code === 'MEMBER_CHAT_INPUT_INVALID' || status === 400 || status === 415) {
    return { message: 'That message could not be sent. Check it and try again.', retryable: false };
  }
  return {
    message: 'Chat is temporarily unavailable. Your message is saved here; try again in a moment.',
    retryable: status === 0 || status === 408 || status === 429 || status >= 500,
  };
}

async function responseFailure(response: Response): Promise<MemberChatClientError> {
  let code = 'MEMBER_CHAT_REQUEST_FAILED';
  try {
    const body = record(await response.json());
    if (body && typeof body.code === 'string') code = body.code;
  } catch {
    // A bounded public message is safer and more useful than leaking an upstream body.
  }
  const friendly = friendlyError(code, response.status);
  return new MemberChatClientError(code, friendly.message, response.status, friendly.retryable);
}

function invalidResponse(): MemberChatClientError {
  return new MemberChatClientError(
    'MEMBER_CHAT_RESPONSE_INVALID',
    'Chat returned an incomplete update. Refresh the conversation to try again.',
    502,
    true,
  );
}

async function requestJson(
  fetcher: MemberChatFetch,
  input: RequestInfo | URL,
  init?: RequestInit,
): Promise<{ response: Response; body: Record<string, unknown> }> {
  const callerSignal = init?.signal ?? undefined;
  if (callerSignal?.aborted) throw abortError();
  const controller = new AbortController();
  let timedOut = false;
  let timeout: ReturnType<typeof setTimeout> | null = null;
  let rejectCallerAbort: (error: Error) => void = () => undefined;
  const callerAbort = new Promise<never>((_resolve, reject) => {
    rejectCallerAbort = reject;
  });
  const abortFromCaller = () => {
    controller.abort();
    rejectCallerAbort(abortError());
  };
  callerSignal?.addEventListener('abort', abortFromCaller, { once: true });
  if (callerSignal?.aborted) abortFromCaller();
  const deadline = new Promise<never>((_resolve, reject) => {
    timeout = setTimeout(() => {
      timedOut = true;
      controller.abort();
      reject(requestTimeoutError());
    }, MEMBER_CHAT_REQUEST_TIMEOUT_MS);
  });

  const operation = (async (): Promise<{ response: Response; body: Record<string, unknown> }> => {
    let response: Response;
    try {
      response = await fetcher(input, { ...init, signal: controller.signal });
    } catch (error) {
      if (callerSignal?.aborted) throw abortError();
      if (timedOut) throw requestTimeoutError();
      if (error instanceof MemberChatClientError) throw error;
      throw networkError();
    }
    if (!response.ok) {
      const failure = await responseFailure(response);
      if (callerSignal?.aborted) throw abortError();
      if (timedOut) throw requestTimeoutError();
      throw failure;
    }
    try {
      const body = record(await response.json());
      if (!body) throw invalidResponse();
      return { response, body };
    } catch (error) {
      if (callerSignal?.aborted) throw abortError();
      if (timedOut) throw requestTimeoutError();
      if (error instanceof MemberChatClientError) throw error;
      if (error instanceof Error && error.name === 'AbortError') throw networkError();
      throw invalidResponse();
    }
  })();

  try {
    return await Promise.race([operation, deadline, callerAbort]);
  } finally {
    if (timeout !== null) clearTimeout(timeout);
    callerSignal?.removeEventListener('abort', abortFromCaller);
  }
}

function parseStatus(value: unknown): MemberChatJobStatus | null {
  return typeof value === 'string' && ALL_STATUSES.has(value as MemberChatJobStatus)
    ? value as MemberChatJobStatus
    : null;
}

function parseReceipt(value: unknown): MemberChatJobReceipt | null {
  const item = record(value);
  if (!item) return null;
  const jobId = requiredString(item.job_id);
  const conversationId = requiredString(item.conversation_id);
  const requestId = requiredString(item.request_id);
  const status = parseStatus(item.status);
  const modelAlias = requiredString(item.model_alias);
  const memoryHash = requiredString(item.memory_manifest_hash);
  const createdAt = requiredString(item.created_at);
  const updatedAt = requiredString(item.updated_at);
  if (
    !jobId || !isMemberChatUuid(jobId)
    || !conversationId || !isMemberChatUuid(conversationId)
    || !requestId || !isMemberChatUuid(requestId)
    || !status || !modelAlias || !memoryHash || !/^[0-9a-f]{64}$/.test(memoryHash)
    || !createdAt || !updatedAt || typeof item.replayed !== 'boolean'
  ) return null;
  return {
    job_id: jobId.toLowerCase(),
    conversation_id: conversationId.toLowerCase(),
    request_id: requestId.toLowerCase(),
    status,
    model_alias: modelAlias,
    memory_manifest_hash: memoryHash,
    created_at: createdAt,
    updated_at: updatedAt,
    replayed: item.replayed,
  };
}

function parseHistoryEntry(value: unknown): MemberChatHistoryEntry | null {
  const item = record(value);
  if (!item) return null;
  const receipt = parseReceipt({ ...item, replayed: false });
  const userMessage = requiredString(item.user_message);
  const assistantMessage = nullableString(item.assistant_message);
  const completedAt = nullableString(item.completed_at);
  const errorCode = nullableString(item.error_code);
  if (
    !receipt || !userMessage || normalizeMemberChatMessage(userMessage) !== userMessage
    || (assistantMessage !== null && assistantMessage !== undefined
      && new TextEncoder().encode(assistantMessage).length > MEMBER_CHAT_ASSISTANT_MAX_BYTES)
    || assistantMessage === undefined || completedAt === undefined || errorCode === undefined
    || typeof item.assistant_truncated !== 'boolean'
  ) return null;
  return {
    ...receipt,
    user_message: userMessage,
    assistant_message: assistantMessage,
    assistant_truncated: item.assistant_truncated,
    completed_at: completedAt,
    error_code: errorCode,
  };
}

function storageKey(subject: string): string {
  return `apocky.member-chat.pending.v1.${encodeURIComponent(subject)}`;
}

export function isMemberChatUuid(value: string): boolean {
  return UUID_RE.test(value);
}

export function isActiveMemberChatStatus(status: MemberChatJobStatus): boolean {
  return ACTIVE_STATUSES.has(status);
}

export function normalizeMemberChatMessage(value: string): string | null {
  const message = value.trim();
  if (!message || DISALLOWED_MESSAGE_CONTROL_RE.test(message)) return null;
  return new TextEncoder().encode(message).length <= MEMBER_CHAT_MESSAGE_MAX_BYTES ? message : null;
}

export function readMemberChatPending(
  subject: string,
  storage: MemberChatStorage,
): MemberChatPendingSubmission | null {
  try {
    const raw = storage.getItem(storageKey(subject));
    if (!raw) return null;
    const item = record(JSON.parse(raw));
    if (!item) return null;
    const conversationId = requiredString(item.conversation_id);
    const requestId = requiredString(item.request_id);
    const message = requiredString(item.message);
    const createdAt = requiredString(item.created_at);
    const jobId = item.job_id === undefined ? undefined : requiredString(item.job_id);
    if (
      !conversationId || !isMemberChatUuid(conversationId)
      || !requestId || !isMemberChatUuid(requestId)
      || !message || normalizeMemberChatMessage(message) !== message
      || !createdAt || (jobId !== undefined && (!jobId || !isMemberChatUuid(jobId)))
    ) return null;
    return {
      conversation_id: conversationId.toLowerCase(),
      request_id: requestId.toLowerCase(),
      message,
      created_at: createdAt,
      ...(jobId ? { job_id: jobId.toLowerCase() } : {}),
    };
  } catch {
    return null;
  }
}

export function saveMemberChatPending(
  subject: string,
  storage: MemberChatStorage,
  pending: MemberChatPendingSubmission,
): void {
  storage.setItem(storageKey(subject), JSON.stringify(pending));
}

export function clearMemberChatPending(subject: string, storage: MemberChatStorage): void {
  try { storage.removeItem(storageKey(subject)); } catch { /* storage unavailable */ }
}

function canonicalMemberChatCursor(value: unknown): string | null {
  if (typeof value !== 'string' || !HISTORY_CURSOR_RE.test(value)) return null;
  try {
    return BigInt(value) <= MAX_POSTGRES_BIGINT ? value : null;
  } catch {
    return null;
  }
}

export async function fetchMemberChatHistoryPage(
  conversationId: string,
  fetcher: MemberChatFetch,
  options: { before?: string | null; signal?: AbortSignal } = {},
): Promise<MemberChatHistoryPage> {
  const before = options.before ?? null;
  if (!isMemberChatUuid(conversationId) || (before !== null && canonicalMemberChatCursor(before) !== before)) {
    throw new MemberChatClientError(
      'MEMBER_CHAT_INPUT_INVALID',
      'This conversation could not be loaded. Refresh to try again.',
      400,
      false,
    );
  }
  const query = `conversation_id=${encodeURIComponent(conversationId)}`
    + (before === null ? '' : `&before=${encodeURIComponent(before)}`);
  const { body } = await requestJson(
    fetcher,
    `/api/apocrypha/member/history?${query}`,
    { cache: 'no-store', signal: options.signal },
  );
  const nextCursor = body.next_cursor === null ? null : canonicalMemberChatCursor(body.next_cursor);
  if (
    body.ok !== true || body.conversation_id !== conversationId || !Array.isArray(body.history)
    || body.history.length > MEMBER_CHAT_HISTORY_LIMIT || body.count !== body.history.length
    || (body.next_cursor !== null && nextCursor === null)
    || (body.history.length === 0 && nextCursor !== null)
    || (before !== null && nextCursor !== null && BigInt(nextCursor) >= BigInt(before))
  ) {
    throw invalidResponse();
  }
  const history = body.history.map(parseHistoryEntry);
  if (history.some((entry) => !entry || entry.conversation_id !== conversationId)) throw invalidResponse();
  const projected = history as MemberChatHistoryEntry[];
  const contentBytes = projected.reduce((total, entry) => total
    + new TextEncoder().encode(entry.user_message).length
    + (entry.assistant_message ? new TextEncoder().encode(entry.assistant_message).length : 0), 0);
  if (contentBytes > MEMBER_CHAT_HISTORY_CONTENT_MAX_BYTES) throw invalidResponse();
  return {
    history: projected.sort((left, right) => left.created_at.localeCompare(right.created_at)),
    next_cursor: nextCursor,
  };
}

export async function fetchMemberChatHistory(
  conversationId: string,
  fetcher: MemberChatFetch,
  signal?: AbortSignal,
): Promise<MemberChatHistoryEntry[]> {
  return (await fetchMemberChatHistoryPage(conversationId, fetcher, { signal })).history;
}

export async function submitMemberChatJob(
  input: { conversationId: string; requestId: string; message: string },
  fetcher: MemberChatFetch,
  signal?: AbortSignal,
): Promise<MemberChatJobReceipt> {
  const message = normalizeMemberChatMessage(input.message);
  if (!isMemberChatUuid(input.conversationId) || !isMemberChatUuid(input.requestId) || !message) {
    throw new MemberChatClientError(
      'MEMBER_CHAT_INPUT_INVALID',
      'That message could not be sent. Check it and try again.',
      400,
      false,
    );
  }
  const { body } = await requestJson(fetcher, '/api/apocrypha/member/jobs', {
    method: 'POST',
    cache: 'no-store',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({
      conversation_id: input.conversationId,
      request_id: input.requestId,
      message,
    }),
    signal,
  });
  const job = parseReceipt(body.job);
  if (
    body.ok !== true || body.accepted !== true || !job
    || job.conversation_id !== input.conversationId || job.request_id !== input.requestId
  ) throw invalidResponse();
  return job;
}

export async function fetchMemberChatJob(
  jobId: string,
  fetcher: MemberChatFetch,
  signal?: AbortSignal,
): Promise<MemberChatHistoryEntry> {
  const { body } = await requestJson(
    fetcher,
    `/api/apocrypha/member/jobs/${encodeURIComponent(jobId)}`,
    { cache: 'no-store', signal },
  );
  const job = parseHistoryEntry(body.job);
  if (body.ok !== true || !job || job.job_id !== jobId) throw invalidResponse();
  return job;
}

export async function pollMemberChatJob(
  jobId: string,
  options: PollOptions,
): Promise<MemberChatHistoryEntry> {
  if (!isMemberChatUuid(jobId)) throw invalidResponse();
  const intervalMs = options.intervalMs ?? MEMBER_CHAT_POLL_INTERVAL_MS;
  const timeoutMs = options.timeoutMs ?? MEMBER_CHAT_POLL_TIMEOUT_MS;
  const now = options.now ?? Date.now;
  const sleep = options.sleep ?? defaultSleep;
  const started = now();

  while (true) {
    if (options.signal?.aborted) throw abortError();
    try {
      const job = await fetchMemberChatJob(jobId, options.fetcher, options.signal);
      options.onJob?.(job);
      if (TERMINAL_STATUSES.has(job.status)) return job;
    } catch (error) {
      if (error instanceof Error && error.name === 'AbortError') throw error;
      if (!(error instanceof MemberChatClientError) || !error.retryable) throw error;
      options.onRetry?.(error);
    }

    const elapsed = now() - started;
    if (elapsed >= timeoutMs) {
      throw new MemberChatClientError(
        'MEMBER_CHAT_STILL_WORKING',
        'Apocrypha is still working. Your message is saved; refresh this conversation later for the reply.',
        408,
        true,
      );
    }
    const delay = Math.min(5_000, intervalMs + Math.floor(elapsed / 60_000) * 500);
    await sleep(delay, options.signal);
  }
}

export function upsertMemberChatHistory(
  history: MemberChatHistoryEntry[],
  job: MemberChatHistoryEntry,
): MemberChatHistoryEntry[] {
  const next = history.filter((entry) => entry.job_id !== job.job_id);
  next.push(job);
  return next.sort((left, right) => left.created_at.localeCompare(right.created_at));
}

export function mergeMemberChatHistory(
  older: MemberChatHistoryEntry[],
  newer: MemberChatHistoryEntry[],
): MemberChatHistoryEntry[] {
  const byJob = new Map<string, MemberChatHistoryEntry>();
  for (const entry of older) byJob.set(entry.job_id, entry);
  for (const entry of newer) byJob.set(entry.job_id, entry);
  return [...byJob.values()].sort((left, right) => left.created_at.localeCompare(right.created_at));
}

export function activeMemberChatJob(history: MemberChatHistoryEntry[]): MemberChatHistoryEntry | null {
  return [...history]
    .reverse()
    .find((entry) => isActiveMemberChatStatus(entry.status)) ?? null;
}

export function projectMemberChatMessages(
  history: MemberChatHistoryEntry[],
  pending: MemberChatPendingSubmission | null,
): MemberChatDisplayMessage[] {
  const messages: MemberChatDisplayMessage[] = [];
  const recordedRequests = new Set<string>();
  for (const entry of history) {
    recordedRequests.add(entry.request_id);
    messages.push({
      key: `${entry.job_id}:user`,
      role: 'user',
      content: entry.user_message,
      request_id: entry.request_id,
      recorded_at: entry.created_at,
    });
    if (entry.assistant_message !== null) {
      messages.push({
        key: `${entry.job_id}:assistant`,
        role: 'assistant',
        content: entry.assistant_message,
        request_id: entry.request_id,
        recorded_at: entry.completed_at ?? entry.updated_at,
        truncated: entry.assistant_truncated,
      });
    }
  }
  if (pending && !recordedRequests.has(pending.request_id)) {
    messages.push({
      key: `${pending.request_id}:pending`,
      role: 'user',
      content: pending.message,
      request_id: pending.request_id,
      recorded_at: pending.created_at,
    });
  }
  return messages;
}

export function memberChatStatusText(job: MemberChatHistoryEntry | null): string {
  if (!job || job.status === 'queued') return 'Message saved. Waiting for Apocrypha…';
  if (job.status === 'leased' || job.status === 'running') return 'Apocrypha is responding…';
  if (job.status === 'cancel_requested') return 'Apocrypha is finishing up…';
  if (job.status === 'failed') return 'Apocrypha could not finish this reply. You can send the message again.';
  if (job.status === 'cancelled') return 'This reply was stopped. You can send another message.';
  return '';
}
