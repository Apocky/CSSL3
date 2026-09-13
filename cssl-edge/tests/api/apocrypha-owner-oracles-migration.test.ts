import { strict as assert } from 'node:assert';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

const migration = readFileSync(resolve(
  process.cwd(),
  '..',
  'cssl-supabase',
  'migrations',
  '0059_apocrypha_owner_chat_oracles.sql',
), 'utf8');

function functionBody(name: string): string {
  const start = migration.indexOf(`CREATE OR REPLACE FUNCTION public.${name}`);
  assert.ok(start >= 0, `${name} exists`);
  const next = migration.indexOf('CREATE OR REPLACE FUNCTION public.', start + 1);
  return migration.slice(start, next >= 0 ? next : migration.length);
}

function testIsolatedManifest(): void {
  assert.match(migration, /CREATE TABLE public\.apocrypha_owner_chat_oracle_run/);
  assert.match(migration, /UNIQUE \(tenant_id, owner_principal_id, nonce\)/, 'nonce replay is owner scoped');
  assert.match(migration, /UNIQUE \(conversation_id\)/, 'generated oracle conversation cannot alias another run');
  assert.match(migration, /CREATE TABLE public\.apocrypha_owner_chat_oracle_job/);
  assert.match(
    migration,
    /CREATE UNIQUE INDEX apocrypha_owner_chat_oracle_one_active_per_owner[\s\S]*WHERE status = 'active'/,
    'one owner cannot accumulate active oracle fixtures',
  );
  assert.match(migration, /REFERENCES public\.apocrypha_job\(id\) ON DELETE RESTRICT/, 'manifest prevents accidental physical deletion');
  assert.match(migration, /owner-chat oracle identity is immutable/, 'run identity cannot be retargeted');
  assert.match(migration, /BEFORE UPDATE ON public\.apocrypha_owner_chat_oracle_job/, 'job ownership evidence is append-only');
  assert.doesNotMatch(migration, /DELETE\s+FROM\s+public\.apocrypha_/i, 'logical cleanup never deletes durable data');
}

function testServerGeneratedSeed(): void {
  const body = functionBody('apocrypha_seed_owner_chat_oracle');
  assert.match(body, /p_nonce uuid[\s\S]*RETURNS TABLE \(run_id uuid, conversation_id uuid, job_id uuid, prompt text\)/);
  assert.doesNotMatch(body.slice(0, body.indexOf('RETURNS TABLE')), /p_(?:run|conversation|job)_id/, 'caller cannot choose durable IDs');
  assert.match(body, /substring\(p_nonce::text, 15, 1\) <> '4'/, 'nonce must be UUIDv4');
  assert.match(body, /pg_advisory_xact_lock/, 'same nonce is serialized');
  assert.match(body, /apocrypha-owner-chat-oracle-owner:/, 'all nonce admissions for one owner share one lock');
  assert.match(body, /owner already has an active browser oracle run[\s\S]*P4091/, 'a second active nonce has a stable conflict code');
  assert.match(body, /principal\.principal_kind = 'owner'/, 'seed requires an active owner');
  assert.match(body, /'primary', 'failed', v_request/, 'seed is a terminal failed turn, never worker-claimable');
  assert.match(body, /'oracle_run_id', v_run\.id/, 'immutable request carries run ownership');
  assert.match(body, /'oracle_synthetic', true/, 'synthetic provenance is explicit');
  assert.match(body, /'memory_scope', 'none'/, 'oracle seed cannot authorize memory access');
  assert.match(body, /'oracle\.synthetic_failure'/, 'synthetic failure receives a non-alerting lifecycle event');
  assert.match(body, /'oracle\.seed', false, false/, 'seed cannot trigger the production alert outbox');
}

function testAtomicRetryRegistration(): void {
  const trigger = functionBody('apocrypha_register_owner_chat_oracle_retry');
  assert.match(
    migration,
    /CREATE TRIGGER apocrypha_owner_chat_oracle_retry_register\s+AFTER INSERT ON public\.apocrypha_job/,
    'retry registration runs inside the durable enqueue transaction',
  );
  assert.match(trigger, /NEW\.request ->> 'oracle_run_id' IS NULL/, 'normal unmarked jobs bypass the oracle trigger');
  assert.match(trigger, /NEW\.request ->> 'retry_of_job_id' IS NULL/, 'seed and retry forms are distinguished');
  assert.match(trigger, /seed_run\.id = v_run_id/, 'even the explicit seed fingerprint must resolve to its run');
  assert.match(trigger, /seed_run\.tenant_id = NEW\.tenant_id/, 'a seed marker cannot cross tenants');
  assert.match(trigger, /seed_run\.owner_principal_id = NEW\.owner_principal_id/, 'a seed marker cannot cross owners');
  assert.match(trigger, /seed_run\.status = 'active'/, 'a cleaned run cannot receive a late synthetic seed');
  assert.match(trigger, /oracle marker is valid only for a synthetic seed or registered retry/, 'marked non-retry spoofing is rejected');
  assert.match(trigger, /oracle_run\.tenant_id = NEW\.tenant_id/, 'run and retry tenant must match');
  assert.match(trigger, /oracle_run\.owner_principal_id = NEW\.owner_principal_id/, 'run and retry owner must match');
  assert.match(trigger, /oracle_run\.status = 'active'/, 'cleaned runs reject late retries');
  assert.match(trigger, /oracle_run\.conversation_id::text = NEW\.request ->> 'conversation_id'/, 'run and retry conversation must match');
  assert.match(trigger, /source_manifest\.job_id = v_retry_of_id/, 'retry source must already belong to the same manifest');
  assert.match(trigger, /source_job\.status = 'failed'/, 'only a failed manifested source can be retried');
  assert.match(trigger, /INSERT INTO public\.apocrypha_owner_chat_oracle_job/, 'manifest write is automatic');
  assert.doesNotMatch(trigger, /ON CONFLICT/, 'a duplicate or cross-run binding aborts enqueue instead of being hidden');
  assert.match(
    migration,
    /REVOKE EXECUTE ON FUNCTION public\.apocrypha_register_owner_chat_oracle_retry\(\)\s+FROM PUBLIC, anon, authenticated, service_role;/,
    'the trigger helper is never directly callable',
  );
}

function testRegistrationAndCleanupFailClosed(): void {
  const register = functionBody('apocrypha_register_owner_chat_oracle_job');
  assert.match(register, /job\.tenant_id = p_tenant_id/);
  assert.match(register, /job\.owner_principal_id = p_owner_principal_id/);
  assert.match(register, /job\.request ->> 'conversation_id' = v_run\.conversation_id::text/);
  assert.match(register, /job\.request ->> 'oracle_run_id' = v_run\.id::text/);
  assert.match(register, /job\.request ->> 'retry_of_job_id' IS NOT NULL/, 'only retry children can be registered');

  const cleanup = functionBody('apocrypha_cleanup_owner_chat_oracle');
  assert.match(cleanup, /job\.request ->> 'oracle_run_id' IS DISTINCT FROM v_run\.id::text/, 'unmarked work blocks cleanup');
  assert.match(cleanup, /manifest\.request_hash = job\.request_hash/, 'cleanup verifies the immutable manifest binding');
  assert.match(cleanup, /job\.status NOT IN \('succeeded', 'failed', 'cancelled'\)/, 'active work blocks cleanup');
  assert.match(cleanup, /SET status = 'cleaned', cleaned_at = now\(\)/, 'cleanup is a logical quarantine');
  assert.doesNotMatch(cleanup, /DELETE/i, 'cleanup has no destructive branch');
}

function testVisibilityAndLeastAuthority(): void {
  const visible = functionBody('apocrypha_owner_chat_conversation_visible');
  assert.match(visible, /oracle_run\.status = 'cleaned'/, 'only cleaned runs are hidden');

  const list = functionBody('apocrypha_list_owner_chat_conversations');
  assert.match(list, /public\.apocrypha_owner_chat_conversation_visible\(/, 'sidebar list applies the same quarantine predicate');
  assert.match(list, /SECURITY INVOKER/, 'normal list retains caller authority');

  assert.match(
    migration,
    /REVOKE ALL ON TABLE[\s\S]*?apocrypha_owner_chat_oracle_run[\s\S]*?FROM PUBLIC, anon, authenticated, service_role;/,
    'even service role cannot inspect or mutate manifests directly',
  );
  for (const name of [
    'apocrypha_seed_owner_chat_oracle',
    'apocrypha_register_owner_chat_oracle_job',
    'apocrypha_cleanup_owner_chat_oracle',
    'apocrypha_owner_chat_conversation_visible',
  ]) {
    assert.match(
      migration,
      new RegExp(`REVOKE EXECUTE ON FUNCTION public\\.${name}\\([\\s\\S]*?FROM PUBLIC, anon, authenticated;`),
      `${name} is not browser-callable`,
    );
    assert.match(
      migration,
      new RegExp(`GRANT EXECUTE ON FUNCTION public\\.${name}\\([\\s\\S]*?TO service_role;`),
      `${name} is server-only`,
    );
  }
  assert.equal(
    (migration.match(/SECURITY DEFINER/g) ?? []).length,
    (migration.match(/SECURITY DEFINER[\s\S]{0,100}SET search_path = pg_catalog, public, extensions/g) ?? []).length,
    'every elevated function pins its search path',
  );
  assert.match(migration, /DO \$verification\$/, 'migration verifies live table and function privileges');
}

testIsolatedManifest();
testServerGeneratedSeed();
testAtomicRetryRegistration();
testRegistrationAndCleanupFailClosed();
testVisibilityAndLeastAuthority();
console.log('apocrypha-owner-oracles-migration.test: 5/5 passed');
