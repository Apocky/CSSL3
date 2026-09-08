import { strict as assert } from 'node:assert';
import { readFileSync } from 'node:fs';
import { generateKeyPairSync, type KeyObject } from 'node:crypto';

import {
  CONTRIBUTOR_LEASE_SCHEMA,
  CONTRIBUTOR_NETWORK_ENABLED,
  ContributorNodeError,
  ContributorWorker,
  signLease,
  verifyResultEnvelope,
  type PublicKeyInput,
  type LeaseEnvelope,
} from '../../contributor-node/src/runtime.js';

const NOW = 1_800_000_000_000;

interface Fixture {
  readonly envelope: LeaseEnvelope;
  readonly controllerPublicKey: KeyObject;
}

function lease(
  nodeId = 'node-test-1',
  leaseId = 'lease-test-1',
  size = 3,
  issuedAt = NOW - 100,
  expiresAt = NOW + 30_000,
): Fixture {
  const controller = generateKeyPairSync('ed25519');
  const values = Array.from({ length: size }, (_, index) => index + 1);
  const envelope = signLease({
    schema_version: CONTRIBUTOR_LEASE_SCHEMA,
    key_id: 'controller-v1',
    lease_id: leaseId,
    node_id: nodeId,
    issued_at: issuedAt,
    expires_at: expiresAt,
    attempt: 0,
    task: { kind: 'vector_dot', left: values, right: values.map((value) => value + 1) },
  }, controller.privateKey);
  return { envelope, controllerPublicKey: controller.publicKey };
}

function workerFor(fixture: Fixture): ContributorWorker {
  const identity = generateKeyPairSync('ed25519');
  const worker = new ContributorWorker({
    nodeId: fixture.envelope.node_id,
    controllerKeyId: fixture.envelope.key_id,
    controllerPublicKey: fixture.controllerPublicKey as PublicKeyInput,
    nodeSigningKey: identity.privateKey,
    clock: () => NOW,
  });
  return worker;
}

function errorIs(code: string) {
  return (error: unknown): boolean => error instanceof ContributorNodeError && error.code === code;
}

export async function testValidSignedLeaseProducesSignedResult(): Promise<void> {
  const fixture = lease();
  const worker = workerFor(fixture);
  worker.optIn();
  const result = await worker.run(fixture.envelope, NOW);
  assert.equal(result.ok, true);
  assert.equal(result.output, 20);
  assert.equal(result.error_code, null);
  assert.equal('left' in result, false);
  assert.equal(result.node_id, fixture.envelope.node_id);
  assert.match(result.signature_b64, /^[A-Za-z0-9_-]{86}$/);
  assert.equal(worker.status().mode, 'ready');
}

export async function testInvalidSignatureFailsBeforeReplayReservation(): Promise<void> {
  const fixture = lease();
  const worker = workerFor(fixture);
  worker.optIn();
  const forged = { ...fixture.envelope, lease_id: 'forged-lease' };
  await assert.rejects(() => worker.run(forged, NOW), errorIs('LEASE_SIGNATURE_INVALID'));
  assert.equal(worker.status().replay_entries, 0);
}

export async function testReplayIsRejectedAfterFirstExecution(): Promise<void> {
  const fixture = lease();
  const worker = workerFor(fixture);
  worker.optIn();
  await worker.run(fixture.envelope, NOW);
  await assert.rejects(() => worker.run(fixture.envelope, NOW), errorIs('LEASE_REPLAY'));
  assert.equal(worker.status().replay_entries, 1);
}

export async function testSignatureResultCanBeVerified(): Promise<void> {
  const fixture = lease();
  const identity = generateKeyPairSync('ed25519');
  const worker = new ContributorWorker({
    nodeId: fixture.envelope.node_id,
    controllerKeyId: fixture.envelope.key_id,
    controllerPublicKey: fixture.controllerPublicKey,
    nodeSigningKey: identity.privateKey,
    clock: () => NOW,
  });
  worker.optIn();
  const result = await worker.run(fixture.envelope, NOW);
  const verified = verifyResultEnvelope(result, identity.publicKey, fixture.envelope.node_id);
  assert.equal(verified.output, 20);
  assert.throws(
    () => verifyResultEnvelope({ ...result, output: 21 }, identity.publicKey, fixture.envelope.node_id),
    errorIs('RESULT_SIGNATURE_INVALID'),
  );
}

export async function testLeaseTemporalAndIdentityGuards(): Promise<void> {
  const expired = lease('node-guards', 'lease-expired', 3, NOW - 30_000, NOW - 1);
  const expiredWorker = workerFor(expired);
  expiredWorker.optIn();
  await assert.rejects(() => expiredWorker.run(expired.envelope, NOW), errorIs('LEASE_EXPIRED'));

  const future = lease('node-guards', 'lease-future', 3, NOW + 30_001, NOW + 60_000);
  const futureWorker = workerFor(future);
  futureWorker.optIn();
  await assert.rejects(() => futureWorker.run(future.envelope, NOW), errorIs('LEASE_NOT_YET_VALID'));

  const mismatch = lease('node-guards', 'lease-mismatch');
  const mismatchWorker = workerFor(mismatch);
  mismatchWorker.optIn();
  await assert.rejects(
    () => mismatchWorker.run({ ...mismatch.envelope, node_id: 'node-other' }, NOW),
    errorIs('LEASE_NODE_MISMATCH'),
  );
  await assert.rejects(
    () => mismatchWorker.run({ ...mismatch.envelope, key_id: 'controller-v2' }, NOW),
    errorIs('CONTROLLER_KEY_ID_MISMATCH'),
  );
}

export async function testInputOperationAndOutputLimitsFailClosed(): Promise<void> {
  const oversized = lease('node-limits', 'lease-elements', 3);
  const elementsWorker = new ContributorWorker({
    nodeId: oversized.envelope.node_id,
    controllerKeyId: oversized.envelope.key_id,
    controllerPublicKey: oversized.controllerPublicKey,
    policy: { maxElements: 2 },
    clock: () => NOW,
  });
  elementsWorker.optIn();
  await assert.rejects(() => elementsWorker.run(oversized.envelope, NOW), errorIs('TASK_INPUT_LIMIT'));

  const operationWorker = new ContributorWorker({
    nodeId: oversized.envelope.node_id,
    controllerKeyId: oversized.envelope.key_id,
    controllerPublicKey: oversized.controllerPublicKey,
    policy: { maxOperations: 1 },
    clock: () => NOW,
  });
  operationWorker.optIn();
  await assert.rejects(() => operationWorker.run(oversized.envelope, NOW), errorIs('TASK_OPERATION_LIMIT'));

  const bytes = lease('node-limits', 'lease-bytes', 64);
  const bytesWorker = new ContributorWorker({
    nodeId: bytes.envelope.node_id,
    controllerKeyId: bytes.envelope.key_id,
    controllerPublicKey: bytes.controllerPublicKey,
    policy: { maxTaskBytes: 256 },
    clock: () => NOW,
  });
  bytesWorker.optIn();
  await assert.rejects(() => bytesWorker.run(bytes.envelope, NOW), errorIs('TASK_INPUT_LIMIT'));
}

export async function testPrivacyCanariesRejectPrivateFields(): Promise<void> {
  const fixture = lease();
  const worker = workerFor(fixture);
  worker.optIn();
  await assert.rejects(
    () => worker.run({ ...fixture.envelope, prompt: 'private conversation' }, NOW),
    errorIs('LEASE_SCHEMA_INVALID'),
  );
  await assert.rejects(
    () => worker.run({ ...fixture.envelope, task: { ...fixture.envelope.task, vault_payload: 'secret' } }, NOW),
    errorIs('TASK_SCHEMA_INVALID'),
  );
  assert.equal(CONTRIBUTOR_NETWORK_ENABLED, false);
  const runtime = readFileSync(new URL('../../contributor-node/src/runtime.ts', import.meta.url), 'utf8');
  const cli = readFileSync(new URL('../../contributor-node/src/cli.ts', import.meta.url), 'utf8');
  assert.doesNotMatch(runtime, /node:(?:fs|http|https|net|tls|dns|child_process)/);
  assert.doesNotMatch(runtime, /\bfetch\s*\(/);
  assert.doesNotMatch(cli, /node:(?:fs|http|https|net|tls|dns|child_process)/);
  assert.doesNotMatch(cli, /\bfetch\s*\(/);
}

export async function testPauseRevokeAndUninstallControls(): Promise<void> {
  const fixture = lease('node-controls', 'lease-controls');
  const worker = workerFor(fixture);
  await assert.rejects(() => worker.run(fixture.envelope, NOW), errorIs('WORKER_NOT_OPTED_IN'));
  worker.optIn();
  worker.pause();
  await assert.rejects(() => worker.run(fixture.envelope, NOW), errorIs('WORKER_NOT_OPTED_IN'));

  const runningFixture = lease('node-controls', 'lease-pause-mid-run', 4_096);
  const runningWorker = workerFor(runningFixture);
  runningWorker.optIn();
  const running = runningWorker.run(runningFixture.envelope, NOW);
  setImmediate(() => runningWorker.pause());
  const pausedResult = await running;
  assert.equal(pausedResult.ok, false);
  assert.equal(pausedResult.error_code, 'TASK_PAUSED');
  assert.equal(runningWorker.status().mode, 'paused');

  const revoked = workerFor(fixture);
  revoked.optIn();
  revoked.revoke();
  assert.equal(revoked.status().node_signing_key_present, false);
  await assert.rejects(() => revoked.run(fixture.envelope, NOW), errorIs('WORKER_REVOKED'));

  const uninstalled = workerFor(fixture);
  uninstalled.optIn();
  await uninstalled.run(fixture.envelope, NOW);
  const receipt = uninstalled.uninstall();
  assert.equal(receipt.state_cleared, true);
  assert.equal(uninstalled.status().mode, 'uninstalled');
  assert.equal(uninstalled.status().replay_entries, 0);
  assert.equal(uninstalled.status().node_signing_key_present, false);
  await assert.rejects(() => uninstalled.run(fixture.envelope, NOW), errorIs('WORKER_UNINSTALLED'));
}

async function runAll(): Promise<void> {
  await testValidSignedLeaseProducesSignedResult();
  await testInvalidSignatureFailsBeforeReplayReservation();
  await testReplayIsRejectedAfterFirstExecution();
  await testSignatureResultCanBeVerified();
  await testLeaseTemporalAndIdentityGuards();
  await testInputOperationAndOutputLimitsFailClosed();
  await testPrivacyCanariesRejectPrivateFields();
  await testPauseRevokeAndUninstallControls();
  console.log('contributor-node/runtime.test : OK · 8 tests passed');
}

declare const require: { main?: unknown } | undefined;
declare const module: { id?: unknown } | undefined;
if (typeof require !== 'undefined' && typeof module !== 'undefined' && require.main === module) {
  void runAll().catch((error: unknown) => {
    console.error(error);
    process.exitCode = 1;
  });
}
