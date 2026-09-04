import assert from 'node:assert/strict';
import type { SupabaseClient } from '@supabase/supabase-js';

import { requireStoredMnemeProfile } from '@/lib/mneme/member-profile';
import { MnemeError } from '@/lib/mneme/types';

const profileId = `member-${'a'.repeat(40)}`;
const privateDetail = 'private-user@example.test PRIVATE_TOKEN PRIVATE_MEMORY';
const profileRow = {
  profile_id: profileId,
  sovereign_pk: `\\x${'11'.repeat(32)}`,
  sigma_mask: `\\x${'00'.repeat(19)}`,
  created_at: '2026-09-04T00:00:00.000Z',
  memory_count: 0,
  message_count: 0,
  meta: { privateDetail },
};

function clientFor(read: () => unknown): SupabaseClient {
  return {
    from(table: string) {
      assert.equal(table, 'mneme_profiles');
      return {
        select(columns: string) {
          assert.equal(columns, '*');
          return {
            eq(column: string, value: string) {
              assert.equal(column, 'profile_id');
              assert.equal(value, profileId);
              return { async maybeSingle() { return read(); } };
            },
          };
        },
      };
    },
  } as unknown as SupabaseClient;
}

async function observe(client: SupabaseClient | null) {
  const calls: unknown[][] = [];
  const previous = console.error;
  console.error = (...args: unknown[]) => { calls.push(args); };
  try {
    const result = await requireStoredMnemeProfile(client, profileId);
    return { result, calls };
  } finally {
    console.error = previous;
  }
}

async function assertFailure(client: SupabaseClient, category: 'upstream_query' | 'decode' | 'unknown') {
  const { result, calls } = await observe(client);
  assert.deepEqual(result, {
    ok: false,
    status: 503,
    code: 'MNEME_STORAGE_UNAVAILABLE',
    message: 'Private memory storage could not verify this profile. No memory was read or changed.',
  });
  assert.deepEqual(calls, [[JSON.stringify({ evt: 'mneme.profile.lookup.fail', category })]]);
  const emitted = JSON.stringify({ result, calls });
  for (const forbidden of [privateDetail, profileId, 'sovereign_pk', 'PRIVATE_STACK', 'PRIVATE_CODE']) {
    assert.ok(!emitted.includes(forbidden), `private content excluded: ${forbidden}`);
  }
}

async function main(): Promise<void> {
  await assertFailure(clientFor(() => ({
    data: null,
    error: { code: 'PRIVATE_CODE', message: privateDetail, details: privateDetail, hint: privateDetail },
  })), 'upstream_query');

  await assertFailure(clientFor(() => ({
    data: { ...profileRow, sovereign_pk: privateDetail },
    error: null,
  })), 'decode');

  await assertFailure(clientFor(() => {
    const error = new Error(privateDetail);
    error.stack = `PRIVATE_STACK ${privateDetail}`;
    throw error;
  }), 'unknown');

  await assertFailure(clientFor(() => {
    throw new MnemeError('PRIVATE_CODE', privateDetail);
  }), 'unknown');

  await assertFailure(clientFor(() => {
    throw { name: 'MnemeError', code: 'SB_PROFILE_GET', message: privateDetail };
  }), 'unknown');

  const missing = await observe(clientFor(() => ({ data: null, error: null })));
  assert.equal(missing.result?.status, 409);
  assert.equal(missing.result?.code, 'MNEME_PROFILE_NOT_PROVISIONED');
  assert.deepEqual(missing.calls, []);

  const disconnected = await observe(null);
  assert.equal(disconnected.result?.status, 503);
  assert.equal(disconnected.result?.code, 'MNEME_STORAGE_UNAVAILABLE');
  assert.deepEqual(disconnected.calls, []);

  const valid = await observe(clientFor(() => ({ data: profileRow, error: null })));
  assert.equal(valid.result, null);
  assert.deepEqual(valid.calls, []);
  console.log('mneme-profile-diagnostics.test: OK (8 cases)');
}

main().catch(error => { console.error(error); process.exitCode = 1; });
