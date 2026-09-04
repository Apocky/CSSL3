import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

const migration = readFileSync(
  resolve(process.cwd(), '..', 'cssl-supabase', 'migrations', '0048_mini_brain_relay_state.sql'),
  'utf8',
);

for (const table of [
  'mini_brain_relay_device_state',
  'mini_brain_relay_request_ledger',
  'mini_brain_relay_sequence_ledger',
  'mini_brain_relay_rate_state',
]) {
  assert.match(migration, new RegExp(`CREATE TABLE IF NOT EXISTS public\\.${table}`));
  assert.match(migration, new RegExp(`ALTER TABLE public\\.${table} ENABLE ROW LEVEL SECURITY`));
  assert.match(
    migration,
    new RegExp(`REVOKE ALL ON TABLE public\\.${table}\\s+FROM PUBLIC, anon, authenticated, service_role`),
  );
}

assert.match(
  migration,
  /PRIMARY KEY \(owner_ref, device_id, sequence\)/,
  'every signed sequence must have one durable envelope binding',
);
assert.match(
  migration,
  /PRIMARY KEY \(owner_ref, request_id\)/,
  'request identity must be unique across every device owned by one principal',
);
assert.match(migration, /logical_digest\s+text\s+NOT NULL/);
assert.match(migration, /envelope_digest\s+text\s+NOT NULL/);
assert.match(migration, /pg_advisory_xact_lock/);
assert.match(migration, /FOR UPDATE/g);
assert.match(migration, /v_request_state\.logical_digest <> p_logical_digest/);
assert.match(migration, /latest_sequence = p_sequence/);
assert.match(migration, /v_outcome := 'identical_retry'/);
assert.match(migration, /request_count >= 30/);
assert.match(migration, /interval '60 seconds'/);
assert.match(migration, /interval '35 days'/);
assert.match(migration, /cleanup_mini_brain_relay_state/);
assert.match(migration, /LIMIT p_limit/g);
assert.match(migration, /LIMIT 256/g);
assert.match(migration, /LIMIT 64/);
assert.match(migration, /NOT EXISTS \(\s+SELECT 1\s+FROM public\.mini_brain_relay_sequence_ledger/s);
assert.match(migration, /NOT EXISTS \(\s+SELECT 1\s+FROM public\.mini_brain_relay_request_ledger/s);

const idempotencyDecision = migration.indexOf("v_outcome := 'identical_retry'");
const stateMutation = migration.indexOf('INSERT INTO public.mini_brain_relay_device_state');
assert.ok(idempotencyDecision > 0 && idempotencyDecision < stateMutation);

assert.match(migration, /LANGUAGE plpgsql\s+SECURITY DEFINER\s+SET search_path = pg_catalog, public/g);
assert.match(
  migration,
  /GRANT EXECUTE ON FUNCTION public\.admit_mini_brain_relay_request\(text,uuid,text,bigint,uuid,text,text\)\s+TO service_role/,
);
assert.match(
  migration,
  /REVOKE ALL ON FUNCTION public\.admit_mini_brain_relay_request\(text,uuid,text,bigint,uuid,text,text\)\s+FROM PUBLIC, anon, authenticated/,
);
assert.doesNotMatch(
  migration,
  /^GRANT[^\r\n]*\sTO\s+(?:PUBLIC|anon|authenticated)(?:\s*[,;]|$)/gim,
);
assert.doesNotMatch(
  migration,
  /\b(?:prompt|payload|content|message_body|body_plaintext)\s+(?:text|jsonb)\b/i,
  'relay state may persist request identity/digests, never raw prompt content',
);

console.log('mobile-relay-state-migration.test : OK · atomic dual-ledger retry/rate + bounded cleanup + no prompt column');
