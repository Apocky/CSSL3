import { strict as assert } from 'node:assert';
import { readFileSync } from 'node:fs';

import type { SupabaseClient } from '@supabase/supabase-js';

import {
  CONTRIBUTOR_RATE_LIMITER_OPT_IN_ENV,
  CONTRIBUTOR_RATE_LIMIT_POLICIES,
  CONTRIBUTOR_RATE_LIMIT_RPC,
  ContributorRateLimiterError,
  createSupabaseContributorRateLimiter,
  createSupabaseContributorRateLimiterForClient,
} from '../../lib/apocrypha/contributor-rate-limit-supabase';

type Json = Record<string, unknown>;

class FakeSupabaseRpc {
  readonly calls: Array<{ readonly name: string; readonly parameters: Json }> = [];
  response: unknown = { allowed: true };
  error: Json | null = null;

  async rpc(name: string, parameters: Json): Promise<{ data: unknown; error: Json | null }> {
    this.calls.push({ name, parameters });
    return { data: this.response, error: this.error };
  }
}

function errorIs(code: string) {
  return (error: unknown): boolean => error instanceof ContributorRateLimiterError
    && error.code === code;
}

function isolateEnv(): () => void {
  const names = [
    CONTRIBUTOR_RATE_LIMITER_OPT_IN_ENV,
    'APOCKY_HUB_SUPABASE_URL',
    'NEXT_PUBLIC_SUPABASE_URL',
    'SUPABASE_SERVICE_ROLE_KEY',
  ] as const;
  const previous = new Map<string, string | undefined>();
  for (const name of names) {
    previous.set(name, process.env[name]);
    delete process.env[name];
  }
  return () => {
    for (const [name, value] of previous) {
      if (value === undefined) delete process.env[name];
      else process.env[name] = value;
    }
  };
}

async function testFactoryRequiresExplicitOptInAndServerKey(): Promise<void> {
  const restore = isolateEnv();
  try {
    const disabled = createSupabaseContributorRateLimiter();
    assert.equal(disabled.ok, false);
    if (!disabled.ok) {
      assert.equal(disabled.code, 'RATE_LIMIT_STORE_UNAVAILABLE');
      assert.doesNotMatch(disabled.reason, /secret|token|service_role_key/i);
    }

    process.env[CONTRIBUTOR_RATE_LIMITER_OPT_IN_ENV] = 'supabase';
    const missingKey = createSupabaseContributorRateLimiter();
    assert.equal(missingKey.ok, false);
    if (!missingKey.ok) assert.equal(missingKey.code, 'RATE_LIMIT_STORE_UNAVAILABLE');
  } finally {
    restore();
  }
}

async function testAtomicRpcHashesKeyAndParsesDecision(): Promise<void> {
  const fake = new FakeSupabaseRpc();
  const limiter = createSupabaseContributorRateLimiterForClient(fake as unknown as SupabaseClient);
  const allowed = await limiter.check({ endpoint: 'enroll', key: '203.0.113.7', method: 'POST' });
  assert.deepEqual(allowed, { allowed: true });
  assert.equal(fake.calls.length, 1);
  assert.equal(fake.calls[0]?.name, CONTRIBUTOR_RATE_LIMIT_RPC);
  assert.equal(fake.calls[0]?.parameters.p_scope, CONTRIBUTOR_RATE_LIMIT_POLICIES.enroll.scope);
  assert.equal(fake.calls[0]?.parameters.p_limit, CONTRIBUTOR_RATE_LIMIT_POLICIES.enroll.limit);
  assert.equal(fake.calls[0]?.parameters.p_window_seconds, CONTRIBUTOR_RATE_LIMIT_POLICIES.enroll.windowSeconds);
  assert.match(String(fake.calls[0]?.parameters.p_key_digest), /^[0-9a-f]{64}$/);
  assert.notEqual(fake.calls[0]?.parameters.p_key_digest, '203.0.113.7');

  fake.response = { allowed: false, retry_after_seconds: 42 };
  const limited = await limiter.check({ endpoint: 'result', key: '203.0.113.7', method: 'POST' });
  assert.deepEqual(limited, { allowed: false, retry_after_seconds: 42 });
}

async function testInvalidInputsResponsesAndRpcErrorsFailClosed(): Promise<void> {
  const fake = new FakeSupabaseRpc();
  const limiter = createSupabaseContributorRateLimiterForClient(fake as unknown as SupabaseClient);
  await assert.rejects(
    () => limiter.check({ endpoint: 'enroll', key: '', method: 'POST' }),
    errorIs('RATE_LIMIT_STORE_UNAVAILABLE'),
  );
  assert.equal(fake.calls.length, 0);

  fake.response = { allowed: false, retry_after_seconds: 0 };
  await assert.rejects(
    () => limiter.check({ endpoint: 'enroll', key: 'client', method: 'POST' }),
    errorIs('RATE_LIMIT_STORE_UNAVAILABLE'),
  );
  fake.response = null;
  await assert.rejects(
    () => limiter.check({ endpoint: 'enroll', key: 'client', method: 'POST' }),
    errorIs('RATE_LIMIT_STORE_UNAVAILABLE'),
  );
  fake.error = { code: 'PGRST202', message: 'function not found' };
  await assert.rejects(
    () => limiter.check({ endpoint: 'enroll', key: 'client', method: 'POST' }),
    errorIs('RATE_LIMIT_STORE_UNAVAILABLE'),
  );
}

async function testMigrationIsDedicatedAtomicAndBounded(): Promise<void> {
  const migration = readFileSync(new URL('../../../cssl-supabase/migrations/0055_apocrypha_contributor_rate_limit.sql', import.meta.url), 'utf8');
  assert.doesNotMatch(migration, /apocrypha_job/i);
  assert.match(migration, /CREATE TABLE IF NOT EXISTS public\.apocrypha_contributor_rate_limit_bucket/);
  assert.match(migration, /key_digest text NOT NULL/);
  assert.match(migration, /key_digest ~ '\^\[0-9a-f\]\{64\}\$'/);
  assert.match(migration, /ON CONFLICT \(scope, key_digest, window_start\) DO UPDATE/);
  assert.match(migration, /WHERE public\.apocrypha_contributor_rate_limit_bucket\.request_count < p_limit/);
  assert.match(migration, /LIMIT 256/);
  assert.match(migration, /SECURITY DEFINER/);
  assert.match(migration, /SET search_path = pg_catalog, public, extensions/);
  assert.match(migration, /REVOKE ALL ON TABLE/);
  assert.match(migration, /FROM PUBLIC, anon, authenticated, service_role/);
  assert.match(migration, /GRANT EXECUTE ON FUNCTION public\.apocrypha_contributor_rate_limit_consume/);
  assert.match(migration, /TO service_role/);
}

async function runAll(): Promise<void> {
  await testFactoryRequiresExplicitOptInAndServerKey();
  await testAtomicRpcHashesKeyAndParsesDecision();
  await testInvalidInputsResponsesAndRpcErrorsFailClosed();
  await testMigrationIsDedicatedAtomicAndBounded();
  console.log('contributor-node/rate-limit-supabase.test : OK · 4 tests passed');
}

declare const require: { main?: unknown } | undefined;
declare const module: { id?: unknown } | undefined;
if (typeof require !== 'undefined' && typeof module !== 'undefined' && require.main === module) {
  void runAll().catch((error: unknown) => {
    console.error(error);
    process.exitCode = 1;
  });
}
