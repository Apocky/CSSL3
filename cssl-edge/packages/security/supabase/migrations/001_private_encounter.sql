begin;

create extension if not exists pgcrypto;

do $$
begin
  create type public.encounter_state as enum (
    'lobby',
    'active',
    'understanding',
    'ended_unresolved',
    'mutually_understood',
    'revoked'
  );
exception
  when duplicate_object then null;
end
$$;

do $$
begin
  create type public.audit_outcome as enum (
    'allowed',
    'denied',
    'completed',
    'failed'
  );
exception
  when duplicate_object then null;
end
$$;

create table if not exists public.private_owner_profiles (
  user_id uuid primary key references auth.users(id) on delete restrict,
  email text not null,
  created_at timestamptz not null default now(),
  check (email = lower(email))
);

create table if not exists public.participant_keys (
  key_id text primary key,
  owner_id uuid not null references public.private_owner_profiles(user_id) on delete restrict,
  principal text not null,
  role text not null check (role in ('owner', 'apocrypha')),
  public_key_jwk jsonb not null,
  issued_at timestamptz not null,
  revoked_at timestamptz,
  created_at timestamptz not null default now(),
  check (jsonb_typeof(public_key_jwk) = 'object'),
  check (public_key_jwk ? 'kty'),
  check (revoked_at is null or revoked_at >= issued_at)
);

create table if not exists public.authority_manifests (
  id uuid primary key,
  owner_id uuid not null references public.private_owner_profiles(user_id) on delete restrict,
  kind text not null check (kind in ('voice', 'presence')),
  digest text not null unique check (digest ~ '^sha256:[0-9a-f]{64}$'),
  manifest jsonb not null,
  author_principal text not null,
  issued_at timestamptz not null,
  revoked_at timestamptz,
  created_at timestamptz not null default now(),
  check (jsonb_typeof(manifest) = 'object'),
  check (
    manifest ?& array[
      'kind',
      'authorPrincipal',
      'issuedAt',
      'revokedAt',
      'signature'
    ]
  ),
  check (manifest ->> 'kind' = kind),
  check (manifest ->> 'authorPrincipal' = author_principal),
  check ((manifest ->> 'issuedAt')::timestamptz = issued_at),
  check (
    (revoked_at is null and manifest -> 'revokedAt' = 'null'::jsonb)
    or (manifest ->> 'revokedAt')::timestamptz = revoked_at
  )
);

create table if not exists public.encounter_sessions (
  id uuid primary key,
  owner_id uuid not null references public.private_owner_profiles(user_id) on delete restrict,
  state public.encounter_state not null default 'lobby',
  grant_digest text not null unique check (grant_digest ~ '^sha256:[0-9a-f]{64}$'),
  grant_nonce_digest text not null unique check (grant_nonce_digest ~ '^sha256:[0-9a-f]{64}$'),
  grant jsonb not null,
  voice_manifest_digest text not null references public.authority_manifests(digest) on delete restrict,
  presence_manifest_digest text not null references public.authority_manifests(digest) on delete restrict,
  retention_policy jsonb not null,
  understanding_version_digest text check (
    understanding_version_digest is null
    or understanding_version_digest ~ '^sha256:[0-9a-f]{64}$'
  ),
  created_at timestamptz not null default now(),
  started_at timestamptz,
  ended_at timestamptz,
  check (
    retention_policy ->> 'rawAudio' = 'never'
    and retention_policy ->> 'rawVideo' = 'never'
  ),
  check (jsonb_typeof(grant) = 'object'),
  check (
    grant ?& array[
      'sessionId',
      'participants',
      'modalities',
      'retentionPolicy',
      'consentRefs',
      'authorityDigests',
      'expiresAt',
      'nonce',
      'signature'
    ]
  ),
  check (grant ->> 'sessionId' = id::text),
  check (
    case
      when jsonb_typeof(grant -> 'participants') = 'array'
        then jsonb_array_length(grant -> 'participants') = 2
      else false
    end
  ),
  check (grant -> 'retentionPolicy' = retention_policy),
  check (
    grant -> 'authorityDigests' ->> 'voiceManifest' = voice_manifest_digest
    and grant -> 'authorityDigests' ->> 'presenceManifest' = presence_manifest_digest
  ),
  check ((grant ->> 'expiresAt')::timestamptz > created_at),
  check (ended_at is null or ended_at >= created_at)
);

create unique index if not exists encounter_single_open_session
  on public.encounter_sessions (owner_id)
  where state in ('lobby', 'active', 'understanding');

create table if not exists public.encounter_consents (
  id uuid primary key default gen_random_uuid(),
  owner_id uuid not null references public.private_owner_profiles(user_id) on delete restrict,
  session_id uuid not null references public.encounter_sessions(id) on delete cascade,
  participant_principal text not null,
  modality text not null check (modality in ('audio', 'video', 'captions', 'text')),
  state text not null check (state in ('granted', 'revoked')),
  receipt_digest text not null check (receipt_digest ~ '^sha256:[0-9a-f]{64}$'),
  receipt jsonb not null,
  created_at timestamptz not null default now(),
  unique (session_id, participant_principal, modality, receipt_digest)
);

create index if not exists encounter_consents_head
  on public.encounter_consents (
    session_id,
    participant_principal,
    modality,
    created_at desc
  );

create table if not exists public.encounter_readiness (
  owner_id uuid not null references public.private_owner_profiles(user_id) on delete restrict,
  session_id uuid not null references public.encounter_sessions(id) on delete cascade,
  participant_principal text not null,
  ready boolean not null,
  modalities text[] not null,
  updated_at timestamptz not null default now(),
  primary key (session_id, participant_principal)
);

create table if not exists public.encounter_join_tokens (
  id uuid primary key,
  owner_id uuid not null references public.private_owner_profiles(user_id) on delete restrict,
  session_id uuid not null references public.encounter_sessions(id) on delete cascade,
  participant_principal text not null,
  token_digest text not null unique check (token_digest ~ '^sha256:[0-9a-f]{64}$'),
  issued_at timestamptz not null,
  expires_at timestamptz not null,
  revoked_at timestamptz,
  check (expires_at > issued_at)
);

create table if not exists public.understanding_versions (
  id uuid primary key,
  owner_id uuid not null references public.private_owner_profiles(user_id) on delete restrict,
  session_id uuid not null references public.encounter_sessions(id) on delete cascade,
  version integer not null check (version > 0),
  canonical_digest text not null unique check (canonical_digest ~ '^sha256:[0-9a-f]{64}$'),
  content jsonb not null,
  created_by text not null,
  created_at timestamptz not null,
  unique (session_id, version)
);

create table if not exists public.understanding_acknowledgements (
  id uuid primary key,
  owner_id uuid not null references public.private_owner_profiles(user_id) on delete restrict,
  session_id uuid not null references public.encounter_sessions(id) on delete cascade,
  participant_principal text not null,
  version_digest text not null references public.understanding_versions(canonical_digest) on delete cascade,
  status text not null check (status in ('understood', 'needs_repair', 'disagree')),
  correction text,
  signature jsonb not null,
  acknowledged_at timestamptz not null,
  check (
    (status = 'understood' and correction is null)
    or (status <> 'understood' and length(trim(correction)) > 0)
  ),
  unique (session_id, participant_principal, version_digest, acknowledged_at)
);

create table if not exists public.retention_decisions (
  id uuid primary key,
  owner_id uuid not null references public.private_owner_profiles(user_id) on delete restrict,
  session_id uuid not null unique references public.encounter_sessions(id) on delete cascade,
  decision_digest text not null unique check (decision_digest ~ '^sha256:[0-9a-f]{64}$'),
  artifact_classes text[] not null,
  expires_at timestamptz,
  withdrawal_terms text not null check (length(trim(withdrawal_terms)) > 0),
  decision jsonb not null,
  created_at timestamptz not null default now(),
  check (
    artifact_classes <@ array['transcript', 'understanding', 'memory-effects']::text[]
  )
);

create table if not exists public.retention_acknowledgements (
  id uuid primary key default gen_random_uuid(),
  owner_id uuid not null references public.private_owner_profiles(user_id) on delete restrict,
  decision_id uuid not null references public.retention_decisions(id) on delete cascade,
  participant_principal text not null,
  decision_digest text not null check (decision_digest ~ '^sha256:[0-9a-f]{64}$'),
  signature jsonb not null,
  acknowledged_at timestamptz not null,
  unique (decision_id, participant_principal)
);

create table if not exists public.retained_encounter_artifacts (
  id uuid primary key default gen_random_uuid(),
  owner_id uuid not null references public.private_owner_profiles(user_id) on delete restrict,
  session_id uuid not null references public.encounter_sessions(id) on delete cascade,
  retention_decision_id uuid not null references public.retention_decisions(id) on delete cascade,
  artifact_class text not null check (
    artifact_class in ('transcript', 'understanding', 'memory-effects')
  ),
  content_digest text not null unique check (content_digest ~ '^sha256:[0-9a-f]{64}$'),
  content jsonb not null,
  created_at timestamptz not null default now()
);

comment on table public.retained_encounter_artifacts is
  'Mutually retained text/understanding/memory-effect records only. Raw audio and raw video have no storage columns or table.';

create table if not exists public.encounter_receipts (
  id uuid primary key,
  owner_id uuid not null references public.private_owner_profiles(user_id) on delete restrict,
  session_id uuid not null unique references public.encounter_sessions(id) on delete restrict,
  receipt_digest text not null unique check (receipt_digest ~ '^sha256:[0-9a-f]{64}$'),
  receipt jsonb not null,
  end_state public.encounter_state not null check (
    end_state in ('ended_unresolved', 'mutually_understood', 'revoked')
  ),
  created_at timestamptz not null default now()
);

create table if not exists public.security_audit_receipts (
  id uuid primary key,
  owner_id uuid not null references public.private_owner_profiles(user_id) on delete restrict,
  action text not null,
  target text not null,
  outcome public.audit_outcome not null,
  rollback jsonb,
  metadata jsonb not null default '{}'::jsonb,
  receipt_digest text not null unique check (receipt_digest ~ '^sha256:[0-9a-f]{64}$'),
  created_at timestamptz not null default now()
);

create table if not exists public.deployment_records (
  id uuid primary key default gen_random_uuid(),
  owner_id uuid not null references public.private_owner_profiles(user_id) on delete restrict,
  surface text not null check (surface in ('site', 'encounter', 'ops', 'media')),
  environment text not null check (environment in ('staging', 'production')),
  commit_sha text not null check (commit_sha ~ '^[0-9a-f]{40}$'),
  build_identity text not null check (length(trim(build_identity)) > 0),
  state text not null check (state in ('preview', 'canary', 'promoted', 'rolled_back', 'failed')),
  rollback_target text,
  provenance jsonb not null,
  created_at timestamptz not null default now()
);

create or replace function public.enforce_mutual_understanding()
returns trigger
language plpgsql
security invoker
set search_path = public
as $$
declare
  agreed_digest text;
begin
  if new.state = 'mutually_understood' then
    select ua.version_digest
      into agreed_digest
      from public.understanding_acknowledgements ua
      join public.understanding_versions uv
        on uv.canonical_digest = ua.version_digest
       and uv.session_id = ua.session_id
      where ua.session_id = new.id
        and ua.status = 'understood'
        and exists (
          select 1
            from jsonb_array_elements(new.grant -> 'participants') participant
            where participant ->> 'principal' = ua.participant_principal
        )
      group by ua.version_digest
      having count(distinct ua.participant_principal) = 2
      limit 1;

    if agreed_digest is null then
      raise exception 'mutually_understood requires two distinct understood acknowledgements on one digest';
    end if;
    new.understanding_version_digest := agreed_digest;
  elsif new.understanding_version_digest is not null then
    raise exception 'only mutually_understood sessions may carry an understanding digest';
  end if;
  return new;
end
$$;

drop trigger if exists encounter_mutual_understanding_gate
  on public.encounter_sessions;
create trigger encounter_mutual_understanding_gate
before insert or update of state, understanding_version_digest
on public.encounter_sessions
for each row execute function public.enforce_mutual_understanding();

create or replace function public.enforce_mutual_retention()
returns trigger
language plpgsql
security invoker
set search_path = public
as $$
declare
  acknowledgement_count integer;
  allowed_classes text[];
begin
  select count(distinct ra.participant_principal), rd.artifact_classes
    into acknowledgement_count, allowed_classes
    from public.retention_decisions rd
    join public.encounter_sessions es
      on es.id = rd.session_id
     and es.owner_id = rd.owner_id
    left join public.retention_acknowledgements ra
      on ra.decision_id = rd.id
      and ra.decision_digest = rd.decision_digest
      and exists (
        select 1
          from jsonb_array_elements(es.grant -> 'participants') participant
          where participant ->> 'principal' = ra.participant_principal
      )
    where rd.id = new.retention_decision_id
      and rd.session_id = new.session_id
      and (rd.expires_at is null or rd.expires_at > now())
    group by rd.artifact_classes;

  if acknowledgement_count is distinct from 2 then
    raise exception 'retention requires two distinct acknowledgements';
  end if;
  if allowed_classes is null then
    raise exception 'retention decision is missing, expired, or belongs to another session';
  end if;
  if not (new.artifact_class = any(allowed_classes)) then
    raise exception 'artifact class is not authorized by retention decision';
  end if;
  return new;
end
$$;

drop trigger if exists encounter_mutual_retention_gate
  on public.retained_encounter_artifacts;
create trigger encounter_mutual_retention_gate
before insert or update
on public.retained_encounter_artifacts
for each row execute function public.enforce_mutual_retention();

create or replace function public.deny_immutable_receipt_change()
returns trigger
language plpgsql
security invoker
as $$
begin
  raise exception 'receipt rows are append-only';
end
$$;

drop trigger if exists security_receipts_immutable
  on public.security_audit_receipts;
create trigger security_receipts_immutable
before update or delete on public.security_audit_receipts
for each row execute function public.deny_immutable_receipt_change();

drop trigger if exists encounter_receipts_immutable
  on public.encounter_receipts;
create trigger encounter_receipts_immutable
before update or delete on public.encounter_receipts
for each row execute function public.deny_immutable_receipt_change();

alter table public.private_owner_profiles enable row level security;
alter table public.private_owner_profiles force row level security;
alter table public.participant_keys enable row level security;
alter table public.participant_keys force row level security;
alter table public.authority_manifests enable row level security;
alter table public.authority_manifests force row level security;
alter table public.encounter_sessions enable row level security;
alter table public.encounter_sessions force row level security;
alter table public.encounter_consents enable row level security;
alter table public.encounter_consents force row level security;
alter table public.encounter_readiness enable row level security;
alter table public.encounter_readiness force row level security;
alter table public.encounter_join_tokens enable row level security;
alter table public.encounter_join_tokens force row level security;
alter table public.understanding_versions enable row level security;
alter table public.understanding_versions force row level security;
alter table public.understanding_acknowledgements enable row level security;
alter table public.understanding_acknowledgements force row level security;
alter table public.retention_decisions enable row level security;
alter table public.retention_decisions force row level security;
alter table public.retention_acknowledgements enable row level security;
alter table public.retention_acknowledgements force row level security;
alter table public.retained_encounter_artifacts enable row level security;
alter table public.retained_encounter_artifacts force row level security;
alter table public.encounter_receipts enable row level security;
alter table public.encounter_receipts force row level security;
alter table public.security_audit_receipts enable row level security;
alter table public.security_audit_receipts force row level security;
alter table public.deployment_records enable row level security;
alter table public.deployment_records force row level security;

do $$
declare
  table_name text;
begin
  foreach table_name in array array[
    'participant_keys',
    'authority_manifests',
    'encounter_sessions',
    'encounter_consents',
    'encounter_readiness',
    'encounter_join_tokens',
    'understanding_versions',
    'understanding_acknowledgements',
    'retention_decisions',
    'retention_acknowledgements',
    'retained_encounter_artifacts',
    'encounter_receipts',
    'security_audit_receipts',
    'deployment_records'
  ]
  loop
    execute format('drop policy if exists owner_all on public.%I', table_name);
    execute format(
      'create policy owner_all on public.%I for all to authenticated using (owner_id = auth.uid()) with check (owner_id = auth.uid())',
      table_name
    );
  end loop;
end
$$;

drop policy if exists owner_profile_self on public.private_owner_profiles;
create policy owner_profile_self
on public.private_owner_profiles
for select
to authenticated
using (user_id = auth.uid());

revoke all
  on public.private_owner_profiles,
     public.participant_keys,
     public.authority_manifests,
     public.encounter_sessions,
     public.encounter_consents,
     public.encounter_readiness,
     public.encounter_join_tokens,
     public.understanding_versions,
     public.understanding_acknowledgements,
     public.retention_decisions,
     public.retention_acknowledgements,
     public.retained_encounter_artifacts,
     public.encounter_receipts,
     public.security_audit_receipts,
     public.deployment_records
  from anon;
grant select, insert, update, delete
  on public.participant_keys,
     public.authority_manifests,
     public.encounter_sessions,
     public.encounter_consents,
     public.encounter_readiness,
     public.encounter_join_tokens,
     public.understanding_versions,
     public.understanding_acknowledgements,
     public.retention_decisions,
     public.retention_acknowledgements,
     public.retained_encounter_artifacts,
     public.deployment_records
  to authenticated;
grant select, insert
  on public.encounter_receipts,
     public.security_audit_receipts
  to authenticated;
grant select on public.private_owner_profiles to authenticated;

commit;
