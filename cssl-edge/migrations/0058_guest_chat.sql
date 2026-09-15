-- 0058 · guest chat: let a signed-out visitor reach the same worker as a member.
--
-- Additive only. No member table, function, or constraint is altered, so a failure here cannot
-- degrade the member path, and the whole thing is reversible by dropping three functions.
--
-- The schema already anticipated this: apocrypha_principal.principal_kind permits 'guest', its
-- auth_user_id is nullable, and apocrypha_principal_identity_present accepts an
-- external_subject_hash in place of an auth user. A guest is therefore an ordinary principal with
-- no auth identity -- not a special case threaded through the job pipeline.
--
-- Guests get their OWN TENANT. Memory retrieval in the worker is tenant-scoped, so putting guests
-- in 'apocky-guests' is what structurally prevents a stranger's turn from reaching member or owner
-- memory. That isolation is the point; it is not a tidiness choice.

-- ---------------------------------------------------------------- guest principal
CREATE OR REPLACE FUNCTION public.apocrypha_ensure_guest_principal(p_subject_hash text)
RETURNS TABLE(tenant_id uuid, principal_id uuid)
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path TO 'pg_catalog', 'public', 'extensions'
AS $function$
DECLARE
    v_tenant    public.apocrypha_tenant;
    v_principal public.apocrypha_principal;
BEGIN
    -- A 64-char lowercase hex digest, computed server-side from an HttpOnly cookie. Requiring the
    -- exact shape means a caller cannot smuggle an arbitrary string in and mint principals at will.
    IF p_subject_hash IS NULL OR p_subject_hash !~ '^[0-9a-f]{64}$' THEN
        RAISE EXCEPTION 'guest subject hash must be a 64 character hex digest'
            USING ERRCODE = '22023';
    END IF;

    PERFORM pg_advisory_xact_lock(
        hashtextextended('apocky-guest-principal:' || p_subject_hash, 0)
    );

    INSERT INTO public.apocrypha_tenant (slug, display_name)
    VALUES ('apocky-guests', 'Apocky guests')
    ON CONFLICT (slug) DO NOTHING;

    SELECT tenant.* INTO v_tenant
    FROM public.apocrypha_tenant AS tenant
    WHERE tenant.slug = 'apocky-guests'
    FOR SHARE;

    IF NOT FOUND OR v_tenant.status <> 'active' THEN
        RAISE EXCEPTION 'Apocky guest tenant is not active' USING ERRCODE = '55000';
    END IF;

    SELECT principal.* INTO v_principal
    FROM public.apocrypha_principal AS principal
    WHERE principal.tenant_id = v_tenant.id
      AND principal.principal_kind = 'guest'
      AND principal.external_subject_hash = p_subject_hash;

    IF NOT FOUND THEN
        INSERT INTO public.apocrypha_principal (
            tenant_id, auth_user_id, principal_kind, external_subject_hash, display_name
        ) VALUES (
            v_tenant.id, NULL, 'guest', p_subject_hash, 'Apocky guest'
        )
        RETURNING * INTO v_principal;
    END IF;

    IF v_principal.status <> 'active' THEN
        RAISE EXCEPTION 'guest principal is not active' USING ERRCODE = '55000';
    END IF;

    RETURN QUERY SELECT v_tenant.id, v_principal.id;
END;
$function$;

-- ---------------------------------------------------------------- guest enqueue
CREATE OR REPLACE FUNCTION public.apocrypha_enqueue_guest_chat_v1(
    p_subject_hash text,
    p_request_id uuid,
    p_message text,
    p_history jsonb,
    p_model_alias text,
    p_profile_hash text,
    p_tool_registry_version text,
    p_memory_manifest_hash text
)
RETURNS TABLE(job_id uuid, request_id uuid, status text, created_at timestamptz, replayed boolean)
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path TO 'pg_catalog', 'public', 'extensions'
AS $function$
DECLARE
    v_identity     record;
    v_job          public.apocrypha_job;
    v_request      jsonb;
    v_request_hash text;
    v_history      jsonb;
    v_recent       integer;
    v_existing     public.apocrypha_job;
BEGIN
    IF p_request_id IS NULL THEN
        RAISE EXCEPTION 'guest chat request id is required' USING ERRCODE = '23502';
    END IF;
    IF p_message IS NULL
       OR p_message <> btrim(p_message)
       OR char_length(p_message) = 0
       OR octet_length(p_message) > 8192
       OR regexp_replace(p_message, E'[\t\n\r]', '', 'g') ~ '[[:cntrl:]]' THEN
        RAISE EXCEPTION 'guest chat message contains invalid bytes or control characters'
            USING ERRCODE = '22023';
    END IF;

    SELECT ensured.tenant_id, ensured.principal_id INTO v_identity
    FROM public.apocrypha_ensure_guest_principal(p_subject_hash) AS ensured;

    PERFORM pg_advisory_xact_lock(hashtextextended(
        'apocky-guest-chat:' || v_identity.principal_id::text, 0
    ));

    -- Replay before anything else: a retried request id must return the SAME job, never a second
    -- one, or a dropped connection becomes two turns against the quota and two runs on the GPU.
    SELECT job.* INTO v_existing
    FROM public.apocrypha_job AS job
    WHERE job.tenant_id = v_identity.tenant_id
      AND job.owner_principal_id = v_identity.principal_id
      AND job.idempotency_scope = 'guest-chat'
      AND job.idempotency_key = p_request_id::text
    FOR UPDATE;

    IF FOUND THEN
        RETURN QUERY SELECT v_existing.id, p_request_id, v_existing.status,
                            v_existing.created_at, true;
        RETURN;
    END IF;

    IF EXISTS (
        SELECT 1 FROM public.apocrypha_job AS job
        WHERE job.tenant_id = v_identity.tenant_id
          AND job.owner_principal_id = v_identity.principal_id
          AND job.status IN ('queued', 'leased', 'running', 'cancel_requested')
    ) THEN
        RAISE EXCEPTION 'guest chat principal already has active work' USING ERRCODE = 'P4091';
    END IF;

    -- Deliberately tighter than the member allowance of 30/hour. The engine behind this is one
    -- local GPU and a guest is unauthenticated: public reach is not unbounded public throughput.
    SELECT count(*) INTO v_recent
    FROM public.apocrypha_job AS job
    WHERE job.tenant_id = v_identity.tenant_id
      AND job.owner_principal_id = v_identity.principal_id
      AND job.created_at >= now() - interval '1 hour';

    IF v_recent >= 12 THEN
        RAISE EXCEPTION 'guest chat rolling quota exceeded: 12 requests per hour'
            USING ERRCODE = 'P4290';
    END IF;

    -- History is CLIENT-supplied for guests, because nothing about a guest turn is retained
    -- server-side. It is bounded and typed here rather than trusted: an unbounded array from an
    -- unauthenticated caller is a prompt-stuffing lever on a shared GPU.
    v_history := '[]'::jsonb;
    IF p_history IS NOT NULL AND jsonb_typeof(p_history) = 'array' THEN
        SELECT coalesce(jsonb_agg(entry ORDER BY ordinality), '[]'::jsonb) INTO v_history
        FROM (
            SELECT jsonb_build_object(
                       'role', CASE WHEN item->>'role' = 'assistant' THEN 'assistant' ELSE 'user' END,
                       'content', left(coalesce(item->>'content', ''), 4000)
                   ) AS entry,
                   ordinality
            FROM jsonb_array_elements(p_history) WITH ORDINALITY AS t(item, ordinality)
            WHERE jsonb_typeof(item) = 'object'
              AND coalesce(item->>'content', '') <> ''
            ORDER BY ordinality DESC
            LIMIT 12
        ) AS bounded;
    END IF;

    v_request := jsonb_build_object(
        'question', p_message,
        'conversation_history', v_history,
        'source', 'apocky.com/guest-chat',
        'privacy_class', 'guest-scoped',
        'history_source', 'client-supplied'
    );
    v_request_hash := public.apocrypha_sha256(v_request::text);

    SELECT * INTO v_job
    FROM public.apocrypha_enqueue_job(
        v_identity.tenant_id,
        v_identity.principal_id,
        'apocky_chat',
        'apocky_member_chat',
        v_request,
        v_request_hash,
        'guest-chat',
        p_request_id::text,
        p_model_alias,
        p_profile_hash,
        p_tool_registry_version,
        p_memory_manifest_hash,
        0::smallint,
        2::smallint,
        now(),
        NULL,
        'primary'
    );

    IF v_job.tenant_id <> v_identity.tenant_id
       OR v_job.owner_principal_id <> v_identity.principal_id THEN
        RAISE EXCEPTION 'guest chat job escaped its principal' USING ERRCODE = 'P4031';
    END IF;

    RETURN QUERY SELECT v_job.id, p_request_id, v_job.status, v_job.created_at, false;
END;
$function$;

-- ---------------------------------------------------------------- guest poll
CREATE OR REPLACE FUNCTION public.apocrypha_get_guest_chat_job(
    p_subject_hash text,
    p_job_id uuid
)
RETURNS TABLE(job_id uuid, status text, answer text, error_code text, updated_at timestamptz)
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path TO 'pg_catalog', 'public', 'extensions'
AS $function$
DECLARE
    v_identity record;
    v_job      public.apocrypha_job;
BEGIN
    IF p_job_id IS NULL THEN
        RAISE EXCEPTION 'guest chat job id is required' USING ERRCODE = '23502';
    END IF;

    SELECT ensured.tenant_id, ensured.principal_id INTO v_identity
    FROM public.apocrypha_ensure_guest_principal(p_subject_hash) AS ensured;

    -- Scoped by principal as well as id. A guest who guesses another guest's job id must get
    -- nothing back, not somebody else's answer.
    SELECT job.* INTO v_job
    FROM public.apocrypha_job AS job
    WHERE job.tenant_id = v_identity.tenant_id
      AND job.owner_principal_id = v_identity.principal_id
      AND job.id = p_job_id;

    IF NOT FOUND THEN
        RAISE EXCEPTION 'guest chat job not found for this visitor' USING ERRCODE = 'P4031';
    END IF;

    RETURN QUERY
    SELECT v_job.id,
           v_job.status,
           (SELECT left(revision.content, 20000)
            FROM public.apocrypha_job_revision AS revision
            WHERE revision.id = v_job.terminal_revision_id),
           v_job.error_code,
           v_job.updated_at;
END;
$function$;

REVOKE ALL ON FUNCTION public.apocrypha_ensure_guest_principal(text) FROM PUBLIC, anon, authenticated;
REVOKE ALL ON FUNCTION public.apocrypha_enqueue_guest_chat_v1(text, uuid, text, jsonb, text, text, text, text) FROM PUBLIC, anon, authenticated;
REVOKE ALL ON FUNCTION public.apocrypha_get_guest_chat_job(text, uuid) FROM PUBLIC, anon, authenticated;
GRANT EXECUTE ON FUNCTION public.apocrypha_enqueue_guest_chat_v1(text, uuid, text, jsonb, text, text, text, text) TO service_role;
GRANT EXECUTE ON FUNCTION public.apocrypha_get_guest_chat_job(text, uuid) TO service_role;
