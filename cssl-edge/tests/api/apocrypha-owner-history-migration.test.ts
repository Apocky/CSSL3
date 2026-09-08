import { readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const migration = readFileSync(resolve(
  dirname(fileURLToPath(import.meta.url)),
  '..',
  '..',
  '..',
  'cssl-supabase',
  'migrations',
  '0050_apocrypha_owner_chat_projection.sql',
), 'utf8');
const hardeningMigration = readFileSync(resolve(
  dirname(fileURLToPath(import.meta.url)),
  '..',
  '..',
  '..',
  'cssl-supabase',
  'migrations',
  '0051_apocrypha_owner_chat_projection_invoker.sql',
), 'utf8');
const listMigration = readFileSync(resolve(
  dirname(fileURLToPath(import.meta.url)),
  '..',
  '..',
  '..',
  'cssl-supabase',
  'migrations',
  '0052_apocrypha_owner_chat_list_and_index.sql',
), 'utf8');

function assert(condition: unknown, message: string): asserts condition {
  if (!condition) throw new Error(`assert failed: ${message}`);
}

assert(
  migration.includes('CREATE OR REPLACE FUNCTION public.apocrypha_project_owner_chat_revisions'),
  'bounded owner-history projection exists',
);
assert(migration.includes('cardinality(p_job_ids) > 8'), 'each projection call is capped at eight jobs');
assert(migration.includes('SECURITY INVOKER'), 'fresh installs use caller authority for the projection');
assert(
  migration.includes('cardinality(p_job_ids) <> cardinality(p_revision_ids)'),
  'job and revision identifiers must form equal bounded sets',
);
assert(migration.includes("principal.principal_kind = 'owner'"), 'projection requires the active owner principal');
assert(migration.includes("job.kind = 'apocky_chat'"), 'projection cannot read another job kind');
assert(migration.includes("job.capability = 'apocky_owner_chat'"), 'projection cannot read another capability');
assert(migration.includes('job.terminal_revision_id = revision.id'), 'only the committed terminal revision is visible');
assert(migration.includes('left(revision.content, 16384)'), 'revision text is bounded before leaving the database');
assert(migration.includes('char_length(revision.content) > 16384'), 'content clipping remains observable');
assert(migration.includes('WHERE trace.ordinal <= 64'), 'tool provenance is bounded before leaving the database');
assert(migration.includes("left(trace.entry ->> 'name', 160)"), 'tool names are bounded in SQL');
assert(migration.includes("left(trace.entry ->> 'error', 500)"), 'tool errors are bounded in SQL');
assert(
  /REVOKE ALL ON FUNCTION public\.apocrypha_project_owner_chat_revisions\([\s\S]*?FROM PUBLIC, anon, authenticated;/m.test(migration),
  'browser roles cannot execute the owner-history projection',
);
assert(
  /GRANT EXECUTE ON FUNCTION public\.apocrypha_project_owner_chat_revisions\([\s\S]*?TO service_role;/m.test(migration),
  'only the server service role receives projection authority',
);
assert(migration.includes('DO $verification$'), 'migration verifies live privileges before completion');
assert(
  hardeningMigration.includes('ALTER FUNCTION public.apocrypha_project_owner_chat_revisions')
    && hardeningMigration.includes('SECURITY INVOKER'),
  'already-deployed projections are reduced to caller authority',
);
assert(
  hardeningMigration.includes('unexpected role retains owner chat projection execution'),
  'the live hardening migration rejects unexpected execute grants',
);
assert(
  listMigration.includes('CREATE INDEX IF NOT EXISTS idx_apocrypha_job_owner_chat_conversation_created'),
  'conversation detail has a matching partial expression index',
);
assert(
  listMigration.includes('CREATE OR REPLACE FUNCTION public.apocrypha_list_owner_chat_conversations'),
  'conversation summaries are grouped over the full owner job set in the database',
);
assert(listMigration.includes('p_limit > 257'), 'the summary result has a bounded sentinel row');
assert(listMigration.includes('SECURITY INVOKER'), 'the summary projection uses caller authority');

console.log('apocrypha-owner-history-migration.test: bounded database projection and least-authority grants OK');
