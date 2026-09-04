import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';

import {
  RuntimeProxyError,
  getOwnerBrainRuntimeSession,
  listOwnerBrainRuntimeSessions,
  publicMemberPrincipalRef,
} from '@/lib/apocv4/runtime-proxy';
import { sendOwnerBrainTurn } from '@/lib/brain/runtime-provider';

type JsonObject = Record<string, unknown>;

const ORIGIN = 'https://198.51.100.42:31234';
const TOKEN = 'g12-owner-runtime-test-token';
const PARTITION = 'owner:apocky';
const SESSION_ID = '11111111-1111-4111-8111-111111111111';
const REQUEST_A = '22222222-2222-4222-8222-222222222222';
const REQUEST_B = '33333333-3333-4333-8333-333333333333';

function canonicalJson(value: unknown): string {
  if (value === null || typeof value === 'string' || typeof value === 'boolean') return JSON.stringify(value);
  if (typeof value === 'number') return JSON.stringify(value);
  if (Array.isArray(value)) return `[${value.map(canonicalJson).join(',')}]`;
  const row = value as JsonObject;
  return `{${Object.keys(row).sort().map(key => `${JSON.stringify(key)}:${canonicalJson(row[key])}`).join(',')}}`;
}

function digestJson(value: unknown): string {
  return createHash('sha256').update(canonicalJson(value), 'utf8').digest('hex');
}

function digest(character: string): string {
  return character.repeat(64);
}

function response(conversationId: string, requestId: string, text = 'A verified G12 response.'): JsonObject {
  const usage = { prompt_tokens: 12, completion_tokens: 7 };
  const model = {
    evidence_lane: 'model_reported_not_observed_fact',
    model_id: 'fixture/g12',
    model_revision: 'fixture-revision',
    model_family: 'fixture-family',
    serving_profile_digest: digest('1'),
    response_id: `response-${requestId}`,
    prompt_digest: digest('2'),
    response_digest: '',
    rationale_present: false,
    rationale_digest: null,
    token_admission_digest: null,
    token_admission: null,
    usage,
  };
  model.response_digest = digestJson({
    model_id: model.model_id,
    model_revision: model.model_revision,
    model_family: model.model_family,
    serving_profile_digest: model.serving_profile_digest,
    response_id: model.response_id,
    prompt_digest: model.prompt_digest,
    token_admission_digest: null,
    text,
    rationale_digest: null,
    usage,
  });
  return {
    schema_version: 'apocv4.chat-response.v2',
    text,
    model_reported: model,
    observed: {
      evidence_lane: 'observed_runtime_transport',
      latency_ms: 4.25,
      transport_kind: 'fixture',
      transport_receipt_digest: null,
    },
    authority: {
      effect_authority: 'NONE',
      tool_authority: 'READ_ONLY_CONTEXT',
      memory_scope: 'owner_partitioned_retrieval',
      conversation_history: 'session_bounded',
      training_consent: false,
    },
    identity: {
      schema_version: 'apocv4.identity.v1',
      system_id: 'apocrypha',
      architecture: 'governed_hybrid_digital_intelligence',
      compiler_version: 'g12-fixture',
      identity_digest: digest('3'),
      learned_model_role: 'replaceable_faculty_not_system_identity',
      lineage: 'g12-fixture-lineage',
    },
    context: {
      frame_id: 'acf-g12-fixture',
      frame_digest: digest('4'),
      provenance_spine_digest: digest('5'),
      retrieval: { status: 'EMPTY', count: 0, refs: [] },
      memory: { provider: 'owner', status: 'EMPTY', records_used: 0, receipt_digest: null, refs: [] },
      capabilities: [],
    },
    conversation_id: conversationId,
    request_id: requestId,
    privacy_partition_ref: digestJson(PARTITION),
    outcome: 'completed',
    learned_faculty_used: true,
    duplicate_effect_protection: 'not_applicable_no_effect_authority',
    living_cognition: {
      user_percept: { state: 'UNCONFIGURED', queued: false },
      response_percept: { state: 'UNCONFIGURED', queued: false },
      runtime: { configured: false, perpetual: true, state: 'UNCONFIGURED' },
    },
  };
}

function completedTurn(conversationId: string, requestId: string): JsonObject {
  const chat = response(conversationId, requestId);
  const model = chat.model_reported as JsonObject;
  const core = {
    schema_version: 'apocv4.chat-history-visible-turn.v3',
    state: 'COMPLETED',
    request_id: requestId,
    conversation_id: conversationId,
    user_message: 'Continue this worldline.',
    assistant_message: chat.text,
    response: chat,
    error_class: null,
    failure_digest: null,
    public_error: null,
    token_admission_digest: null,
    token_admission: null,
    recorded_at: '2026-09-04T20:00:00.000Z',
    response_digest: model.response_digest,
    terminal_receipt_digest: digest('6'),
  };
  return { ...core, turn_digest: digestJson(core) };
}

function failedTurn(conversationId: string, requestId: string): JsonObject {
  const core = {
    schema_version: 'apocv4.chat-history-visible-turn.v3',
    state: 'FAILED',
    request_id: requestId,
    conversation_id: conversationId,
    user_message: 'A request that reached a typed capacity boundary.',
    assistant_message: null,
    response: null,
    error_class: 'ChatPromptCapacityError',
    failure_digest: digest('7'),
    public_error: {
      schema_version: 'apocv4.chat-public-failure.v1',
      http_status: 422,
      error: 'chat_prompt_capacity_exceeded',
      error_digest: digest('8'),
    },
    token_admission_digest: null,
    token_admission: null,
    recorded_at: '2026-09-04T20:01:00.000Z',
    response_digest: null,
    terminal_receipt_digest: digest('9'),
  };
  return { ...core, turn_digest: digestJson(core) };
}

function page(
  conversationId: string | null,
  turns: JsonObject[],
  hasMore = false,
): JsonObject {
  const core = {
    schema_version: 'apocv4.chat-history-page.v1',
    conversation_id: conversationId,
    turns,
    next_cursor: hasMore ? turns.at(-1)?.request_id ?? null : null,
    has_more: hasMore,
    persistence: 'DURABLE_PRINCIPAL_BOUND',
    effect_authority: 'NONE',
  };
  return { ...core, page_digest: digestJson(core) };
}

function runtimeEnvelope(result: JsonObject): Response {
  const principalRef = digest('c');
  const privacyPartitionRef = digestJson({
    schema_version: 'apocv4.runtime-auth.v1',
    privacy_partition: PARTITION,
  });
  const bindingRef = digestJson({
    schema_version: 'apocv4.runtime-auth.v1',
    principal_ref: principalRef,
    privacy_partition_ref: privacyPartitionRef,
  });
  return new Response(JSON.stringify({ schema_version: 'apocv4.runtime-service.v1', result }), {
    status: 200,
    headers: {
      'Content-Type': 'application/json',
      'X-Apocv4-Auth-Mode': 'STRICT_REGISTRY',
      'X-Apocv4-Auth-Registry-Ref': digest('a'),
      'X-Apocv4-Binding-Ref': bindingRef,
      'X-Apocv4-Principal-Ref': principalRef,
      'X-Apocv4-Privacy-Partition-Ref': privacyPartitionRef,
    },
  });
}

async function rejectsCode(run: () => Promise<unknown>, code: string): Promise<void> {
  await assert.rejects(run, (error: unknown) => error instanceof RuntimeProxyError && error.code === code);
}

const originalFetch = globalThis.fetch;
const previous = {
  url: process.env.APOCV4_RUNTIME_URL,
  token: process.env.APOCV4_API_TOKEN,
  transport: process.env.APOCV4_RUNTIME_TRANSPORT,
  ip: process.env.APOCV4_RUNTIME_DIRECT_IP,
  port: process.env.APOCV4_RUNTIME_DIRECT_PORT,
  enabled: process.env.APOCKY_BRAIN_LOCAL_PROVIDER_ENABLED,
  tunnelHost: process.env.APOCRYPHA_TUNNEL_HOST,
  accessId: process.env.CF_ACCESS_CLIENT_ID,
  accessSecret: process.env.CF_ACCESS_CLIENT_SECRET,
};

async function main(): Promise<void> {
  try {
    process.env.APOCV4_RUNTIME_URL = ORIGIN;
    process.env.APOCV4_API_TOKEN = TOKEN;
    process.env.APOCV4_RUNTIME_TRANSPORT = 'test-fetch';
    process.env.APOCV4_RUNTIME_DIRECT_IP = '198.51.100.42';
    process.env.APOCV4_RUNTIME_DIRECT_PORT = '31234';
    process.env.APOCKY_BRAIN_LOCAL_PROVIDER_ENABLED = '1';

    let capturedUrl = '';
    let capturedInit: RequestInit | undefined;
    globalThis.fetch = async (input, init) => {
      capturedUrl = String(input);
      capturedInit = init;
      return runtimeEnvelope(response(SESSION_ID, REQUEST_A));
    };
    const chat = await sendOwnerBrainTurn({
      userId: 'owner-user',
      text: 'Continue this worldline.',
      sessionId: SESSION_ID,
      requestId: REQUEST_A,
    });
    assert.equal(capturedUrl, `${ORIGIN}/v1/chat`);
    const body = JSON.parse(String(capturedInit?.body)) as JsonObject;
    assert.deepEqual(Object.keys(body).sort(), [
      'conversation_id', 'message', 'privacy_partition', 'request_id',
    ]);
    assert.equal(body.conversation_id, SESSION_ID, 'G12 stores the browser worldline ID inside its principal-bound scope');
    assert.equal(body.request_id, REQUEST_A, 'device request identity round-trips for durable idempotency');
    assert.equal(body.privacy_partition, PARTITION);
    assert(!Object.hasOwn(body, 'session_principal'), 'G12 rejects the retired body principal field');
    assert(!Object.hasOwn(body, 'session_binding_mac'), 'G12 uses credential-registry binding rather than legacy body HMAC');
    assert.equal(chat.authority.conversation_history, 'session_bounded');

    const first = completedTurn(SESSION_ID, REQUEST_A);
    const second = failedTurn(SESSION_ID, REQUEST_B);
    const seenUrls: string[] = [];
    globalThis.fetch = async (input) => {
      const url = String(input);
      seenUrls.push(url);
      const parsed = new URL(url);
      assert.equal(parsed.pathname, '/v1/chat/history');
      assert.equal(parsed.searchParams.get('privacy_partition'), PARTITION);
      assert.equal(parsed.searchParams.get('limit'), '32');
      if (parsed.searchParams.get('cursor') === REQUEST_A) return runtimeEnvelope(page(SESSION_ID, [second]));
      return runtimeEnvelope(page(SESSION_ID, [first], true));
    };
    const binding = {
      sessionPrincipal: publicMemberPrincipalRef('owner-user'),
      privacyPartition: PARTITION,
      credentialProfile: 'owner' as const,
    };
    const listing = await listOwnerBrainRuntimeSessions({ ...binding, limit: 24 });
    assert.equal(listing.discovery_scope, 'latest_conversation_only');
    assert.equal(listing.count, 1);
    assert.equal(listing.sessions[0]?.session_id, SESSION_ID, 'latest G12 worldline remains browser-addressable');
    assert.equal(listing.sessions[0]?.message_count, 3, 'failed turns retain the user message without inventing an assistant reply');
    assert.equal(listing.sessions[0]?.failed_turn_count, 1);
    assert.equal(listing.observed.page_count, 2);
    assert.equal(new URL(seenUrls[0]!).searchParams.has('conversation_id'), false, 'list asks G12 for its latest principal-bound conversation');
    assert.equal(new URL(seenUrls[1]!).searchParams.get('conversation_id'), SESSION_ID);

    seenUrls.length = 0;
    const loaded = await getOwnerBrainRuntimeSession({ ...binding, sessionId: SESSION_ID });
    assert.equal(loaded.kind, 'owner_brain_history_get');
    assert.equal(loaded.session.history_surface, 'g12_chat_history');
    assert.equal(loaded.session.session_id, SESSION_ID);
    assert.equal(loaded.session.events_truncated, false);
    assert.equal(loaded.session.failed_turn_count, 1);
    assert.match(loaded.session.tip_digest ?? '', /^[0-9a-f]{64}$/u);
    assert.deepEqual(
      loaded.session.messages.map(message => [message.role, message.request_id]),
      [['user', REQUEST_A], ['assistant', REQUEST_A], ['user', REQUEST_B]],
      'desktop and mobile readback preserve request IDs and chronological role mapping',
    );
    const assistant = loaded.session.messages[1] as JsonObject;
    assert.equal((assistant.receipt as JsonObject).conversation_history, 'session_bounded');
    assert.equal(new URL(seenUrls[0]!).searchParams.get('conversation_id'), SESSION_ID);

    const corrupted = page(SESSION_ID, [first]);
    corrupted.page_digest = digest('f');
    globalThis.fetch = async () => runtimeEnvelope(corrupted);
    await rejectsCode(
      () => getOwnerBrainRuntimeSession({ ...binding, sessionId: SESSION_ID }),
      'runtime_response_invalid',
    );

    process.env.APOCV4_RUNTIME_TRANSPORT = 'cloudflare-access';
    process.env.APOCV4_RUNTIME_URL = 'https://apocrypha.apocky.com';
    process.env.APOCRYPHA_TUNNEL_HOST = 'apocrypha.apocky.com';
    process.env.CF_ACCESS_CLIENT_ID = 'fixture-access-client';
    process.env.CF_ACCESS_CLIENT_SECRET = 'fixture-access-secret';
    let cloudflareInit: RequestInit | undefined;
    globalThis.fetch = async (_input, init) => {
      cloudflareInit = init;
      return runtimeEnvelope(page(SESSION_ID, [first]));
    };
    const cloudflareHistory = await getOwnerBrainRuntimeSession({ ...binding, sessionId: SESSION_ID });
    const cloudflareHeaders = new Headers(cloudflareInit?.headers);
    assert.equal(cloudflareHeaders.get('cf-access-client-id'), 'fixture-access-client');
    assert.equal(cloudflareHeaders.get('cf-access-client-secret'), 'fixture-access-secret');
    assert.equal(cloudflareHistory.session.session_id, SESSION_ID, 'Cloudflare Access transport reaches the same strict G12 adapter');

    globalThis.fetch = async () => runtimeEnvelope({ reflected: 'fixture-access-secret' });
    await rejectsCode(
      () => getOwnerBrainRuntimeSession({ ...binding, sessionId: SESSION_ID }),
      'runtime_reflected_credential',
    );

    console.log('apocv4-g12-brain-history.test : OK · exact G12 chat/history + principal-bound load + paged desktop/mobile mapping');
  } finally {
    globalThis.fetch = originalFetch;
    const mutableEnv = process.env as Record<string, string | undefined>;
    for (const [key, value] of Object.entries({
      APOCV4_RUNTIME_URL: previous.url,
      APOCV4_API_TOKEN: previous.token,
      APOCV4_RUNTIME_TRANSPORT: previous.transport,
      APOCV4_RUNTIME_DIRECT_IP: previous.ip,
      APOCV4_RUNTIME_DIRECT_PORT: previous.port,
      APOCKY_BRAIN_LOCAL_PROVIDER_ENABLED: previous.enabled,
      APOCRYPHA_TUNNEL_HOST: previous.tunnelHost,
      CF_ACCESS_CLIENT_ID: previous.accessId,
      CF_ACCESS_CLIENT_SECRET: previous.accessSecret,
    })) {
      if (value === undefined) delete mutableEnv[key];
      else mutableEnv[key] = value;
    }
  }
}

void main().catch(error => {
  console.error(error);
  process.exitCode = 1;
});
