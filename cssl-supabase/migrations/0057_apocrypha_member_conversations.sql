-- Members get more than one conversation.
--
-- WHAT WAS TRUE BEFORE
-- -------------------
-- A member had exactly one conversation, forever, and it was enforced in three
-- places at once:
--
--   * apocrypha_member_chat_conversation had PRIMARY KEY (tenant_id, principal_id)
--     - one row per member, structurally;
--   * a CHECK constraint required conversation_id = auth_user_id;
--   * apocrypha_ensure_member_conversation and apocrypha_enqueue_member_chat_v2
--     both raised P4031 unless the presented id equalled the auth user id.
--
-- The application agreed: AccountChat.tsx opened `account.toLowerCase()` as the
-- conversation and offered no way to start another. So every signed-in member
-- had one permanent thread. Reported 2026-09-10: "they are all the same one
-- single chat."
--
-- WHY IT WAS BUILT THAT WAY, AND WHAT REPLACES IT
-- ----------------------------------------------
-- The equality was doing real work: it made "this conversation belongs to this
-- member" true by construction, with no lookup to get wrong. That guarantee is
-- not negotiable and is NOT being relaxed here - it is being moved.
--
-- Ownership now comes from UNIQUE (tenant_id, conversation_id), which already
-- existed on this table. Because a conversation id can belong to at most one
-- row tenant-wide, "find the row for this conversation id, then check its
-- principal" is total: a member presenting somebody else's conversation id
-- finds that row, fails the principal check, and is refused. A member
-- presenting an unused id creates their own. There is no third case.
--
-- So the invariant is the same one, proved differently:
--   before : conversation_id = auth_user_id            (equality)
--   after  : the row for conversation_id has this principal   (lookup)
--
-- Existing rows are untouched and still satisfy both, so nothing in flight
-- breaks and v2 callers keep working.

DO $migration$
BEGIN
    IF to_regclass('public.apocrypha_member_chat_conversation') IS NULL THEN
        RAISE EXCEPTION
            '0057_apocrypha_member_conversations requires 0049_apocrypha_member_chat_hardening';
    END IF;
END;
$migration$;

-- ── the table stops being one-row-per-member ─────────────────────────────

-- The binding that made a conversation id the member's own id.
ALTER TABLE public.apocrypha_member_chat_conversation
    DROP CONSTRAINT IF EXISTS apocrypha_member_chat_conversation_stable_id;

-- PRIMARY KEY (tenant_id, principal_id) was the structural "one conversation".
-- The scope unique already covers the wider key, so it is promoted and the
-- duplicate dropped rather than leaving two identical indexes behind.
ALTER TABLE public.apocrypha_member_chat_conversation
    DROP CONSTRAINT IF EXISTS apocrypha_member_chat_conversation_primary;
ALTER TABLE public.apocrypha_member_chat_conversation
    DROP CONSTRAINT IF EXISTS apocrypha_member_chat_conversation_scope_unique;
ALTER TABLE public.apocrypha_member_chat_conversation
    ADD CONSTRAINT apocrypha_member_chat_conversation_primary
        PRIMARY KEY (tenant_id, principal_id, conversation_id);

-- UNIQUE (tenant_id, conversation_id) is deliberately KEPT. It is now the
-- entire ownership guarantee: one conversation id, at most one owner.

-- A name the member gave the thread. NULL means "derive one from the first
-- message", which is what the listing does - storing a derived title would go
-- stale the moment the first turn is edited or removed.
ALTER TABLE public.apocrypha_member_chat_conversation
    ADD COLUMN IF NOT EXISTS title text;

ALTER TABLE public.apocrypha_member_chat_conversation
    DROP CONSTRAINT IF EXISTS apocrypha_member_chat_conversation_title_sane;
ALTER TABLE public.apocrypha_member_chat_conversation
    ADD CONSTRAINT apocrypha_member_chat_conversation_title_sane
        CHECK (
            title IS NULL
            OR (
                char_length(title) BETWEEN 1 AND 120
                AND title = btrim(title)
                AND regexp_replace(title, E'[\t\n\r]', '', 'g') !~ '[[:cntrl:]]'
            )
        );

COMMENT ON TABLE public.apocrypha_member_chat_conversation IS
    'One row per member conversation. Ownership is UNIQUE (tenant_id, conversation_id): a conversation id has at most one owning principal.';

-- ── open a conversation, creating it if it is the member''s to create ────

CREATE OR REPLACE FUNCTION public.apocrypha_open_member_conversation(
    p_verified_auth_user_id uuid,
    p_conversation_id uuid
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
    IF p_verified_auth_user_id IS NULL OR p_conversation_id IS NULL THEN
        RAISE EXCEPTION 'member conversation identity is required'
            USING ERRCODE = 'P4031';
    END IF;

    SELECT ensured.tenant_id, ensured.principal_id INTO v_identity
    FROM public.apocrypha_ensure_member_principal(
        p_verified_auth_user_id
    ) AS ensured;

    -- Looked up by conversation id ALONE, across the whole tenant, and locked.
    -- Scoping this query by principal would be the bug: a conversation owned by
    -- somebody else would simply not be found, and the INSERT below would then
    -- fail on the unique constraint with a confusing error instead of a clean
    -- refusal - or, if that constraint were ever dropped, succeed.
    SELECT conversation.* INTO v_conversation
    FROM public.apocrypha_member_chat_conversation AS conversation
    WHERE conversation.tenant_id = v_identity.tenant_id
      AND conversation.conversation_id = p_conversation_id
    FOR UPDATE;

    IF FOUND THEN
        IF v_conversation.principal_id <> v_identity.principal_id
           OR v_conversation.auth_user_id <> p_verified_auth_user_id THEN
            RAISE EXCEPTION 'member conversation is not owned by the verified identity'
                USING ERRCODE = 'P4031';
        END IF;
    ELSE
        INSERT INTO public.apocrypha_member_chat_conversation (
            tenant_id, principal_id, auth_user_id, conversation_id
        ) VALUES (
            v_identity.tenant_id,
            v_identity.principal_id,
            p_verified_auth_user_id,
            p_conversation_id
        )
        RETURNING * INTO v_conversation;
    END IF;

    RETURN QUERY SELECT
        v_conversation.tenant_id,
        v_conversation.principal_id,
        v_conversation.conversation_id;
END;
$$;

-- ── list what the member has ─────────────────────────────────────────────

CREATE OR REPLACE FUNCTION public.apocrypha_list_member_conversations(
    p_verified_auth_user_id uuid,
    p_limit integer DEFAULT 50
)
RETURNS TABLE (
    conversation_id  uuid,
    title            text,
    turn_count       bigint,
    created_at       timestamptz,
    last_activity_at timestamptz
)
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog, public, extensions
AS $$
DECLARE
    v_identity record;
    v_limit    integer := LEAST(GREATEST(COALESCE(p_limit, 50), 1), 200);
BEGIN
    IF p_verified_auth_user_id IS NULL THEN
        RAISE EXCEPTION 'member identity is required' USING ERRCODE = 'P4031';
    END IF;

    SELECT ensured.tenant_id, ensured.principal_id INTO v_identity
    FROM public.apocrypha_ensure_member_principal(
        p_verified_auth_user_id
    ) AS ensured;

    RETURN QUERY
    SELECT
        conversation.conversation_id,
        -- The stored title if the member set one, else the opening message
        -- trimmed to something that fits a sidebar. Derived at read time on
        -- purpose: a stored derivation goes stale and then lies.
        COALESCE(
            conversation.title,
            NULLIF(btrim(left(first_turn.user_message, 60)), ''),
            'New conversation'
        ) AS title,
        COALESCE(activity.turn_count, 0) AS turn_count,
        conversation.created_at,
        COALESCE(activity.last_activity_at, conversation.created_at) AS last_activity_at
    FROM public.apocrypha_member_chat_conversation AS conversation
    LEFT JOIN LATERAL (
        SELECT count(*) AS turn_count, max(request.created_at) AS last_activity_at
        FROM public.apocrypha_member_chat_request AS request
        WHERE request.tenant_id = conversation.tenant_id
          AND request.principal_id = conversation.principal_id
          AND request.conversation_id = conversation.conversation_id
    ) AS activity ON TRUE
    LEFT JOIN LATERAL (
        SELECT request.user_message
        FROM public.apocrypha_member_chat_request AS request
        WHERE request.tenant_id = conversation.tenant_id
          AND request.principal_id = conversation.principal_id
          AND request.conversation_id = conversation.conversation_id
        ORDER BY request.turn_sequence ASC
        LIMIT 1
    ) AS first_turn ON TRUE
    WHERE conversation.tenant_id = v_identity.tenant_id
      AND conversation.principal_id = v_identity.principal_id
    ORDER BY COALESCE(activity.last_activity_at, conversation.created_at) DESC
    LIMIT v_limit;
END;
$$;

-- ── name a conversation ──────────────────────────────────────────────────

CREATE OR REPLACE FUNCTION public.apocrypha_rename_member_conversation(
    p_verified_auth_user_id uuid,
    p_conversation_id uuid,
    p_title text
)
RETURNS TABLE (conversation_id uuid, title text)
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog, public, extensions
AS $$
DECLARE
    v_identity record;
    v_title    text := NULLIF(btrim(COALESCE(p_title, '')), '');
BEGIN
    SELECT opened.tenant_id, opened.principal_id INTO v_identity
    FROM public.apocrypha_open_member_conversation(
        p_verified_auth_user_id, p_conversation_id
    ) AS opened;

    IF v_title IS NOT NULL AND char_length(v_title) > 120 THEN
        v_title := left(v_title, 120);
    END IF;

    UPDATE public.apocrypha_member_chat_conversation AS conversation
    SET title = v_title
    WHERE conversation.tenant_id = v_identity.tenant_id
      AND conversation.principal_id = v_identity.principal_id
      AND conversation.conversation_id = p_conversation_id;

    RETURN QUERY SELECT p_conversation_id, v_title;
END;
$$;

-- ── the admission and history paths bind by ownership, not by equality ───

CREATE OR REPLACE FUNCTION public.apocrypha_ensure_member_conversation(
    p_verified_auth_user_id uuid
)
RETURNS TABLE (tenant_id uuid, principal_id uuid, conversation_id uuid)
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog, public, extensions
AS $$
BEGIN
    -- Kept for callers that never presented an id. It now means "the member's
    -- long-standing conversation", which is the one whose id equals their auth
    -- user id - exactly the row every member already had.
    RETURN QUERY
    SELECT opened.tenant_id, opened.principal_id, opened.conversation_id
    FROM public.apocrypha_open_member_conversation(
        p_verified_auth_user_id, p_verified_auth_user_id
    ) AS opened;
END;
$$;

DO $grants$
BEGIN
    -- Same grant shape as the functions these sit beside: the same-origin
    -- Next server calls them with the service role and nothing else may.
    EXECUTE 'REVOKE ALL ON FUNCTION public.apocrypha_open_member_conversation(uuid, uuid) FROM PUBLIC, anon, authenticated';
    EXECUTE 'REVOKE ALL ON FUNCTION public.apocrypha_list_member_conversations(uuid, integer) FROM PUBLIC, anon, authenticated';
    EXECUTE 'REVOKE ALL ON FUNCTION public.apocrypha_rename_member_conversation(uuid, uuid, text) FROM PUBLIC, anon, authenticated';
    EXECUTE 'GRANT EXECUTE ON FUNCTION public.apocrypha_open_member_conversation(uuid, uuid) TO service_role';
    EXECUTE 'GRANT EXECUTE ON FUNCTION public.apocrypha_list_member_conversations(uuid, integer) TO service_role';
    EXECUTE 'GRANT EXECUTE ON FUNCTION public.apocrypha_rename_member_conversation(uuid, uuid, text) TO service_role';
EXCEPTION WHEN undefined_object THEN
    -- A database without the Supabase roles (a bare test instance) still
    -- applies the schema; the grants are the deployment's concern.
    NULL;
END;
$grants$;

-- ── v2 admission and history, rebound to ownership ──────────────────────
--
-- Lifted verbatim from 0049 with exactly two changes each, so the behaviour
-- everything already depends on is unchanged apart from the binding:
--   1. the `presented <> verified` equality guard becomes a null check;
--   2. apocrypha_ensure_member_conversation(auth) becomes
--      apocrypha_open_member_conversation(auth, presented).
-- Everything else - the replay resolution, the lock, the quota checks, the
-- message validation - is byte-for-byte what was there.

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
    -- The presented id no longer has to BE the auth user id. It has to be a
    -- conversation this principal owns, which apocrypha_open_member_conversation
    -- below decides by looking the id up tenant-wide and checking its owner.
    -- Same refusal, same P4031, proved by lookup instead of by equality.
    IF p_verified_auth_user_id IS NULL
       OR p_presented_conversation_id IS NULL THEN
        RAISE EXCEPTION 'member conversation identity is required'
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
    FROM public.apocrypha_open_member_conversation(
        p_verified_auth_user_id,
        p_presented_conversation_id
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
    -- The presented id no longer has to BE the auth user id. It has to be a
    -- conversation this principal owns, which apocrypha_open_member_conversation
    -- below decides by looking the id up tenant-wide and checking its owner.
    -- Same refusal, same P4031, proved by lookup instead of by equality.
    IF p_verified_auth_user_id IS NULL
       OR p_presented_conversation_id IS NULL THEN
        RAISE EXCEPTION 'member conversation identity is required'
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
    FROM public.apocrypha_open_member_conversation(
        p_verified_auth_user_id,
        p_presented_conversation_id
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
