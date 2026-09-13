import assert from 'node:assert/strict';

import type { NextApiRequest, NextApiResponse } from 'next';

import { resetApocryphaServiceClientForTests } from '@/lib/apocrypha/job-control';
import conversationsHandler from '@/pages/api/admin/apocrypha/conversations';
import jobsHandler from '@/pages/api/admin/apocrypha/jobs';

interface Output {
  statusCode: number;
  body: unknown;
  headers: Record<string, string>;
}

const TENANT_ID = '10000000-0000-4000-8000-000000000001';
const PRINCIPAL_ID = '10000000-0000-4000-8000-000000000002';
const CONVERSATION_ID = '11111111-1111-4111-8111-111111111111';
const FIRST_JOB_ID = '22222222-2222-4222-8222-222222222222';
const SECOND_JOB_ID = '33333333-3333-4333-8333-333333333333';
const LEGACY_JOB_ID = '44444444-4444-4444-8444-444444444444';
const FIRST_REVISION_ID = '55555555-5555-4555-8555-555555555555';
const SECOND_REVISION_ID = '66666666-6666-4666-8666-666666666666';
const LEGACY_REVISION_ID = '77777777-7777-4777-8777-777777777777';
const OLD_CONVERSATION_ID = '88888888-8888-4888-8888-888888888888';
const OLD_JOB_ID = '88888888-8888-4888-8888-888888888889';
const OLD_REVISION_ID = '88888888-8888-4888-8888-888888888890';
const RETRY_KEY = 'owner-history-retry-key';
const NULL_CONVERSATION_RETRY_KEY = 'owner-history-null-conversation-retry-key';
const FAILED_JOB_ID = '99999999-9999-4999-8999-999999999991';
const RETRIED_JOB_ID = '99999999-9999-4999-8999-999999999992';
const RETRIED_REVISION_ID = '99999999-9999-4999-8999-999999999993';

function reqRes(
  method: string,
  options: { body?: unknown; query?: Record<string, string | string[]> } = {},
): { req: NextApiRequest; res: NextApiResponse; out: Output } {
  const out: Output = { statusCode: 0, body: null, headers: {} };
  const req = {
    method,
    body: options.body,
    query: options.query ?? {},
    headers: {
      host: 'www.apocky.com',
      origin: 'https://www.apocky.com',
      'x-forwarded-proto': 'https',
      'x-apocky-test-admin-email': 'owner@example.test',
    },
  } as unknown as NextApiRequest;
  const res = {
    status(code: number) { out.statusCode = code; return this; },
    json(body: unknown) { out.body = body; return this; },
    setHeader(name: string, value: string | number | readonly string[]) {
      out.headers[name.toLowerCase()] = Array.isArray(value) ? value.join(', ') : String(value);
      return this;
    },
  } as unknown as NextApiResponse;
  return { req, res, out };
}

function jsonResponse(body: unknown, status = 200): Response {
  return new Response(JSON.stringify(body), {
    status,
    headers: { 'content-type': 'application/json' },
  });
}

function requestOf(jobId: string, prompt: string, conversationId: string | null) {
  return {
    id: jobId,
    status: 'succeeded',
    request: { prompt, conversation_id: conversationId },
    terminal_revision_id: jobId === FIRST_JOB_ID
      ? FIRST_REVISION_ID
      : jobId === SECOND_JOB_ID ? SECOND_REVISION_ID : LEGACY_REVISION_ID,
    created_at: jobId === FIRST_JOB_ID
      ? '2026-09-08T10:00:00.000Z'
      : jobId === SECOND_JOB_ID ? '2026-09-08T10:01:00.000Z' : '2026-09-08T10:02:00.000Z',
    updated_at: jobId === FIRST_JOB_ID
      ? '2026-09-08T10:00:30.000Z'
      : jobId === SECOND_JOB_ID ? '2026-09-08T10:01:30.000Z' : '2026-09-08T10:02:30.000Z',
    completed_at: jobId === FIRST_JOB_ID
      ? '2026-09-08T10:00:30.000Z'
      : jobId === SECOND_JOB_ID ? '2026-09-08T10:01:30.000Z' : '2026-09-08T10:02:30.000Z',
  };
}

const originalEnv = { ...process.env };
const originalFetch = globalThis.fetch;

async function main(): Promise<void> {
  process.env.LAZARUS_TEST_AUTH_BYPASS = '1';
  process.env.APOCKY_ADMIN_EMAILS = 'owner@example.test';
  process.env.NEXT_PUBLIC_SUPABASE_URL = 'https://supabase.example.test';
  process.env.SUPABASE_SERVICE_ROLE_KEY = 'test-service-role';

  const jobs: Array<Record<string, unknown>> = [
    requestOf(LEGACY_JOB_ID, 'The live turn created before conversation IDs.', null),
    requestOf(SECOND_JOB_ID, 'What follows from that?', CONVERSATION_ID),
    requestOf(FIRST_JOB_ID, 'Remember this durable opening.', CONVERSATION_ID),
  ];
  const revisions: Array<Record<string, unknown>> = [
    {
      id: FIRST_REVISION_ID, job_id: FIRST_JOB_ID, content: 'First durable answer.',
      provenance: { tool_calls: [{ name: 'memory_search', ok: true, elapsed_ms: 12 }] }, usage: {},
      created_at: '2026-09-08T10:00:29.000Z',
    },
    {
      id: SECOND_REVISION_ID, job_id: SECOND_JOB_ID, content: 'Second durable answer.',
      provenance: {}, usage: {}, created_at: '2026-09-08T10:01:29.000Z',
    },
    {
      id: LEGACY_REVISION_ID, job_id: LEGACY_JOB_ID, content: 'The response that vanished after reload.',
      provenance: {}, usage: {}, created_at: '2026-09-08T10:02:29.000Z',
    },
    {
      id: OLD_REVISION_ID, job_id: OLD_JOB_ID, content: 'An older answer outside the bounded sidebar page.',
      provenance: {}, usage: {}, created_at: '2026-01-01T10:00:29.000Z',
    },
  ];
  const oldJob = {
    id: OLD_JOB_ID,
    status: 'succeeded',
    request: { prompt: 'Open an older conversation directly.', conversation_id: OLD_CONVERSATION_ID },
    terminal_revision_id: OLD_REVISION_ID,
    created_at: '2026-01-01T10:00:00.000Z',
    updated_at: '2026-01-01T10:00:30.000Z',
    completed_at: '2026-01-01T10:00:30.000Z',
  };
  const jobReads: URL[] = [];
  const revisionReads: URL[] = [];
  const enqueueCalls: Array<Record<string, unknown>> = [];
  const idempotentJobs = new Map<string, Record<string, unknown>>();
  let enqueued = 0;

  globalThis.fetch = async (input, init) => {
    const url = new URL(String(input));
    if (url.pathname.endsWith('/rest/v1/rpc/apocrypha_ensure_owner_principal')) {
      return jsonResponse({ tenant_id: TENANT_ID, principal_id: PRINCIPAL_ID });
    }
    if (url.pathname.endsWith('/rest/v1/rpc/apocrypha_list_owner_chat_conversations')) {
      const body = JSON.parse(String(init?.body ?? '{}')) as Record<string, unknown>;
      assert.equal(body.p_tenant_id, TENANT_ID);
      assert.equal(body.p_owner_principal_id, PRINCIPAL_ID);
      assert.equal(body.p_limit, 257, 'summary RPC includes one explicit truncation sentinel');
      return jsonResponse([
        {
          conversation_id: LEGACY_JOB_ID,
          title: 'The live turn created before conversation IDs.',
          last_active_iso: '2026-09-08T10:02:30.000Z',
          message_count: 2,
        },
        {
          conversation_id: CONVERSATION_ID,
          title: 'Remember this durable opening.',
          last_active_iso: '2026-09-08T10:01:30.000Z',
          message_count: 4,
        },
      ]);
    }
    if (url.pathname.endsWith('/rest/v1/apocrypha_job')) {
      jobReads.push(url);
      const idempotencyFilter = url.searchParams.get('idempotency_key');
      if (idempotencyFilter?.startsWith('eq.')) {
        return jsonResponse(idempotentJobs.get(idempotencyFilter.slice(3)) ?? null);
      }
      const requestFilter = url.searchParams.get('request->>conversation_id')
        ?? url.searchParams.get('request')
        ?? '';
      const idFilter = url.searchParams.get('id') ?? '';
      if (requestFilter.includes(OLD_CONVERSATION_ID)) return jsonResponse([oldJob]);
      if (requestFilter.includes(CONVERSATION_ID)) {
        return jsonResponse(jobs.filter((job) => (
          (job.request as Record<string, unknown>).conversation_id === CONVERSATION_ID
        )));
      }
      if (requestFilter) return jsonResponse([]);
      if (idFilter === `eq.${OLD_CONVERSATION_ID}`) return jsonResponse([]);
      if (idFilter.startsWith('eq.')) return jsonResponse(jobs.filter((job) => job.id === idFilter.slice(3)));
      return jsonResponse(jobs);
    }
    if (url.pathname.endsWith('/rest/v1/rpc/apocrypha_project_owner_chat_revisions')) {
      revisionReads.push(url);
      const body = JSON.parse(String(init?.body ?? '{}')) as {
        p_job_ids?: string[];
        p_revision_ids?: string[];
      };
      assert.ok((body.p_job_ids?.length ?? 0) <= 8, 'revision projection receives at most eight job ids');
      assert.equal(body.p_job_ids?.length, body.p_revision_ids?.length);
      return jsonResponse(revisions.filter((revision) => (
        body.p_job_ids?.includes(String(revision.job_id)) && body.p_revision_ids?.includes(String(revision.id))
      )));
    }
    if (url.pathname.endsWith('/rest/v1/rpc/apocrypha_enqueue_job')) {
      const body = JSON.parse(String(init?.body ?? '{}')) as Record<string, unknown>;
      enqueueCalls.push(body);
      const idempotencyKey = String(body.p_idempotency_key ?? '');
      if (idempotentJobs.has(idempotencyKey)) {
        return jsonResponse({ code: 'P0001', message: 'idempotency collision' }, 409);
      }
      enqueued += 1;
      const id = `90000000-0000-4000-8000-${String(enqueued).padStart(12, '0')}`;
      const accepted = {
        id,
        tenant_id: TENANT_ID,
        owner_principal_id: PRINCIPAL_ID,
        kind: 'apocky_chat',
        capability: 'apocky_owner_chat',
        status: 'queued',
        request: body.p_request,
        idempotency_scope: body.p_idempotency_scope,
        idempotency_key: body.p_idempotency_key,
      };
      idempotentJobs.set(idempotencyKey, accepted);
      return jsonResponse(accepted);
    }
    throw new Error(`Unexpected Supabase request: ${url.pathname}`);
  };
  resetApocryphaServiceClientForTests();

  try {
    const listed = reqRes('GET');
    await conversationsHandler(listed.req, listed.res);
    assert.equal(listed.out.statusCode, 200);
    assert.match(listed.out.headers['cache-control'] ?? '', /private/);
    assert.match(listed.out.headers['cache-control'] ?? '', /no-store/);
    const listBody = listed.out.body as { data: { conversations: Array<Record<string, unknown>> } };
    assert.deepEqual(
      listBody.data.conversations.map((conversation) => conversation.id),
      [LEGACY_JOB_ID, CONVERSATION_ID],
      'newest conversation sorts first and a recent pre-patch null-ID job remains recoverable by its job UUID',
    );
    assert.equal(listBody.data.conversations[1]?.title, 'Remember this durable opening.');
    assert.equal(listBody.data.conversations[1]?.message_count, 4);

    const oldDetail = reqRes('GET', { query: { id: OLD_CONVERSATION_ID } });
    await conversationsHandler(oldDetail.req, oldDetail.res);
    assert.equal(oldDetail.out.statusCode, 200);
    const oldDetailBody = oldDetail.out.body as { data: { messages: Array<Record<string, unknown>> } };
    assert.equal(oldDetailBody.data.messages[1]?.text, 'An older answer outside the bounded sidebar page.');
    assert.ok(
      jobReads.some((url) => (
        url.searchParams.get('request->>conversation_id')
        ?? url.searchParams.get('request')
        ?? ''
      ).includes(OLD_CONVERSATION_ID)),
      'direct detail uses a conversation-bound query instead of relying on the bounded sidebar page',
    );

    const detail = reqRes('GET', { query: { id: CONVERSATION_ID } });
    await conversationsHandler(detail.req, detail.res);
    assert.equal(detail.out.statusCode, 200);
    const detailBody = detail.out.body as { data: { messages: Array<Record<string, unknown>> } };
    assert.deepEqual(
      detailBody.data.messages.map((message) => [message.role, message.text]),
      [
        ['user', 'Remember this durable opening.'],
        ['apocrypha', 'First durable answer.'],
        ['user', 'What follows from that?'],
        ['apocrypha', 'Second durable answer.'],
      ],
      'detail projects job prompts and immutable terminal revisions in chronological turn order',
    );
    assert.deepEqual(detailBody.data.messages[1]?.tool_trace, [
      { name: 'memory_search', ok: true, elapsed_ms: 12 },
    ]);

    const legacy = reqRes('GET', { query: { id: LEGACY_JOB_ID } });
    await conversationsHandler(legacy.req, legacy.res);
    assert.equal(legacy.out.statusCode, 200);
    const legacyBody = legacy.out.body as { data: { messages: Array<Record<string, unknown>> } };
    assert.equal(legacyBody.data.messages[1]?.text, 'The response that vanished after reload.');

    const invalid = reqRes('GET', { query: { id: 'not-a-uuid' } });
    await conversationsHandler(invalid.req, invalid.res);
    assert.equal(invalid.out.statusCode, 400);
    const repeatedId = reqRes('GET', { query: { id: [CONVERSATION_ID, OLD_CONVERSATION_ID] } });
    await conversationsHandler(repeatedId.req, repeatedId.res);
    assert.equal(repeatedId.out.statusCode, 400, 'array-valued IDs fail closed');
    const repeatedScope = reqRes('GET', { query: { scope: ['active', 'active'] } });
    await conversationsHandler(repeatedScope.req, repeatedScope.res);
    assert.equal(repeatedScope.out.statusCode, 400, 'array-valued scopes fail closed');
    const unsupportedScope = reqRes('GET', { query: { scope: 'archived' } });
    await conversationsHandler(unsupportedScope.req, unsupportedScope.res);
    assert.equal(unsupportedScope.out.statusCode, 400, 'unimplemented lifecycle scopes are not presented as empty history');

    const followup = reqRes('POST', {
      body: { kind: 'apocky_chat', prompt: 'Continue durably.', conversation_id: CONVERSATION_ID },
    });
    await jobsHandler(followup.req, followup.res);
    assert.equal(followup.out.statusCode, 202);
    assert.equal((followup.out.body as Record<string, unknown>).conversation_id, CONVERSATION_ID);
    const followupCall = enqueueCalls.at(-1)!;
    assert.equal(followupCall.p_idempotency_scope, `owner-chat:${CONVERSATION_ID}`);
    assert.equal((followupCall.p_request as Record<string, unknown>).retrieval_query, 'Continue durably.');
    assert.deepEqual((followupCall.p_request as Record<string, unknown>).conversation_history, [
      { role: 'user', content: 'Remember this durable opening.' },
      { role: 'assistant', content: 'First durable answer.' },
      { role: 'user', content: 'What follows from that?' },
      { role: 'assistant', content: 'Second durable answer.' },
    ]);
    assert.deepEqual((followupCall.p_request as Record<string, unknown>).messages, [
      { role: 'user', content: 'Remember this durable opening.' },
      { role: 'assistant', content: 'First durable answer.' },
      { role: 'user', content: 'What follows from that?' },
      { role: 'assistant', content: 'Second durable answer.' },
      { role: 'user', content: 'Continue durably.' },
    ], 'the worker-facing messages array carries prior turns plus the current prompt');

    const minted = reqRes('POST', {
      body: { kind: 'apocky_chat', prompt: 'Start a durable conversation.', conversation_id: null },
    });
    await jobsHandler(minted.req, minted.res);
    assert.equal(minted.out.statusCode, 202);
    const mintedId = (minted.out.body as Record<string, unknown>).conversation_id;
    assert.equal(typeof mintedId, 'string');
    assert.match(String(mintedId), /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/);
    const mintedCall = enqueueCalls.at(-1)!;
    assert.equal((mintedCall.p_request as Record<string, unknown>).conversation_id, mintedId);
    assert.equal(mintedCall.p_idempotency_scope, `owner-chat:${String(mintedId)}`);

    const reused = reqRes('POST', {
      body: { kind: 'apocky_chat', prompt: 'Reuse that conversation.', conversation_id: String(mintedId) },
    });
    await jobsHandler(reused.req, reused.res);
    assert.equal(reused.out.statusCode, 202);
    assert.equal((reused.out.body as Record<string, unknown>).conversation_id, mintedId);
    assert.equal((enqueueCalls.at(-1)?.p_request as Record<string, unknown>).conversation_id, mintedId);

    const nullConversationRetryBody = {
      kind: 'apocky_chat',
      prompt: 'Survive a lost admission receipt without a client conversation id.',
      conversation_id: null,
      idempotency_key: NULL_CONVERSATION_RETRY_KEY,
    };
    const nullConversationFirst = reqRes('POST', { body: nullConversationRetryBody });
    await jobsHandler(nullConversationFirst.req, nullConversationFirst.res);
    assert.equal(nullConversationFirst.out.statusCode, 202);
    const nullConversationId = (nullConversationFirst.out.body as Record<string, unknown>).conversation_id;
    const nullConversationJobId = (
      (nullConversationFirst.out.body as Record<string, unknown>).job as Record<string, unknown>
    ).id;
    assert.match(String(nullConversationId), /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/);
    const enqueueCallsBeforeNullReplay = enqueueCalls.length;

    const nullConversationReplay = reqRes('POST', { body: nullConversationRetryBody });
    await jobsHandler(nullConversationReplay.req, nullConversationReplay.res);
    assert.equal(nullConversationReplay.out.statusCode, 202);
    assert.equal((nullConversationReplay.out.body as Record<string, unknown>).replayed, true);
    assert.equal((nullConversationReplay.out.body as Record<string, unknown>).conversation_id, nullConversationId);
    assert.equal(
      ((nullConversationReplay.out.body as Record<string, unknown>).job as Record<string, unknown>).id,
      nullConversationJobId,
      'a null-conversation retry resolves the original admitted job',
    );
    assert.equal(enqueueCalls.length, enqueueCallsBeforeNullReplay, 'a null-conversation retry never enqueues twice');

    const retryBody = {
      kind: 'apocky_chat',
      prompt: 'Return the already accepted durable job.',
      conversation_id: CONVERSATION_ID,
      idempotency_key: RETRY_KEY,
    };
    const firstAdmission = reqRes('POST', { body: retryBody });
    await jobsHandler(firstAdmission.req, firstAdmission.res);
    assert.equal(firstAdmission.out.statusCode, 202);
    const acceptedRetryJobId = ((firstAdmission.out.body as Record<string, unknown>).job as Record<string, unknown>).id;
    assert.equal((firstAdmission.out.body as Record<string, unknown>).replayed, false);
    const historyReadsBeforeReplay = jobReads.filter((url) => url.searchParams.has('request')).length;
    const enqueueCallsBeforeReplay = enqueueCalls.length;

    const replayedAdmission = reqRes('POST', { body: retryBody });
    await jobsHandler(replayedAdmission.req, replayedAdmission.res);
    assert.equal(replayedAdmission.out.statusCode, 202, 'a lost 202 response can replay the same admitted job');
    assert.equal((replayedAdmission.out.body as Record<string, unknown>).replayed, true);
    assert.equal(
      ((replayedAdmission.out.body as Record<string, unknown>).job as Record<string, unknown>).id,
      acceptedRetryJobId,
      'same-key retry returns the original job instead of creating or rejecting a new one',
    );
    assert.equal(
      jobReads.filter((url) => url.searchParams.has('request')).length,
      historyReadsBeforeReplay,
      'an acknowledged retry resolves from the idempotency binding without loading history',
    );
    assert.equal(enqueueCalls.length, enqueueCallsBeforeReplay, 'an acknowledged retry never enqueues twice');

    const conflictingAdmission = reqRes('POST', { body: { ...retryBody, prompt: 'Different content.' } });
    await jobsHandler(conflictingAdmission.req, conflictingAdmission.res);
    assert.equal(conflictingAdmission.out.statusCode, 409, 'same key with different content remains a conflict');

    const failedPrompt = 'Preserve this failed attempt exactly once.';
    jobs.push({
      id: FAILED_JOB_ID, status: 'failed',
      request: { prompt: failedPrompt, conversation_id: CONVERSATION_ID }, terminal_revision_id: null,
      created_at: '2026-09-08T10:03:00.000Z', updated_at: '2026-09-08T10:03:30.000Z', completed_at: '2026-09-08T10:03:30.000Z',
    });
    const retryAttempt = reqRes('POST', { body: {
      prompt: failedPrompt, conversation_id: CONVERSATION_ID, retry_job_id: FAILED_JOB_ID,
    } });
    await jobsHandler(retryAttempt.req, retryAttempt.res);
    assert.equal(retryAttempt.out.statusCode, 202);
    const retryRequest = enqueueCalls.at(-1)!.p_request as Record<string, unknown>;
    assert.equal(retryRequest.retry_of_job_id, FAILED_JOB_ID);
    assert.equal((retryRequest.messages as Array<{ content: string }>).filter((message) => message.content === failedPrompt).length, 1);

    jobs.push({
      id: RETRIED_JOB_ID, status: 'succeeded',
      request: { prompt: failedPrompt, conversation_id: CONVERSATION_ID, retry_of_job_id: FAILED_JOB_ID },
      terminal_revision_id: RETRIED_REVISION_ID,
      created_at: '2026-09-08T10:04:00.000Z', updated_at: '2026-09-08T10:04:30.000Z', completed_at: '2026-09-08T10:04:30.000Z',
    });
    revisions.push({
      id: RETRIED_REVISION_ID, job_id: RETRIED_JOB_ID, content: 'Recovered answer.', provenance: {}, usage: {},
      created_at: '2026-09-08T10:04:29.000Z',
    });
    const retriedDetail = reqRes('GET', { query: { id: CONVERSATION_ID } });
    await conversationsHandler(retriedDetail.req, retriedDetail.res);
    const retriedBody = retriedDetail.out.body as { data: { messages: Array<Record<string, unknown>> } };
    assert.equal(retriedBody.data.messages.filter((message) => message.text === failedPrompt).length, 1, 'retry never duplicates the user turn');
    assert.equal(retriedBody.data.messages.at(-1)?.text, 'Recovered answer.');

    const retrySucceeded = reqRes('POST', { body: {
      prompt: 'Remember this durable opening.', conversation_id: CONVERSATION_ID, retry_job_id: FIRST_JOB_ID,
    } });
    await jobsHandler(retrySucceeded.req, retrySucceeded.res);
    assert.equal(retrySucceeded.out.statusCode, 409, 'a succeeded attempt cannot be retried');

    jobs.splice(0, jobs.length);
    revisions.splice(0, revisions.length);
    for (let index = 1; index <= 65; index += 1) {
      const suffix = index.toString(16).padStart(12, '0');
      const jobId = `a0000000-0000-4000-8000-${suffix}`;
      const revisionId = `b0000000-0000-4000-8000-${suffix}`;
      jobs.push({
        id: jobId,
        status: 'succeeded',
        request: { prompt: `User ${suffix}: ${'u'.repeat(10_000)}`, conversation_id: CONVERSATION_ID },
        terminal_revision_id: revisionId,
        created_at: '2026-09-08T13:00:00.000Z',
        updated_at: '2026-09-08T13:01:00.000Z',
        completed_at: '2026-09-08T13:01:00.000Z',
      });
      revisions.push({
        id: revisionId,
        job_id: jobId,
        content: `Assistant ${suffix}: ${'a'.repeat(10_000)}`,
        provenance: {},
        usage: {},
        created_at: '2026-09-08T13:00:30.000Z',
      });
    }
    revisionReads.splice(0, revisionReads.length);
    const tiedDetail = reqRes('GET', { query: { id: CONVERSATION_ID } });
    await conversationsHandler(tiedDetail.req, tiedDetail.res);
    assert.equal(tiedDetail.out.statusCode, 200);
    const tiedData = (tiedDetail.out.body as {
      data: {
        messages: Array<Record<string, unknown>>;
        history_window: { truncated: boolean; row_window_truncated: boolean };
      };
    }).data;
    const tiedMessages = tiedData.messages;
    assert.equal(tiedMessages[0]?.id, 'a0000000-0000-4000-8000-000000000029:user');
    assert.equal(tiedMessages.at(-1)?.id, 'a0000000-0000-4000-8000-000000000041:apocrypha');
    assert.equal(tiedData.history_window.truncated, true, 'oversized history explicitly reports its bounded window');
    assert.equal(tiedData.history_window.row_window_truncated, true, 'the extra row sentinel reports database row clipping');
    assert.equal(
      ((tiedDetail.out.body as { data: { conversation: { message_count_is_lower_bound: boolean } } }).data)
        .conversation.message_count_is_lower_bound,
      true,
      'partial message counts are explicitly identified as lower bounds',
    );
    assert.equal(revisionReads.length, 8, '64 retained terminal revisions are read in bounded eight-row query chunks');

    const bounded = reqRes('POST', {
      body: { kind: 'apocky_chat', prompt: 'Keep the admitted history bounded.', conversation_id: CONVERSATION_ID },
    });
    await jobsHandler(bounded.req, bounded.res);
    assert.equal(bounded.out.statusCode, 202);
    const boundedRequest = enqueueCalls.at(-1)?.p_request as Record<string, unknown>;
    const boundedHistory = boundedRequest.conversation_history;
    assert.ok(Array.isArray(boundedHistory));
    assert.ok(boundedHistory.length <= 20);
    assert.equal((boundedHistory[0] as Record<string, unknown> | undefined)?.role, 'user');
    assert.ok(
      Buffer.byteLength(JSON.stringify(boundedHistory), 'utf8') <= 128 * 1024,
      'aggregate serialized UTF-8 history stays within the admission budget',
    );
    const workerMessages = boundedRequest.messages as Array<Record<string, unknown>>;
    assert.deepEqual(workerMessages.at(-1), { role: 'user', content: 'Keep the admitted history bounded.' });
    assert.ok(
      Buffer.byteLength(JSON.stringify(boundedRequest), 'utf8') < 1024 * 1024,
      'duplicated compatibility and worker history fields remain below the durable request row limit',
    );

    assert.ok(jobReads.length >= 4, 'detail, legacy, idempotency, and admission paths read the durable owner projection');
    for (const url of jobReads) {
      assert.equal(url.searchParams.get('tenant_id'), `eq.${TENANT_ID}`);
      assert.equal(url.searchParams.get('owner_principal_id'), `eq.${PRINCIPAL_ID}`);
      assert.equal(url.searchParams.get('kind'), 'eq.apocky_chat');
      assert.equal(url.searchParams.get('capability'), 'eq.apocky_owner_chat');
      assert.ok(['1', '65'].includes(url.searchParams.get('limit') ?? ''), 'owner job reads are explicitly bounded');
    }
    for (const url of jobReads.filter((candidate) => (
      !candidate.searchParams.has('idempotency_key')
      && (candidate.searchParams.get('select') ?? '').includes('request_prompt')
    ))) {
      const fields = (url.searchParams.get('select') ?? '').split(',');
      assert.ok(!fields.includes('request'), 'history projection never transfers the full request document');
      assert.ok(fields.some((field) => field.includes('request->>prompt')), 'history projection selects only the prompt scalar');
      assert.ok(
        fields.some((field) => field.includes('request->>conversation_id')),
        'history projection selects only the conversation identifier scalar',
      );
      assert.ok(fields.some((field) => field.includes('request->>retry_of_job_id')), 'history projection selects the retry link scalar');
      if ((url.searchParams.get('limit') ?? '') === '65') {
        assert.match(
          url.searchParams.get('request->>conversation_id') ?? '',
          /^eq\.[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/,
          'conversation detail uses the indexed scalar equality filter',
        );
      }
    }

    console.log('apocrypha-owner-history.test: durable owner projection, ordering, legacy recovery, and UUID mint/reuse OK');
  } finally {
    globalThis.fetch = originalFetch;
    process.env = originalEnv;
    resetApocryphaServiceClientForTests();
  }
}

void main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
