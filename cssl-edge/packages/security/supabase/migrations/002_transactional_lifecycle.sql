begin;

create table if not exists public.retention_withdrawals (
  id uuid primary key default gen_random_uuid(),
  owner_id uuid not null references public.private_owner_profiles(user_id) on delete restrict,
  session_id uuid not null references public.encounter_sessions(id) on delete restrict,
  artifact_classes text[] not null,
  artifact_digests text[] not null,
  deleted_artifact_count integer not null check (deleted_artifact_count >= 0),
  workflow_state text not null check (
    workflow_state in ('completed', 'pending_upstream', 'failed')
  ),
  upstream_receipt_digest text check (
    upstream_receipt_digest is null
    or upstream_receipt_digest ~ '^sha256:[0-9a-f]{64}$'
  ),
  requested_at timestamptz not null default now(),
  completed_at timestamptz,
  check (
    artifact_classes <@ array['transcript', 'understanding', 'memory-effects']::text[]
  ),
  check (cardinality(artifact_classes) > 0),
  check (
    (workflow_state = 'pending_upstream'
      and upstream_receipt_digest is null
      and completed_at is null)
    or (workflow_state = 'completed' and completed_at is not null)
    or workflow_state = 'failed'
  )
);

comment on table public.retention_withdrawals is
  'Append-only evidence that mutually retained content was locally deleted. Memory-effect withdrawal remains pending until an upstream receipt is recorded.';

alter table public.retention_withdrawals enable row level security;
alter table public.retention_withdrawals force row level security;

drop policy if exists owner_all on public.retention_withdrawals;
create policy owner_all
on public.retention_withdrawals
for all
to authenticated
using (owner_id = auth.uid())
with check (owner_id = auth.uid());

revoke all on public.retention_withdrawals from anon;
grant select on public.retention_withdrawals to authenticated;

create or replace function public.finalize_encounter(
  p_session_id uuid,
  p_end_state public.encounter_state,
  p_receipt_id uuid,
  p_receipt_digest text,
  p_receipt jsonb,
  p_ended_at timestamptz,
  p_audit_metadata jsonb default '{}'::jsonb
)
returns jsonb
language plpgsql
security definer
set search_path = public, extensions
as $$
declare
  caller_owner_id uuid := auth.uid();
  current_session public.encounter_sessions%rowtype;
  current_consent_heads text[];
  receipt_consent_heads text[];
  current_content_digests text[];
  receipt_content_digests text[];
  terminal_understanding_digest text;
  audit_id uuid := gen_random_uuid();
  audit_created_at timestamptz := now();
  audit_payload jsonb;
  audit_digest text;
begin
  if caller_owner_id is null then
    raise exception 'authenticated owner identity is required';
  end if;
  if p_end_state not in (
    'ended_unresolved'::public.encounter_state,
    'mutually_understood'::public.encounter_state,
    'revoked'::public.encounter_state
  ) then
    raise exception 'invalid terminal encounter state';
  end if;
  if p_receipt_digest !~ '^sha256:[0-9a-f]{64}$' then
    raise exception 'invalid encounter receipt digest';
  end if;
  if jsonb_typeof(coalesce(p_audit_metadata, '{}'::jsonb))
      is distinct from 'object'
  then
    raise exception 'audit metadata must be an object';
  end if;
  if p_receipt is null
    or jsonb_typeof(p_receipt) is distinct from 'object'
    or not (
      p_receipt ?& array[
        'receiptId',
        'sessionId',
        'startedAt',
        'endedAt',
        'endState',
        'authorityDigests',
        'consentHeads',
        'retainedContentDigests',
        'understandingVersionDigest',
        'signature'
      ]
    )
  then
    raise exception 'encounter receipt is structurally incomplete';
  end if;

  select *
    into current_session
    from public.encounter_sessions
    where id = p_session_id
      and owner_id = caller_owner_id
    for update;
  if not found then
    raise exception 'encounter session is unavailable';
  end if;
  if current_session.state not in (
    'lobby'::public.encounter_state,
    'active'::public.encounter_state,
    'understanding'::public.encounter_state
  ) then
    raise exception 'encounter is already terminal';
  end if;
  if p_ended_at < current_session.created_at or p_ended_at > now() + interval '5 minutes' then
    raise exception 'encounter end time is invalid';
  end if;
  if p_receipt ->> 'receiptId' is distinct from p_receipt_id::text
    or p_receipt ->> 'sessionId' is distinct from p_session_id::text
    or p_receipt ->> 'endState' is distinct from p_end_state::text
    or (p_receipt ->> 'endedAt')::timestamptz is distinct from p_ended_at
  then
    raise exception 'encounter receipt identity or terminal state is inconsistent';
  end if;
  if (
    current_session.started_at is null
    and p_receipt -> 'startedAt' is distinct from 'null'::jsonb
  ) or (
    current_session.started_at is not null
    and (p_receipt ->> 'startedAt')::timestamptz
      is distinct from current_session.started_at
  ) then
    raise exception 'encounter receipt start time is inconsistent';
  end if;
  if jsonb_typeof(p_receipt -> 'authorityDigests') is distinct from 'object'
    or p_receipt -> 'authorityDigests' ->> 'voiceManifest'
      is distinct from current_session.voice_manifest_digest
    or p_receipt -> 'authorityDigests' ->> 'presenceManifest'
      is distinct from current_session.presence_manifest_digest
    or p_receipt -> 'authorityDigests' ->> 'encounterGrant'
      is distinct from current_session.grant_digest
  then
    raise exception 'encounter receipt authority digests are inconsistent';
  end if;
  if not exists (
    select 1
      from jsonb_array_elements(current_session.grant -> 'participants') participant
      where participant ->> 'keyId' = p_receipt -> 'signature' ->> 'keyId'
  ) then
    raise exception 'encounter receipt signer is not a session participant';
  end if;
  if p_end_state = 'mutually_understood'::public.encounter_state then
    select ua.version_digest
      into terminal_understanding_digest
      from public.understanding_acknowledgements ua
      join public.understanding_versions uv
        on uv.canonical_digest = ua.version_digest
       and uv.session_id = ua.session_id
      where ua.session_id = p_session_id
        and ua.owner_id = caller_owner_id
        and ua.status = 'understood'
        and exists (
          select 1
            from jsonb_array_elements(
              current_session.grant -> 'participants'
            ) participant
            where participant ->> 'principal' = ua.participant_principal
        )
      group by ua.version_digest
      having count(distinct ua.participant_principal) = 2
      limit 1;
    if terminal_understanding_digest is null
      or p_receipt ->> 'understandingVersionDigest'
        is distinct from terminal_understanding_digest
    then
      raise exception 'mutually understood receipt lacks the bilateral understanding digest';
    end if;
  elsif p_receipt -> 'understandingVersionDigest' is distinct from 'null'::jsonb then
    raise exception 'unresolved or revoked receipt cannot claim an understanding digest';
  end if;
  if jsonb_typeof(p_receipt -> 'consentHeads') is distinct from 'array'
    or jsonb_typeof(p_receipt -> 'retainedContentDigests')
      is distinct from 'array'
  then
    raise exception 'encounter receipt digest collections must be arrays';
  end if;

  select coalesce(
      array_agg(head.receipt_digest order by head.receipt_digest),
      array[]::text[]
    )
    into current_consent_heads
    from (
      select distinct on (participant_principal)
        participant_principal,
        receipt_digest
      from public.encounter_consents
      where owner_id = caller_owner_id
        and session_id = p_session_id
      order by participant_principal, created_at desc, id desc
    ) head;
  select coalesce(array_agg(value order by value), array[]::text[])
    into receipt_consent_heads
    from jsonb_array_elements_text(p_receipt -> 'consentHeads') value;
  if cardinality(current_consent_heads) <> 2
    or cardinality(receipt_consent_heads) <> 2
    or current_consent_heads <> receipt_consent_heads
  then
    raise exception 'encounter receipt does not bind both current consent heads';
  end if;

  select coalesce(
      array_agg(content_digest order by content_digest),
      array[]::text[]
    )
    into current_content_digests
    from public.retained_encounter_artifacts
    where owner_id = caller_owner_id
      and session_id = p_session_id;
  select coalesce(array_agg(value order by value), array[]::text[])
    into receipt_content_digests
    from jsonb_array_elements_text(p_receipt -> 'retainedContentDigests') value;
  if current_content_digests <> receipt_content_digests then
    raise exception 'encounter receipt retained-content digests are inconsistent';
  end if;

  update public.encounter_sessions
    set state = p_end_state,
        ended_at = p_ended_at
    where id = p_session_id
      and owner_id = caller_owner_id;

  update public.encounter_join_tokens
    set revoked_at = coalesce(revoked_at, p_ended_at)
    where session_id = p_session_id
      and owner_id = caller_owner_id
      and revoked_at is null;

  insert into public.encounter_receipts (
    id,
    owner_id,
    session_id,
    receipt_digest,
    receipt,
    end_state,
    created_at
  ) values (
    p_receipt_id,
    caller_owner_id,
    p_session_id,
    p_receipt_digest,
    p_receipt,
    p_end_state,
    p_ended_at
  );

  audit_payload := jsonb_build_object(
    'id', audit_id,
    'ownerId', caller_owner_id,
    'action', 'end_encounter',
    'target', p_session_id,
    'outcome', 'completed',
    'rollback', null,
    'metadata', coalesce(p_audit_metadata, '{}'::jsonb)
      || jsonb_build_object(
        'encounterReceiptId', p_receipt_id,
        'encounterReceiptDigest', p_receipt_digest,
        'endState', p_end_state
      ),
    'createdAt', audit_created_at
  );
  audit_digest := 'sha256:' || encode(
    digest(convert_to(audit_payload::text, 'UTF8'), 'sha256'),
    'hex'
  );
  insert into public.security_audit_receipts (
    id,
    owner_id,
    action,
    target,
    outcome,
    rollback,
    metadata,
    receipt_digest,
    created_at
  ) values (
    audit_id,
    caller_owner_id,
    'end_encounter',
    p_session_id::text,
    'completed',
    null,
    audit_payload -> 'metadata',
    audit_digest,
    audit_created_at
  );

  return jsonb_build_object(
    'encounterReceiptId', p_receipt_id,
    'encounterReceiptDigest', p_receipt_digest,
    'auditReceiptId', audit_id,
    'auditReceiptDigest', audit_digest,
    'endState', p_end_state,
    'endedAt', p_ended_at
  );
end
$$;

create or replace function public.withdraw_retained_history(
  p_session_id uuid,
  p_artifact_classes text[],
  p_audit_metadata jsonb default '{}'::jsonb
)
returns jsonb
language plpgsql
security definer
set search_path = public, extensions
as $$
declare
  caller_owner_id uuid := auth.uid();
  withdrawal_id uuid := gen_random_uuid();
  requested_at timestamptz := now();
  deleted_count integer;
  deleted_digests text[];
  memory_effects_withdrawn boolean;
  withdrawal_state text;
  audit_id uuid := gen_random_uuid();
  audit_payload jsonb;
  audit_digest text;
begin
  if caller_owner_id is null then
    raise exception 'authenticated owner identity is required';
  end if;
  if cardinality(p_artifact_classes) is null
    or cardinality(p_artifact_classes) = 0
    or not (
      p_artifact_classes <@
      array['transcript', 'understanding', 'memory-effects']::text[]
    )
    or cardinality(p_artifact_classes)
      <> cardinality(array(select distinct unnest(p_artifact_classes)))
  then
    raise exception 'invalid retained artifact classes';
  end if;
  if jsonb_typeof(coalesce(p_audit_metadata, '{}'::jsonb))
      is distinct from 'object'
  then
    raise exception 'audit metadata must be an object';
  end if;
  perform 1
    from public.encounter_sessions
    where id = p_session_id
      and owner_id = caller_owner_id
    for update;
  if not found then
    raise exception 'encounter session is unavailable';
  end if;

  with deleted as (
    delete from public.retained_encounter_artifacts
      where owner_id = caller_owner_id
        and session_id = p_session_id
        and artifact_class = any(p_artifact_classes)
      returning artifact_class, content_digest
  )
  select
    count(*)::integer,
    coalesce(
      array_agg(content_digest order by content_digest),
      array[]::text[]
    ),
    coalesce(bool_or(artifact_class = 'memory-effects'), false)
    into deleted_count, deleted_digests, memory_effects_withdrawn
    from deleted;

  withdrawal_state := case
    when memory_effects_withdrawn then 'pending_upstream'
    else 'completed'
  end;
  insert into public.retention_withdrawals (
    id,
    owner_id,
    session_id,
    artifact_classes,
    artifact_digests,
    deleted_artifact_count,
    workflow_state,
    upstream_receipt_digest,
    requested_at,
    completed_at
  ) values (
    withdrawal_id,
    caller_owner_id,
    p_session_id,
    p_artifact_classes,
    deleted_digests,
    deleted_count,
    withdrawal_state,
    null,
    requested_at,
    case when withdrawal_state = 'completed' then requested_at else null end
  );

  audit_payload := jsonb_build_object(
    'id', audit_id,
    'ownerId', caller_owner_id,
    'action', 'delete_retained_history',
    'target', p_session_id,
    'outcome', 'completed',
    'rollback', null,
    'metadata', coalesce(p_audit_metadata, '{}'::jsonb)
      || jsonb_build_object(
        'withdrawalId', withdrawal_id,
        'artifactClasses', p_artifact_classes,
        'deletedArtifactCount', deleted_count,
        'workflowState', withdrawal_state,
        'rollbackAvailable', false
      ),
    'createdAt', requested_at
  );
  audit_digest := 'sha256:' || encode(
    digest(convert_to(audit_payload::text, 'UTF8'), 'sha256'),
    'hex'
  );
  insert into public.security_audit_receipts (
    id,
    owner_id,
    action,
    target,
    outcome,
    rollback,
    metadata,
    receipt_digest,
    created_at
  ) values (
    audit_id,
    caller_owner_id,
    'delete_retained_history',
    p_session_id::text,
    'completed',
    null,
    audit_payload -> 'metadata',
    audit_digest,
    requested_at
  );

  return jsonb_build_object(
    'withdrawalId', withdrawal_id,
    'deletedArtifactCount', deleted_count,
    'deletedContentDigests', deleted_digests,
    'workflowState', withdrawal_state,
    'auditReceiptId', audit_id,
    'auditReceiptDigest', audit_digest
  );
end
$$;

create or replace function public.complete_retention_withdrawal(
  p_withdrawal_id uuid,
  p_upstream_receipt_digest text,
  p_audit_metadata jsonb default '{}'::jsonb
)
returns jsonb
language plpgsql
security definer
set search_path = public, extensions
as $$
declare
  caller_owner_id uuid := auth.uid();
  completed_time timestamptz := now();
  current_withdrawal public.retention_withdrawals%rowtype;
  audit_id uuid := gen_random_uuid();
  audit_payload jsonb;
  audit_digest text;
begin
  if caller_owner_id is null then
    raise exception 'authenticated owner identity is required';
  end if;
  if p_upstream_receipt_digest !~ '^sha256:[0-9a-f]{64}$' then
    raise exception 'invalid upstream withdrawal receipt digest';
  end if;
  if jsonb_typeof(coalesce(p_audit_metadata, '{}'::jsonb))
      is distinct from 'object'
  then
    raise exception 'audit metadata must be an object';
  end if;
  select *
    into current_withdrawal
    from public.retention_withdrawals
    where id = p_withdrawal_id
      and owner_id = caller_owner_id
    for update;
  if not found or current_withdrawal.workflow_state <> 'pending_upstream' then
    raise exception 'pending retention withdrawal is unavailable';
  end if;

  update public.retention_withdrawals
    set workflow_state = 'completed',
        upstream_receipt_digest = p_upstream_receipt_digest,
        completed_at = completed_time
    where id = p_withdrawal_id
      and owner_id = caller_owner_id;

  audit_payload := jsonb_build_object(
    'id', audit_id,
    'ownerId', caller_owner_id,
    'action', 'complete_retention_withdrawal',
    'target', p_withdrawal_id,
    'outcome', 'completed',
    'rollback', null,
    'metadata', coalesce(p_audit_metadata, '{}'::jsonb)
      || jsonb_build_object(
        'sessionId', current_withdrawal.session_id,
        'upstreamReceiptDigest', p_upstream_receipt_digest,
        'rollbackAvailable', false
      ),
    'createdAt', completed_time
  );
  audit_digest := 'sha256:' || encode(
    digest(convert_to(audit_payload::text, 'UTF8'), 'sha256'),
    'hex'
  );
  insert into public.security_audit_receipts (
    id,
    owner_id,
    action,
    target,
    outcome,
    rollback,
    metadata,
    receipt_digest,
    created_at
  ) values (
    audit_id,
    caller_owner_id,
    'complete_retention_withdrawal',
    p_withdrawal_id::text,
    'completed',
    null,
    audit_payload -> 'metadata',
    audit_digest,
    completed_time
  );

  return jsonb_build_object(
    'withdrawalId', p_withdrawal_id,
    'workflowState', 'completed',
    'upstreamReceiptDigest', p_upstream_receipt_digest,
    'auditReceiptId', audit_id,
    'auditReceiptDigest', audit_digest
  );
end
$$;

revoke all on function public.finalize_encounter(
  uuid,
  public.encounter_state,
  uuid,
  text,
  jsonb,
  timestamptz,
  jsonb
) from public, anon;
revoke all on function public.withdraw_retained_history(
  uuid,
  text[],
  jsonb
) from public, anon;
revoke all on function public.complete_retention_withdrawal(
  uuid,
  text,
  jsonb
) from public, anon;

grant execute on function public.finalize_encounter(
  uuid,
  public.encounter_state,
  uuid,
  text,
  jsonb,
  timestamptz,
  jsonb
) to authenticated;
grant execute on function public.withdraw_retained_history(
  uuid,
  text[],
  jsonb
) to authenticated;
grant execute on function public.complete_retention_withdrawal(
  uuid,
  text,
  jsonb
) to authenticated;

revoke delete on public.retained_encounter_artifacts from authenticated;
revoke insert on public.encounter_receipts from authenticated;
revoke update on public.retention_withdrawals from authenticated;

commit;
