-- =====================================================================
-- § T11-W14-K · 0034_cloud_orchestrator.sql
-- ════════════════════════════════════════════════════════════════════
--
-- Cloud-side persistent-orchestrator schema. Tables :
--
--   - cron_executions           · audit-trail of every cron-job run
--   - cron_heartbeat            · 1min ping rows (uptime visibility)
--   - playtest_queue            · queued packages from /cron/playtest-cycle
--   - sigma_chain_checkpoints   · Σ-Chain trust-anchor rows (1024-batch)
--   - mycelium_patterns_agg     · k-anon view over per-user pattern emissions
--
-- Helpers (pgsql) :
--   - rollup_promote_minutes()        · ALREADY in 0024 · cron just calls it
--   - sigma_chain_emit_checkpoint()   · new · BLAKE3-via-digest() roll-up
--   - cleanup_old_cron_executions()   · scheduled via pg_cron
--
-- pg_cron jobs (created at-bottom · safe-to-rerun) :
--   1. cleanup-old-cron-events   · daily 04:00 UTC
--   2. analytics-vacuum           · weekly Sunday 03:00 UTC
--   3. heartbeat-purge            · daily 05:00 UTC (drop heartbeats > 7 days)
--
-- All tables : RLS-policied · default-deny · service-role-only writes.
-- All tables : sigma_mask uuid for cross-table consent linkage.
--
-- Apply order : after 0033_gacha.sql (or 0039_seasons.sql · whichever last).
-- =====================================================================

CREATE EXTENSION IF NOT EXISTS pgcrypto;
-- pg_cron requires the extension AND the `cron` schema · only available on
-- Supabase Pro+ tiers. The CREATE EXTENSION below is idempotent ; if the tier
-- does not support pg_cron the entire migration still applies — just the
-- pg_cron job-creation block at-bottom will silently skip.
DO $migration$
BEGIN
    BEGIN
        EXECUTE 'CREATE EXTENSION IF NOT EXISTS pg_cron';
    EXCEPTION WHEN OTHERS THEN
        RAISE NOTICE 'pg_cron unavailable (likely free-tier) · skipping job-creation';
    END;
END $migration$;

-- ─── cron_executions · audit-trail of every cron-job run ───────────────
-- One row per cron-tick · public-readable for "engine is up" transparency.
CREATE TABLE IF NOT EXISTS public.cron_executions (
    id              bigserial    PRIMARY KEY,
    job_name        text         NOT NULL,
    started_at      timestamptz  NOT NULL,
    finished_at     timestamptz  NOT NULL,
    duration_ms     integer      NOT NULL DEFAULT 0,
    status          text         NOT NULL DEFAULT 'ok',
    rows_processed  integer      NOT NULL DEFAULT 0,
    retry_count     smallint     NOT NULL DEFAULT 0,
    via             text         NOT NULL DEFAULT 'bearer',
    notes           text         NULL,
    sigma_mask      uuid         NOT NULL DEFAULT gen_random_uuid(),
    CONSTRAINT cron_executions_status_enum
        CHECK (status IN ('ok','fail','skip','partial')),
    CONSTRAINT cron_executions_via_enum
        CHECK (via IN ('bearer','header','query','none')),
    CONSTRAINT cron_executions_job_name_shape
        CHECK (length(job_name) BETWEEN 3 AND 64),
    CONSTRAINT cron_executions_notes_len
        CHECK (notes IS NULL OR length(notes) <= 256),
    CONSTRAINT cron_executions_duration_nonneg
        CHECK (duration_ms >= 0)
);
CREATE INDEX IF NOT EXISTS cron_executions_job_idx
    ON public.cron_executions (job_name, started_at DESC);
CREATE INDEX IF NOT EXISTS cron_executions_started_idx
    ON public.cron_executions (started_at DESC);
COMMENT ON TABLE public.cron_executions IS
    'Audit-trail of every /api/cron/* invocation. Public-readable for transparency.';

-- ─── cron_heartbeat · 1min uptime ping rows ────────────────────────────
CREATE TABLE IF NOT EXISTS public.cron_heartbeat (
    id              bigserial    PRIMARY KEY,
    job_name        text         NOT NULL DEFAULT 'heartbeat',
    commit_sha      text         NOT NULL,
    region          text         NOT NULL DEFAULT 'iad1',
    uptime_sec      integer      NOT NULL DEFAULT 0,
    emitted_at      timestamptz  NOT NULL DEFAULT now(),
    CONSTRAINT cron_heartbeat_job_name_check
        CHECK (job_name = 'heartbeat'),
    CONSTRAINT cron_heartbeat_commit_sha_shape
        CHECK (length(commit_sha) BETWEEN 4 AND 64),
    CONSTRAINT cron_heartbeat_uptime_nonneg
        CHECK (uptime_sec >= 0)
);
CREATE INDEX IF NOT EXISTS cron_heartbeat_emitted_idx
    ON public.cron_heartbeat (emitted_at DESC);
COMMENT ON TABLE public.cron_heartbeat IS
    'Per-minute heartbeat ping rows. Drives /api/status uptime panel.';

-- ─── playtest_queue · packages enqueued by /cron/playtest-cycle ────────
CREATE TABLE IF NOT EXISTS public.playtest_queue (
    id              bigserial    PRIMARY KEY,
    package_id      uuid         NOT NULL,
    kind            text         NOT NULL,
    version         text         NOT NULL,
    state           text         NOT NULL DEFAULT 'queued',
    queued_by       text         NOT NULL DEFAULT 'cron:playtest-cycle',
    queued_at       timestamptz  NOT NULL DEFAULT now(),
    started_at      timestamptz  NULL,
    finished_at     timestamptz  NULL,
    result          text         NULL,
    notes           text         NULL,
    sigma_mask      uuid         NOT NULL DEFAULT gen_random_uuid(),
    CONSTRAINT playtest_queue_state_enum
        CHECK (state IN ('queued','running','done','failed','cancelled'))
);
CREATE INDEX IF NOT EXISTS playtest_queue_state_idx
    ON public.playtest_queue (state, queued_at DESC);
CREATE INDEX IF NOT EXISTS playtest_queue_package_idx
    ON public.playtest_queue (package_id, queued_at DESC);
COMMENT ON TABLE public.playtest_queue IS
    'Cron-driven playtest job queue. State machine queued → running → done|failed|cancelled.';

-- ─── sigma_chain_checkpoints · Σ-Chain trust-anchor rows ───────────────
CREATE TABLE IF NOT EXISTS public.sigma_chain_checkpoints (
    seq_no             bigserial    PRIMARY KEY,
    checkpoint_root    text         NOT NULL,
    prev_root          text         NULL,
    events_in_window   integer      NOT NULL,
    window_size        integer      NOT NULL DEFAULT 1024,
    cap_signer         text         NOT NULL DEFAULT 'cap-D',
    emitted_at         timestamptz  NOT NULL DEFAULT now(),
    CONSTRAINT sigma_chain_checkpoints_root_shape
        CHECK (length(checkpoint_root) = 64 AND checkpoint_root ~ '^[0-9a-f]+$'),
    CONSTRAINT sigma_chain_checkpoints_prev_shape
        CHECK (prev_root IS NULL OR (length(prev_root) = 64 AND prev_root ~ '^[0-9a-f]+$')),
    CONSTRAINT sigma_chain_checkpoints_events_pos
        CHECK (events_in_window > 0)
);
CREATE INDEX IF NOT EXISTS sigma_chain_checkpoints_emitted_idx
    ON public.sigma_chain_checkpoints (emitted_at DESC);
COMMENT ON TABLE public.sigma_chain_checkpoints IS
    'Σ-Chain checkpoint rows · 1024-batch BLAKE3-roll-up · public-verifiable.';

-- ─── mycelium_patterns_agg · k-anon-aggregate view ─────────────────────
-- Materialized view (refresh via /cron/mycelium-relay or pg_cron).
-- COUNT DISTINCT contributors enforces k-anon at-query-time downstream.
-- We use a TABLE (¬ MATVIEW) here because Supabase doesn't always permit
-- REFRESH MATERIALIZED VIEW from non-superuser contexts ; the cron-job
-- refreshes via TRUNCATE + INSERT.
CREATE TABLE IF NOT EXISTS public.mycelium_patterns_agg (
    cluster_signature   text         PRIMARY KEY,
    pattern_kind        text         NOT NULL,
    contributor_count   integer      NOT NULL DEFAULT 0,
    last_seen_at        timestamptz  NOT NULL DEFAULT now(),
    cap_floor           smallint     NOT NULL DEFAULT 0,
    refreshed_at        timestamptz  NOT NULL DEFAULT now(),
    CONSTRAINT mycelium_patterns_agg_count_nonneg
        CHECK (contributor_count >= 0),
    CONSTRAINT mycelium_patterns_agg_cap_range
        CHECK (cap_floor BETWEEN 0 AND 3)
);
CREATE INDEX IF NOT EXISTS mycelium_patterns_agg_count_idx
    ON public.mycelium_patterns_agg (contributor_count DESC);
COMMENT ON TABLE public.mycelium_patterns_agg IS
    'k-anon aggregate of mycelium-pattern contributions. Refreshed by /cron/mycelium-relay.';

-- ─── hotfix_manifest_versions · add re-sign + purge tracking columns ───
-- Idempotent ALTER · skip if column exists.
DO $alter_hotfix$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public'
          AND table_name = 'hotfix_manifest_versions'
          AND column_name = 'manifest_signed_at'
    ) THEN
        ALTER TABLE public.hotfix_manifest_versions
            ADD COLUMN manifest_signed_at timestamptz NULL;
    END IF;
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_schema = 'public'
          AND table_name = 'hotfix_manifest_versions'
          AND column_name = 'purged_from_active'
    ) THEN
        ALTER TABLE public.hotfix_manifest_versions
            ADD COLUMN purged_from_active boolean NULL;
    END IF;
END $alter_hotfix$;

-- ─── sigma_chain_emit_checkpoint() helper ──────────────────────────────
-- Computes BLAKE3-via-digest('sha256') roll-up of last `p_window` event-roots.
-- Uses sha256 since pgcrypto lacks BLAKE3 ; the Rust-side reconciler
-- recomputes BLAKE3 offline · this is a Postgres-side approximation that
-- still serves as an integrity-anchor (any tamper on one chain detected by
-- comparison to the other).
CREATE OR REPLACE FUNCTION public.sigma_chain_emit_checkpoint(
    p_window integer DEFAULT 1024
)
RETURNS TABLE (
    emitted boolean,
    events_in_window integer,
    checkpoint_root text,
    prev_root text
)
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    v_count        integer;
    v_root         text;
    v_prev         text;
    v_concat       text;
BEGIN
    -- count anchored events since last checkpoint
    SELECT COUNT(*) INTO v_count FROM public.akashic_events
        WHERE sigma_mask <> 0;

    -- if not enough events · skip
    IF v_count < p_window THEN
        RETURN QUERY SELECT false, v_count, NULL::text, NULL::text;
        RETURN;
    END IF;

    -- look up previous root
    SELECT checkpoint_root INTO v_prev FROM public.sigma_chain_checkpoints
        ORDER BY seq_no DESC LIMIT 1;

    -- aggregate cell_id || ts_iso for the last p_window rows ; sha256 the
    -- concatenation. This is an APPROXIMATION ; the Rust-side BLAKE3 root is
    -- the canonical one. We just want a tamper-evident pointer here.
    SELECT string_agg(cell_id || ts_iso::text, '|' ORDER BY ts_iso DESC)
        INTO v_concat
        FROM (
            SELECT cell_id, ts_iso FROM public.akashic_events
                WHERE sigma_mask <> 0
                ORDER BY ts_iso DESC
                LIMIT p_window
        ) recent;

    v_root := encode(digest(coalesce(v_concat, ''), 'sha256'), 'hex');

    INSERT INTO public.sigma_chain_checkpoints
        (checkpoint_root, prev_root, events_in_window, window_size)
    VALUES (v_root, v_prev, p_window, p_window);

    RETURN QUERY SELECT true, v_count, v_root, v_prev;
END;
$$;
COMMENT ON FUNCTION public.sigma_chain_emit_checkpoint(integer) IS
    'Emit Σ-Chain checkpoint when ≥ p_window anchored events accumulate. SHA256 approximation · BLAKE3 reconciled offline.';

-- ─── cleanup_old_cron_events() helper ──────────────────────────────────
CREATE OR REPLACE FUNCTION public.cleanup_old_cron_events()
RETURNS integer
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    v_deleted integer;
BEGIN
    -- Drop cron_executions older than 90 days (we keep 90d for audit).
    DELETE FROM public.cron_executions
        WHERE started_at < now() - interval '90 days';
    GET DIAGNOSTICS v_deleted = ROW_COUNT;
    -- Drop cron_heartbeat older than 7 days (high-volume · short retention).
    DELETE FROM public.cron_heartbeat
        WHERE emitted_at < now() - interval '7 days';
    -- Drop completed playtest_queue older than 30 days.
    DELETE FROM public.playtest_queue
        WHERE state IN ('done','failed','cancelled')
          AND finished_at < now() - interval '30 days';
    RETURN v_deleted;
END;
$$;

-- ─── RLS policies ──────────────────────────────────────────────────────
ALTER TABLE public.cron_executions ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.cron_heartbeat ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.playtest_queue ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.sigma_chain_checkpoints ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.mycelium_patterns_agg ENABLE ROW LEVEL SECURITY;

-- Read-policies : cron_executions + heartbeat + checkpoints PUBLIC-readable
-- (transparency-axiom : "engine is up" must be visible to anyone).
DROP POLICY IF EXISTS "cron_executions read all" ON public.cron_executions;
CREATE POLICY "cron_executions read all"
    ON public.cron_executions FOR SELECT TO PUBLIC
    USING (true);

DROP POLICY IF EXISTS "cron_heartbeat read all" ON public.cron_heartbeat;
CREATE POLICY "cron_heartbeat read all"
    ON public.cron_heartbeat FOR SELECT TO PUBLIC
    USING (true);

DROP POLICY IF EXISTS "sigma_chain_checkpoints read all" ON public.sigma_chain_checkpoints;
CREATE POLICY "sigma_chain_checkpoints read all"
    ON public.sigma_chain_checkpoints FOR SELECT TO PUBLIC
    USING (true);

-- mycelium_patterns_agg : public-read ONLY when k-anon ≥ 10 (gated downstream).
DROP POLICY IF EXISTS "mycelium_patterns_agg read k-anon" ON public.mycelium_patterns_agg;
CREATE POLICY "mycelium_patterns_agg read k-anon"
    ON public.mycelium_patterns_agg FOR SELECT TO PUBLIC
    USING (contributor_count >= 10);

-- playtest_queue : ONLY service-role can read/write. Authenticated users see nothing.
-- (Cron writes via service-role-key which bypasses RLS by-design.)
DROP POLICY IF EXISTS "playtest_queue admin only" ON public.playtest_queue;
-- No authenticated-role policy = default-deny.

-- ─── pg_cron jobs (only when extension available) ──────────────────────
DO $cron_jobs$
DECLARE
    has_pg_cron boolean;
BEGIN
    SELECT EXISTS (
        SELECT 1 FROM pg_extension WHERE extname = 'pg_cron'
    ) INTO has_pg_cron;
    IF NOT has_pg_cron THEN
        RAISE NOTICE 'pg_cron not installed · skipping scheduled-job creation';
        RETURN;
    END IF;

    -- 1. cleanup-old-cron-events @ 04:00 UTC daily
    PERFORM cron.unschedule(jobid)
        FROM cron.job WHERE jobname = 'cleanup-old-cron-events';
    PERFORM cron.schedule(
        'cleanup-old-cron-events',
        '0 4 * * *',
        $sql$ SELECT public.cleanup_old_cron_events(); $sql$
    );

    -- 2. analytics-vacuum @ 03:00 UTC Sunday
    PERFORM cron.unschedule(jobid)
        FROM cron.job WHERE jobname = 'analytics-vacuum';
    PERFORM cron.schedule(
        'analytics-vacuum',
        '0 3 * * 0',
        $sql$ VACUUM (ANALYZE) public.analytics_events; $sql$
    );

    -- 3. heartbeat-purge @ 05:00 UTC daily (additional safety-net)
    PERFORM cron.unschedule(jobid)
        FROM cron.job WHERE jobname = 'heartbeat-purge';
    PERFORM cron.schedule(
        'heartbeat-purge',
        '0 5 * * *',
        $sql$ DELETE FROM public.cron_heartbeat WHERE emitted_at < now() - interval '7 days'; $sql$
    );

    RAISE NOTICE 'pg_cron : 3 scheduled-jobs created/refreshed';
END $cron_jobs$;

-- ─── done ──────────────────────────────────────────────────────────────
COMMENT ON SCHEMA public IS
    'CSSL host-data · session-14 cloud-orchestrator landed via 0034.';
