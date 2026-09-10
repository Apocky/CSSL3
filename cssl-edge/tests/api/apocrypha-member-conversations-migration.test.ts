// 0057 is not applied by this repo, so it is verified by reading it.
//
// The properties below are the ones the application now DEPENDS ON. If someone
// edits the migration and drops one, member chat does not fail loudly - it
// either refuses every conversation, or stops refusing foreign ones. Both are
// silent from the application's side, which is why they are pinned here.

import { strict as assert } from 'node:assert';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';

const migration = readFileSync(
  resolve(
    process.cwd(),
    '..',
    'cssl-supabase',
    'migrations',
    '0057_apocrypha_member_conversations.sql',
  ),
  'utf8',
);

function main(): void {
  // ── the one-conversation-per-member structure is actually removed ──────
  assert.match(
    migration,
    /DROP CONSTRAINT IF EXISTS apocrypha_member_chat_conversation_stable_id/,
    'the CHECK forcing conversation_id = auth_user_id must be dropped, or a member still cannot have a second conversation',
  );
  assert.match(
    migration,
    /DROP CONSTRAINT IF EXISTS apocrypha_member_chat_conversation_primary/,
    'PRIMARY KEY (tenant_id, principal_id) is the structural one-row-per-member and must be dropped',
  );
  assert.match(
    migration,
    /ADD CONSTRAINT apocrypha_member_chat_conversation_primary\s+PRIMARY KEY \(tenant_id, principal_id, conversation_id\)/,
    'the replacement primary key must include conversation_id',
  );

  // ── the dependent foreign key is dropped and re-created ───────────────
  //
  // MEASURED: the first attempt against the live database failed with 2BP01.
  // apocrypha_member_chat_request holds an FK standing on scope_unique, and a
  // foreign key depends on the specific constraint backing it - so adding the
  // new primary key first does not release it either.
  //
  // Both halves have to be asserted. Dropping it without re-creating it would
  // make the migration succeed while requests quietly lose their referential
  // integrity, and nothing else in this suite would notice.
  assert.match(
    migration,
    /ALTER TABLE public\.apocrypha_member_chat_request\s+DROP CONSTRAINT IF EXISTS apocrypha_member_chat_request_conversation_fk/,
    'the dependent foreign key must be dropped, or the key swap fails with 2BP01',
  );
  assert.match(
    migration,
    /ADD CONSTRAINT apocrypha_member_chat_request_conversation_fk\s+FOREIGN KEY \(tenant_id, principal_id, conversation_id\)/,
    'the dependent foreign key must be RE-CREATED, or requests lose referential integrity',
  );
  assert.match(
    migration,
    /REFERENCES public\.apocrypha_member_chat_conversation\s*\(tenant_id, principal_id, conversation_id\)\s+ON DELETE CASCADE/,
    'the re-created foreign key must keep its columns and its cascade',
  );

  // ── the ownership guarantee is KEPT, and it is the whole point ─────────
  //
  // UNIQUE (tenant_id, conversation_id) is what makes "look the id up, then
  // check its owner" total: an id has at most one owner tenant-wide. Drop it
  // and apocrypha_open_member_conversation's lookup stops being conclusive -
  // two principals could hold the same conversation id and the INSERT branch
  // would quietly create the second.
  assert.doesNotMatch(
    migration,
    /DROP CONSTRAINT IF EXISTS apocrypha_member_chat_conversation_id_unique/,
    'UNIQUE (tenant_id, conversation_id) is the entire ownership guarantee and must survive this migration',
  );

  // ── the lookup must be by conversation id ALONE ────────────────────────
  //
  // Scoping that SELECT by principal is the subtle way to break this: a
  // conversation owned by someone else would simply not be found, and control
  // would fall through to the INSERT.
  const opener = migration.slice(
    migration.indexOf('FUNCTION public.apocrypha_open_member_conversation'),
    migration.indexOf('FUNCTION public.apocrypha_list_member_conversations'),
  );
  assert.ok(opener.length > 0, 'apocrypha_open_member_conversation must be defined');
  assert.match(
    opener,
    /WHERE conversation\.tenant_id = v_identity\.tenant_id\s+AND conversation\.conversation_id = p_conversation_id\s+FOR UPDATE/,
    'the lookup must be by tenant + conversation id alone, locked - adding principal_id here silently reopens the hole',
  );
  assert.match(
    opener,
    /IF v_conversation\.principal_id <> v_identity\.principal_id/,
    'the found row must be checked against the verified principal',
  );
  assert.match(opener, /P4031/, 'a foreign conversation must be refused with P4031');

  // ── the admission path must not still demand equality ──────────────────
  assert.doesNotMatch(
    migration,
    /p_presented_conversation_id <> p_verified_auth_user_id/,
    'the equality guard must be gone from the rebound v2 functions, or members still get exactly one conversation',
  );
  assert.match(
    migration,
    /FUNCTION public\.apocrypha_enqueue_member_chat_v2/,
    'enqueue_v2 must be rebound in this migration, not left calling the old ensure',
  );
  assert.match(
    migration,
    /FUNCTION public\.apocrypha_list_member_chat_history_v2/,
    'history_v2 must be rebound too, or reading a second conversation fails while writing it succeeds',
  );

  // ── the RPCs the server calls must exist, and be service-role only ─────
  for (const fn of [
    'apocrypha_open_member_conversation',
    'apocrypha_list_member_conversations',
  ]) {
    assert.match(
      migration,
      new RegExp(`CREATE OR REPLACE FUNCTION public\\.${fn}`),
      `${fn} must be defined - the server calls it by name`,
    );
    assert.match(
      migration,
      new RegExp(`REVOKE ALL ON FUNCTION public\\.${fn}[^;]*FROM PUBLIC, anon, authenticated`),
      `${fn} must be revoked from anon and authenticated`,
    );
    assert.match(
      migration,
      new RegExp(`GRANT EXECUTE ON FUNCTION public\\.${fn}[^;]*TO service_role`),
      `${fn} must be executable by service_role`,
    );
  }

  // Every function that reads identity runs SECURITY DEFINER with a pinned
  // search_path; an unpinned one is a privilege-escalation vector.
  const definers = migration.match(/SECURITY DEFINER/g) ?? [];
  const pinned = migration.match(/SET search_path = pg_catalog, public, extensions/g) ?? [];
  assert.equal(
    definers.length,
    pinned.length,
    'every SECURITY DEFINER function must pin search_path',
  );

  console.log('apocrypha-member-conversations-migration: 0057 ownership properties OK');
}

main();
