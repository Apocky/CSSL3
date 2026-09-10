// What an upstream worker failure is allowed to write into a log.
//
// Vercel showed apocrypha_claim_job failing eight times over two days with
// `database_code: "unknown"` and `""` and nothing else. An absent SQLSTATE
// means the call never reached PostgreSQL - a fetch failure, a timeout, a
// gateway error - and the message that would have said which was computed for
// classification and then discarded.
//
// So the detail is now kept. These tests pin the two halves of that: that it
// survives at all, and that it cannot carry a credential into a log line,
// because a log line is forever and ends up in places a secret should not be.
//
// Tested through `workerDatabaseError`, which is already exported, rather than
// by exporting the redactor - the boundary that matters is what a caller can
// observe, and widening the module's surface to test it would be the tail
// wagging the dog.

import { strict as assert } from 'node:assert';

import { workerDatabaseError } from '@/lib/apocrypha/worker-http';

interface Detailed extends Error {
  detail: string;
  databaseCode: string;
  operation: string;
}

function build(code: string | null, message: string | null): Detailed {
  return workerDatabaseError({ code, message }) as Detailed;
}

function main(): void {
  // ── the detail survives ────────────────────────────────────────────────
  const transport = build(null, 'TypeError: fetch failed');
  assert.equal(
    transport.databaseCode,
    'unknown',
    'an absent SQLSTATE is recorded as unknown, which is the signal that the database was never reached',
  );
  assert.equal(
    transport.detail,
    'TypeError: fetch failed',
    'the upstream message must survive classification - discarding it is what made this undiagnosable',
  );

  const empty = build('', 'upstream request timeout');
  assert.equal(empty.detail, 'upstream request timeout', 'an empty code still keeps its message');

  // ── it cannot carry a credential ───────────────────────────────────────
  const jwt = build('28000', 'auth failed for eyJhbGciOiJIUzI1NiJ9.abcdefghijklmnop');
  assert.ok(!jwt.detail.includes('eyJhbGci'), `a JWT reached the log: ${jwt.detail}`);
  assert.ok(jwt.detail.includes('<jwt>'), jwt.detail);

  const dsn = build('08006', 'could not connect to postgresql://postgres.abc:hunter2@db.host:5432/postgres');
  assert.ok(!dsn.detail.includes('hunter2'), `a password reached the log: ${dsn.detail}`);
  assert.ok(dsn.detail.includes('<dsn>'), dsn.detail);

  const key = build(null, 'rejected key sb_secret_abc123XYZ_deadbeef');
  assert.ok(!key.detail.includes('sb_secret_abc'), `a secret key reached the log: ${key.detail}`);

  const url = build(null, 'fetch failed https://pzirbmyfmrbtkllrtcmx.supabase.co/rest/v1/rpc/x');
  assert.ok(!url.detail.includes('supabase.co'), `an endpoint reached the log: ${url.detail}`);

  // ── it is bounded ──────────────────────────────────────────────────────
  // An unbounded upstream string is an unbounded log line, and a database can
  // quote a whole row back at you in an error.
  const huge = build('23505', 'x'.repeat(5000));
  assert.ok(huge.detail.length <= 300, `detail was ${huge.detail.length} chars`);

  const noisy = build(null, '  line one\n\n   line two\t\tline three  ');
  assert.equal(noisy.detail, 'line one line two line three', 'whitespace is collapsed so one failure is one log line');

  assert.equal(build(null, null).detail, '', 'a missing message is empty, not the string "null"');

  // ── classification is unchanged ────────────────────────────────────────
  // The message is what the response shape depends on, so keeping the detail
  // must not have started leaking prose into it.
  assert.equal(
    build('28000', 'worker authentication failed').message,
    'WORKER_UNAUTHORIZED',
    'an auth failure still classifies as WORKER_UNAUTHORIZED',
  );
  assert.equal(
    build('57014', 'worker lease expired').message,
    'WORKER_FENCE_LOST',
    'a lost fence still classifies as WORKER_FENCE_LOST',
  );
  assert.equal(
    build('42883', 'function does not exist').message,
    'WORKER_RPC_FAILED:42883',
    'an unrecognised failure still classifies by code alone, carrying no prose',
  );

  console.log('apocrypha-worker-http.test: upstream detail is kept, redacted, bounded, and classification is unchanged');
}

main();
