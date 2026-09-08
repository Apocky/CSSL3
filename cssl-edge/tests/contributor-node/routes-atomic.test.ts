import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';

import type { NextApiRequest, NextApiResponse } from 'next';

import {
  createContributorRouteHandler,
  type ContributorAtomicRouteController,
  type ContributorRateLimiter,
} from '@/lib/apocrypha/contributor-http';
import type {
  ContributorNodeRecord,
  EnrollmentReceipt,
  LeaseDispatch,
  ResultReceipt,
  RevokeReceipt,
} from '@/lib/apocrypha/contributor-transport';
import type {
  ContributorTransportAtomicEnrollmentInput,
  ContributorTransportAtomicLeaseInput,
  ContributorTransportAtomicOperations,
  ContributorTransportAtomicResultInput,
  ContributorTransportAtomicRevokeInput,
} from '@/lib/apocrypha/contributor-transport-supabase';

interface Output {
  statusCode: number;
  body: unknown;
  headers: Record<string, string>;
}

function mock(
  method: string,
  body: unknown = undefined,
  headers: Record<string, string> = {},
  query: Record<string, unknown> = {},
): { req: NextApiRequest; res: NextApiResponse; out: Output } {
  const out: Output = { statusCode: 0, body: null, headers: {} };
  const req = {
    method,
    headers,
    query,
    body,
    socket: { remoteAddress: '127.0.0.1' },
  } as unknown as NextApiRequest;
  const res = {
    status(code: number) { out.statusCode = code; return this; },
    json(value: unknown) { out.body = value; return this; },
    setHeader(name: string, value: string | number | readonly string[]) {
      out.headers[name.toLowerCase()] = Array.isArray(value) ? value.join(', ') : String(value);
      return this;
    },
  } as unknown as NextApiResponse;
  return { req, res, out };
}

function bodyHeaders(body: unknown): Record<string, string> {
  return {
    'content-type': 'application/json',
    'content-length': String(Buffer.byteLength(JSON.stringify(body), 'utf8')),
  };
}

const allowAll: ContributorRateLimiter = { check: () => ({ allowed: true }) };

const node: ContributorNodeRecord = {
  node_id: 'atomic-route-node-01',
  node_key_id: 'atomic-route-node-key-v1',
  node_public_key_spki_b64: 'MCowBQYDK2VwAyEA' + 'A'.repeat(44),
  platform: 'windows-x64',
  capabilities: ['vector_dot'],
  status: 'active',
  revision: 1,
  enrolled_at: 1_800_000_000_000,
  revoked_at: null,
  revoke_reason: null,
};

const enrollmentReceipt: EnrollmentReceipt = {
  schema_version: 'apocrypha.contributor.enrollment-receipt.v1',
  request_id: 'atomic-enroll-request-01',
  enrollment_id: 'enr-atomic-route-01',
  node_id: node.node_id,
  node_key_id: node.node_key_id,
  controller_key_id: 'atomic-route-controller-v1',
  status: 'active',
  revision: 1,
  request_hash: 'a'.repeat(64),
  issued_at: 1_800_000_000_000,
  expires_at: 1_800_000_060_000,
  signature_b64: 'A'.repeat(86),
};

const leaseDispatch: LeaseDispatch = {
  schema_version: 'apocrypha.contributor.lease-dispatch.v1',
  dispatch_id: 'dispatch-' + 'b'.repeat(48),
  request_id: 'atomic-lease-request-01',
  idempotency_key: 'atomic-idempotency-01',
  node_id: node.node_id,
  lease: {
    schema_version: 'apocrypha.contributor.lease.v1',
    key_id: 'atomic-route-controller-v1',
    lease_id: 'lease-' + 'b'.repeat(56),
    node_id: node.node_id,
    issued_at: 1_800_000_000_000,
    expires_at: 1_800_000_060_000,
    attempt: 0,
    task: { kind: 'vector_dot', left: [1], right: [2] },
    signature_b64: 'A'.repeat(86),
  },
  signature_b64: 'A'.repeat(86),
};

const resultReceipt: ResultReceipt = {
  schema_version: 'apocrypha.contributor.result-receipt.v1',
  dispatch_id: leaseDispatch.dispatch_id,
  request_id: leaseDispatch.request_id,
  idempotency_key: leaseDispatch.idempotency_key,
  node_id: node.node_id,
  lease_id: leaseDispatch.lease.lease_id,
  result_hash: 'c'.repeat(64),
  status: 'accepted',
  accepted_at: 1_800_000_000_001,
  signature_b64: 'A'.repeat(86),
};

const revokeReceipt: RevokeReceipt = {
  schema_version: 'apocrypha.contributor.revoke-receipt.v1',
  request_id: 'atomic-revoke-request-01',
  node_id: node.node_id,
  controller_key_id: 'atomic-route-controller-v1',
  status: 'revoked',
  revision: 2,
  reason: 'operator requested',
  revoked_at: 1_800_000_000_002,
  signature_b64: 'A'.repeat(86),
};

interface Calls {
  readonly operations: string[];
  readonly prepared: string[];
}

function atomicController(calls: Calls, enabled = true): ContributorAtomicRouteController {
  const enrollment: ContributorTransportAtomicEnrollmentInput = {
    requestId: enrollmentReceipt.request_id,
    requestHash: enrollmentReceipt.request_hash,
    node,
    receipt: enrollmentReceipt,
  };
  const lease: ContributorTransportAtomicLeaseInput = {
    nodeId: leaseDispatch.node_id,
    idempotencyKey: leaseDispatch.idempotency_key,
    requestHash: 'b'.repeat(64),
    dispatch: leaseDispatch,
  };
  const result: ContributorTransportAtomicResultInput = {
    nodeId: resultReceipt.node_id,
    dispatchId: resultReceipt.dispatch_id,
    submissionHash: 'd'.repeat(64),
    receipt: resultReceipt,
  };
  const revoke: ContributorTransportAtomicRevokeInput = {
    requestId: revokeReceipt.request_id,
    nodeId: revokeReceipt.node_id,
    requestHash: 'e'.repeat(64),
    reason: revokeReceipt.reason,
    receipt: revokeReceipt,
  };
  const operations: ContributorTransportAtomicOperations = {
    async atomicEnroll(input) { calls.operations.push('enroll'); assert.equal(input, enrollment); return enrollment.receipt; },
    async atomicIssueLease(input) { calls.operations.push('lease'); assert.equal(input, lease); return lease.dispatch; },
    async atomicAcceptResult(input) { calls.operations.push('result'); assert.equal(input, result); return result.receipt; },
    async atomicRevoke(input) { calls.operations.push('revoke'); assert.equal(input, revoke); return revoke.receipt; },
  };
  return {
    atomicRpcCapable: true,
    genericTransactionCapable: false,
    controllerSigningConfigured: enabled,
    operatorConfigured: enabled,
    operations,
    prepare: {
      async enrollment() { calls.prepared.push('enroll'); return enrollment; },
      async lease() { calls.prepared.push('lease'); return lease; },
      async result() { calls.prepared.push('result'); return result; },
      async revoke() { calls.prepared.push('revoke'); return revoke; },
    },
  };
}

const token = 'atomic-route-controller-token';
const tokenDigest = createHash('sha256').update(token, 'utf8').digest('hex');

export async function testAtomicRoutesUsePreparationAndOperationRpc(): Promise<void> {
  const calls: Calls = { operations: [], prepared: [] };
  const controller = atomicController(calls);
  const authorize = async () => ({
    user: { id: 'owner', email: 'owner@example.test', provider: 'test', createdAt: new Date(0).toISOString() },
    authConfigured: true,
    authorized: true,
  });

  const enrollBody = { request: 'enroll' };
  const enrolled = mock('POST', enrollBody, bodyHeaders(enrollBody));
  await createContributorRouteHandler('enroll', { atomicController: controller, rateLimiter: allowAll })(enrolled.req, enrolled.res);
  assert.equal(enrolled.out.statusCode, 200);
  assert.deepEqual(calls.prepared, ['enroll']);
  assert.deepEqual(calls.operations, ['enroll']);

  const leaseBody = { request: 'lease' };
  const leased = mock('POST', leaseBody, {
    ...bodyHeaders(leaseBody),
    authorization: `Bearer ${token}`,
  });
  await createContributorRouteHandler('lease', {
    atomicController: controller,
    controllerTokenSha256: tokenDigest,
    rateLimiter: allowAll,
  })(leased.req, leased.res);
  assert.equal(leased.out.statusCode, 200);

  const resultBody = { request: 'result' };
  const accepted = mock('POST', resultBody, bodyHeaders(resultBody));
  await createContributorRouteHandler('result', { atomicController: controller, rateLimiter: allowAll })(accepted.req, accepted.res);
  assert.equal(accepted.out.statusCode, 200);

  const revokeBody = { request: 'revoke' };
  const revoked = mock('POST', revokeBody, bodyHeaders(revokeBody));
  await createContributorRouteHandler('revoke', {
    atomicController: controller,
    rateLimiter: allowAll,
    authorize,
  })(revoked.req, revoked.res);
  assert.equal(revoked.out.statusCode, 200);
  assert.deepEqual(calls.prepared, ['enroll', 'lease', 'result', 'revoke']);
  assert.deepEqual(calls.operations, ['enroll', 'lease', 'result', 'revoke']);
}

export async function testAtomicStatusSeparatesRpcAndGenericTransaction(): Promise<void> {
  const calls: Calls = { operations: [], prepared: [] };
  const status = mock('GET');
  await createContributorRouteHandler('status', {
    atomicController: atomicController(calls),
    controllerTokenSha256: tokenDigest,
    rateLimiter: allowAll,
  })(status.req, status.res);
  assert.equal(status.out.statusCode, 200);
  const payload = (status.out.body as { status: Record<string, unknown> }).status;
  assert.equal(payload.atomic_rpc_capable, true);
  assert.equal(payload.atomic_controller_configured, true);
  assert.equal(payload.transactional_store_configured, false);
  assert.equal(payload.mutating_routes_enabled, true);
  assert.equal(payload.public_metadata_only, true);
}

export async function testAtomicRouteRejectsUnboundSigner(): Promise<void> {
  const calls: Calls = { operations: [], prepared: [] };
  const body = { request: 'enroll' };
  const noLimiter = mock('POST', body, bodyHeaders(body));
  await createContributorRouteHandler('enroll', {
    atomicController: atomicController(calls),
  })(noLimiter.req, noLimiter.res);
  assert.equal(noLimiter.out.statusCode, 503);
  assert.equal((noLimiter.out.body as Record<string, unknown>).code, 'transport_unconfigured');
  assert.deepEqual(calls.prepared, []);
  assert.deepEqual(calls.operations, []);

  const response = mock('POST', body, bodyHeaders(body));
  await createContributorRouteHandler('enroll', {
    atomicController: atomicController(calls, false),
    rateLimiter: allowAll,
  })(response.req, response.res);
  assert.equal(response.out.statusCode, 503);
  assert.equal((response.out.body as Record<string, unknown>).code, 'transport_unconfigured');
  assert.deepEqual(calls.prepared, []);
  assert.deepEqual(calls.operations, []);
}

async function runAll(): Promise<void> {
  // Keep the token variable visibly test-scoped without ever deriving or
  // storing a production credential in this fixture.
  assert.equal(token.length > 0, true);
  await testAtomicRoutesUsePreparationAndOperationRpc();
  await testAtomicStatusSeparatesRpcAndGenericTransaction();
  await testAtomicRouteRejectsUnboundSigner();
  console.log('contributor-node/routes-atomic.test : OK · 3 tests passed');
}

declare const require: { main?: unknown } | undefined;
declare const module: { id?: unknown } | undefined;
if (typeof require !== 'undefined' && typeof module !== 'undefined' && require.main === module) {
  void runAll().catch((error: unknown) => {
    console.error(error);
    process.exitCode = 1;
  });
}
