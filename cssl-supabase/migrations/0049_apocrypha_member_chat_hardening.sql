-- Forward-only hardening for the live 0048 member-chat foundation.
--
-- The browser conversation UUID is now only a proof that the caller is bound
-- to the verified Supabase auth subject. The database derives and persists the
-- authoritative conversation UUID from auth.users.id.

DO $migration$
BEGIN
    IF to_regclass('public.apocrypha_member_chat_request') IS NULL
       OR to_regprocedure(
           'public.apocrypha_ensure_member_principal(uuid)'
       ) IS NULL
       OR to_regprocedure(
           'public.apocrypha_enqueue_member_chat(uuid,uuid,uuid,text,text,text,text,text)'
       ) IS NULL
       OR to_regprocedure(
           'public.apocrypha_list_member_chat_history(uuid,uuid)'
       ) IS NULL THEN
        RAISE EXCEPTION '0049_apocrypha_member_chat_hardening requires live 0048_apocrypha_member_chat';
    END IF;
END;
$migration$;

-- Remove the bootstrap cycle between the service-role API and a tenant row
-- that previously existed only after the first successful enqueue.
INSERT INTO public.apocrypha_tenant (slug, display_name)
VALUES ('apocky-members', 'Apocky members')
ON CONFLICT (slug) DO NOTHING;

DO $migration$
BEGIN
    IF NOT EXISTS (
        SELECT 1
        FROM public.apocrypha_tenant AS tenant
        WHERE tenant.slug = 'apocky-members'
          AND tenant.status = 'active'
    ) THEN
        RAISE EXCEPTION '0049 requires the Apocky member tenant to be active'
            USING ERRCODE = '55000';
    END IF;
END;
$migration$;

-- This candidate key lets the conversation registry prove that its auth UUID
-- belongs to the same tenant/principal row, rather than trusting a copied UUID.
ALTER TABLE public.apocrypha_principal
    ADD CONSTRAINT apocrypha_principal_member_conversation_identity_unique
    UNIQUE (tenant_id, id, auth_user_id);

CREATE TABLE public.apocrypha_member_chat_conversation (
    tenant_id       uuid        NOT NULL,
    principal_id    uuid        NOT NULL,
    auth_user_id    uuid        NOT NULL,
    conversation_id uuid        NOT NULL,
    created_at      timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT apocrypha_member_chat_conversation_primary
        PRIMARY KEY (tenant_id, principal_id),
    CONSTRAINT apocrypha_member_chat_conversation_id_unique
        UNIQUE (tenant_id, conversation_id),
    CONSTRAINT apocrypha_member_chat_conversation_scope_unique
        UNIQUE (tenant_id, principal_id, conversation_id),
    CONSTRAINT apocrypha_member_chat_conversation_principal_fk
        FOREIGN KEY (tenant_id, principal_id, auth_user_id)
        REFERENCES public.apocrypha_principal(tenant_id, id, auth_user_id)
        ON DELETE CASCADE,
    CONSTRAINT apocrypha_member_chat_conversation_auth_fk
        FOREIGN KEY (auth_user_id)
        REFERENCES auth.users(id) ON DELETE CASCADE,
    CONSTRAINT apocrypha_member_chat_conversation_stable_id
        CHECK (conversation_id = auth_user_id)
);

CREATE TRIGGER apocrypha_member_chat_conversation_immutable
    BEFORE UPDATE ON public.apocrypha_member_chat_conversation
    FOR EACH ROW EXECUTE FUNCTION public.apocrypha_reject_historical_update();

-- Refuse to rewrite live 0048 history. A noncanonical legacy row blocks this
-- migration with a clear error so an operator can investigate it explicitly.
DO $migration$
BEGIN
    IF EXISTS (
        SELECT 1
        FROM public.apocrypha_member_chat_request AS request
        LEFT JOIN public.apocrypha_principal AS principal
          ON principal.tenant_id = request.tenant_id
         AND principal.id = request.principal_id
        LEFT JOIN public.apocrypha_tenant AS tenant
          ON tenant.id = request.tenant_id
        WHERE tenant.slug IS DISTINCT FROM 'apocky-members'
           OR principal.principal_kind IS DISTINCT FROM 'member'
           OR principal.auth_user_id IS NULL
           OR request.conversation_id IS DISTINCT FROM principal.auth_user_id
    ) THEN
        RAISE EXCEPTION
            '0049 found noncanonical 0048 member history; no rows were rewritten'
            USING ERRCODE = 'P4031';
    END IF;
END;
$migration$;

INSERT INTO public.apocrypha_member_chat_conversation (
    tenant_id, principal_id, auth_user_id, conversation_id
)
SELECT
    principal.tenant_id,
    principal.id,
    principal.auth_user_id,
    principal.auth_user_id
FROM public.apocrypha_principal AS principal
JOIN public.apocrypha_tenant AS tenant
  ON tenant.id = principal.tenant_id
 AND tenant.slug = 'apocky-members'
WHERE principal.principal_kind = 'member'
  AND principal.auth_user_id IS NOT NULL
ON CONFLICT (tenant_id, principal_id) DO NOTHING;

ALTER TABLE public.apocrypha_member_chat_request
    ADD CONSTRAINT apocrypha_member_chat_request_conversation_fk
    FOREIGN KEY (tenant_id, principal_id, conversation_id)
    REFERENCES public.apocrypha_member_chat_conversation(
        tenant_id, principal_id, conversation_id
    ) ON DELETE CASCADE
    NOT VALID;

ALTER TABLE public.apocrypha_member_chat_request
    VALIDATE CONSTRAINT apocrypha_member_chat_request_conversation_fk;

CREATE INDEX apocrypha_member_chat_request_principal_quota
    ON public.apocrypha_member_chat_request (
        tenant_id, principal_id, created_at DESC
    );

CREATE OR REPLACE FUNCTION public.apocrypha_ensure_member_conversation(
    p_verified_auth_user_id uuid
)
RETURNS TABLE (tenant_id uuid, principal_id uuid, conversation_id uuid)
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog, public, extensions
AS $$
DECLARE
    v_identity     record;
    v_conversation public.apocrypha_member_chat_conversation;
BEGIN
    SELECT ensured.tenant_id, ensured.principal_id INTO v_identity
    FROM public.apocrypha_ensure_member_principal(
        p_verified_auth_user_id
    ) AS ensured;

    INSERT INTO public.apocrypha_member_chat_conversation (
        tenant_id, principal_id, auth_user_id, conversation_id
    ) VALUES (
        v_identity.tenant_id,
        v_identity.principal_id,
        p_verified_auth_user_id,
        p_verified_auth_user_id
    )
    ON CONFLICT (tenant_id, principal_id) DO NOTHING;

    SELECT conversation.* INTO v_conversation
    FROM public.apocrypha_member_chat_conversation AS conversation
    WHERE conversation.tenant_id = v_identity.tenant_id
      AND conversation.principal_id = v_identity.principal_id
    FOR UPDATE;

    IF NOT FOUND
       OR v_conversation.auth_user_id <> p_verified_auth_user_id
       OR v_conversation.conversation_id <> p_verified_auth_user_id THEN
        RAISE EXCEPTION 'member conversation identity binding is invalid'
            USING ERRCODE = 'P4031';
    END IF;

    RETURN QUERY SELECT
        v_conversation.tenant_id,
        v_conversation.principal_id,
        v_conversation.conversation_id;
END;
$$;

CREATE OR REPLACE FUNCTION public.apocrypha_enqueue_member_chat_v2(
    p_verified_auth_user_id uuid,
    p_presented_conversation_id uuid,
    p_request_id uuid,
    p_message text,
    p_model_alias text,
    p_profile_hash text,
    p_tool_registry_version text,
    p_memory_manifest_hash text
)
RETURNS TABLE (
    job_id uuid,
    conversation_id uuid,
    request_id uuid,
    status text,
    model_alias text,
    memory_manifest_hash text,
    created_at timestamptz,
    updated_at timestamptz,
    replayed boolean
)
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog, public, extensions
AS $$
DECLARE
    v_identity record;
    v_existing public.apocrypha_member_chat_request;
    v_recent_count integer;
BEGIN
    IF p_verified_auth_user_id IS NULL
       OR p_presented_conversation_id IS NULL
       OR p_presented_conversation_id <> p_verified_auth_user_id THEN
        RAISE EXCEPTION 'member conversation does not match verified auth identity'
            USING ERRCODE = 'P4031';
    END IF;
    IF p_request_id IS NULL THEN
        RAISE EXCEPTION 'member chat request id is required'
            USING ERRCODE = '23502';
    END IF;
    IF p_message IS NULL
       OR p_message <> btrim(p_message)
       OR char_length(p_message) = 0
       OR octet_length(p_message) > 16384
       OR regexp_replace(p_message, E'[\t\n\r]', '', 'g') ~ '[[:cntrl:]]' THEN
        RAISE EXCEPTION
            'member chat message contains invalid bytes or control characters'
            USING ERRCODE = '22023';
    END IF;

    SELECT
        ensured.tenant_id,
        ensured.principal_id,
        ensured.conversation_id
    INTO v_identity
    FROM public.apocrypha_ensure_member_conversation(
        p_verified_auth_user_id
    ) AS ensured;

    -- Every v2 admission for the principal shares one lock. Replays are
    -- resolved before active-work and quota checks.
    PERFORM pg_advisory_xact_lock(hashtextextended(
        'apocky-member-chat-principal:' || v_identity.principal_id::text,
        0
    ));

    SELECT request.* INTO v_existing
    FROM public.apocrypha_member_chat_request AS request
    WHERE request.tenant_id = v_identity.tenant_id
      AND request.principal_id = v_identity.principal_id
      AND request.conversation_id = v_identity.conversation_id
      AND request.request_id = p_request_id
    FOR UPDATE;

    IF FOUND THEN
        RETURN QUERY
        SELECT legacy.*
        FROM public.apocrypha_enqueue_member_chat(
            p_verified_auth_user_id,
            v_identity.conversation_id,
            p_request_id,
            p_message,
            p_model_alias,
            p_profile_hash,
            p_tool_registry_version,
            p_memory_manifest_hash
        ) AS legacy;
        RETURN;
    END IF;

    IF EXISTS (
        SELECT 1
        FROM public.apocrypha_job AS job
        WHERE job.tenant_id = v_identity.tenant_id
          AND job.owner_principal_id = v_identity.principal_id
          AND job.kind = 'apocky_chat'
          AND job.capability = 'apocky_member_chat'
          AND job.status IN ('queued', 'leased', 'running', 'cancel_requested')
    ) THEN
        RAISE EXCEPTION 'member chat principal already has active work'
            USING ERRCODE = 'P4091';
    END IF;

    SELECT count(*) INTO v_recent_count
    FROM public.apocrypha_member_chat_request AS request
    WHERE request.tenant_id = v_identity.tenant_id
      AND request.principal_id = v_identity.principal_id
      AND request.created_at >= now() - interval '1 hour';

    IF v_recent_count >= 30 THEN
        RAISE EXCEPTION 'member chat rolling quota exceeded: 30 requests per hour'
            USING ERRCODE = 'P4290';
    END IF;

    RETURN QUERY
    SELECT admitted.*
    FROM public.apocrypha_enqueue_member_chat(
        p_verified_auth_user_id,
        v_identity.conversation_id,
        p_request_id,
        p_message,
        p_model_alias,
        p_profile_hash,
        p_tool_registry_version,
        p_memory_manifest_hash
    ) AS admitted;
END;
$$;

CREATE OR REPLACE FUNCTION public.apocrypha_list_member_chat_history_v2(
    p_verified_auth_user_id uuid,
    p_presented_conversation_id uuid,
    p_before_turn_sequence text DEFAULT NULL,
    p_limit integer DEFAULT 50
)
RETURNS TABLE (
    job_id uuid,
    conversation_id uuid,
    request_id uuid,
    status text,
    user_message text,
    assistant_message text,
    assistant_truncated boolean,
    model_alias text,
    memory_manifest_hash text,
    created_at timestamptz,
    updated_at timestamptz,
    completed_at timestamptz,
    error_code text,
    turn_cursor text,
    has_more boolean
)
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog, public, extensions
AS $$
DECLARE
    v_identity record;
    v_before bigint;
BEGIN
    IF p_verified_auth_user_id IS NULL
       OR p_presented_conversation_id IS NULL
       OR p_presented_conversation_id <> p_verified_auth_user_id THEN
        RAISE EXCEPTION 'member conversation does not match verified auth identity'
            USING ERRCODE = 'P4031';
    END IF;
    IF p_limit IS NULL OR p_limit < 1 OR p_limit > 50 THEN
        RAISE EXCEPTION 'member chat history limit must be between 1 and 50'
            USING ERRCODE = '22023';
    END IF;
    IF p_before_turn_sequence IS NOT NULL THEN
        IF p_before_turn_sequence !~ '^[1-9][0-9]{0,18}$' THEN
            RAISE EXCEPTION 'member chat history cursor is invalid'
                USING ERRCODE = '22023';
        END IF;
        BEGIN
            v_before := p_before_turn_sequence::bigint;
        EXCEPTION WHEN numeric_value_out_of_range THEN
            RAISE EXCEPTION 'member chat history cursor is invalid'
                USING ERRCODE = '22023';
        END;
    END IF;

    SELECT
        ensured.tenant_id,
        ensured.principal_id,
        ensured.conversation_id
    INTO v_identity
    FROM public.apocrypha_ensure_member_conversation(
        p_verified_auth_user_id
    ) AS ensured;

    RETURN QUERY
    WITH bounded AS (
        SELECT
            job.id AS job_id,
            request.conversation_id,
            request.request_id,
            job.status,
            request.user_message,
            CASE
                WHEN job.status = 'succeeded' THEN left(revision.content, 16384)
                ELSE NULL
            END AS assistant_message,
            coalesce(
                job.status = 'succeeded'
                AND char_length(revision.content) > 16384,
                false
            ) AS assistant_truncated,
            job.model_alias,
            job.memory_manifest_hash,
            job.created_at,
            job.updated_at,
            job.completed_at,
            job.error_code,
            request.turn_sequence
        FROM public.apocrypha_member_chat_request AS request
        JOIN public.apocrypha_job AS job
          ON job.id = request.job_id
         AND job.tenant_id = request.tenant_id
         AND job.owner_principal_id = request.principal_id
         AND job.kind = 'apocky_chat'
         AND job.capability = 'apocky_member_chat'
        LEFT JOIN public.apocrypha_job_revision AS revision
          ON revision.id = job.terminal_revision_id
         AND revision.job_id = job.id
        WHERE request.tenant_id = v_identity.tenant_id
          AND request.principal_id = v_identity.principal_id
          AND request.conversation_id = v_identity.conversation_id
          AND (v_before IS NULL OR request.turn_sequence < v_before)
        ORDER BY request.turn_sequence DESC
        LIMIT (p_limit + 1)
    ), page_state AS (
        SELECT count(*) > p_limit AS has_more FROM bounded
    ), page_rows AS (
        SELECT *
        FROM bounded
        ORDER BY bounded.turn_sequence DESC
        LIMIT p_limit
    )
    SELECT
        page_rows.job_id,
        page_rows.conversation_id,
        page_rows.request_id,
        page_rows.status,
        page_rows.user_message,
        page_rows.assistant_message,
        page_rows.assistant_truncated,
        page_rows.model_alias,
        page_rows.memory_manifest_hash,
        page_rows.created_at,
        page_rows.updated_at,
        page_rows.completed_at,
        page_rows.error_code,
        page_rows.turn_sequence::text,
        page_state.has_more
    FROM page_rows
    CROSS JOIN page_state
    ORDER BY page_rows.turn_sequence ASC;
END;
$$;

ALTER TABLE public.apocrypha_member_chat_conversation ENABLE ROW LEVEL SECURITY;

REVOKE ALL ON TABLE public.apocrypha_member_chat_conversation
FROM PUBLIC, anon, authenticated, service_role;

REVOKE EXECUTE ON FUNCTION public.apocrypha_ensure_member_conversation(uuid)
FROM PUBLIC, anon, authenticated, service_role;

-- 0048 remains callable by its SECURITY DEFINER v2 successor, but the shared
-- service role can no longer select arbitrary conversation UUIDs through it.
REVOKE EXECUTE ON FUNCTION public.apocrypha_enqueue_member_chat(uuid, uuid, uuid, text, text, text, text, text)
FROM service_role;
REVOKE EXECUTE ON FUNCTION public.apocrypha_list_member_chat_history(uuid, uuid)
FROM service_role;

REVOKE EXECUTE ON FUNCTION public.apocrypha_enqueue_member_chat_v2(uuid, uuid, uuid, text, text, text, text, text)
FROM PUBLIC, anon, authenticated, service_role;
REVOKE EXECUTE ON FUNCTION public.apocrypha_list_member_chat_history_v2(uuid, uuid, text, integer)
FROM PUBLIC, anon, authenticated, service_role;

GRANT EXECUTE ON FUNCTION public.apocrypha_enqueue_member_chat_v2(uuid, uuid, uuid, text, text, text, text, text)
TO service_role;
GRANT EXECUTE ON FUNCTION public.apocrypha_list_member_chat_history_v2(uuid, uuid, text, integer)
TO service_role;

-- Executable live postconditions. A privilege regression or incomplete
-- canonical backfill aborts the migration instead of leaving a partial cutover.
DO $verification$
BEGIN
    IF NOT has_function_privilege(
        'service_role',
        'public.apocrypha_enqueue_member_chat_v2(uuid,uuid,uuid,text,text,text,text,text)',
        'EXECUTE'
    ) OR NOT has_function_privilege(
        'service_role',
        'public.apocrypha_list_member_chat_history_v2(uuid,uuid,text,integer)',
        'EXECUTE'
    ) THEN
        RAISE EXCEPTION '0049 verification failed: service role lacks a v2 member RPC'
            USING ERRCODE = '42501';
    END IF;

    IF has_function_privilege(
        'service_role',
        'public.apocrypha_enqueue_member_chat(uuid,uuid,uuid,text,text,text,text,text)',
        'EXECUTE'
    ) OR has_function_privilege(
        'service_role',
        'public.apocrypha_list_member_chat_history(uuid,uuid)',
        'EXECUTE'
    ) THEN
        RAISE EXCEPTION '0049 verification failed: service role retains a client-selected legacy RPC'
            USING ERRCODE = '42501';
    END IF;

    IF NOT has_function_privilege(
        'service_role',
        'public.apocrypha_get_member_chat_job(uuid,uuid)',
        'EXECUTE'
    ) THEN
        RAISE EXCEPTION '0049 verification failed: scoped member job reads were not preserved'
            USING ERRCODE = '42501';
    END IF;

    IF has_function_privilege(
        'authenticated',
        'public.apocrypha_enqueue_member_chat_v2(uuid,uuid,uuid,text,text,text,text,text)',
        'EXECUTE'
    ) OR has_function_privilege(
        'authenticated',
        'public.apocrypha_list_member_chat_history_v2(uuid,uuid,text,integer)',
        'EXECUTE'
    ) OR has_function_privilege(
        'anon',
        'public.apocrypha_enqueue_member_chat_v2(uuid,uuid,uuid,text,text,text,text,text)',
        'EXECUTE'
    ) OR has_function_privilege(
        'anon',
        'public.apocrypha_list_member_chat_history_v2(uuid,uuid,text,integer)',
        'EXECUTE'
    ) THEN
        RAISE EXCEPTION '0049 verification failed: a browser role can execute a v2 member RPC'
            USING ERRCODE = '42501';
    END IF;

    IF EXISTS (
        SELECT 1
        FROM public.apocrypha_principal AS principal
        JOIN public.apocrypha_tenant AS tenant
          ON tenant.id = principal.tenant_id
         AND tenant.slug = 'apocky-members'
        LEFT JOIN public.apocrypha_member_chat_conversation AS conversation
          ON conversation.tenant_id = principal.tenant_id
         AND conversation.principal_id = principal.id
        WHERE principal.principal_kind = 'member'
          AND principal.auth_user_id IS NOT NULL
          AND (
              conversation.principal_id IS NULL
              OR conversation.auth_user_id <> principal.auth_user_id
              OR conversation.conversation_id <> principal.auth_user_id
          )
    ) THEN
        RAISE EXCEPTION '0049 verification failed: canonical member conversation backfill is incomplete'
            USING ERRCODE = 'P4031';
    END IF;

    IF EXISTS (
        SELECT 1
        FROM public.apocrypha_member_chat_request AS request
        JOIN public.apocrypha_member_chat_conversation AS conversation
          ON conversation.tenant_id = request.tenant_id
         AND conversation.principal_id = request.principal_id
         AND conversation.conversation_id = request.conversation_id
        WHERE request.conversation_id <> conversation.auth_user_id
    ) THEN
        RAISE EXCEPTION '0049 verification failed: request history escaped its canonical conversation'
            USING ERRCODE = 'P4031';
    END IF;
END;
$verification$;

COMMENT ON TABLE public.apocrypha_member_chat_conversation IS
    'One server-owned member conversation per principal. Its stable conversation UUID is the verified auth.users UUID.';
COMMENT ON FUNCTION public.apocrypha_ensure_member_conversation(uuid) IS
    'Internal-only canonical member conversation provisioner; never exposed to the service role or browser roles.';
COMMENT ON FUNCTION public.apocrypha_enqueue_member_chat_v2(uuid, uuid, uuid, text, text, text, text, text) IS
    'Canonical member admission: verifies the presented auth UUID, serializes principal work, preserves replay, and enforces a durable 30-per-hour quota.';
COMMENT ON FUNCTION public.apocrypha_list_member_chat_history_v2(uuid, uuid, text, integer) IS
    'Canonical member history projection with a stable exclusive turn cursor and a maximum 50-row page.';
