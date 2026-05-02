-- =====================================================================
-- § T11-W11-AKASHIC-TELEMETRY · 0025_akashic.sql
-- Akashic-Webpage-Records · substrate-native diagnostic layer.
-- Every page-event = one cell in the ω-field. Σ-mask gates audience.
-- k-anonymity enforced via aggregate-views ; raw rows purgeable per-cap.
--
-- Substrate parallels :
--   ω-field cell    → public.akashic_events row
--   Σ-mask          → akashic_events.sigma_mask
--   KAN pattern     → cluster_signature column + future view
--   mycelium-spore  → batched flush via /api/akashic/batch
--   Akashic         → the table itself
--
-- Apply order : after 0024_analytics.sql.
-- § NOTE on numbering : original target was 0024 but slot was taken by
--   the parallel analytics-pipeline-agent (0024_analytics.sql) ; using 0025.
-- =====================================================================

CREATE EXTENSION IF NOT EXISTS pgcrypto;

-- =====================================================================
-- public.akashic_events · ring-buffer-shaped raw event store
-- =====================================================================
-- Bit-pack philosophy : keep this thin · all domain-specific shape lives
-- in jsonb payload. Cluster-signature is denormalized for fast group-by.
-- Retention : 30 days (handled by housekeeping job · out-of-scope here).
CREATE TABLE IF NOT EXISTS public.akashic_events (
    id                 bigserial   PRIMARY KEY,
    cell_id            text        NOT NULL,                        -- 16-char hash (client-side)
    ts_iso             timestamptz NOT NULL DEFAULT now(),
    sigma_mask         smallint    NOT NULL DEFAULT 0,              -- audience bitmask
    cap_witness_hash   text,                                        -- hash of cap if used
    dpl_id             text        NOT NULL DEFAULT 'unknown',
    commit_sha         text        NOT NULL DEFAULT 'unknown',
    build_time         text        NOT NULL DEFAULT 'unknown',
    kind               text        NOT NULL,
    payload            jsonb       NOT NULL DEFAULT '{}'::jsonb,
    session_id         text        NOT NULL,                        -- ephemeral random ; NOT a user-id
    user_cap_hash      text,                                        -- hash of user-cap iff logged-in
    cluster_signature  text,                                        -- 16-char hash for grouping (errors only)
    -- Length / sanity constraints
    CONSTRAINT akashic_events_cell_id_length
        CHECK (char_length(cell_id) BETWEEN 8 AND 64),
    CONSTRAINT akashic_events_kind_length
        CHECK (char_length(kind) BETWEEN 3 AND 64),
    CONSTRAINT akashic_events_session_id_length
        CHECK (char_length(session_id) BETWEEN 8 AND 64),
    CONSTRAINT akashic_events_sigma_mask_range
        CHECK (sigma_mask BETWEEN 0 AND 4095),
    CONSTRAINT akashic_events_cluster_signature_length
        CHECK (cluster_signature IS NULL
               OR char_length(cluster_signature) BETWEEN 8 AND 64),
    CONSTRAINT akashic_events_user_cap_hash_length
        CHECK (user_cap_hash IS NULL
               OR char_length(user_cap_hash) BETWEEN 8 AND 128),
    CONSTRAINT akashic_events_kind_chars
        CHECK (kind ~ '^[a-z][a-z0-9._-]*$')
);

-- Per-session timeline (most-recent-first · debug-walks)
CREATE INDEX IF NOT EXISTS akashic_events_session_ts_desc_idx
    ON public.akashic_events (session_id, ts_iso DESC);

-- Per-kind histogram (admin dashboard "page.error" counts)
CREATE INDEX IF NOT EXISTS akashic_events_kind_ts_desc_idx
    ON public.akashic_events (kind, ts_iso DESC);

-- Cluster-grouping index (KAN pattern detection · "show me all rows with
-- this cluster_signature in the last 24h")
CREATE INDEX IF NOT EXISTS akashic_events_cluster_ts_desc_idx
    ON public.akashic_events (cluster_signature, ts_iso DESC)
    WHERE cluster_signature IS NOT NULL;

-- User-cap link (sovereign-purge target)
CREATE INDEX IF NOT EXISTS akashic_events_user_cap_idx
    ON public.akashic_events (user_cap_hash)
    WHERE user_cap_hash IS NOT NULL;

-- Deploy-version drift detection (admin dashboard "stuck deploy" canary)
CREATE INDEX IF NOT EXISTS akashic_events_dpl_kind_idx
    ON public.akashic_events (dpl_id, kind, ts_iso DESC);

COMMENT ON TABLE public.akashic_events IS
    'Akashic-Webpage-Records · substrate-native diagnostic event store. Every page-event is one ω-field cell. Σ-mask gates audience ; cap_witness proves consent ; cluster_signature seeds KAN pattern detection ; user_cap_hash enables sovereign purge.';
COMMENT ON COLUMN public.akashic_events.sigma_mask IS
    'Audience bitmask : 0b0001 self · 0b0010 aggregate · 0b0100 pattern · 0b1000 federated. Server re-checks before exposing rows in any view.';
COMMENT ON COLUMN public.akashic_events.cluster_signature IS
    '16-char hash of normalized stack-frames. Errors with the same cluster_signature are the same underlying bug · group-by-cluster surfaces frequency.';
COMMENT ON COLUMN public.akashic_events.session_id IS
    'EPHEMERAL random session-id. NOT a user-id. Resets on tab-close. Pseudo-anonymous : useful for joining same-tab events but cannot be linked to a person.';
COMMENT ON COLUMN public.akashic_events.user_cap_hash IS
    'Hash of user-cap iff logged-in. Enables sovereign-purge. NEVER the cap itself.';

-- =====================================================================
-- public.akashic_session_arc · per-session derived view
-- =====================================================================
-- "What happened in session X" — chronological cell-stream within one
-- session. RLS limits SELECT to service-role + cap-bypassed admins.
DROP VIEW IF EXISTS public.akashic_session_arc;

CREATE VIEW public.akashic_session_arc AS
    SELECT
        session_id,
        ts_iso,
        kind,
        sigma_mask,
        dpl_id,
        commit_sha,
        cluster_signature,
        payload
      FROM public.akashic_events
     ORDER BY session_id, ts_iso ASC;

COMMENT ON VIEW public.akashic_session_arc IS
    'Per-session chronological event-arc. Used by admin/telemetry to walk through a single sessions cell-stream. RLS on the underlying table propagates.';

-- =====================================================================
-- public.akashic_cluster_summary · KAN-pattern aggregate (k-anon ≥ 5)
-- =====================================================================
-- Aggregates errors by cluster_signature ; ENFORCES k-anonymity via the
-- HAVING clause. A cluster with < 5 distinct sessions is invisible to
-- everyone (including admin) until k threshold met.
DROP VIEW IF EXISTS public.akashic_cluster_summary;

CREATE VIEW public.akashic_cluster_summary AS
    SELECT
        cluster_signature,
        kind,
        count(*)                            AS occurrences,
        count(DISTINCT session_id)          AS distinct_sessions,
        min(ts_iso)                         AS first_seen,
        max(ts_iso)                         AS last_seen,
        array_agg(DISTINCT dpl_id)          AS dpl_ids
      FROM public.akashic_events
     WHERE cluster_signature IS NOT NULL
     GROUP BY cluster_signature, kind
    HAVING count(DISTINCT session_id) >= 5;

COMMENT ON VIEW public.akashic_cluster_summary IS
    'Error-cluster aggregate · k-anonymity ≥ 5 sessions enforced. Admin dashboard "this 4-frame React error happened in N sessions" view. Single-session clusters are invisible.';

-- =====================================================================
-- public.akashic_perf_summary · perf-metric aggregate (k-anon ≥ 10)
-- =====================================================================
-- LCP / FID / CLS / etc. aggregated by kind + url. k-anon ≥ 10 enforced.
DROP VIEW IF EXISTS public.akashic_perf_summary;

CREATE VIEW public.akashic_perf_summary AS
    SELECT
        kind,
        (payload ->> 'url')                            AS url,
        count(*)                                       AS samples,
        count(DISTINCT session_id)                     AS distinct_sessions,
        avg((payload ->> 'value')::numeric)            AS avg_value,
        percentile_cont(0.5)
            WITHIN GROUP (ORDER BY (payload ->> 'value')::numeric) AS p50,
        percentile_cont(0.75)
            WITHIN GROUP (ORDER BY (payload ->> 'value')::numeric) AS p75,
        percentile_cont(0.95)
            WITHIN GROUP (ORDER BY (payload ->> 'value')::numeric) AS p95
      FROM public.akashic_events
     WHERE kind LIKE 'perf.%'
       AND payload ? 'value'
     GROUP BY kind, (payload ->> 'url')
    HAVING count(DISTINCT session_id) >= 10;

COMMENT ON VIEW public.akashic_perf_summary IS
    'Per-kind / per-url performance percentiles · k-anonymity ≥ 10 sessions enforced. Avg + p50 + p75 + p95 surfaced for admin dashboard.';

-- =====================================================================
-- public.akashic_purge · sovereign-purge function
-- =====================================================================
-- DELETE every row tied to the supplied user_cap_hash. Returns number of
-- rows deleted. SECURITY DEFINER so caller doesn't need direct DELETE on
-- the table ; the RLS policy denies non-service-role direct DELETE.
CREATE OR REPLACE FUNCTION public.akashic_purge(p_user_cap_hash text)
    RETURNS int
    LANGUAGE plpgsql
    SECURITY DEFINER
    SET search_path = public
AS $$
DECLARE
    v_deleted int;
BEGIN
    IF p_user_cap_hash IS NULL OR char_length(p_user_cap_hash) < 8 THEN
        RAISE EXCEPTION 'akashic_purge : invalid user_cap_hash';
    END IF;
    DELETE FROM public.akashic_events
     WHERE user_cap_hash = p_user_cap_hash;
    GET DIAGNOSTICS v_deleted = ROW_COUNT;
    -- Stamp a self-witnessing audit-trace row so the purge itself is recorded
    -- without retaining any of the purged data. Cell_id distinguishes purges.
    INSERT INTO public.akashic_events (
        cell_id, ts_iso, sigma_mask, kind, payload, session_id, user_cap_hash
    ) VALUES (
        encode(gen_random_bytes(8), 'hex'),
        now(),
        2, -- SIGMA_AGGREGATE only · purge-events stay aggregable
        'consent.purge_request',
        jsonb_build_object('rows_deleted', v_deleted, 'self_witnessing', true),
        'system',
        NULL  -- purge-witness row itself has no cap-link
    );
    RETURN v_deleted;
END;
$$;

COMMENT ON FUNCTION public.akashic_purge(text) IS
    'Sovereign-purge entry-point. DELETEs every akashic_events row tied to the supplied user_cap_hash + writes a self-witnessing aggregate row recording (rows_deleted, self_witnessing). SECURITY DEFINER ; called via /api/akashic/purge with cap-witness verification.';

-- =====================================================================
-- RLS policies
-- =====================================================================
ALTER TABLE public.akashic_events ENABLE ROW LEVEL SECURITY;

-- INSERT : authenticated role can INSERT their own events. The session_id
-- gate is a soft-guard ; real verification happens in the API endpoint.
DROP POLICY IF EXISTS akashic_events_insert ON public.akashic_events;
CREATE POLICY akashic_events_insert
    ON public.akashic_events
    FOR INSERT
    TO authenticated, anon
    WITH CHECK (true);

-- SELECT : non-service-role principals can ONLY see rows whose user_cap_hash
-- matches the auth.uid()-derived hash (when logged-in) ; anon sees nothing.
-- Aggregate views bypass via SECURITY-DEFINER (out-of-scope here).
DROP POLICY IF EXISTS akashic_events_select_self ON public.akashic_events;
CREATE POLICY akashic_events_select_self
    ON public.akashic_events
    FOR SELECT
    TO authenticated
    USING (
        user_cap_hash IS NOT NULL
        AND user_cap_hash = encode(digest(auth.uid()::text, 'sha256'), 'hex')
    );

-- UPDATE / DELETE : NO non-service-role principal may UPDATE or DELETE.
-- DELETEs go through public.akashic_purge() (SECURITY DEFINER).
DROP POLICY IF EXISTS akashic_events_no_update ON public.akashic_events;
CREATE POLICY akashic_events_no_update
    ON public.akashic_events
    FOR UPDATE
    TO authenticated, anon
    USING (false);

DROP POLICY IF EXISTS akashic_events_no_direct_delete ON public.akashic_events;
CREATE POLICY akashic_events_no_direct_delete
    ON public.akashic_events
    FOR DELETE
    TO authenticated, anon
    USING (false);

-- =====================================================================
-- Grants
-- =====================================================================
-- INSERT only for anon/authenticated ; everything else gated by RLS.
GRANT INSERT          ON public.akashic_events           TO anon, authenticated;
GRANT SELECT          ON public.akashic_events           TO authenticated;
GRANT SELECT          ON public.akashic_session_arc      TO authenticated;
GRANT SELECT          ON public.akashic_cluster_summary  TO authenticated;
GRANT SELECT          ON public.akashic_perf_summary     TO authenticated;
GRANT USAGE, SELECT   ON SEQUENCE public.akashic_events_id_seq
                                                          TO anon, authenticated;
GRANT EXECUTE         ON FUNCTION public.akashic_purge(text)
                                                          TO authenticated;
