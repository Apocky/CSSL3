import assert from 'node:assert/strict';
import { createHash, webcrypto } from 'node:crypto';
import type { NextApiRequest, NextApiResponse } from 'next';

import type { RuntimeChatProjection, RuntimeSessionGetProjection } from '@/lib/apocv4/runtime-proxy';
import {
  MINI_BRAIN_SYNC_REQUEST_SCHEMA,
  canonicalJson,
  syncSigningPayload,
  validateMiniBrainSyncResponse,
  type MiniBrainSyncPayload,
  type MiniBrainSyncRequest,
  type MiniBrainSyncResponse,
  type MiniBrainSyncUnsignedRequest,
} from '@/lib/brain/mobile-contracts';
import {
  issueMiniBrainDeviceCapability,
  resetMiniBrainRelayStateForTests,
  verifyMiniBrainSyncRequest,
} from '@/lib/brain/mobile-relay';
import type { MiniBrainRelayStateStore } from '@/lib/brain/mobile-relay-state';
import {
  MiniBrainVault,
  deterministicMiniBrainReply,
  type MiniBrainMemory,
  type MiniBrainState,
} from '@/lib/brain/mini-brain';
import { ownerBrainRuntimeRequestId } from '@/lib/brain/runtime-provider';
import deviceHandler from '@/pages/api/brain/mobile/device';
import { createMiniBrainSyncHandler } from '@/pages/api/brain/mobile/sync';
import unlockHandler from '@/pages/api/brain/mobile/unlock';

interface Output {
  statusCode: number;
  body: Record<string, unknown>;
  headers: Record<string, string>;
}

function request(body: unknown, origin = 'http://localhost:3000'): { req: NextApiRequest; res: NextApiResponse; out: Output } {
  const out: Output = { statusCode: 200, body: {}, headers: {} };
  const req = {
    method: 'POST',
    headers: {
      host: 'localhost:3000',
      origin,
      'content-type': 'application/json',
      'x-apocky-test-admin-email': 'owner@example.com',
    },
    query: {},
    body,
  } as unknown as NextApiRequest;
  const res = {
    setHeader(name: string, value: string | number | readonly string[]) {
      out.headers[name.toLowerCase()] = Array.isArray(value) ? value.join(', ') : String(value);
      return this;
    },
    status(statusCode: number) { out.statusCode = statusCode; return this; },
    json(value: Record<string, unknown>) { out.body = value; return this; },
    end() { return this; },
  } as unknown as NextApiResponse;
  return { req, res, out };
}

function hexDigest(value: string): string {
  return createHash('sha256').update(value, 'utf8').digest('hex');
}

function testRelayState(): MiniBrainRelayStateStore {
  return {
    async admit(input) {
      return {
        replayKind: 'new_sequence',
        acceptedSequence: input.sequence,
        rateCount: 1,
        rateLimit: 30,
        rateResetsAt: new Date(Date.now() + 60_000).toISOString(),
        stateExpiresAt: new Date(Date.now() + 35 * 24 * 60 * 60_000).toISOString(),
      };
    },
    async cleanup() {
      return { sequenceRowsDeleted: 0, requestRowsDeleted: 0, deviceRowsDeleted: 0, rateRowsDeleted: 0 };
    },
  };
}

async function device() {
  const keyPair = await webcrypto.subtle.generateKey(
    { name: 'ECDSA', namedCurve: 'P-256' }, false, ['sign', 'verify'],
  ) as CryptoKeyPair;
  const publicKeyJwk = await webcrypto.subtle.exportKey('jwk', keyPair.publicKey);
  const deviceId = webcrypto.randomUUID();
  const capability = issueMiniBrainDeviceCapability({
    userId: 'test-admin', deviceId, publicKeyJwk,
  });
  return { keyPair, publicKeyJwk, deviceId, capability };
}

async function signedRequest(input: {
  readonly device: Awaited<ReturnType<typeof device>>;
  readonly sequence: number;
  readonly operation?: 'pull' | 'append';
  readonly sessionId?: string;
  readonly requestId?: string;
  readonly baseCursor?: string | null;
  readonly payload?: MiniBrainSyncPayload | null;
}): Promise<MiniBrainSyncRequest> {
  const operation = input.operation ?? 'pull';
  const payload = operation === 'append' ? input.payload ?? { text: 'Continue this worldline.' } : null;
  const unsigned: MiniBrainSyncUnsignedRequest = {
    schema_version: MINI_BRAIN_SYNC_REQUEST_SCHEMA,
    device_id: input.device.deviceId,
    sequence: input.sequence,
    issued_at: new Date().toISOString(),
    operation,
    session_id: input.sessionId ?? '11111111-1111-4111-8111-111111111111',
    request_id: input.requestId ?? webcrypto.randomUUID(),
    base_cursor: input.baseCursor ?? null,
    payload,
    payload_digest: hexDigest(canonicalJson(payload)),
  };
  const signature = await webcrypto.subtle.sign(
    { name: 'ECDSA', hash: 'SHA-256' },
    input.device.keyPair.privateKey,
    Buffer.from(syncSigningPayload(unsigned), 'utf8'),
  );
  return {
    ...unsigned,
    device_token: input.device.capability.device_token,
    signature: Buffer.from(signature).toString('base64url'),
  };
}

function projection(input: {
  readonly sessionId?: string;
  readonly cursor?: string;
  readonly requestId?: string;
  readonly turnStates?: readonly Record<string, unknown>[];
  readonly turnStatesTruncated?: boolean;
} = {}): RuntimeSessionGetProjection {
  const sessionId = input.sessionId ?? '11111111-1111-4111-8111-111111111111';
  const requestId = input.requestId ?? '22222222-2222-4222-8222-222222222222';
  return {
    schema_version: 'apocky.apocv4-runtime-proxy.v1',
    kind: 'session_get',
    observed: {
      evidence_lane: 'observed_runtime_http_and_principal_bound_session',
      request_contract: 'session_id',
      receipt: {
        observed_at: new Date().toISOString(), latency_ms: 4, upstream_status: 200,
        auth_mode: 'STRICT_REGISTRY', auth_registry_ref: '1'.repeat(64), binding_ref: '2'.repeat(64),
        principal_ref: '3'.repeat(64), privacy_partition_ref: '4'.repeat(64), effect_scope_ref: null,
      },
    },
    session: {
      schema_version: 'apocv4.workspace-session-snapshot.v1',
      session_id: sessionId,
      title: 'Current worldline',
      created_at: '2026-09-03T00:00:00.000Z', updated_at: '2026-09-03T00:01:00.000Z',
      event_count: 2, events_truncated: false, tip_digest: input.cursor ?? 'a'.repeat(64),
      messages: [{ role: 'user', content: 'Continue this worldline.', request_id: requestId, recorded_at: '2026-09-03T00:01:00.000Z', event_digest: '5'.repeat(64) }],
      turn_states: [...(input.turnStates ?? [])], jobs: [], artifacts: [], code_requests: [], proposals: [], effects: [],
      surface_truncation: input.turnStatesTruncated ? { turn_states: { truncated: true } } : {}, world: {}, workspace: {},
    },
  };
}

function interruptedTurnState(requestId: string): Record<string, unknown> {
  return {
    request_id: requestId,
    state: 'FAILED',
    recorded_at: '2026-09-03T00:01:01.000Z',
    user_event_digest: 'a'.repeat(64),
    terminal_event_digest: 'b'.repeat(64),
    error_class: 'InterruptedChatAttempt',
    error_digest: 'c'.repeat(64),
    failure_code: 'interrupted_on_restart',
  };
}

function chatProjection(requestId: string): RuntimeChatProjection {
  return {
    schema_version: 'apocky.apocv4-runtime-proxy.v1',
    kind: 'chat',
    observed: {
      evidence_lane: 'observed_runtime_http_and_transport',
      receipt: {
        observed_at: new Date().toISOString(), latency_ms: 4, upstream_status: 200,
        auth_mode: 'STRICT_REGISTRY', auth_registry_ref: '1'.repeat(64), binding_ref: '2'.repeat(64),
        principal_ref: '3'.repeat(64), privacy_partition_ref: '4'.repeat(64), effect_scope_ref: null,
      },
      runtime: { schema_version: 'apocv4.chat-response.v2', request_id: requestId, outcome: 'completed' },
    },
    model_reported: {
      evidence_lane: 'model_reported_not_observed_fact',
      text: 'Retained answer.',
      model_id: 'test-model', response_id: 'test-response',
      prompt_digest: '6'.repeat(64), response_digest: '7'.repeat(64), serving_profile_digest: '8'.repeat(64),
    },
    authority: {
      effect_authority: 'NONE', tool_authority: 'READ_ONLY_CONTEXT', memory_scope: 'owner_partitioned_retrieval',
      conversation_history: 'durable_principal_bound', training_consent: false,
    },
    identity: {
      schema_version: 'apocv4.identity.v1', system_id: 'apocrypha',
      architecture: 'governed_hybrid_digital_intelligence', compiler_version: 'test',
      identity_digest: '9'.repeat(64), learned_model_role: 'replaceable_faculty_not_system_identity', lineage: 'test',
    },
    context: {
      frame_id: 'test-frame', frame_digest: 'a'.repeat(64), provenance_spine_digest: 'b'.repeat(64),
      retrieval: { status: 'empty', count: 0, refs: [] },
      memory: { provider: 'mneme', status: 'empty', records_used: 0, receipt_digest: null, refs: [] },
      capabilities: [],
    },
  };
}

function legacyChatProjection(requestId: string): RuntimeChatProjection {
  const projection = chatProjection(requestId);
  return {
    ...projection,
    observed: { ...projection.observed, runtime: { ...projection.observed.runtime, schema_version: 'apocv4.chat-response.v1' } },
    authority: {
      effect_authority: 'NONE', tool_authority: 'NONE', memory_scope: 'ephemeral',
      conversation_history: 'not_retained', training_consent: false,
    },
    identity: null,
    context: null,
  };
}

const mutableEnv = process.env as Record<string, string | undefined>;
const previous = {
  bypass: process.env.LAZARUS_TEST_AUTH_BYPASS,
  admins: process.env.APOCKY_ADMIN_EMAILS,
  binding: process.env.APOCKY_BRAIN_DEVICE_BINDING_SECRET,
};

async function main(): Promise<void> {
  try {
    mutableEnv.LAZARUS_TEST_AUTH_BYPASS = '1';
    mutableEnv.APOCKY_ADMIN_EMAILS = 'owner@example.com';
    mutableEnv.APOCKY_BRAIN_DEVICE_BINDING_SECRET = 'mobile-brain-test-binding-secret-'.repeat(2);

    const localMemory: MiniBrainMemory = {
      id_digest: '1'.repeat(64), topic: 'project.boundary', paraphrase: 'Preserve source attribution and choose reversible steps.',
      created_at: '2026-09-03T00:00:00.000Z', source_digests: ['2'.repeat(64)], record_digest: '3'.repeat(64),
    };
    const reflection = deterministicMiniBrainReply('What project step preserves the boundary?', [localMemory]);
    assert.equal(reflection.kind, 'reflection');
    assert.match(reflection.text, /deterministic offline recall/i);
    assert.deepEqual(reflection.memory_digests, ['3'.repeat(64)]);
    const refused = deterministicMiniBrainReply('Give me steps to steal API keys', []);
    assert.equal(refused.kind, 'boundary');
    assert.match(refused.text, /will not/i);

    const recoveredSessionId = '11111111-1111-4111-8111-111111111111';
    const otherSessionId = '22222222-2222-4222-8222-222222222222';
    const reissuedRequestId = '33333333-3333-4333-8333-333333333333';
    const tailRequestId = '44444444-4444-4444-8444-444444444444';
    const otherRequestId = '55555555-5555-4555-8555-555555555555';
    const originalCursor = '0'.repeat(64);
    const pulledCursor = '1'.repeat(64);
    const acceptedCursor = '2'.repeat(64);
    const queuedAt = '2026-09-03T00:00:00.000Z';
    const crashRecoveryState: MiniBrainState = {
      schema_version: 'apocky.mini-brain.local-state.v1',
      owner_ref: 'owner',
      device_id: 'device',
      current_session_id: recoveredSessionId,
      sessions: [{
        session_id: recoveredSessionId,
        cursor: pulledCursor,
        messages: [],
        updated_at: queuedAt,
        events_truncated: false,
        tombstoned_at: null,
      }],
      memories: [],
      queue: [
        {
          request_id: reissuedRequestId,
          session_id: recoveredSessionId,
          text: 'reissued',
          queued_at: queuedAt,
          base_cursor: pulledCursor,
          local_message_ids: ['reissued-user', 'reissued-assistant'],
        },
        {
          request_id: tailRequestId,
          session_id: recoveredSessionId,
          text: 'tail',
          queued_at: queuedAt,
          base_cursor: originalCursor,
          local_message_ids: ['tail-user', 'tail-assistant'],
        },
        {
          request_id: otherRequestId,
          session_id: otherSessionId,
          text: 'other session',
          queued_at: queuedAt,
          base_cursor: originalCursor,
          local_message_ids: ['other-user', 'other-assistant'],
        },
      ],
      updated_at: queuedAt,
    };
    const acceptedResponse: MiniBrainSyncResponse = {
      schema_version: 'apocky.mini-brain.sync-response.v1',
      status: 'appended',
      session_id: recoveredSessionId,
      request_id: reissuedRequestId,
      acknowledged_request_ids: [reissuedRequestId],
      cursor: acceptedCursor,
      messages: [],
      tombstones: [],
      events_truncated: false,
      provenance: {
        transport: 'owner_bound_apocv4_runtime',
        privacy_partition_ref: null,
        principal_ref: null,
        binding_ref: null,
      },
      controls: {
        owner_session: 'verified',
        device_signature: 'verified',
        replay: 'bounded_sequence_and_idempotent_request',
        rate_limit: 'owner_durable_window',
        partition: 'server_derived_owner',
      },
      served_by: 'test',
      ts: '2026-09-03T00:00:01.000Z',
    };
    const queueVault = Object.create(MiniBrainVault.prototype) as MiniBrainVault;
    Object.defineProperty(queueVault, 'save', {
      value: async (state: MiniBrainState): Promise<MiniBrainState> => state,
    });
    const recovered = await queueVault.applySync(crashRecoveryState, acceptedResponse);
    assert.equal(recovered.queue.find(turn => turn.request_id === tailRequestId)?.base_cursor, acceptedCursor,
      'a persisted same-session tail must survive a crash after a reissued prefix ACK');
    assert.equal(recovered.queue.find(turn => turn.request_id === otherRequestId)?.base_cursor, originalCursor,
      'another session must not be rebased by this worldline');

    const outOfOrder = await queueVault.applySync(crashRecoveryState, {
      ...acceptedResponse,
      request_id: tailRequestId,
      acknowledged_request_ids: [tailRequestId],
    });
    assert.equal(outOfOrder.queue.find(turn => turn.request_id === reissuedRequestId)?.base_cursor, pulledCursor,
      'a non-prefix acknowledgement must not advance an earlier queued turn');

    const bound = await device();
    resetMiniBrainRelayStateForTests();
    const first = await signedRequest({ device: bound, sequence: 1 });
    const verified = await verifyMiniBrainSyncRequest({ body: first, userId: 'test-admin' });
    assert.equal(verified.request.request_id, first.request_id);
    await verifyMiniBrainSyncRequest({ body: first, userId: 'test-admin' });
    const tampered = { ...first, base_cursor: 'f'.repeat(64) };
    await assert.rejects(() => verifyMiniBrainSyncRequest({ body: tampered, userId: 'test-admin' }), /BRAIN_DEVICE_SIGNATURE_INVALID/);
    await assert.rejects(() => verifyMiniBrainSyncRequest({ body: first, userId: 'different-owner' }), /BRAIN_DEVICE_TOKEN_INVALID/);

    const crossOrigin = request({});
    crossOrigin.req.headers.origin = 'https://attacker.invalid';
    await deviceHandler(crossOrigin.req, crossOrigin.res);
    assert.equal(crossOrigin.out.statusCode, 403);
    assert.equal(crossOrigin.out.body.code, 'BRAIN_ORIGIN_DENIED');

    const registration = request({ device_id: bound.deviceId, public_key_jwk: bound.publicKeyJwk });
    await deviceHandler(registration.req, registration.res);
    assert.equal(registration.out.statusCode, 200);
    assert.equal(registration.out.body.status, 'bound');
    assert.match(registration.out.headers['cache-control'] ?? '', /private.*no-store/);

    const lockGeneration = '44444444-4444-4444-8444-444444444444';
    const testAuthAttempt = 'test-auth-attempt-'.repeat(8);
    const ownerUnlock = request({ lock_generation: lockGeneration, auth_attempt: testAuthAttempt });
    await unlockHandler(ownerUnlock.req, ownerUnlock.res);
    assert.equal(ownerUnlock.out.statusCode, 200);
    assert.equal(ownerUnlock.out.body.schema_version, 'apocky.mini-brain.owner-rebind.v1');
    assert.equal(ownerUnlock.out.body.status, 'rebind_authorized');
    assert.equal(ownerUnlock.out.body.owner_ref, hexDigest('apocky.mini-brain.owner.v1\u0000test-admin'));
    assert.equal(ownerUnlock.out.body.lock_generation, lockGeneration);
    const memberUnlock = request({ lock_generation: lockGeneration, auth_attempt: testAuthAttempt });
    memberUnlock.req.headers['x-apocky-test-admin-email'] = 'member@example.com';
    await unlockHandler(memberUnlock.req, memberUnlock.res);
    assert.equal(memberUnlock.out.statusCode, 403);
    assert.equal(memberUnlock.out.body.code, 'BRAIN_OWNER_REQUIRED');

    resetMiniBrainRelayStateForTests();
    const disabledRequest = await signedRequest({ device: bound, sequence: 1 });
    const disabled = request(disabledRequest);
    await createMiniBrainSyncHandler({
      configured: () => false,
      getSession: async () => { throw new Error('must not call'); },
      sendTurn: async () => { throw new Error('must not call'); },
      relayState: testRelayState,
    })(disabled.req, disabled.res);
    assert.equal(disabled.out.statusCode, 503);
    assert.equal(disabled.out.body.code, 'BRAIN_LOCAL_PROVIDER_DISABLED');

    resetMiniBrainRelayStateForTests();
    const currentRequest = await signedRequest({ device: bound, sequence: 1, baseCursor: 'a'.repeat(64) });
    const current = request(currentRequest);
    await createMiniBrainSyncHandler({
      configured: () => true,
      getSession: async () => projection(),
      sendTurn: async () => { throw new Error('pull must not send'); },
      relayState: testRelayState,
    })(current.req, current.res);
    assert.equal(current.out.statusCode, 200);
    assert.equal(current.out.body.status, 'current');
    assert.deepEqual(current.out.body.messages, [], 'matching cursor does not duplicate the local tail');

    resetMiniBrainRelayStateForTests();
    const tombstoneRequest = await signedRequest({ device: bound, sequence: 1, baseCursor: 'a'.repeat(64) });
    const tombstone = request(tombstoneRequest);
    await createMiniBrainSyncHandler({
      configured: () => true,
      getSession: async () => {
        throw new (await import('@/lib/apocv4/runtime-proxy')).RuntimeProxyError('session_not_found', 404, 404);
      },
      sendTurn: async () => { throw new Error('pull must not send'); },
      relayState: testRelayState,
    })(tombstone.req, tombstone.res);
    assert.equal(tombstone.out.statusCode, 200);
    assert.equal(tombstone.out.body.status, 'tombstoned');
    assert.deepEqual(
      (tombstone.out.body.tombstones as Array<Record<string, unknown>>).map(item => item.reason),
      ['REMOTE_SESSION_ABSENT'],
    );

    resetMiniBrainRelayStateForTests();
    const conflictRequest = await signedRequest({ device: bound, sequence: 1, operation: 'append', baseCursor: 'b'.repeat(64) });
    const conflict = request(conflictRequest);
    let conflictSends = 0;
    await createMiniBrainSyncHandler({
      configured: () => true,
      getSession: async () => projection(),
      sendTurn: async () => { conflictSends += 1; return {} as never; },
      relayState: testRelayState,
    })(conflict.req, conflict.res);
    assert.equal(conflict.out.statusCode, 409);
    assert.equal(conflict.out.body.code, 'BRAIN_SYNC_CONFLICT');
    assert.equal(conflictSends, 0, 'conflicting history never appends silently');

    resetMiniBrainRelayStateForTests();
    const appendRequestId = webcrypto.randomUUID();
    const scopedAppendRequestId = ownerBrainRuntimeRequestId('test-admin', appendRequestId);
    const appendRequest = await signedRequest({ device: bound, sequence: 1, operation: 'append', requestId: appendRequestId });
    const append = request(appendRequest);
    let committed = false;
    let sends = 0;
    const appendHandler = createMiniBrainSyncHandler({
      configured: () => true,
      getSession: async () => {
        if (!committed) {
          const error = new (await import('@/lib/apocv4/runtime-proxy')).RuntimeProxyError('session_not_found', 404, 404);
          throw error;
        }
        return projection({ requestId: scopedAppendRequestId, cursor: 'c'.repeat(64) });
      },
      sendTurn: async () => { sends += 1; committed = true; return chatProjection(scopedAppendRequestId); },
      relayState: testRelayState,
    });
    await appendHandler(append.req, append.res);
    assert.equal(append.out.statusCode, 200);
    assert.equal(append.out.body.status, 'appended');
    assert.deepEqual(append.out.body.acknowledged_request_ids, [appendRequestId]);
    assert.equal(sends, 1);
    assert.equal((append.out.body.controls as Record<string, unknown>).partition, 'server_derived_owner');
    assert.equal(
      ((append.out.body.messages as Array<Record<string, unknown>>)[0] ?? {}).request_id,
      appendRequestId,
      'the relay returns the client request id while retaining the scoped runtime identity internally',
    );

    const resignedRetry = await signedRequest({
      device: bound,
      sequence: 2,
      operation: 'append',
      requestId: appendRequestId,
    });
    const replayResult = request(resignedRetry);
    await appendHandler(replayResult.req, replayResult.res);
    assert.equal(replayResult.out.statusCode, 200);
    assert.equal(replayResult.out.body.status, 'idempotent_replay');
    assert.equal(sends, 1, 'a response-lost retry recognizes the scoped receipt and never invokes the model twice');
    assert.deepEqual(replayResult.out.body.acknowledged_request_ids, [appendRequestId]);
    assert.equal(
      ((replayResult.out.body.messages as Array<Record<string, unknown>>)[0] ?? {}).request_id,
      appendRequestId,
      'a response-lost retry still acknowledges the client queue identity',
    );

    resetMiniBrainRelayStateForTests();
    const agedRequestId = webcrypto.randomUUID();
    const scopedAgedRequestId = ownerBrainRuntimeRequestId('test-admin', agedRequestId);
    const agedRequest = await signedRequest({ device: bound, sequence: 1, operation: 'append', requestId: agedRequestId, baseCursor: 'd'.repeat(64) });
    const aged = request(agedRequest);
    let agedSends = 0;
    await createMiniBrainSyncHandler({
      configured: () => true,
      getSession: async () => projection({ requestId: webcrypto.randomUUID(), cursor: 'd'.repeat(64) }),
      sendTurn: async () => { agedSends += 1; return chatProjection(scopedAgedRequestId); },
      relayState: testRelayState,
    })(aged.req, aged.res);
    assert.equal(aged.out.statusCode, 200);
    assert.deepEqual(aged.out.body.acknowledged_request_ids, [agedRequestId]);
    assert.equal(agedSends, 1, 'a projection-aged request is governed by the runtime durable request identity');
    assert.ok(
      (aged.out.body.messages as Array<Record<string, unknown>>).some(message => (
        message.request_id === agedRequestId && message.role === 'assistant' && message.content === 'Retained answer.'
      )),
      'a durable v2 receipt supplies the actual answer when the bounded session window has aged past the turn',
    );
    assert.throws(
      () => validateMiniBrainSyncResponse({
        ...aged.out.body,
        session_id: webcrypto.randomUUID(),
        acknowledged_request_ids: [webcrypto.randomUUID()],
      }, agedRequest),
      /MINI_BRAIN_SYNC_RESPONSE_INVALID/,
      'a mismatched 200 response cannot acknowledge any encrypted queue item',
    );

    const missingReadbackRequest = await signedRequest({
      device: bound, sequence: 2, operation: 'append', requestId: webcrypto.randomUUID(),
    });
    const missingReadback = request(missingReadbackRequest);
    await createMiniBrainSyncHandler({
      configured: () => true,
      getSession: async () => {
        throw new (await import('@/lib/apocv4/runtime-proxy')).RuntimeProxyError('session_not_found', 404, 404);
      },
      sendTurn: async () => chatProjection(ownerBrainRuntimeRequestId('test-admin', missingReadbackRequest.request_id)),
      relayState: testRelayState,
    })(missingReadback.req, missingReadback.res);
    assert.equal(missingReadback.out.statusCode, 502);
    assert.equal(missingReadback.out.body.code, 'BRAIN_RUNTIME_CHAT_SESSION_READBACK_MISSING');
    assert.equal(missingReadback.out.body.acknowledged_request_ids, undefined, 'no readback means the encrypted queue is never acknowledged');

    const legacyRequest = await signedRequest({
      device: bound, sequence: 3, operation: 'append', requestId: webcrypto.randomUUID(),
    });
    const legacy = request(legacyRequest);
    let legacyRead = 0;
    await createMiniBrainSyncHandler({
      configured: () => true,
      getSession: async () => {
        legacyRead += 1;
        if (legacyRead === 1) {
          throw new (await import('@/lib/apocv4/runtime-proxy')).RuntimeProxyError('session_not_found', 404, 404);
        }
        return projection({ requestId: webcrypto.randomUUID(), cursor: 'e'.repeat(64) });
      },
      sendTurn: async () => legacyChatProjection(ownerBrainRuntimeRequestId('test-admin', legacyRequest.request_id)),
      relayState: testRelayState,
    })(legacy.req, legacy.res);
    assert.equal(legacy.out.statusCode, 502);
    assert.equal(legacy.out.body.code, 'BRAIN_RUNTIME_CHAT_SESSION_RECEIPT_MISSING');
    assert.equal(legacy.out.body.acknowledged_request_ids, undefined, 'non-retained v1 answers cannot clear the queue');

    const failedRequestId = webcrypto.randomUUID();
    const scopedFailedRequestId = ownerBrainRuntimeRequestId('test-admin', failedRequestId);
    const failedRequest = await signedRequest({
      device: bound, sequence: 4, operation: 'append', requestId: failedRequestId, baseCursor: 'f'.repeat(64),
    });
    const failed = request(failedRequest);
    let failedSends = 0;
    await createMiniBrainSyncHandler({
      configured: () => true,
      getSession: async () => projection({
        requestId: webcrypto.randomUUID(), cursor: 'f'.repeat(64),
        turnStates: [interruptedTurnState(scopedFailedRequestId)],
      }),
      sendTurn: async () => { failedSends += 1; return chatProjection(scopedFailedRequestId); },
      relayState: testRelayState,
    })(failed.req, failed.res);
    assert.equal(failed.out.statusCode, 409);
    assert.equal(failed.out.body.code, 'BRAIN_SYNC_TERMINAL_FAILED');
    assert.equal(failed.out.body.request_id, failedRequestId);
    assert.equal(failed.out.body.session_id, failedRequest.session_id);
    assert.equal(failed.out.body.reissue_safe, true);
    assert.equal(failed.out.body.acknowledged_request_ids, undefined);
    assert.equal(failedSends, 0, 'a terminal same-ID attempt never invokes the model again');

    const interruptedDuringSendId = webcrypto.randomUUID();
    const scopedInterruptedDuringSendId = ownerBrainRuntimeRequestId('test-admin', interruptedDuringSendId);
    const interruptedDuringSendRequest = await signedRequest({
      device: bound, sequence: 5, operation: 'append', requestId: interruptedDuringSendId,
    });
    const interruptedDuringSend = request(interruptedDuringSendRequest);
    let interruptedRead = 0;
    await createMiniBrainSyncHandler({
      configured: () => true,
      getSession: async () => {
        interruptedRead += 1;
        if (interruptedRead === 1) {
          throw new (await import('@/lib/apocv4/runtime-proxy')).RuntimeProxyError('session_not_found', 404, 404);
        }
        return projection({
          requestId: webcrypto.randomUUID(), cursor: '1'.repeat(64),
          turnStates: [interruptedTurnState(scopedInterruptedDuringSendId)],
        });
      },
      sendTurn: async () => {
        throw new (await import('@/lib/apocv4/runtime-proxy')).RuntimeProxyError('runtime_http_error', 502, 500);
      },
      relayState: testRelayState,
    })(interruptedDuringSend.req, interruptedDuringSend.res);
    assert.equal(interruptedDuringSend.out.statusCode, 409);
    assert.equal(interruptedDuringSend.out.body.code, 'BRAIN_SYNC_TERMINAL_FAILED');
    assert.equal(interruptedDuringSend.out.body.session_id, interruptedDuringSendRequest.session_id);
    assert.equal(interruptedDuringSend.out.body.reissue_safe, true);
    assert.equal(interruptedDuringSend.out.body.acknowledged_request_ids, undefined);

    const agedFailureId = webcrypto.randomUUID();
    const agedFailureRequest = await signedRequest({
      device: bound, sequence: 6, operation: 'append', requestId: agedFailureId,
    });
    const agedFailure = request(agedFailureRequest);
    let agedFailureRead = 0;
    await createMiniBrainSyncHandler({
      configured: () => true,
      getSession: async () => {
        agedFailureRead += 1;
        if (agedFailureRead === 1) {
          throw new (await import('@/lib/apocv4/runtime-proxy')).RuntimeProxyError('session_not_found', 404, 404);
        }
        return projection({
          requestId: webcrypto.randomUUID(), cursor: '2'.repeat(64), turnStatesTruncated: true,
        });
      },
      sendTurn: async () => {
        throw new (await import('@/lib/apocv4/runtime-proxy')).RuntimeProxyError('runtime_http_error', 502, 500);
      },
      relayState: testRelayState,
    })(agedFailure.req, agedFailure.res);
    assert.equal(agedFailure.out.statusCode, 409);
    assert.equal(agedFailure.out.body.code, 'BRAIN_SYNC_OUTCOME_UNRESOLVED');
    assert.equal(agedFailure.out.body.request_id, agedFailureId);
    assert.equal(agedFailure.out.body.session_id, agedFailureRequest.session_id);
    assert.equal(agedFailure.out.body.reissue_safe, false);
    assert.equal(agedFailure.out.body.acknowledged_request_ids, undefined);

    console.log('mobile-sync.test : OK · durable admission + verified ACK + tombstone/conflict/interruption recovery boundaries');
  } finally {
    resetMiniBrainRelayStateForTests();
    for (const [key, value] of Object.entries({
      LAZARUS_TEST_AUTH_BYPASS: previous.bypass,
      APOCKY_ADMIN_EMAILS: previous.admins,
      APOCKY_BRAIN_DEVICE_BINDING_SECRET: previous.binding,
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
