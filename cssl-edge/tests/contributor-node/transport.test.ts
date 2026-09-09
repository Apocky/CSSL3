import { strict as assert } from 'node:assert';
import { generateKeyPairSync, type KeyObject } from 'node:crypto';

import {
  CONTRIBUTOR_ENROLLMENT_SCHEMA,
  CONTRIBUTOR_LEASE_REQUEST_SCHEMA,
  CONTRIBUTOR_LEASE_POLL_SCHEMA,
  CONTRIBUTOR_RESULT_SUBMISSION_SCHEMA,
  CONTRIBUTOR_REVOKE_SCHEMA,
  ContributorTransportController,
  ContributorTransportError,
  MemoryContributorTransportStore,
  publicKeySpkiB64,
  signEnrollmentRequest,
  signLeasePollRequest,
  signRevokeRequest,
  signResultSubmission,
  verifyEnrollmentReceipt,
  verifyLeaseDispatch,
  verifyContributorLeasePollRequest,
  verifyResultReceipt,
  verifyResultSubmission,
  verifyRevokeReceipt,
  type EnrollmentRequest,
  type LeaseDispatch,
  type LeaseIssueRequest,
  type LeasePollRequestPayload,
  type RevokeRequestPayload,
} from '../../lib/apocrypha/contributor-transport';
import {
  CONTRIBUTOR_LEASE_SCHEMA,
  ContributorNodeError,
  ContributorWorker,
  type LeaseEnvelope,
} from '../../contributor-node/src/runtime.js';

const NOW = 1_800_000_000_000;

function errorIs(code: string) {
  return (error: unknown): boolean => error instanceof ContributorTransportError && error.code === code;
}

function fixture(): {
  controller: KeyObject;
  controllerPublic: KeyObject;
  operator: KeyObject;
  operatorPublic: KeyObject;
  node: KeyObject;
  nodePublic: KeyObject;
  enrollment: EnrollmentRequest;
  leaseRequest: LeaseIssueRequest;
  transport: ContributorTransportController;
} {
  const controller = generateKeyPairSync('ed25519');
  const operator = generateKeyPairSync('ed25519');
  const node = generateKeyPairSync('ed25519');
  const enrollment = signEnrollmentRequest({
    schema_version: CONTRIBUTOR_ENROLLMENT_SCHEMA,
    request_id: 'enroll-01',
    node_id: 'node-transport-1',
    node_key_id: 'node-transport-v1',
    node_public_key_spki_b64: publicKeySpkiB64(node.publicKey),
    platform: 'windows-x64',
    capabilities: ['vector_dot'],
    consent_revision: 'consent-v1',
    issued_at: NOW - 1_000,
    expires_at: NOW + 120_000,
  }, node.privateKey);
  const leaseRequest: LeaseIssueRequest = {
    schema_version: CONTRIBUTOR_LEASE_REQUEST_SCHEMA,
    request_id: 'lease-request-01',
    idempotency_key: 'idem-001',
    node_id: enrollment.node_id,
    issued_at: NOW - 500,
    expires_at: NOW + 30_000,
    attempt: 1,
    task: { kind: 'vector_dot', left: [1, 2, 3], right: [4, 5, 6] },
  };
  const transport = new ContributorTransportController({
    controllerKeyId: 'controller-v1',
    controllerPrivateKey: controller.privateKey,
    operatorKeyId: 'operator-v1',
    operatorPublicKey: operator.publicKey,
    store: new MemoryContributorTransportStore(),
    now: () => NOW,
  });
  return {
    controller: controller.privateKey,
    controllerPublic: controller.publicKey,
    operator: operator.privateKey,
    operatorPublic: operator.publicKey,
    node: node.privateKey,
    nodePublic: node.publicKey,
    enrollment,
    leaseRequest,
    transport,
  };
}

export async function testEnrollmentProofOfPossessionAndReplay(): Promise<void> {
  const f = fixture();
  const first = await f.transport.enroll(f.enrollment);
  const replay = await f.transport.enroll(f.enrollment);
  assert.deepEqual(replay, first, 'same enrollment request must return exact receipt');
  assert.equal(verifyEnrollmentReceipt(first, f.controllerPublic, f.enrollment.node_id).status, 'active');

  const { signature_b64: _enrollmentSignature, ...enrollmentPayload } = f.enrollment;
  const changed = signEnrollmentRequest({ ...enrollmentPayload, platform: 'linux-x64' }, f.node);
  await assert.rejects(() => f.transport.enroll(changed), errorIs('TRANSPORT_ENROLLMENT_REPLAY'));

  const forged = { ...f.enrollment, node_id: 'node-forged' };
  await assert.rejects(() => f.transport.enroll(forged), errorIs('TRANSPORT_SIGNATURE_INVALID'));
}

export async function testLeaseDispatchIsSignedAndIdempotent(): Promise<void> {
  const f = fixture();
  await f.transport.enroll(f.enrollment);
  const [one, two] = await Promise.all([
    f.transport.issueLease(f.leaseRequest),
    f.transport.issueLease(f.leaseRequest),
  ]);
  assert.deepEqual(one, two, 'concurrent retry must not issue two leases');
  assert.equal(one.lease.key_id, 'controller-v1');
  assert.equal(one.lease.node_id, f.enrollment.node_id);
  const verified = verifyLeaseDispatch(one, f.controllerPublic, f.enrollment.node_id, 'controller-v1');
  assert.equal(verified.dispatch_id, one.dispatch_id);

  const changed = { ...f.leaseRequest, task: { kind: 'vector_dot' as const, left: [7], right: [8] } };
  await assert.rejects(() => f.transport.issueLease(changed), errorIs('TRANSPORT_IDEMPOTENCY_CONFLICT'));

  const tampered = { ...one, node_id: 'node-other' };
  assert.throws(() => verifyLeaseDispatch(tampered, f.controllerPublic), errorIs('TRANSPORT_SIGNATURE_INVALID'));
}

export async function testNodeSignedLeasePollProofOfPossession(): Promise<void> {
  const f = fixture();
  const poll: LeasePollRequestPayload = {
    schema_version: CONTRIBUTOR_LEASE_POLL_SCHEMA,
    request_id: 'poll-request-01',
    idempotency_key: 'poll-idempotency-01',
    node_id: f.enrollment.node_id,
    node_key_id: f.enrollment.node_key_id,
    issued_at: NOW - 500,
    expires_at: NOW + 30_000,
    attempt: 0,
    task: { kind: 'vector_dot', left: [1, 2], right: [3, 4] },
  };
  const signed = signLeasePollRequest(poll, f.node);
  assert.equal(
    verifyContributorLeasePollRequest(signed, f.nodePublic, f.enrollment.node_id, f.enrollment.node_key_id, NOW).node_id,
    f.enrollment.node_id,
  );
  await assert.rejects(
    async () => verifyContributorLeasePollRequest({ ...signed, node_id: 'node-other' }, f.nodePublic, f.enrollment.node_id, f.enrollment.node_key_id, NOW),
    errorIs('TRANSPORT_NODE_KEY_MISMATCH'),
  );
}

export async function testWorkerResultRoundTripAndReplay(): Promise<void> {
  const f = fixture();
  await f.transport.enroll(f.enrollment);
  const dispatch = await f.transport.issueLease(f.leaseRequest);
  const worker = new ContributorWorker({
    nodeId: f.enrollment.node_id,
    controllerKeyId: 'controller-v1',
    controllerPublicKey: f.controllerPublic,
    nodeKeyId: f.enrollment.node_key_id,
    nodeSigningKey: f.node,
    clock: () => NOW,
  });
  worker.optIn();
  const result = await worker.run(dispatch.lease, NOW);
  const submission = signResultSubmission({
    schema_version: CONTRIBUTOR_RESULT_SUBMISSION_SCHEMA,
    dispatch_id: dispatch.dispatch_id,
    request_id: dispatch.request_id,
    idempotency_key: dispatch.idempotency_key,
    node_id: dispatch.node_id,
    result,
  }, f.node);
  assert.equal(verifyResultSubmission(submission, f.nodePublic, f.enrollment.node_id, f.enrollment.node_key_id).result.output, 32);
  const receipt = await f.transport.acceptResult(submission);
  const replay = await f.transport.acceptResult(submission);
  assert.deepEqual(replay, receipt, 'lost result acknowledgement must be idempotent');
  assert.equal(verifyResultReceipt(receipt, f.controllerPublic).lease_id, dispatch.lease.lease_id);

  const alteredResult = { ...result, output: 33 };
  const altered = signResultSubmission({ ...submission, result: alteredResult }, f.node);
  await assert.rejects(() => f.transport.acceptResult(altered), errorIs('TRANSPORT_SIGNATURE_INVALID'));

  const wrongNode = { ...submission, node_id: 'node-other' };
  assert.throws(() => verifyResultSubmission(wrongNode, f.nodePublic, f.enrollment.node_id), errorIs('TRANSPORT_NODE_KEY_MISMATCH'));
}

export async function testRevocationSeversFutureTransport(): Promise<void> {
  const f = fixture();
  await f.transport.enroll(f.enrollment);
  const revokeInput: RevokeRequestPayload = {
    schema_version: CONTRIBUTOR_REVOKE_SCHEMA,
    request_id: 'revoke-01',
    node_id: f.enrollment.node_id,
    reason: 'operator requested stop',
    issued_at: NOW - 1_000,
    expires_at: NOW + 30_000,
    operator_key_id: 'operator-v1',
  };
  const command = signRevokeRequest(revokeInput, f.operator);
  const receipt = await f.transport.revoke(command);
  assert.equal(verifyRevokeReceipt(receipt, f.controllerPublic).status, 'revoked');
  assert.deepEqual(await f.transport.revoke(command), receipt, 'revoke retry must be idempotent');
  await assert.rejects(() => f.transport.issueLease(fixture().leaseRequest), errorIs('TRANSPORT_NODE_REVOKED'));

  const dispatchInput = { ...f.leaseRequest, request_id: 'lease-request-02', idempotency_key: 'idem-002' };
  await assert.rejects(() => f.transport.issueLease(dispatchInput), errorIs('TRANSPORT_NODE_REVOKED'));
}

export async function testFailClosedBoundaries(): Promise<void> {
  const f = fixture();
  await assert.rejects(() => f.transport.issueLease(f.leaseRequest), errorIs('TRANSPORT_NODE_NOT_ENROLLED'));
  await assert.rejects(() => f.transport.acceptResult({}), errorIs('TRANSPORT_RESULT_INVALID'));
  assert.throws(
    () => new ContributorTransportController({
      controllerKeyId: 'controller-v1',
      controllerPrivateKey: f.controller,
      store: undefined as never,
      now: () => NOW,
    }),
    errorIs('TRANSPORT_STORE_UNAVAILABLE'),
  );
  const badLease = {
    schema_version: CONTRIBUTOR_LEASE_SCHEMA,
    key_id: 'controller-v1',
    lease_id: 'lease-bad',
    node_id: f.enrollment.node_id,
    issued_at: NOW - 1_000,
    expires_at: NOW + 10_000,
    attempt: 1,
    task: { kind: 'vector_dot', left: [1], right: [1] },
    signature_b64: 'A'.repeat(86),
  } as LeaseEnvelope;
  await f.transport.enroll(f.enrollment);
  await assert.rejects(
    () => f.transport.acceptResult({
      schema_version: CONTRIBUTOR_RESULT_SUBMISSION_SCHEMA,
      dispatch_id: 'dispatch-unknown', request_id: 'lease-request-01', idempotency_key: 'idem-001',
      node_id: f.enrollment.node_id, result: {
        schema_version: 'apocrypha.contributor.result.v1', key_id: 'node-transport-v1', lease_id: badLease.lease_id,
        node_id: f.enrollment.node_id, attempt: 1, started_at: NOW, finished_at: NOW, ok: true, output: 1,
        error_code: null, error_message: null, signature_b64: 'A'.repeat(86),
      }, signature_b64: 'A'.repeat(86),
    }),
    errorIs('TRANSPORT_LEASE_UNKNOWN'),
  );
  assert.equal(ContributorNodeError.name, 'ContributorNodeError');
}

async function runAll(): Promise<void> {
  await testEnrollmentProofOfPossessionAndReplay();
  await testLeaseDispatchIsSignedAndIdempotent();
  await testNodeSignedLeasePollProofOfPossession();
  await testWorkerResultRoundTripAndReplay();
  await testRevocationSeversFutureTransport();
  await testFailClosedBoundaries();
  console.log('contributor-node/transport.test : OK · 6 tests passed');
}

declare const require: { main?: unknown } | undefined;
declare const module: { id?: unknown } | undefined;
if (typeof require !== 'undefined' && typeof module !== 'undefined' && require.main === module) {
  void runAll().catch((error: unknown) => {
    console.error(error);
    process.exitCode = 1;
  });
}
