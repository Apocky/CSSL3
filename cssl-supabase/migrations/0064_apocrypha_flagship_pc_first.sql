-- 0064 · flagship turns go to the PC worker first (it carries each account's memory); the Vercel
-- runner only takes one that has waited 30 s unclaimed. Live apocrypha_claim_job with that one change.

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
                  -- 0064: the Vercel runner (a flagship-only node, no memory) is the fallback: it takes
                  -- a flagship turn only after 30 s unclaimed, so the PC worker answers first, with memory.
                  AND (coalesce(v_node.metadata->>'lane', 'local') IS DISTINCT FROM 'flagship'
                       OR j.created_at < now() - interval '30 seconds')
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
REVOKE ALL ON FUNCTION public.apocrypha_claim_job(uuid, text, text, integer) FROM PUBLIC, anon, authenticated;
GRANT EXECUTE ON FUNCTION public.apocrypha_claim_job(uuid, text, text, integer) TO service_role;
