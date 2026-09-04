import { randomUUID } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import type { NextApiRequest, NextApiResponse } from 'next';

import {
  scopeConversationId,
  scopeRequestId,
} from '@/lib/apocrypha/proxy';
import { publicMemberPrincipalRef } from '@/lib/apocv4/runtime-proxy';
import chatHandler from '@/pages/api/apocrypha/chat';

interface Output {
  statusCode: number;
  body: unknown;
  headers: Record<string, string>;
  chunks: string[];
  ended: boolean;
}

interface RequestOptions {
  body?: unknown;
  contentType?: string;
  member?: boolean;
  origin?: string;
  email?: string;
  accept?: string;
}

function assert(condition: boolean, message: string): asserts condition {
  if (!condition) throw new Error(`assert failed: ${message}`);
}

function equal(actual: unknown, expected: unknown, message: string): void {
  if (actual !== expected) {
    throw new Error(
      `assert failed: ${message}; expected=${String(expected)} actual=${String(actual)}`,
    );
  }
}

function isUuidV5(value: unknown): value is string {
  return typeof value === 'string'
    && /^[0-9a-f]{8}-[0-9a-f]{4}-5[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i.test(value);
}

function reqRes(
  method: string,
  options: RequestOptions = {},
): { req: NextApiRequest; res: NextApiResponse; out: Output } {
  const out: Output = { statusCode: 0, body: null, headers: {}, chunks: [], ended: false };
  const headers: Record<string, string> = {
    host: 'www.apocky.com',
    origin: options.origin ?? 'https://www.apocky.com',
    'x-forwarded-proto': 'https',
    'content-type': options.contentType ?? 'application/json',
  };
  if (options.member !== false) {
    headers['x-apocky-test-admin-email'] = options.email ?? 'member@example.test';
  }
  if (options.accept) headers.accept = options.accept;
  const req = {
    method,
    body: options.body,
    query: {},
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
      out.headers[name.toLowerCase()] = Array.isArray(value)
        ? value.join(', ')
        : String(value);
      return this;
    },
    flushHeaders() {
      return this;
    },
    write(value: string | Uint8Array) {
      out.chunks.push(String(value));
      return true;
    },
    end(value?: string | Uint8Array) {
      if (value !== undefined) out.chunks.push(String(value));
      out.ended = true;
      return this;
    },
  } as unknown as NextApiResponse;
  return { req, res, out };
}

function assertPrivate(out: Output): void {
  assert(out.headers['cache-control']?.includes('private') === true, 'response is private');
  assert(out.headers['cache-control']?.includes('no-store') === true, 'response is no-store');
  equal(out.headers.vary, 'Authorization, Cookie', 'cache partitions auth and cookie');
  equal(out.headers['x-content-type-options'], 'nosniff', 'MIME sniffing is disabled');
}

const originalEnv = { ...process.env };
const originalFetch = globalThis.fetch;
const RUNTIME_ORIGIN = 'https://203.0.113.10:9443';
const RUNTIME_TOKEN = 'test-runtime-token';
const PUBLIC_RUNTIME_TOKEN = 'test-public-runtime-token';
const MODEL_ID = 'fixture/frontier-coder';
const SERVING_PROFILE_DIGEST = 'a'.repeat(64);
const PROMPT_DIGEST = 'b'.repeat(64);
const RESPONSE_DIGEST = 'c'.repeat(64);
const PRIVACY_PARTITION_REF = 'd'.repeat(64);

function runtimeChatEnvelope(
  request: Record<string, unknown>,
  text = 'A bounded Apocv4 response.',
  overrides: Record<string, unknown> = {},
): Record<string, unknown> {
  const ownerProfile = request.privacy_partition === 'owner:apocky';
  return {
    schema_version: 'apocv4.runtime-service.v1',
    result: {
      schema_version: 'apocv4.chat-response.v2',
      text,
      model_reported: {
        evidence_lane: 'model_reported_not_observed_fact',
        model_id: MODEL_ID,
        model_revision: 'fixture-revision',
        model_family: 'fixture-family',
        serving_profile_digest: SERVING_PROFILE_DIGEST,
        response_id: 'fixture-response-id',
        prompt_digest: PROMPT_DIGEST,
        response_digest: RESPONSE_DIGEST,
      },
      observed: {
        evidence_lane: 'observed_runtime_transport',
      },
      authority: {
        effect_authority: 'NONE',
        tool_authority: 'READ_ONLY_CONTEXT',
        memory_scope: ownerProfile ? 'owner_partitioned_retrieval' : 'public_safe_retrieval',
        conversation_history: 'durable_principal_bound',
        training_consent: false,
      },
      identity: {
        schema_version: 'apocv4.identity.v1',
        system_id: 'apocrypha',
        architecture: 'governed_hybrid_digital_intelligence',
        compiler_version: 'fixture-compiler-v1',
        identity_digest: 'e'.repeat(64),
        learned_model_role: 'replaceable_faculty_not_system_identity',
        lineage: 'shawn-apocky-directed',
      },
      context: {
        frame_id: 'acf-fixture-frame',
        frame_digest: 'f'.repeat(64),
        provenance_spine_digest: '0'.repeat(64),
        retrieval: { status: 'ready', count: 2, refs: ['source:a', 'source:b'] },
        memory: {
          provider: '3mneme',
          status: 'ready',
          records_used: 1,
          receipt_digest: '1'.repeat(64),
          refs: ['3mneme:fixture'],
        },
        capabilities: [{
          id: '3mneme.recall',
          status: 'ready',
          authority: 'read_only',
          evidence: 'fixture',
        }],
      },
      conversation_id: request.conversation_id,
      request_id: request.request_id,
      privacy_partition_ref: PRIVACY_PARTITION_REF,
      outcome: 'completed',
      learned_faculty_used: true,
      duplicate_effect_protection: 'not_applicable_no_effect_authority',
      ...overrides,
    },
  };
}

async function main(): Promise<void> {
  process.env.LAZARUS_TEST_AUTH_BYPASS = '1';
  Object.assign(process.env, { NODE_ENV: 'test' });
  process.env.APOCV4_RUNTIME_URL = RUNTIME_ORIGIN;
  process.env.APOCV4_RUNTIME_DIRECT_IP = '203.0.113.10';
  process.env.APOCV4_RUNTIME_DIRECT_PORT = '9443';
  process.env.APOCV4_RUNTIME_TRANSPORT = 'test-fetch';
  process.env.APOCV4_API_TOKEN = RUNTIME_TOKEN;
    process.env.APOCV4_PUBLIC_API_TOKEN = PUBLIC_RUNTIME_TOKEN;
    process.env.APOCV4_SESSION_BINDING_SECRET = 's'.repeat(64);

  const sessionId = randomUUID();
  const requestId = randomUUID();
  const baseBody = {
    text: 'Hello, Apocrypha.',
    session_id: sessionId,
    request_id: requestId,
  };

  const principalA = publicMemberPrincipalRef('member-a');
  const principalARepeat = publicMemberPrincipalRef('member-a');
  const principalB = publicMemberPrincipalRef('member-b');
  equal(principalA, principalARepeat, 'member principal is deterministic');
  assert(principalA !== principalB, 'different members receive different principals');
  assert(!principalA.includes('member-a'), 'raw member identity is not embedded');
  assert(
    scopeConversationId(principalA, sessionId)
      !== scopeConversationId(principalB, sessionId),
    'one browser conversation UUID is isolated across members',
  );
  assert(
    scopeRequestId(principalA, requestId) !== scopeRequestId(principalB, requestId),
    'one browser request UUID is isolated across members',
  );

  let upstreamCalls = 0;
  const observed: Array<{
    body: Record<string, unknown>;
    headers: Headers;
    url: string;
  }> = [];
  globalThis.fetch = async (input, init) => {
    upstreamCalls += 1;
    const body = JSON.parse(String(init?.body ?? '{}')) as Record<string, unknown>;
    observed.push({
      body,
      headers: new Headers(init?.headers),
      url: String(input),
    });
    const runtimeEnvelope = runtimeChatEnvelope(body);
    if (String(input).endsWith('/v1/chat/stream')) {
      const result = runtimeEnvelope.result as Record<string, unknown>;
      const answer = String(result.text);
      const events = [
        { schema_version: 'apocv4.chat-stream-event.v1', type: 'delta', text: answer.slice(0, 10) },
        { schema_version: 'apocv4.chat-stream-event.v1', type: 'delta', text: answer.slice(10) },
        { schema_version: 'apocv4.chat-stream-event.v1', type: 'completed', result },
      ];
      return new Response(`${events.map((event) => JSON.stringify(event)).join('\n')}\n`, {
        status: 200,
        headers: {
          'Content-Type': 'application/x-ndjson',
          'X-Apocv4-Session-Binding': 'VERIFIED',
        },
      });
    }
    return new Response(JSON.stringify(runtimeEnvelope), {
      status: 200,
      headers: {
        'Content-Type': 'application/json',
        'X-Apocv4-Session-Binding': 'VERIFIED',
      },
    });
  };

  const wrongMethod = reqRes('GET');
  await chatHandler(wrongMethod.req, wrongMethod.res);
  equal(wrongMethod.out.statusCode, 405, 'only POST is admitted');
  assertPrivate(wrongMethod.out);

  const crossOrigin = reqRes('POST', {
    body: baseBody,
    origin: 'https://attacker.example',
  });
  await chatHandler(crossOrigin.req, crossOrigin.res);
  equal(crossOrigin.out.statusCode, 403, 'cross-origin turn fails closed');

  const unauthenticated = reqRes('POST', { body: baseBody, member: false });
  await chatHandler(unauthenticated.req, unauthenticated.res);
  equal(unauthenticated.out.statusCode, 401, 'signed-out turn fails closed');
  assertPrivate(unauthenticated.out);

  const wrongContentType = reqRes('POST', {
    body: baseBody,
    contentType: 'text/plain',
  });
  await chatHandler(wrongContentType.req, wrongContentType.res);
  equal(wrongContentType.out.statusCode, 415, 'non-JSON content type is rejected');

  const malformedBody = reqRes('POST', { body: [] });
  await chatHandler(malformedBody.req, malformedBody.res);
  equal(malformedBody.out.statusCode, 400, 'array body is rejected');

  const invalidSession = reqRes('POST', {
    body: { ...baseBody, session_id: 'not-a-uuid' },
  });
  await chatHandler(invalidSession.req, invalidSession.res);
  equal(invalidSession.out.statusCode, 400, 'invalid session UUID is rejected');

  const invalidRequest = reqRes('POST', {
    body: { ...baseBody, request_id: 'not-a-uuid' },
  });
  await chatHandler(invalidRequest.req, invalidRequest.res);
  equal(invalidRequest.out.statusCode, 400, 'invalid request UUID is rejected');

  const oversized = reqRes('POST', {
    body: { ...baseBody, text: '😀'.repeat(4097) },
  });
  await chatHandler(oversized.req, oversized.res);
  equal(oversized.out.statusCode, 400, 'UTF-8 payload above 16 KiB is rejected');
  const forgedPrincipal = reqRes('POST', {
    body: { ...baseBody, session_principal: `principal:apocky-member:${'f'.repeat(64)}` },
  });
  await chatHandler(forgedPrincipal.req, forgedPrincipal.res);
  equal(forgedPrincipal.out.statusCode, 400, 'client-asserted session principal is rejected');
  const conflictingIdentity = reqRes('POST', {
    body: { ...baseBody, conversation_id: randomUUID() },
  });
  await chatHandler(conflictingIdentity.req, conflictingIdentity.res);
  equal(conflictingIdentity.out.statusCode, 400, 'dual session and conversation identities are rejected');
  equal(upstreamCalls, 0, 'denied inputs never reach the private body');

  const success = reqRes('POST', { body: baseBody });
  await chatHandler(success.req, success.res);
  equal(success.out.statusCode, 200, 'verified member turn succeeds');
  assertPrivate(success.out);
  const successBody = success.out.body as Record<string, unknown>;
  equal(successBody.text, 'A bounded Apocv4 response.', 'validated runtime text is returned');
  equal(successBody.session_id, sessionId, 'canonical client session UUID is echoed');
  equal(successBody.conversation_id, sessionId, 'legacy UI continuity alias remains compatible');
  equal(successBody.request_id, requestId, 'client request UUID is echoed');
  equal(successBody.model_id, MODEL_ID, 'model identity is exposed as model-reported evidence');
  equal(successBody.response_id, 'fixture-response-id', 'runtime response identity is exposed');
  equal(successBody.response_digest, RESPONSE_DIGEST, 'response digest is exposed');
  equal(
    successBody.serving_profile_digest,
    SERVING_PROFILE_DIGEST,
    'serving-profile digest is exposed',
  );
  equal(successBody.effect_authority, 'NONE', 'public turn receives no effect authority');
  equal(successBody.tool_authority, 'READ_ONLY_CONTEXT', 'public turn receives read-only context authority');
  equal(successBody.outcome, 'completed', 'runtime completion is reported');
  equal(successBody.learned_faculty_used, true, 'runtime confirms a learned faculty answered');
  equal(successBody.memory_scope, 'public_safe_retrieval', 'public conversation uses only public-safe retrieval');
  equal(successBody.training_consent, false, 'public training consent is off');
  equal(
    successBody.conversation_history,
    'durable_principal_bound',
    'public API exposes the durable principal-bound history contract',
  );
  equal(
    successBody.duplicate_effect_protection,
    'not_applicable_no_effect_authority',
    'effect replay is inapplicable because the route has no effect authority',
  );
  equal(successBody.upstream_status, 200, 'runtime transport status is exposed');
  equal(
    (successBody.identity as Record<string, unknown>).system_id,
    'apocrypha',
    'governed Apocrypha identity is surfaced',
  );
  equal(
    ((successBody.context as Record<string, unknown>).memory as Record<string, unknown>).provider,
    '3mneme',
    'memory provider receipt is surfaced',
  );
  assert(!JSON.stringify(successBody).includes('principal:apocky-member:'), 'principal stays server-side');
  assert(!JSON.stringify(successBody).includes('member@example.test'), 'member email stays server-side');

  const streamRequestId = randomUUID();
  const streamed = reqRes('POST', {
    body: { ...baseBody, request_id: streamRequestId },
    accept: 'application/x-ndjson',
  });
  await chatHandler(streamed.req, streamed.res);
  equal(streamed.out.statusCode, 200, 'verified member stream succeeds');
  equal(streamed.out.ended, true, 'member stream closes after its terminal event');
  assert(
    Boolean(streamed.out.headers['content-type']?.startsWith('application/x-ndjson')),
    'member stream exposes the typed NDJSON contract',
  );
  const streamEvents = streamed.out.chunks.join('').trim().split('\n').map(
    (line) => JSON.parse(line) as Record<string, unknown>,
  );
  equal(streamEvents[0]?.type, 'delta', 'first browser event is a real model delta');
  equal(streamEvents[1]?.type, 'delta', 'second browser event is a real model delta');
  equal(streamEvents[2]?.type, 'completed', 'terminal browser event carries the verified receipt');
  equal(
    `${streamEvents[0]?.text}${streamEvents[1]?.text}`,
    'A bounded Apocv4 response.',
    'streamed text matches the terminal runtime response',
  );
  equal(observed[1]?.url, `${RUNTIME_ORIGIN}/v1/chat/stream`, 'stream uses the direct private runtime stream route');

  const firstCall = observed[0];
  assert(firstCall !== undefined, 'upstream call observed');
  equal(firstCall.url, `${RUNTIME_ORIGIN}/v1/chat`, 'direct private Apocv4 chat route used');
  equal(firstCall.headers.get('authorization'), `Bearer ${PUBLIC_RUNTIME_TOKEN}`, 'public runtime credential stays server-side');
  equal(firstCall.headers.get('accept-encoding'), 'identity', 'compressed ambiguity is disabled');
  equal(firstCall.body.message, baseBody.text, 'bounded message crosses the runtime boundary');
  equal(firstCall.body.privacy_partition, 'public:apocrypha', 'member request stays in the public-safe privacy partition');
  assert(firstCall.body.request_id !== requestId, 'raw client request ID is not sent upstream');
  assert(firstCall.body.conversation_id !== sessionId, 'raw client session ID is not used as transport conversation ID');
  assert(isUuidV5(firstCall.body.request_id), 'member-scoped request identity is UUIDv5');
  assert(isUuidV5(firstCall.body.conversation_id), 'member-scoped conversation identity is UUIDv5');
  equal(firstCall.body.session_id, sessionId, 'durable session keeps the original client session UUID');
  equal(
    firstCall.body.session_principal,
    publicMemberPrincipalRef('test-admin'),
    'durable session is bound to the server-derived member principal',
  );
  assert(
    typeof firstCall.body.session_binding_mac === 'string'
      && /^[0-9a-f]{64}$/.test(firstCall.body.session_binding_mac),
    'runtime receives a detached exact-request member binding',
  );
  assert(!('training_consent' in firstCall.body), 'no training opt-in crosses the boundary');
  assert(!('tool_call' in firstCall.body), 'public route grants no tool request');

  const ownerTurn = reqRes('POST', {
    body: { ...baseBody, request_id: randomUUID() },
    email: 'apocky13@gmail.com',
  });
  await chatHandler(ownerTurn.req, ownerTurn.res);
  equal(ownerTurn.out.statusCode, 200, 'allowlisted owner uses the governed owner profile');
  const ownerCall = observed[2];
  assert(ownerCall !== undefined, 'owner upstream call observed');
  equal(ownerCall.headers.get('authorization'), `Bearer ${RUNTIME_TOKEN}`, 'owner runtime credential stays server-side');
  equal(ownerCall.body.privacy_partition, 'owner:apocky', 'owner request uses the owner memory partition');
  equal(
    (ownerTurn.out.body as Record<string, unknown>).memory_scope,
    'owner_partitioned_retrieval',
    'owner receives only owner-partitioned retrieval',
  );

  const legacyTurn = reqRes('POST', {
    body: {
      text: baseBody.text,
      conversation_id: sessionId,
      request_id: randomUUID(),
    },
  });
  await chatHandler(legacyTurn.req, legacyTurn.res);
  equal(legacyTurn.out.statusCode, 200, 'just-committed UI conversation_id remains a typed compatibility input');
  equal(
    (legacyTurn.out.body as Record<string, unknown>).session_id,
    sessionId,
    'legacy input receives the canonical session_id response',
  );

  globalThis.fetch = async (input, init) => {
    upstreamCalls += 1;
    const body = JSON.parse(String(init?.body ?? '{}')) as Record<string, unknown>;
    observed.push({ body, headers: new Headers(init?.headers), url: String(input) });
    return new Response(JSON.stringify(runtimeChatEnvelope(
      body,
      'Must not escape.',
      { conversation_id: randomUUID() },
    )), {
      status: 200,
      headers: {
        'Content-Type': 'application/json',
        'X-Apocv4-Session-Binding': 'VERIFIED',
      },
    });
  };
  const invalidEnvelope = reqRes('POST', {
    body: { ...baseBody, request_id: randomUUID() },
  });
  await chatHandler(invalidEnvelope.req, invalidEnvelope.res);
  equal(invalidEnvelope.out.statusCode, 502, 'foreign conversation envelope fails closed');
  assert(
    !JSON.stringify(invalidEnvelope.out.body).includes('Must not escape.'),
    'unverified body text is not emitted',
  );

  globalThis.fetch = async (input, init) => {
    upstreamCalls += 1;
    const body = JSON.parse(String(init?.body ?? '{}')) as Record<string, unknown>;
    observed.push({ body, headers: new Headers(init?.headers), url: String(input) });
    return new Response(JSON.stringify(runtimeChatEnvelope(
      body,
      'Privacy-invalid text must not escape.',
      {
        authority: {
          effect_authority: 'NONE',
          tool_authority: 'READ_ONLY_CONTEXT',
          memory_scope: 'public_safe_retrieval',
          conversation_history: 'durable_principal_bound',
          training_consent: true,
        },
      },
    )), {
      status: 200,
      headers: {
        'Content-Type': 'application/json',
        'X-Apocv4-Session-Binding': 'VERIFIED',
      },
    });
  };
  const invalidPrivacy = reqRes('POST', {
    body: { ...baseBody, request_id: randomUUID() },
  });
  await chatHandler(invalidPrivacy.req, invalidPrivacy.res);
  equal(invalidPrivacy.out.statusCode, 502, 'runtime training opt-in fails closed');
  assert(
    !JSON.stringify(invalidPrivacy.out.body).includes('Privacy-invalid text must not escape.'),
    'privacy-invalid runtime text is not emitted',
  );

  globalThis.fetch = async (input, init) => {
    upstreamCalls += 1;
    const body = JSON.parse(String(init?.body ?? '{}')) as Record<string, unknown>;
    observed.push({ body, headers: new Headers(init?.headers), url: String(input) });
    return new Response(JSON.stringify(runtimeChatEnvelope(body, 'Rate-limited fixture.')), {
      status: 200,
      headers: {
        'Content-Type': 'application/json',
        'X-Apocv4-Session-Binding': 'VERIFIED',
      },
    });
  };
  let rateLimited: Output | null = null;
  for (let index = 0; index < 10; index += 1) {
    const turn = reqRes('POST', {
      body: { ...baseBody, request_id: randomUUID() },
    });
    await chatHandler(turn.req, turn.res);
    if (turn.out.statusCode === 429) {
      rateLimited = turn.out;
      break;
    }
  }
  assert(rateLimited !== null, 'per-member short turn budget is enforced');
  assert(Number(rateLimited.headers['retry-after']) >= 1, 'rate limit provides retry timing');
  equal(upstreamCalls, 8, 'only eight valid turns reach one warm-instance body window');

  const primaryPage = readFileSync(resolve(process.cwd(), 'pages/apocrypha.tsx'), 'utf8');
  const component = readFileSync(
    resolve(process.cwd(), 'components/apocrypha/PublicChat.tsx'),
    'utf8',
  );
  const css = readFileSync(
    resolve(process.cwd(), 'styles/PublicApocrypha.module.css'),
    'utf8',
  );
  const runtimeProxy = readFileSync(
    resolve(process.cwd(), 'lib/apocv4/runtime-proxy.ts'),
    'utf8',
  );
  const vercel = JSON.parse(
    readFileSync(resolve(process.cwd(), 'vercel.json'), 'utf8'),
  ) as { functions?: Record<string, { maxDuration?: number }> };
  assert(primaryPage.includes('BrainExperience'), 'exact /apocrypha now aliases the private Brain experience');
  assert(primaryPage.includes('requireBrainOwner'), 'exact /apocrypha remains owner-gated');
  assert(!primaryPage.includes('PublicChat'), 'exact /apocrypha must not revive the retired member chat');
  assert(component.includes("authFetch('/api/apocrypha/chat'"), 'browser calls the member BFF');
  assert(component.includes('training_consent === false'), 'browser verifies no training consent');
  assert(component.includes("body.memory_scope === 'public_safe_retrieval'"), 'browser verifies public-safe memory');
  assert(component.includes("effect_authority === 'NONE'"), 'browser verifies no effect authority');
  assert(component.includes("body.tool_authority === 'READ_ONLY_CONTEXT'"), 'browser verifies read-only context authority');
  assert(component.includes('identityReceipt(body.identity)'), 'browser verifies governed identity receipts');
  assert(component.includes('contextReceipt(body.context)'), 'browser verifies context and memory receipts');
  assert(component.includes('response_digest'), 'browser retains response evidence');
  assert(component.includes('serving_profile_digest'), 'browser retains serving-profile evidence');
  assert(component.includes('Retry same turn'), 'bounded retry reuses one turn identity');
  assert(component.includes('CHAT_BROWSER_DEADLINE_MS = 85_000'), 'browser allows a full governed model turn');
  assert(runtimeProxy.includes('CHAT_DEADLINE_MS = 80_000'), 'BFF allows a full governed model turn');
  equal(vercel.functions?.['pages/api/apocrypha/chat.ts']?.maxDuration, undefined, 'retired public chat no longer reserves a production function duration');
  assert(component.includes('No message is sent until the session is verified.'), 'signed-out boundary is explicit');
  assert(
    component.includes('<Link href="/clearing" className={styles.railAction}>')
      && component.includes('<small>Clearing</small>'),
    'the preserved conversation component still routes Clearing as a distinct surface',
  );
  assert(css.includes('@media (max-width: 680px)'), 'narrow mobile layout exists');
  assert(css.includes('prefers-reduced-motion'), 'reduced-motion path exists');
  assert(css.includes('forced-colors'), 'forced-colors path exists');

  console.log('public-apocrypha-chat.test : OK');
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
