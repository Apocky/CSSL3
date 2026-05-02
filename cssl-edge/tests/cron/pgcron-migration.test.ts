// § T11-W14-K · tests/cron/pgcron-migration.test.ts
// Validates 0036_cloud_orchestrator_pgcron.sql shape :
//   - file exists
//   - declares 3 mission-required pg_cron jobs by name
//   - defines rollup_promote_minutes_v2() TABLE wrapper
//   - defines cleanup_old_analytics_events() helper
//   - defines orchestrator_heartbeat VIEW
//   - declares 90-day retention guard
//   - graceful when pg_cron extension unavailable
//
// We grep the SQL source rather than attempting Postgres execution ; the
// integration-test path is covered by Supabase CLI's `supabase db reset`
// in CI. This test ensures the SQL ARTIFACT contains the canonical
// statements before we ever ship it to Supabase.

import * as fs from 'node:fs';
import * as path from 'node:path';

function assert(cond: boolean, msg: string): void {
  if (!cond) throw new Error(`assert failed : ${msg}`);
}

const ROOT = path.resolve(__dirname, '..', '..', '..');
const MIGRATION_PATH = path.join(
  ROOT,
  'cssl-supabase',
  'migrations',
  '0036_cloud_orchestrator_pgcron.sql'
);

function readMigration(): string {
  return fs.readFileSync(MIGRATION_PATH, 'utf-8');
}

// 1. File exists.
function testMigrationExists(): void {
  assert(fs.existsSync(MIGRATION_PATH), '0036 migration must exist');
  const stat = fs.statSync(MIGRATION_PATH);
  assert(stat.size > 1000, '0036 migration should be > 1 KB');
}

// 2. Declares 3 mission-required pg_cron jobs by canonical name.
function testCanonicalJobsDeclared(): void {
  const src = readMigration();
  for (const jobName of [
    'cleanup_old_events',
    'analytics_rollup_promote',
    'vacuum_analytics_events',
  ]) {
    assert(
      src.includes(`'${jobName}'`),
      `pg_cron job '${jobName}' must be scheduled`
    );
  }
}

// 3. Defines rollup_promote_minutes_v2() TABLE wrapper.
function testWrapperFnDeclared(): void {
  const src = readMigration();
  assert(
    src.includes('FUNCTION public.rollup_promote_minutes_v2()'),
    'rollup_promote_minutes_v2() must be declared'
  );
  assert(
    src.includes('promoted_to_1hr') && src.includes('promoted_to_1day'),
    'wrapper must return promoted_to_1hr + promoted_to_1day'
  );
}

// 4. Defines cleanup_old_analytics_events() helper.
function testCleanupFnDeclared(): void {
  const src = readMigration();
  assert(
    src.includes('FUNCTION public.cleanup_old_analytics_events()'),
    'cleanup_old_analytics_events() must be declared'
  );
  assert(
    src.includes("interval '90 days'"),
    '90-day retention window must be enforced'
  );
}

// 5. Defines orchestrator_heartbeat VIEW.
function testHeartbeatViewDeclared(): void {
  const src = readMigration();
  assert(
    src.includes('VIEW public.orchestrator_heartbeat'),
    'orchestrator_heartbeat VIEW must be defined'
  );
  assert(
    src.includes('cron_executions') && src.includes('cron_heartbeat'),
    'view must combine cron_executions + cron_heartbeat'
  );
}

// 6. Declares retry-backoff helper.
function testRetryBackoffFn(): void {
  const src = readMigration();
  assert(
    src.includes('FUNCTION public.cron_retry_schedule('),
    'cron_retry_schedule() must be declared'
  );
  assert(
    src.includes('exponential backoff') ||
      src.includes('LEAST(3600') ||
      src.includes('1 << '),
    'must implement exponential-backoff (cap 1h)'
  );
}

// 7. Graceful when pg_cron unavailable.
function testGracefulNoPgCron(): void {
  const src = readMigration();
  assert(
    src.includes('pg_cron unavailable') ||
      src.includes('pg_cron not installed'),
    'must skip job-creation when pg_cron extension unavailable'
  );
  assert(
    src.includes('IF NOT has_pg_cron'),
    'must gate job-creation on extension presence'
  );
}

// 8. Public-readable views (transparency-axiom).
function testPublicGrants(): void {
  const src = readMigration();
  assert(
    src.includes('GRANT SELECT ON public.orchestrator_heartbeat'),
    'heartbeat view must be public-readable'
  );
  assert(
    src.includes('GRANT SELECT ON public.orchestrator_failure_summary'),
    'failure-summary view must be public-readable'
  );
  // anonymous-role grant
  assert(
    src.includes('TO anon'),
    'must grant SELECT to anon role for transparency'
  );
}

// 9. Idempotency : OR REPLACE / IF NOT EXISTS guards.
function testIdempotency(): void {
  const src = readMigration();
  assert(
    src.includes('CREATE OR REPLACE FUNCTION'),
    'functions must use CREATE OR REPLACE'
  );
  assert(
    src.includes('CREATE EXTENSION IF NOT EXISTS pgcrypto'),
    'extensions guarded by IF NOT EXISTS'
  );
  assert(
    src.includes('cron.unschedule(jobid)'),
    'pg_cron jobs unscheduled-then-rescheduled (idempotent)'
  );
}

// 10. Service-role isolation (no anon-write paths).
function testServiceRoleIsolation(): void {
  const src = readMigration();
  // Mission requirement : service-role-key isolated.
  // The migration MUST NOT grant INSERT/UPDATE/DELETE to anon.
  // Heuristic : count the GRANT lines and ensure none of the writes are to anon.
  const grantLines = src
    .split('\n')
    .filter((l) => l.trim().startsWith('GRANT '));
  for (const line of grantLines) {
    if (
      line.includes(' INSERT ') ||
      line.includes(' UPDATE ') ||
      line.includes(' DELETE ')
    ) {
      assert(
        !line.includes('TO anon'),
        `anon must not have write-grants : ${line}`
      );
    }
  }
}

declare const require: { main?: unknown } | undefined;
declare const module: { id?: string } | undefined;
const isMain =
  typeof require !== 'undefined' &&
  typeof module !== 'undefined' &&
  require.main === module;

function runAll(): void {
  testMigrationExists();
  testCanonicalJobsDeclared();
  testWrapperFnDeclared();
  testCleanupFnDeclared();
  testHeartbeatViewDeclared();
  testRetryBackoffFn();
  testGracefulNoPgCron();
  testPublicGrants();
  testIdempotency();
  testServiceRoleIsolation();
  // eslint-disable-next-line no-console
  console.log('pgcron-migration.test : OK · 10 tests passed');
}

if (isMain) {
  try {
    runAll();
  } catch (err) {
    // eslint-disable-next-line no-console
    console.error(err);
    process.exit(1);
  }
}

export { runAll };
