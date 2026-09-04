import assert from 'node:assert/strict';
import { createHash, webcrypto } from 'node:crypto';
import type { NextApiRequest, NextApiResponse } from 'next';

import type { OwnerBrainHistoryGetProjection } from '@/lib/apocv4/runtime-proxy';
import {
  MINI_BRAIN_SYNC_REQUEST_SCHEMA,
  canonicalJson,
  syncSigningPayload,
  type MiniBrainSyncPayload,
  type MiniBrainSyncRequest,
  type MiniBrainSyncUnsignedRequest,
} from '@/lib/brain/mobile-contracts';
import {
  issueMiniBrainDeviceCapability,
  resetMiniBrainRelayStateForTests,
  verifyMiniBrainSyncRequest,
} from '@/lib/brain/mobile-relay';
import {
  deterministicMiniBrainReply,
  normalizeMiniBrainRemoteMessages,
  type MiniBrainMemory,
} from '@/lib/brain/mini-brain';
import deviceHandler from '@/pages/api/brain/mobile/device';
import { createMiniBrainSyncHandler } from '@/pages/api/brain/mobile/sync';

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
  readonly cursor?: string | null;
  readonly requestId?: string;
  readonly empty?: boolean;
} = {}): OwnerBrainHistoryGetProjection {
  const sessionId = input.sessionId ?? '11111111-1111-4111-8111-111111111111';
  const requestId = input.requestId ?? '22222222-2222-4222-8222-222222222222';
  return {
    schema_version: 'apocky.apocv4-runtime-proxy.v1',
    kind: 'owner_brain_history_get',
    observed: {
      evidence_lane: 'observed_runtime_http_and_principal_bound_history',
      request_contract: 'conversation_id',
      page_count: 1,
      receipt: {
        observed_at: new Date().toISOString(), latency_ms: 4, upstream_status: 200,
        auth_mode: 'STRICT_REGISTRY', auth_registry_ref: '1'.repeat(64), binding_ref: '2'.repeat(64),
        principal_ref: '3'.repeat(64), privacy_partition_ref: '4'.repeat(64), effect_scope_ref: null,
      },
    },
    session: {
      schema_version: 'apocky.owner-brain.history-session.v1',
      session_id: sessionId,
      title: 'Current worldline',
      created_at: '2026-09-03T00:00:00.000Z', updated_at: '2026-09-03T00:01:00.000Z',
      event_count: input.empty ? 0 : 2,
      events_truncated: false,
      tip_digest: input.cursor === undefined ? 'a'.repeat(64) : input.cursor,
      messages: input.empty ? [] : [{ role: 'user', content: 'Continue this worldline.', request_id: requestId, recorded_at: '2026-09-03T00:01:00.000Z', event_digest: '5'.repeat(64) }],
      failed_turn_count: 0,
      history_surface: 'g12_chat_history',
    },
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
    const mapped = normalizeMiniBrainRemoteMessages([{
      role: 'assistant',
      content: 'Verified desktop response.',
      request_id: '22222222-2222-4222-8222-222222222222',
      recorded_at: '2026-09-03T00:01:00.000Z',
      event_digest: '4'.repeat(64),
      receipt: {
        context: {
          frame_digest: '5'.repeat(64),
          provenance_spine_digest: '6'.repeat(64),
        },
      },
    }]);
    assert.deepEqual(mapped.map(message => ({
      id: message.id,
      request_id: message.request_id,
      origin: message.origin,
      provenance_digests: message.provenance_digests,
    })), [{
      id: '4'.repeat(64),
      request_id: '22222222-2222-4222-8222-222222222222',
      origin: 'desktop',
      provenance_digests: ['5'.repeat(64), '6'.repeat(64)],
    }], 'the same bounded remote projection maps on desktop, Android Chrome, and iPhone WebKit');

    const bound = await device();
    resetMiniBrainRelayStateForTests();
    const first = await signedRequest({ device: bound, sequence: 1 });
    const verified = await verifyMiniBrainSyncRequest({ body: first, userId: 'test-admin' });
    assert.equal(verified.replayKind, 'new_sequence');
    const retry = await verifyMiniBrainSyncRequest({ body: first, userId: 'test-admin' });
    assert.equal(retry.replayKind, 'identical_retry', 'exact retry is safe and remains idempotent');
    const tampered = { ...first, base_cursor: 'f'.repeat(64) };
    await assert.rejects(() => verifyMiniBrainSyncRequest({ body: tampered, userId: 'test-admin' }), /BRAIN_DEVICE_SIGNATURE_INVALID/);
    await assert.rejects(() => verifyMiniBrainSyncRequest({ body: first, userId: 'different-owner' }), /BRAIN_DEVICE_TOKEN_INVALID/);

    resetMiniBrainRelayStateForTests();
    for (let sequence = 1; sequence <= 30; sequence += 1) {
      const allowed = await signedRequest({ device: bound, sequence });
      await verifyMiniBrainSyncRequest({ body: allowed, userId: 'test-admin' });
    }
    const rateLimited = await signedRequest({ device: bound, sequence: 31 });
    await assert.rejects(
      () => verifyMiniBrainSyncRequest({ body: rateLimited, userId: 'test-admin' }),
      /BRAIN_SYNC_RATE_LIMITED/,
    );

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

    resetMiniBrainRelayStateForTests();
    const disabledRequest = await signedRequest({ device: bound, sequence: 1 });
    const disabled = request(disabledRequest);
    await createMiniBrainSyncHandler({
      configured: () => false,
      getSession: async () => { throw new Error('must not call'); },
      sendTurn: async () => { throw new Error('must not call'); },
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
    })(current.req, current.res);
    assert.equal(current.out.statusCode, 200);
    assert.equal(current.out.body.status, 'current');
    assert.deepEqual(current.out.body.messages, [], 'matching cursor does not duplicate the local tail');

    resetMiniBrainRelayStateForTests();
    const emptyRequest = await signedRequest({ device: bound, sequence: 1 });
    const empty = request(emptyRequest);
    await createMiniBrainSyncHandler({
      configured: () => true,
      getSession: async () => projection({ cursor: null, empty: true }),
      sendTurn: async () => { throw new Error('pull must not send'); },
    })(empty.req, empty.res);
    assert.equal(empty.out.statusCode, 200);
    assert.equal(empty.out.body.status, 'empty', 'an exact empty G12 history page remains distinct from a failed load');
    assert.equal((empty.out.body.provenance as Record<string, unknown>).principal_ref, '3'.repeat(64));

    resetMiniBrainRelayStateForTests();
    const tombstoneRequest = await signedRequest({ device: bound, sequence: 1, baseCursor: 'a'.repeat(64) });
    const tombstone = request(tombstoneRequest);
    await createMiniBrainSyncHandler({
      configured: () => true,
      getSession: async () => {
        throw new (await import('@/lib/apocv4/runtime-proxy')).RuntimeProxyError('session_not_found', 404, 404);
      },
      sendTurn: async () => { throw new Error('pull must not send'); },
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
    })(conflict.req, conflict.res);
    assert.equal(conflict.out.statusCode, 409);
    assert.equal(conflict.out.body.code, 'BRAIN_SYNC_CONFLICT');
    assert.equal(conflictSends, 0, 'conflicting history never appends silently');

    resetMiniBrainRelayStateForTests();
    const appendRequestId = webcrypto.randomUUID();
    const appendRequest = await signedRequest({ device: bound, sequence: 1, operation: 'append', requestId: appendRequestId });
    const append = request(appendRequest);
    let reads = 0;
    let sends = 0;
    await createMiniBrainSyncHandler({
      configured: () => true,
      getSession: async () => {
        reads += 1;
        if (reads === 1) {
          return projection({ cursor: null, empty: true });
        }
        return projection({ requestId: appendRequestId, cursor: 'c'.repeat(64) });
      },
      sendTurn: async () => { sends += 1; return {} as never; },
    })(append.req, append.res);
    assert.equal(append.out.statusCode, 200);
    assert.equal(append.out.body.status, 'appended');
    assert.equal(sends, 1);
    assert.equal((append.out.body.controls as Record<string, unknown>).partition, 'server_derived_owner');

    resetMiniBrainRelayStateForTests();
    const replayRequest = await signedRequest({ device: bound, sequence: 1, operation: 'append', requestId: appendRequestId, baseCursor: '0'.repeat(64) });
    const replayResult = request(replayRequest);
    let replaySends = 0;
    await createMiniBrainSyncHandler({
      configured: () => true,
      getSession: async () => projection({ requestId: appendRequestId, cursor: 'c'.repeat(64) }),
      sendTurn: async () => { replaySends += 1; return {} as never; },
    })(replayResult.req, replayResult.res);
    assert.equal(replayResult.out.statusCode, 200);
    assert.equal(replayResult.out.body.status, 'idempotent_replay');
    assert.equal(replaySends, 0, 'known request id never invokes the model twice');

    console.log('mobile-sync.test : OK · owner/device signature + replay/rate + cursor/tombstone + idempotent append + local boundary');
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
