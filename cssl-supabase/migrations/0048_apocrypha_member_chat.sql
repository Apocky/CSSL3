-- Durable, principal-bound Apocrypha chat for ordinary signed-in members.
--
-- Trust boundary:
--   * only the same-origin Next server may call these service-role RPCs;
--   * the server supplies an auth user id returned by Supabase getUser();
--   * tenant, principal, capability, model profile, and conversation history
--     are selected or constructed inside this transaction;
--   * browsers supply only opaque conversation/request UUIDs and one message.

DO $migration$
BEGIN
    IF to_regclass('public.apocrypha_job') IS NULL
       OR to_regclass('public.apocrypha_principal') IS NULL
       OR to_regclass('public.apocrypha_job_revision') IS NULL THEN
        RAISE EXCEPTION '0048_apocrypha_member_chat requires 0046_apocrypha_jobs';
    END IF;
END;
$migration$;

CREATE TABLE public.apocrypha_member_chat_request (
    id                  uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id           uuid        NOT NULL,
    principal_id        uuid        NOT NULL,
    conversation_id     uuid        NOT NULL,
    turn_sequence       bigint      NOT NULL,
    request_id          uuid        NOT NULL,
    job_id              uuid        NOT NULL,
    request_hash        text        NOT NULL,
    user_message        text        NOT NULL,
    created_at          timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT apocrypha_member_chat_request_principal_fk
        FOREIGN KEY (tenant_id, principal_id)
        REFERENCES public.apocrypha_principal(tenant_id, id) ON DELETE CASCADE,
    CONSTRAINT apocrypha_member_chat_request_job_fk
        FOREIGN KEY (tenant_id, job_id)
        REFERENCES public.apocrypha_job(tenant_id, id) ON DELETE CASCADE,
    CONSTRAINT apocrypha_member_chat_request_scope_unique
        UNIQUE (tenant_id, principal_id, conversation_id, request_id),
    CONSTRAINT apocrypha_member_chat_request_turn_unique
        UNIQUE (tenant_id, principal_id, conversation_id, turn_sequence),
    CONSTRAINT apocrypha_member_chat_request_job_unique UNIQUE (job_id),
    CONSTRAINT apocrypha_member_chat_request_turn_positive CHECK (turn_sequence > 0),
    CONSTRAINT apocrypha_member_chat_request_hash_shape
        CHECK (request_hash ~ '^[0-9a-f]{64}$'),
    CONSTRAINT apocrypha_member_chat_request_message_shape
        CHECK (
            user_message = btrim(user_message)
            AND char_length(user_message) > 0
            AND octet_length(user_message) <= 16384
        )
);

CREATE INDEX apocrypha_member_chat_request_history
    ON public.apocrypha_member_chat_request (
        tenant_id, principal_id, conversation_id, turn_sequence DESC
    );

CREATE TRIGGER apocrypha_member_chat_request_immutable
    BEFORE UPDATE ON public.apocrypha_member_chat_request
    FOR EACH ROW EXECUTE FUNCTION public.apocrypha_reject_historical_update();

CREATE OR REPLACE FUNCTION public.apocrypha_ensure_member_principal(
    p_verified_auth_user_id uuid
)
RETURNS TABLE (tenant_id uuid, principal_id uuid)
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog, public, extensions
AS $$
DECLARE
    v_tenant       public.apocrypha_tenant;
    v_principal    public.apocrypha_principal;
BEGIN
    IF p_verified_auth_user_id IS NULL THEN
        RAISE EXCEPTION 'verified member auth user is required' USING ERRCODE = '23502';
    END IF;
    IF NOT EXISTS (
        SELECT 1 FROM auth.users AS auth_user
        WHERE auth_user.id = p_verified_auth_user_id
    ) THEN
        RAISE EXCEPTION 'verified member auth user does not exist' USING ERRCODE = '23503';
    END IF;

    PERFORM pg_advisory_xact_lock(
        hashtextextended('apocky-member-principal:' || p_verified_auth_user_id::text, 0)
    );

    INSERT INTO public.apocrypha_tenant (slug, display_name)
    VALUES ('apocky-members', 'Apocky members')
    ON CONFLICT (slug) DO NOTHING;

    SELECT tenant.* INTO v_tenant
    FROM public.apocrypha_tenant AS tenant
    WHERE tenant.slug = 'apocky-members'
    FOR SHARE;

    IF NOT FOUND OR v_tenant.status <> 'active' THEN
        RAISE EXCEPTION 'Apocky member tenant is not active' USING ERRCODE = '55000';
    END IF;

    INSERT INTO public.apocrypha_principal (
        tenant_id, auth_user_id, principal_kind, display_name
    ) VALUES (
        v_tenant.id, p_verified_auth_user_id, 'member', 'Apocky member'
    )
    ON CONFLICT (tenant_id, auth_user_id) WHERE auth_user_id IS NOT NULL
    DO NOTHING;

    SELECT principal.* INTO v_principal
    FROM public.apocrypha_principal AS principal
    WHERE principal.tenant_id = v_tenant.id
      AND principal.auth_user_id = p_verified_auth_user_id
    FOR UPDATE;

    IF NOT FOUND
       OR v_principal.principal_kind <> 'member'
       OR v_principal.status <> 'active' THEN
        RAISE EXCEPTION 'active Apocky member principal is unavailable' USING ERRCODE = '42501';
    END IF;

    RETURN QUERY SELECT v_tenant.id, v_principal.id;
END;
$$;

CREATE OR REPLACE FUNCTION public.apocrypha_enqueue_member_chat(
    p_verified_auth_user_id uuid,
    p_conversation_id uuid,
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
    v_identity      record;
    v_existing      public.apocrypha_member_chat_request;
    v_job           public.apocrypha_job;
    v_request       jsonb;
    v_request_hash  text;
    v_history       jsonb;
    v_inserted_id   uuid;
    v_turn_sequence bigint;
BEGIN
    IF p_conversation_id IS NULL OR p_request_id IS NULL THEN
        RAISE EXCEPTION 'conversation and request ids are required' USING ERRCODE = '23502';
    END IF;
    IF p_message IS NULL
       OR p_message <> btrim(p_message)
       OR char_length(p_message) = 0
       OR octet_length(p_message) > 16384 THEN
        RAISE EXCEPTION 'member chat message must contain 1-16384 canonical UTF-8 bytes'
            USING ERRCODE = '23514';
    END IF;

    SELECT ensured.tenant_id, ensured.principal_id INTO v_identity
    FROM public.apocrypha_ensure_member_principal(p_verified_auth_user_id) AS ensured;

    -- Serialize all turns within one member conversation. This keeps concurrent
    -- submissions from observing different history prefixes and also makes a
    -- lost-response replay deterministic.
    PERFORM pg_advisory_xact_lock(hashtextextended(
        'apocky-member-chat:'
        || v_identity.principal_id::text || ':' || p_conversation_id::text,
        0
    ));

    SELECT request.* INTO v_existing
    FROM public.apocrypha_member_chat_request AS request
    WHERE request.tenant_id = v_identity.tenant_id
      AND request.principal_id = v_identity.principal_id
      AND request.conversation_id = p_conversation_id
      AND request.request_id = p_request_id
    FOR UPDATE;

    IF FOUND THEN
        IF v_existing.user_message IS DISTINCT FROM p_message THEN
            RAISE EXCEPTION 'request id is already bound to different member chat content'
                USING ERRCODE = '23505';
        END IF;

        SELECT job.* INTO STRICT v_job
        FROM public.apocrypha_job AS job
        WHERE job.id = v_existing.job_id
          AND job.tenant_id = v_identity.tenant_id
          AND job.owner_principal_id = v_identity.principal_id;

        IF v_job.kind <> 'apocky_chat'
           OR v_job.capability <> 'apocky_member_chat'
           OR v_job.job_role <> 'primary'
           OR v_job.request_hash <> v_existing.request_hash
           OR public.apocrypha_sha256(v_job.request::text) <> v_existing.request_hash
           OR v_job.parent_job_id IS NOT NULL
           OR v_job.priority <> 0
           OR v_job.max_attempts <> 3
           OR v_job.idempotency_scope <> ('member-chat:' || p_conversation_id::text)
           OR v_job.idempotency_key <> p_request_id::text THEN
            RAISE EXCEPTION 'member chat request is bound to an invalid job rail'
                USING ERRCODE = '42501';
        END IF;

        RETURN QUERY SELECT
            v_job.id, v_existing.conversation_id, v_existing.request_id,
            v_job.status, v_job.model_alias, v_job.memory_manifest_hash,
            v_job.created_at, v_job.updated_at, true;
        RETURN;
    END IF;

    IF EXISTS (
        SELECT 1
        FROM public.apocrypha_member_chat_request AS request
        JOIN public.apocrypha_job AS job
          ON job.id = request.job_id
         AND job.tenant_id = request.tenant_id
         AND job.owner_principal_id = request.principal_id
        WHERE request.tenant_id = v_identity.tenant_id
          AND request.principal_id = v_identity.principal_id
          AND request.conversation_id = p_conversation_id
          AND job.kind = 'apocky_chat'
          AND job.capability = 'apocky_member_chat'
          AND job.status NOT IN ('succeeded', 'failed', 'cancelled')
    ) THEN
        RAISE EXCEPTION 'member chat conversation already has a turn in progress'
            USING ERRCODE = '55000';
    END IF;

    SELECT coalesce(max(request.turn_sequence), 0) + 1 INTO v_turn_sequence
    FROM public.apocrypha_member_chat_request AS request
    WHERE request.tenant_id = v_identity.tenant_id
      AND request.principal_id = v_identity.principal_id
      AND request.conversation_id = p_conversation_id;

    WITH recent_requests AS (
        SELECT request.id, request.turn_sequence, request.user_message, job.terminal_revision_id
        FROM public.apocrypha_member_chat_request AS request
        JOIN public.apocrypha_job AS job
          ON job.id = request.job_id
         AND job.tenant_id = request.tenant_id
         AND job.owner_principal_id = request.principal_id
        WHERE request.tenant_id = v_identity.tenant_id
          AND request.principal_id = v_identity.principal_id
          AND request.conversation_id = p_conversation_id
        ORDER BY request.turn_sequence DESC
        LIMIT 10
    ), chronological_requests AS (
        SELECT * FROM recent_requests ORDER BY turn_sequence ASC
    ), history_messages AS (
        SELECT
            request.turn_sequence,
            0 AS role_order,
            'user'::text AS role,
            left(request.user_message, 10000) AS content
        FROM chronological_requests AS request
        UNION ALL
        SELECT
            request.turn_sequence,
            1 AS role_order,
            'assistant'::text AS role,
            left(revision.content, 10000) AS content
        FROM chronological_requests AS request
        JOIN public.apocrypha_job_revision AS revision
          ON revision.id = request.terminal_revision_id
        WHERE request.terminal_revision_id IS NOT NULL
    )
    SELECT coalesce(
        jsonb_agg(
            jsonb_build_object('role', role, 'content', content)
            ORDER BY turn_sequence, role_order
        ),
        '[]'::jsonb
    ) INTO v_history
    FROM history_messages;

    v_request := jsonb_build_object(
        'question', p_message,
        'conversation_history', v_history,
        'source', 'apocky.com/member-chat',
        'privacy_class', 'principal-scoped',
        'history_source', 'server-projected',
        'conversation_id', p_conversation_id::text
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
        'member-chat:' || p_conversation_id::text,
        p_request_id::text,
        p_model_alias,
        p_profile_hash,
        p_tool_registry_version,
        p_memory_manifest_hash,
        0::smallint,
        3::smallint,
        now(),
        NULL,
        'primary'
    );

    IF v_job.tenant_id <> v_identity.tenant_id
       OR v_job.owner_principal_id <> v_identity.principal_id
       OR v_job.kind <> 'apocky_chat'
       OR v_job.capability <> 'apocky_member_chat'
       OR v_job.job_role <> 'primary'
       OR v_job.parent_job_id IS NOT NULL
       OR v_job.priority <> 0
       OR v_job.max_attempts <> 3
       OR v_job.request_hash <> v_request_hash
       OR public.apocrypha_sha256(v_job.request::text) <> v_request_hash
       OR v_job.model_alias <> p_model_alias
       OR v_job.profile_hash <> lower(p_profile_hash)
       OR v_job.tool_registry_version <> p_tool_registry_version
       OR v_job.memory_manifest_hash <> lower(p_memory_manifest_hash) THEN
        RAISE EXCEPTION 'member chat idempotency key resolved to a foreign job'
            USING ERRCODE = '23505';
    END IF;

    INSERT INTO public.apocrypha_member_chat_request (
        tenant_id, principal_id, conversation_id, turn_sequence, request_id,
        job_id, request_hash, user_message
    ) VALUES (
        v_identity.tenant_id, v_identity.principal_id,
        p_conversation_id, v_turn_sequence, p_request_id,
        v_job.id, v_request_hash, p_message
    )
    ON CONFLICT (tenant_id, principal_id, conversation_id, request_id)
    DO NOTHING
    RETURNING id INTO v_inserted_id;

    IF v_inserted_id IS NULL THEN
        SELECT request.* INTO STRICT v_existing
        FROM public.apocrypha_member_chat_request AS request
        WHERE request.tenant_id = v_identity.tenant_id
          AND request.principal_id = v_identity.principal_id
          AND request.conversation_id = p_conversation_id
          AND request.request_id = p_request_id
        FOR UPDATE;

        IF v_existing.job_id <> v_job.id
           OR v_existing.request_hash <> v_request_hash
           OR v_existing.user_message <> p_message THEN
            RAISE EXCEPTION 'member chat replay binding conflict' USING ERRCODE = '23505';
        END IF;
    END IF;

    RETURN QUERY SELECT
        v_job.id, p_conversation_id, p_request_id,
        v_job.status, v_job.model_alias, v_job.memory_manifest_hash,
        v_job.created_at, v_job.updated_at, v_inserted_id IS NULL;
END;
$$;

CREATE OR REPLACE FUNCTION public.apocrypha_get_member_chat_job(
    p_verified_auth_user_id uuid,
    p_job_id uuid
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
    error_code text
)
LANGUAGE sql
STABLE
SECURITY DEFINER
SET search_path = pg_catalog, public, extensions
AS $$
    SELECT
        job.id,
        request.conversation_id,
        request.request_id,
        job.status,
        request.user_message,
        CASE WHEN job.status = 'succeeded' THEN left(revision.content, 16384) ELSE NULL END,
        coalesce(job.status = 'succeeded' AND char_length(revision.content) > 16384, false),
        job.model_alias,
        job.memory_manifest_hash,
        job.created_at,
        job.updated_at,
        job.completed_at,
        job.error_code
    FROM public.apocrypha_member_chat_request AS request
    JOIN public.apocrypha_tenant AS tenant
      ON tenant.id = request.tenant_id
     AND tenant.slug = 'apocky-members'
     AND tenant.status = 'active'
    JOIN public.apocrypha_principal AS principal
      ON principal.id = request.principal_id
     AND principal.tenant_id = request.tenant_id
     AND principal.auth_user_id = p_verified_auth_user_id
     AND principal.principal_kind = 'member'
     AND principal.status = 'active'
    JOIN public.apocrypha_job AS job
      ON job.id = request.job_id
     AND job.id = p_job_id
     AND job.tenant_id = request.tenant_id
     AND job.owner_principal_id = request.principal_id
     AND job.kind = 'apocky_chat'
     AND job.capability = 'apocky_member_chat'
    LEFT JOIN public.apocrypha_job_revision AS revision
      ON revision.id = job.terminal_revision_id
     AND revision.job_id = job.id;
$$;

CREATE OR REPLACE FUNCTION public.apocrypha_list_member_chat_history(
    p_verified_auth_user_id uuid,
    p_conversation_id uuid
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
    error_code text
)
LANGUAGE sql
STABLE
SECURITY DEFINER
SET search_path = pg_catalog, public, extensions
AS $$
    WITH recent AS (
        SELECT
            job.id AS job_id,
            request.conversation_id,
            request.request_id,
            job.status,
            request.user_message,
            CASE WHEN job.status = 'succeeded' THEN left(revision.content, 16384) ELSE NULL END AS assistant_message,
            coalesce(job.status = 'succeeded' AND char_length(revision.content) > 16384, false) AS assistant_truncated,
            job.model_alias,
            job.memory_manifest_hash,
            job.created_at,
            job.updated_at,
            job.completed_at,
            job.error_code,
            request.turn_sequence
        FROM public.apocrypha_member_chat_request AS request
        JOIN public.apocrypha_tenant AS tenant
          ON tenant.id = request.tenant_id
         AND tenant.slug = 'apocky-members'
         AND tenant.status = 'active'
        JOIN public.apocrypha_principal AS principal
          ON principal.id = request.principal_id
         AND principal.tenant_id = request.tenant_id
         AND principal.auth_user_id = p_verified_auth_user_id
         AND principal.principal_kind = 'member'
         AND principal.status = 'active'
        JOIN public.apocrypha_job AS job
          ON job.id = request.job_id
         AND job.tenant_id = request.tenant_id
         AND job.owner_principal_id = request.principal_id
         AND job.kind = 'apocky_chat'
         AND job.capability = 'apocky_member_chat'
        LEFT JOIN public.apocrypha_job_revision AS revision
          ON revision.id = job.terminal_revision_id
         AND revision.job_id = job.id
        WHERE request.conversation_id = p_conversation_id
        ORDER BY request.turn_sequence DESC
        LIMIT 50
    )
    SELECT
        recent.job_id,
        recent.conversation_id,
        recent.request_id,
        recent.status,
        recent.user_message,
        recent.assistant_message,
        recent.assistant_truncated,
        recent.model_alias,
        recent.memory_manifest_hash,
        recent.created_at,
        recent.updated_at,
        recent.completed_at,
        recent.error_code
    FROM recent
    ORDER BY recent.turn_sequence ASC;
$$;

ALTER TABLE public.apocrypha_member_chat_request ENABLE ROW LEVEL SECURITY;

REVOKE ALL ON TABLE public.apocrypha_member_chat_request
FROM PUBLIC, anon, authenticated, service_role;

REVOKE EXECUTE ON FUNCTION public.apocrypha_ensure_member_principal(uuid)
FROM PUBLIC, anon, authenticated, service_role;
REVOKE EXECUTE ON FUNCTION public.apocrypha_enqueue_member_chat(uuid, uuid, uuid, text, text, text, text, text)
FROM PUBLIC, anon, authenticated;
REVOKE EXECUTE ON FUNCTION public.apocrypha_get_member_chat_job(uuid, uuid)
FROM PUBLIC, anon, authenticated;
REVOKE EXECUTE ON FUNCTION public.apocrypha_list_member_chat_history(uuid, uuid)
FROM PUBLIC, anon, authenticated;

GRANT EXECUTE ON FUNCTION public.apocrypha_enqueue_member_chat(uuid, uuid, uuid, text, text, text, text, text)
TO service_role;
GRANT EXECUTE ON FUNCTION public.apocrypha_get_member_chat_job(uuid, uuid)
TO service_role;
GRANT EXECUTE ON FUNCTION public.apocrypha_list_member_chat_history(uuid, uuid)
TO service_role;

COMMENT ON TABLE public.apocrypha_member_chat_request IS
    'Immutable member conversation/job binding. Inputs are attached only by the service-role member-chat RPC.';
COMMENT ON FUNCTION public.apocrypha_enqueue_member_chat(uuid, uuid, uuid, text, text, text, text, text) IS
    'Atomically provisions a verified auth-user principal, projects server-held history, and replay-safely enqueues apocky_member_chat.';
COMMENT ON FUNCTION public.apocrypha_get_member_chat_job(uuid, uuid) IS
    'Strict member-owned job projection. Foreign or non-member jobs return no row.';
COMMENT ON FUNCTION public.apocrypha_list_member_chat_history(uuid, uuid) IS
    'Durable member-owned conversation projection; history is selected by verified auth identity and conversation.';
