import { strict as assert } from 'node:assert';
import { readFileSync } from 'node:fs';

import type { SupabaseClient } from '@supabase/supabase-js';

import {
  CONTRIBUTOR_TRANSPORT_TABLES,
  createSupabaseContributorTransportStore,
  createSupabaseContributorTransportStoreForClient,
} from '../../lib/apocrypha/contributor-transport-supabase';
import { ContributorTransportError } from '../../lib/apocrypha/contributor-transport';

type Row = Record<string, unknown>;

class FakeQuery {
  private readonly filters: Array<[string, unknown]> = [];

  constructor(
    private readonly table: string,
    private readonly rows: Map<string, Row[]>,
    private readonly failures: Set<string>,
  ) {}

  select(_columns: string): this { return this; }

  eq(column: string, value: unknown): this {
    this.filters.push([column, value]);
    return this;
  }

  async maybeSingle(): Promise<{ data: Row | null; error: Row | null }> {
    if (this.failures.has(this.table)) return { data: null, error: { code: 'PGRST_TEST_FAILURE' } };
    const match = (this.rows.get(this.table) ?? []).filter((row) => this.filters.every(([key, value]) => row[key] === value));
    return { data: match.length === 0 ? null : match[0] ?? null, error: null };
  }

  async upsert(value: Row, _options?: { onConflict?: string }): Promise<{ data: null; error: Row | null }> {
    if (this.failures.has(this.table)) return { data: null, error: { code: 'PGRST_TEST_FAILURE' } };
    const tableRows = this.rows.get(this.table) ?? [];
    const key = this.table === CONTRIBUTOR_TRANSPORT_TABLES.node
      ? 'node_id'
      : this.table === CONTRIBUTOR_TRANSPORT_TABLES.leaseReplay
        ? 'dispatch_id'
        : this.table === CONTRIBUTOR_TRANSPORT_TABLES.resultReplay
          ? 'dispatch_id'
          : 'request_id';
    const index = tableRows.findIndex((row) => row[key] === value[key]);
    if (index < 0) tableRows.push({ ...value });
    else tableRows[index] = { ...tableRows[index], ...value };
    this.rows.set(this.table, tableRows);
    return { data: null, error: null };
  }
}

class FakeSupabase {
  readonly rows = new Map<string, Row[]>();
  readonly failures = new Set<string>();

  from(table: string): FakeQuery {
    return new FakeQuery(table, this.rows, this.failures);
  }
}

function errorIs(code: string) {
  return (error: unknown): boolean => error instanceof ContributorTransportError && error.code === code;
}

function nodeRow(): Row {
  return {
    node_id: 'node-supabase-1',
    node_key_id: 'node-supabase-v1',
    node_public_key_spki_b64: 'MCowBQYDK2VwAyEA' + 'A'.repeat(44),
    platform: 'windows-x64',
    capabilities: ['vector_dot'],
    status: 'active',
    revision: 1,
    enrolled_at: '2026-01-01T00:00:00.000Z',
    revoked_at: null,
    revoke_reason: null,
  };
}

function signed(schemaVersion: string): Row {
  if (schemaVersion === 'apocrypha.contributor.enrollment-receipt.v1') {
    return {
      schema_version: schemaVersion,
      request_id: 'enroll-supabase-1',
      enrollment_id: 'enr-supabase-1',
      node_id: 'node-supabase-1',
      node_key_id: 'node-supabase-v1',
      controller_key_id: 'controller-v1',
      status: 'active',
      revision: 1,
      request_hash: 'a'.repeat(64),
      issued_at: 1_800_000_000_000,
      expires_at: 1_800_000_060_000,
      signature_b64: 'A'.repeat(86),
    };
  }
  return { schema_version: schemaVersion, signature_b64: 'A'.repeat(86) };
}

async function testFactoryFailsClosedWithoutServerConfiguration(): Promise<void> {
  const previousUrl = process.env.APOCKY_HUB_SUPABASE_URL;
  const previousPublicUrl = process.env.NEXT_PUBLIC_SUPABASE_URL;
  const previousKey = process.env.SUPABASE_SERVICE_ROLE_KEY;
  try {
    delete process.env.APOCKY_HUB_SUPABASE_URL;
    delete process.env.NEXT_PUBLIC_SUPABASE_URL;
    delete process.env.SUPABASE_SERVICE_ROLE_KEY;
    const result = createSupabaseContributorTransportStore();
    assert.equal(result.ok, false);
    if (!result.ok) {
      assert.equal(result.code, 'TRANSPORT_STORE_UNAVAILABLE');
      assert.doesNotMatch(result.reason, /service_role_key|secret|token/i);
    }
  } finally {
    if (previousUrl === undefined) delete process.env.APOCKY_HUB_SUPABASE_URL;
    else process.env.APOCKY_HUB_SUPABASE_URL = previousUrl;
    if (previousPublicUrl === undefined) delete process.env.NEXT_PUBLIC_SUPABASE_URL;
    else process.env.NEXT_PUBLIC_SUPABASE_URL = previousPublicUrl;
    if (previousKey === undefined) delete process.env.SUPABASE_SERVICE_ROLE_KEY;
    else process.env.SUPABASE_SERVICE_ROLE_KEY = previousKey;
  }
}

async function testPostgrestAdapterIsTableBoundAndTransactionExplicit(): Promise<void> {
  const fake = new FakeSupabase();
  fake.rows.set(CONTRIBUTOR_TRANSPORT_TABLES.node, [nodeRow()]);
  const store = createSupabaseContributorTransportStoreForClient(
    fake as unknown as SupabaseClient,
  );
  const node = await store.getNode('node-supabase-1');
  assert.equal(node?.node_id, 'node-supabase-1');
  assert.equal(store.transactional, false);
  await assert.rejects(
    () => store.transaction(async () => undefined),
    errorIs('TRANSPORT_STORE_UNAVAILABLE'),
  );

  const replay = {
    request_hash: 'a'.repeat(64),
    receipt: signed('apocrypha.contributor.enrollment-receipt.v1'),
  };
  await store.putEnrollmentReplay('enroll-supabase-1', replay as never);
  const loaded = await store.getEnrollmentReplay('enroll-supabase-1');
  assert.equal(loaded?.request_hash, replay.request_hash);
  assert.equal(loaded?.receipt.schema_version, replay.receipt.schema_version);

  const seen: string[] = [];
  const transactionStore = createSupabaseContributorTransportStoreForClient(
    fake as unknown as SupabaseClient,
    async (operation) => {
      seen.push('transaction');
      return operation();
    },
  );
  assert.equal(transactionStore.transactional, true);
  assert.equal(await transactionStore.transaction(async () => 42), 42);
  assert.deepEqual(seen, ['transaction']);
}

async function testDatabaseAndRowFailuresRemainTyped(): Promise<void> {
  const malformed = new FakeSupabase();
  malformed.rows.set(CONTRIBUTOR_TRANSPORT_TABLES.node, [{ node_id: 'not-a-valid-row' }]);
  const malformedStore = createSupabaseContributorTransportStoreForClient(
    malformed as unknown as SupabaseClient,
  );
  await assert.rejects(() => malformedStore.getNode('not-a-valid-row'), errorIs('TRANSPORT_STORE_UNAVAILABLE'));

  const failed = new FakeSupabase();
  failed.failures.add(CONTRIBUTOR_TRANSPORT_TABLES.node);
  const failedStore = createSupabaseContributorTransportStoreForClient(
    failed as unknown as SupabaseClient,
  );
  await assert.rejects(() => failedStore.getNode('node-supabase-1'), errorIs('TRANSPORT_STORE_UNAVAILABLE'));
}

async function testSchemaIsDedicatedAndLeastPrivilege(): Promise<void> {
  const migration = readFileSync(new URL('../../../cssl-supabase/migrations/0053_apocrypha_contributor_transport.sql', import.meta.url), 'utf8');
  assert.doesNotMatch(migration, /apocrypha_job/i);
  assert.match(migration, /ENABLE ROW LEVEL SECURITY/);
  assert.match(migration, /REVOKE ALL ON TABLE/);
  assert.match(migration, /TO service_role/);
  assert.match(migration, /pg_column_size\(receipt\) <= 65536/);
  assert.match(migration, /pg_column_size\(dispatch\) <= 131072/);
}

async function runAll(): Promise<void> {
  await testFactoryFailsClosedWithoutServerConfiguration();
  await testPostgrestAdapterIsTableBoundAndTransactionExplicit();
  await testDatabaseAndRowFailuresRemainTyped();
  await testSchemaIsDedicatedAndLeastPrivilege();
  console.log('contributor-node/transport-supabase.test : OK · 4 tests passed');
}

declare const require: { main?: unknown } | undefined;
declare const module: { id?: unknown } | undefined;
if (typeof require !== 'undefined' && typeof module !== 'undefined' && require.main === module) {
  void runAll().catch((error: unknown) => {
    console.error(error);
    process.exitCode = 1;
  });
}
