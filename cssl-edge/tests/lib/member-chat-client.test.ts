import assert from 'node:assert/strict';

import {
  MEMBER_CHAT_POLL_TIMEOUT_MS,
  MemberChatClientError,
  clearMemberChatPending,
  fetchMemberChatHistory,
  fetchMemberChatHistoryPage,
  memberChatStatusText,
  mergeMemberChatHistory,
  normalizeMemberChatMessage,
  pollMemberChatJob,
  projectMemberChatMessages,
  readMemberChatPending,
  saveMemberChatPending,
  submitMemberChatJob,
  type MemberChatFetch,
  type MemberChatHistoryEntry,
  type MemberChatPendingSubmission,
  type MemberChatStorage,
} from '../../lib/apocrypha/member-chat-client';

const USER_ID = 'aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa';
const CONVERSATION_ID = USER_ID;
const REQUEST_ID = '22222222-2222-4222-8222-222222222222';
const JOB_ID = '33333333-3333-4333-8333-333333333333';
const MEMORY_HASH = 'a'.repeat(64);

function job(
  status: MemberChatHistoryEntry['status'],
  overrides: Partial<MemberChatHistoryEntry> = {},
): MemberChatHistoryEntry {
  return {
    job_id: JOB_ID,
    conversation_id: CONVERSATION_ID,
    request_id: REQUEST_ID,
    status,
    user_message: 'Tell me something durable.',
    assistant_message: status === 'succeeded' ? 'This reply came back from the durable job.' : null,
    assistant_truncated: false,
    model_alias: 'qwen3:30b-a3b',
    memory_manifest_hash: MEMORY_HASH,
    created_at: '2026-09-08T12:00:00.000Z',
    updated_at: '2026-09-08T12:00:01.000Z',
    completed_at: status === 'succeeded' || status === 'failed' || status === 'cancelled'
      ? '2026-09-08T12:00:02.000Z'
      : null,
    error_code: status === 'failed' ? 'MODEL_FAILED' : null,
    ...overrides,
  };
}

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'Content-Type': 'application/json' },
  });
}

class MemoryStorage implements MemberChatStorage {
  readonly values = new Map<string, string>();
  getItem(key: string): string | null { return this.values.get(key) ?? null; }
  setItem(key: string, value: string): void { this.values.set(key, value); }
  removeItem(key: string): void { this.values.delete(key); }
}

async function main(): Promise<void> {
  let passed = 0;

// 1. History is read from the member route, validated, ordered, and projected as chat messages.
{
  const calls: string[] = [];
  const later = job('succeeded', {
    job_id: '44444444-4444-4444-8444-444444444444',
    request_id: '55555555-5555-4555-8555-555555555555',
    user_message: 'Second message',
    assistant_message: 'Second reply',
    created_at: '2026-09-08T12:01:00.000Z',
  });
  const fetcher: MemberChatFetch = async (input) => {
    calls.push(String(input));
    return jsonResponse({
      ok: true,
      conversation_id: CONVERSATION_ID,
      history: [later, job('succeeded')],
      count: 2,
      next_cursor: null,
    });
  };
  const history = await fetchMemberChatHistory(CONVERSATION_ID, fetcher);
  assert.deepEqual(calls, [`/api/apocrypha/member/history?conversation_id=${CONVERSATION_ID}`]);
  assert.equal(history[0]?.request_id, REQUEST_ID, 'history must be chronological even if the payload is not');
  assert.deepEqual(
    projectMemberChatMessages(history, null).map((message) => [message.role, message.content]),
    [
      ['user', 'Tell me something durable.'],
      ['assistant', 'This reply came back from the durable job.'],
      ['user', 'Second message'],
      ['assistant', 'Second reply'],
    ],
  );
  passed += 1;
}

// 2. Older pages use the server cursor unchanged and merge without duplicate turns.
{
  const calls: string[] = [];
  const older = job('succeeded', {
    job_id: '66666666-6666-4666-8666-666666666666',
    request_id: '77777777-7777-4777-8777-777777777777',
    user_message: 'An earlier message',
    assistant_message: 'An earlier reply',
    created_at: '2026-09-08T11:59:00.000Z',
  });
  const fetcher: MemberChatFetch = async (input) => {
    calls.push(String(input));
    return jsonResponse({
      ok: true,
      conversation_id: CONVERSATION_ID,
      history: [older],
      count: 1,
      next_cursor: '12',
    });
  };
  const page = await fetchMemberChatHistoryPage(CONVERSATION_ID, fetcher, { before: '50' });
  assert.deepEqual(calls, [
    `/api/apocrypha/member/history?conversation_id=${CONVERSATION_ID}&before=50`,
  ]);
  assert.equal(page.next_cursor, '12');
  assert.deepEqual(
    mergeMemberChatHistory(page.history, [job('succeeded')]).map((entry) => entry.user_message),
    ['An earlier message', 'Tell me something durable.'],
  );
  await assert.rejects(
    () => fetchMemberChatHistoryPage(CONVERSATION_ID, fetcher, { before: '0' }),
    (error: unknown) => error instanceof MemberChatClientError
      && error.code === 'MEMBER_CHAT_INPUT_INVALID',
  );
  passed += 1;
}

// 3. Submit sends the exact durable-job body and accepts the immediate receipt.
{
  let capturedUrl = '';
  let capturedInit: RequestInit | undefined;
  const fetcher: MemberChatFetch = async (input, init) => {
    capturedUrl = String(input);
    capturedInit = init;
    return jsonResponse({
      ok: true,
      accepted: true,
      replayed: false,
      job: {
        job_id: JOB_ID,
        conversation_id: CONVERSATION_ID,
        request_id: REQUEST_ID,
        status: 'queued',
        model_alias: 'qwen3:30b-a3b',
        memory_manifest_hash: MEMORY_HASH,
        created_at: '2026-09-08T12:00:00.000Z',
        updated_at: '2026-09-08T12:00:00.000Z',
        replayed: false,
      },
    }, 202);
  };
  const receipt = await submitMemberChatJob({
    conversationId: CONVERSATION_ID,
    requestId: REQUEST_ID,
    message: 'Tell me something durable.',
  }, fetcher);
  assert.equal(capturedUrl, '/api/apocrypha/member/jobs');
  assert.equal(capturedInit?.method, 'POST');
  assert.equal(new Headers(capturedInit?.headers).get('content-type'), 'application/json');
  assert.deepEqual(JSON.parse(String(capturedInit?.body)), {
    conversation_id: CONVERSATION_ID,
    request_id: REQUEST_ID,
    message: 'Tell me something durable.',
  });
  assert.equal(receipt.job_id, JOB_ID);
  assert.equal(receipt.status, 'queued');
  passed += 1;
}

// 4. Polling tolerates a transient outage and follows queued/running work to completion.
{
  const responses = [
    jsonResponse({ ok: false, code: 'MEMBER_CHAT_STORAGE_UNAVAILABLE' }, 503),
    jsonResponse({ ok: true, job: job('queued') }),
    jsonResponse({ ok: true, job: job('running') }),
    jsonResponse({ ok: true, job: job('succeeded') }),
  ];
  let now = 0;
  let retries = 0;
  const states: string[] = [];
  const fetcher: MemberChatFetch = async (input) => {
    assert.equal(String(input), `/api/apocrypha/member/jobs/${JOB_ID}`);
    const response = responses.shift();
    assert(response, 'poll made more requests than expected');
    return response;
  };
  const final = await pollMemberChatJob(JOB_ID, {
    fetcher,
    intervalMs: 10,
    timeoutMs: 100,
    now: () => now,
    sleep: async (delay) => { now += delay; },
    onRetry: () => { retries += 1; },
    onJob: (entry) => states.push(entry.status),
  });
  assert.equal(retries, 1);
  assert.deepEqual(states, ['queued', 'running', 'succeeded']);
  assert.equal(final.assistant_message, 'This reply came back from the durable job.');
  assert(MEMBER_CHAT_POLL_TIMEOUT_MS >= 20 * 60_000, 'the live poll budget must allow a long Qwen turn');
  passed += 1;
}

// 5. An unconfirmed send survives a reload and de-duplicates once durable history records it.
{
  const storage = new MemoryStorage();
  const pending: MemberChatPendingSubmission = {
    conversation_id: CONVERSATION_ID,
    request_id: REQUEST_ID,
    message: 'Tell me something durable.',
    created_at: '2026-09-08T12:00:00.000Z',
  };
  saveMemberChatPending(USER_ID, storage, pending);

  // A new component instance derives the same cross-device conversation from
  // the account UUID and restores its browser-local optimistic message.
  const restored = readMemberChatPending(USER_ID, storage);
  assert.deepEqual(restored, pending);
  assert.equal(restored?.conversation_id, USER_ID);
  assert.deepEqual(
    projectMemberChatMessages([], restored).map((message) => message.content),
    ['Tell me something durable.'],
  );

  // Once the server records the request, only the durable user/reply pair remains.
  assert.deepEqual(
    projectMemberChatMessages([job('succeeded')], restored).map((message) => message.content),
    ['Tell me something durable.', 'This reply came back from the durable job.'],
  );
  clearMemberChatPending(USER_ID, storage);
  assert.equal(readMemberChatPending(USER_ID, storage), null);
  passed += 1;
}

// 6. Public errors stay actionable, and a durable terminal failure becomes sendable again.
{
  await assert.rejects(
    () => fetchMemberChatHistory(CONVERSATION_ID, async () => jsonResponse({
      ok: false,
      code: 'MEMBER_SESSION_REQUIRED',
      error: 'internal text must not be surfaced',
    }, 401)),
    (error: unknown) => {
      assert(error instanceof MemberChatClientError);
      assert.equal(error.code, 'MEMBER_SESSION_REQUIRED');
      assert.equal(error.message, 'Your sign-in expired. Sign in again to continue.');
      assert.equal(error.retryable, false);
      return true;
    },
  );
  const failed = await pollMemberChatJob(JOB_ID, {
    fetcher: async () => jsonResponse({ ok: true, job: job('failed') }),
    sleep: async () => undefined,
  });
  assert.equal(failed.status, 'failed');
  assert.match(memberChatStatusText(failed), /send the message again/i);
  passed += 1;
}

// 7. Caller cancellation remains active while a successful response body is still decoding.
{
  const controller = new AbortController();
  let decodingStarted: () => void = () => undefined;
  const started = new Promise<void>((resolve) => { decodingStarted = resolve; });
  const fetcher: MemberChatFetch = async () => ({
    ok: true,
    status: 200,
    json: async () => {
      decodingStarted();
      return await new Promise<never>(() => undefined);
    },
  } as unknown as Response);
  const request = fetchMemberChatHistory(CONVERSATION_ID, fetcher, controller.signal);
  await started;
  controller.abort();
  await assert.rejects(request, (error: unknown) => error instanceof Error && error.name === 'AbortError');
  passed += 1;
}

// 8. The absolute request deadline also covers a non-cooperative fetch implementation.
{
  const originalSetTimeout = globalThis.setTimeout;
  globalThis.setTimeout = ((
    callback: (...args: unknown[]) => void,
    delay?: number,
    ...args: unknown[]
  ) => originalSetTimeout(callback, delay === 30_000 ? 0 : delay, ...args)) as typeof globalThis.setTimeout;
  try {
    await assert.rejects(
      () => fetchMemberChatHistory(CONVERSATION_ID, async () => await new Promise<Response>(() => undefined)),
      (error: unknown) => error instanceof MemberChatClientError
        && error.code === 'MEMBER_CHAT_REQUEST_TIMEOUT',
    );
  } finally {
    globalThis.setTimeout = originalSetTimeout;
  }
  passed += 1;
}

// 9. Client validation matches the server: ordinary line breaks remain valid, controls do not.
{
  assert.equal(normalizeMemberChatMessage('First line\nSecond line'), 'First line\nSecond line');
  assert.equal(normalizeMemberChatMessage('Visible\u0001hidden'), null);
  let called = false;
  await assert.rejects(
    () => submitMemberChatJob({
      conversationId: CONVERSATION_ID,
      requestId: REQUEST_ID,
      message: 'Visible\u0001hidden',
    }, async () => {
      called = true;
      return jsonResponse({ ok: true });
    }),
    (error: unknown) => error instanceof MemberChatClientError
      && error.code === 'MEMBER_CHAT_INPUT_INVALID',
  );
  assert.equal(called, false, 'invalid controls must be rejected before the network request');
  passed += 1;
}

console.log(`member-chat-client.test: ${passed}/9 passed`);
}

void main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
