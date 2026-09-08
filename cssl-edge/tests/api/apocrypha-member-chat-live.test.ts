import { randomUUID } from 'node:crypto';

import { createClient } from '@supabase/supabase-js';

// Post-deploy probe: point the three MEMBER_CHAT_LIVE_* variables below at a
// dedicated existing auth user, then run this file with `node --import tsx`.
// It performs only reads, expected validation failures, and idempotent stable
// conversation provisioning; it never enqueues or deletes a job.

const REQUIRED_ENV = [
  'MEMBER_CHAT_LIVE_SUPABASE_URL',
  'MEMBER_CHAT_LIVE_SUPABASE_SERVICE_ROLE_KEY',
  'MEMBER_CHAT_LIVE_AUTH_USER_ID',
] as const;
const UUID_RE = /^[0-9a-f]{8}-[0-9a-f]{4}-[1-5][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i;

function assert(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(`assert failed: ${message}`);
}

function record(value: unknown): Record<string, unknown> | null {
  return value !== null && typeof value === 'object' && !Array.isArray(value)
    ? value as Record<string, unknown>
    : null;
}

async function main(): Promise<void> {
  const configured = REQUIRED_ENV.filter((name) => Boolean(process.env[name]));
  if (configured.length === 0) {
    console.log('apocrypha-member-chat-live.test: skipped (dedicated live fixture not configured)');
    return;
  }
  assert(
    configured.length === REQUIRED_ENV.length,
    `configure all dedicated live fixture variables: ${REQUIRED_ENV.join(', ')}`,
  );

  const url = process.env.MEMBER_CHAT_LIVE_SUPABASE_URL!;
  const serviceRoleKey = process.env.MEMBER_CHAT_LIVE_SUPABASE_SERVICE_ROLE_KEY!;
  const verifiedAuthUserId = process.env.MEMBER_CHAT_LIVE_AUTH_USER_ID!.toLowerCase();
  assert(/^https?:\/\//.test(url), 'live Supabase URL is HTTP(S)');
  assert(serviceRoleKey.length >= 20, 'live service-role credential has a plausible shape');
  assert(UUID_RE.test(verifiedAuthUserId), 'live auth fixture is a UUID');

  const client = createClient(url, serviceRoleKey, {
    auth: {
      persistSession: false,
      autoRefreshToken: false,
      detectSessionInUrl: false,
    },
  });

  // The canonical read may provision the one stable conversation row for the
  // explicitly configured fixture user. It never creates a job or deletes data.
  const canonical = await client.rpc('apocrypha_list_member_chat_history_v2', {
    p_verified_auth_user_id: verifiedAuthUserId,
    p_presented_conversation_id: verifiedAuthUserId,
    p_before_turn_sequence: null,
    p_limit: 1,
  });
  assert(!canonical.error, `canonical v2 history RPC succeeds: ${canonical.error?.code ?? 'unknown'}`);
  const canonicalRows = Array.isArray(canonical.data) ? canonical.data : [];
  assert(canonicalRows.length <= 1, 'live history obeys requested page bound');
  for (const value of canonicalRows) {
    const row = record(value);
    assert(row?.conversation_id === verifiedAuthUserId, 'live history is canonical-auth bound');
    assert(typeof row.turn_cursor === 'string', 'live cursor is lossless text');
    assert(typeof row.has_more === 'boolean', 'live continuation state is explicit');
  }

  const mismatchId = verifiedAuthUserId === 'aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa'
    ? 'bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb'
    : 'aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa';
  const mismatch = await client.rpc('apocrypha_list_member_chat_history_v2', {
    p_verified_auth_user_id: verifiedAuthUserId,
    p_presented_conversation_id: mismatchId,
    p_before_turn_sequence: null,
    p_limit: 1,
  });
  assert(mismatch.error?.code === 'P4031', 'live v2 history rejects a client-selected conversation');

  // Validation happens before provisioning/enqueue, so this exercises the
  // database control-character rail without creating durable work.
  const invalidControl = await client.rpc('apocrypha_enqueue_member_chat_v2', {
    p_verified_auth_user_id: verifiedAuthUserId,
    p_presented_conversation_id: verifiedAuthUserId,
    p_request_id: randomUUID(),
    p_message: 'unsafe\u0001control',
    p_model_alias: 'live-validation-only',
    p_profile_hash: 'a'.repeat(64),
    p_tool_registry_version: 'live-validation-only',
    p_memory_manifest_hash: 'b'.repeat(64),
  });
  assert(invalidControl.error?.code === '22023', 'live database rejects disallowed controls before enqueue');

  const legacyHistory = await client.rpc('apocrypha_list_member_chat_history', {
    p_verified_auth_user_id: verifiedAuthUserId,
    p_conversation_id: verifiedAuthUserId,
  });
  assert(legacyHistory.error, 'service role cannot execute client-selected legacy history');
  assert(
    ['42501', '42883', 'PGRST202'].includes(legacyHistory.error.code ?? ''),
    `legacy history is unavailable because of privilege/schema admission: ${legacyHistory.error.code ?? 'unknown'}`,
  );

  console.log('apocrypha-member-chat-live.test: 4/4 passed');
}

main().catch((error) => {
  console.error(error);
  process.exit(1);
});
