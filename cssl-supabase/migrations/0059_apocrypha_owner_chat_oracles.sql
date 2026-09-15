-- Reusable production browser-oracle fixtures for owner chat.
--
-- The browser never receives database or worker credentials. A same-origin,
-- owner-authenticated server endpoint supplies the verified owner identity and
-- an idempotency nonce. This migration creates the synthetic failed turn and
-- keeps an immutable manifest of every job belonging to the run. Cleanup is a
-- reversible logical quarantine: no job, revision, event, or user row is
-- physically deleted.

DO $migration$
BEGIN
    IF to_regclass('public.apocrypha_job') IS NULL
       OR to_regclass('public.apocrypha_principal') IS NULL
       OR to_regprocedure('public.apocrypha_sha256(text)') IS NULL
       OR to_regprocedure('public.apocrypha_list_owner_chat_conversations(uuid,uuid,integer)') IS NULL THEN
        RAISE EXCEPTION '0059 owner-chat oracles require durable jobs and owner history';
    END IF;
END;
$migration$;

CREATE TABLE public.apocrypha_owner_chat_oracle_run (
    id                  uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id           uuid        NOT NULL,
    owner_principal_id  uuid        NOT NULL,
    nonce               uuid        NOT NULL,
    conversation_id     uuid        NOT NULL DEFAULT gen_random_uuid(),
    status              text        NOT NULL DEFAULT 'active',
    cleaned_at          timestamptz,
    created_at          timestamptz NOT NULL DEFAULT now(),
    updated_at          timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT apocrypha_owner_chat_oracle_run_owner_fk
        FOREIGN KEY (tenant_id, owner_principal_id)
        REFERENCES public.apocrypha_principal(tenant_id, id) ON DELETE RESTRICT,
    CONSTRAINT apocrypha_owner_chat_oracle_run_nonce_unique
        UNIQUE (tenant_id, owner_principal_id, nonce),
    CONSTRAINT apocrypha_owner_chat_oracle_run_conversation_unique
        UNIQUE (conversation_id),
    CONSTRAINT apocrypha_owner_chat_oracle_run_status_enum
        CHECK (status IN ('active', 'cleaned')),
    CONSTRAINT apocrypha_owner_chat_oracle_run_cleaned_consistency
        CHECK (
            (status = 'cleaned' AND cleaned_at IS NOT NULL)
            OR (status = 'active' AND cleaned_at IS NULL)
        )
);

CREATE TABLE public.apocrypha_owner_chat_oracle_job (
    run_id              uuid        NOT NULL,
    job_id              uuid        NOT NULL,
    job_role            text        NOT NULL,
    request_hash        text        NOT NULL,
    created_at          timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (run_id, job_id),
    CONSTRAINT apocrypha_owner_chat_oracle_job_run_fk
        FOREIGN KEY (run_id)
        REFERENCES public.apocrypha_owner_chat_oracle_run(id) ON DELETE RESTRICT,
    CONSTRAINT apocrypha_owner_chat_oracle_job_job_fk
        FOREIGN KEY (job_id)
        REFERENCES public.apocrypha_job(id) ON DELETE RESTRICT,
    CONSTRAINT apocrypha_owner_chat_oracle_job_unique UNIQUE (job_id),
    CONSTRAINT apocrypha_owner_chat_oracle_job_role_enum
        CHECK (job_role IN ('seed', 'retry')),
    CONSTRAINT apocrypha_owner_chat_oracle_job_request_hash_shape
        CHECK (request_hash ~ '^[0-9a-f]{64}$')
);

CREATE UNIQUE INDEX apocrypha_owner_chat_oracle_one_active_per_owner
    ON public.apocrypha_owner_chat_oracle_run (tenant_id, owner_principal_id)
    WHERE status = 'active';

CREATE OR REPLACE FUNCTION public.apocrypha_guard_owner_chat_oracle_run()
RETURNS trigger
LANGUAGE plpgsql
SET search_path = pg_catalog, public, extensions
AS $$
BEGIN
    IF NEW.id IS DISTINCT FROM OLD.id
       OR NEW.tenant_id IS DISTINCT FROM OLD.tenant_id
       OR NEW.owner_principal_id IS DISTINCT FROM OLD.owner_principal_id
       OR NEW.nonce IS DISTINCT FROM OLD.nonce
       OR NEW.conversation_id IS DISTINCT FROM OLD.conversation_id
       OR NEW.created_at IS DISTINCT FROM OLD.created_at THEN
        RAISE EXCEPTION 'owner-chat oracle identity is immutable'
            USING ERRCODE = '55000';
    END IF;
    IF OLD.status = 'cleaned' AND NEW.status <> 'cleaned' THEN
        RAISE EXCEPTION 'cleaned owner-chat oracle runs cannot be reopened'
            USING ERRCODE = '55000';
    END IF;
    NEW.updated_at := now();
    RETURN NEW;
END;
$$;

CREATE TRIGGER apocrypha_owner_chat_oracle_run_guard
    BEFORE UPDATE ON public.apocrypha_owner_chat_oracle_run
    FOR EACH ROW EXECUTE FUNCTION public.apocrypha_guard_owner_chat_oracle_run();

CREATE TRIGGER apocrypha_owner_chat_oracle_job_immutable
    BEFORE UPDATE ON public.apocrypha_owner_chat_oracle_job
    FOR EACH ROW EXECUTE FUNCTION public.apocrypha_reject_historical_update();

CREATE OR REPLACE FUNCTION public.apocrypha_seed_owner_chat_oracle(
    p_tenant_id uuid,
    p_owner_principal_id uuid,
    p_nonce uuid
)
RETURNS TABLE (run_id uuid, conversation_id uuid, job_id uuid, prompt text)
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog, public, extensions
AS $$
DECLARE
    v_run       public.apocrypha_owner_chat_oracle_run;
    v_job       public.apocrypha_job;
    v_request   jsonb;
    v_prompt    text := 'Feature 002 browser oracle. Reply with exactly: BROWSER ORACLE PASSED';
BEGIN
    IF p_tenant_id IS NULL OR p_owner_principal_id IS NULL OR p_nonce IS NULL
       OR substring(p_nonce::text, 15, 1) <> '4'
       OR substring(p_nonce::text, 20, 1) NOT IN ('8', '9', 'a', 'b') THEN
        RAISE EXCEPTION 'owner-chat oracle input is invalid'
            USING ERRCODE = '22023';
    END IF;

    IF NOT EXISTS (
        SELECT 1
        FROM public.apocrypha_principal AS principal
        JOIN public.apocrypha_tenant AS tenant ON tenant.id = principal.tenant_id
        WHERE principal.tenant_id = p_tenant_id
          AND principal.id = p_owner_principal_id
          AND principal.principal_kind = 'owner'
          AND principal.status = 'active'
          AND tenant.status = 'active'
    ) THEN
        RAISE EXCEPTION 'active owner identity is required'
            USING ERRCODE = 'P4031';
    END IF;

    -- Serialize all seeds for one owner, not merely equal nonces. This makes
    -- the one-active-run admission decision deterministic under concurrency.
    PERFORM pg_advisory_xact_lock(hashtextextended(
        'apocrypha-owner-chat-oracle-owner:' || p_tenant_id::text || ':'
        || p_owner_principal_id::text,
        0
    ));

    SELECT oracle_run.* INTO v_run
    FROM public.apocrypha_owner_chat_oracle_run AS oracle_run
    WHERE oracle_run.tenant_id = p_tenant_id
      AND oracle_run.owner_principal_id = p_owner_principal_id
      AND oracle_run.nonce = p_nonce
    FOR UPDATE;

    IF FOUND THEN
        SELECT job.* INTO v_job
        FROM public.apocrypha_owner_chat_oracle_job AS manifest
        JOIN public.apocrypha_job AS job ON job.id = manifest.job_id
        WHERE manifest.run_id = v_run.id
          AND manifest.job_role = 'seed';
        IF NOT FOUND THEN
            RAISE EXCEPTION 'owner-chat oracle seed manifest is incomplete'
                USING ERRCODE = '55000';
        END IF;
        RETURN QUERY SELECT v_run.id, v_run.conversation_id, v_job.id, v_prompt;
        RETURN;
    END IF;

    IF EXISTS (
        SELECT 1
        FROM public.apocrypha_owner_chat_oracle_run AS oracle_run
        WHERE oracle_run.tenant_id = p_tenant_id
          AND oracle_run.owner_principal_id = p_owner_principal_id
          AND oracle_run.status = 'active'
    ) THEN
        RAISE EXCEPTION 'owner already has an active browser oracle run'
            USING ERRCODE = 'P4091';
    END IF;

    INSERT INTO public.apocrypha_owner_chat_oracle_run (
        tenant_id, owner_principal_id, nonce
    ) VALUES (
        p_tenant_id, p_owner_principal_id, p_nonce
    ) RETURNING * INTO v_run;

    v_request := jsonb_build_object(
        'prompt', v_prompt,
        'messages', jsonb_build_array(jsonb_build_object('role', 'user', 'content', v_prompt)),
        'conversation_id', v_run.conversation_id,
        'conversation_history', '[]'::jsonb,
        'retrieval_query', v_prompt,
        'output_budget', 1536,
        'response_mode', 'standard',
        'source', 'apocky.com',
        'privacy_class', 'restricted',
        'memory_scope', 'none',
        'oracle_run_id', v_run.id,
        'oracle_synthetic', true
    );

    INSERT INTO public.apocrypha_job (
        tenant_id, owner_principal_id, kind, capability, job_role, status,
        request, request_hash, idempotency_scope, idempotency_key,
        priority, max_attempts, model_alias, profile_hash,
        tool_registry_version, memory_manifest_hash,
        completed_at, error_code, error_detail, metadata
    ) VALUES (
        p_tenant_id, p_owner_principal_id, 'apocky_chat', 'apocky_owner_chat',
        'primary', 'failed', v_request, public.apocrypha_sha256(v_request::text),
        'owner-chat:' || v_run.conversation_id::text,
        'oracle-seed:' || v_run.id::text,
        20, 1, 'oracle.synthetic', repeat('0', 64),
        'oracle-v1', repeat('0', 64),
        now(), 'oracle_forced_failure',
        'Synthetic failure created for the signed-in browser acceptance oracle.',
        jsonb_build_object('oracle_run_id', v_run.id, 'oracle_synthetic', true)
    ) RETURNING * INTO v_job;

    INSERT INTO public.apocrypha_owner_chat_oracle_job (
        run_id, job_id, job_role, request_hash
    ) VALUES (
        v_run.id, v_job.id, 'seed', v_job.request_hash
    );

    INSERT INTO public.apocrypha_job_event (
        job_id, ordinal, event_type, outcome, severity, source,
        flagged, alert_eligible, metadata
    ) VALUES (
        v_job.id, 1, 'oracle.synthetic_failure', 'expected_fired', 'info',
        'oracle.seed', false, false,
        jsonb_build_object('oracle_run_id', v_run.id, 'oracle_synthetic', true)
    );

    RETURN QUERY SELECT v_run.id, v_run.conversation_id, v_job.id, v_prompt;
END;
$$;

-- Every oracle-marked retry is bound to its manifest by the same transaction
-- that inserts the durable job. A crash after enqueue therefore cannot leave
-- an unregistered retry that cleanup can neither trust nor quarantine.
CREATE OR REPLACE FUNCTION public.apocrypha_register_owner_chat_oracle_retry()
RETURNS trigger
LANGUAGE plpgsql
SET search_path = pg_catalog, public, extensions
AS $$
DECLARE
    v_run_id       uuid;
    v_retry_of_id  uuid;
    v_run          public.apocrypha_owner_chat_oracle_run;
BEGIN
    IF NEW.kind <> 'apocky_chat' OR NEW.capability <> 'apocky_owner_chat'
       OR NEW.request ->> 'oracle_run_id' IS NULL THEN
        RETURN NEW;
    END IF;

    BEGIN
        v_run_id := (NEW.request ->> 'oracle_run_id')::uuid;
    EXCEPTION WHEN invalid_text_representation THEN
        RAISE EXCEPTION 'oracle retry marker is invalid'
            USING ERRCODE = 'P4031';
    END;

    -- The seed is inserted terminal-failed by the seed RPC and receives its
    -- explicit `seed` manifest row there. It is the sole marked non-retry form.
    IF NEW.request ->> 'retry_of_job_id' IS NULL THEN
        IF NEW.status = 'failed'
           AND NEW.error_code = 'oracle_forced_failure'
           AND NEW.request ->> 'oracle_synthetic' = 'true'
           AND NEW.metadata ->> 'oracle_synthetic' = 'true' THEN
            IF NOT EXISTS (
                SELECT 1
                FROM public.apocrypha_owner_chat_oracle_run AS seed_run
                WHERE seed_run.id = v_run_id
                  AND seed_run.tenant_id = NEW.tenant_id
                  AND seed_run.owner_principal_id = NEW.owner_principal_id
                  AND seed_run.status = 'active'
                  AND seed_run.conversation_id::text = NEW.request ->> 'conversation_id'
            ) THEN
                RAISE EXCEPTION 'synthetic seed does not belong to an active owner-chat oracle run'
                    USING ERRCODE = 'P4031';
            END IF;
            RETURN NEW;
        END IF;
        RAISE EXCEPTION 'oracle marker is valid only for a synthetic seed or registered retry'
            USING ERRCODE = 'P4031';
    END IF;

    BEGIN
        v_retry_of_id := (NEW.request ->> 'retry_of_job_id')::uuid;
    EXCEPTION WHEN invalid_text_representation THEN
        RAISE EXCEPTION 'oracle retry marker is invalid'
            USING ERRCODE = 'P4031';
    END;

    SELECT oracle_run.* INTO v_run
    FROM public.apocrypha_owner_chat_oracle_run AS oracle_run
    WHERE oracle_run.id = v_run_id
      AND oracle_run.tenant_id = NEW.tenant_id
      AND oracle_run.owner_principal_id = NEW.owner_principal_id
      AND oracle_run.status = 'active'
      AND oracle_run.conversation_id::text = NEW.request ->> 'conversation_id'
    FOR UPDATE;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'active owner-chat oracle run does not own this retry'
            USING ERRCODE = 'P4031';
    END IF;

    IF NOT EXISTS (
        SELECT 1
        FROM public.apocrypha_owner_chat_oracle_job AS source_manifest
        JOIN public.apocrypha_job AS source_job ON source_job.id = source_manifest.job_id
        WHERE source_manifest.run_id = v_run.id
          AND source_manifest.job_id = v_retry_of_id
          AND source_job.tenant_id = NEW.tenant_id
          AND source_job.owner_principal_id = NEW.owner_principal_id
          AND source_job.request ->> 'conversation_id' = v_run.conversation_id::text
          AND source_job.request ->> 'oracle_run_id' = v_run.id::text
          AND source_job.status = 'failed'
    ) THEN
        RAISE EXCEPTION 'oracle retry source is not a failed job in this run'
            USING ERRCODE = 'P4031';
    END IF;

    INSERT INTO public.apocrypha_owner_chat_oracle_job (
        run_id, job_id, job_role, request_hash
    ) VALUES (
        v_run.id, NEW.id, 'retry', NEW.request_hash
    );
    RETURN NEW;
END;
$$;

CREATE TRIGGER apocrypha_owner_chat_oracle_retry_register
    AFTER INSERT ON public.apocrypha_job
    FOR EACH ROW EXECUTE FUNCTION public.apocrypha_register_owner_chat_oracle_retry();

CREATE OR REPLACE FUNCTION public.apocrypha_register_owner_chat_oracle_job(
    p_tenant_id uuid,
    p_owner_principal_id uuid,
    p_run_id uuid,
    p_job_id uuid
)
RETURNS TABLE (run_id uuid, job_id uuid, registered boolean)
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog, public, extensions
AS $$
DECLARE
    v_run       public.apocrypha_owner_chat_oracle_run;
    v_job       public.apocrypha_job;
BEGIN
    IF p_tenant_id IS NULL OR p_owner_principal_id IS NULL
       OR p_run_id IS NULL OR p_job_id IS NULL THEN
        RAISE EXCEPTION 'owner-chat oracle registration input is invalid'
            USING ERRCODE = '22023';
    END IF;

    PERFORM pg_advisory_xact_lock(hashtextextended('apocrypha-owner-chat-oracle-run:' || p_run_id::text, 0));

    SELECT oracle_run.* INTO v_run
    FROM public.apocrypha_owner_chat_oracle_run AS oracle_run
    WHERE oracle_run.id = p_run_id
      AND oracle_run.tenant_id = p_tenant_id
      AND oracle_run.owner_principal_id = p_owner_principal_id
    FOR UPDATE;
    IF NOT FOUND OR v_run.status <> 'active' THEN
        RAISE EXCEPTION 'active owner-chat oracle run not found'
            USING ERRCODE = 'P4031';
    END IF;

    SELECT job.* INTO v_job
    FROM public.apocrypha_job AS job
    WHERE job.id = p_job_id
      AND job.tenant_id = p_tenant_id
      AND job.owner_principal_id = p_owner_principal_id
      AND job.kind = 'apocky_chat'
      AND job.capability = 'apocky_owner_chat'
      AND job.request ->> 'conversation_id' = v_run.conversation_id::text
      AND job.request ->> 'oracle_run_id' = v_run.id::text
      AND job.request ->> 'retry_of_job_id' IS NOT NULL;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'job does not belong to this owner-chat oracle run'
            USING ERRCODE = 'P4031';
    END IF;

    INSERT INTO public.apocrypha_owner_chat_oracle_job (
        run_id, job_id, job_role, request_hash
    ) VALUES (
        v_run.id, v_job.id, 'retry', v_job.request_hash
    ) ON CONFLICT (run_id, job_id) DO NOTHING;

    IF EXISTS (
        SELECT 1 FROM public.apocrypha_owner_chat_oracle_job AS manifest
        WHERE manifest.run_id = v_run.id
          AND manifest.job_id = v_job.id
          AND manifest.request_hash = v_job.request_hash
    ) THEN
        RETURN QUERY SELECT v_run.id, v_job.id, true;
        RETURN;
    END IF;
    RAISE EXCEPTION 'owner-chat oracle job manifest conflict'
        USING ERRCODE = '23505';
END;
$$;

CREATE OR REPLACE FUNCTION public.apocrypha_cleanup_owner_chat_oracle(
    p_tenant_id uuid,
    p_owner_principal_id uuid,
    p_run_id uuid
)
RETURNS TABLE (run_id uuid, cleaned boolean)
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog, public, extensions
AS $$
DECLARE
    v_run public.apocrypha_owner_chat_oracle_run;
BEGIN
    IF p_tenant_id IS NULL OR p_owner_principal_id IS NULL OR p_run_id IS NULL THEN
        RAISE EXCEPTION 'owner-chat oracle cleanup input is invalid'
            USING ERRCODE = '22023';
    END IF;

    PERFORM pg_advisory_xact_lock(hashtextextended('apocrypha-owner-chat-oracle-run:' || p_run_id::text, 0));

    SELECT oracle_run.* INTO v_run
    FROM public.apocrypha_owner_chat_oracle_run AS oracle_run
    WHERE oracle_run.id = p_run_id
      AND oracle_run.tenant_id = p_tenant_id
      AND oracle_run.owner_principal_id = p_owner_principal_id
    FOR UPDATE;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'owner-chat oracle run not found'
            USING ERRCODE = 'P4031';
    END IF;
    IF v_run.status = 'cleaned' THEN
        RETURN QUERY SELECT v_run.id, true;
        RETURN;
    END IF;

    IF EXISTS (
        SELECT 1
        FROM public.apocrypha_job AS job
        WHERE job.tenant_id = v_run.tenant_id
          AND job.owner_principal_id = v_run.owner_principal_id
          AND job.kind = 'apocky_chat'
          AND job.capability = 'apocky_owner_chat'
          AND job.request ->> 'conversation_id' = v_run.conversation_id::text
          AND (
              job.request ->> 'oracle_run_id' IS DISTINCT FROM v_run.id::text
              OR NOT EXISTS (
                  SELECT 1
                  FROM public.apocrypha_owner_chat_oracle_job AS manifest
                  WHERE manifest.run_id = v_run.id
                    AND manifest.job_id = job.id
                    AND manifest.request_hash = job.request_hash
              )
          )
    ) THEN
        RAISE EXCEPTION 'owner-chat oracle conversation contains unowned work'
            USING ERRCODE = 'P4091';
    END IF;

    IF EXISTS (
        SELECT 1
        FROM public.apocrypha_owner_chat_oracle_job AS manifest
        JOIN public.apocrypha_job AS job ON job.id = manifest.job_id
        WHERE manifest.run_id = v_run.id
          AND (
              job.tenant_id <> v_run.tenant_id
              OR job.owner_principal_id <> v_run.owner_principal_id
              OR job.kind <> 'apocky_chat'
              OR job.capability <> 'apocky_owner_chat'
              OR job.request ->> 'conversation_id' IS DISTINCT FROM v_run.conversation_id::text
              OR job.request ->> 'oracle_run_id' IS DISTINCT FROM v_run.id::text
              OR job.request_hash <> manifest.request_hash
              OR job.status NOT IN ('succeeded', 'failed', 'cancelled')
          )
    ) THEN
        RAISE EXCEPTION 'owner-chat oracle run is incomplete or inconsistent'
            USING ERRCODE = 'P4091';
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM public.apocrypha_owner_chat_oracle_job AS manifest
        WHERE manifest.run_id = v_run.id AND manifest.job_role = 'seed'
    ) THEN
        RAISE EXCEPTION 'owner-chat oracle seed manifest is missing'
            USING ERRCODE = '55000';
    END IF;

    UPDATE public.apocrypha_owner_chat_oracle_run AS oracle_run
    SET status = 'cleaned', cleaned_at = now()
    WHERE oracle_run.id = v_run.id;

    RETURN QUERY SELECT v_run.id, true;
END;
$$;

CREATE OR REPLACE FUNCTION public.apocrypha_owner_chat_conversation_visible(
    p_tenant_id uuid,
    p_owner_principal_id uuid,
    p_conversation_id uuid
)
RETURNS boolean
LANGUAGE sql
STABLE
SECURITY DEFINER
SET search_path = pg_catalog, public, extensions
AS $$
    SELECT p_tenant_id IS NOT NULL
       AND p_owner_principal_id IS NOT NULL
       AND p_conversation_id IS NOT NULL
       AND NOT EXISTS (
           SELECT 1
           FROM public.apocrypha_owner_chat_oracle_run AS oracle_run
           WHERE oracle_run.tenant_id = p_tenant_id
             AND oracle_run.owner_principal_id = p_owner_principal_id
             AND oracle_run.conversation_id = p_conversation_id
             AND oracle_run.status = 'cleaned'
       );
$$;

-- Rebind the complete owner-chat list projection so a cleaned synthetic run
-- disappears from the signed-in sidebar without deleting its evidence rows.
CREATE OR REPLACE FUNCTION public.apocrypha_list_owner_chat_conversations(
    p_tenant_id uuid,
    p_owner_principal_id uuid,
    p_limit integer DEFAULT 257
)
RETURNS TABLE (
    conversation_id uuid,
    title text,
    last_active_iso timestamptz,
    message_count bigint
)
LANGUAGE plpgsql
SECURITY INVOKER
SET search_path = pg_catalog, public, extensions
AS $$
BEGIN
    IF p_tenant_id IS NULL
       OR p_owner_principal_id IS NULL
       OR p_limit < 1
       OR p_limit > 257 THEN
        RAISE EXCEPTION 'owner chat list input is invalid'
            USING ERRCODE = '22023';
    END IF;

    IF NOT EXISTS (
        SELECT 1
        FROM public.apocrypha_principal AS principal
        WHERE principal.tenant_id = p_tenant_id
          AND principal.id = p_owner_principal_id
          AND principal.principal_kind = 'owner'
          AND principal.status = 'active'
    ) THEN
        RAISE EXCEPTION 'active owner identity is required'
            USING ERRCODE = 'P4031';
    END IF;

    RETURN QUERY
    WITH scoped AS (
        SELECT
            CASE
                WHEN (job.request ->> 'conversation_id') ~* '^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$'
                    THEN (job.request ->> 'conversation_id')::uuid
                ELSE job.id
            END AS conversation_id,
            NULLIF(btrim(job.request ->> 'prompt'), '') AS prompt,
            job.status,
            job.terminal_revision_id,
            job.created_at,
            job.updated_at,
            job.id
        FROM public.apocrypha_job AS job
        WHERE job.tenant_id = p_tenant_id
          AND job.owner_principal_id = p_owner_principal_id
          AND job.kind = 'apocky_chat'
          AND job.capability = 'apocky_owner_chat'
          AND (
              (job.request ->> 'conversation_id') !~* '^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$'
              OR public.apocrypha_owner_chat_conversation_visible(
                  job.tenant_id,
                  job.owner_principal_id,
                  (job.request ->> 'conversation_id')::uuid
              )
          )
    ), grouped AS (
        SELECT
            scoped.conversation_id,
            COALESCE(
                (array_agg(
                    left(regexp_replace(scoped.prompt, '\s+', ' ', 'g'), 80)
                    ORDER BY scoped.created_at ASC, scoped.id ASC
                ) FILTER (WHERE scoped.prompt IS NOT NULL))[1],
                'New conversation'
            )::text AS title,
            max(scoped.updated_at) AS last_active_iso,
            sum(
                (CASE WHEN scoped.prompt IS NOT NULL THEN 1 ELSE 0 END)
                + (CASE WHEN scoped.status = 'succeeded' AND scoped.terminal_revision_id IS NOT NULL THEN 1 ELSE 0 END)
            )::bigint AS message_count
        FROM scoped
        GROUP BY scoped.conversation_id
    )
    SELECT grouped.conversation_id, grouped.title, grouped.last_active_iso, grouped.message_count
    FROM grouped
    ORDER BY grouped.last_active_iso DESC, grouped.conversation_id ASC
    LIMIT p_limit;
END;
$$;

ALTER TABLE public.apocrypha_owner_chat_oracle_run ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.apocrypha_owner_chat_oracle_job ENABLE ROW LEVEL SECURITY;

REVOKE ALL ON TABLE
    public.apocrypha_owner_chat_oracle_run,
    public.apocrypha_owner_chat_oracle_job
FROM PUBLIC, anon, authenticated, service_role;

REVOKE EXECUTE ON FUNCTION public.apocrypha_guard_owner_chat_oracle_run()
    FROM PUBLIC, anon, authenticated, service_role;
REVOKE EXECUTE ON FUNCTION public.apocrypha_register_owner_chat_oracle_retry()
    FROM PUBLIC, anon, authenticated, service_role;
REVOKE EXECUTE ON FUNCTION public.apocrypha_seed_owner_chat_oracle(uuid, uuid, uuid)
    FROM PUBLIC, anon, authenticated;
REVOKE EXECUTE ON FUNCTION public.apocrypha_register_owner_chat_oracle_job(uuid, uuid, uuid, uuid)
    FROM PUBLIC, anon, authenticated;
REVOKE EXECUTE ON FUNCTION public.apocrypha_cleanup_owner_chat_oracle(uuid, uuid, uuid)
    FROM PUBLIC, anon, authenticated;
REVOKE EXECUTE ON FUNCTION public.apocrypha_owner_chat_conversation_visible(uuid, uuid, uuid)
    FROM PUBLIC, anon, authenticated;

GRANT EXECUTE ON FUNCTION public.apocrypha_seed_owner_chat_oracle(uuid, uuid, uuid)
    TO service_role;
GRANT EXECUTE ON FUNCTION public.apocrypha_register_owner_chat_oracle_job(uuid, uuid, uuid, uuid)
    TO service_role;
GRANT EXECUTE ON FUNCTION public.apocrypha_cleanup_owner_chat_oracle(uuid, uuid, uuid)
    TO service_role;
GRANT EXECUTE ON FUNCTION public.apocrypha_owner_chat_conversation_visible(uuid, uuid, uuid)
    TO service_role;

DO $verification$
DECLARE
    function_signature text;
BEGIN
    IF to_regclass('public.apocrypha_owner_chat_oracle_run') IS NULL
       OR to_regclass('public.apocrypha_owner_chat_oracle_job') IS NULL THEN
        RAISE EXCEPTION 'owner-chat oracle manifest tables are missing';
    END IF;
    IF has_table_privilege('service_role', 'public.apocrypha_owner_chat_oracle_run', 'SELECT')
       OR has_table_privilege('service_role', 'public.apocrypha_owner_chat_oracle_run', 'INSERT')
       OR has_table_privilege('service_role', 'public.apocrypha_owner_chat_oracle_run', 'UPDATE')
       OR has_table_privilege('service_role', 'public.apocrypha_owner_chat_oracle_job', 'SELECT')
       OR has_table_privilege('service_role', 'public.apocrypha_owner_chat_oracle_job', 'INSERT') THEN
        RAISE EXCEPTION 'service role must use scoped oracle RPCs, not manifest tables';
    END IF;

    FOREACH function_signature IN ARRAY ARRAY[
        'public.apocrypha_seed_owner_chat_oracle(uuid,uuid,uuid)',
        'public.apocrypha_register_owner_chat_oracle_job(uuid,uuid,uuid,uuid)',
        'public.apocrypha_cleanup_owner_chat_oracle(uuid,uuid,uuid)',
        'public.apocrypha_owner_chat_conversation_visible(uuid,uuid,uuid)'
    ] LOOP
        IF to_regprocedure(function_signature) IS NULL THEN
            RAISE EXCEPTION 'owner-chat oracle function missing: %', function_signature;
        END IF;
        IF has_function_privilege('anon', function_signature, 'EXECUTE')
           OR has_function_privilege('authenticated', function_signature, 'EXECUTE')
           OR NOT has_function_privilege('service_role', function_signature, 'EXECUTE') THEN
            RAISE EXCEPTION 'owner-chat oracle function privilege mismatch: %', function_signature;
        END IF;
    END LOOP;
END;
$verification$;
