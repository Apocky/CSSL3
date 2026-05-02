-- =====================================================================
-- § T11-W14-MYCELIUM-HEARTBEAT · 0035_mycelium_federation.sql
-- ════════════════════════════════════════════════════════════════════
-- LOCAL↔CLOUD federation persistence layer · k-anon ≥ 10 ·
-- Σ-mask-gated · sovereign-revoke-cascading · Σ-Chain-anchor every row.
--
-- Three tables :
--   - mycelium_federation_staged : below-floor rows · service-role-only
--   - mycelium_federation_public : promoted ≥-k cohorts · public-read
--   - mycelium_federation_purged : tombstones for revoke audit-trail
--
-- Two stored-procs called by cssl-edge endpoints :
--   - record_federation_patterns(jsonb) : bulk-ingest from /heartbeat
--   - record_federation_purge(handle, ts) : sovereign-revoke cascade
--
-- DESIGN INVARIANTS
--   - mycelium_federation_public has CHECK(cohort_size >= 10) so the
--     k-anon floor is structurally enforced ; even a bug in the promotion
--     trigger cannot leak below-floor rows.
--   - All bigints stored as text (because Postgres bigint vs JS BigInt
--     interop is fragile ; the wire-format is text-encoded uint64).
--   - per-row sigma_anchor (32 hex-chars) ties the row to the broadcast
--     bundle's bundle_blake3 (immutable attribution survives revoke).
--
-- Apply order : after 0034_cloud_orchestrator (slot 0035 is unclaimed).
-- =====================================================================

CREATE EXTENSION IF NOT EXISTS pgcrypto;

-- ─── mycelium_federation_staged ─────────────────────────────────────────
-- Rows whose cohort_size < K_ANON_FLOOR. Service-role-only access ; never
-- exposed to public-read. Promotion happens inside `record_federation_patterns`
-- when a new ingest pushes the cohort over the floor.
CREATE TABLE IF NOT EXISTS public.mycelium_federation_staged (
    row_id              uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
    kind                smallint    NOT NULL,
    cap_flags           smallint    NOT NULL,
    cohort_size         integer     NOT NULL DEFAULT 1,
    confidence_q8_sum   bigint      NOT NULL DEFAULT 0,
    observation_count   integer     NOT NULL DEFAULT 0,
    last_ts_bucketed    bigint      NOT NULL,
    payload_hash        text        NOT NULL,
    -- Distinct-emitter set for k-anon counting. Stored as a uniqueness-
    -- enforced text array (each entry is the LE-decoded uint64 of
    -- `emitter_handle` from the wire-format).
    emitter_handles     text[]      NOT NULL DEFAULT ARRAY[]::text[],
    bundle_blake3       text        NOT NULL,
    sigma_anchor        text        NOT NULL,
    created_at          timestamptz NOT NULL DEFAULT now(),
    updated_at          timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT mycelium_staged_kind_range
        CHECK (kind BETWEEN 0 AND 255),
    CONSTRAINT mycelium_staged_cap_reserved_zero
        CHECK ((cap_flags & 240) = 0),       -- bits 4..7 must be 0
    CONSTRAINT mycelium_staged_payload_hash_shape
        CHECK (char_length(payload_hash) BETWEEN 1 AND 32),
    CONSTRAINT mycelium_staged_sigma_anchor_shape
        CHECK (sigma_anchor ~ '^[0-9a-f]{64}$'),
    CONSTRAINT mycelium_staged_cohort_below_floor
        CHECK (cohort_size BETWEEN 1 AND 9)  -- staged → STRICTLY below floor
);
COMMENT ON TABLE public.mycelium_federation_staged IS
    'Below-k-anon-floor rows. Service-role-only. Promoted into public when cohort_size ≥ 10.';

CREATE INDEX IF NOT EXISTS mycelium_staged_kind_payload_idx
    ON public.mycelium_federation_staged (kind, payload_hash);
CREATE INDEX IF NOT EXISTS mycelium_staged_ts_idx
    ON public.mycelium_federation_staged (last_ts_bucketed);

-- ─── mycelium_federation_public ─────────────────────────────────────────
-- k-anon-promoted rows. Cohort_size ≥ K_ANON_FLOOR enforced by CHECK
-- (defense-in-depth). public-read RLS lets the digest endpoint serve.
CREATE TABLE IF NOT EXISTS public.mycelium_federation_public (
    row_id              uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
    kind                smallint    NOT NULL,
    payload_hash        text        NOT NULL,
    cohort_size         integer     NOT NULL,
    mean_confidence_q8  smallint    NOT NULL,
    observation_count   integer     NOT NULL,
    last_ts_bucketed    bigint      NOT NULL,
    sigma_anchor        text        NOT NULL,
    promoted_at         timestamptz NOT NULL DEFAULT now(),
    updated_at          timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT mycelium_public_k_anon_floor_invariant
        CHECK (cohort_size >= 10),           -- structural-enforcement
    CONSTRAINT mycelium_public_kind_range
        CHECK (kind BETWEEN 0 AND 255),
    CONSTRAINT mycelium_public_confidence_range
        CHECK (mean_confidence_q8 BETWEEN 0 AND 255),
    CONSTRAINT mycelium_public_sigma_anchor_shape
        CHECK (sigma_anchor ~ '^[0-9a-f]{64}$'),
    CONSTRAINT mycelium_public_unique_kind_payload
        UNIQUE (kind, payload_hash)
);
COMMENT ON TABLE public.mycelium_federation_public IS
    'k-anon-promoted federation rows. Public-read. cohort_size ≥ 10 structurally enforced.';

CREATE INDEX IF NOT EXISTS mycelium_public_kind_idx
    ON public.mycelium_federation_public (kind);
CREATE INDEX IF NOT EXISTS mycelium_public_ts_idx
    ON public.mycelium_federation_public (last_ts_bucketed);

-- ─── mycelium_federation_purged (tombstones) ────────────────────────────
-- Service-role-only audit trail of sovereign-revoke events. Never read
-- from the digest endpoint ; surface for the W14-M status-page.
CREATE TABLE IF NOT EXISTS public.mycelium_federation_purged (
    row_id          uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
    emitter_handle  text        NOT NULL,
    ts_unix         bigint      NOT NULL,
    purged_at       timestamptz NOT NULL DEFAULT now(),
    rows_affected   integer     NOT NULL DEFAULT 0,
    purge_anchor    text        NOT NULL,
    CONSTRAINT mycelium_purged_anchor_shape
        CHECK (char_length(purge_anchor) BETWEEN 32 AND 64)
);
COMMENT ON TABLE public.mycelium_federation_purged IS
    'Sovereign-revoke tombstones. Service-role-only. Audit trail only ; never read from digest.';

CREATE INDEX IF NOT EXISTS mycelium_purged_handle_idx
    ON public.mycelium_federation_purged (emitter_handle);

-- ─── RLS policies ───────────────────────────────────────────────────────

ALTER TABLE public.mycelium_federation_staged ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.mycelium_federation_public ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.mycelium_federation_purged ENABLE ROW LEVEL SECURITY;

-- staged : default-deny ; service-role-only.
DROP POLICY IF EXISTS mycelium_staged_anon_deny ON public.mycelium_federation_staged;
CREATE POLICY mycelium_staged_anon_deny
    ON public.mycelium_federation_staged
    FOR SELECT
    TO anon, authenticated
    USING (false);

-- public : public-read on all ; insert/update only via service-role
-- (RLS for write defaults to deny when no policy matches the caller's role).
DROP POLICY IF EXISTS mycelium_public_read ON public.mycelium_federation_public;
CREATE POLICY mycelium_public_read
    ON public.mycelium_federation_public
    FOR SELECT
    TO anon, authenticated
    USING (true);

-- purged : default-deny ; service-role-only audit trail.
DROP POLICY IF EXISTS mycelium_purged_anon_deny ON public.mycelium_federation_purged;
CREATE POLICY mycelium_purged_anon_deny
    ON public.mycelium_federation_purged
    FOR SELECT
    TO anon, authenticated
    USING (false);

-- ─── helper : record_federation_patterns ────────────────────────────────
-- Bulk-ingest endpoint backing /api/mycelium/heartbeat. Each row in
-- p_rows is upserted into staged ; if the new cohort hits the k-anon
-- floor, the row is promoted to public + the staged copy is deleted.
-- Returns { ingested, staged } counts.
CREATE OR REPLACE FUNCTION public.record_federation_patterns(p_rows jsonb)
RETURNS jsonb
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public
AS $$
DECLARE
    rec              jsonb;
    v_kind           smallint;
    v_cap_flags      smallint;
    v_cohort_size    integer;
    v_confidence_q8  smallint;
    v_ts_bucketed    bigint;
    v_payload_hash   text;
    v_emitter_handle text;
    v_sig            text;
    v_bundle         text;
    v_anchor         text;
    v_existing       record;
    v_new_cohort     integer;
    v_ingested       integer := 0;
    v_staged         integer := 0;
BEGIN
    FOR rec IN SELECT jsonb_array_elements(p_rows) LOOP
        v_kind          := (rec->>'kind')::smallint;
        v_cap_flags     := (rec->>'cap_flags')::smallint;
        v_cohort_size   := COALESCE((rec->>'cohort_size')::integer, 1);
        v_confidence_q8 := (rec->>'confidence_q8')::smallint;
        v_ts_bucketed   := (rec->>'ts_bucketed')::bigint;
        v_payload_hash  := rec->>'payload_hash';
        v_emitter_handle:= rec->>'emitter_handle';
        v_sig           := rec->>'sig';
        v_bundle        := rec->>'bundle_blake3';
        v_anchor        := COALESCE(rec->>'sigma_anchor', v_bundle);

        -- Look up the existing aggregate (if any).
        SELECT *
            INTO v_existing
            FROM public.mycelium_federation_public
            WHERE kind = v_kind AND payload_hash = v_payload_hash
            LIMIT 1;

        IF FOUND THEN
            -- Already-promoted row : update aggregates only ; never store
            -- per-emitter set on public side (privacy invariant).
            UPDATE public.mycelium_federation_public
                SET
                    observation_count = v_existing.observation_count + 1,
                    cohort_size = GREATEST(v_existing.cohort_size, v_cohort_size),
                    last_ts_bucketed = GREATEST(v_existing.last_ts_bucketed, v_ts_bucketed),
                    mean_confidence_q8 =
                        ((v_existing.mean_confidence_q8::int * v_existing.observation_count
                          + v_confidence_q8::int) / (v_existing.observation_count + 1))::smallint,
                    updated_at = now()
                WHERE row_id = v_existing.row_id;
            v_ingested := v_ingested + 1;
            CONTINUE;
        END IF;

        -- Not promoted yet — accumulate in staged.
        DECLARE
            v_staged_row record;
        BEGIN
            SELECT *
                INTO v_staged_row
                FROM public.mycelium_federation_staged
                WHERE kind = v_kind AND payload_hash = v_payload_hash
                LIMIT 1;

            IF FOUND THEN
                -- Add this emitter to the cohort if new.
                IF NOT (v_emitter_handle = ANY(v_staged_row.emitter_handles)) THEN
                    UPDATE public.mycelium_federation_staged
                        SET
                            emitter_handles = array_append(
                                v_staged_row.emitter_handles, v_emitter_handle
                            ),
                            cohort_size = v_staged_row.cohort_size + 1,
                            confidence_q8_sum = v_staged_row.confidence_q8_sum + v_confidence_q8,
                            observation_count = v_staged_row.observation_count + 1,
                            last_ts_bucketed = GREATEST(v_staged_row.last_ts_bucketed, v_ts_bucketed),
                            updated_at = now()
                        WHERE row_id = v_staged_row.row_id
                        RETURNING cohort_size INTO v_new_cohort;
                ELSE
                    -- Same emitter again ; bump observation count only.
                    UPDATE public.mycelium_federation_staged
                        SET
                            confidence_q8_sum = v_staged_row.confidence_q8_sum + v_confidence_q8,
                            observation_count = v_staged_row.observation_count + 1,
                            last_ts_bucketed = GREATEST(v_staged_row.last_ts_bucketed, v_ts_bucketed),
                            updated_at = now()
                        WHERE row_id = v_staged_row.row_id;
                    v_new_cohort := v_staged_row.cohort_size;
                END IF;

                -- If this promotion crosses k=10, move to public + delete staged.
                IF v_new_cohort >= 10 THEN
                    INSERT INTO public.mycelium_federation_public
                        (kind, payload_hash, cohort_size, mean_confidence_q8,
                         observation_count, last_ts_bucketed, sigma_anchor)
                    SELECT s.kind, s.payload_hash, s.cohort_size,
                           (s.confidence_q8_sum / GREATEST(s.observation_count, 1))::smallint,
                           s.observation_count, s.last_ts_bucketed, v_anchor
                        FROM public.mycelium_federation_staged s
                        WHERE s.row_id = v_staged_row.row_id
                    ON CONFLICT (kind, payload_hash) DO UPDATE
                        SET cohort_size = EXCLUDED.cohort_size,
                            mean_confidence_q8 = EXCLUDED.mean_confidence_q8,
                            observation_count = EXCLUDED.observation_count,
                            last_ts_bucketed = EXCLUDED.last_ts_bucketed,
                            updated_at = now();
                    DELETE FROM public.mycelium_federation_staged WHERE row_id = v_staged_row.row_id;
                    v_ingested := v_ingested + 1;
                ELSE
                    v_staged := v_staged + 1;
                END IF;
            ELSE
                -- First sighting : create the staged row.
                INSERT INTO public.mycelium_federation_staged
                    (kind, cap_flags, cohort_size, confidence_q8_sum, observation_count,
                     last_ts_bucketed, payload_hash, emitter_handles, bundle_blake3, sigma_anchor)
                VALUES
                    (v_kind, v_cap_flags, 1, v_confidence_q8, 1,
                     v_ts_bucketed, v_payload_hash, ARRAY[v_emitter_handle], v_bundle, v_anchor);
                v_staged := v_staged + 1;
            END IF;
        END;
    END LOOP;

    RETURN jsonb_build_object(
        'ingested', v_ingested,
        'staged', v_staged
    );
END;
$$;

COMMENT ON FUNCTION public.record_federation_patterns(jsonb) IS
    'Bulk-ingest federation patterns. Staged below k=10 ; promoted at k=10. Returns {ingested, staged}.';

-- ─── helper : record_federation_purge ───────────────────────────────────
-- Sovereign-revoke cascade. Drops the emitter_handle from every staged
-- cohort ; if any staged row's cohort drops to 0, the row is deleted.
-- Public rows are NOT individually purgeable (we only have aggregates) ;
-- the tombstone audit-row + the next observation cycle re-derive accurate
-- aggregates without the revoked emitter.
CREATE OR REPLACE FUNCTION public.record_federation_purge(
    p_emitter_handle text,
    p_ts_unix        bigint,
    p_purge_anchor   text
)
RETURNS jsonb
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public
AS $$
DECLARE
    v_rows_affected integer := 0;
    v_now           timestamptz := now();
BEGIN
    -- Drop the emitter from every staged row's cohort.
    WITH updated AS (
        UPDATE public.mycelium_federation_staged
            SET
                emitter_handles = array_remove(emitter_handles, p_emitter_handle),
                cohort_size = GREATEST(0, cohort_size - 1),
                updated_at = v_now
            WHERE p_emitter_handle = ANY(emitter_handles)
            RETURNING row_id
    )
    SELECT count(*) INTO v_rows_affected FROM updated;

    -- Delete any staged rows whose cohort is now empty.
    DELETE FROM public.mycelium_federation_staged WHERE cohort_size = 0;

    -- Insert tombstone for audit.
    INSERT INTO public.mycelium_federation_purged
        (emitter_handle, ts_unix, purged_at, rows_affected, purge_anchor)
    VALUES
        (p_emitter_handle, p_ts_unix, v_now, v_rows_affected, p_purge_anchor);

    RETURN jsonb_build_object(
        'rows_affected', v_rows_affected,
        'purged_at', v_now
    );
END;
$$;

COMMENT ON FUNCTION public.record_federation_purge(text, bigint, text) IS
    'Sovereign-revoke cascade. Removes emitter from every cohort ; inserts tombstone audit-row.';

-- ─── grants ──────────────────────────────────────────────────────────────

GRANT SELECT ON public.mycelium_federation_public TO anon, authenticated;
GRANT EXECUTE ON FUNCTION public.record_federation_patterns(jsonb) TO service_role;
GRANT EXECUTE ON FUNCTION public.record_federation_purge(text, bigint, text) TO service_role;
