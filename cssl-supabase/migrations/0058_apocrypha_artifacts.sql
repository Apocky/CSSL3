-- 0058_apocrypha_artifacts.sql
--
-- Cross-device artifact store for Apocrypha.
--
-- WHY
-- ---
-- apx-tools gives the local model tools that act on the desktop's disk. That is
-- the right place to ACT and the wrong place for the result to stay: the person
-- asking is often on a phone, and a file written into a directory on a desktop
-- is, from a phone, indistinguishable from a file that was never written.
--
-- This table is the handoff. The desktop writes an artifact under a name; the
-- web app, the phone, or a later session reads it back by that name.
--
-- ACCESS
-- ------
-- RLS is on and there is NO policy, which is the point rather than an omission.
-- With no policy, anon and authenticated can see nothing at all - not the rows,
-- not the table in a select. Exactly one thing reaches it: a caller holding the
-- service role, which bypasses RLS. That is the desktop service (via
-- APX_CLOUD_KEY) and any server-side route that already holds the same key.
--
-- If this ever needs to be readable by a signed-in member rather than only by
-- the owner's own machines, that is a policy added deliberately in a later
-- migration - not a default anyone inherited.

create table if not exists public.apocrypha_artifacts (
  name        text primary key,
  body        text not null,
  -- Which machine wrote it. Two devices writing one name leave a trail rather
  -- than a mystery about which won.
  device      text not null default 'unknown',
  updated_at  timestamptz not null default now(),
  created_at  timestamptz not null default now()
);

comment on table public.apocrypha_artifacts is
  'Apocrypha cross-device artifact store. Service-role only; no RLS policy by design.';
comment on column public.apocrypha_artifacts.name is
  'Caller-chosen key, e.g. notes/2026/tide-tables. Slashes are ordinary characters here, never a path.';

-- Listing is "most recent first"; prefix filters are the other common query.
create index if not exists apocrypha_artifacts_updated_at_idx
  on public.apocrypha_artifacts (updated_at desc);
create index if not exists apocrypha_artifacts_name_prefix_idx
  on public.apocrypha_artifacts (name text_pattern_ops);

alter table public.apocrypha_artifacts enable row level security;

-- An upsert has to actually move updated_at, or "most recent first" becomes a
-- lie the first time anything is overwritten.
create or replace function public.apocrypha_artifacts_touch()
returns trigger
language plpgsql
security definer
set search_path = ''
as $$
begin
  new.updated_at := now();
  return new;
end;
$$;

drop trigger if exists apocrypha_artifacts_touch on public.apocrypha_artifacts;
create trigger apocrypha_artifacts_touch
  before update on public.apocrypha_artifacts
  for each row execute function public.apocrypha_artifacts_touch();
