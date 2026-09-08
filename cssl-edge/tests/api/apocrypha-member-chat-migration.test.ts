import { readFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { resolve } from 'node:path';

const foundationMigrationPath = resolve(
  process.cwd(),
  '..',
  'cssl-supabase',
  'migrations',
  '0048_apocrypha_member_chat.sql',
);
const hardeningMigrationPath = resolve(
  process.cwd(),
  '..',
  'cssl-supabase',
  'migrations',
  '0049_apocrypha_member_chat_hardening.sql',
);
const sql = readFileSync(foundationMigrationPath, 'utf8');
const hardeningSql = readFileSync(hardeningMigrationPath, 'utf8');

function assert(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(`assert failed: ${message}`);
}

function includes(fragment: string, message: string): void {
  assert(sql.includes(fragment), message);
}

function functionBody(name: string, source = sql): string {
  const start = source.indexOf(`CREATE OR REPLACE FUNCTION public.${name}`);
  assert(start >= 0, `${name} exists`);
  const next = source.indexOf('CREATE OR REPLACE FUNCTION public.', start + 1);
  return source.slice(start, next >= 0 ? next : source.length);
}

function testAtomicReplayContract(): void {
  const body = functionBody('apocrypha_enqueue_member_chat');
  includes('CREATE TABLE public.apocrypha_member_chat_request', 'immutable request/job binding table exists');
  includes('UNIQUE (tenant_id, principal_id, conversation_id, request_id)', 'replay key is principal and conversation scoped');
  includes('CONSTRAINT apocrypha_member_chat_request_job_unique UNIQUE (job_id)', 'one durable request binds one job');
  includes('UNIQUE (tenant_id, principal_id, conversation_id, turn_sequence)', 'conversation turns have a durable total order');
  includes('BEFORE UPDATE ON public.apocrypha_member_chat_request', 'history binding cannot be rewritten');
  assert(body.includes('pg_advisory_xact_lock'), 'conversation submissions are transactionally serialized');
  assert(body.indexOf('SELECT request.* INTO v_existing') < body.indexOf('WITH recent_requests AS'), 'replay returns before rebuilding a changed history prefix');
  assert(body.includes("job.status NOT IN ('succeeded', 'failed', 'cancelled')"), 'a new turn cannot snapshot incomplete assistant history');
  assert(body.includes('max(request.turn_sequence)'), 'turn order is allocated inside the serialized transaction');
  assert(body.includes('FROM public.apocrypha_enqueue_job('), 'existing durable job transaction is reused');
  assert(body.includes("'apocky_member_chat'"), 'exact member capability is enqueued');
  assert(body.includes("'member-chat:' || p_conversation_id::text"), 'idempotency scope is conversation bound');
  assert(body.includes('v_job.request_hash <> v_request_hash'), 'foreign/colliding job hash fails closed');
  assert(body.includes('v_job.request_hash <> v_existing.request_hash'), 'replay verifies its immutable request/job hash binding');
  assert(body.match(/apocrypha_sha256\(v_job\.request::text\)/g)?.length === 2, 'replay and idempotency adoption both rehash the stored request');
  assert(body.match(/v_job\.model_alias <> p_model_alias/g)?.length === 1, 'first-time idempotency adoption verifies the server-selected model');
  assert(body.match(/v_job\.memory_manifest_hash <> lower\(p_memory_manifest_hash\)/g)?.length === 1, 'first-time idempotency adoption verifies the memory rail');
  assert(body.match(/v_job\.job_role <> 'primary'/g)?.length === 2, 'replay and idempotency adoption both reject non-primary jobs');
  assert(body.match(/v_job\.parent_job_id IS NOT NULL/g)?.length === 2, 'replay and idempotency adoption both reject child jobs');
  assert(body.match(/v_job\.max_attempts <> 3/g)?.length === 2, 'replay and idempotency adoption both verify the retry policy');
}

function testServerProjectedHistory(): void {
  const body = functionBody('apocrypha_enqueue_member_chat');
  assert(body.includes("'conversation_history', v_history"), 'history is assembled inside the enqueue transaction');
  assert(body.includes("'history_source', 'server-projected'"), 'request declares server history provenance');
  assert(body.includes('request.principal_id = v_identity.principal_id'), 'history is principal scoped');
  assert(body.includes('request.conversation_id = p_conversation_id'), 'history is conversation scoped');
  assert(body.includes('LIMIT 10'), 'model history input has a fixed server-side bound');
  assert(!/p_(tenant|principal|history)\b/.test(body.slice(0, body.indexOf('RETURNS TABLE'))), 'enqueue signature accepts no tenant, principal, or history parameter');

  const getBody = functionBody('apocrypha_get_member_chat_job');
  const historyBody = functionBody('apocrypha_list_member_chat_history');
  assert(getBody.includes('left(revision.content, 16384)'), 'single-job assistant output is server bounded');
  assert(historyBody.includes('left(revision.content, 16384)'), 'conversation assistant outputs are server bounded');
  assert(getBody.includes('char_length(revision.content) > 16384'), 'single-job projection reports truncation');
  assert(historyBody.includes('char_length(revision.content) > 16384'), 'history projection reports truncation');
}

function testVerifiedIdentityAndForeignIsolation(): void {
  const ensureBody = functionBody('apocrypha_ensure_member_principal');
  const getBody = functionBody('apocrypha_get_member_chat_job');
  const historyBody = functionBody('apocrypha_list_member_chat_history');
  assert(ensureBody.includes('FROM auth.users AS auth_user'), 'member principal requires a real auth.users row');
  assert(ensureBody.includes("tenant.slug = 'apocky-members'"), 'tenant is selected inside the function');
  assert(ensureBody.includes('FOR SHARE'), 'member principal provisioning does not serialize all members on the shared tenant row');
  assert(ensureBody.includes("principal_kind, display_name"), 'member principal kind is fixed server-side');
  for (const [label, body] of [['job', getBody], ['history', historyBody]] as const) {
    assert(body.includes('principal.auth_user_id = p_verified_auth_user_id'), `${label} projection binds verified auth identity`);
    assert(body.includes('job.owner_principal_id = request.principal_id'), `${label} projection binds the stored job owner`);
    assert(body.includes("job.capability = 'apocky_member_chat'"), `${label} projection excludes other capabilities`);
    assert(body.includes("tenant.slug = 'apocky-members'"), `${label} projection excludes other tenants`);
  }
}

function testLeastAuthority(): void {
  includes('ALTER TABLE public.apocrypha_member_chat_request ENABLE ROW LEVEL SECURITY', 'RLS is enabled');
  includes('FROM PUBLIC, anon, authenticated, service_role;', 'direct table access is revoked even from the shared service role');
  includes('REVOKE EXECUTE ON FUNCTION public.apocrypha_ensure_member_principal(uuid)', 'principal provisioning helper is not externally callable');
  includes('GRANT EXECUTE ON FUNCTION public.apocrypha_enqueue_member_chat', 'service role receives only the member enqueue RPC');
  includes('GRANT EXECUTE ON FUNCTION public.apocrypha_get_member_chat_job', 'service role receives the scoped job projection RPC');
  includes('GRANT EXECUTE ON FUNCTION public.apocrypha_list_member_chat_history', 'service role receives the scoped history projection RPC');
  assert(!/GRANT\s+(?:SELECT|INSERT|UPDATE|DELETE)[\s\S]{0,120}apocrypha_member_chat_request[\s\S]{0,80}TO\s+authenticated/i.test(sql), 'authenticated clients receive no table grant');
  assert(!/GRANT\s+EXECUTE[\s\S]{0,180}apocrypha_(?:enqueue|get|list)_member_chat[\s\S]{0,100}TO\s+(?:PUBLIC|anon|authenticated)/i.test(sql), 'member RPCs are never granted to browser roles');
}

function testForwardOnlyCanonicalConversation(): void {
  assert(
    createHash('sha256').update(sql.replace(/\r\n/g, '\n')).digest('hex')
      === '138d42a0884eb762ab33037d2598808705da903469c05f1c49c639f252e81542',
    'live 0048 foundation remains byte-for-byte unchanged',
  );
  assert(
    hardeningSql.includes("VALUES ('apocky-members', 'Apocky members')"),
    '0049 seeds the active member tenant before first request',
  );
  assert(
    hardeningSql.includes('CREATE TABLE public.apocrypha_member_chat_conversation'),
    'server-owned conversation registry exists',
  );
  assert(
    hardeningSql.includes('PRIMARY KEY (tenant_id, principal_id)'),
    'there is exactly one conversation row per principal',
  );
  assert(
    hardeningSql.includes('CHECK (conversation_id = auth_user_id)'),
    'stable conversation id is the verified auth UUID',
  );
  assert(
    hardeningSql.includes('FOREIGN KEY (tenant_id, principal_id, auth_user_id)'),
    'conversation auth UUID is tied to the same principal row',
  );
  assert(
    hardeningSql.includes('0049 found noncanonical 0048 member history; no rows were rewritten'),
    'noncanonical live history fails forward instead of being rewritten',
  );
  assert(
    hardeningSql.includes('ADD CONSTRAINT apocrypha_member_chat_request_conversation_fk'),
    'all existing and future requests reference the canonical conversation',
  );
  assert(
    !/UPDATE\s+public\.apocrypha_member_chat_request/i.test(hardeningSql),
    '0049 never rewrites immutable 0048 request history',
  );
}

function testV2AdmissionAndQuota(): void {
  const body = functionBody('apocrypha_enqueue_member_chat_v2', hardeningSql);
  assert(
    body.includes('p_presented_conversation_id <> p_verified_auth_user_id'),
    'browser conversation UUID is validation only',
  );
  assert(
    body.includes("'apocky-member-chat-principal:' || v_identity.principal_id::text"),
    'all admissions for one principal share one transaction lock',
  );
  assert(
    body.indexOf('IF FOUND THEN') < body.indexOf("job.status IN ('queued', 'leased', 'running', 'cancel_requested')"),
    'idempotent replay precedes the active-work cap',
  );
  assert(
    body.indexOf('IF FOUND THEN') < body.indexOf("request.created_at >= now() - interval '1 hour'"),
    'idempotent replay precedes rolling quota admission',
  );
  assert(body.includes("USING ERRCODE = 'P4091'"), 'active-principal cap has a stable database code');
  assert(body.includes('IF v_recent_count >= 30 THEN'), 'rolling quota is capped at 30 accepted turns');
  assert(body.includes("interval '1 hour'"), 'quota uses a durable rolling one-hour window');
  assert(body.includes("USING ERRCODE = 'P4290'"), 'quota has a stable database code');
  assert(body.includes("~ '[[:cntrl:]]'"), 'database rejects disallowed control characters');
  assert(
    body.includes('v_identity.conversation_id'),
    'legacy atomic enqueue receives only the server-derived conversation id',
  );
}

function testV2HistoryPagination(): void {
  const body = functionBody('apocrypha_list_member_chat_history_v2', hardeningSql);
  assert(body.includes('p_before_turn_sequence text DEFAULT NULL'), 'cursor crosses JSON losslessly as text');
  assert(body.includes('p_limit integer DEFAULT 50'), 'history default remains 50 rows');
  assert(body.includes('p_limit < 1 OR p_limit > 50'), 'history page size is server capped');
  assert(body.includes('request.turn_sequence < v_before'), 'history cursor is exclusive and overlap free');
  assert(body.includes('LIMIT (p_limit + 1)'), 'one extra row determines continuation');
  assert(body.includes('page_rows.turn_sequence::text'), 'continuation cursor returns losslessly');
  assert(body.includes('count(*) > p_limit AS has_more'), 'database reports older durable rows');
  assert(body.includes('ORDER BY page_rows.turn_sequence ASC'), 'each page is chronological');
}

function testV2LeastAuthority(): void {
  assert(
    hardeningSql.includes('REVOKE ALL ON TABLE public.apocrypha_member_chat_conversation'),
    'conversation registry has no direct shared-role access',
  );
  assert(
    /REVOKE EXECUTE ON FUNCTION public\.apocrypha_enqueue_member_chat\([\s\S]*?FROM service_role;/m.test(hardeningSql),
    'service role loses client-selected legacy enqueue',
  );
  assert(
    /REVOKE EXECUTE ON FUNCTION public\.apocrypha_list_member_chat_history\([\s\S]*?FROM service_role;/m.test(hardeningSql),
    'service role loses client-selected legacy history',
  );
  assert(
    /GRANT EXECUTE ON FUNCTION public\.apocrypha_enqueue_member_chat_v2\([\s\S]*?TO service_role;/m.test(hardeningSql),
    'service role receives canonical v2 enqueue only',
  );
  assert(
    /GRANT EXECUTE ON FUNCTION public\.apocrypha_list_member_chat_history_v2\([\s\S]*?TO service_role;/m.test(hardeningSql),
    'service role receives canonical paged history',
  );
  assert(
    hardeningSql.includes('DO $verification$'),
    'migration executes live postconditions before completion',
  );
  assert(
    hardeningSql.includes("'public.apocrypha_get_member_chat_job(uuid,uuid)'"),
    'live privilege verification preserves scoped job reads',
  );
  assert(
    hardeningSql.includes('canonical member conversation backfill is incomplete'),
    'live row verification rejects an incomplete canonical backfill',
  );
}

testAtomicReplayContract();
testServerProjectedHistory();
testVerifiedIdentityAndForeignIsolation();
testLeastAuthority();
testForwardOnlyCanonicalConversation();
testV2AdmissionAndQuota();
testV2HistoryPagination();
testV2LeastAuthority();
console.log('apocrypha-member-chat-migration.test: 8/8 passed');
