import { strict as assert } from 'node:assert';
import { readFileSync } from 'node:fs';

import type { SupabaseClient } from '@supabase/supabase-js';

import {
  CONTRIBUTOR_TRANSPORT_ATOMIC_RPCS,
  createSupabaseContributorTransportStoreForClient,
  type ContributorTransportAtomicEnrollmentInput,
  type ContributorTransportAtomicLeaseInput,
  type ContributorTransportAtomicResultInput,
  type ContributorTransportAtomicRevokeInput,
} from '../../lib/apocrypha/contributor-transport-supabase';
import {
  ContributorTransportError,
  type ContributorNodeRecord,
  type EnrollmentReceipt,
  type LeaseDispatch,
  type ResultReceipt,
  type RevokeReceipt,
} from '../../lib/apocrypha/contributor-transport';

type Json = Record<string, unknown>;

const LEASE_REQUEST_HASH = 'c'.repeat(64);

class FakeSupabaseRpc {
  readonly calls: Array<{ readonly name: string; readonly parameters: Json }> = [];
  readonly responses = new Map<string, unknown>();
  readonly failures = new Map<string, Json>();

  from(_table: string): never {
    throw new Error('table path not expected in atomic RPC test');
  }

  async rpc(name: string, parameters: Json): Promise<{ data: unknown; error: Json | null }> {
    this.calls.push({ name, parameters });
    const error = this.failures.get(name);
    if (error) return { data: null, error };
    return { data: this.responses.get(name) ?? null, error: null };
  }
}

function errorIs(code: string) {
  return (error: unknown): boolean => error instanceof ContributorTransportError && error.code === code;
}

function node(): ContributorNodeRecord {
  return {
    node_id: 'node-rpc-1',
    node_key_id: 'node-rpc-v1',
    node_public_key_spki_b64: 'MCowBQYDK2VwAyEA' + 'A'.repeat(44),
    platform: 'windows-x64',
    capabilities: ['vector_dot'],
    status: 'active',
    revision: 1,
    enrolled_at: Date.parse('2026-01-01T00:00:00.000Z'),
    revoked_at: null,
    revoke_reason: null,
  };
}

function enrollmentReceipt(): EnrollmentReceipt {
  return {
    schema_version: 'apocrypha.contributor.enrollment-receipt.v1',
    request_id: 'enroll-rpc-1',
    enrollment_id: 'enr-rpc-1',
    node_id: 'node-rpc-1',
    node_key_id: 'node-rpc-v1',
    controller_key_id: 'controller-rpc-v1',
    status: 'active',
    revision: 1,
    request_hash: 'a'.repeat(64),
    issued_at: 1_800_000_000_000,
    expires_at: 1_800_000_060_000,
    signature_b64: 'A'.repeat(86),
  };
}

function leaseDispatch(): LeaseDispatch {
  return {
    schema_version: 'apocrypha.contributor.lease-dispatch.v1',
    dispatch_id: `dispatch-${LEASE_REQUEST_HASH.slice(0, 48)}`,
    request_id: 'lease-rpc-1',
    idempotency_key: 'idempotency-rpc-1',
    node_id: 'node-rpc-1',
    lease: {
      schema_version: 'apocrypha.contributor.lease.v1',
      key_id: 'controller-rpc-v1',
      lease_id: `lease-${LEASE_REQUEST_HASH.slice(0, 56)}`,
      node_id: 'node-rpc-1',
      issued_at: 1_800_000_000_000,
      expires_at: 1_800_000_060_000,
      attempt: 0,
      task: { kind: 'vector_dot', left: [1], right: [2] },
      signature_b64: 'A'.repeat(86),
    },
    signature_b64: 'A'.repeat(86),
  };
}

function resultReceipt(): ResultReceipt {
  return {
    schema_version: 'apocrypha.contributor.result-receipt.v1',
    dispatch_id: `dispatch-${LEASE_REQUEST_HASH.slice(0, 48)}`,
    request_id: 'lease-rpc-1',
    idempotency_key: 'idempotency-rpc-1',
    node_id: 'node-rpc-1',
    lease_id: `lease-${LEASE_REQUEST_HASH.slice(0, 56)}`,
    result_hash: 'b'.repeat(64),
    status: 'accepted',
    accepted_at: 1_800_000_000_001,
    signature_b64: 'A'.repeat(86),
  };
}

function revokeReceipt(): RevokeReceipt {
  return {
    schema_version: 'apocrypha.contributor.revoke-receipt.v1',
    request_id: 'revoke-rpc-1',
    node_id: 'node-rpc-1',
    controller_key_id: 'controller-rpc-v1',
    status: 'revoked',
    revision: 2,
    reason: 'operator requested',
    revoked_at: 1_800_000_000_002,
    signature_b64: 'A'.repeat(86),
  };
}

async function testAtomicOperationsUseDedicatedRpcNames(): Promise<void> {
  const fake = new FakeSupabaseRpc();
  const receipt = enrollmentReceipt();
  const dispatch = leaseDispatch();
  const result = resultReceipt();
  const revoke = revokeReceipt();
  fake.responses.set(CONTRIBUTOR_TRANSPORT_ATOMIC_RPCS.enroll, receipt);
  fake.responses.set(CONTRIBUTOR_TRANSPORT_ATOMIC_RPCS.issueLease, dispatch);
  fake.responses.set(CONTRIBUTOR_TRANSPORT_ATOMIC_RPCS.acceptResult, result);
  fake.responses.set(CONTRIBUTOR_TRANSPORT_ATOMIC_RPCS.revoke, revoke);
  const store = createSupabaseContributorTransportStoreForClient(fake as unknown as SupabaseClient);

  const enrollInput: ContributorTransportAtomicEnrollmentInput = {
    requestId: receipt.request_id,
    requestHash: receipt.request_hash,
    node: node(),
    receipt,
  };
  const leaseInput: ContributorTransportAtomicLeaseInput = {
    nodeId: dispatch.node_id,
    idempotencyKey: dispatch.idempotency_key,
    requestHash: LEASE_REQUEST_HASH,
    dispatch,
  };
  const resultInput: ContributorTransportAtomicResultInput = {
    nodeId: result.node_id,
    dispatchId: result.dispatch_id,
    submissionHash: 'd'.repeat(64),
    receipt: result,
  };
  const revokeInput: ContributorTransportAtomicRevokeInput = {
    requestId: revoke.request_id,
    nodeId: revoke.node_id,
    requestHash: 'e'.repeat(64),
    reason: revoke.reason,
    receipt: revoke,
  };

  assert.equal(store.atomicRpcCapable, true);
  assert.deepEqual(await store.atomicEnroll(enrollInput), receipt);
  assert.deepEqual(await store.atomicIssueLease(leaseInput), dispatch);
  assert.deepEqual(await store.atomicAcceptResult(resultInput), result);
  assert.deepEqual(await store.atomicRevoke(revokeInput), revoke);
  assert.deepEqual(fake.calls.map((call) => call.name), [
    CONTRIBUTOR_TRANSPORT_ATOMIC_RPCS.enroll,
    CONTRIBUTOR_TRANSPORT_ATOMIC_RPCS.issueLease,
    CONTRIBUTOR_TRANSPORT_ATOMIC_RPCS.acceptResult,
    CONTRIBUTOR_TRANSPORT_ATOMIC_RPCS.revoke,
  ]);
  assert.equal(fake.calls[0]?.parameters.p_request_id, 'enroll-rpc-1');
  assert.equal(fake.calls[1]?.parameters.p_node_id, 'node-rpc-1');
  assert.equal(fake.calls[2]?.parameters.p_dispatch_id, dispatch.dispatch_id);
  assert.equal(fake.calls[3]?.parameters.p_reason, 'operator requested');
}

async function testRpcErrorsAndUnavailableSurfaceFailClosed(): Promise<void> {
  const fake = new FakeSupabaseRpc();
  const receipt = enrollmentReceipt();
  fake.failures.set(CONTRIBUTOR_TRANSPORT_ATOMIC_RPCS.enroll, { message: 'TRANSPORT_IDEMPOTENCY_CONFLICT' });
  const store = createSupabaseContributorTransportStoreForClient(fake as unknown as SupabaseClient);
  await assert.rejects(
    () => store.atomicEnroll({
      requestId: receipt.request_id,
      requestHash: receipt.request_hash,
      node: node(),
      receipt,
    }),
    errorIs('TRANSPORT_IDEMPOTENCY_CONFLICT'),
  );

  fake.failures.set(CONTRIBUTOR_TRANSPORT_ATOMIC_RPCS.enroll, { message: 'function does not exist' });
  await assert.rejects(
    () => store.atomicEnroll({
      requestId: receipt.request_id,
      requestHash: receipt.request_hash,
      node: node(),
      receipt,
    }),
    errorIs('TRANSPORT_STORE_UNAVAILABLE'),
  );

  const noRpc = createSupabaseContributorTransportStoreForClient({
    from: () => { throw new Error('not expected'); },
  } as unknown as SupabaseClient);
  assert.equal(noRpc.atomicRpcCapable, false);
  await assert.rejects(
    () => noRpc.atomicEnroll({
      requestId: receipt.request_id,
      requestHash: receipt.request_hash,
      node: node(),
      receipt,
    }),
    errorIs('TRANSPORT_STORE_UNAVAILABLE'),
  );
}

async function testAtomicInputBindingIsValidatedBeforeRpc(): Promise<void> {
  const fake = new FakeSupabaseRpc();
  const store = createSupabaseContributorTransportStoreForClient(fake as unknown as SupabaseClient);
  const receipt = enrollmentReceipt();
  await assert.rejects(
    () => store.atomicEnroll({
      requestId: 'wrong-request',
      requestHash: receipt.request_hash,
      node: node(),
      receipt,
    }),
    errorIs('TRANSPORT_STORE_UNAVAILABLE'),
  );
  assert.equal(fake.calls.length, 0);
}

async function testMigrationDefinesAtomicLeastPrivilegeBoundary(): Promise<void> {
  const migration = readFileSync(new URL('../../../cssl-supabase/migrations/0054_apocrypha_contributor_transport_rpc.sql', import.meta.url), 'utf8');
  assert.doesNotMatch(migration, /apocrypha_job/i);
  for (const name of Object.values(CONTRIBUTOR_TRANSPORT_ATOMIC_RPCS)) {
    assert.match(migration, new RegExp(`CREATE OR REPLACE FUNCTION public\\.${name}\\(`));
    assert.match(migration, new RegExp(`REVOKE EXECUTE ON FUNCTION public\\.${name}`));
    assert.match(migration, new RegExp(`GRANT EXECUTE ON FUNCTION public\\.${name}`));
  }
  assert.match(migration, /SECURITY DEFINER/);
  assert.match(migration, /SET search_path = pg_catalog, public, extensions/);
  assert.match(migration, /FOR UPDATE/);
  assert.match(migration, /TO service_role/);
  assert.match(migration, /TRANSPORT_IDEMPOTENCY_CONFLICT/);
  assert.match(migration, /TRANSPORT_NODE_REVOKED/);
  assert.match(migration, /TRANSPORT_RESULT_REPLAY/);
  assert.match(migration, /jsonb_object_keys\(v_lease\)\) <> 9/);
  assert.match(migration, /v_lease->>'signature_b64'/);
}

async function runAll(): Promise<void> {
  await testAtomicOperationsUseDedicatedRpcNames();
  await testRpcErrorsAndUnavailableSurfaceFailClosed();
  await testAtomicInputBindingIsValidatedBeforeRpc();
  await testMigrationDefinesAtomicLeastPrivilegeBoundary();
  console.log('contributor-node/transport-supabase-rpc.test : OK · 4 tests passed');
}

declare const require: { main?: unknown } | undefined;
declare const module: { id?: unknown } | undefined;
if (typeof require !== 'undefined' && typeof module !== 'undefined' && require.main === module) {
  void runAll().catch((error: unknown) => {
    console.error(error);
    process.exitCode = 1;
  });
}
