import { randomUUID } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

import type { NextApiRequest, NextApiResponse } from 'next';

import {
  expectedConversationRef,
  scopeConversationId,
  scopeRequestId,
} from '@/lib/apocrypha/proxy';
import chatHandler, {
  publicMemberPrincipalRef,
} from '@/pages/api/apocrypha/chat';

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

function reqRes(
  method: string,
  options: RequestOptions = {},
): { req: NextApiRequest; res: NextApiResponse; out: Output } {
  const out: Output = { statusCode: 0, body: null, headers: {} };
  const headers: Record<string, string> = {
    host: 'www.apocky.com',
    origin: options.origin ?? 'https://www.apocky.com',
    'x-forwarded-proto': 'https',
    'content-type': options.contentType ?? 'application/json',
  };
  if (options.member !== false) {
    headers['x-apocky-test-admin-email'] = 'member@example.test';
  }
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

async function main(): Promise<void> {
  process.env.LAZARUS_TEST_AUTH_BYPASS = '1';
  process.env.APOCRYPHA_TUNNEL_HOST = 'apocrypha.apocky.com';
  process.env.CF_ACCESS_CLIENT_ID = 'test-client-id';
  process.env.CF_ACCESS_CLIENT_SECRET = 'test-client-secret';

  const conversationId = randomUUID();
  const requestId = randomUUID();
  const baseBody = {
    text: 'Hello, Apocrypha.',
    conversation_id: conversationId,
    request_id: requestId,
  };

  const principalA = publicMemberPrincipalRef('member-a');
  const principalARepeat = publicMemberPrincipalRef('member-a');
  const principalB = publicMemberPrincipalRef('member-b');
  equal(principalA, principalARepeat, 'member principal is deterministic');
  assert(principalA !== principalB, 'different members receive different principals');
  assert(!principalA.includes('member-a'), 'raw member identity is not embedded');
  assert(
    scopeConversationId(principalA, conversationId)
      !== scopeConversationId(principalB, conversationId),
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
    return new Response(JSON.stringify({
      schema: 'apocrypha.v2.turn-response.v1',
      text: 'A bounded native response.',
      request_id: body.request_id,
      conversation_ref: expectedConversationRef(
        String(body.conversation_id),
        String(body.source_ref),
      ),
      transition_id: `transition-${upstreamCalls}`,
      state_root: `state-${upstreamCalls}`,
      expression_mode: 'bootstrap_shallow',
      effect_authority: 'deny_all_O10_membrane',
      external_inference: false,
      outcome: 'committed',
    }), { status: 200, headers: { 'Content-Type': 'application/json' } });
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

  const invalidConversation = reqRes('POST', {
    body: { ...baseBody, conversation_id: 'not-a-uuid' },
  });
  await chatHandler(invalidConversation.req, invalidConversation.res);
  equal(invalidConversation.out.statusCode, 400, 'invalid conversation UUID is rejected');

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
  equal(upstreamCalls, 0, 'denied inputs never reach the private body');

  const success = reqRes('POST', { body: baseBody });
  await chatHandler(success.req, success.res);
  equal(success.out.statusCode, 200, 'verified member turn succeeds');
  assertPrivate(success.out);
  const successBody = success.out.body as Record<string, unknown>;
  equal(successBody.text, 'A bounded native response.', 'sanitized native text is returned');
  equal(successBody.conversation_id, conversationId, 'client conversation UUID is echoed');
  equal(successBody.request_id, requestId, 'client request UUID is echoed');
  equal(successBody.memory_scope, 'ephemeral', 'public conversation memory is off');
  equal(successBody.training_consent, false, 'public training consent is off');
  equal(
    successBody.conversation_history,
    'not_retained_by_public_interface',
    'public UI does not claim cross-session history',
  );
  equal(successBody.effect_authority, 'deny_all_O10_membrane', 'turn effects remain denied');
  assert(!JSON.stringify(successBody).includes('principal:apocky-member:'), 'principal stays server-side');
  assert(!JSON.stringify(successBody).includes('member@example.test'), 'member email stays server-side');

  const firstCall = observed[0];
  assert(firstCall !== undefined, 'upstream call observed');
  equal(firstCall.url, 'https://apocrypha.apocky.com/v2/turn', 'canonical private V2 route used');
  equal(firstCall.headers.get('cf-access-client-id'), 'test-client-id', 'service credential stays on server hop');
  equal(firstCall.body.privacy_class, 'restricted', 'member message is restricted');
  equal(firstCall.body.memory_scope, 'ephemeral', 'backend archive memory is disabled');
  equal(firstCall.body.request_id, firstCall.body.idempotency_key, 'request identity binds replay');
  assert(firstCall.body.request_id !== requestId, 'raw client request ID is not sent upstream');
  assert(firstCall.body.conversation_id !== conversationId, 'raw client conversation ID is not sent upstream');
  assert(
    String(firstCall.body.authority_ref).includes('principal:apocky-member:'),
    'authority is member-principal scoped',
  );
  assert(
    String(firstCall.body.consent_ref).startsWith('consent:single-public-turn:'),
    'send action grants one-turn consent only',
  );
  assert(!('training_consent' in firstCall.body), 'no training opt-in crosses the boundary');
  assert(!('tool_call' in firstCall.body), 'public route grants no tool request');

  globalThis.fetch = async (input, init) => {
    upstreamCalls += 1;
    const body = JSON.parse(String(init?.body ?? '{}')) as Record<string, unknown>;
    observed.push({ body, headers: new Headers(init?.headers), url: String(input) });
    return new Response(JSON.stringify({
      schema: 'apocrypha.v2.turn-response.v1',
      text: 'Must not escape.',
      request_id: body.request_id,
      conversation_ref: 'wrong-conversation',
      transition_id: 'transition-invalid',
      state_root: 'state-invalid',
      expression_mode: 'bootstrap_shallow',
      effect_authority: 'deny_all_O10_membrane',
      external_inference: false,
      outcome: 'committed',
    }), { status: 200, headers: { 'Content-Type': 'application/json' } });
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
    return new Response(JSON.stringify({
      schema: 'apocrypha.v2.turn-response.v1',
      text: 'Rate-limited fixture.',
      request_id: body.request_id,
      conversation_ref: expectedConversationRef(String(body.conversation_id), String(body.source_ref)),
      transition_id: `transition-rate-${upstreamCalls}`,
      state_root: `state-rate-${upstreamCalls}`,
      expression_mode: 'bootstrap_shallow',
      effect_authority: 'deny_all_O10_membrane',
      external_inference: false,
      outcome: 'committed',
    }), { status: 200, headers: { 'Content-Type': 'application/json' } });
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

  const page = readFileSync(resolve(process.cwd(), 'pages/apocrypha.tsx'), 'utf8');
  const component = readFileSync(
    resolve(process.cwd(), 'components/apocrypha/PublicChat.tsx'),
    'utf8',
  );
  const css = readFileSync(
    resolve(process.cwd(), 'styles/PublicApocrypha.module.css'),
    'utf8',
  );
  assert(page.includes('<PublicChat />'), '/apocrypha renders the native public chat');
  assert(!page.includes('ClearingRoom'), '/apocrypha is no longer the social room');
  assert(component.includes("authFetch('/api/apocrypha/chat'"), 'browser calls the member BFF');
  assert(component.includes('training_consent === false'), 'browser verifies no training consent');
  assert(component.includes("memory_scope === 'ephemeral'"), 'browser verifies ephemeral memory');
  assert(component.includes('Retry same turn'), 'bounded retry reuses one turn identity');
  assert(component.includes('No message is sent until the session is verified.'), 'signed-out boundary is explicit');
  assert(component.includes('This is not the social room.'), 'Apocrypha and Clearing roles are distinct');
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
