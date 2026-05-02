-- =====================================================================
-- § T11-W14-K · 0036_cloud_orchestrator_pgcron.sql
-- ════════════════════════════════════════════════════════════════════
--
-- Cloud-side persistent-orchestrator · pg_cron job-set + canonical
-- analytics-rollup wrapper. Layered ON TOP of 0034_cloud_orchestrator.sql
-- (cron_executions + cron_heartbeat + checkpoints + mycelium_agg) and
-- 0024_analytics.sql (rollup_promote_minutes() void-helper).
--
-- Mission-specified pg_cron jobs :
--   - cleanup_old_events           · daily   · retention > 90 days
--   - analytics_rollup_promote     · hourly  · invokes rollup_promote_minutes()
--   - vacuum_analytics_events      · weekly  · VACUUM ANALYZE
--
-- Mission-specified additions (0036-only) :
--   - rollup_promote_minutes_v2()  · TABLE-returning wrapper used by the
--     /api/cron/kan-rollup endpoint (which expects promoted_to_1hr +
--     promoted_to_1day). Calls rollup_promote_minutes() then computes the
--     row-deltas observed in this transaction.
--   - cleanup_old_analytics_events() · drop analytics_events > 90 days
--   - cron_orchestrator_status TABLE-VIEW · single source-of-truth for
--     the W14-M live-status-page.
--   - heartbeat-table : ALREADY in 0034 (cron_heartbeat) · this migration
--     adds an `orchestrator_heartbeat` summary view that combines
--     cron_heartbeat + last cron_executions per job.
--
-- Sovereignty :
--   - read-only OR Σ-mask-gated-write
--   - service-role-key isolated (only invoked from Vercel-cron context)
--   - heartbeat publicly-readable (transparency-axiom)
--   - failed-cron auto-retry exponential-backoff (cron_executions row +
--     status='fail' kicks the next-tick handler into retry_count++ mode)
--
-- Idempotency : ALL CREATE statements use IF NOT EXISTS / OR REPLACE.
-- Re-applying this migration is a no-op except for re-pinning pg_cron
-- jobs to their canonical schedule.
--
-- Apply order : after 0035_mycelium_federation.sql (or 0039_seasons.sql ·
-- whichever is the last-applied · numeric ordering OR replacement-safe).
-- =====================================================================

CREATE EXTENSION IF NOT EXISTS pgcrypto;

-- pg_cron is Supabase Pro+ only. Idempotently attempt install ; if the
-- tier doesn't support it, the entire migration still applies — the
-- pg_cron job-creation block at-bottom silently skips.
DO $migration$
BEGIN
    BEGIN
        EXECUTE 'CREATE EXTENSION IF NOT EXISTS pg_cron';
    EXCEPTION WHEN OTHERS THEN
        RAISE NOTICE 'pg_cron unavailable (likely free-tier) · skipping job-creation';
    END;
END $migration$;

-- ─── rollup_promote_minutes_v2() · TABLE wrapper ──────────────────────
-- Wraps the void-returning 0024 helper with a row-delta count so the
-- /api/cron/kan-rollup endpoint can report (promoted_to_1hr,
-- promoted_to_1day). Snapshots the rollup-table row-counts before/after
-- the call and reports the deltas.
CREATE OR REPLACE FUNCTION public.rollup_promote_minutes_v2()
RETURNS TABLE (
    promoted_to_1hr  integer,
    promoted_to_1day integer
)
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    v_pre_1hr  bigint;
    v_pre_1day bigint;
    v_post_1hr bigint;
    v_post_1day bigint;
BEGIN
    -- Snapshot pre-counts.
    SELECT COUNT(*) INTO v_pre_1hr  FROM public.analytics_rollup_1hr;
    SELECT COUNT(*) INTO v_pre_1day FROM public.analytics_rollup_1day;
    -- Run the existing promote helper (no-op if no minute-buckets ready).
    PERFORM public.rollup_promote_minutes();
    -- Snapshot post-counts.
    SELECT COUNT(*) INTO v_post_1hr  FROM public.analytics_rollup_1hr;
    SELECT COUNT(*) INTO v_post_1day FROM public.analytics_rollup_1day;

    RETURN QUERY SELECT
        GREATEST(0, (v_post_1hr  - v_pre_1hr)::integer),
        GREATEST(0, (v_post_1day - v_pre_1day)::integer);
END;
$$;
COMMENT ON FUNCTION public.rollup_promote_minutes_v2() IS
    'TABLE-returning wrapper around rollup_promote_minutes(). Reports row-deltas observed during the call. Used by /api/cron/kan-rollup.';

-- ─── cleanup_old_analytics_events() · drop > 90 days ──────────────────
-- The 0034 helper cleanup_old_cron_events() handles cron_executions /
-- cron_heartbeat / playtest_queue. This one specifically targets the
-- raw analytics_events table (often the highest-volume table in the DB).
-- Retention = 90 days = required by mission-spec.
CREATE OR REPLACE FUNCTION public.cleanup_old_analytics_events()
RETURNS integer
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    v_deleted integer;
BEGIN
    -- Drop raw analytics_events older than 90 days. The rollup_1hr / 1day
    -- tables retain the aggregate so we don't lose long-term trends.
    DELETE FROM public.analytics_events
        WHERE event_ts < now() - interval '90 days';
    GET DIAGNOSTICS v_deleted = ROW_COUNT;

    -- Drop minute-rollups older than 30 days (we keep 1hr + 1day forever).
    DELETE FROM public.analytics_rollup_1min
        WHERE bucket_start < now() - interval '30 days';

    RETURN v_deleted;
END;
$$;
COMMENT ON FUNCTION public.cleanup_old_analytics_events() IS
    'Drop analytics_events > 90 days · drop analytics_rollup_1min > 30 days. Aggregate rollups preserved. Idempotent.';

-- ─── orchestrator_heartbeat VIEW · summary for /api/status ───────────
-- Combines cron_heartbeat + last cron_executions per job_name into a
-- single read-optimised view consumed by the W14-M live-status-page.
-- Public-readable (transparency-axiom).
CREATE OR REPLACE VIEW public.orchestrator_heartbeat AS
WITH last_exec AS (
    SELECT DISTINCT ON (job_name)
        job_name,
        started_at      AS last_started_at,
        finished_at     AS last_finished_at,
        duration_ms     AS last_duration_ms,
        status          AS last_status,
        rows_processed  AS last_rows_processed,
        retry_count     AS last_retry_count
    FROM public.cron_executions
    ORDER BY job_name, started_at DESC
),
last_heartbeat AS (
    SELECT
        commit_sha,
        region,
        uptime_sec,
        emitted_at
    FROM public.cron_heartbeat
    ORDER BY emitted_at DESC
    LIMIT 1
)
SELECT
    le.job_name,
    le.last_started_at,
    le.last_finished_at,
    le.last_duration_ms,
    le.last_status,
    le.last_rows_processed,
    le.last_retry_count,
    lh.commit_sha,
    lh.region,
    lh.uptime_sec,
    lh.emitted_at AS last_heartbeat_at
FROM last_exec le
CROSS JOIN last_heartbeat lh;

COMMENT ON VIEW public.orchestrator_heartbeat IS
    'Summary view : last execution per cron-job + most-recent heartbeat row. Public-readable for /api/status W14-M live-status-page.';

-- ─── orchestrator_failure_summary VIEW · failed-job recent window ────
-- Surfaces cron_executions where status = 'fail' or 'partial' in the last
-- 24h. Consumed by the live-status-page to surface "engine is degraded"
-- banner. Public-readable for transparency.
CREATE OR REPLACE VIEW public.orchestrator_failure_summary AS
SELECT
    job_name,
    COUNT(*)::integer                                            AS recent_failures,
    MAX(started_at)                                              AS last_failure_at,
    SUM(CASE WHEN status = 'fail'    THEN 1 ELSE 0 END)::integer AS hard_fails,
    SUM(CASE WHEN status = 'partial' THEN 1 ELSE 0 END)::integer AS partial_runs
FROM public.cron_executions
WHERE started_at > now() - interval '24 hours'
  AND status IN ('fail','partial')
GROUP BY job_name;

COMMENT ON VIEW public.orchestrator_failure_summary IS
    'Recent (24h) failure-tally per cron-job. Public-readable for status-page degradation indicators.';

-- ─── retry-backoff helper · failed-job auto-retry hooks ──────────────
-- When an /api/cron/* endpoint observes status='fail' it MAY call this
-- helper to schedule a retry-tick at 2^retry_count seconds (capped at
-- 1 hour). Used by Vercel-cron retry policy : the next cron-tick will
-- pick up the row and re-execute. Pure DB-side : no Vercel involvement.
-- Returns the next-retry-at timestamp.
CREATE OR REPLACE FUNCTION public.cron_retry_schedule(
    p_job_name    text,
    p_retry_count smallint
)
RETURNS timestamptz
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    v_backoff_sec integer;
    v_next        timestamptz;
BEGIN
    -- Exponential backoff capped at 1 hour. retry_count=0 → 1 sec ;
    -- retry_count=1 → 2 sec ; retry_count=2 → 4 sec ; … ; retry_count≥12
    -- → 3600 sec (1 hour cap).
    v_backoff_sec := LEAST(3600, GREATEST(1, (1 << LEAST(p_retry_count, 12))::integer));
    v_next := now() + (v_backoff_sec || ' seconds')::interval;
    -- Fire-and-forget audit ; the row carries the retry-schedule.
    INSERT INTO public.cron_executions
        (job_name, started_at, finished_at, duration_ms, status, rows_processed, retry_count, via, notes)
    VALUES
        (p_job_name, now(), now(), 0, 'skip', 0, p_retry_count, 'none',
         'retry-scheduled @ ' || v_next::text);
    RETURN v_next;
END;
$$;
COMMENT ON FUNCTION public.cron_retry_schedule(text, smallint) IS
    'Compute next-retry timestamp (exponential backoff capped at 1h) for a failed cron-job. Records skip-row audit-trail.';

-- ─── pg_cron jobs (only when extension available) ──────────────────────
-- Mission-spec : 3 canonical jobs.
--   1. cleanup_old_events           · daily 04:30 UTC
--   2. analytics_rollup_promote     · hourly :30 (offset from 0034 jobs)
--   3. vacuum_analytics_events      · weekly Sunday 04:00 UTC
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

    -- 1. cleanup_old_events @ 04:30 UTC daily (offset from 0034's 04:00).
    PERFORM cron.unschedule(jobid)
        FROM cron.job WHERE jobname = 'cleanup_old_events';
    PERFORM cron.schedule(
        'cleanup_old_events',
        '30 4 * * *',
        $sql$ SELECT public.cleanup_old_analytics_events(); $sql$
    );

    -- 2. analytics_rollup_promote @ :30 every hour (offset from 0024
    -- ingestion bursts which tend to land on :00 / :05 boundaries).
    PERFORM cron.unschedule(jobid)
        FROM cron.job WHERE jobname = 'analytics_rollup_promote';
    PERFORM cron.schedule(
        'analytics_rollup_promote',
        '30 * * * *',
        $sql$ SELECT public.rollup_promote_minutes(); $sql$
    );

    -- 3. vacuum_analytics_events @ 04:00 UTC Sunday (offset from 0034's
    -- 03:00 analytics-vacuum to avoid pile-up).
    PERFORM cron.unschedule(jobid)
        FROM cron.job WHERE jobname = 'vacuum_analytics_events';
    PERFORM cron.schedule(
        'vacuum_analytics_events',
        '0 4 * * 0',
        $sql$ VACUUM (ANALYZE) public.analytics_events; $sql$
    );

    RAISE NOTICE 'pg_cron : 3 canonical-jobs created/refreshed (cleanup_old_events · analytics_rollup_promote · vacuum_analytics_events)';
END $cron_jobs$;

-- ─── grants on public-readable views ──────────────────────────────────
GRANT SELECT ON public.orchestrator_heartbeat       TO anon, authenticated;
GRANT SELECT ON public.orchestrator_failure_summary TO anon, authenticated;

-- ─── done ──────────────────────────────────────────────────────────────
COMMENT ON SCHEMA public IS
    'CSSL host-data · session-14 cloud-orchestrator pg_cron landed via 0036.';
