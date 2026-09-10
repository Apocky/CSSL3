import { randomUUID } from 'node:crypto';

import type { NextApiRequest, NextApiResponse } from 'next';

import { publicMemberPrincipalRef } from '@/lib/apocv4/runtime-proxy';
import sessionsHandler from '@/pages/api/apocrypha/sessions';

interface Output {
  statusCode: number;
  body: unknown;
  headers: Record<string, string>;
}

interface RequestOptions {
  body?: unknown;
  contentType?: string;
  member?: boolean;
  origin?: string;
  query?: Record<string, string | string[]>;
}

function assert(condition: boolean, message: string): asserts condition {
  if (!condition) throw new Error(`assert failed: ${message}`);
}

function equal(actual: unknown, expected: unknown, message: string): void {
  if (actual !== expected) {
    throw new Error(`assert failed: ${message}; expected=${String(expected)} actual=${String(actual)}`);
  }
}

function reqRes(method: string, options: RequestOptions = {}): {
  req: NextApiRequest;
  res: NextApiResponse;
  out: Output;
} {
  const out: Output = { statusCode: 0, body: null, headers: {} };
  const headers: Record<string, string> = {
    host: 'www.apocky.com',
    origin: options.origin ?? 'https://www.apocky.com',
    'x-forwarded-proto': 'https',
    'content-type': options.contentType ?? 'application/json',
  };
  if (options.member !== false) headers['x-apocky-test-admin-email'] = 'member@example.test';
  const req = {
    method,
    body: options.body,
    query: options.query ?? {},
    headers,
  } as unknown as NextApiRequest;
  const res = {
    status(code: number) { out.statusCode = code; return this; },
    json(value: unknown) { out.body = value; return this; },
    setHeader(name: string, value: string | number | readonly string[]) {
      out.headers[name.toLowerCase()] = Array.isArray(value) ? value.join(', ') : String(value);
      return this;
    },
  } as unknown as NextApiResponse;
  return { req, res, out };
}

function runtimeEnvelope(result: Record<string, unknown>): Record<string, unknown> {
  return { schema_version: 'apocv4.runtime-service.v1', result };
}

const DIGEST = 'a'.repeat(64);
const TIMESTAMP = '2026-08-03T00:00:00+00:00';

function summary(sessionId: string, legacy = false): Record<string, unknown> {
  return {
    [legacy ? 'conversation_id' : 'session_id']: sessionId,
    title: 'Persistent thread',
    created_at: TIMESTAMP,
    updated_at: TIMESTAMP,
    message_count: 2,
    active_job_count: 0,
    artifact_count: 0,
    tip_digest: DIGEST,
  };
}

function message(index: number): Record<string, unknown> {
  const assistant = index % 2 === 1;
  const content = assistant ? `Reply ${index}` : `Question ${index}`;
  return {
    role: assistant ? 'assistant' : 'user',
    content,
    request_id: randomUUID(),
    recorded_at: TIMESTAMP,
    event_digest: index.toString(16).padStart(64, '0'),
    ...(assistant ? {
      result: {
        schema_version: 'apocv4.chat-response.v2',
        text: content,
        model_reported: {
          model_id: 'fixture/model',
          response_id: `response-${index}`,
          response_digest: 'b'.repeat(64),
          serving_profile_digest: 'c'.repeat(64),
        },
        authority: {
          effect_authority: 'NONE',
          tool_authority: 'READ_ONLY_CONTEXT',
          memory_scope: 'public_safe_retrieval',
          conversation_history: 'durable_principal_bound',
          training_consent: false,
        },
        identity: {
          schema_version: 'apocv4.identity.v1',
          system_id: 'apocrypha',
          architecture: 'governed_hybrid_digital_intelligence',
          compiler_version: 'fixture',
          identity_digest: 'd'.repeat(64),
          learned_model_role: 'replaceable_faculty_not_system_identity',
          lineage: 'fixture-lineage',
        },
        context: {
          frame_id: `acf-${index}`,
          frame_digest: 'e'.repeat(64),
          provenance_spine_digest: 'f'.repeat(64),
          retrieval: { status: 'ready', count: 0, refs: [] },
          memory: {
            provider: 'fixture', status: 'ready', records_used: 0,
            receipt_digest: null, refs: [],
          },
          capabilities: [],
        },
      },
    } : {}),
  };
}

function snapshot(
  sessionId: string,
  options: { legacy?: boolean; messageCount?: number; foreignId?: string; dualId?: string } = {},
): Record<string, unknown> {
  const messageCount = options.messageCount ?? 2;
  const identity = options.foreignId ?? sessionId;
  return {
    [options.legacy ? 'conversation_id' : 'session_id']: identity,
    ...(options.dualId === undefined ? {} : { conversation_id: options.dualId }),
    schema_version: 'apocv4.workspace-session-snapshot.v1',
    title: options.foreignId ? 'B-private-content-must-not-escape' : 'Persistent thread',
    created_at: TIMESTAMP,
    updated_at: TIMESTAMP,
    event_count: messageCount,
    events_truncated: false,
    tip_digest: DIGEST,
    messages: Array.from({ length: messageCount }, (_, index) => message(index)),
    turn_states: [], jobs: [], artifacts: [], code_requests: [], proposals: [], effects: [],
    surface_truncation: {
      messages: { total: messageCount, visible: messageCount, truncated: false },
      turn_states: { total: 0, visible: 0, truncated: false },
      jobs: { total: 0, visible: 0, truncated: false },
      artifacts: { total: 0, visible: 0, truncated: false },
      code_requests: { total: 0, visible: 0, truncated: false },
      proposals: { total: 0, visible: 0, truncated: false },
      effects: { total: 0, visible: 0, truncated: false },
    },
    world: {
      message_count: messageCount, pending_turn_count: 0, failed_turn_count: 0,
      active_job_count: 0, artifact_count: 0, code_request_count: 0,
      proposal_count: 0, effect_count: 0, last_event_type: 'CHAT_ASSISTANT',
      last_event_digest: (messageCount - 1).toString(16).padStart(64, '0'),
    },
    workspace: { status: 'not_authorized', effect_authority: 'NONE' },
  };
}

async function main(): Promise<void> {
  const originalEnv = { ...process.env };
  const originalFetch = globalThis.fetch;
  try {
    process.env.LAZARUS_TEST_AUTH_BYPASS = '1';
    Object.assign(process.env, { NODE_ENV: 'test' });
    process.env.APOCV4_RUNTIME_URL = 'https://203.0.113.10:9443';
    process.env.APOCV4_RUNTIME_DIRECT_IP = '203.0.113.10';
    process.env.APOCV4_RUNTIME_DIRECT_PORT = '9443';
    process.env.APOCV4_RUNTIME_TRANSPORT = 'test-fetch';
    process.env.APOCV4_API_TOKEN = 'owner-token';
    process.env.APOCV4_PUBLIC_API_TOKEN = 'public-token';
    process.env.APOCV4_SESSION_BINDING_SECRET = 's'.repeat(64);

    const sessionId = randomUUID();
    let getMode: 'normal' | 'bounded' | 'foreign' | 'dual' = 'normal';
    let listIncludesGlobalTip = false;
    let bindingVerified = true;
    let unsafeReceiptAuthority = false;
    const calls: Array<{ url: string; body: Record<string, unknown>; headers: Headers }> = [];
    globalThis.fetch = async (input, init) => {
      const url = String(input);
      const body = JSON.parse(String(init?.body ?? '{}')) as Record<string, unknown>;
      calls.push({ url, body, headers: new Headers(init?.headers) });
      if (url.endsWith('/v1/sessions/list')) {
        return new Response(JSON.stringify(runtimeEnvelope({
          schema_version: 'apocv4.workspace-sessions.v1',
          sessions: [summary(sessionId, true)],
          count: 1,
          ...(listIncludesGlobalTip ? { ledger_tip_digest: DIGEST } : {}),
        })), {
          status: 200,
          headers: {
            'Content-Type': 'application/json',
            ...(bindingVerified ? { 'X-Apocv4-Session-Binding': 'VERIFIED' } : {}),
          },
        });
      }
      if (url.endsWith('/v1/sessions/get')) {
        const requestedId = String(body.session_id ?? body.conversation_id);
        const result = getMode === 'bounded'
          ? snapshot(requestedId, { messageCount: 130 })
          : getMode === 'foreign'
            ? snapshot(requestedId, { foreignId: randomUUID() })
            : getMode === 'dual'
              ? snapshot(requestedId, { dualId: randomUUID() })
              : snapshot(requestedId);
        if (unsafeReceiptAuthority) {
          const assistant = (result.messages as Record<string, unknown>[])[1];
          const turn = assistant?.result as Record<string, unknown> | undefined;
          const authority = turn?.authority as Record<string, unknown> | undefined;
          if (authority) authority.training_consent = true;
        }
        return new Response(JSON.stringify(runtimeEnvelope(result)), {
          status: 200,
          headers: {
            'Content-Type': 'application/json',
            ...(bindingVerified ? { 'X-Apocv4-Session-Binding': 'VERIFIED' } : {}),
          },
        });
      }
      return new Response(JSON.stringify(runtimeEnvelope({
        schema_version: 'apocv4.workspace-session-deletion.v1',
        session_id: body.session_id,
        deleted: true,
        event_digest: 'e'.repeat(64),
      })), {
        status: 200,
        headers: {
          'Content-Type': 'application/json',
          ...(bindingVerified ? { 'X-Apocv4-Session-Binding': 'VERIFIED' } : {}),
        },
      });
    };

    const principalA = publicMemberPrincipalRef('member-a');
    const principalB = publicMemberPrincipalRef('member-b');
    assert(principalA !== principalB, 'server-derived principals isolate authenticated members');

    const wrongMethod = reqRes('POST');
    await sessionsHandler(wrongMethod.req, wrongMethod.res);
    equal(wrongMethod.out.statusCode, 405, 'only GET and DELETE are admitted');

    const unauthenticated = reqRes('GET', { member: false });
    await sessionsHandler(unauthenticated.req, unauthenticated.res);
    equal(unauthenticated.out.statusCode, 401, 'session reads require authentication');

    const invalidQuery = reqRes('GET', { query: { conversation_id: sessionId } });
    await sessionsHandler(invalidQuery.req, invalidQuery.res);
    equal(invalidQuery.out.statusCode, 400, 'legacy public query naming fails closed');

    const forgedQuery = reqRes('GET', {
      query: { session_id: sessionId, session_principal: principalB },
    });
    await sessionsHandler(forgedQuery.req, forgedQuery.res);
    equal(forgedQuery.out.statusCode, 400, 'client-asserted principal query fails closed');

    const list = reqRes('GET');
    await sessionsHandler(list.req, list.res);
    equal(list.out.statusCode, 200, 'principal-bound session list succeeds');
    const listBody = list.out.body as Record<string, unknown>;
    equal(listBody.count, 1, 'validated list count is returned');
    assert(!('ledger_tip_digest' in listBody), 'cross-tenant global ledger tip is not exposed');
    equal(
      ((listBody.sessions as Record<string, unknown>[])[0] ?? {}).session_id,
      sessionId,
      'legacy upstream summary is normalized to canonical session_id',
    );
    assert(list.out.headers['cache-control']?.includes('no-store') === true, 'list is never cached');
    const listCall = calls.find((call) => call.url.endsWith('/v1/sessions/list'));
    assert(listCall !== undefined, 'list runtime call was observed');
    equal(listCall.headers.get('authorization'), 'Bearer public-token', 'public credential stays server-side');
    equal(listCall.body.privacy_partition, 'public:apocrypha', 'public partition is server selected');
    equal(listCall.body.limit, 24, 'list bound is server selected');
    equal(
      listCall.body.session_principal,
      publicMemberPrincipalRef('test-admin'),
      'list principal is derived from the authenticated member',
    );
    assert(
      typeof listCall.body.session_binding_mac === 'string'
        && /^[0-9a-f]{64}$/.test(listCall.body.session_binding_mac),
      'runtime receives an exact-request member binding independent of the shared bearer',
    );
    equal(
      listCall.body.session_binding_mac,
      '7cf8ca85f447f15606641dbc1f20da1a2840ddbec0989bdf87909337453622c6',
      'TypeScript signing matches the runtime canonical-JSON HMAC vector',
    );

    listIncludesGlobalTip = true;
    const unsafeList = reqRes('GET');
    await sessionsHandler(unsafeList.req, unsafeList.res);
    equal(unsafeList.out.statusCode, 502, 'a global ledger tip is rejected rather than reprojected');
    assert(!JSON.stringify(unsafeList.out.body).includes(DIGEST), 'global ledger tip does not escape');
    listIncludesGlobalTip = false;

    const get = reqRes('GET', { query: { session_id: sessionId } });
    await sessionsHandler(get.req, get.res);
    equal(get.out.statusCode, 200, 'one owned session can be fetched');
    const getSession = (get.out.body as Record<string, unknown>).session as Record<string, unknown>;
    equal(getSession.session_id, sessionId, 'session identity survives strict validation');
    assert(!('conversation_id' in getSession), 'legacy upstream identity never reaches the public response');
    assert(
      !('result' in ((getSession.messages as Record<string, unknown>[])[1] ?? {})),
      'duplicated internal assistant result is removed from the public snapshot',
    );
    assert(
      typeof ((getSession.messages as Record<string, unknown>[])[1] ?? {}).receipt === 'object',
      'a validated assistant receipt survives restoration',
    );

    unsafeReceiptAuthority = true;
    const unsafeReceipt = reqRes('GET', { query: { session_id: sessionId } });
    await sessionsHandler(unsafeReceipt.req, unsafeReceipt.res);
    equal(unsafeReceipt.out.statusCode, 200, 'session remains readable when one receipt is unsafe');
    const unsafeReceiptSession = (unsafeReceipt.out.body as Record<string, unknown>).session as Record<string, unknown>;
    assert(
      !('receipt' in ((unsafeReceiptSession.messages as Record<string, unknown>[])[1] ?? {})),
      'a training-authorized turn is never projected as a no-training reconciliation receipt',
    );
    unsafeReceiptAuthority = false;
    const getCall = calls.find((call) => call.url.endsWith('/v1/sessions/get'));
    equal(getCall?.body.session_id, sessionId, 'get tries canonical upstream session_id first');

    const crossOriginDelete = reqRes('DELETE', {
      body: { session_id: sessionId, request_id: randomUUID() },
      origin: 'https://attacker.example',
    });
    await sessionsHandler(crossOriginDelete.req, crossOriginDelete.res);
    equal(crossOriginDelete.out.statusCode, 403, 'cross-origin deletion fails closed');

    const extraDeleteField = reqRes('DELETE', {
      body: { session_id: sessionId, request_id: randomUUID(), session_principal: principalB },
    });
    await sessionsHandler(extraDeleteField.req, extraDeleteField.res);
    equal(extraDeleteField.out.statusCode, 400, 'delete rejects client-asserted principal authority');

    const deleteRequestId = randomUUID();
    const remove = reqRes('DELETE', {
      body: { session_id: sessionId, request_id: deleteRequestId },
    });
    await sessionsHandler(remove.req, remove.res);
    equal(remove.out.statusCode, 200, 'same-origin exact deletion succeeds');
    const removeBody = remove.out.body as Record<string, unknown>;
    equal(removeBody.session_id, sessionId, 'canonical session UUID is returned');
    assert(!('conversation_id' in removeBody), 'legacy delete identity is not exposed');
    assert(!('ledger_tip_digest' in removeBody), 'delete does not expose the global ledger tip');
    const deleteCalls = calls.filter((call) => call.url.endsWith('/v1/sessions/delete'));
    equal(deleteCalls.length, 1, 'delete uses one canonical runtime request');
    equal(deleteCalls[0]?.body.session_id, sessionId, 'delete uses canonical session_id');
    assert(deleteCalls[0]?.body.request_id !== deleteRequestId, 'delete request identity is principal scoped upstream');
    assert(!JSON.stringify(remove.out.body).includes('principal:apocky-member:'), 'principal is never reflected');

    getMode = 'bounded';
    const bounded = reqRes('GET', { query: { session_id: sessionId } });
    await sessionsHandler(bounded.req, bounded.res);
    equal(bounded.out.statusCode, 200, 'oversized-but-bounded upstream history is safely projected');
    const boundedSession = (bounded.out.body as Record<string, unknown>).session as Record<string, unknown>;
    equal((boundedSession.messages as unknown[]).length, 64, 'public snapshot materializes at most 64 messages');
    equal(boundedSession.events_truncated, true, 'gateway truncation is disclosed honestly');
    equal(
      (boundedSession.world as Record<string, unknown>).message_count,
      130,
      'world count retains the total while the visible materialization remains bounded',
    );
    equal(
      (((boundedSession.surface_truncation as Record<string, unknown>).messages as Record<string, unknown>).visible),
      64,
      'gateway-specific visible count is reprojected after its byte/count bound',
    );

    bindingVerified = false;
    getMode = 'normal';
    const unverifiedBinding = reqRes('GET', { query: { session_id: sessionId } });
    await sessionsHandler(unverifiedBinding.req, unverifiedBinding.res);
    equal(unverifiedBinding.out.statusCode, 502, 'missing runtime binding attestation fails closed');
    bindingVerified = true;

    getMode = 'foreign';
    const foreign = reqRes('GET', { query: { session_id: sessionId } });
    await sessionsHandler(foreign.req, foreign.res);
    equal(foreign.out.statusCode, 502, 'foreign session envelope fails closed');
    assert(
      !JSON.stringify(foreign.out.body).includes('B-private-content-must-not-escape'),
      'foreign session content is never reflected',
    );

    getMode = 'dual';
    const dual = reqRes('GET', { query: { session_id: sessionId } });
    await sessionsHandler(dual.req, dual.res);
    equal(dual.out.statusCode, 502, 'conflicting canonical and legacy upstream identities fail closed');

    console.log('public-apocrypha-sessions.test : OK');
  } finally {
    globalThis.fetch = originalFetch;
    for (const key of Object.keys(process.env)) {
      if (!(key in originalEnv)) delete process.env[key];
    }
    Object.assign(process.env, originalEnv);
  }
}

main().catch((error) => {
  console.error(error);
  process.exitCode = 1;
});
