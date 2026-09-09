import assert from 'node:assert/strict';
import { generateKeyPairSync, type KeyObject } from 'node:crypto';

import {
  CONTRIBUTOR_ENROLLMENT_SCHEMA,
  CONTRIBUTOR_LEASE_REQUEST_SCHEMA,
  CONTRIBUTOR_REVOKE_SCHEMA,
  ContributorTransportError,
  publicKeySpkiB64,
  signEnrollmentRequest,
  signLeasePollRequest,
  signRevokeRequest,
  signResultSubmission,
  verifyEnrollmentReceipt,
  verifyLeaseDispatch,
  verifyResultReceipt,
  verifyRevokeReceipt,
  type ContributorNodeRecord,
  type EnrollmentRequest,
  type LeaseDispatch,
  type LeaseReplay,
  type RevokeRequestPayload,
  type LeasePollRequestPayload,
} from '../../lib/apocrypha/contributor-transport';
import {
  CONTRIBUTOR_CONTROLLER_ENV,
  createProductionContributorAtomicController,
  type AtomicContributorStore,
} from '../../lib/apocrypha/contributor-atomic-controller';
import {
  type ContributorTransportAtomicEnrollmentInput,
  type ContributorTransportAtomicLeaseInput,
  type ContributorTransportAtomicResultInput,
  type ContributorTransportAtomicRevokeInput,
} from '../../lib/apocrypha/contributor-transport-supabase';
import {
  ContributorWorker,
} from '../../contributor-node/src/runtime';

const NOW = 1_800_000_000_000;

function pem(key: KeyObject, type: 'pkcs8' | 'spki'): string {
  return key.export({ type, format: 'pem' }).toString();
}

class FakeAtomicStore {
  readonly atomicRpcCapable = true as const;
  readonly transactional = false;
  node: ContributorNodeRecord | null = null;
  leaseReplay: LeaseReplay | null = null;
  accepted: ContributorTransportAtomicResultInput | null = null;
  revoked: ContributorTransportAtomicRevokeInput | null = null;

  async getNode(nodeId: string): Promise<ContributorNodeRecord | null> {
    return this.node?.node_id === nodeId ? this.node : null;
  }

  async getLeaseByDispatch(dispatchId: string): Promise<LeaseReplay | null> {
    return this.leaseReplay?.dispatch.dispatch_id === dispatchId ? this.leaseReplay : null;
  }

  async atomicEnroll(input: ContributorTransportAtomicEnrollmentInput) {
    this.node = input.node;
    return input.receipt;
  }

  async atomicIssueLease(input: ContributorTransportAtomicLeaseInput) {
    this.leaseReplay = { request_hash: input.requestHash, dispatch: input.dispatch };
    return input.dispatch;
  }

  async atomicAcceptResult(input: ContributorTransportAtomicResultInput) {
    this.accepted = input;
    return input.receipt;
  }

  async atomicRevoke(input: ContributorTransportAtomicRevokeInput) {
    this.revoked = input;
    if (this.node) {
      this.node = {
        ...this.node,
        status: 'revoked',
        revision: input.receipt.revision,
        revoked_at: input.receipt.revoked_at,
        revoke_reason: input.reason,
      };
    }
    return input.receipt;
  }
}

function configureEnv(controller: { privateKey: KeyObject }, operator: { publicKey: KeyObject }): () => void {
  const names = Object.values(CONTRIBUTOR_CONTROLLER_ENV);
  const previous = new Map<string, string | undefined>();
  for (const name of names) previous.set(name, process.env[name]);
  process.env[CONTRIBUTOR_CONTROLLER_ENV.keyId] = 'atomic-controller-test-v1';
  process.env[CONTRIBUTOR_CONTROLLER_ENV.privateKeyPem] = pem(controller.privateKey, 'pkcs8');
  process.env[CONTRIBUTOR_CONTROLLER_ENV.operatorKeyId] = 'atomic-operator-test-v1';
  process.env[CONTRIBUTOR_CONTROLLER_ENV.operatorPublicKeyPem] = pem(operator.publicKey, 'spki');
  return () => {
    for (const [name, value] of previous) {
      if (value === undefined) delete process.env[name];
      else process.env[name] = value;
    }
  };
}

function enrollment(node: { publicKey: KeyObject; privateKey: KeyObject }): EnrollmentRequest {
  return signEnrollmentRequest({
    schema_version: CONTRIBUTOR_ENROLLMENT_SCHEMA,
    request_id: 'atomic-controller-enroll-01',
    node_id: 'atomic-controller-node-01',
    node_key_id: 'atomic-controller-node-key-v1',
    node_public_key_spki_b64: publicKeySpkiB64(node.publicKey),
    platform: 'windows-x64',
    capabilities: ['vector_dot'],
    consent_revision: 'consent-v1',
    issued_at: NOW - 1_000,
    expires_at: NOW + 120_000,
  }, node.privateKey);
}

export async function testProductionBindingPreparesVerifiesSignsAndCallsAtomicOperations(): Promise<void> {
  const controller = generateKeyPairSync('ed25519');
  const operator = generateKeyPairSync('ed25519');
  const node = generateKeyPairSync('ed25519');
  const restore = configureEnv(controller, operator);
  try {
    const store = new FakeAtomicStore();
    const bound = createProductionContributorAtomicController({
      rateLimiterConfigured: true,
      store: store as unknown as AtomicContributorStore,
      now: () => NOW,
    });
    assert.ok(bound);
    assert.equal(bound.controllerSigningConfigured, true);
    assert.equal(bound.operatorConfigured, true);
    assert.equal(bound.atomicRpcCapable, true);

    const request = enrollment(node);
    const enrollmentInput = await bound.prepare.enrollment(request);
    assert.equal(
      verifyEnrollmentReceipt(enrollmentInput.receipt, controller.publicKey, request.node_id).status,
      'active',
    );
    await bound.operations.atomicEnroll(enrollmentInput);
    assert.equal(store.node?.node_id, request.node_id);

    const leaseRequest = {
      schema_version: CONTRIBUTOR_LEASE_REQUEST_SCHEMA,
      request_id: 'atomic-controller-lease-01',
      idempotency_key: 'atomic-controller-idempotency-01',
      node_id: request.node_id,
      issued_at: NOW - 500,
      expires_at: NOW + 30_000,
      attempt: 0,
      task: { kind: 'vector_dot' as const, left: [1, 2], right: [3, 4] },
    };
    const leaseInput = await bound.prepare.lease(leaseRequest);
    const dispatch = await bound.operations.atomicIssueLease(leaseInput);
    assert.equal(verifyLeaseDispatch(dispatch, controller.publicKey, request.node_id, 'atomic-controller-test-v1').node_id, request.node_id);

    const worker = new ContributorWorker({
      nodeId: request.node_id,
      controllerKeyId: 'atomic-controller-test-v1',
      controllerPublicKey: controller.publicKey,
      nodeKeyId: request.node_key_id,
      nodeSigningKey: node.privateKey,
      clock: () => NOW,
    });
    worker.optIn();
    const result = await worker.run(dispatch.lease, NOW);
    const submission = signResultSubmission({
      schema_version: 'apocrypha.contributor.result-submission.v1',
      dispatch_id: dispatch.dispatch_id,
      request_id: dispatch.request_id,
      idempotency_key: dispatch.idempotency_key,
      node_id: dispatch.node_id,
      result,
    }, node.privateKey);
    const resultInput = await bound.prepare.result(submission);
    const resultReceipt = await bound.operations.atomicAcceptResult(resultInput);
    assert.equal(verifyResultReceipt(resultReceipt, controller.publicKey).dispatch_id, dispatch.dispatch_id);

    const pollPayload: LeasePollRequestPayload = {
      schema_version: 'apocrypha.contributor.lease-poll.v1',
      request_id: 'atomic-controller-poll-01',
      idempotency_key: 'atomic-controller-poll-idem-01',
      node_id: request.node_id,
      node_key_id: request.node_key_id,
      issued_at: NOW - 500,
      expires_at: NOW + 30_000,
      attempt: 0,
      task: { kind: 'vector_dot', left: [1, 2], right: [3, 4] },
    };
    const pollRequest = signLeasePollRequest(pollPayload, node.privateKey);
    assert.ok(bound.prepare.poll);
    const pollInput = await bound.prepare.poll(pollRequest);
    const pollDispatch = await bound.operations.atomicIssueLease(pollInput);
    assert.equal(verifyLeaseDispatch(pollDispatch, controller.publicKey, request.node_id, 'atomic-controller-test-v1').node_id, request.node_id);

    const revokePayload: RevokeRequestPayload = {
      schema_version: CONTRIBUTOR_REVOKE_SCHEMA,
      request_id: 'atomic-controller-revoke-01',
      node_id: request.node_id,
      reason: 'operator requested',
      issued_at: NOW - 500,
      expires_at: NOW + 30_000,
      operator_key_id: 'atomic-operator-test-v1',
    };
    const revoke = signRevokeRequest(revokePayload, operator.privateKey);
    const revokeInput = await bound.prepare.revoke(revoke);
    const revokeReceipt = await bound.operations.atomicRevoke(revokeInput);
    assert.equal(verifyRevokeReceipt(revokeReceipt, controller.publicKey).status, 'revoked');
    assert.equal(store.node?.status, 'revoked');
  } finally {
    restore();
  }
}

export async function testProductionBindingFailsClosedWithoutRateLimiterOrControllerKeys(): Promise<void> {
  const controller = generateKeyPairSync('ed25519');
  const operator = generateKeyPairSync('ed25519');
  const restore = configureEnv(controller, operator);
  try {
    const store = new FakeAtomicStore();
    assert.equal(createProductionContributorAtomicController({
      rateLimiterConfigured: false,
      store: store as unknown as AtomicContributorStore,
    }), null);
  } finally {
    restore();
  }

  const names = Object.values(CONTRIBUTOR_CONTROLLER_ENV);
  const previous = new Map<string, string | undefined>();
  for (const name of names) {
    previous.set(name, process.env[name]);
    delete process.env[name];
  }
  try {
    assert.equal(createProductionContributorAtomicController({
      rateLimiterConfigured: true,
      store: new FakeAtomicStore() as unknown as AtomicContributorStore,
    }), null);
  } finally {
    for (const [name, value] of previous) {
      if (value === undefined) delete process.env[name];
      else process.env[name] = value;
    }
  }
}

export async function testProductionBindingDoesNotAcceptNonAtomicStore(): Promise<void> {
  const controller = generateKeyPairSync('ed25519');
  const operator = generateKeyPairSync('ed25519');
  const restore = configureEnv(controller, operator);
  try {
    const nonAtomic = {
      ...new FakeAtomicStore(),
      atomicRpcCapable: false,
    } as unknown as AtomicContributorStore;
    assert.equal(createProductionContributorAtomicController({
      rateLimiterConfigured: true,
      store: nonAtomic,
    }), null);
  } finally {
    restore();
  }
}

async function runAll(): Promise<void> {
  await testProductionBindingPreparesVerifiesSignsAndCallsAtomicOperations();
  await testProductionBindingFailsClosedWithoutRateLimiterOrControllerKeys();
  await testProductionBindingDoesNotAcceptNonAtomicStore();
  console.log('contributor-node/atomic-controller.test : OK · 3 tests passed');
}

declare const require: { main?: unknown } | undefined;
declare const module: { id?: unknown } | undefined;
if (typeof require !== 'undefined' && typeof module !== 'undefined' && require.main === module) {
  void runAll().catch((error: unknown) => {
    console.error(error);
    process.exitCode = 1;
  });
}
