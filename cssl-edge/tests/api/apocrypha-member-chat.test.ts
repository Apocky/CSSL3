import type { NextApiRequest, NextApiResponse } from 'next';

import {
  APOCRYPHA_MEMBER_CHAT_CAPABILITY,
  canonicalMemberChatCursor,
  canonicalMemberChatMessage,
  enqueueMemberChat,
  getMemberChatJob,
  listMemberChatHistory,
  MEMBER_CHAT_ASSISTANT_MAX_BYTES,
  MEMBER_CHAT_HISTORY_CONTENT_MAX_BYTES,
  MEMBER_CHAT_HISTORY_LIMIT,
  MEMBER_CHAT_HISTORY_WIRE_MAX_BYTES,
  MEMBER_CHAT_MESSAGE_MAX_BYTES,
  MemberChatStoreError,
  type MemberChatHistoryEntry,
  type MemberChatJobReceipt,
  type MemberChatRpcClient,
} from '@/lib/apocrypha/member-chat';
import { createMemberChatHistoryHandler } from '@/pages/api/apocrypha/member/history';
import { createMemberChatJobReadHandler } from '@/pages/api/apocrypha/member/jobs/[id]';
import {
  config as memberChatSubmitConfig,
  createMemberChatSubmitHandler,
} from '@/pages/api/apocrypha/member/jobs';

interface Output {
  statusCode: number;
  body: unknown;
  headers: Record<string, string>;
}

interface RequestOptions {
  body?: unknown;
  query?: Record<string, string | string[]>;
  origin?: string | null;
  referer?: string;
  secFetchSite?: string;
  contentType?: string;
}

const CONVERSATION_ID = '11111111-1111-4111-8111-111111111111';
const FOREIGN_MEMBER_ID = 'aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa';
const REQUEST_ID = '22222222-2222-4222-8222-222222222222';
const JOB_ID = '33333333-3333-4333-8333-333333333333';
const MEMORY_HASH = 'a'.repeat(64);

function assert(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(`assert failed: ${message}`);
}

function equal(actual: unknown, expected: unknown, message: string): void {
  if (actual !== expected) {
    throw new Error(`assert failed: ${message}; expected=${String(expected)} actual=${String(actual)}`);
  }
}

function reqRes(
  method: string,
  options: RequestOptions = {},
): { req: NextApiRequest; res: NextApiResponse; out: Output } {
  const out: Output = { statusCode: 0, body: null, headers: {} };
  const headers: Record<string, string> = {
    host: 'www.apocky.com',
    'x-forwarded-proto': 'https',
    'content-type': options.contentType ?? 'application/json',
  };
  if (options.origin !== null) headers.origin = options.origin ?? 'https://www.apocky.com';
  if (options.referer) headers.referer = options.referer;
  if (options.secFetchSite) headers['sec-fetch-site'] = options.secFetchSite;
  const req = {
    method,
    body: options.body,
    query: options.query ?? {},
    headers,
  } as unknown as NextApiRequest;
  const res = {
    status(code: number) {
      out.statusCode = code;
      return this;
    },
    json(value: unknown) {
      out.body = value;
      return this;
    },
    setHeader(name: string, value: string | number | readonly string[]) {
      out.headers[name.toLowerCase()] = Array.isArray(value) ? value.join(', ') : String(value);
      return this;
    },
  } as unknown as NextApiResponse;
  return { req, res, out };
}

function member(userId: string) {
  return {
    authConfigured: true,
    user: {
      id: userId,
      email: `${userId}@example.test`,
      provider: 'test',
      createdAt: new Date(0).toISOString(),
    },
  };
}

function signedOut() {
  return {
    authConfigured: true,
    user: null,
    failureKind: 'unauthenticated' as const,
  };
}

function receipt(replayed = false): MemberChatJobReceipt {
  return {
    job_id: JOB_ID,
    conversation_id: CONVERSATION_ID,
    request_id: REQUEST_ID,
    status: 'queued',
    model_alias: 'qwen35-35b-a3b-q4',
    memory_manifest_hash: MEMORY_HASH,
    created_at: '2026-09-08T00:00:00.000Z',
    updated_at: '2026-09-08T00:00:00.000Z',
    replayed,
  };
}

function historyEntry(overrides: Partial<MemberChatHistoryEntry> = {}): MemberChatHistoryEntry {
  return {
    job_id: JOB_ID,
    conversation_id: CONVERSATION_ID,
    request_id: REQUEST_ID,
    status: 'succeeded',
    user_message: 'Hello, Apocrypha.',
    assistant_message: 'Hello. What are we building?',
    assistant_truncated: false,
    model_alias: 'qwen35-35b-a3b-q4',
    memory_manifest_hash: MEMORY_HASH,
    created_at: '2026-09-08T00:00:00.000Z',
    updated_at: '2026-09-08T00:00:01.000Z',
    completed_at: '2026-09-08T00:00:01.000Z',
    error_code: null,
    ...overrides,
  };
}

function assertPrivate(out: Output): void {
  assert(out.headers['cache-control']?.includes('private'), 'response is private');
  assert(out.headers['cache-control']?.includes('no-store'), 'response is no-store');
  equal(out.headers.vary, 'Authorization, Cookie, Origin', 'response varies by member credentials and origin');
  equal(out.headers['x-frame-options'], 'DENY', 'member response cannot be framed');
}

async function testRpcReceivesOnlyServerBindings(): Promise<void> {
  let calledFunction = '';
  let calledArgs: Record<string, unknown> = {};
  const client: MemberChatRpcClient = {
    async rpc(functionName, args) {
      calledFunction = functionName;
      calledArgs = args;
      return { data: [receipt(false)], error: null };
    },
  };
  const result = await enqueueMemberChat({
    verifiedAuthUserId: CONVERSATION_ID,
    conversationId: CONVERSATION_ID,
    requestId: REQUEST_ID,
    message: 'Hello, Apocrypha.',
  }, client);
  equal(calledFunction, 'apocrypha_enqueue_member_chat_v2', 'canonical member enqueue RPC is used');
  equal(calledArgs.p_verified_auth_user_id, CONVERSATION_ID, 'verified user binds the RPC');
  equal(calledArgs.p_presented_conversation_id, CONVERSATION_ID, 'the browser conversation id reaches the RPC, which owns the ownership check');
  equal(calledArgs.p_request_id, REQUEST_ID, 'opaque replay id crosses the boundary');
  equal(calledArgs.p_message, 'Hello, Apocrypha.', 'canonical message crosses the boundary');
  assert(!('p_tenant_id' in calledArgs), 'caller cannot supply a tenant');
  assert(!('p_principal_id' in calledArgs), 'caller cannot supply a principal');
  assert(!('p_history' in calledArgs), 'caller cannot supply conversation history');
  assert(typeof calledArgs.p_model_alias === 'string', 'model alias is server-selected');
  assert(typeof calledArgs.p_memory_manifest_hash === 'string', 'memory manifest is server-selected');
  equal(result.job_id, JOB_ID, 'validated receipt returned');

  // ── ownership moved from equality to lookup (migration 0057) ──────────
  //
  // This used to assert that a conversation id differing from the auth user id
  // was refused HERE, before storage. That was the whole implementation of
  // "one conversation per member", so it had to change when members got more
  // than one - but it is being replaced, not deleted, and the replacement has
  // to be at least as strong or the change is not worth making.
  //
  // What must still hold, and is asserted below:
  //   1. a malformed conversation id is STILL refused before storage;
  //   2. a well-formed foreign id is forwarded, and the two identities reach
  //      the RPC as SEPARATE arguments - which is the only reason the database
  //      can tell they differ and refuse;
  //   3. the database's refusal (P4031) surfaces to the caller as a refusal
  //      rather than as a success or a crash.
  //
  // 1. Shape is still checked in-process. A junk id never costs a round trip.
  let malformedRpcCalls = 0;
  const malformedClient: MemberChatRpcClient = {
    async rpc() {
      malformedRpcCalls += 1;
      return { data: null, error: null };
    },
  };
  let malformedRejected = false;
  try {
    await enqueueMemberChat({
      verifiedAuthUserId: CONVERSATION_ID,
      conversationId: 'not-a-uuid',
      requestId: REQUEST_ID,
      message: 'Hello, Apocrypha.',
    }, malformedClient);
  } catch (error) {
    malformedRejected = error instanceof MemberChatStoreError
      && error.publicStatus === 403
      && error.publicCode === 'MEMBER_CHAT_CONVERSATION_MISMATCH';
  }
  assert(malformedRejected, 'a malformed conversation id is refused');
  equal(malformedRpcCalls, 0, 'a malformed conversation id is refused before storage');

  // 2. A foreign but well-formed id is forwarded WITH the verified identity
  //    kept separate. If these two were ever collapsed back into one argument
  //    the database would be comparing a value against itself and could never
  //    refuse anything - so this assertion is the one holding the boundary.
  let foreignArgs: Record<string, unknown> = {};
  const foreignClient: MemberChatRpcClient = {
    async rpc(_fn: string, args: Record<string, unknown>) {
      foreignArgs = args;
      return {
        data: null,
        error: { code: 'P4031', message: 'member conversation is not owned by the verified identity' },
      };
    },
  };
  let foreignRejected = false;
  try {
    await enqueueMemberChat({
      verifiedAuthUserId: FOREIGN_MEMBER_ID,
      conversationId: CONVERSATION_ID,
      requestId: REQUEST_ID,
      message: 'Hello, Apocrypha.',
    }, foreignClient);
  } catch (error) {
    foreignRejected = error instanceof MemberChatStoreError;
  }
  equal(
    foreignArgs.p_verified_auth_user_id,
    FOREIGN_MEMBER_ID,
    'the verified identity reaches the RPC as itself',
  );
  equal(
    foreignArgs.p_presented_conversation_id,
    CONVERSATION_ID,
    'the presented conversation reaches the RPC as a separate argument',
  );
  assert(
    foreignArgs.p_verified_auth_user_id !== foreignArgs.p_presented_conversation_id,
    'identity and conversation are not collapsed into one value',
  );
  // 3. And the database's refusal is surfaced, not swallowed.
  assert(foreignRejected, 'a P4031 from the ownership check surfaces as a refusal');

  const oversizedProjection: MemberChatRpcClient = {
    async rpc() {
      return {
        data: [historyEntry({ assistant_message: 'x'.repeat(MEMBER_CHAT_ASSISTANT_MAX_BYTES + 1) })],
        error: null,
      };
    },
  };
  let projectionRejected = false;
  try {
    await getMemberChatJob({
      verifiedAuthUserId: CONVERSATION_ID,
      jobId: JOB_ID,
    }, oversizedProjection);
  } catch (error) {
    projectionRejected = error instanceof Error && error.message.includes('assistant_message');
  }
  assert(projectionRejected, 'oversized assistant projections fail closed');

  assert(
    MEMBER_CHAT_HISTORY_LIMIT * (MEMBER_CHAT_MESSAGE_MAX_BYTES + MEMBER_CHAT_ASSISTANT_MAX_BYTES)
      <= MEMBER_CHAT_HISTORY_CONTENT_MAX_BYTES,
    'all individually valid history rows fit inside the aggregate response bound',
  );

  const activeTurnClient: MemberChatRpcClient = {
    async rpc() {
      return {
        data: null,
        error: { code: 'P4091', message: 'member chat principal already has active work' },
      };
    },
  };
  let activeTurnRejected = false;
  try {
    await enqueueMemberChat({
      verifiedAuthUserId: CONVERSATION_ID,
      conversationId: CONVERSATION_ID,
      requestId: REQUEST_ID,
      message: 'Hello, Apocrypha.',
    }, activeTurnClient);
  } catch (error) {
    activeTurnRejected = error instanceof Error
      && error.message.includes('turn in progress')
      && (error as { publicStatus?: unknown }).publicStatus === 409;
  }
  assert(activeTurnRejected, 'a second active turn returns a retryable conflict');

  const quotaClient: MemberChatRpcClient = {
    async rpc() {
      return {
        data: null,
        error: { code: 'P4290', message: 'member chat rolling quota exceeded' },
      };
    },
  };
  let quotaRejected = false;
  try {
    await enqueueMemberChat({
      verifiedAuthUserId: CONVERSATION_ID,
      conversationId: CONVERSATION_ID,
      requestId: REQUEST_ID,
      message: 'Hello, Apocrypha.',
    }, quotaClient);
  } catch (error) {
    quotaRejected = error instanceof MemberChatStoreError
      && error.publicStatus === 429
      && error.publicCode === 'MEMBER_CHAT_QUOTA_EXCEEDED'
      && error.retryAfterSeconds === 3600;
  }
  assert(quotaRejected, 'the durable database quota has a stable public mapping');

  equal(canonicalMemberChatMessage('safe\tline\nnext'), 'safe\tline\nnext', 'tab and newline remain valid');
  equal(canonicalMemberChatMessage('unsafe\u0000message'), null, 'NUL is rejected');
  equal(canonicalMemberChatMessage('unsafe\u0001message'), null, 'disallowed C0 controls are rejected');
  equal(canonicalMemberChatMessage('unsafe\u007fmessage'), null, 'DEL is rejected');
  equal(canonicalMemberChatCursor('9223372036854775807'), '9223372036854775807', 'largest bigint cursor is valid');
  equal(canonicalMemberChatCursor('9223372036854775808'), null, 'overflowing cursor is rejected');
}

async function testHistoryPaginationAndWireBound(): Promise<void> {
  let calledFunction = '';
  let calledArgs: Record<string, unknown> = {};
  const pagedClient: MemberChatRpcClient = {
    async rpc(functionName, args) {
      calledFunction = functionName;
      calledArgs = args;
      return {
        data: [{ ...historyEntry(), turn_cursor: '49', has_more: true }],
        error: null,
      };
    },
  };
  const page = await listMemberChatHistory({
    verifiedAuthUserId: CONVERSATION_ID,
    conversationId: CONVERSATION_ID,
    beforeCursor: '50',
  }, pagedClient);
  equal(calledFunction, 'apocrypha_list_member_chat_history_v2', 'canonical history RPC is used');
  equal(calledArgs.p_verified_auth_user_id, CONVERSATION_ID, 'history is auth bound');
  equal(calledArgs.p_presented_conversation_id, CONVERSATION_ID, 'presented id is validation only');
  equal(calledArgs.p_before_turn_sequence, '50', 'exclusive cursor crosses as lossless text');
  equal(calledArgs.p_limit, MEMBER_CHAT_HISTORY_LIMIT, 'database page size is server bounded');
  equal(page.nextCursor, '49', 'database continuation cursor is projected');

  const largeRows = Array.from({ length: MEMBER_CHAT_HISTORY_LIMIT }, (_, index) => ({
    ...historyEntry({ assistant_message: '\u0001'.repeat(16_000) }),
    turn_cursor: String(index + 1),
    has_more: false,
  }));
  const largeClient: MemberChatRpcClient = {
    async rpc() { return { data: largeRows, error: null }; },
  };
  const bounded = await listMemberChatHistory({
    verifiedAuthUserId: CONVERSATION_ID,
    conversationId: CONVERSATION_ID,
  }, largeClient);
  assert(bounded.history.length > 0, 'a useful newest history suffix remains');
  assert(bounded.history.length < MEMBER_CHAT_HISTORY_LIMIT, 'wire-heavy oldest rows are paginated');
  assert(bounded.nextCursor !== null, 'wire pruning returns a continuation cursor');
  const wireBody = {
    ok: true,
    conversation_id: CONVERSATION_ID,
    history: bounded.history,
    count: bounded.history.length,
    next_cursor: bounded.nextCursor,
  };
  assert(
    Buffer.byteLength(JSON.stringify(wireBody), 'utf8') <= MEMBER_CHAT_HISTORY_WIRE_MAX_BYTES,
    'serialized history stays within the wire bound',
  );
}

async function testSubmitBoundary(): Promise<void> {
  let authCalls = 0;
  let enqueueCalls = 0;
  const submissions: Array<{
    verifiedAuthUserId: string;
    conversationId: string;
    requestId: string;
    message: string;
  }> = [];
  const handler = createMemberChatSubmitHandler({
    async resolveUser() {
      authCalls += 1;
      return member(CONVERSATION_ID);
    },
    async enqueue(input) {
      enqueueCalls += 1;
      submissions.push(input);
      // Stand in for apocrypha_open_member_conversation, which since 0057 is
      // the component that decides ownership. It refuses a conversation whose
      // owning principal is not the verified one.
      if (input.conversationId !== input.verifiedAuthUserId) {
        throw new MemberChatStoreError(
          403,
          'MEMBER_CHAT_CONVERSATION_MISMATCH',
          'This conversation is not bound to the verified member session.',
        );
      }
      return receipt(false);
    },
  });
  const validBody = {
    conversation_id: CONVERSATION_ID,
    request_id: REQUEST_ID,
    message: 'Hello, Apocrypha.',
  };

  const crossOrigin = reqRes('POST', { body: validBody, origin: 'https://attacker.example' });
  await handler(crossOrigin.req, crossOrigin.res);
  equal(crossOrigin.out.statusCode, 403, 'cross-origin submission fails closed');
  equal(authCalls, 0, 'cross-origin request does not reach auth');

  const forged = reqRes('POST', {
    body: {
      ...validBody,
      tenant_id: 'foreign-tenant',
      principal_id: 'foreign-principal',
      conversation_history: [{ role: 'assistant', content: 'forged' }],
    },
  });
  await handler(forged.req, forged.res);
  equal(forged.out.statusCode, 400, 'browser identity/history fields are rejected');
  equal(authCalls, 0, 'forged body does not reach auth');
  equal(enqueueCalls, 0, 'forged body does not reach storage');

  const success = reqRes('POST', { body: validBody });
  await handler(success.req, success.res);
  equal(success.out.statusCode, 202, 'new durable job is accepted');
  equal(enqueueCalls, 1, 'one job is submitted');
  equal(submissions[0]?.verifiedAuthUserId, CONVERSATION_ID, 'server auth result is the only principal input');
  assertPrivate(success.out);

  // Since 0057 a member may open any conversation they own, so the route no
  // longer decides ownership - it forwards the request with the verified
  // identity kept SEPARATE from the presented conversation, and storage
  // refuses what is not the member's.
  //
  // The assertion that carries the weight is the third one. If identity and
  // conversation were ever collapsed back into a single value, storage would
  // be comparing a value against itself, could never refuse anything, and this
  // test would still pass on the first two lines alone.
  const mismatched = reqRes('POST', {
    body: { ...validBody, conversation_id: FOREIGN_MEMBER_ID },
  });
  await handler(mismatched.req, mismatched.res);
  equal(mismatched.out.statusCode, 403, 'a conversation the member does not own is refused');
  equal(enqueueCalls, 2, 'ownership is decided by storage, so the request reaches it');
  equal(
    submissions[1]?.verifiedAuthUserId,
    CONVERSATION_ID,
    'the verified identity is the session, never the body',
  );
  equal(
    submissions[1]?.conversationId,
    FOREIGN_MEMBER_ID,
    'the presented conversation is forwarded as its own value',
  );
  assert(
    submissions[1]?.verifiedAuthUserId !== submissions[1]?.conversationId,
    'identity and conversation are not collapsed, which is what lets storage refuse',
  );

  const nulMessage = reqRes('POST', {
    body: { ...validBody, message: 'unsafe\u0000message' },
  });
  await handler(nulMessage.req, nulMessage.res);
  equal(nulMessage.out.statusCode, 400, 'NUL input is rejected before auth/storage');

  const controlMessage = reqRes('POST', {
    body: { ...validBody, message: 'unsafe\u0001message' },
  });
  await handler(controlMessage.req, controlMessage.res);
  equal(controlMessage.out.statusCode, 400, 'disallowed control input is rejected before auth/storage');

  equal(memberChatSubmitConfig.api.bodyParser.sizeLimit, '128kb', 'parser admits worst-case valid JSON encoding');
  const worstCaseValidBody = JSON.stringify({
    ...validBody,
    message: '"'.repeat(MEMBER_CHAT_MESSAGE_MAX_BYTES),
  });
  assert(
    Buffer.byteLength(worstCaseValidBody, 'utf8') < 128 * 1024,
    'maximum valid escaped message fits inside the configured parser bound',
  );

  const replayHandler = createMemberChatSubmitHandler({
    async resolveUser() { return member(CONVERSATION_ID); },
    async enqueue() { return receipt(true); },
  });
  const replay = reqRes('POST', { body: validBody });
  await replayHandler(replay.req, replay.res);
  equal(replay.out.statusCode, 200, 'lost-response replay returns the existing job');
  equal((replay.out.body as Record<string, unknown>).replayed, true, 'replay receipt is explicit');

  const unauthenticatedHandler = createMemberChatSubmitHandler({
    async resolveUser() { return signedOut(); },
    async enqueue() { throw new Error('must not run'); },
  });
  const unauthenticated = reqRes('POST', { body: validBody });
  await unauthenticatedHandler(unauthenticated.req, unauthenticated.res);
  equal(unauthenticated.out.statusCode, 401, 'signed-out submission fails closed');

  const quotaHandler = createMemberChatSubmitHandler({
    async resolveUser() { return member(CONVERSATION_ID); },
    async enqueue() {
      throw new MemberChatStoreError(
        429,
        'MEMBER_CHAT_QUOTA_EXCEEDED',
        'This member has reached the rolling chat limit. Retry later.',
        3600,
      );
    },
  });
  const quota = reqRes('POST', { body: validBody });
  await quotaHandler(quota.req, quota.res);
  equal(quota.out.statusCode, 429, 'rolling quota is an HTTP 429');
  equal(quota.out.headers['retry-after'], '3600', 'quota response carries a retry bound');
}

async function testStrictForeignJobIsolation(): Promise<void> {
  let observedUser = '';
  let lookupCalls = 0;
  const handler = createMemberChatJobReadHandler({
    async resolveUser() { return member(FOREIGN_MEMBER_ID); },
    async getJob(input) {
      lookupCalls += 1;
      observedUser = input.verifiedAuthUserId;
      return null;
    },
  });

  const foreign = reqRes('GET', { query: { id: JOB_ID } });
  await handler(foreign.req, foreign.res);
  equal(foreign.out.statusCode, 404, 'foreign job is indistinguishable from an absent job');
  equal(observedUser, FOREIGN_MEMBER_ID, 'lookup scope comes from verified auth');
  equal(lookupCalls, 1, 'one scoped lookup occurs');

  const forgedScope = reqRes('GET', {
    query: { id: JOB_ID, principal_id: CONVERSATION_ID },
  });
  await handler(forgedScope.req, forgedScope.res);
  equal(forgedScope.out.statusCode, 400, 'job query cannot override principal scope');
  equal(lookupCalls, 1, 'forged query never reaches storage');

  const crossOrigin = reqRes('GET', { query: { id: JOB_ID }, origin: 'https://attacker.example' });
  await handler(crossOrigin.req, crossOrigin.res);
  equal(crossOrigin.out.statusCode, 403, 'cross-origin job read fails closed');
  equal(lookupCalls, 1, 'cross-origin read never reaches storage');

  const escapedProjectionHandler = createMemberChatJobReadHandler({
    async resolveUser() { return member(CONVERSATION_ID); },
    async getJob() {
      return historyEntry({ conversation_id: FOREIGN_MEMBER_ID });
    },
  });
  const escapedProjection = reqRes('GET', { query: { id: JOB_ID } });
  await escapedProjectionHandler(escapedProjection.req, escapedProjection.res);
  equal(escapedProjection.out.statusCode, 502, 'job projection cannot escape the verified conversation');
}

async function testDurableHistoryBoundary(): Promise<void> {
  let observedUser = '';
  let observedConversation = '';
  let observedCursor: string | null | undefined;
  let listCalls = 0;
  const handler = createMemberChatHistoryHandler({
    async resolveUser() { return member(CONVERSATION_ID); },
    async listHistory(input) {
      listCalls += 1;
      observedUser = input.verifiedAuthUserId;
      observedConversation = input.conversationId;
      observedCursor = input.beforeCursor;
      // Stands in for the ownership check, which since 0057 lives in the RPC.
      if (input.conversationId !== input.verifiedAuthUserId) {
        throw new MemberChatStoreError(
          403,
          'MEMBER_CHAT_CONVERSATION_MISMATCH',
          'This conversation is not bound to the verified member session.',
        );
      }
      return { history: [historyEntry()], nextCursor: null };
    },
  });

  const success = reqRes('GET', { query: { conversation_id: CONVERSATION_ID } });
  await handler(success.req, success.res);
  equal(success.out.statusCode, 200, 'durable history is returned');
  equal(observedUser, CONVERSATION_ID, 'history principal comes from verified auth');
  equal(observedConversation, CONVERSATION_ID, 'history is scoped to the canonical conversation');
  equal((success.out.body as Record<string, unknown>).count, 1, 'history count is bounded and explicit');
  equal((success.out.body as Record<string, unknown>).next_cursor, null, 'history continuation is explicit');
  assertPrivate(success.out);

  const cursorPage = reqRes('GET', {
    query: { conversation_id: CONVERSATION_ID, before: '50' },
  });
  await handler(cursorPage.req, cursorPage.res);
  equal(cursorPage.out.statusCode, 200, 'valid cursor page is accepted');
  equal(observedCursor, '50', 'cursor is passed as lossless decimal text');

  const badCursor = reqRes('GET', {
    query: { conversation_id: CONVERSATION_ID, before: '0' },
  });
  await handler(badCursor.req, badCursor.res);
  equal(badCursor.out.statusCode, 400, 'invalid cursor is rejected before auth/storage');

  const forged = reqRes('GET', {
    query: { conversation_id: CONVERSATION_ID, tenant_id: 'foreign-tenant' },
  });
  await handler(forged.req, forged.res);
  equal(forged.out.statusCode, 400, 'history query cannot override tenant scope');
  equal(listCalls, 2, 'forged history query never reaches storage');

  // As with submit: reading a conversation the member does not own is refused,
  // but the refusal is now storage's, so the request reaches it with identity
  // and conversation kept apart.
  const mismatchedConversation = reqRes('GET', {
    query: { conversation_id: FOREIGN_MEMBER_ID },
  });
  await handler(mismatchedConversation.req, mismatchedConversation.res);
  equal(mismatchedConversation.out.statusCode, 403, 'history of a conversation the member does not own is refused');
  equal(listCalls, 3, 'ownership is decided by storage, so the query reaches it');
  equal(observedUser, CONVERSATION_ID, 'the verified identity is the session, never the query string');
  equal(observedConversation, FOREIGN_MEMBER_ID, 'the requested conversation is forwarded as its own value');
  assert(observedUser !== observedConversation, 'identity and conversation are not collapsed');

  const sameOriginReferer = reqRes('GET', {
    query: { conversation_id: CONVERSATION_ID },
    origin: null,
    referer: 'https://www.apocky.com/account',
  });
  await handler(sameOriginReferer.req, sameOriginReferer.res);
  equal(sameOriginReferer.out.statusCode, 200, 'same-origin browser read may use a verified Referer');

  const contradictoryReferer = reqRes('GET', {
    query: { conversation_id: CONVERSATION_ID },
    origin: null,
    referer: 'https://attacker.example/history',
    secFetchSite: 'same-origin',
  });
  await handler(contradictoryReferer.req, contradictoryReferer.res);
  equal(contradictoryReferer.out.statusCode, 403, 'foreign Referer cannot be rescued by a forged same-origin fetch signal');

  const contradictoryFetchSite = reqRes('GET', {
    query: { conversation_id: CONVERSATION_ID },
    referer: 'https://www.apocky.com/account',
    secFetchSite: 'cross-site',
  });
  await handler(contradictoryFetchSite.req, contradictoryFetchSite.res);
  equal(contradictoryFetchSite.out.statusCode, 403, 'cross-site fetch signal overrides otherwise same-origin headers');

  const unlabeled = reqRes('GET', {
    query: { conversation_id: CONVERSATION_ID },
    origin: null,
  });
  await handler(unlabeled.req, unlabeled.res);
  equal(unlabeled.out.statusCode, 403, 'unlabelled read context fails closed');
}

async function main(): Promise<void> {
  equal(APOCRYPHA_MEMBER_CHAT_CAPABILITY, 'apocky_member_chat', 'member capability is exact');
  await testRpcReceivesOnlyServerBindings();
  await testHistoryPaginationAndWireBound();
  await testSubmitBoundary();
  await testStrictForeignJobIsolation();
  await testDurableHistoryBoundary();
  console.log('apocrypha-member-chat.test: 5/5 passed');
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});
