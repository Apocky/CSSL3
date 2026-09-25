-- 0061 · the living room speaks through the jobs queue.
--
-- Owner steering 2026-09-24: "make room messages pass through the jobs queue as intended".
-- Until now a human row in apocrypha_room_events was answered by a loop on the owner's PC that
-- read the river directly; the jobs control plane (claim / chunks / complete, provenance, usage,
-- telemetry) never saw a room turn, and the Vercel flagship runner could not answer one.
--
--   say      apocrypha_room_say / apocrypha_room_say_guest write the human row AND enqueue its job in
--            ONE transaction: a message either has a job or was never posted.
--   answer   a trigger on apocrypha_job_revision posts the answer into the river, whoever completed
--            the job (PC worker on the local lane, Vercel runner on the flagship lane), carrying
--            engine_lane / model / elapsed_s / total_cost_usd from the revision's usage.
--   failure  a trigger on apocrypha_job posts a quiet system row when a room job ends failed or
--            cancelled, so a turn never just goes silent.
--   live     apocrypha_room_live(room) returns the in-flight turns with their streamed text so far.
--   lanes    enforced at CLAIM time, not by whoever polls fastest: a job whose metadata says
--            engine_lane = 'flagship' is claimable only by a node whose model_profiles names
--            'flagship'; a node whose metadata says lane = 'flagship' (the Vercel runner) claims
--            nothing else. Jobs without a lane are 'local', which is every job that existed before.
--
-- Everything here is service_role only; the site calls it after resolving the speaker.

CREATE TABLE IF NOT EXISTS public.apocrypha_room_turn (
    job_id      uuid        PRIMARY KEY REFERENCES public.apocrypha_job(id) ON DELETE CASCADE,
    room        text        NOT NULL,
    event_id    bigint      NOT NULL REFERENCES public.apocrypha_room_events(id) ON DELETE CASCADE,
    author      text        NOT NULL,
    engine_lane text        NOT NULL DEFAULT 'local',
    created_at  timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT apocrypha_room_turn_room_shape CHECK (room IN ('lobby', 'owner')),
    CONSTRAINT apocrypha_room_turn_lane_shape CHECK (engine_lane IN ('local', 'flagship'))
);
CREATE INDEX IF NOT EXISTS apocrypha_room_turn_room_recent ON public.apocrypha_room_turn (room, created_at DESC);
ALTER TABLE public.apocrypha_room_turn ENABLE ROW LEVEL SECURITY;

-- One answer (or one failure notice) per job, however many times it is completed or retried.
CREATE UNIQUE INDEX IF NOT EXISTS apocrypha_room_events_one_answer_per_job
    ON public.apocrypha_room_events ((meta->>'job_id'))
    WHERE author = 'apocrypha' AND kind IN ('utterance', 'system') AND meta ? 'job_id';

-- ─── say: owner and members ─────────────────────────────────────────────────
CREATE OR REPLACE FUNCTION public.apocrypha_room_say(
    p_room text,
    p_author text,
    p_body text,
    p_tenant_id uuid,
    p_principal_id uuid,
    p_capability text,
    p_request jsonb,
    p_engine_lane text,
    p_flagship_allowed boolean,
    p_model_alias text,
    p_profile_hash text,
    p_tool_registry_version text,
    p_memory_manifest_hash text
)
RETURNS TABLE (event_id bigint, created_at timestamptz, job_id uuid, job_status text, engine_lane text)
LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog, public, extensions AS $$
DECLARE
    v_lane    text := coalesce(p_engine_lane, 'local');
    v_event   public.apocrypha_room_events;
    v_request jsonb;
    v_job     public.apocrypha_job;
    v_recent  integer;
BEGIN
    IF p_room IS NULL OR p_room NOT IN ('lobby', 'owner') THEN
        RAISE EXCEPTION 'room must be lobby or owner' USING ERRCODE = '22023';
    END IF;
    IF v_lane NOT IN ('local', 'flagship') THEN
        RAISE EXCEPTION 'engine lane is invalid' USING ERRCODE = '22023';
    END IF;
    IF v_lane = 'flagship' AND NOT coalesce(p_flagship_allowed, false) THEN
        RAISE EXCEPTION 'the flagship lane needs an active Apocrypha Premium plan' USING ERRCODE = 'P4020';
    END IF;
    IF p_capability IS NULL OR p_capability NOT IN ('apocky_owner_chat', 'apocky_member_chat') THEN
        RAISE EXCEPTION 'room turns run on the owner or member chat capability' USING ERRCODE = '22023';
    END IF;
    IF p_room = 'owner' AND p_capability <> 'apocky_owner_chat' THEN
        RAISE EXCEPTION 'that room is private' USING ERRCODE = 'P4031';
    END IF;
    IF p_author IS NULL OR char_length(btrim(p_author)) = 0 OR char_length(p_author) > 80 THEN
        RAISE EXCEPTION 'author is required' USING ERRCODE = '22023';
    END IF;
    IF p_body IS NULL OR char_length(btrim(p_body)) = 0 OR char_length(p_body) > 4000 THEN
        RAISE EXCEPTION 'message must be 1-4000 characters' USING ERRCODE = '22023';
    END IF;

    IF p_capability = 'apocky_member_chat' THEN
        PERFORM pg_advisory_xact_lock(hashtextextended('apocrypha-room-principal:' || p_principal_id::text, 0));
        IF EXISTS (
            SELECT 1 FROM public.apocrypha_room_turn AS t
            JOIN public.apocrypha_job AS j ON j.id = t.job_id
            WHERE j.tenant_id = p_tenant_id AND j.owner_principal_id = p_principal_id
              AND j.status IN ('queued', 'leased', 'running', 'cancel_requested')
        ) THEN
            RAISE EXCEPTION 'Apocrypha is still answering your last message' USING ERRCODE = 'P4091';
        END IF;
        SELECT count(*) INTO v_recent FROM public.apocrypha_room_turn AS t
        JOIN public.apocrypha_job AS j ON j.id = t.job_id
        WHERE j.tenant_id = p_tenant_id AND j.owner_principal_id = p_principal_id
          AND t.created_at >= now() - interval '1 hour';
        IF v_recent >= 60 THEN
            RAISE EXCEPTION 'room quota: 60 messages an hour' USING ERRCODE = 'P4290';
        END IF;
    END IF;

    INSERT INTO public.apocrypha_room_events (room, author, kind, body, meta)
    VALUES (p_room, p_author, 'utterance', p_body, jsonb_build_object('engine_lane', v_lane))
    RETURNING * INTO v_event;

    v_request := coalesce(p_request, '{}'::jsonb)
        || jsonb_build_object('room', p_room, 'room_event_id', v_event.id, 'engine_lane', v_lane);

    SELECT * INTO v_job FROM public.apocrypha_enqueue_job(
        p_tenant_id, p_principal_id, 'apocky_chat', p_capability,
        v_request, public.apocrypha_sha256(v_request::text),
        'room:' || p_room, 'event:' || v_event.id::text,
        p_model_alias, p_profile_hash, p_tool_registry_version, p_memory_manifest_hash,
        (CASE WHEN p_capability = 'apocky_owner_chat' THEN 20 ELSE 0 END)::smallint,
        3::smallint, now(), NULL, 'primary');

    UPDATE public.apocrypha_job AS j
    SET metadata = j.metadata || jsonb_build_object('engine_lane', v_lane, 'room', p_room, 'room_event_id', v_event.id)
    WHERE j.id = v_job.id;

    INSERT INTO public.apocrypha_room_turn (job_id, room, event_id, author, engine_lane)
    VALUES (v_job.id, p_room, v_event.id, p_author, v_lane);

    RETURN QUERY SELECT v_event.id, v_event.created_at, v_job.id, v_job.status, v_lane;
END;
$$;

-- ─── say: guests (lobby only, local lane, the guest queue's own quota and busy rules) ────────
CREATE OR REPLACE FUNCTION public.apocrypha_room_say_guest(
    p_author text,
    p_body text,
    p_subject_hash text,
    p_request_id uuid,
    p_history jsonb,
    p_model_alias text,
    p_profile_hash text,
    p_tool_registry_version text,
    p_memory_manifest_hash text
)
RETURNS TABLE (event_id bigint, created_at timestamptz, job_id uuid, job_status text, engine_lane text)
LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog, public, extensions AS $$
DECLARE
    v_event  public.apocrypha_room_events;
    v_job_id uuid;
    v_status text;
BEGIN
    IF p_author IS NULL OR p_author !~ '^guest:[0-9a-f]{16}$' THEN
        RAISE EXCEPTION 'guest author is invalid' USING ERRCODE = '22023';
    END IF;
    IF p_body IS NULL OR char_length(btrim(p_body)) = 0 OR char_length(p_body) > 4000 THEN
        RAISE EXCEPTION 'message must be 1-4000 characters' USING ERRCODE = '22023';
    END IF;

    INSERT INTO public.apocrypha_room_events (room, author, kind, body, meta)
    VALUES ('lobby', p_author, 'utterance', p_body, jsonb_build_object('engine_lane', 'local'))
    RETURNING * INTO v_event;

    SELECT g.job_id, g.status INTO v_job_id, v_status
    FROM public.apocrypha_enqueue_guest_chat_v1(
        p_subject_hash, p_request_id, p_body, coalesce(p_history, '[]'::jsonb),
        p_model_alias, p_profile_hash, p_tool_registry_version, p_memory_manifest_hash) AS g;
    IF v_job_id IS NULL THEN
        RAISE EXCEPTION 'the guest queue returned no job' USING ERRCODE = '55000';
    END IF;

    UPDATE public.apocrypha_job AS j
    SET metadata = j.metadata || jsonb_build_object('engine_lane', 'local', 'room', 'lobby', 'room_event_id', v_event.id)
    WHERE j.id = v_job_id;

    INSERT INTO public.apocrypha_room_turn (job_id, room, event_id, author, engine_lane)
    VALUES (v_job_id, 'lobby', v_event.id, p_author, 'local');

    RETURN QUERY SELECT v_event.id, v_event.created_at, v_job_id, v_status, 'local'::text;
END;
$$;

-- ─── live: the in-flight turns of a room, with the text streamed so far ─────
CREATE OR REPLACE FUNCTION public.apocrypha_room_live(p_room text)
RETURNS TABLE (
    job_id uuid, event_id bigint, author text, engine_lane text, status text,
    text text, chunks integer, created_at timestamptz, updated_at timestamptz
)
LANGUAGE sql STABLE SECURITY DEFINER SET search_path = pg_catalog, public AS $$
    SELECT t.job_id, t.event_id, t.author, t.engine_lane, j.status,
           coalesce((
               SELECT string_agg(c.delta, '' ORDER BY c.seq)
               FROM public.apocrypha_job_chunk AS c
               WHERE c.job_id = j.id AND c.attempt_id = j.current_attempt_id AND c.chunk_kind = 'token'
           ), '') AS text,
           (SELECT count(*)::integer FROM public.apocrypha_job_chunk AS c
            WHERE c.job_id = j.id AND c.attempt_id = j.current_attempt_id) AS chunks,
           t.created_at, j.updated_at
    FROM public.apocrypha_room_turn AS t
    JOIN public.apocrypha_job AS j ON j.id = t.job_id
    WHERE t.room = p_room
      AND t.created_at > now() - interval '30 minutes'
      AND j.status IN ('queued', 'leased', 'running', 'cancel_requested')
    ORDER BY t.created_at
    LIMIT 20;
$$;

-- ─── answer + failure triggers ──────────────────────────────────────────────
-- A failure here must never fail the job completion that fired it: the room is a view of the
-- queue, and the queue's own commit is the thing that matters.
CREATE OR REPLACE FUNCTION public.apocrypha_room_answer_from_revision()
RETURNS trigger LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog, public AS $$
DECLARE
    v_turn public.apocrypha_room_turn;
BEGIN
    IF NEW.revision_role IS DISTINCT FROM 'primary' THEN RETURN NEW; END IF;
    SELECT * INTO v_turn FROM public.apocrypha_room_turn WHERE job_id = NEW.job_id;
    IF NOT FOUND THEN RETURN NEW; END IF;
    BEGIN
        INSERT INTO public.apocrypha_room_events (room, author, kind, body, meta)
        VALUES (v_turn.room, 'apocrypha', 'utterance', NEW.content, jsonb_strip_nulls(jsonb_build_object(
            'job_id', NEW.job_id::text,
            'reply_to', v_turn.event_id,
            'engine_lane', coalesce(NEW.usage->>'engine_lane', NEW.provenance->>'engine_lane', v_turn.engine_lane),
            'model', coalesce(NEW.usage->>'model', NEW.provenance->>'model_alias', NEW.model_alias),
            'elapsed_s', NEW.usage->'elapsed_s',
            'total_cost_usd', NEW.usage->'total_cost_usd',
            'withheld', NEW.provenance->'withheld',
            'lane_fallback', NEW.provenance->'lane_fallback',
            -- counts only: the lobby is public, so what was recalled stays out of the row
            'recall_records', (
                SELECT sum(coalesce((s->>'records')::integer, 0))
                FROM jsonb_array_elements(CASE WHEN jsonb_typeof(NEW.provenance->'memory_sources') = 'array'
                                               THEN NEW.provenance->'memory_sources' ELSE '[]'::jsonb END) AS s
            )
        )))
        ON CONFLICT ((meta->>'job_id')) WHERE author = 'apocrypha' AND kind IN ('utterance', 'system') AND meta ? 'job_id'
        DO NOTHING;
    EXCEPTION WHEN OTHERS THEN
        RAISE WARNING 'apocrypha room answer for job % not posted: %', NEW.job_id, SQLERRM;
    END;
    RETURN NEW;
END;
$$;

DROP TRIGGER IF EXISTS apocrypha_room_answer ON public.apocrypha_job_revision;
CREATE TRIGGER apocrypha_room_answer
    AFTER INSERT ON public.apocrypha_job_revision
    FOR EACH ROW EXECUTE FUNCTION public.apocrypha_room_answer_from_revision();

CREATE OR REPLACE FUNCTION public.apocrypha_room_answer_from_failure()
RETURNS trigger LANGUAGE plpgsql SECURITY DEFINER SET search_path = pg_catalog, public AS $$
DECLARE
    v_turn public.apocrypha_room_turn;
BEGIN
    SELECT * INTO v_turn FROM public.apocrypha_room_turn WHERE job_id = NEW.id;
    IF NOT FOUND THEN RETURN NEW; END IF;
    BEGIN
        INSERT INTO public.apocrypha_room_events (room, author, kind, body, meta)
        VALUES (v_turn.room, 'apocrypha', 'system',
            CASE WHEN NEW.status = 'cancelled'
                 THEN 'Apocrypha stopped answering that message.'
                 ELSE 'Apocrypha could not answer that message (' || coalesce(NEW.error_code, 'error') || '). Say it again to retry.'
            END,
            jsonb_strip_nulls(jsonb_build_object(
                'job_id', NEW.id::text, 'reply_to', v_turn.event_id,
                'error_code', NEW.error_code, 'engine_lane', v_turn.engine_lane)))
        ON CONFLICT ((meta->>'job_id')) WHERE author = 'apocrypha' AND kind IN ('utterance', 'system') AND meta ? 'job_id'
        DO NOTHING;
    EXCEPTION WHEN OTHERS THEN
        RAISE WARNING 'apocrypha room failure notice for job % not posted: %', NEW.id, SQLERRM;
    END;
    RETURN NEW;
END;
$$;

DROP TRIGGER IF EXISTS apocrypha_room_failure ON public.apocrypha_job;
CREATE TRIGGER apocrypha_room_failure
    AFTER UPDATE OF status ON public.apocrypha_job
    FOR EACH ROW
    WHEN (NEW.status IN ('failed', 'cancelled') AND OLD.status IS DISTINCT FROM NEW.status)
    EXECUTE FUNCTION public.apocrypha_room_answer_from_failure();

-- ─── lanes at claim time ────────────────────────────────────────────────────
-- The live apocrypha_claim_job (as of 2026-09-25) with two added clauses: the lane filter on the
-- candidate, and per-lane ordering in its FIFO check.

CREATE OR REPLACE FUNCTION public.apocrypha_claim_job(p_node_id uuid, p_node_token text, p_claim_key text, p_lease_seconds integer DEFAULT 180)
 RETURNS TABLE(job_id uuid, attempt_id uuid, attempt_no integer, lease_epoch bigint, lease_token text, lease_expires_at timestamp with time zone, tenant_id uuid, owner_principal_id uuid, kind text, capability text, request jsonb, model_alias text, profile_hash text, tool_registry_version text, memory_manifest_hash text)
 LANGUAGE plpgsql
 SECURITY DEFINER
 SET search_path TO 'pg_catalog', 'public', 'extensions'
AS $function$
DECLARE
    v_node          public.apocrypha_worker_node;
    v_job           public.apocrypha_job;
    v_attempt       public.apocrypha_job_attempt;
    v_lease_token   text;
    v_lease_seconds integer;
    v_active_count  integer;
BEGIN
    v_node := public.apocrypha_require_worker(p_node_id, p_node_token, true);
    v_lease_seconds := least(900, greatest(30, coalesce(p_lease_seconds, 180)));

    IF char_length(btrim(coalesce(p_claim_key, ''))) NOT BETWEEN 8 AND 200 THEN
        RAISE EXCEPTION 'claim idempotency key must contain 8-200 characters'
            USING ERRCODE = '23514';
    END IF;

    -- Replay an ambiguously acknowledged claim before enforcing concurrency.
    -- The lease token is encrypted with the already-validated high-entropy node
    -- token; plaintext is never stored. Reusing the key can never lease a new
    -- job while the original attempt record exists.
    SELECT a.* INTO v_attempt
    FROM public.apocrypha_job_attempt AS a
    WHERE a.worker_node_id = p_node_id
      AND a.claim_key = btrim(p_claim_key);

    IF FOUND THEN
        SELECT * INTO v_job
        FROM public.apocrypha_job AS j
        WHERE j.id = v_attempt.job_id;

        IF NOT FOUND THEN
            RAISE EXCEPTION 'claim idempotency record lost its job'
                USING ERRCODE = '55000';
        END IF;

        BEGIN
            v_lease_token := pgp_sym_decrypt(v_attempt.lease_token_ciphertext, p_node_token);
        EXCEPTION WHEN OTHERS THEN
            RAISE EXCEPTION 'claim replay integrity check failed' USING ERRCODE = '28000';
        END;

        UPDATE public.apocrypha_worker_node SET last_seen_at = now() WHERE id = p_node_id;

        RETURN QUERY SELECT
            v_job.id,
            v_attempt.id,
            v_attempt.attempt_no,
            v_attempt.lease_epoch,
            v_lease_token,
            v_attempt.lease_expires_at,
            v_job.tenant_id,
            v_job.owner_principal_id,
            v_job.kind,
            v_job.capability,
            v_job.request,
            v_job.model_alias,
            v_job.profile_hash,
            v_job.tool_registry_version,
            v_job.memory_manifest_hash;
        RETURN;
    END IF;

    SELECT count(*) INTO v_active_count
    FROM public.apocrypha_job_attempt AS a
    WHERE a.worker_node_id = p_node_id
      AND a.status IN ('leased', 'running')
      AND a.lease_expires_at > now();

    UPDATE public.apocrypha_worker_node
    SET last_seen_at = now()
    WHERE id = p_node_id;

    IF v_active_count >= v_node.max_concurrency THEN
        RETURN;
    END IF;

    SELECT j.* INTO v_job
    FROM public.apocrypha_job AS j
    WHERE j.status = 'queued'
      AND j.available_at <= now()
      AND j.attempt_count < j.max_attempts
      AND (v_node.tenant_id IS NULL OR v_node.tenant_id = j.tenant_id)
      AND (
          '*' = ANY(v_node.allowed_capabilities)
          OR j.capability = ANY(v_node.allowed_capabilities)
      )
      -- 0061: lanes. A flagship job goes only to a node that serves the flagship; a flagship-only
      -- node (the Vercel runner) takes nothing else. No lane in the metadata means 'local'.
      AND (
          CASE coalesce(j.metadata->>'engine_lane', 'local')
              WHEN 'flagship' THEN coalesce(v_node.model_profiles ? 'flagship', false)
              ELSE coalesce(v_node.metadata->>'lane', 'local') IS DISTINCT FROM 'flagship'
          END
      )
      AND NOT EXISTS (
          SELECT 1
          FROM public.apocrypha_job AS earlier
          WHERE earlier.owner_principal_id = j.owner_principal_id
            AND earlier.status = 'queued'
            -- 0061: order is kept per principal PER LANE, so a turn waiting on one lane's server
            -- (the PC asleep, the gateway down) never holds up the other lane.
            AND coalesce(earlier.metadata->>'engine_lane', 'local') = coalesce(j.metadata->>'engine_lane', 'local')
            AND earlier.available_at <= now()
            AND earlier.attempt_count < earlier.max_attempts
            AND (
                earlier.priority > j.priority
                OR (
                    earlier.priority = j.priority
                    AND (earlier.created_at, earlier.id) < (j.created_at, j.id)
                )
            )
      )
    ORDER BY j.priority DESC, j.available_at, j.created_at, j.id
    FOR UPDATE OF j SKIP LOCKED
    LIMIT 1;

    IF NOT FOUND THEN
        RETURN;
    END IF;

    v_lease_token := 'apl_' || encode(gen_random_bytes(32), 'hex');

    UPDATE public.apocrypha_job AS j
    SET status = 'leased',
        lease_epoch = j.lease_epoch + 1,
        attempt_count = j.attempt_count + 1,
        error_code = NULL,
        error_detail = NULL
    WHERE j.id = v_job.id
    RETURNING * INTO v_job;

    INSERT INTO public.apocrypha_job_attempt (
        job_id, worker_node_id, claim_key, attempt_no, lease_epoch,
        lease_token_hash, lease_token_ciphertext, status, leased_at,
        lease_expires_at, last_heartbeat_at
    ) VALUES (
        v_job.id,
        p_node_id,
        btrim(p_claim_key),
        v_job.attempt_count,
        v_job.lease_epoch,
        public.apocrypha_sha256(v_lease_token),
        pgp_sym_encrypt(
            v_lease_token,
            p_node_token,
            'cipher-algo=aes256,compress-algo=0'
        ),
        'leased',
        now(),
        now() + make_interval(secs => v_lease_seconds),
        now()
    ) RETURNING * INTO v_attempt;

    UPDATE public.apocrypha_job
    SET current_attempt_id = v_attempt.id
    WHERE id = v_job.id
    RETURNING * INTO v_job;

    PERFORM public.apocrypha_record_job_event(
        v_job.id, v_attempt.id, 'job.claimed', 'expected_fired', 'info',
        'control_plane.claim', false,
        jsonb_build_object(
            'worker_node_id', p_node_id,
            'attempt_no', v_attempt.attempt_no,
            'lease_epoch', v_attempt.lease_epoch,
            'lease_expires_at', v_attempt.lease_expires_at
        )
    );

    RETURN QUERY SELECT
        v_job.id,
        v_attempt.id,
        v_attempt.attempt_no,
        v_attempt.lease_epoch,
        v_lease_token,
        v_attempt.lease_expires_at,
        v_job.tenant_id,
        v_job.owner_principal_id,
        v_job.kind,
        v_job.capability,
        v_job.request,
        v_job.model_alias,
        v_job.profile_hash,
        v_job.tool_registry_version,
        v_job.memory_manifest_hash;
END;
$function$;

-- ─── grants: service_role only ──────────────────────────────────────────────
-- anon is named explicitly: Supabase grants it EXECUTE on new public functions by default, and a
-- REVOKE that leaves it out leaves these callable with the public key (see 0060).
REVOKE ALL ON FUNCTION public.apocrypha_room_say(text, text, text, uuid, uuid, text, jsonb, text, boolean, text, text, text, text) FROM PUBLIC, anon, authenticated;
REVOKE ALL ON FUNCTION public.apocrypha_room_say_guest(text, text, text, uuid, jsonb, text, text, text, text) FROM PUBLIC, anon, authenticated;
REVOKE ALL ON FUNCTION public.apocrypha_room_live(text) FROM PUBLIC, anon, authenticated;
REVOKE ALL ON FUNCTION public.apocrypha_room_answer_from_revision() FROM PUBLIC, anon, authenticated;
REVOKE ALL ON FUNCTION public.apocrypha_room_answer_from_failure() FROM PUBLIC, anon, authenticated;
REVOKE ALL ON FUNCTION public.apocrypha_claim_job(uuid, text, text, integer) FROM PUBLIC, anon, authenticated;
GRANT EXECUTE ON FUNCTION public.apocrypha_room_say(text, text, text, uuid, uuid, text, jsonb, text, boolean, text, text, text, text) TO service_role;
GRANT EXECUTE ON FUNCTION public.apocrypha_room_say_guest(text, text, text, uuid, jsonb, text, text, text, text) TO service_role;
GRANT EXECUTE ON FUNCTION public.apocrypha_room_live(text) TO service_role;
GRANT EXECUTE ON FUNCTION public.apocrypha_claim_job(uuid, text, text, integer) TO service_role;
