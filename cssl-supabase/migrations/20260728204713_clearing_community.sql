-- The Clearing public room transport.
-- Public projections intentionally do not carry auth UUIDs, emails, or raw JWT metadata.
-- Authorship and idempotency live in private companion tables; only the projections
-- below are readable/realtime-visible to anon and authenticated clients.

create extension if not exists pgcrypto;

create table if not exists public.clearing_room (
  id uuid primary key default gen_random_uuid(),
  slug text not null unique check (slug ~ '^[a-z0-9][a-z0-9-]{1,47}$'),
  title text not null check (char_length(title) between 1 and 120),
  description text not null default '' check (char_length(description) <= 500),
  glyph text not null default '◇' check (char_length(glyph) between 1 and 8),
  visibility text not null default 'public' check (visibility in ('public', 'closed')),
  created_at timestamptz not null default now(),
  archived_at timestamptz
);

create table if not exists public.clearing_thread (
  id uuid primary key default gen_random_uuid(),
  room_id uuid not null references public.clearing_room(id) on delete cascade,
  creator_ref text not null check (creator_ref ~ '^[a-f0-9]{16}$'),
  title text not null default 'Thread' check (char_length(title) between 1 and 160),
  created_at timestamptz not null default now(),
  locked_at timestamptz,
  deleted_at timestamptz,
  unique (id, room_id)
);

create table if not exists public.clearing_message (
  id uuid primary key default gen_random_uuid(),
  room_id uuid not null references public.clearing_room(id) on delete cascade,
  thread_id uuid,
  reply_to_id uuid references public.clearing_message(id) on delete set null,
  author_ref text not null check (author_ref ~ '^[a-f0-9]{16}$'),
  author_label text not null check (char_length(author_label) between 1 and 64),
  body text not null check (char_length(btrim(body)) between 1 and 2000),
  created_at timestamptz not null default now(),
  edited_at timestamptz,
  deleted_at timestamptz,
  foreign key (thread_id, room_id) references public.clearing_thread(id, room_id)
);

create table if not exists public.clearing_room_member (
  room_id uuid not null references public.clearing_room(id) on delete cascade,
  actor_ref text not null check (actor_ref ~ '^[a-f0-9]{16}$'),
  display_name text not null check (char_length(display_name) between 1 and 64),
  joined_at timestamptz not null default now(),
  last_posted_at timestamptz,
  primary key (room_id, actor_ref)
);

create table if not exists public.clearing_reaction (
  message_id uuid not null references public.clearing_message(id) on delete cascade,
  actor_ref text not null check (actor_ref ~ '^[a-f0-9]{16}$'),
  kind text not null check (kind in ('spark', 'heart', 'echo', 'curious')),
  created_at timestamptz not null default now(),
  primary key (message_id, actor_ref, kind)
);

-- Private ownership/idempotency companions. No grants are given to client roles.
create table if not exists public.clearing_message_auth (
  message_id uuid primary key references public.clearing_message(id) on delete cascade,
  user_id uuid not null references auth.users(id) on delete cascade,
  client_nonce uuid not null,
  created_at timestamptz not null default now(),
  unique (user_id, client_nonce)
);

create table if not exists public.clearing_member_auth (
  room_id uuid not null references public.clearing_room(id) on delete cascade,
  user_id uuid not null references auth.users(id) on delete cascade,
  actor_ref text not null check (actor_ref ~ '^[a-f0-9]{16}$'),
  updated_at timestamptz not null default now(),
  primary key (room_id, user_id),
  unique (room_id, actor_ref)
);

create table if not exists public.clearing_reaction_auth (
  message_id uuid not null references public.clearing_message(id) on delete cascade,
  user_id uuid not null references auth.users(id) on delete cascade,
  kind text not null check (kind in ('spark', 'heart', 'echo', 'curious')),
  created_at timestamptz not null default now(),
  primary key (message_id, user_id, kind)
);

create index if not exists clearing_message_room_created_idx
  on public.clearing_message (room_id, created_at, id)
  where deleted_at is null;
create index if not exists clearing_message_thread_created_idx
  on public.clearing_message (thread_id, created_at, id)
  where deleted_at is null;
create index if not exists clearing_message_reply_idx
  on public.clearing_message (reply_to_id, created_at)
  where deleted_at is null;
create index if not exists clearing_thread_room_created_idx
  on public.clearing_thread (room_id, created_at desc)
  where deleted_at is null;

alter table public.clearing_room enable row level security;
alter table public.clearing_thread enable row level security;
alter table public.clearing_message enable row level security;
alter table public.clearing_room_member enable row level security;
alter table public.clearing_reaction enable row level security;
alter table public.clearing_message_auth enable row level security;
alter table public.clearing_member_auth enable row level security;
alter table public.clearing_reaction_auth enable row level security;

drop policy if exists clearing_room_public_read on public.clearing_room;
create policy clearing_room_public_read on public.clearing_room
  for select to anon, authenticated
  using (visibility = 'public' and archived_at is null);

drop policy if exists clearing_thread_public_read on public.clearing_thread;
create policy clearing_thread_public_read on public.clearing_thread
  for select to anon, authenticated
  using (
    deleted_at is null and exists (
      select 1 from public.clearing_room r
      where r.id = clearing_thread.room_id
        and r.visibility = 'public' and r.archived_at is null
    )
  );

drop policy if exists clearing_message_public_read on public.clearing_message;
create policy clearing_message_public_read on public.clearing_message
  for select to anon, authenticated
  using (
    deleted_at is null and exists (
      select 1 from public.clearing_room r
      where r.id = clearing_message.room_id
        and r.visibility = 'public' and r.archived_at is null
    )
  );

drop policy if exists clearing_member_public_read on public.clearing_room_member;
create policy clearing_member_public_read on public.clearing_room_member
  for select to anon, authenticated
  using (exists (
    select 1 from public.clearing_room r
    where r.id = clearing_room_member.room_id
      and r.visibility = 'public' and r.archived_at is null
  ));

drop policy if exists clearing_reaction_public_read on public.clearing_reaction;
create policy clearing_reaction_public_read on public.clearing_reaction
  for select to anon, authenticated
  using (exists (
    select 1 from public.clearing_message m
    join public.clearing_room r on r.id = m.room_id
    where m.id = clearing_reaction.message_id
      and m.deleted_at is null and r.visibility = 'public' and r.archived_at is null
  ));

revoke all on public.clearing_message_auth from anon, authenticated;
revoke all on public.clearing_member_auth from anon, authenticated;
revoke all on public.clearing_reaction_auth from anon, authenticated;
revoke insert, update, delete on public.clearing_room from anon, authenticated;
revoke insert, update, delete on public.clearing_thread from anon, authenticated;
revoke insert, update, delete on public.clearing_message from anon, authenticated;
revoke insert, update, delete on public.clearing_room_member from anon, authenticated;
revoke insert, update, delete on public.clearing_reaction from anon, authenticated;
grant select on public.clearing_room, public.clearing_thread, public.clearing_message,
  public.clearing_room_member, public.clearing_reaction to anon, authenticated;

create or replace function public.clearing_actor_ref(p_user_id uuid)
returns text
language sql immutable strict parallel safe
set search_path = public, extensions
as $$ select substr(encode(digest(p_user_id::text, 'sha256'), 'hex'), 1, 16) $$;

create or replace function public.clearing_display_name()
returns text
language plpgsql stable security definer
set search_path = public, auth, extensions
as $$
declare
  v_name text;
  v_user uuid := auth.uid();
begin
  if v_user is null then raise exception 'authentication required' using errcode = '28000'; end if;
  v_name := coalesce(
    nullif(btrim(auth.jwt() -> 'user_metadata' ->> 'display_name'), ''),
    nullif(btrim(auth.jwt() -> 'user_metadata' ->> 'full_name'), ''),
    nullif(btrim(auth.jwt() -> 'user_metadata' ->> 'name'), ''),
    'Wanderer ' || upper(substr(replace(v_user::text, '-', ''), 1, 6))
  );
  v_name := regexp_replace(v_name, '[[:cntrl:]]', '', 'g');
  return left(v_name, 64);
end;
$$;

create or replace function public.clearing_join_room(p_room_slug text)
returns public.clearing_room_member
language plpgsql security definer
set search_path = public, auth, extensions
as $$
declare
  v_uid uuid := auth.uid();
  v_room public.clearing_room;
  v_ref text;
  v_name text;
  v_member public.clearing_room_member;
begin
  if v_uid is null then raise exception 'authentication required' using errcode = '28000'; end if;
  select * into v_room from public.clearing_room where slug = lower(btrim(p_room_slug))
    and visibility = 'public' and archived_at is null;
  if not found then raise exception 'room not found' using errcode = 'P0002'; end if;
  v_ref := public.clearing_actor_ref(v_uid);
  v_name := public.clearing_display_name();
  insert into public.clearing_member_auth(room_id, user_id, actor_ref)
    values (v_room.id, v_uid, v_ref)
    on conflict (room_id, user_id) do update set actor_ref = excluded.actor_ref, updated_at = now();
  insert into public.clearing_room_member(room_id, actor_ref, display_name)
    values (v_room.id, v_ref, v_name)
    on conflict (room_id, actor_ref) do update set display_name = excluded.display_name;
  select * into v_member from public.clearing_room_member
    where room_id = v_room.id and actor_ref = v_ref;
  return v_member;
end;
$$;

create or replace function public.clearing_send_message(
  p_room_slug text,
  p_body text,
  p_client_nonce uuid,
  p_reply_to_id uuid default null
)
returns public.clearing_message
language plpgsql security definer
set search_path = public, auth, extensions
as $$
declare
  v_uid uuid := auth.uid();
  v_room public.clearing_room;
  v_parent public.clearing_message;
  v_thread public.clearing_thread;
  v_thread_id uuid;
  v_ref text;
  v_name text;
  v_message public.clearing_message;
  v_body text := btrim(coalesce(p_body, ''));
begin
  if v_uid is null then raise exception 'authentication required' using errcode = '28000'; end if;
  if p_client_nonce is null then raise exception 'client nonce required' using errcode = '22023'; end if;
  if char_length(v_body) < 1 or char_length(v_body) > 2000 then
    raise exception 'message must be between 1 and 2000 characters' using errcode = '22023';
  end if;
  if exists (select 1 from public.clearing_message_auth a
             join public.clearing_message m on m.id = a.message_id
             where a.user_id = v_uid and m.created_at > now() - interval '2 seconds') then
    raise exception 'please wait a moment before sending again' using errcode = 'P0001';
  end if;
  if (select count(*) from public.clearing_message_auth a
      join public.clearing_message m on m.id = a.message_id
      where a.user_id = v_uid and m.created_at > now() - interval '1 minute') >= 20 then
    raise exception 'message rate limit reached; try again shortly' using errcode = 'P0001';
  end if;
  select * into v_room from public.clearing_room where slug = lower(btrim(p_room_slug))
    and visibility = 'public' and archived_at is null;
  if not found then raise exception 'room not found' using errcode = 'P0002'; end if;
  if p_reply_to_id is not null then
    select * into v_parent from public.clearing_message where id = p_reply_to_id
      and room_id = v_room.id and deleted_at is null;
    if not found then raise exception 'reply target not found' using errcode = 'P0002'; end if;
    if v_parent.thread_id is null then
      insert into public.clearing_thread(room_id, creator_ref, title)
        values (v_room.id, public.clearing_actor_ref(v_uid), 'A thread in ' || v_room.title)
        returning * into v_thread;
      v_thread_id := v_thread.id;
    else
      v_thread_id := v_parent.thread_id;
    end if;
  end if;
  v_ref := public.clearing_actor_ref(v_uid);
  v_name := public.clearing_display_name();
  insert into public.clearing_member_auth(room_id, user_id, actor_ref)
    values (v_room.id, v_uid, v_ref)
    on conflict (room_id, user_id) do update set updated_at = now();
  insert into public.clearing_room_member(room_id, actor_ref, display_name, last_posted_at)
    values (v_room.id, v_ref, v_name, now())
    on conflict (room_id, actor_ref) do update set display_name = excluded.display_name, last_posted_at = now();
  insert into public.clearing_message(room_id, thread_id, reply_to_id, author_ref, author_label, body)
    values (v_room.id, v_thread_id, p_reply_to_id, v_ref, v_name, v_body)
    returning * into v_message;
  insert into public.clearing_message_auth(message_id, user_id, client_nonce)
    values (v_message.id, v_uid, p_client_nonce);
  return v_message;
exception
  when unique_violation then
    select m.* into v_message from public.clearing_message m
      join public.clearing_message_auth a on a.message_id = m.id
      where a.user_id = v_uid and a.client_nonce = p_client_nonce;
    if v_message.id is not null then return v_message; end if;
    raise;
end;
$$;

create or replace function public.clearing_toggle_reaction(p_message_id uuid, p_kind text)
returns public.clearing_reaction
language plpgsql security definer
set search_path = public, auth, extensions
as $$
declare
  v_uid uuid := auth.uid();
  v_ref text;
  v_room_id uuid;
  v_reaction public.clearing_reaction;
begin
  if v_uid is null then raise exception 'authentication required' using errcode = '28000'; end if;
  if p_kind not in ('spark', 'heart', 'echo', 'curious') then raise exception 'unsupported reaction' using errcode = '22023'; end if;
  select m.room_id into v_room_id from public.clearing_message m
    join public.clearing_room r on r.id = m.room_id
    where m.id = p_message_id and m.deleted_at is null and r.visibility = 'public' and r.archived_at is null;
  if v_room_id is null then raise exception 'message not found' using errcode = 'P0002'; end if;
  v_ref := public.clearing_actor_ref(v_uid);
  if exists (select 1 from public.clearing_reaction_auth where message_id = p_message_id and user_id = v_uid and kind = p_kind) then
    delete from public.clearing_reaction where message_id = p_message_id and actor_ref = v_ref and kind = p_kind;
    delete from public.clearing_reaction_auth where message_id = p_message_id and user_id = v_uid and kind = p_kind;
    return null;
  end if;
  insert into public.clearing_reaction(message_id, actor_ref, kind) values (p_message_id, v_ref, p_kind) returning * into v_reaction;
  insert into public.clearing_reaction_auth(message_id, user_id, kind) values (p_message_id, v_uid, p_kind);
  return v_reaction;
end;
$$;

revoke all on function public.clearing_actor_ref(uuid) from public, anon, authenticated;
revoke all on function public.clearing_display_name() from public, anon, authenticated;
revoke all on function public.clearing_join_room(text) from public, anon;
revoke all on function public.clearing_send_message(text, text, uuid, uuid) from public, anon;
revoke all on function public.clearing_toggle_reaction(uuid, text) from public, anon;
grant execute on function public.clearing_join_room(text) to authenticated;
grant execute on function public.clearing_send_message(text, text, uuid, uuid) to authenticated;
grant execute on function public.clearing_toggle_reaction(uuid, text) to authenticated;

insert into public.clearing_room (id, slug, title, description, glyph)
values
  ('c1ea0000-0000-4000-8000-000000000001', 'north-clearing', 'North Clearing', 'A public room for messages that want a little more sky around them.', '◇'),
  ('c1ea0000-0000-4000-8000-000000000002', 'dreamfall', 'Dreamfall', 'Half-formed ideas, soft edges, early signals.', '✦'),
  ('c1ea0000-0000-4000-8000-000000000003', 'stars', 'Stars', 'Small discoveries worth leaving visible.', '✧'),
  ('c1ea0000-0000-4000-8000-000000000004', 'letters', 'Letters', 'Longer notes and replies with somewhere to land.', '◌'),
  ('c1ea0000-0000-4000-8000-000000000005', 'quiet', 'Quiet', 'A low-volume room for listening first.', '○')
on conflict (slug) do update set title = excluded.title, description = excluded.description, glyph = excluded.glyph, archived_at = null;

do $$
declare
  v_table text;
begin
  foreach v_table in array array['clearing_room','clearing_thread','clearing_message','clearing_room_member','clearing_reaction'] loop
    if not exists (select 1 from pg_publication_tables where pubname = 'supabase_realtime' and schemaname = 'public' and tablename = v_table) then
      execute format('alter publication supabase_realtime add table public.%I', v_table);
    end if;
  end loop;
end $$;

alter table public.clearing_room replica identity full;
alter table public.clearing_thread replica identity full;
alter table public.clearing_message replica identity full;
alter table public.clearing_room_member replica identity full;
alter table public.clearing_reaction replica identity full;
