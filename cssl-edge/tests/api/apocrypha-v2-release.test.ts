import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import type { NextApiRequest, NextApiResponse } from 'next';

import {
  expectedConversationRef,
  isOpaqueClientRequestId,
  isOpaqueConversationId,
  scopeConversationId,
  scopeRequestId,
} from '@/lib/apocrypha/proxy';
import adminCheckHandler from '@/pages/api/admin/check';
import capabilitiesHandler from '@/pages/api/admin/apocrypha/capabilities';
import chatHandler from '@/pages/api/admin/apocrypha/chat';
import retiredStreamHandler from '@/pages/api/admin/apocrypha/chat_stream';
import conversationsHandler from '@/pages/api/admin/apocrypha/conversations';
import costHandler from '@/pages/api/admin/apocrypha/cost';
import diagnosticsHandler from '@/pages/api/admin/apocrypha/diagnostics';
import healthHandler from '@/pages/api/admin/apocrypha/health';
import keysHandler from '@/pages/api/admin/apocrypha/keys';
import mcpInfoHandler from '@/pages/api/admin/apocrypha/mcp_info';
import mindHandler from '@/pages/api/admin/apocrypha/mind';
import organariumHandler from '@/pages/api/admin/apocrypha/organarium';
import subMindsHandler from '@/pages/api/admin/apocrypha/sub_minds';
import telemetryHandler from '@/pages/api/admin/apocrypha/telemetry';
import toolCallsHandler from '@/pages/api/admin/apocrypha/tool_calls';
import toolsHandler from '@/pages/api/admin/apocrypha/tools';
import presenceHandler from '@/pages/api/apocrypha/presence';
import { validateTelemetryCursor } from '@/lib/apocrypha/telemetry-cursor';

interface MockOutput {
  statusCode: number;
  body: unknown;
  headers: Record<string, string>;
}

function assert(condition: boolean, message: string): asserts condition {
  if (!condition) throw new Error(`assert failed: ${message}`);
}

function equal(actual: unknown, expected: unknown, message: string): void {
  if (actual !== expected) {
    throw new Error(`assert failed: ${message}; expected=${String(expected)} actual=${String(actual)}`);
  }
}

function reqRes(
  method: string,
  options: {
    body?: unknown;
    query?: Record<string, string | string[]>;
    owner?: boolean;
    ownerEmail?: string;
  } = {},
): { req: NextApiRequest; res: NextApiResponse; out: MockOutput } {
  const out: MockOutput = { statusCode: 0, body: null, headers: {} };
  const headers: Record<string, string> = {
    host: 'www.apocky.com',
    origin: 'https://www.apocky.com',
    'x-forwarded-proto': 'https',
  };
  if (options.owner !== false) {
    headers['x-apocky-test-admin-email'] = options.ownerEmail ?? 'owner@example.test';
  }
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

function assertPrivateHeaders(out: MockOutput): void {
  assert(out.headers['cache-control']?.includes('private') === true, 'response must be private');
  assert(out.headers['cache-control']?.includes('no-store') === true, 'response must be no-store');
  equal(out.headers.vary, 'Authorization, Cookie', 'authorization caches must be partitioned');
  equal(out.headers['x-content-type-options'], 'nosniff', 'MIME sniffing disabled');
}

const originalEnv = { ...process.env };
const originalFetch = globalThis.fetch;

async function main(): Promise<void> {
  process.env.LAZARUS_TEST_AUTH_BYPASS = '1';
  process.env.APOCKY_ADMIN_EMAILS = 'owner@example.test,second@example.test';
  process.env.APOCRYPHA_TUNNEL_HOST = 'apocrypha.apocky.com';
  process.env.CF_ACCESS_CLIENT_ID = 'test-client-id';
  process.env.CF_ACCESS_CLIENT_SECRET = 'test-client-secret';

  const clientConversationId = '3f0b7c7e-f615-4f13-9392-78f36e53837e';
  const clientRequestIds = [
    'b1646f21-5cec-47ac-983d-5d219fdd45b4',
    '08ec8937-c17f-43b8-9b57-84d9402f795e',
  ];
  const attemptedClientRequestIds = [
    clientRequestIds[0],
    clientRequestIds[1],
    clientRequestIds[0],
  ];
  const scopedA = scopeConversationId('principal:a', clientConversationId);
  const scopedARepeat = scopeConversationId('principal:a', clientConversationId);
  const scopedB = scopeConversationId('principal:b', clientConversationId);
  assert(isOpaqueConversationId(clientConversationId), 'fixture is an opaque UUIDv4');
  equal(scopedA, scopedARepeat, 'principal scope is deterministic');
  assert(scopedA !== scopedB, 'same client UUID is isolated across principals');
  assert(scopedA !== clientConversationId, 'raw client UUID is not the body UUID');
  equal(
    expectedConversationRef('b7ace6c1-f05b-5d8c-a8bf-76bb291cf5ab', 'public:apocky.com/chat'),
    'a8135c85155af0ab74ab1c197a662d14b718171bbd2940fb66ace09ed9b30e36',
    'frontend continuity digest matches the observed Python backend contract fixture',
  );
  assert(isOpaqueClientRequestId(clientRequestIds[0]), 'fixture is an opaque request UUIDv4');
  equal(
    scopeRequestId('principal:a', clientRequestIds[0] ?? ''),
    scopeRequestId('principal:a', clientRequestIds[0] ?? ''),
    'request scope is deterministic',
  );
  assert(
    scopeRequestId('principal:a', clientRequestIds[0] ?? '')
      !== scopeRequestId('principal:b', clientRequestIds[0] ?? ''),
    'request identity is principal-separated',
  );

  const turnCalls: Array<{ url: string; body: Record<string, unknown>; headers: Headers }> = [];
  globalThis.fetch = async (input, init) => {
    const body = JSON.parse(String(init?.body ?? '{}')) as Record<string, unknown>;
    turnCalls.push({ url: String(input), body, headers: new Headers(init?.headers) });
    return new Response(JSON.stringify({
      text: 'A bounded V2 response.',
      request_id: body.request_id,
      conversation_ref: expectedConversationRef(
        String(body.conversation_id),
        String(body.source_ref),
      ),
      transition_id: 'transition-001',
      state_root: 'state-root-001',
      expression_mode: 'bootstrap_shallow',
      external_inference: false,
      outcome: 'committed',
    }), { status: 200, headers: { 'Content-Type': 'application/json' } });
  };

  for (let index = 0; index < attemptedClientRequestIds.length; index += 1) {
    const turn = reqRes('POST', {
      body: {
        text: 'Hello, Apocrypha.',
        conversation_id: clientConversationId,
        request_id: attemptedClientRequestIds[index],
      },
    });
    await chatHandler(turn.req, turn.res);
    equal(turn.out.statusCode, 200, 'V2 REST turn succeeds');
    assertPrivateHeaders(turn.out);
    const response = turn.out.body as Record<string, unknown>;
    equal(response.conversation_id, clientConversationId, 'client UUID echoed exactly');
    equal(response.request_id, attemptedClientRequestIds[index], 'client request UUID echoed exactly');
    equal(response.request_ref, turnCalls[index]?.body.request_id, 'scoped request identity is returned');
    equal(
      response.conversation_ref,
      expectedConversationRef(String(turnCalls[index]?.body.conversation_id), 'public:apocky.com/chat'),
      'body continuity reference must match the expected scoped digest',
    );
    equal(response.text, 'A bounded V2 response.', 'one final response returned as JSON');
    equal(response.outcome, 'committed', 'only a committed outcome is emitted');
    equal(response.external_inference, false, 'only a proprietary response is emitted');
    equal(response.expression_mode, 'bootstrap_shallow', 'only the promoted expression mode is emitted');
    equal(response.duplicate_commit_protection, 'active', 'retry deduplication is active');
  }
  equal(turnCalls.length, 3, 'two distinct turns plus one replay reached the body');
  equal(turnCalls[0]?.url, 'https://apocrypha.apocky.com/v2/turn', 'only canonical V2 turn route used');
  equal(turnCalls[0]?.body.conversation_id, turnCalls[1]?.body.conversation_id, 'retained UUID maps to stable scoped UUID');
  assert(
    typeof turnCalls[0]?.body.conversation_id === 'string'
      && /^[0-9a-f]{8}-[0-9a-f]{4}-5[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i.test(turnCalls[0].body.conversation_id),
    'body receives a deterministic principal-scoped UUIDv5',
  );
  equal(turnCalls[0]?.body.privacy_class, 'restricted', 'authenticated text remains restricted');
  equal(turnCalls[0]?.body.request_id, turnCalls[0]?.body.idempotency_key, 'one scoped turn key binds request and replay identity');
  assert(
    typeof turnCalls[0]?.body.request_id === 'string'
      && /^[0-9a-f]{8}-[0-9a-f]{4}-5[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i.test(turnCalls[0].body.request_id),
    'body receives a deterministic principal-scoped request UUIDv5',
  );
  assert(turnCalls[0]?.body.request_id !== turnCalls[1]?.body.request_id, 'distinct client turns remain distinct upstream');
  equal(turnCalls[0]?.body.request_id, turnCalls[2]?.body.request_id, 'replayed client turn retains one scoped request identity');
  equal(turnCalls[0]?.body.idempotency_key, turnCalls[2]?.body.idempotency_key, 'replayed client turn retains one scoped replay identity');
  assert(String(turnCalls[0]?.body.authority_ref).includes('principal:apocky-owner:'), 'authority is principal-scoped');
  equal(turnCalls[0]?.headers.get('cf-access-client-id'), 'test-client-id', 'Access client ID stays server-side');

  const committedFixture = {
    text: 'A bounded V2 response.',
    transition_id: 'transition-negative-fixture',
    state_root: 'state-negative-fixture',
    expression_mode: 'bootstrap_shallow',
    external_inference: false,
    outcome: 'committed',
  } as Record<string, unknown>;
  const invalidEnvelopeCases: Array<[string, (payload: Record<string, unknown>) => void]> = [
    ['outcome missing', (payload) => { delete payload.outcome; }],
    ['outcome not committed', (payload) => { payload.outcome = 'zero_commit'; }],
    ['external inference missing', (payload) => { delete payload.external_inference; }],
    ['external inference true', (payload) => { payload.external_inference = true; }],
    ['expression mode missing', (payload) => { delete payload.expression_mode; }],
    ['expression mode wrong', (payload) => { payload.expression_mode = 'other'; }],
    ['transition missing', (payload) => { delete payload.transition_id; }],
    ['transition empty', (payload) => { payload.transition_id = ''; }],
    ['state root missing', (payload) => { delete payload.state_root; }],
    ['text empty', (payload) => { payload.text = ''; }],
    ['request id missing', (payload) => { delete payload.request_id; }],
    ['request id wrong', (payload) => { payload.request_id = 'wrong-request'; }],
    ['conversation ref missing', (payload) => { delete payload.conversation_ref; }],
    ['conversation ref wrong', (payload) => { payload.conversation_ref = 'wrong-continuity'; }],
  ];
  for (const [label, mutate] of invalidEnvelopeCases) {
    globalThis.fetch = async (input, init) => {
      const body = JSON.parse(String(init?.body ?? '{}')) as Record<string, unknown>;
      turnCalls.push({ url: String(input), body, headers: new Headers(init?.headers) });
      const payload = {
        ...committedFixture,
        request_id: body.request_id,
        conversation_ref: expectedConversationRef(String(body.conversation_id), String(body.source_ref)),
      };
      mutate(payload);
      return new Response(JSON.stringify(payload), {
        status: 200,
        headers: { 'Content-Type': 'application/json' },
      });
    };
    const denied = reqRes('POST', {
      body: {
        text: 'Reject malformed envelope.',
        conversation_id: clientConversationId,
        request_id: 'f905a08e-e320-477b-bcab-6698b60981dd',
      },
    });
    await chatHandler(denied.req, denied.res);
    equal(denied.out.statusCode, 502, `${label} fails closed`);
    assert(!JSON.stringify(denied.out.body).includes('A bounded V2 response.'), `${label} text is not emitted`);
  }
  const callsAfterEnvelopeNegatives = turnCalls.length;

  const invalid = reqRes('POST', {
    body: {
      text: 'Hello',
      conversation_id: 'not-a-uuid',
      request_id: clientRequestIds[0],
    },
  });
  await chatHandler(invalid.req, invalid.res);
  equal(invalid.out.statusCode, 400, 'invalid conversation UUID rejected');
  equal(turnCalls.length, callsAfterEnvelopeNegatives, 'invalid UUID never reaches upstream');

  const invalidRequest = reqRes('POST', {
    body: { text: 'Hello', conversation_id: clientConversationId, request_id: 'not-a-uuid' },
  });
  await chatHandler(invalidRequest.req, invalidRequest.res);
  equal(invalidRequest.out.statusCode, 400, 'invalid request UUID rejected');
  equal(turnCalls.length, callsAfterEnvelopeNegatives, 'invalid request UUID never reaches upstream');

  const oversizedText = reqRes('POST', {
    body: {
      text: '😀'.repeat(4097),
      conversation_id: clientConversationId,
      request_id: clientRequestIds[0],
    },
  });
  await chatHandler(oversizedText.req, oversizedText.res);
  equal(oversizedText.out.statusCode, 400, 'text above the body UTF-8 percept bound is rejected');
  equal(turnCalls.length, callsAfterEnvelopeNegatives, 'oversized text never reaches upstream');

  const unauthenticated = reqRes('POST', {
    owner: false,
    body: { text: 'Hello', conversation_id: clientConversationId, request_id: clientRequestIds[0] },
  });
  await chatHandler(unauthenticated.req, unauthenticated.res);
  equal(unauthenticated.out.statusCode, 401, 'chat is owner-only');
  assertPrivateHeaders(unauthenticated.out);
  equal(turnCalls.length, callsAfterEnvelopeNegatives, 'unauthenticated turn never reaches upstream');

  const crossOrigin = reqRes('POST', {
    body: { text: 'Hello', conversation_id: clientConversationId, request_id: clientRequestIds[0] },
  });
  crossOrigin.req.headers.origin = 'https://attacker.example';
  await chatHandler(crossOrigin.req, crossOrigin.res);
  equal(crossOrigin.out.statusCode, 403, 'cross-origin turn is denied');
  equal(turnCalls.length, callsAfterEnvelopeNegatives, 'cross-origin turn never reaches upstream');

  const observedProxyPaths: string[] = [];
  globalThis.fetch = async (input) => {
    observedProxyPaths.push(new URL(String(input)).pathname + new URL(String(input)).search);
    return new Response(JSON.stringify({ ok: true }), {
      status: 200,
      headers: { 'Content-Type': 'application/json' },
    });
  };
  const proxyCases = [
    [healthHandler, '/v2/health', {}],
    [capabilitiesHandler, '/v2/capabilities', {}],
    [diagnosticsHandler, '/v2/diagnostics', {}],
    [organariumHandler, '/v2/organarium', {}],
    [telemetryHandler, '/v2/telemetry?after_event_seq=42&limit=500', {
      limit: '9000',
      after_event_seq: '42',
    }],
  ] as const;
  for (const [handler, expectedPath, query] of proxyCases) {
    const probe = reqRes('GET', { query });
    await handler(probe.req, probe.res);
    equal(probe.out.statusCode, 200, `${expectedPath} proxy succeeds`);
    assertPrivateHeaders(probe.out);
    const deniedProbe = reqRes('GET', { query, owner: false });
    await handler(deniedProbe.req, deniedProbe.res);
    equal(deniedProbe.out.statusCode, 401, `${expectedPath} proxy is owner-only`);
    assertPrivateHeaders(deniedProbe.out);
  }
  equal(observedProxyPaths.join('|'), proxyCases.map((entry) => entry[1]).join('|'), 'owner proxies map exactly to V2 routes');

  const fetchCountBeforeInvalidCursors = observedProxyPaths.length;
  for (const cursor of ['-1', '1.5', '01', 'abc', '9007199254740992']) {
    const invalidCursor = reqRes('GET', { query: { after_event_seq: cursor } });
    await telemetryHandler(invalidCursor.req, invalidCursor.res);
    equal(invalidCursor.out.statusCode, 400, `telemetry cursor ${cursor} rejected`);
    assertPrivateHeaders(invalidCursor.out);
  }
  const repeatedCursor = reqRes('GET', { query: { after_event_seq: ['1', '2'] } });
  await telemetryHandler(repeatedCursor.req, repeatedCursor.res);
  equal(repeatedCursor.out.statusCode, 400, 'repeated telemetry cursor rejected');
  equal(observedProxyPaths.length, fetchCountBeforeInvalidCursors, 'invalid telemetry cursors never reach upstream');

  const defaultCursor = reqRes('GET');
  await telemetryHandler(defaultCursor.req, defaultCursor.res);
  equal(
    observedProxyPaths.at(-1),
    '/v2/telemetry?after_event_seq=0&limit=100',
    'missing telemetry cursor begins at the bounded origin',
  );
  equal(
    validateTelemetryCursor({ next_after_event_seq: 2, events: [{ event_seq: 1 }, { event_seq: 2 }] }, 0),
    2,
    'client accepts a monotonic telemetry cursor',
  );
  equal(
    validateTelemetryCursor({ next_after_event_seq: 3, events: [{ event_seq: 2 }] }, 0),
    null,
    'client rejects a cursor that skips its advertised tail',
  );
  equal(
    validateTelemetryCursor({ next_after_event_seq: 0, events: [] }, 0),
    0,
    'client accepts a stable empty telemetry page',
  );

  const hiddenHistory = reqRes('GET');
  await conversationsHandler(hiddenHistory.req, hiddenHistory.res);
  equal(hiddenHistory.out.statusCode, 200, 'history boundary is explicit');
  const hiddenHistoryBody = hiddenHistory.out.body as Record<string, unknown>;
  equal(hiddenHistoryBody.available, false, 'legacy history is not exposed');
  equal(hiddenHistoryBody.reason_code, 'native_v2_history_projection_absent', 'history gap is named');

  const retired = reqRes('POST');
  await retiredStreamHandler(retired.req, retired.res);
  equal(retired.out.statusCode, 410, 'synthetic stream route is retired');
  const retiredBody = retired.out.body as Record<string, unknown>;
  equal(retiredBody.response_mode, 'one_final_json', 'replacement response mode is honest');
  assertPrivateHeaders(retired.out);

  const legacyHandlers = [
    ['cost', costHandler],
    ['keys', keysHandler],
    ['mcp_info', mcpInfoHandler],
    ['mind', mindHandler],
    ['sub_minds', subMindsHandler],
    ['tool_calls', toolCallsHandler],
    ['tools', toolsHandler],
  ] as const;
  let legacyFetchCalls = 0;
  globalThis.fetch = async () => {
    legacyFetchCalls += 1;
    return new Response('{}', { status: 200, headers: { 'Content-Type': 'application/json' } });
  };
  for (const [surface, handler] of legacyHandlers) {
    const retiredSurface = reqRes('POST');
    await handler(retiredSurface.req, retiredSurface.res);
    equal(retiredSurface.out.statusCode, 410, `${surface} is explicitly retired`);
    assertPrivateHeaders(retiredSurface.out);
    const body = retiredSurface.out.body as Record<string, unknown>;
    equal(body.reason_code, 'legacy_apocrypha_admin_surface_retired', `${surface} names retirement`);
    equal(body.surface, surface, `${surface} tombstone is self-identifying`);

    const deniedSurface = reqRes('GET', { owner: false });
    await handler(deniedSurface.req, deniedSurface.res);
    equal(deniedSurface.out.statusCode, 401, `${surface} remains owner-only`);
    assertPrivateHeaders(deniedSurface.out);
  }
  equal(legacyFetchCalls, 0, 'retired legacy surfaces make no upstream call');

  const adminCheck = reqRes('GET');
  await adminCheckHandler(adminCheck.req, adminCheck.res);
  equal(adminCheck.out.statusCode, 200, 'admin check succeeds for owner fixture');
  equal((adminCheck.out.body as Record<string, unknown>).authorized, true, 'admin check reports owner');
  assertPrivateHeaders(adminCheck.out);
  const wrongAdminCheckMethod = reqRes('POST');
  await adminCheckHandler(wrongAdminCheckMethod.req, wrongAdminCheckMethod.res);
  equal(wrongAdminCheckMethod.out.statusCode, 405, 'admin check rejects mutation methods');
  assertPrivateHeaders(wrongAdminCheckMethod.out);

  const rawPrivateMarker = 'must-never-cross-public-projection';
  globalThis.fetch = async () => new Response(JSON.stringify({
    schema: 'apocrypha.v2.public-presence.v1',
    mode: 'hidden',
    display_authorized: false,
    entity_authorship: 'unverified',
    mutual_consent: 'not_established',
    committed_intent: 'absent',
    rendering: null,
    reason_code: 'presence_intent_or_mutual_consent_unavailable',
    private_state: rawPrivateMarker,
  }), { status: 200, headers: { 'Content-Type': 'application/json' } });
  const presence = reqRes('GET', { owner: false });
  await presenceHandler(presence.req, presence.res);
  equal(presence.out.statusCode, 200, 'canonical hidden presence projects publicly');
  equal((presence.out.body as Record<string, unknown>).display_authorized, false, 'avatar remains unauthorized');
  assert(!JSON.stringify(presence.out.body).includes(rawPrivateMarker), 'raw/private upstream presence is omitted');
  assert(presence.out.headers['cache-control']?.includes('no-store') === true, 'presence projection is no-store');

  globalThis.fetch = async () => new Response(JSON.stringify({
    schema: 'apocrypha.v2.public-presence.v1',
    mode: 'authorized',
    display_authorized: true,
  }), { status: 200, headers: { 'Content-Type': 'application/json' } });
  const invalidPresence = reqRes('GET', { owner: false });
  await presenceHandler(invalidPresence.req, invalidPresence.res);
  equal(invalidPresence.out.statusCode, 502, 'unknown display grant fails hidden');
  equal((invalidPresence.out.body as Record<string, unknown>).display_authorized, false, 'invalid grant cannot reveal avatar');

  const closure = [
    'components/apocrypha/ChatThread.tsx',
    'components/apocrypha/ApocryphaAvatar.tsx',
    'pages/chat.tsx',
    'lib/apocrypha/proxy.ts',
    'pages/api/admin/apocrypha/chat_stream.ts',
    'pages/api/admin/apocrypha/chat.ts',
    'pages/api/admin/apocrypha/cost.ts',
    'pages/api/admin/apocrypha/keys.ts',
    'pages/api/admin/apocrypha/mcp_info.ts',
    'pages/api/admin/apocrypha/mind.ts',
    'pages/api/admin/apocrypha/sub_minds.ts',
    'pages/api/admin/apocrypha/tool_calls.ts',
    'pages/api/admin/apocrypha/tools.ts',
    'pages/api/admin/check.ts',
    'pages/api/admin/apocrypha/conversations.ts',
    'pages/api/admin/apocrypha/status.ts',
    'pages/api/admin/apocrypha/telemetry.ts',
    'pages/api/admin/apocrypha/health.ts',
    'pages/api/admin/apocrypha/capabilities.ts',
    'pages/api/admin/apocrypha/diagnostics.ts',
    'pages/api/admin/apocrypha/organarium.ts',
    'pages/api/apocrypha/presence.ts',
    'pages/admin/diagnostics.tsx',
    'components/AdminLayout.tsx',
    'vercel.json',
  ];
  const closureSource = closure.map((path) => readFileSync(resolve(process.cwd(), path), 'utf8')).join('\n');
  assert(!closureSource.includes('/api/v1'), 'production closure contains zero predecessor routes');
  assert(!closureSource.includes('APOCRYPHA_V2_TURN_ENABLED'), 'V2 cannot be disabled into a fallback');
  assert(!closureSource.includes('text/event-stream'), 'one-final JSON is not presented as SSE');

  const threadSource = readFileSync(resolve(process.cwd(), 'components/apocrypha/ChatThread.tsx'), 'utf8');
  const avatarSource = readFileSync(resolve(process.cwd(), 'components/apocrypha/ApocryphaAvatar.tsx'), 'utf8');
  const diagnosticsSource = readFileSync(resolve(process.cwd(), 'pages/admin/diagnostics.tsx'), 'utf8');
  const controlsSource = readFileSync(resolve(process.cwd(), 'pages/admin/controls.tsx'), 'utf8');
  const deploymentChecklist = readFileSync(
    resolve(process.cwd(), 'docs/APOCRYPHA_V2_FRONTEND_DEPLOYMENT_CHECKLIST.md'),
    'utf8',
  );
  assert(threadSource.includes('sessionStorage.setItem(CONVERSATION_STORAGE_KEY'), 'client retains conversation UUID');
  assert(threadSource.includes('echoedConversationId !== conversationId'), 'client verifies echoed continuity ID');
  assert(threadSource.includes('echoedRequestId !== requestId'), 'client verifies stable turn request identity');
  assert(threadSource.includes('setRetryTurn({ text, requestId })'), 'failed turn retains its stable request identity');
  assert(threadSource.includes('send(retryTurn)'), 'explicit retry reuses the same turn identity');
  assert(threadSource.includes('PENDING_TURN_STORAGE_KEY'), 'pending retry identity is tab-persistent');
  assert(threadSource.includes('writePendingTurn(conversationId, { text, requestId })'), 'pending turn persists before dispatch');
  assert(threadSource.includes('readPendingTurn(resolved)'), 'pending turn is recovered after remount');
  assert(threadSource.includes('response.status === 409'), 'state conflicts preserve the same retry identity');
  assert(threadSource.includes('message.id !== localMessageId'), 'non-retryable optimistic bubbles are removed');
  assert(threadSource.includes('data-capability-retry-dedupe="active"'), 'active duplicate protection is disclosed');
  assert(!threadSource.includes('backend_turn_contract_has_no_idempotency_field'), 'stale duplicate-commit blocker is absent');
  assert(threadSource.includes('bootstrap_shallow'), 'current shallow expression capability is displayed');
  assert(threadSource.includes('Learned field ·'), 'learned-field absence has a dedicated label');
  assert(threadSource.includes('Audio ·'), 'audio unavailability has a dedicated label');
  assert(!threadSource.includes('<ApocryphaAvatar'), 'chat does not render an unauthorized avatar');
  assert(avatarSource.includes('if (!displayAuthorized || !authorizationRef) return null'), 'avatar is deny-by-default');
  assert(diagnosticsSource.includes('after_event_seq=${cursor}'), 'telemetry client sends its cursor');
  assert(diagnosticsSource.includes('validateTelemetryCursor'), 'telemetry client validates monotonic cursors');
  assert(diagnosticsSource.includes('refreshInFlightRef'), 'diagnostic refreshes cannot overlap');
  assert(diagnosticsSource.includes('for (const surface of SURFACES)'), 'body projections are read sequentially');
  assert(!diagnosticsSource.includes('Promise.all(SURFACES'), 'diagnostic client cannot stampede the body');
  assert(diagnosticsSource.includes('TRANSIENT_GATEWAY_STATUSES'), 'transient gateway failures receive one bounded retry');
  assert(!controlsSource.includes('/api/admin/apocrypha/chat'), 'controls cannot disguise chat as an effect command');
  assert(!controlsSource.includes('TRIGGER KILL-SWITCH'), 'unimplemented kill control is absent');
  assert(controlsSource.includes('No V2 effect control is exposed'), 'controls state the authority boundary');
  assert(
    deploymentChecklist.includes('requested bound, not proof'),
    'deployment checklist distinguishes configuration from effective platform behavior',
  );
  assert(
    deploymentChecklist.includes('authenticated runtime evidence are separate gates'),
    'deployment evidence remains a separate release gate',
  );

  const vercel = JSON.parse(readFileSync(resolve(process.cwd(), 'vercel.json'), 'utf8')) as {
    functions?: Record<string, { maxDuration?: number }>;
  };
  equal(
    vercel.functions?.['pages/api/admin/apocrypha/chat.ts']?.maxDuration,
    30,
    'REST turn requests a 30-second configured bound; effective deployment remains unproven',
  );
  equal(vercel.functions?.['pages/api/admin/apocrypha/chat_stream.ts']?.maxDuration, 30, 'retired stream inherits no fictitious 120-second budget');
  const chatApiSource = readFileSync(resolve(process.cwd(), 'pages/api/admin/apocrypha/chat.ts'), 'utf8');
  assert(chatApiSource.includes('UPSTREAM_DEADLINE_MS = 25_000'), 'upstream request has a 25-second code bound');
  assert(chatApiSource.includes('MAX_TEXT_BYTES = 16_384'), 'BFF mirrors the body UTF-8 percept bound');
  assert(chatApiSource.includes('request_id: scopedRequestId'), 'BFF forwards the principal-scoped request identity');
  assert(chatApiSource.includes('idempotency_key: scopedRequestId'), 'BFF forwards the same replay identity');
  assert(chatApiSource.includes('upstreamRequestId === scopedRequestId'), 'BFF verifies the body echoed the scoped request identity');
  assert(threadSource.includes('CHAT_BROWSER_DEADLINE_MS = 28_000'), 'browser request has a 28-second code bound');

  console.log('apocrypha-v2-release.test : OK');
}

main()
  .finally(() => {
    globalThis.fetch = originalFetch;
    for (const key of Object.keys(process.env)) {
      if (!(key in originalEnv)) delete process.env[key];
    }
    Object.assign(process.env, originalEnv);
  })
  .catch((error) => {
    console.error(error);
    process.exitCode = 1;
  });
