import { strict as assert } from 'node:assert';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

const migration = readFileSync(
  resolve(process.cwd(), '..', 'cssl-supabase', 'migrations', '0046_apocrypha_sms.sql'),
  'utf8',
);

function sqlSection(start: string, end: string): string {
  const startIndex = migration.indexOf(start);
  const endIndex = migration.indexOf(end, startIndex + start.length);
  assert.notEqual(startIndex, -1, `missing SQL section start: ${start}`);
  assert.notEqual(endIndex, -1, `missing SQL section end: ${end}`);
  return migration.slice(startIndex, endIndex);
}

assert.match(migration, /CREATE TABLE IF NOT EXISTS public\.apocrypha_sms_channels/);
assert.match(migration, /CREATE TABLE IF NOT EXISTS public\.apocrypha_sms_messages/);
assert.match(migration, /CREATE TABLE IF NOT EXISTS public\.apocrypha_sms_delivery_events/);
assert.match(migration, /ingest_apocrypha_sms_message/);
assert.match(migration, /claim_apocrypha_sms_job/);
assert.match(migration, /claim_apocrypha_sms_send/);
assert.match(migration, /record_apocrypha_sms_delivery/);
assert.equal((migration.match(/ENABLE ROW LEVEL SECURITY/g) ?? []).length, 3);

// Storage is service-role-only: PUBLIC/browser roles get neither tables nor RPCs.
for (const table of ['channels', 'messages', 'delivery_events']) {
  assert.match(
    migration,
    new RegExp(`REVOKE ALL ON TABLE public\\.apocrypha_sms_${table} FROM PUBLIC, anon, authenticated`),
  );
}
assert.match(
  migration,
  /GRANT SELECT, INSERT, UPDATE ON TABLE public\.apocrypha_sms_messages TO service_role/,
);
assert.match(
  migration,
  /GRANT SELECT, INSERT ON TABLE public\.apocrypha_sms_delivery_events TO service_role/,
);
assert.doesNotMatch(
  migration,
  /GRANT[^;]*(?:UPDATE|DELETE)[^;]*apocrypha_sms_delivery_events/i,
  'delivery-event evidence must be append-only for the application role',
);
assert.doesNotMatch(
  migration,
  /^GRANT[^\r\n]*\sTO\s+(?:anon|authenticated|PUBLIC)(?:\s*[,;]|$)/im,
);

// No direct identifiers or plaintext message columns cross the persistence boundary.
assert.doesNotMatch(
  migration,
  /\b(?:from_phone|to_phone|phone_number|body_plaintext|reply_plaintext)\b/i,
  'schema must not persist raw phone numbers or plaintext message bodies',
);
assert.match(migration, /phone_hash\s+text PRIMARY KEY/);
assert.match(migration, /consent_generation\s+bigint NOT NULL DEFAULT 0/);
assert.match(
  migration,
  /UNIQUE \(\s*provider,\s*provider_account_sid,\s*provider_message_sid\s*\)/,
  'provider/account/message tuple must be the durable inbound idempotency key',
);

// Both legacy and keyed/rotatable AES-GCM envelope versions are admitted, but no loose prefix is.
assert.match(migration, /body_ciphertext ~ '\^\(v1\[\.\].*\|v2\[\.\]/);
assert.match(migration, /reply_ciphertext ~ '\^\(v1\[\.\].*\|v2\[\.\]/);
assert.match(migration, /char_length\(body_ciphertext\) BETWEEN 32 AND 32768/);
assert.doesNotMatch(migration, /body_ciphertext (?:LIKE|NOT LIKE) 'v1\.%'/);

// Provider START is only a carrier state. Local activation requires a prior,
// version-matching disclosure presentation and a later explicit consent command.
assert.match(migration, /disclosure_presented_at\s+timestamptz/);
assert.match(migration, /disclosure_presented_method\s+text/);
assert.match(migration, /consent_disclosure_sha256/);
const startBranch = sqlSection("ELSIF p_command_kind = 'start'", "ELSIF p_command_kind = 'consent'");
assert.match(startBranch, /v_consent := 'carrier_started'/);
assert.doesNotMatch(startBranch, /v_consent := 'active'/);
const consentBranch = sqlSection("ELSIF p_command_kind = 'consent'", "ELSIF p_command_kind = 'help'");
assert.match(
  consentBranch,
  /v_presented_digest = p_consent_disclosure_sha256\s+AND v_presented_at IS NOT NULL/,
);
assert.match(consentBranch, /v_action := 'consent_required'/);
assert.match(consentBranch, /disclosure_presented_method = 'sms:consent_required'/);
assert.match(consentBranch, /consent_method = 'sms:CONSENT_APOCRYPHA'/);
const disclosureInvalidation = sqlSection(
  "IF v_consent = 'active'",
  "IF p_command_kind = 'stop'",
);
assert.match(disclosureInvalidation, /v_presented_digest IS DISTINCT FROM p_consent_disclosure_sha256/);
assert.match(disclosureInvalidation, /consent_generation = consent_generation \+ 1/);

// Commands are evaluated before media. Unsupported media is durably terminal
// and never reaches the runtime queue; all non-command ingress shares the DB rate limit.
assert.match(migration, /p_media_count integer/);
assert.match(migration, /media_count\s+integer NOT NULL DEFAULT 0 CHECK \(media_count BETWEEN 0 AND 10\)/);
assert.match(migration, /v_action := 'media_unsupported'/);
assert.match(migration, /v_status := 'media_unsupported'/);
assert.ok(
  migration.indexOf("IF p_command_kind = 'stop'") < migration.indexOf('ELSIF p_media_count > 0'),
  'STOP/START/CONSENT/HELP must be handled before media rejection',
);
assert.match(migration, /m\.created_at >= now\(\) - interval '60 seconds'/);
assert.match(migration, /IF v_recent >= 4 THEN/);

// STOP revokes locally and cancels every not-yet-dispatched state. `sending`
// is intentionally excluded: rewriting an in-flight provider call would hide ambiguity.
const stopBranch = sqlSection("IF p_command_kind = 'stop'", "ELSIF p_command_kind = 'start'");
assert.match(stopBranch, /consent_state = 'revoked'/);
assert.match(stopBranch, /consent_generation = consent_generation \+ 1/);
assert.match(stopBranch, /status IN \('queued','processing','ready_to_send'\)/);
assert.doesNotMatch(stopBranch, /status IN \([^)]*'sending'/);
assert.match(stopBranch, /error_code = 'consent_revoked'/);

// Work and dispatch claims lease with SKIP LOCKED. Dispatch re-locks the
// channel, re-checks consent, and charges the day of dispatch rather than ingress.
const jobClaim = sqlSection(
  'CREATE OR REPLACE FUNCTION public.claim_apocrypha_sms_job',
  'CREATE OR REPLACE FUNCTION public.mark_apocrypha_sms_job_ready',
);
assert.match(jobClaim, /FOR UPDATE OF m SKIP LOCKED/);
assert.match(jobClaim, /m\.status = 'processing'/);
assert.match(jobClaim, /m\.leased_at <= now\(\) - interval '5 minutes'/);
assert.match(jobClaim, /\(m\.status = 'processing'\) AS reconcile_only/);
assert.match(jobClaim, /candidate\.reconcile_only/);
assert.match(jobClaim, /lease_token = gen_random_uuid\(\)/);
assert.match(jobClaim, /m\.lease_token/);
assert.doesNotMatch(jobClaim, /SET status = 'queued'/, 'stale work must be reconciled, never requeued');
const readyTransition = sqlSection(
  'CREATE OR REPLACE FUNCTION public.mark_apocrypha_sms_job_ready',
  'CREATE OR REPLACE FUNCTION public.mark_apocrypha_sms_runtime_failed',
);
assert.match(readyTransition, /p_lease_token uuid/);
assert.match(readyTransition, /m\.lease_token = p_lease_token/);
assert.match(readyTransition, /lease_token = p_lease_token/);
const failedTransition = sqlSection(
  'CREATE OR REPLACE FUNCTION public.mark_apocrypha_sms_runtime_failed',
  'CREATE OR REPLACE FUNCTION public.claim_apocrypha_sms_send',
);
assert.match(failedTransition, /p_lease_token uuid/);
assert.match(failedTransition, /lease_token = p_lease_token/);
const sendClaim = sqlSection(
  'CREATE OR REPLACE FUNCTION public.claim_apocrypha_sms_send',
  'CREATE OR REPLACE FUNCTION public.authorize_apocrypha_sms_send',
);
assert.match(sendClaim, /WHERE m\.status = 'ready_to_send'/);
assert.match(sendClaim, /SET status = 'uncertain'/);
assert.match(sendClaim, /error_code = 'send_worker_lost'/);
assert.match(sendClaim, /WITH stale_channels AS MATERIALIZED/);
assert.match(sendClaim, /ORDER BY c\.phone_hash\s+FOR UPDATE OF c/);
assert.match(sendClaim, /m\.status = 'sending'/);
assert.match(sendClaim, /m\.outbound_message_sid IS NULL/);
assert.match(sendClaim, /m\.leased_at <= now\(\) - interval '5 minutes'/);
assert.doesNotMatch(sendClaim, /SET status = 'ready_to_send'/, 'lost sends must never be requeued');
assert.match(sendClaim, /WHERE c\.phone_hash = v_candidate_phone_hash\s+FOR UPDATE/);
assert.match(sendClaim, /FOR UPDATE SKIP LOCKED/);
assert.match(sendClaim, /IF v_consent <> 'active' THEN/);
assert.match(migration, /daily_segment_budget/);
assert.match(sendClaim, /m\.dispatched_at >= date_trunc\('day', now\(\)\)/);
assert.match(sendClaim, /m\.phone_hash = v_job\.phone_hash/);
assert.match(sendClaim, /dispatched_at = now\(\)/);
assert.match(sendClaim, /dispatch_token = gen_random_uuid\(\)/);
assert.match(sendClaim, /dispatch_consent_generation = v_consent_generation/);
assert.match(sendClaim, /m\.dispatch_token/);
assert.doesNotMatch(sendClaim, /m\.created_at >= date_trunc\('day'/);
const sendAuthorization = sqlSection(
  'CREATE OR REPLACE FUNCTION public.authorize_apocrypha_sms_send',
  'CREATE OR REPLACE FUNCTION public.project_apocrypha_sms_delivery',
);
assert.match(sendAuthorization, /p_dispatch_token uuid/);
assert.match(sendAuthorization, /WHERE c\.phone_hash = v_phone_hash\s+FOR UPDATE/);
assert.match(sendAuthorization, /v_consent_generation IS DISTINCT FROM v_dispatch_consent_generation/);
assert.match(sendAuthorization, /error_code = 'consent_generation_stale'/);
assert.match(sendAuthorization, /dispatch_token = p_dispatch_token/);

// Callback rows remain append-only and deduplicated. Current state is projected
// by semantic precedence, including callbacks that arrive before SID binding.
assert.match(migration, /event_fingerprint\s+text NOT NULL UNIQUE/);
assert.match(migration, /jsonb_build_array\(p_outbound_message_sid, p_provider_status, p_error_code\)::text/);
assert.doesNotMatch(migration, /E'\\000'/, 'PostgreSQL text cannot carry a NUL delimiter');
const projection = sqlSection(
  'CREATE OR REPLACE FUNCTION public.project_apocrypha_sms_delivery',
  'CREATE OR REPLACE FUNCTION public.record_apocrypha_sms_sent',
);
assert.match(projection, /WHEN 'delivered' THEN 50/);
assert.match(projection, /WHEN 'undelivered' THEN 40/);
assert.match(
  projection,
  /WHEN v_provider_status IN \('accepted','scheduled','queued','sending','sent','delivered','read'\) THEN NULL/,
  'a positive semantic winner must clear a stale lower-priority provider error',
);
const sentReceipt = sqlSection(
  'CREATE OR REPLACE FUNCTION public.record_apocrypha_sms_sent',
  'CREATE OR REPLACE FUNCTION public.record_apocrypha_sms_send_failure',
);
assert.match(sentReceipt, /p_dispatch_token uuid/);
assert.match(sentReceipt, /dispatch_token = p_dispatch_token/);
assert.match(sentReceipt, /status = 'sending'/);
assert.match(sentReceipt, /status = 'uncertain'/);
assert.match(sentReceipt, /error_code = 'send_worker_lost'/);
assert.match(sentReceipt, /outbound_message_sid IS NULL/);
assert.match(sentReceipt, /error_code = NULL/);
assert.match(sentReceipt, /PERFORM public\.project_apocrypha_sms_delivery/);
const sendFailure = sqlSection(
  'CREATE OR REPLACE FUNCTION public.record_apocrypha_sms_send_failure',
  'CREATE OR REPLACE FUNCTION public.record_apocrypha_sms_delivery',
);
assert.match(sendFailure, /p_dispatch_token uuid/);
assert.match(sendFailure, /dispatch_token = p_dispatch_token/);

// An outbound timeout or lost worker can be terminally `uncertain`; only ready
// rows are dispatch-claimed, so the database supplies no blind ambiguous resend.
assert.match(migration, /p_outcome NOT IN \('failed','uncertain'\)/);
assert.doesNotMatch(sendClaim, /(?:WHERE|OR)\s+m\.status\s*=\s*'uncertain'/);
assert.match(migration, /status IN \('queued','processing'/);
assert.match(migration, /UPDATE public\.apocrypha_sms_messages\s+SET status = 'suppressed'/);

assert.match(
  migration,
  /ingest_apocrypha_sms_message\(text,text,text,text,uuid,uuid,text,text,integer,text\)/,
  'GRANT/REVOKE signature must match the account- and media-aware ingress RPC',
);
assert.match(
  migration,
  /mark_apocrypha_sms_job_ready\(uuid,uuid,text,text,integer\)/,
  'ready RPC grants must require the current processing lease token',
);
assert.match(
  migration,
  /mark_apocrypha_sms_runtime_failed\(uuid,uuid,text\)/,
  'failure RPC grants must require the current processing lease token',
);
assert.match(migration, /authorize_apocrypha_sms_send\(uuid,uuid\)/);
assert.match(migration, /record_apocrypha_sms_sent\(uuid,uuid,text,text\)/);
assert.match(migration, /record_apocrypha_sms_send_failure\(uuid,uuid,text,text\)/);

// eslint-disable-next-line no-console
console.log('apocrypha-sms-migration.test : OK · consent, idempotency, queue, budget, and delivery contracts present');
