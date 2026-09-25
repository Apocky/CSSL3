-- 0060 · the function half of 0057_apocrypha_member_threads_lane_attachments.
--
-- WHAT HAPPENED: 0057 (threads, flagship lane, attachments, consent, telemetry) was run through the
-- Supabase SQL Editor, which splits plpgsql bodies. Measured on the hub 2026-09-25: every TABLE,
-- COLUMN, CONSTRAINT, INDEX and the apocrypha-attachments bucket from 0057 exist; of its twelve
-- functions only apocrypha_set_member_consent and apocrypha_member_has_flagship landed. The member
-- routes therefore ran their v2 fallbacks.
--
-- This file is lines 97-end of 0057 verbatim: every CREATE OR REPLACE FUNCTION plus the REVOKE /
-- GRANT block. It is idempotent (re-running replaces the same bodies) and touches no table, so it
-- is safe on a database where 0057's DDL half is present. Apply with a real Postgres session
-- (pooler, session mode), never the SQL Editor.
--
-- SECURITY FIX vs 0057: its REVOKE lines named PUBLIC and authenticated but not anon, and Supabase
-- grants EXECUTE to anon by default, so every one of these SECURITY DEFINER functions -- which trust
-- p_verified_auth_user_id -- was callable with the public anon key (measured 2026-09-25: 12/12).
-- Here each REVOKE also names anon.
--
-- Also note: there are TWO files numbered 0057. 0057_apocrypha_member_conversations (several
-- conversations per member, spoken through v2) was applied 2026-09-10; this lane's 0057 keeps one
-- conversation with threads inside it (v3). lib/apocrypha/member-chat.ts routes between them.

CREATE OR REPLACE FUNCTION public.apocrypha_set_member_consent(p_verified_auth_user_id uuid, p_analytics boolean)
RETURNS TABLE (auth_user_id uuid, analytics boolean, updated_at timestamptz)
LANGUAGE sql SECURITY DEFINER SET search_path = pg_catalog, public AS $$
    INSERT INTO public.apocrypha_member_consent (auth_user_id, analytics, updated_at)
    VALUES (p_verified_auth_user_id, coalesce(p_analytics, false), now())
    ON CONFLICT (auth_user_id) DO UPDATE SET analytics = EXCLUDED.analytics, updated_at = now()
    RETURNING auth_user_id, analytics, updated_at;
$$;

CREATE OR REPLACE FUNCTION public.apocrypha_record_analytics_event(p_auth_user_id uuid, p_kind text, p_props jsonb)
RETURNS boolean
LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog, public AS $$
BEGIN
    -- Anonymous events (no member) carry no identity and are kept; member events need consent.
    IF p_auth_user_id IS NOT NULL AND NOT EXISTS (
        SELECT 1 FROM public.apocrypha_member_consent c WHERE c.auth_user_id = p_auth_user_id AND c.analytics
    ) THEN
        RETURN false;
    END IF;
    INSERT INTO public.apocrypha_analytics_event (auth_user_id, kind, props)
    VALUES (p_auth_user_id, p_kind, coalesce(p_props, '{}'::jsonb));
    RETURN true;
END;
$$;

-- ─── entitlement: the premium plan unlocks the flagship lane ─────────────────
CREATE OR REPLACE FUNCTION public.apocrypha_member_has_flagship(p_verified_auth_user_id uuid)
RETURNS boolean
LANGUAGE sql STABLE SECURITY DEFINER SET search_path = pg_catalog, public AS $$
    SELECT EXISTS (
        SELECT 1 FROM public.entitlements e
        WHERE e.player_id = p_verified_auth_user_id
          AND e.product_id = 'apocrypha-premium'
          AND e.cancelled_at IS NULL
          AND (e.expires_at IS NULL OR e.expires_at > now())
    );
$$;

-- ─── threads: ensure / list / create / update ────────────────────────────────
CREATE OR REPLACE FUNCTION public.apocrypha_ensure_member_thread(p_verified_auth_user_id uuid, p_thread_id uuid)
RETURNS public.apocrypha_member_chat_thread
LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog, public, extensions AS $$
DECLARE
    v_identity record;
    v_thread   public.apocrypha_member_chat_thread;
BEGIN
    SELECT ensured.tenant_id, ensured.principal_id, ensured.conversation_id INTO v_identity
    FROM public.apocrypha_ensure_member_conversation(p_verified_auth_user_id) AS ensured;
    IF p_thread_id IS NOT NULL THEN
        SELECT t.* INTO v_thread FROM public.apocrypha_member_chat_thread t
        WHERE t.tenant_id = v_identity.tenant_id AND t.principal_id = v_identity.principal_id AND t.id = p_thread_id;
        IF NOT FOUND THEN
            RAISE EXCEPTION 'member thread does not belong to the verified member' USING ERRCODE = 'P4031';
        END IF;
        RETURN v_thread;
    END IF;
    -- No thread named: the most recent open one, else a fresh one.
    SELECT t.* INTO v_thread FROM public.apocrypha_member_chat_thread t
    WHERE t.tenant_id = v_identity.tenant_id AND t.principal_id = v_identity.principal_id AND t.archived_at IS NULL
    ORDER BY t.last_active_at DESC LIMIT 1;
    IF FOUND THEN RETURN v_thread; END IF;
    INSERT INTO public.apocrypha_member_chat_thread (tenant_id, principal_id, conversation_id)
    VALUES (v_identity.tenant_id, v_identity.principal_id, v_identity.conversation_id)
    RETURNING * INTO v_thread;
    -- Legacy turns (before threads) belong to the first thread, so history stays whole.
    UPDATE public.apocrypha_member_chat_request r SET thread_id = v_thread.id
    WHERE r.tenant_id = v_identity.tenant_id AND r.principal_id = v_identity.principal_id AND r.thread_id IS NULL;
    RETURN v_thread;
END;
$$;

CREATE OR REPLACE FUNCTION public.apocrypha_create_member_thread(p_verified_auth_user_id uuid, p_title text)
RETURNS public.apocrypha_member_chat_thread
LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog, public, extensions AS $$
DECLARE
    v_identity record;
    v_thread   public.apocrypha_member_chat_thread;
    v_open     integer;
BEGIN
    SELECT ensured.tenant_id, ensured.principal_id, ensured.conversation_id INTO v_identity
    FROM public.apocrypha_ensure_member_conversation(p_verified_auth_user_id) AS ensured;
    SELECT count(*) INTO v_open FROM public.apocrypha_member_chat_thread t
    WHERE t.tenant_id = v_identity.tenant_id AND t.principal_id = v_identity.principal_id AND t.archived_at IS NULL;
    IF v_open >= 200 THEN
        RAISE EXCEPTION 'member has too many open threads; archive some first' USING ERRCODE = 'P4290';
    END IF;
    INSERT INTO public.apocrypha_member_chat_thread (tenant_id, principal_id, conversation_id, title)
    VALUES (v_identity.tenant_id, v_identity.principal_id, v_identity.conversation_id,
            coalesce(nullif(btrim(coalesce(p_title, '')), ''), 'New conversation'))
    RETURNING * INTO v_thread;
    RETURN v_thread;
END;
$$;

CREATE OR REPLACE FUNCTION public.apocrypha_list_member_threads(p_verified_auth_user_id uuid, p_include_archived boolean DEFAULT false)
RETURNS TABLE (
    thread_id uuid, title text, pinned boolean, archived boolean,
    created_at timestamptz, last_active_at timestamptz, turn_count bigint, preview text
)
LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog, public, extensions AS $$
DECLARE
    v_identity record;
BEGIN
    SELECT ensured.tenant_id, ensured.principal_id INTO v_identity
    FROM public.apocrypha_ensure_member_conversation(p_verified_auth_user_id) AS ensured;
    PERFORM public.apocrypha_ensure_member_thread(p_verified_auth_user_id, NULL);
    RETURN QUERY
    SELECT t.id, t.title, t.pinned_at IS NOT NULL, t.archived_at IS NOT NULL, t.created_at, t.last_active_at,
           (SELECT count(*) FROM public.apocrypha_member_chat_request r
             WHERE r.tenant_id = t.tenant_id AND r.principal_id = t.principal_id AND r.thread_id = t.id),
           (SELECT left(r.user_message, 120) FROM public.apocrypha_member_chat_request r
             WHERE r.tenant_id = t.tenant_id AND r.principal_id = t.principal_id AND r.thread_id = t.id
             ORDER BY r.turn_sequence DESC LIMIT 1)
    FROM public.apocrypha_member_chat_thread t
    WHERE t.tenant_id = v_identity.tenant_id AND t.principal_id = v_identity.principal_id
      AND (p_include_archived OR t.archived_at IS NULL)
    ORDER BY (t.pinned_at IS NOT NULL) DESC, t.pinned_at DESC NULLS LAST, t.last_active_at DESC
    LIMIT 500;
END;
$$;

CREATE OR REPLACE FUNCTION public.apocrypha_update_member_thread(
    p_verified_auth_user_id uuid, p_thread_id uuid, p_title text, p_pinned boolean, p_archived boolean)
RETURNS public.apocrypha_member_chat_thread
LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog, public, extensions AS $$
DECLARE
    v_thread public.apocrypha_member_chat_thread;
BEGIN
    v_thread := public.apocrypha_ensure_member_thread(p_verified_auth_user_id, p_thread_id);
    UPDATE public.apocrypha_member_chat_thread t SET
        title       = CASE WHEN p_title IS NULL THEN t.title ELSE left(btrim(p_title), 120) END,
        pinned_at   = CASE WHEN p_pinned IS NULL THEN t.pinned_at WHEN p_pinned THEN coalesce(t.pinned_at, now()) ELSE NULL END,
        archived_at = CASE WHEN p_archived IS NULL THEN t.archived_at WHEN p_archived THEN coalesce(t.archived_at, now()) ELSE NULL END
    WHERE t.id = v_thread.id
    RETURNING * INTO v_thread;
    RETURN v_thread;
END;
$$;

-- ─── attachments: register (bytes already in storage) / read for a job ──────
CREATE OR REPLACE FUNCTION public.apocrypha_register_member_attachment(
    p_verified_auth_user_id uuid, p_thread_id uuid, p_file_name text, p_mime_type text,
    p_byte_size integer, p_storage_path text, p_extracted_text text)
RETURNS public.apocrypha_member_chat_attachment
LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog, public, extensions AS $$
DECLARE
    v_thread public.apocrypha_member_chat_thread;
    v_row    public.apocrypha_member_chat_attachment;
BEGIN
    v_thread := public.apocrypha_ensure_member_thread(p_verified_auth_user_id, p_thread_id);
    INSERT INTO public.apocrypha_member_chat_attachment
        (tenant_id, principal_id, thread_id, file_name, mime_type, byte_size, storage_path, extracted_text)
    VALUES (v_thread.tenant_id, v_thread.principal_id, v_thread.id, p_file_name, p_mime_type, p_byte_size, p_storage_path,
            left(p_extracted_text, 65536))
    RETURNING * INTO v_row;
    RETURN v_row;
END;
$$;

-- ─── enqueue v3: thread + lane + attachments, the 0049 admission rules kept ──
CREATE OR REPLACE FUNCTION public.apocrypha_enqueue_member_chat_v3(
    p_verified_auth_user_id uuid,
    p_presented_conversation_id uuid,
    p_request_id uuid,
    p_message text,
    p_model_alias text,
    p_profile_hash text,
    p_tool_registry_version text,
    p_memory_manifest_hash text,
    p_thread_id uuid,
    p_engine_lane text,
    p_attachment_ids uuid[]
)
RETURNS TABLE (
    job_id uuid, conversation_id uuid, request_id uuid, status text, model_alias text,
    memory_manifest_hash text, created_at timestamptz, updated_at timestamptz, replayed boolean,
    thread_id uuid, engine_lane text
)
LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog, public, extensions AS $$
DECLARE
    v_identity      record;
    v_thread        public.apocrypha_member_chat_thread;
    v_existing      public.apocrypha_member_chat_request;
    v_job           public.apocrypha_job;
    v_request       jsonb;
    v_request_hash  text;
    v_history       jsonb;
    v_attachments   jsonb;
    v_inserted_id   uuid;
    v_turn_sequence bigint;
    v_recent_count  integer;
    v_lane          text := coalesce(p_engine_lane, 'local');
BEGIN
    IF p_verified_auth_user_id IS NULL OR p_presented_conversation_id IS NULL
       OR p_presented_conversation_id <> p_verified_auth_user_id THEN
        RAISE EXCEPTION 'member conversation does not match verified auth identity' USING ERRCODE = 'P4031';
    END IF;
    IF p_request_id IS NULL THEN
        RAISE EXCEPTION 'member chat request id is required' USING ERRCODE = '23502';
    END IF;
    IF p_message IS NULL OR p_message <> btrim(p_message) OR char_length(p_message) = 0
       OR octet_length(p_message) > 16384
       OR regexp_replace(p_message, E'[\t\n\r]', '', 'g') ~ '[[:cntrl:]]' THEN
        RAISE EXCEPTION 'member chat message contains invalid bytes or control characters' USING ERRCODE = '22023';
    END IF;
    IF v_lane NOT IN ('local', 'flagship') THEN
        RAISE EXCEPTION 'member chat engine lane is invalid' USING ERRCODE = '22023';
    END IF;
    -- The premium switch is enforced here, against the entitlement, not trusted from the client.
    IF v_lane = 'flagship' AND NOT public.apocrypha_member_has_flagship(p_verified_auth_user_id) THEN
        RAISE EXCEPTION 'the flagship lane needs an active Apocrypha Premium plan' USING ERRCODE = 'P4020';
    END IF;

    SELECT ensured.tenant_id, ensured.principal_id, ensured.conversation_id INTO v_identity
    FROM public.apocrypha_ensure_member_conversation(p_verified_auth_user_id) AS ensured;
    v_thread := public.apocrypha_ensure_member_thread(p_verified_auth_user_id, p_thread_id);
    IF v_thread.archived_at IS NOT NULL THEN
        RAISE EXCEPTION 'this thread is archived; restore it to continue' USING ERRCODE = 'P4022';
    END IF;

    PERFORM pg_advisory_xact_lock(hashtextextended('apocky-member-chat-principal:' || v_identity.principal_id::text, 0));

    SELECT request.* INTO v_existing FROM public.apocrypha_member_chat_request AS request
    WHERE request.tenant_id = v_identity.tenant_id AND request.principal_id = v_identity.principal_id
      AND request.conversation_id = v_identity.conversation_id AND request.request_id = p_request_id
    FOR UPDATE;
    IF FOUND THEN
        IF v_existing.user_message IS DISTINCT FROM p_message THEN
            RAISE EXCEPTION 'request id is already bound to different member chat content' USING ERRCODE = '23505';
        END IF;
        SELECT job.* INTO STRICT v_job FROM public.apocrypha_job AS job
        WHERE job.id = v_existing.job_id AND job.tenant_id = v_identity.tenant_id
          AND job.owner_principal_id = v_identity.principal_id;
        RETURN QUERY SELECT v_job.id, v_existing.conversation_id, v_existing.request_id, v_job.status,
            v_job.model_alias, v_job.memory_manifest_hash, v_job.created_at, v_job.updated_at, true,
            v_existing.thread_id, v_existing.engine_lane;
        RETURN;
    END IF;

    IF EXISTS (SELECT 1 FROM public.apocrypha_job AS job
        WHERE job.tenant_id = v_identity.tenant_id AND job.owner_principal_id = v_identity.principal_id
          AND job.kind = 'apocky_chat' AND job.capability = 'apocky_member_chat'
          AND job.status IN ('queued', 'leased', 'running', 'cancel_requested')) THEN
        RAISE EXCEPTION 'member chat principal already has active work' USING ERRCODE = 'P4091';
    END IF;
    SELECT count(*) INTO v_recent_count FROM public.apocrypha_member_chat_request AS request
    WHERE request.tenant_id = v_identity.tenant_id AND request.principal_id = v_identity.principal_id
      AND request.created_at >= now() - interval '1 hour';
    IF v_recent_count >= 30 THEN
        RAISE EXCEPTION 'member chat rolling quota exceeded: 30 requests per hour' USING ERRCODE = 'P4290';
    END IF;

    SELECT coalesce(max(request.turn_sequence), 0) + 1 INTO v_turn_sequence
    FROM public.apocrypha_member_chat_request AS request
    WHERE request.tenant_id = v_identity.tenant_id AND request.principal_id = v_identity.principal_id
      AND request.conversation_id = v_identity.conversation_id;

    -- History is the THREAD's last ten turns, not the whole conversation's.
    WITH recent_requests AS (
        SELECT request.turn_sequence, request.user_message, job.terminal_revision_id
        FROM public.apocrypha_member_chat_request AS request
        JOIN public.apocrypha_job AS job ON job.id = request.job_id AND job.tenant_id = request.tenant_id
         AND job.owner_principal_id = request.principal_id
        WHERE request.tenant_id = v_identity.tenant_id AND request.principal_id = v_identity.principal_id
          AND request.thread_id = v_thread.id
        ORDER BY request.turn_sequence DESC LIMIT 10
    ), chronological_requests AS (SELECT * FROM recent_requests ORDER BY turn_sequence ASC),
    history_messages AS (
        SELECT r.turn_sequence, 0 AS role_order, 'user'::text AS role, left(r.user_message, 10000) AS content
        FROM chronological_requests r
        UNION ALL
        SELECT r.turn_sequence, 1, 'assistant'::text, left(revision.content, 10000)
        FROM chronological_requests r
        JOIN public.apocrypha_job_revision AS revision ON revision.id = r.terminal_revision_id
        WHERE r.terminal_revision_id IS NOT NULL
    )
    SELECT coalesce(jsonb_agg(jsonb_build_object('role', role, 'content', content) ORDER BY turn_sequence, role_order), '[]'::jsonb)
    INTO v_history FROM history_messages;

    -- Attachments must belong to this member and this thread; the model reads their text.
    SELECT coalesce(jsonb_agg(jsonb_build_object(
        'id', a.id, 'name', a.file_name, 'mime', a.mime_type, 'bytes', a.byte_size,
        'storage_path', a.storage_path, 'text', left(coalesce(a.extracted_text, ''), 32768)
    ) ORDER BY a.created_at), '[]'::jsonb) INTO v_attachments
    FROM public.apocrypha_member_chat_attachment a
    WHERE a.tenant_id = v_identity.tenant_id AND a.principal_id = v_identity.principal_id
      AND a.thread_id = v_thread.id AND a.id = ANY (coalesce(p_attachment_ids, '{}'::uuid[]));
    IF jsonb_array_length(v_attachments) <> coalesce(array_length(p_attachment_ids, 1), 0) THEN
        RAISE EXCEPTION 'an attachment does not belong to this thread' USING ERRCODE = 'P4031';
    END IF;

    v_request := jsonb_build_object(
        'question', p_message,
        'conversation_history', v_history,
        'source', 'apocky.com/member-chat',
        'privacy_class', 'principal-scoped',
        'history_source', 'server-projected',
        'conversation_id', v_identity.conversation_id::text,
        'thread_id', v_thread.id::text,
        'engine_lane', v_lane,
        'attachments', v_attachments
    );
    v_request_hash := public.apocrypha_sha256(v_request::text);

    SELECT * INTO v_job FROM public.apocrypha_enqueue_job(
        v_identity.tenant_id, v_identity.principal_id, 'apocky_chat', 'apocky_member_chat',
        v_request, v_request_hash, 'member-chat:' || v_identity.conversation_id::text, p_request_id::text,
        p_model_alias, p_profile_hash, p_tool_registry_version, p_memory_manifest_hash,
        0::smallint, 3::smallint, now(), NULL, 'primary');
    IF v_job.tenant_id <> v_identity.tenant_id OR v_job.owner_principal_id <> v_identity.principal_id
       OR v_job.request_hash <> v_request_hash OR public.apocrypha_sha256(v_job.request::text) <> v_request_hash THEN
        RAISE EXCEPTION 'member chat idempotency key resolved to a foreign job' USING ERRCODE = '23505';
    END IF;
    UPDATE public.apocrypha_job SET metadata = metadata || jsonb_build_object('engine_lane', v_lane, 'thread_id', v_thread.id::text,
        'attachment_count', jsonb_array_length(v_attachments)) WHERE id = v_job.id;

    INSERT INTO public.apocrypha_member_chat_request (
        tenant_id, principal_id, conversation_id, turn_sequence, request_id, job_id, request_hash, user_message,
        thread_id, engine_lane, attachment_ids
    ) VALUES (
        v_identity.tenant_id, v_identity.principal_id, v_identity.conversation_id, v_turn_sequence, p_request_id,
        v_job.id, v_request_hash, p_message, v_thread.id, v_lane, coalesce(p_attachment_ids, '{}'::uuid[])
    )
    ON CONFLICT (tenant_id, principal_id, conversation_id, request_id) DO NOTHING
    RETURNING id INTO v_inserted_id;
    IF v_inserted_id IS NULL THEN
        RAISE EXCEPTION 'member chat replay binding conflict' USING ERRCODE = '23505';
    END IF;
    UPDATE public.apocrypha_member_chat_attachment SET used_by_job_id = v_job.id
    WHERE tenant_id = v_identity.tenant_id AND principal_id = v_identity.principal_id
      AND id = ANY (coalesce(p_attachment_ids, '{}'::uuid[])) AND used_by_job_id IS NULL;
    UPDATE public.apocrypha_member_chat_thread SET last_active_at = now(),
        title = CASE WHEN title = 'New conversation' THEN left(btrim(p_message), 80) ELSE title END
    WHERE id = v_thread.id;

    RETURN QUERY SELECT v_job.id, v_identity.conversation_id, p_request_id, v_job.status, v_job.model_alias,
        v_job.memory_manifest_hash, v_job.created_at, v_job.updated_at, false, v_thread.id, v_lane;
END;
$$;

-- ─── history v3: a thread's turns ────────────────────────────────────────────
CREATE OR REPLACE FUNCTION public.apocrypha_list_member_chat_history_v3(
    p_verified_auth_user_id uuid, p_presented_conversation_id uuid, p_thread_id uuid,
    p_before_turn_sequence text DEFAULT NULL, p_limit integer DEFAULT 50)
RETURNS TABLE (
    job_id uuid, conversation_id uuid, request_id uuid, status text, user_message text,
    assistant_message text, assistant_truncated boolean, model_alias text, memory_manifest_hash text,
    created_at timestamptz, updated_at timestamptz, completed_at timestamptz, error_code text,
    turn_cursor text, has_more boolean, thread_id uuid, engine_lane text, attachment_ids uuid[]
)
LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog, public, extensions AS $$
DECLARE
    v_identity record;
    v_thread   public.apocrypha_member_chat_thread;
    v_before   bigint;
BEGIN
    IF p_verified_auth_user_id IS NULL OR p_presented_conversation_id IS NULL
       OR p_presented_conversation_id <> p_verified_auth_user_id THEN
        RAISE EXCEPTION 'member conversation does not match verified auth identity' USING ERRCODE = 'P4031';
    END IF;
    IF p_limit IS NULL OR p_limit < 1 OR p_limit > 50 THEN
        RAISE EXCEPTION 'member chat history limit must be between 1 and 50' USING ERRCODE = '22023';
    END IF;
    IF p_before_turn_sequence IS NOT NULL THEN
        IF p_before_turn_sequence !~ '^[1-9][0-9]{0,18}$' THEN
            RAISE EXCEPTION 'member chat history cursor is invalid' USING ERRCODE = '22023';
        END IF;
        v_before := p_before_turn_sequence::bigint;
    END IF;
    SELECT ensured.tenant_id, ensured.principal_id, ensured.conversation_id INTO v_identity
    FROM public.apocrypha_ensure_member_conversation(p_verified_auth_user_id) AS ensured;
    v_thread := public.apocrypha_ensure_member_thread(p_verified_auth_user_id, p_thread_id);

    RETURN QUERY
    WITH bounded AS (
        SELECT job.id AS job_id, request.conversation_id, request.request_id, job.status, request.user_message,
            CASE WHEN job.status = 'succeeded' THEN left(revision.content, 16384) ELSE NULL END AS assistant_message,
            coalesce(job.status = 'succeeded' AND char_length(revision.content) > 16384, false) AS assistant_truncated,
            job.model_alias, job.memory_manifest_hash, job.created_at, job.updated_at, job.completed_at, job.error_code,
            request.turn_sequence, request.thread_id, request.engine_lane, request.attachment_ids
        FROM public.apocrypha_member_chat_request AS request
        JOIN public.apocrypha_job AS job ON job.id = request.job_id AND job.tenant_id = request.tenant_id
         AND job.owner_principal_id = request.principal_id AND job.kind = 'apocky_chat' AND job.capability = 'apocky_member_chat'
        LEFT JOIN public.apocrypha_job_revision AS revision ON revision.id = job.terminal_revision_id AND revision.job_id = job.id
        WHERE request.tenant_id = v_identity.tenant_id AND request.principal_id = v_identity.principal_id
          AND request.conversation_id = v_identity.conversation_id AND request.thread_id = v_thread.id
          AND (v_before IS NULL OR request.turn_sequence < v_before)
        ORDER BY request.turn_sequence DESC LIMIT (p_limit + 1)
    ), page_state AS (SELECT count(*) > p_limit AS has_more FROM bounded),
    page_rows AS (SELECT * FROM bounded ORDER BY bounded.turn_sequence DESC LIMIT p_limit)
    SELECT r.job_id, r.conversation_id, r.request_id, r.status, r.user_message, r.assistant_message, r.assistant_truncated,
           r.model_alias, r.memory_manifest_hash, r.created_at, r.updated_at, r.completed_at, r.error_code,
           r.turn_sequence::text, s.has_more, r.thread_id, r.engine_lane, r.attachment_ids
    FROM page_rows r CROSS JOIN page_state s ORDER BY r.turn_sequence ASC;
END;
$$;

-- ─── telemetry projection for the admin page ─────────────────────────────────
-- Aggregates only; no message text leaves this function. Latency and tokens come from the
-- worker's revision.usage; lane and thread from the job metadata written at enqueue.
CREATE OR REPLACE FUNCTION public.apocrypha_admin_telemetry(p_days integer DEFAULT 30)
RETURNS TABLE (
    day date, lane text, turns bigint, succeeded bigint, failed bigint,
    latency_p50_s numeric, latency_p95_s numeric, prompt_tokens bigint, completion_tokens bigint,
    cost_usd numeric, tool_calls bigint, withheld bigint, attachments bigint, active_members bigint
)
LANGUAGE sql STABLE SECURITY DEFINER SET search_path = pg_catalog, public AS $$
    SELECT (job.created_at AT TIME ZONE 'UTC')::date AS day,
           coalesce(job.metadata->>'engine_lane', 'local') AS lane,
           count(*) AS turns,
           count(*) FILTER (WHERE job.status = 'succeeded') AS succeeded,
           count(*) FILTER (WHERE job.status = 'failed') AS failed,
           percentile_cont(0.5) WITHIN GROUP (ORDER BY (rev.usage->>'elapsed_s')::numeric)::numeric(10,3) AS latency_p50_s,
           percentile_cont(0.95) WITHIN GROUP (ORDER BY (rev.usage->>'elapsed_s')::numeric)::numeric(10,3) AS latency_p95_s,
           coalesce(sum((rev.usage->>'prompt_tokens')::bigint), 0) AS prompt_tokens,
           coalesce(sum((rev.usage->>'completion_tokens')::bigint), 0) AS completion_tokens,
           coalesce(sum((rev.usage->>'total_cost_usd')::numeric), 0)::numeric(12,6) AS cost_usd,
           coalesce(sum(jsonb_array_length(coalesce(rev.provenance->'tool_calls', '[]'::jsonb))), 0) AS tool_calls,
           count(*) FILTER (WHERE coalesce((rev.usage->>'withheld')::boolean, false)) AS withheld,
           coalesce(sum((job.metadata->>'attachment_count')::int), 0) AS attachments,
           count(DISTINCT job.owner_principal_id) AS active_members
    FROM public.apocrypha_job job
    LEFT JOIN public.apocrypha_job_revision rev ON rev.id = job.terminal_revision_id
    WHERE job.kind = 'apocky_chat' AND job.created_at >= now() - make_interval(days => greatest(1, least(coalesce(p_days, 30), 365)))
    GROUP BY 1, 2 ORDER BY 1 DESC, 2;
$$;

CREATE OR REPLACE FUNCTION public.apocrypha_admin_analytics(p_days integer DEFAULT 30)
RETURNS TABLE (day date, kind text, events bigint, members bigint)
LANGUAGE sql STABLE SECURITY DEFINER SET search_path = pg_catalog, public AS $$
    SELECT (created_at AT TIME ZONE 'UTC')::date, kind, count(*), count(DISTINCT auth_user_id)
    FROM public.apocrypha_analytics_event
    WHERE created_at >= now() - make_interval(days => greatest(1, least(coalesce(p_days, 30), 365)))
    GROUP BY 1, 2 ORDER BY 1 DESC, 2;
$$;

-- ─── grants: service role only, like every member-chat function before ──────
REVOKE ALL ON FUNCTION public.apocrypha_set_member_consent(uuid, boolean) FROM PUBLIC, anon, authenticated;
REVOKE ALL ON FUNCTION public.apocrypha_record_analytics_event(uuid, text, jsonb) FROM PUBLIC, anon, authenticated;
REVOKE ALL ON FUNCTION public.apocrypha_member_has_flagship(uuid) FROM PUBLIC, anon, authenticated;
REVOKE ALL ON FUNCTION public.apocrypha_ensure_member_thread(uuid, uuid) FROM PUBLIC, anon, authenticated;
REVOKE ALL ON FUNCTION public.apocrypha_create_member_thread(uuid, text) FROM PUBLIC, anon, authenticated;
REVOKE ALL ON FUNCTION public.apocrypha_list_member_threads(uuid, boolean) FROM PUBLIC, anon, authenticated;
REVOKE ALL ON FUNCTION public.apocrypha_update_member_thread(uuid, uuid, text, boolean, boolean) FROM PUBLIC, anon, authenticated;
REVOKE ALL ON FUNCTION public.apocrypha_register_member_attachment(uuid, uuid, text, text, integer, text, text) FROM PUBLIC, anon, authenticated;
REVOKE ALL ON FUNCTION public.apocrypha_enqueue_member_chat_v3(uuid, uuid, uuid, text, text, text, text, text, uuid, text, uuid[]) FROM PUBLIC, anon, authenticated;
REVOKE ALL ON FUNCTION public.apocrypha_list_member_chat_history_v3(uuid, uuid, uuid, text, integer) FROM PUBLIC, anon, authenticated;
REVOKE ALL ON FUNCTION public.apocrypha_admin_telemetry(integer) FROM PUBLIC, anon, authenticated;
REVOKE ALL ON FUNCTION public.apocrypha_admin_analytics(integer) FROM PUBLIC, anon, authenticated;
GRANT EXECUTE ON FUNCTION public.apocrypha_set_member_consent(uuid, boolean) TO service_role;
GRANT EXECUTE ON FUNCTION public.apocrypha_record_analytics_event(uuid, text, jsonb) TO service_role;
GRANT EXECUTE ON FUNCTION public.apocrypha_member_has_flagship(uuid) TO service_role;
GRANT EXECUTE ON FUNCTION public.apocrypha_ensure_member_thread(uuid, uuid) TO service_role;
GRANT EXECUTE ON FUNCTION public.apocrypha_create_member_thread(uuid, text) TO service_role;
GRANT EXECUTE ON FUNCTION public.apocrypha_list_member_threads(uuid, boolean) TO service_role;
GRANT EXECUTE ON FUNCTION public.apocrypha_update_member_thread(uuid, uuid, text, boolean, boolean) TO service_role;
GRANT EXECUTE ON FUNCTION public.apocrypha_register_member_attachment(uuid, uuid, text, text, integer, text, text) TO service_role;
GRANT EXECUTE ON FUNCTION public.apocrypha_enqueue_member_chat_v3(uuid, uuid, uuid, text, text, text, text, text, uuid, text, uuid[]) TO service_role;
GRANT EXECUTE ON FUNCTION public.apocrypha_list_member_chat_history_v3(uuid, uuid, uuid, text, integer) TO service_role;
GRANT EXECUTE ON FUNCTION public.apocrypha_admin_telemetry(integer) TO service_role;
GRANT EXECUTE ON FUNCTION public.apocrypha_admin_analytics(integer) TO service_role;
