import assert from 'node:assert/strict';
import { createHash, randomUUID, webcrypto } from 'node:crypto';

import {
  MINI_BRAIN_SYNC_REQUEST_SCHEMA,
  canonicalJson,
  syncSigningPayload,
  type MiniBrainSyncRequest,
  type MiniBrainSyncUnsignedRequest,
} from '@/lib/brain/mobile-contracts';
import {
  MiniBrainRelayError,
  issueMiniBrainDeviceCapability,
  resetMiniBrainRelayStateForTests,
  verifyMiniBrainSyncRequest,
  type VerifiedMiniBrainRequest,
} from '@/lib/brain/mobile-relay';
import {
  admitVerifiedMiniBrainRequest,
  createMiniBrainRelayStateStore,
  createMiniBrainRelayStateStoreForRpcClient,
  durableMiniBrainEnvelopeDigest,
  durableMiniBrainLogicalRequestDigest,
  type MiniBrainRelayAdmissionInput,
  type MiniBrainRelayStateRpcClient,
} from '@/lib/brain/mobile-relay-state';

const DAY_MS = 24 * 60 * 60 * 1_000;
const RETENTION_MS = 35 * DAY_MS;
const RATE_WINDOW_MS = 60_000;
const RATE_LIMIT = 30;

interface DeviceState {
  readonly keyThumbprint: string;
  latestSequence: number;
  expiresAt: number;
}

interface RequestState {
  readonly ownerRef: string;
  readonly deviceId: string;
  readonly requestId: string;
  readonly logicalRequestDigest: string;
  latestSequence: number;
  expiresAt: number;
}

interface SequenceState {
  readonly ownerRef: string;
  readonly deviceId: string;
  readonly sequence: number;
  readonly requestId: string;
  readonly logicalRequestDigest: string;
  readonly envelopeDigest: string;
  readonly expiresAt: number;
}

interface RateState {
  startedAt: number;
  count: number;
  expiresAt: number;
}

function fakeError(code: string) {
  return { data: null, error: { code: 'P0001', message: code, details: '', hint: '' } };
}

/** Models the single-transaction RPC, shared by independently-created stores. */
class FakeDurableRelay implements MiniBrainRelayStateRpcClient {
  now = Date.parse('2026-09-03T20:00:00.000Z');
  readonly calls: Array<{ name: string; parameters: Record<string, unknown> }> = [];
  private readonly devices = new Map<string, DeviceState>();
  private readonly requests = new Map<string, RequestState>();
  private readonly sequences = new Map<string, SequenceState>();
  private readonly rates = new Map<string, RateState>();
  private tail: Promise<void> = Promise.resolve();

  rpc(name: string, parameters: Record<string, unknown>) {
    const result = this.tail.then(() => this.handle(name, parameters));
    this.tail = result.then(() => undefined, () => undefined);
    return result;
  }

  advance(milliseconds: number): void {
    this.now += milliseconds;
  }

  private pruneOwner(ownerRef: string): void {
    for (const [key, sequence] of this.sequences) {
      if (sequence.ownerRef === ownerRef && sequence.expiresAt <= this.now) this.sequences.delete(key);
    }
    for (const [key, request] of this.requests) {
      if (
        request.ownerRef === ownerRef
        && request.expiresAt <= this.now
        && ![...this.sequences.values()].some(sequence => (
          sequence.ownerRef === request.ownerRef && sequence.requestId === request.requestId
        ))
      ) this.requests.delete(key);
    }
    for (const [key, state] of this.devices) {
      const [candidateOwner, candidateDevice] = key.split(':');
      if (
        candidateOwner === ownerRef
        && state.expiresAt <= this.now
        && ![...this.requests.values()].some(request => (
          request.ownerRef === ownerRef && request.deviceId === candidateDevice
        ))
      ) this.devices.delete(key);
    }
    const rate = this.rates.get(ownerRef);
    if (rate && rate.expiresAt <= this.now) this.rates.delete(ownerRef);
  }

  private handle(name: string, parameters: Record<string, unknown>) {
    this.calls.push({ name, parameters: { ...parameters } });
    if (name === 'cleanup_mini_brain_relay_state') {
      const before = {
        sequences: this.sequences.size,
        requests: this.requests.size,
        devices: this.devices.size,
        rates: this.rates.size,
      };
      for (const ownerRef of new Set([
        ...[...this.sequences.values()].map(row => row.ownerRef),
        ...[...this.requests.values()].map(row => row.ownerRef),
        ...[...this.devices.keys()].map(key => key.slice(0, 64)),
        ...this.rates.keys(),
      ])) this.pruneOwner(ownerRef);
      return {
        data: [{
          sequence_rows_deleted: before.sequences - this.sequences.size,
          request_rows_deleted: before.requests - this.requests.size,
          device_rows_deleted: before.devices - this.devices.size,
          rate_rows_deleted: before.rates - this.rates.size,
        }],
        error: null,
      };
    }
    if (name !== 'admit_mini_brain_relay_request') {
      return fakeError('BRAIN_RELAY_STATE_UNAVAILABLE');
    }

    const input: MiniBrainRelayAdmissionInput = {
      ownerRef: String(parameters.p_owner_ref),
      deviceId: String(parameters.p_device_id),
      keyThumbprint: String(parameters.p_key_thumbprint),
      sequence: Number(parameters.p_sequence),
      requestId: String(parameters.p_request_id),
      logicalRequestDigest: String(parameters.p_logical_digest),
      envelopeDigest: String(parameters.p_envelope_digest),
    };
    this.pruneOwner(input.ownerRef);
    const requestKey = `${input.ownerRef}:${input.requestId}`;
    const sequenceKey = `${input.ownerRef}:${input.deviceId}:${input.sequence}`;
    const deviceKey = `${input.ownerRef}:${input.deviceId}`;
    let rate = this.rates.get(input.ownerRef);
    if (!rate || rate.startedAt + RATE_WINDOW_MS <= this.now) {
      rate = { startedAt: this.now, count: 0, expiresAt: this.now + DAY_MS };
      this.rates.set(input.ownerRef, rate);
    }
    const rejected = (code: string, stateExpiresAt = this.now + RETENTION_MS) => ({
      data: [{
        outcome: code,
        accepted_sequence: input.sequence,
        rate_count: rate.count,
        rate_limit: RATE_LIMIT,
        rate_resets_at: new Date(rate.startedAt + RATE_WINDOW_MS).toISOString(),
        state_expires_at: new Date(stateExpiresAt).toISOString(),
      }],
      error: null,
    });
    if (rate.count >= RATE_LIMIT) return rejected('BRAIN_SYNC_RATE_LIMITED');
    rate.count += 1;
    rate.expiresAt = this.now + DAY_MS;

    const device = this.devices.get(deviceKey);
    if (device && device.keyThumbprint !== input.keyThumbprint) {
      return rejected('BRAIN_DEVICE_STATE_BINDING_MISMATCH', device.expiresAt);
    }

    const existingSequence = this.sequences.get(sequenceKey);
    let outcome: 'new_sequence' | 'identical_retry' = 'new_sequence';
    let exactEnvelope = false;
    if (existingSequence) {
      if (
        existingSequence.requestId !== input.requestId
        || existingSequence.logicalRequestDigest !== input.logicalRequestDigest
        || existingSequence.envelopeDigest !== input.envelopeDigest
      ) return rejected('BRAIN_SYNC_REPLAY_REJECTED', existingSequence.expiresAt);
      outcome = 'identical_retry';
      exactEnvelope = true;
    } else {
      if (device && input.sequence <= device.latestSequence) {
        return rejected('BRAIN_SYNC_REPLAY_REJECTED', device.expiresAt);
      }
      const existingRequest = this.requests.get(requestKey);
      if (existingRequest) {
        if (
          existingRequest.deviceId !== input.deviceId
          || existingRequest.logicalRequestDigest !== input.logicalRequestDigest
        ) return rejected('BRAIN_SYNC_REQUEST_ID_REUSED', existingRequest.expiresAt);
        outcome = 'identical_retry';
      }
    }

    let expiry = existingSequence?.expiresAt ?? this.now + RETENTION_MS;
    if (!exactEnvelope) {
      expiry = this.now + RETENTION_MS;
      this.devices.set(deviceKey, {
        keyThumbprint: input.keyThumbprint,
        latestSequence: input.sequence,
        expiresAt: expiry,
      });
      const existingRequest = this.requests.get(requestKey);
      if (existingRequest) {
        existingRequest.latestSequence = input.sequence;
        existingRequest.expiresAt = expiry;
      } else {
        this.requests.set(requestKey, {
          ownerRef: input.ownerRef,
          deviceId: input.deviceId,
          requestId: input.requestId,
          logicalRequestDigest: input.logicalRequestDigest,
          latestSequence: input.sequence,
          expiresAt: expiry,
        });
      }
      this.sequences.set(sequenceKey, {
        ownerRef: input.ownerRef,
        deviceId: input.deviceId,
        sequence: input.sequence,
        requestId: input.requestId,
        logicalRequestDigest: input.logicalRequestDigest,
        envelopeDigest: input.envelopeDigest,
        expiresAt: expiry,
      });
    }
    return {
      data: [{
        outcome,
        accepted_sequence: input.sequence,
        rate_count: rate.count,
        rate_limit: RATE_LIMIT,
        rate_resets_at: new Date(rate.startedAt + RATE_WINDOW_MS).toISOString(),
        state_expires_at: new Date(expiry).toISOString(),
      }],
      error: null,
    };
  }
}

function admissionInput(overrides: Partial<MiniBrainRelayAdmissionInput> = {}): MiniBrainRelayAdmissionInput {
  return {
    ownerRef: '1'.repeat(64),
    deviceId: '11111111-1111-4111-8111-111111111111',
    keyThumbprint: '2'.repeat(64),
    sequence: 1,
    requestId: randomUUID(),
    logicalRequestDigest: '3'.repeat(64),
    envelopeDigest: '4'.repeat(64),
    ...overrides,
  };
}

async function signedRequest(input: {
  readonly deviceId: string;
  readonly deviceToken: string;
  readonly privateKey: CryptoKey;
  readonly sequence: number;
  readonly issuedAt: string;
  readonly requestId: string;
  readonly payload: { readonly text: string };
  readonly baseCursor?: string | null;
}): Promise<MiniBrainSyncRequest> {
  const unsigned: MiniBrainSyncUnsignedRequest = {
    schema_version: MINI_BRAIN_SYNC_REQUEST_SCHEMA,
    device_id: input.deviceId,
    sequence: input.sequence,
    issued_at: input.issuedAt,
    operation: 'append',
    session_id: '55555555-5555-4555-8555-555555555555',
    request_id: input.requestId,
    base_cursor: input.baseCursor ?? null,
    payload: input.payload,
    payload_digest: createHash('sha256').update(canonicalJson(input.payload)).digest('hex'),
  };
  const signature = await webcrypto.subtle.sign(
    { name: 'ECDSA', hash: 'SHA-256' },
    input.privateKey,
    Buffer.from(syncSigningPayload(unsigned), 'utf8'),
  );
  return {
    ...unsigned,
    device_token: input.deviceToken,
    signature: Buffer.from(signature).toString('base64url'),
  };
}

async function expectRelayCode(promise: Promise<unknown>, code: string, status: number): Promise<void> {
  await assert.rejects(promise, error => {
    assert.ok(error instanceof MiniBrainRelayError);
    assert.equal(error.code, code);
    assert.equal(error.publicStatus, status);
    return true;
  });
}

async function main(): Promise<void> {
  const backend = new FakeDurableRelay();
  const instanceA = createMiniBrainRelayStateStoreForRpcClient(backend);
  const logicalRequestId = '11111111-1111-4111-8111-111111111112';
  const first = admissionInput({ requestId: logicalRequestId });
  assert.equal((await instanceA.admit(first)).replayKind, 'new_sequence');

  const coldInstance = createMiniBrainRelayStateStoreForRpcClient(backend);
  const unrelatedSecond = admissionInput({
    sequence: 2,
    requestId: '11111111-1111-4111-8111-111111111113',
    logicalRequestDigest: '5'.repeat(64),
    envelopeDigest: '6'.repeat(64),
  });
  assert.equal((await coldInstance.admit(unrelatedSecond)).replayKind, 'new_sequence');

  // Real-client response retry: same logical request_id and logical fields,
  // newly signed with a newer sequence/timestamp, hence a new envelope digest.
  const resignedRetry = {
    ...first,
    sequence: 3,
    envelopeDigest: '7'.repeat(64),
  };
  const retryFromAnotherColdInstance = createMiniBrainRelayStateStoreForRpcClient(backend);
  assert.equal(
    (await retryFromAnotherColdInstance.admit(resignedRetry)).replayKind,
    'identical_retry',
  );
  assert.equal(
    (await instanceA.admit(resignedRetry)).replayKind,
    'identical_retry',
    'an exact retransmission of the newer envelope is also idempotent',
  );
  await expectRelayCode(
    instanceA.admit({
      ...resignedRetry,
      sequence: 4,
      logicalRequestDigest: '8'.repeat(64),
      envelopeDigest: '9'.repeat(64),
    }),
    'BRAIN_SYNC_REQUEST_ID_REUSED',
    409,
  );
  await expectRelayCode(
    coldInstance.admit({ ...first, requestId: randomUUID(), envelopeDigest: 'a'.repeat(64) }),
    'BRAIN_SYNC_REPLAY_REJECTED',
    409,
  );

  // Two concurrent cold instances cannot both reserve one device sequence.
  const concurrentBackend = new FakeDurableRelay();
  const concurrentA = createMiniBrainRelayStateStoreForRpcClient(concurrentBackend);
  const concurrentB = createMiniBrainRelayStateStoreForRpcClient(concurrentBackend);
  const raceBase = admissionInput({ ownerRef: 'b'.repeat(64) });
  const race = await Promise.allSettled([
    concurrentA.admit({ ...raceBase, requestId: randomUUID(), envelopeDigest: 'c'.repeat(64) }),
    concurrentB.admit({ ...raceBase, requestId: randomUUID(), envelopeDigest: 'd'.repeat(64) }),
  ]);
  assert.equal(race.filter(result => result.status === 'fulfilled').length, 1);
  assert.equal(race.filter(result => result.status === 'rejected').length, 1);
  const rejected = race.find(result => result.status === 'rejected');
  assert.ok(rejected?.status === 'rejected' && rejected.reason instanceof MiniBrainRelayError);
  assert.equal(rejected.reason.code, 'BRAIN_SYNC_REPLAY_REJECTED');

  // The 30/minute window is owner-wide across device and serverless instances.
  // Retries are idempotent but still metered, so replay cannot be an unbounded
  // database/runtime read channel.
  const rateBackend = new FakeDurableRelay();
  const rateA = createMiniBrainRelayStateStoreForRpcClient(rateBackend);
  const rateB = createMiniBrainRelayStateStoreForRpcClient(rateBackend);
  const devices = [
    '22222222-2222-4222-8222-222222222222',
    '33333333-3333-4333-8333-333333333333',
  ];
  let firstRateRequest: MiniBrainRelayAdmissionInput | null = null;
  for (let index = 0; index < RATE_LIMIT - 1; index += 1) {
    const deviceIndex = index % devices.length;
    const input = admissionInput({
      ownerRef: 'e'.repeat(64),
      deviceId: devices[deviceIndex]!,
      sequence: Math.floor(index / devices.length) + 1,
      requestId: randomUUID(),
      logicalRequestDigest: index.toString(16).padStart(64, '0'),
      envelopeDigest: (index + 100).toString(16).padStart(64, '0'),
    });
    if (index === 0) firstRateRequest = input;
    await (index % 2 === 0 ? rateA : rateB).admit(input);
  }
  assert.ok(firstRateRequest);
  const retryAtLimit = await rateB.admit(firstRateRequest);
  assert.equal(retryAtLimit.replayKind, 'identical_retry');
  assert.equal(retryAtLimit.rateCount, RATE_LIMIT);
  await expectRelayCode(
    rateA.admit(admissionInput({
      ownerRef: 'e'.repeat(64),
      deviceId: devices[1]!,
      sequence: 16,
      requestId: randomUUID(),
      logicalRequestDigest: 'f'.repeat(64),
      envelopeDigest: '0'.repeat(64),
    })),
    'BRAIN_SYNC_RATE_LIMITED',
    429,
  );

  // A cryptographically verified but invalid replay must consume the same
  // durable owner budget. Otherwise a compromised device can bypass the rate
  // boundary by submitting endlessly divergent envelopes for one sequence.
  const rejectedRateBackend = new FakeDurableRelay();
  const rejectedRateStore = createMiniBrainRelayStateStoreForRpcClient(rejectedRateBackend);
  const chargedBase = admissionInput({ ownerRef: '9'.repeat(64) });
  await rejectedRateStore.admit(chargedBase);
  for (let index = 0; index < RATE_LIMIT - 1; index += 1) {
    await expectRelayCode(
      rejectedRateStore.admit({
        ...chargedBase,
        requestId: randomUUID(),
        envelopeDigest: (index + 500).toString(16).padStart(64, '0'),
      }),
      'BRAIN_SYNC_REPLAY_REJECTED',
      409,
    );
  }
  await expectRelayCode(
    rejectedRateStore.admit(chargedBase),
    'BRAIN_SYNC_RATE_LIMITED',
    429,
  );

  // The adapter transmits only opaque identity and two digests to storage.
  const promptBackend = new FakeDurableRelay();
  const promptStore = createMiniBrainRelayStateStoreForRpcClient(promptBackend);
  const payload = { text: 'A private prompt that must not cross the relay-state boundary.' };
  const request: MiniBrainSyncRequest = {
    schema_version: MINI_BRAIN_SYNC_REQUEST_SCHEMA,
    device_id: '44444444-4444-4444-8444-444444444444',
    sequence: 1,
    issued_at: '2026-09-03T20:00:00.000Z',
    operation: 'append',
    session_id: '55555555-5555-4555-8555-555555555555',
    request_id: '66666666-6666-4666-8666-666666666666',
    base_cursor: null,
    payload,
    payload_digest: 'a'.repeat(64),
    device_token: 'not-persisted',
    signature: 'not-persisted',
  };
  const verified: VerifiedMiniBrainRequest = {
    request,
    ownerRef: 'c'.repeat(64),
    keyThumbprint: 'd'.repeat(64),
  };
  const admitted = await admitVerifiedMiniBrainRequest(promptStore, verified);
  assert.equal(admitted.replayKind, 'new_sequence');
  const persistedParameters = promptBackend.calls.at(-1)?.parameters ?? {};
  assert.deepEqual(Object.keys(persistedParameters).sort(), [
    'p_device_id',
    'p_envelope_digest',
    'p_key_thumbprint',
    'p_logical_digest',
    'p_owner_ref',
    'p_request_id',
    'p_sequence',
  ]);
  assert.ok(!JSON.stringify(persistedParameters).includes(payload.text));
  assert.equal(persistedParameters.p_logical_digest, durableMiniBrainLogicalRequestDigest(request));
  assert.equal(persistedParameters.p_envelope_digest, durableMiniBrainEnvelopeDigest(request));

  const reSignedRequest = {
    ...request,
    sequence: 2,
    issued_at: '2026-09-03T20:00:05.000Z',
    signature: 'new-signature',
  };
  assert.equal(
    durableMiniBrainLogicalRequestDigest(reSignedRequest),
    durableMiniBrainLogicalRequestDigest(request),
  );
  assert.equal(
    durableMiniBrainLogicalRequestDigest({ ...reSignedRequest, base_cursor: 'f'.repeat(64) }),
    durableMiniBrainLogicalRequestDigest(request),
    'an explicit conflict rebase keeps the same logical request identity',
  );
  assert.notEqual(durableMiniBrainEnvelopeDigest(reSignedRequest), durableMiniBrainEnvelopeDigest(request));
  assert.notEqual(
    durableMiniBrainLogicalRequestDigest({
      ...reSignedRequest,
      payload: { text: 'changed' },
      payload_digest: 'b'.repeat(64),
    }),
    durableMiniBrainLogicalRequestDigest(request),
  );

  // Full verifier path: the real client signs the same logical request again
  // with a newer sequence and timestamp after a cold start. Cryptographic
  // verification remains fresh while durable idempotency stays stable.
  const previousBinding = process.env.APOCKY_BRAIN_DEVICE_BINDING_SECRET;
  try {
    process.env.APOCKY_BRAIN_DEVICE_BINDING_SECRET = 'durable-relay-test-binding-secret-'.repeat(2);
    const nowMs = Date.parse('2026-09-03T21:00:00.000Z');
    const keyPair = await webcrypto.subtle.generateKey(
      { name: 'ECDSA', namedCurve: 'P-256' },
      false,
      ['sign', 'verify'],
    ) as CryptoKeyPair;
    const deviceId = '77777777-7777-4777-8777-777777777777';
    const publicKeyJwk = await webcrypto.subtle.exportKey('jwk', keyPair.publicKey);
    const capability = issueMiniBrainDeviceCapability({
      userId: 'durable-owner',
      deviceId,
      publicKeyJwk,
      nowMs,
    });
    const clientRequestId = '88888888-8888-4888-8888-888888888888';
    const privatePayload = { text: 'A second private prompt that remains digest-only.' };
    const signedFirst = await signedRequest({
      deviceId,
      deviceToken: capability.device_token,
      privateKey: keyPair.privateKey,
      sequence: 1,
      issuedAt: new Date(nowMs).toISOString(),
      requestId: clientRequestId,
      payload: privatePayload,
      baseCursor: '1'.repeat(64),
    });
    resetMiniBrainRelayStateForTests();
    const cryptographicallyVerifiedFirst = await verifyMiniBrainSyncRequest({
      body: signedFirst,
      userId: 'durable-owner',
      nowMs,
    });
    const realBackend = new FakeDurableRelay();
    realBackend.now = nowMs;
    const durableFirst = await admitVerifiedMiniBrainRequest(
      createMiniBrainRelayStateStoreForRpcClient(realBackend),
      cryptographicallyVerifiedFirst,
    );
    assert.equal(durableFirst.replayKind, 'new_sequence');

    const signedRetry = await signedRequest({
      deviceId,
      deviceToken: capability.device_token,
      privateKey: keyPair.privateKey,
      sequence: 2,
      issuedAt: new Date(nowMs + 5_000).toISOString(),
      requestId: clientRequestId,
      payload: privatePayload,
      baseCursor: '2'.repeat(64),
    });
    resetMiniBrainRelayStateForTests(); // model a different cold serverless instance
    const cryptographicallyVerifiedRetry = await verifyMiniBrainSyncRequest({
      body: signedRetry,
      userId: 'durable-owner',
      nowMs: nowMs + 5_000,
    });
    const durableRetry = await admitVerifiedMiniBrainRequest(
      createMiniBrainRelayStateStoreForRpcClient(realBackend),
      cryptographicallyVerifiedRetry,
    );
    assert.equal(durableRetry.replayKind, 'identical_retry');
    assert.equal(
      durableMiniBrainLogicalRequestDigest(signedRetry),
      durableMiniBrainLogicalRequestDigest(signedFirst),
    );
    assert.notEqual(
      durableMiniBrainEnvelopeDigest(signedRetry),
      durableMiniBrainEnvelopeDigest(signedFirst),
    );
    assert.ok(!JSON.stringify(realBackend.calls).includes(privatePayload.text));
  } finally {
    resetMiniBrainRelayStateForTests();
    if (previousBinding === undefined) delete process.env.APOCKY_BRAIN_DEVICE_BINDING_SECRET;
    else process.env.APOCKY_BRAIN_DEVICE_BINDING_SECRET = previousBinding;
  }

  // TTL cleanup is bounded and observable across all three ledgers plus rate.
  promptBackend.advance(RETENTION_MS + 1);
  assert.deepEqual(await promptStore.cleanup(100), {
    sequenceRowsDeleted: 1,
    requestRowsDeleted: 1,
    deviceRowsDeleted: 1,
    rateRowsDeleted: 1,
  });

  // Missing/malformed durable service yields a typed fail-closed response.
  assert.throws(
    () => createMiniBrainRelayStateStore({}),
    error => error instanceof MiniBrainRelayError
      && error.code === 'BRAIN_RELAY_STATE_UNAVAILABLE'
      && error.publicStatus === 503,
  );
  assert.throws(
    () => createMiniBrainRelayStateStore({
      APOCKY_HUB_SUPABASE_URL: 'https://user:pass@example.com/',
      APOCKY_HUB_SUPABASE_SERVICE_ROLE_KEY: 'secret',
    }),
    error => error instanceof MiniBrainRelayError
      && error.code === 'BRAIN_RELAY_STATE_UNAVAILABLE',
  );

  console.log('mobile-relay-state.test : OK · re-signed logical retry + cold-instance sequence/rate + digest-only RPC + TTL');
}

void main().catch(error => {
  console.error(error);
  process.exitCode = 1;
});
