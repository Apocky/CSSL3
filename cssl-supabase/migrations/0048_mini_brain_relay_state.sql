-- =====================================================================
-- § APOCRYPHA MINI BRAIN · durable serverless relay admission state
-- =====================================================================
-- Purpose:
--   - preserve device sequence and logical-request idempotency across cold starts
--   - admit a re-signed retry at a newer sequence without duplicating its effect
--   - enforce one owner-wide rate window atomically across all instances
--   - retain only opaque identifiers and SHA-256 digests (never prompts)
--   - expose mutation only through one service-role SECURITY DEFINER RPC
--
-- Identity split:
--   envelope_digest = signed request fields, including sequence + issued_at
--   logical_digest  = operation + session + base cursor + payload digest
-- A sequence may bind one envelope only. A request_id may advance through newly
-- signed envelopes only while its logical digest remains identical.
--
-- Retention:
--   device/request/sequence replay evidence = 35 days. Device capabilities live
--   for 30 days, so an unexpired capability cannot outlive its replay boundary.
--   Rate rows expire after one day. Admission prunes its owner opportunistically;
--   cleanup_mini_brain_relay_state() provides a bounded global maintenance pass.
-- =====================================================================

CREATE TABLE IF NOT EXISTS public.mini_brain_relay_device_state (
    owner_ref              text        NOT NULL,
    device_id              uuid        NOT NULL,
    key_thumbprint         text        NOT NULL,
    latest_sequence        bigint      NOT NULL,
    latest_request_id      uuid        NOT NULL,
    latest_envelope_digest text        NOT NULL,
    updated_at             timestamptz NOT NULL DEFAULT clock_timestamp(),
    expires_at             timestamptz NOT NULL,
    PRIMARY KEY (owner_ref, device_id),
    CONSTRAINT mini_brain_device_owner_ref_shape
        CHECK (owner_ref ~ '^[0-9a-f]{64}$'),
    CONSTRAINT mini_brain_device_thumbprint_shape
        CHECK (key_thumbprint ~ '^[0-9a-f]{64}$'),
    CONSTRAINT mini_brain_device_envelope_digest_shape
        CHECK (latest_envelope_digest ~ '^[0-9a-f]{64}$'),
    CONSTRAINT mini_brain_device_sequence_range
        CHECK (latest_sequence BETWEEN 1 AND 9007199254740991),
    CONSTRAINT mini_brain_device_expiry_order
        CHECK (expires_at > updated_at)
);

CREATE INDEX IF NOT EXISTS mini_brain_relay_device_expiry_idx
    ON public.mini_brain_relay_device_state (expires_at);

CREATE TABLE IF NOT EXISTS public.mini_brain_relay_request_ledger (
    owner_ref             text        NOT NULL,
    request_id            uuid        NOT NULL,
    device_id             uuid        NOT NULL,
    logical_digest        text        NOT NULL,
    first_sequence        bigint      NOT NULL,
    latest_sequence       bigint      NOT NULL,
    retry_count           bigint      NOT NULL DEFAULT 0,
    first_admitted_at     timestamptz NOT NULL DEFAULT clock_timestamp(),
    last_admitted_at      timestamptz NOT NULL DEFAULT clock_timestamp(),
    expires_at            timestamptz NOT NULL,
    PRIMARY KEY (owner_ref, request_id),
    CONSTRAINT mini_brain_request_device_fk
        FOREIGN KEY (owner_ref, device_id)
        REFERENCES public.mini_brain_relay_device_state(owner_ref, device_id)
        ON DELETE CASCADE,
    CONSTRAINT mini_brain_request_owner_ref_shape
        CHECK (owner_ref ~ '^[0-9a-f]{64}$'),
    CONSTRAINT mini_brain_request_logical_digest_shape
        CHECK (logical_digest ~ '^[0-9a-f]{64}$'),
    CONSTRAINT mini_brain_request_sequence_range
        CHECK (
            first_sequence BETWEEN 1 AND 9007199254740991
            AND latest_sequence BETWEEN first_sequence AND 9007199254740991
        ),
    CONSTRAINT mini_brain_request_retry_count_range
        CHECK (retry_count >= 0),
    CONSTRAINT mini_brain_request_time_order
        CHECK (last_admitted_at >= first_admitted_at AND expires_at > last_admitted_at)
);

CREATE INDEX IF NOT EXISTS mini_brain_relay_request_expiry_idx
    ON public.mini_brain_relay_request_ledger (expires_at);

CREATE TABLE IF NOT EXISTS public.mini_brain_relay_sequence_ledger (
    owner_ref      text        NOT NULL,
    device_id      uuid        NOT NULL,
    sequence       bigint      NOT NULL,
    request_id     uuid        NOT NULL,
    logical_digest text        NOT NULL,
    envelope_digest text       NOT NULL,
    admitted_at    timestamptz NOT NULL DEFAULT clock_timestamp(),
    expires_at     timestamptz NOT NULL,
    PRIMARY KEY (owner_ref, device_id, sequence),
    CONSTRAINT mini_brain_sequence_device_fk
        FOREIGN KEY (owner_ref, device_id)
        REFERENCES public.mini_brain_relay_device_state(owner_ref, device_id)
        ON DELETE CASCADE,
    CONSTRAINT mini_brain_sequence_request_fk
        FOREIGN KEY (owner_ref, request_id)
        REFERENCES public.mini_brain_relay_request_ledger(owner_ref, request_id)
        ON DELETE CASCADE,
    CONSTRAINT mini_brain_sequence_owner_ref_shape
        CHECK (owner_ref ~ '^[0-9a-f]{64}$'),
    CONSTRAINT mini_brain_sequence_logical_digest_shape
        CHECK (logical_digest ~ '^[0-9a-f]{64}$'),
    CONSTRAINT mini_brain_sequence_envelope_digest_shape
        CHECK (envelope_digest ~ '^[0-9a-f]{64}$'),
    CONSTRAINT mini_brain_sequence_range
        CHECK (sequence BETWEEN 1 AND 9007199254740991),
    CONSTRAINT mini_brain_sequence_expiry_order
        CHECK (expires_at > admitted_at)
);

CREATE INDEX IF NOT EXISTS mini_brain_relay_sequence_expiry_idx
    ON public.mini_brain_relay_sequence_ledger (expires_at);

CREATE TABLE IF NOT EXISTS public.mini_brain_relay_rate_state (
    owner_ref         text        PRIMARY KEY,
    window_started_at timestamptz NOT NULL,
    request_count     integer     NOT NULL,
    updated_at        timestamptz NOT NULL,
    expires_at        timestamptz NOT NULL,
    CONSTRAINT mini_brain_rate_owner_ref_shape
        CHECK (owner_ref ~ '^[0-9a-f]{64}$'),
    CONSTRAINT mini_brain_rate_count_range
        CHECK (request_count BETWEEN 0 AND 30),
    CONSTRAINT mini_brain_rate_expiry_order
        CHECK (expires_at > updated_at)
);

CREATE INDEX IF NOT EXISTS mini_brain_relay_rate_expiry_idx
    ON public.mini_brain_relay_rate_state (expires_at);

ALTER TABLE public.mini_brain_relay_device_state ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.mini_brain_relay_request_ledger ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.mini_brain_relay_sequence_ledger ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.mini_brain_relay_rate_state ENABLE ROW LEVEL SECURITY;

-- No table policy is created. Browser roles are denied by RLS, and even the
-- service role is denied direct grants so application code cannot split the
-- atomic decision into a read-then-write race.
REVOKE ALL ON TABLE public.mini_brain_relay_device_state
    FROM PUBLIC, anon, authenticated, service_role;
REVOKE ALL ON TABLE public.mini_brain_relay_request_ledger
    FROM PUBLIC, anon, authenticated, service_role;
REVOKE ALL ON TABLE public.mini_brain_relay_sequence_ledger
    FROM PUBLIC, anon, authenticated, service_role;
REVOKE ALL ON TABLE public.mini_brain_relay_rate_state
    FROM PUBLIC, anon, authenticated, service_role;

CREATE OR REPLACE FUNCTION public.admit_mini_brain_relay_request(
    p_owner_ref text,
    p_device_id uuid,
    p_key_thumbprint text,
    p_sequence bigint,
    p_request_id uuid,
    p_logical_digest text,
    p_envelope_digest text
)
RETURNS TABLE (
    outcome text,
    accepted_sequence bigint,
    rate_count integer,
    rate_limit integer,
    rate_resets_at timestamptz,
    state_expires_at timestamptz
)
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog, public
AS $$
DECLARE
    v_now              timestamptz := clock_timestamp();
    v_state_expiry     timestamptz := v_now + interval '35 days';
    v_device_state     public.mini_brain_relay_device_state%ROWTYPE;
    v_request_state    public.mini_brain_relay_request_ledger%ROWTYPE;
    v_sequence_state   public.mini_brain_relay_sequence_ledger%ROWTYPE;
    v_rate_state       public.mini_brain_relay_rate_state%ROWTYPE;
    v_outcome          text := 'new_sequence';
    v_exact_envelope   boolean := false;
    v_rate_count       integer;
    v_rate_reset       timestamptz;
    v_result_expiry    timestamptz;
BEGIN
    IF p_owner_ref IS NULL OR p_owner_ref !~ '^[0-9a-f]{64}$' THEN
        RAISE EXCEPTION USING ERRCODE = 'P0001', MESSAGE = 'BRAIN_RELAY_STATE_INPUT_INVALID';
    END IF;
    IF p_device_id IS NULL OR p_request_id IS NULL THEN
        RAISE EXCEPTION USING ERRCODE = 'P0001', MESSAGE = 'BRAIN_RELAY_STATE_INPUT_INVALID';
    END IF;
    IF p_key_thumbprint IS NULL OR p_key_thumbprint !~ '^[0-9a-f]{64}$' THEN
        RAISE EXCEPTION USING ERRCODE = 'P0001', MESSAGE = 'BRAIN_RELAY_STATE_INPUT_INVALID';
    END IF;
    IF p_logical_digest IS NULL OR p_logical_digest !~ '^[0-9a-f]{64}$' THEN
        RAISE EXCEPTION USING ERRCODE = 'P0001', MESSAGE = 'BRAIN_RELAY_STATE_INPUT_INVALID';
    END IF;
    IF p_envelope_digest IS NULL OR p_envelope_digest !~ '^[0-9a-f]{64}$' THEN
        RAISE EXCEPTION USING ERRCODE = 'P0001', MESSAGE = 'BRAIN_RELAY_STATE_INPUT_INVALID';
    END IF;
    IF p_sequence IS NULL OR p_sequence < 1 OR p_sequence > 9007199254740991 THEN
        RAISE EXCEPTION USING ERRCODE = 'P0001', MESSAGE = 'BRAIN_RELAY_STATE_INPUT_INVALID';
    END IF;

    -- One owner lock supplies a total order across devices, instances, and
    -- cold starts. Hash collisions only serialize unrelated owners; they do
    -- not merge state or relax authorization.
    PERFORM pg_advisory_xact_lock(
        hashtextextended('apocky.mini-brain.relay.v1:' || p_owner_ref, 0)
    );

    -- Bounded, owner-local opportunistic cleanup keeps the hot path cheap.
    -- Parent rows are removed only after their bounded child ledger is empty,
    -- avoiding a large ON DELETE cascade inside an interactive admission.
    DELETE FROM public.mini_brain_relay_sequence_ledger AS target
     USING (
        SELECT ctid
          FROM public.mini_brain_relay_sequence_ledger
         WHERE owner_ref = p_owner_ref AND expires_at <= v_now
         ORDER BY expires_at
         LIMIT 256
     ) AS doomed
     WHERE target.ctid = doomed.ctid;
    DELETE FROM public.mini_brain_relay_request_ledger AS target
     USING (
        SELECT req.ctid
          FROM public.mini_brain_relay_request_ledger AS req
         WHERE req.owner_ref = p_owner_ref
           AND req.expires_at <= v_now
           AND NOT EXISTS (
                SELECT 1
                  FROM public.mini_brain_relay_sequence_ledger AS seq
                 WHERE seq.owner_ref = req.owner_ref
                   AND seq.request_id = req.request_id
           )
         ORDER BY req.expires_at
         LIMIT 256
     ) AS doomed
     WHERE target.ctid = doomed.ctid;
    DELETE FROM public.mini_brain_relay_device_state AS target
     USING (
        SELECT dev.ctid
          FROM public.mini_brain_relay_device_state AS dev
         WHERE dev.owner_ref = p_owner_ref
           AND dev.expires_at <= v_now
           AND NOT EXISTS (
                SELECT 1
                  FROM public.mini_brain_relay_request_ledger AS req
                 WHERE req.owner_ref = dev.owner_ref
                   AND req.device_id = dev.device_id
           )
         ORDER BY dev.expires_at
         LIMIT 64
     ) AS doomed
     WHERE target.ctid = doomed.ctid;
    DELETE FROM public.mini_brain_relay_rate_state
     WHERE owner_ref = p_owner_ref AND expires_at <= v_now;

    SELECT *
      INTO v_device_state
      FROM public.mini_brain_relay_device_state
     WHERE owner_ref = p_owner_ref
       AND device_id = p_device_id
     FOR UPDATE;

    IF FOUND AND v_device_state.key_thumbprint <> p_key_thumbprint THEN
        RAISE EXCEPTION USING ERRCODE = 'P0001', MESSAGE = 'BRAIN_DEVICE_STATE_BINDING_MISMATCH';
    END IF;

    -- Exact same-sequence retries bind to the exact signed envelope. A newly
    -- signed retry must use a newer sequence and is matched later by its stable
    -- logical digest.
    SELECT *
      INTO v_sequence_state
      FROM public.mini_brain_relay_sequence_ledger
     WHERE owner_ref = p_owner_ref
       AND device_id = p_device_id
       AND sequence = p_sequence
     FOR UPDATE;

    IF FOUND THEN
        IF v_sequence_state.request_id <> p_request_id
           OR v_sequence_state.logical_digest <> p_logical_digest
           OR v_sequence_state.envelope_digest <> p_envelope_digest THEN
            RAISE EXCEPTION USING ERRCODE = 'P0001', MESSAGE = 'BRAIN_SYNC_REPLAY_REJECTED';
        END IF;
        v_outcome := 'identical_retry';
        v_exact_envelope := true;
        v_result_expiry := v_sequence_state.expires_at;
    ELSE
        IF v_device_state.owner_ref IS NOT NULL
           AND p_sequence <= v_device_state.latest_sequence THEN
            RAISE EXCEPTION USING ERRCODE = 'P0001', MESSAGE = 'BRAIN_SYNC_REPLAY_REJECTED';
        END IF;

        SELECT *
          INTO v_request_state
          FROM public.mini_brain_relay_request_ledger
         WHERE owner_ref = p_owner_ref
           AND request_id = p_request_id
         FOR UPDATE;

        IF FOUND THEN
            IF v_request_state.device_id <> p_device_id
               OR v_request_state.logical_digest <> p_logical_digest THEN
                RAISE EXCEPTION USING ERRCODE = 'P0001', MESSAGE = 'BRAIN_SYNC_REQUEST_ID_REUSED';
            END IF;
            v_outcome := 'identical_retry';
        END IF;
        v_result_expiry := v_state_expiry;
    END IF;

    -- Every verified envelope, including an exact retry, is charged against
    -- the owner-wide window. Idempotency prevents duplicate effect; it does not
    -- create an unmetered replay channel.
    SELECT *
      INTO v_rate_state
      FROM public.mini_brain_relay_rate_state
     WHERE owner_ref = p_owner_ref
     FOR UPDATE;

    IF NOT FOUND THEN
        v_rate_count := 1;
        v_rate_reset := v_now + interval '60 seconds';
        INSERT INTO public.mini_brain_relay_rate_state (
            owner_ref, window_started_at, request_count, updated_at, expires_at
        ) VALUES (
            p_owner_ref, v_now, v_rate_count, v_now, v_now + interval '1 day'
        );
    ELSIF v_rate_state.window_started_at + interval '60 seconds' <= v_now THEN
        v_rate_count := 1;
        v_rate_reset := v_now + interval '60 seconds';
        UPDATE public.mini_brain_relay_rate_state
           SET window_started_at = v_now,
               request_count = v_rate_count,
               updated_at = v_now,
               expires_at = v_now + interval '1 day'
         WHERE owner_ref = p_owner_ref;
    ELSE
        IF v_rate_state.request_count >= 30 THEN
            RAISE EXCEPTION USING ERRCODE = 'P0001', MESSAGE = 'BRAIN_SYNC_RATE_LIMITED';
        END IF;
        v_rate_count := v_rate_state.request_count + 1;
        v_rate_reset := v_rate_state.window_started_at + interval '60 seconds';
        UPDATE public.mini_brain_relay_rate_state
           SET request_count = v_rate_count,
               updated_at = v_now,
               expires_at = v_now + interval '1 day'
         WHERE owner_ref = p_owner_ref;
    END IF;

    IF NOT v_exact_envelope THEN
        INSERT INTO public.mini_brain_relay_device_state (
            owner_ref,
            device_id,
            key_thumbprint,
            latest_sequence,
            latest_request_id,
            latest_envelope_digest,
            updated_at,
            expires_at
        ) VALUES (
            p_owner_ref,
            p_device_id,
            p_key_thumbprint,
            p_sequence,
            p_request_id,
            p_envelope_digest,
            v_now,
            v_state_expiry
        )
        ON CONFLICT (owner_ref, device_id) DO UPDATE
           SET latest_sequence = EXCLUDED.latest_sequence,
               latest_request_id = EXCLUDED.latest_request_id,
               latest_envelope_digest = EXCLUDED.latest_envelope_digest,
               updated_at = EXCLUDED.updated_at,
               expires_at = EXCLUDED.expires_at;

        IF v_outcome = 'new_sequence' THEN
            INSERT INTO public.mini_brain_relay_request_ledger (
                owner_ref,
                request_id,
                device_id,
                logical_digest,
                first_sequence,
                latest_sequence,
                retry_count,
                first_admitted_at,
                last_admitted_at,
                expires_at
            ) VALUES (
                p_owner_ref,
                p_request_id,
                p_device_id,
                p_logical_digest,
                p_sequence,
                p_sequence,
                0,
                v_now,
                v_now,
                v_state_expiry
            );
        ELSE
            UPDATE public.mini_brain_relay_request_ledger
               SET latest_sequence = p_sequence,
                   retry_count = retry_count + 1,
                   last_admitted_at = v_now,
                   expires_at = v_state_expiry
             WHERE owner_ref = p_owner_ref
               AND request_id = p_request_id;
        END IF;

        INSERT INTO public.mini_brain_relay_sequence_ledger (
            owner_ref,
            device_id,
            sequence,
            request_id,
            logical_digest,
            envelope_digest,
            admitted_at,
            expires_at
        ) VALUES (
            p_owner_ref,
            p_device_id,
            p_sequence,
            p_request_id,
            p_logical_digest,
            p_envelope_digest,
            v_now,
            v_state_expiry
        );
    END IF;

    RETURN QUERY SELECT
        v_outcome,
        p_sequence,
        v_rate_count,
        30,
        v_rate_reset,
        v_result_expiry;
END;
$$;

CREATE OR REPLACE FUNCTION public.cleanup_mini_brain_relay_state(
    p_limit integer DEFAULT 5000
)
RETURNS TABLE (
    sequence_rows_deleted bigint,
    request_rows_deleted bigint,
    device_rows_deleted bigint,
    rate_rows_deleted bigint
)
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = pg_catalog, public
AS $$
DECLARE
    v_now       timestamptz := clock_timestamp();
    v_sequences bigint := 0;
    v_requests  bigint := 0;
    v_devices   bigint := 0;
    v_rates     bigint := 0;
BEGIN
    IF p_limit IS NULL OR p_limit < 1 OR p_limit > 50000 THEN
        RAISE EXCEPTION USING ERRCODE = 'P0001', MESSAGE = 'BRAIN_RELAY_CLEANUP_LIMIT_INVALID';
    END IF;

    WITH doomed AS (
        SELECT ctid
          FROM public.mini_brain_relay_sequence_ledger
         WHERE expires_at <= v_now
         ORDER BY expires_at
         LIMIT p_limit
    ), removed AS (
        DELETE FROM public.mini_brain_relay_sequence_ledger AS target
         USING doomed
         WHERE target.ctid = doomed.ctid
         RETURNING 1
    )
    SELECT count(*) INTO v_sequences FROM removed;

    WITH doomed AS (
        SELECT req.ctid
          FROM public.mini_brain_relay_request_ledger AS req
         WHERE req.expires_at <= v_now
           AND NOT EXISTS (
                SELECT 1
                  FROM public.mini_brain_relay_sequence_ledger AS seq
                 WHERE seq.owner_ref = req.owner_ref
                   AND seq.request_id = req.request_id
           )
         ORDER BY req.expires_at
         LIMIT p_limit
    ), removed AS (
        DELETE FROM public.mini_brain_relay_request_ledger AS target
         USING doomed
         WHERE target.ctid = doomed.ctid
         RETURNING 1
    )
    SELECT count(*) INTO v_requests FROM removed;

    WITH doomed AS (
        SELECT dev.ctid
          FROM public.mini_brain_relay_device_state AS dev
         WHERE dev.expires_at <= v_now
           AND NOT EXISTS (
                SELECT 1
                  FROM public.mini_brain_relay_request_ledger AS req
                 WHERE req.owner_ref = dev.owner_ref
                   AND req.device_id = dev.device_id
           )
         ORDER BY dev.expires_at
         LIMIT p_limit
    ), removed AS (
        DELETE FROM public.mini_brain_relay_device_state AS target
         USING doomed
         WHERE target.ctid = doomed.ctid
         RETURNING 1
    )
    SELECT count(*) INTO v_devices FROM removed;

    WITH doomed AS (
        SELECT ctid
          FROM public.mini_brain_relay_rate_state
         WHERE expires_at <= v_now
         ORDER BY expires_at
         LIMIT p_limit
    ), removed AS (
        DELETE FROM public.mini_brain_relay_rate_state AS target
         USING doomed
         WHERE target.ctid = doomed.ctid
         RETURNING 1
    )
    SELECT count(*) INTO v_rates FROM removed;

    RETURN QUERY SELECT v_sequences, v_requests, v_devices, v_rates;
END;
$$;

REVOKE ALL ON FUNCTION public.admit_mini_brain_relay_request(text,uuid,text,bigint,uuid,text,text)
    FROM PUBLIC, anon, authenticated;
GRANT EXECUTE ON FUNCTION public.admit_mini_brain_relay_request(text,uuid,text,bigint,uuid,text,text)
    TO service_role;

REVOKE ALL ON FUNCTION public.cleanup_mini_brain_relay_state(integer)
    FROM PUBLIC, anon, authenticated;
GRANT EXECUTE ON FUNCTION public.cleanup_mini_brain_relay_state(integer)
    TO service_role;

COMMENT ON TABLE public.mini_brain_relay_request_ledger IS
    'Bounded logical-request idempotency ledger: opaque request identity and digest only; never prompt content.';
COMMENT ON TABLE public.mini_brain_relay_sequence_ledger IS
    'Bounded signed-envelope replay ledger: per-device sequence and digest only; never prompt content.';
COMMENT ON FUNCTION public.admit_mini_brain_relay_request(text,uuid,text,bigint,uuid,text,text) IS
    'Atomically enforces owner-wide rate, per-device sequence, and re-signed logical-request idempotency.';
COMMENT ON FUNCTION public.cleanup_mini_brain_relay_state(integer) IS
    'Deletes bounded batches of expired Mini Brain relay metadata; safe for scheduled or manual service-role use.';
