-- =====================================================================
-- § T11-W11-ANALYTICS · 0024_analytics.sql
-- ════════════════════════════════════════════════════════════════════
--
-- Analytics-event ingest schema. Σ-mask consent-column on EVERY row.
-- Default-deny per-player surfacing ; only aggregate views are
-- exposed unless cap permits per-event relay.
--
-- § NOTE on numbering : the spec asked for slot 0023 but that's already
--   taken by 0023_payments_rls.sql. Using monotonic 0024.
--
-- § Tables :
--   - analytics_events             · raw event-stream (16-byte bit-pack
--                                    in `payload_b64`) · player_id +
--                                    sigma_consent_cap on every row
--   - analytics_rollup_1min        · per-(player, minute, kind) counters
--   - analytics_rollup_1hr         · per-(player, hour, kind) counters
--   - analytics_rollup_1day        · per-(player, day, kind) counters
--   - analytics_event_kinds        · LUT mapping kind_id → kind_name
--                                    (matches Rust EventKind enum 0..13)
--
-- § Helpers :
--   - ingest_event(...)            · single-row insert with Σ-mask check
--   - rollup_promote_minutes()     · fold rows from minute → hour →day
--
-- § Σ-mask discipline :
--   - sigma_consent_cap : 0=Deny · 1=LocalOnly · 2=AggregateRelay · 3=Full
--   - rollup tables PRESERVE consent-cap so downstream aggregators can
--     filter rows where cap < 2 (aggregate-relay disallowed)
--   - RLS in 0025_analytics_rls.sql enforces self-only or aggregate-only
--
-- Apply order : after 0023.
-- =====================================================================
CREATE EXTENSION IF NOT EXISTS pgcrypto;

-- ─── analytics_event_kinds LUT ─────────────────────────────────────
-- Static mapping kind_id → canonical kind_name. Matches Rust crate
-- cssl-analytics-aggregator::EventKind enum exactly.
CREATE TABLE IF NOT EXISTS public.analytics_event_kinds (
    kind_id    smallint    PRIMARY KEY,
    kind_name  text        NOT NULL UNIQUE,
    CONSTRAINT analytics_event_kinds_id_range CHECK (kind_id BETWEEN 0 AND 14)
);
COMMENT ON TABLE public.analytics_event_kinds IS
    'LUT for event-kind discriminants. Matches Rust EventKind enum 0..13.';

INSERT INTO public.analytics_event_kinds (kind_id, kind_name) VALUES
    (0, 'engine.frame_tick'),
    (1, 'engine.render_mode_changed'),
    (2, 'input.text_typed'),
    (3, 'input.text_submitted'),
    (4, 'intent.classified'),
    (5, 'intent.routed'),
    (6, 'gm.response_emitted'),
    (7, 'dm.phase_transition'),
    (8, 'procgen.scene_built'),
    (9, 'mcp.tool_called'),
    (10, 'kan.classified'),
    (11, 'mycelium.sync_event'),
    (12, 'consent.cap_granted'),
    (13, 'consent.cap_revoked')
ON CONFLICT (kind_id) DO NOTHING;

-- ─── analytics_events · raw event-stream ───────────────────────────
-- One row per ingested event. `payload_b64` carries the Rust 8-byte
-- payload base64-encoded. `frame_offset` is differential from session
-- start (matches the bit-pack record `frame_offset` u32 field).
CREATE TABLE IF NOT EXISTS public.analytics_events (
    event_id           uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
    player_id          uuid        NOT NULL,
    session_id         uuid        NOT NULL,
    kind_id            smallint    NOT NULL,
    payload_kind       smallint    NOT NULL,
    flags              integer     NOT NULL DEFAULT 0,
    frame_offset       integer     NOT NULL,
    payload_b64        text        NOT NULL,
    sigma_consent_cap  smallint    NOT NULL DEFAULT 0,
    ingested_at        timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT analytics_events_kind_fk
        FOREIGN KEY (kind_id) REFERENCES public.analytics_event_kinds(kind_id),
    CONSTRAINT analytics_events_consent_range
        CHECK (sigma_consent_cap BETWEEN 0 AND 3),
    CONSTRAINT analytics_events_frame_nonneg
        CHECK (frame_offset >= 0),
    CONSTRAINT analytics_events_payload_b64_length
        CHECK (char_length(payload_b64) BETWEEN 1 AND 24)
);
CREATE INDEX IF NOT EXISTS analytics_events_player_idx
    ON public.analytics_events (player_id, ingested_at DESC);
CREATE INDEX IF NOT EXISTS analytics_events_session_idx
    ON public.analytics_events (session_id);
CREATE INDEX IF NOT EXISTS analytics_events_kind_idx
    ON public.analytics_events (kind_id, ingested_at DESC);
CREATE INDEX IF NOT EXISTS analytics_events_consent_idx
    ON public.analytics_events (sigma_consent_cap)
    WHERE sigma_consent_cap >= 2;
COMMENT ON TABLE public.analytics_events IS
    'Raw analytics-event stream. payload_b64 = Rust EventRecord.payload8 base64. sigma_consent_cap gates per-player surfacing.';

-- ─── analytics_rollup_1min · per-minute aggregates ──────────────────
-- Bucket-key = (player_id, kind_id, bucket_start) where bucket_start is
-- date_trunc('minute', ts). Counters mirror Rust BucketCounters struct.
CREATE TABLE IF NOT EXISTS public.analytics_rollup_1min (
    player_id          uuid        NOT NULL,
    kind_id            smallint    NOT NULL,
    bucket_start       timestamptz NOT NULL,
    sigma_consent_cap  smallint    NOT NULL,
    count              integer     NOT NULL DEFAULT 0,
    sum_payload32      bigint      NOT NULL DEFAULT 0,
    min_payload32      integer     NOT NULL DEFAULT 0,
    max_payload32      integer     NOT NULL DEFAULT 0,
    fallback_count     integer     NOT NULL DEFAULT 0,
    error_count        integer     NOT NULL DEFAULT 0,
    PRIMARY KEY (player_id, kind_id, bucket_start),
    CONSTRAINT analytics_rollup_1min_consent_range
        CHECK (sigma_consent_cap BETWEEN 0 AND 3),
    CONSTRAINT analytics_rollup_1min_kind_fk
        FOREIGN KEY (kind_id) REFERENCES public.analytics_event_kinds(kind_id)
);
CREATE INDEX IF NOT EXISTS analytics_rollup_1min_bucket_idx
    ON public.analytics_rollup_1min (bucket_start DESC);
CREATE INDEX IF NOT EXISTS analytics_rollup_1min_kind_idx
    ON public.analytics_rollup_1min (kind_id, bucket_start DESC);
COMMENT ON TABLE public.analytics_rollup_1min IS
    '1-minute bucketed event-counters. Matches Rust BucketCounters. PK = (player, kind, bucket).';

-- ─── analytics_rollup_1hr · per-hour aggregates ─────────────────────
CREATE TABLE IF NOT EXISTS public.analytics_rollup_1hr (
    player_id          uuid        NOT NULL,
    kind_id            smallint    NOT NULL,
    bucket_start       timestamptz NOT NULL,
    sigma_consent_cap  smallint    NOT NULL,
    count              integer     NOT NULL DEFAULT 0,
    sum_payload32      bigint      NOT NULL DEFAULT 0,
    min_payload32      integer     NOT NULL DEFAULT 0,
    max_payload32      integer     NOT NULL DEFAULT 0,
    fallback_count     integer     NOT NULL DEFAULT 0,
    error_count        integer     NOT NULL DEFAULT 0,
    PRIMARY KEY (player_id, kind_id, bucket_start),
    CONSTRAINT analytics_rollup_1hr_consent_range
        CHECK (sigma_consent_cap BETWEEN 0 AND 3),
    CONSTRAINT analytics_rollup_1hr_kind_fk
        FOREIGN KEY (kind_id) REFERENCES public.analytics_event_kinds(kind_id)
);
CREATE INDEX IF NOT EXISTS analytics_rollup_1hr_bucket_idx
    ON public.analytics_rollup_1hr (bucket_start DESC);
COMMENT ON TABLE public.analytics_rollup_1hr IS
    '1-hour bucketed event-counters. Filled by rollup_promote_minutes().';

-- ─── analytics_rollup_1day · per-day aggregates ─────────────────────
CREATE TABLE IF NOT EXISTS public.analytics_rollup_1day (
    player_id          uuid        NOT NULL,
    kind_id            smallint    NOT NULL,
    bucket_start       timestamptz NOT NULL,
    sigma_consent_cap  smallint    NOT NULL,
    count              integer     NOT NULL DEFAULT 0,
    sum_payload32      bigint      NOT NULL DEFAULT 0,
    min_payload32      integer     NOT NULL DEFAULT 0,
    max_payload32      integer     NOT NULL DEFAULT 0,
    fallback_count     integer     NOT NULL DEFAULT 0,
    error_count        integer     NOT NULL DEFAULT 0,
    PRIMARY KEY (player_id, kind_id, bucket_start),
    CONSTRAINT analytics_rollup_1day_consent_range
        CHECK (sigma_consent_cap BETWEEN 0 AND 3),
    CONSTRAINT analytics_rollup_1day_kind_fk
        FOREIGN KEY (kind_id) REFERENCES public.analytics_event_kinds(kind_id)
);
CREATE INDEX IF NOT EXISTS analytics_rollup_1day_bucket_idx
    ON public.analytics_rollup_1day (bucket_start DESC);
COMMENT ON TABLE public.analytics_rollup_1day IS
    '1-day bucketed event-counters. Filled by rollup_promote_minutes().';

-- ─── ingest_event helper · idempotent per-event insert ──────────────
-- Called from /api/analytics/event POST. Validates Σ-mask + writes raw
-- row + bumps the 1min rollup atomically.
CREATE OR REPLACE FUNCTION public.ingest_event(
    p_player_id          uuid,
    p_session_id         uuid,
    p_kind_id            smallint,
    p_payload_kind       smallint,
    p_flags              integer,
    p_frame_offset       integer,
    p_payload_b64        text,
    p_sigma_consent_cap  smallint
) RETURNS uuid
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    new_event_id uuid;
    bucket_min   timestamptz;
    payload32    integer;
    fallback_inc integer;
    error_inc    integer;
BEGIN
    -- Σ-mask gate : Deny ⇒ silently drop (return NULL).
    IF p_sigma_consent_cap = 0 THEN
        RETURN NULL;
    END IF;

    -- Insert raw row.
    INSERT INTO public.analytics_events
        (player_id, session_id, kind_id, payload_kind, flags,
         frame_offset, payload_b64, sigma_consent_cap)
    VALUES
        (p_player_id, p_session_id, p_kind_id, p_payload_kind, p_flags,
         p_frame_offset, p_payload_b64, p_sigma_consent_cap)
    RETURNING event_id INTO new_event_id;

    -- Bump 1-minute rollup. payload32 = first 4 bytes of payload (decoded
    -- on Rust client ; here we approximate with frame_offset for sum/min/max
    -- because the b64-payload is opaque to SQL. Real downstream rollup
    -- jobs decode the b64 server-side.)
    bucket_min := date_trunc('minute', now());
    payload32 := p_frame_offset;
    fallback_inc := CASE WHEN (p_flags & 4) <> 0 THEN 1 ELSE 0 END;
    error_inc := CASE WHEN (p_flags & 8) = 0 AND p_kind_id = 9 THEN 1 ELSE 0 END;

    INSERT INTO public.analytics_rollup_1min
        (player_id, kind_id, bucket_start, sigma_consent_cap,
         count, sum_payload32, min_payload32, max_payload32,
         fallback_count, error_count)
    VALUES
        (p_player_id, p_kind_id, bucket_min, p_sigma_consent_cap,
         1, payload32, payload32, payload32,
         fallback_inc, error_inc)
    ON CONFLICT (player_id, kind_id, bucket_start)
    DO UPDATE SET
        count          = analytics_rollup_1min.count + 1,
        sum_payload32  = analytics_rollup_1min.sum_payload32 + EXCLUDED.sum_payload32,
        min_payload32  = LEAST(analytics_rollup_1min.min_payload32, EXCLUDED.min_payload32),
        max_payload32  = GREATEST(analytics_rollup_1min.max_payload32, EXCLUDED.max_payload32),
        fallback_count = analytics_rollup_1min.fallback_count + fallback_inc,
        error_count    = analytics_rollup_1min.error_count + error_inc;

    RETURN new_event_id;
END;
$$;
COMMENT ON FUNCTION public.ingest_event(uuid, uuid, smallint, smallint, integer, integer, text, smallint) IS
    'Σ-mask-gated event insert + 1min rollup bump. cap=0 ⇒ returns NULL.';

-- ─── rollup_promote_minutes() · fold 1min → 1hr → 1day ──────────────
-- Run periodically (e.g. cron once-per-hour). Folds each 1min row into
-- the matching 1hr + 1day bucket. Idempotent ON CONFLICT.
CREATE OR REPLACE FUNCTION public.rollup_promote_minutes()
RETURNS void
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
BEGIN
    -- Fold minute-rows into hour buckets.
    INSERT INTO public.analytics_rollup_1hr
        (player_id, kind_id, bucket_start, sigma_consent_cap,
         count, sum_payload32, min_payload32, max_payload32,
         fallback_count, error_count)
    SELECT
        player_id,
        kind_id,
        date_trunc('hour', bucket_start),
        MAX(sigma_consent_cap),
        SUM(count),
        SUM(sum_payload32),
        MIN(min_payload32),
        MAX(max_payload32),
        SUM(fallback_count),
        SUM(error_count)
    FROM public.analytics_rollup_1min
    WHERE bucket_start < date_trunc('hour', now())
    GROUP BY player_id, kind_id, date_trunc('hour', bucket_start)
    ON CONFLICT (player_id, kind_id, bucket_start)
    DO UPDATE SET
        count          = EXCLUDED.count,
        sum_payload32  = EXCLUDED.sum_payload32,
        min_payload32  = LEAST(analytics_rollup_1hr.min_payload32, EXCLUDED.min_payload32),
        max_payload32  = GREATEST(analytics_rollup_1hr.max_payload32, EXCLUDED.max_payload32),
        fallback_count = EXCLUDED.fallback_count,
        error_count    = EXCLUDED.error_count;

    -- Fold hour-rows into day buckets.
    INSERT INTO public.analytics_rollup_1day
        (player_id, kind_id, bucket_start, sigma_consent_cap,
         count, sum_payload32, min_payload32, max_payload32,
         fallback_count, error_count)
    SELECT
        player_id,
        kind_id,
        date_trunc('day', bucket_start),
        MAX(sigma_consent_cap),
        SUM(count),
        SUM(sum_payload32),
        MIN(min_payload32),
        MAX(max_payload32),
        SUM(fallback_count),
        SUM(error_count)
    FROM public.analytics_rollup_1hr
    WHERE bucket_start < date_trunc('day', now())
    GROUP BY player_id, kind_id, date_trunc('day', bucket_start)
    ON CONFLICT (player_id, kind_id, bucket_start)
    DO UPDATE SET
        count          = EXCLUDED.count,
        sum_payload32  = EXCLUDED.sum_payload32,
        min_payload32  = LEAST(analytics_rollup_1day.min_payload32, EXCLUDED.min_payload32),
        max_payload32  = GREATEST(analytics_rollup_1day.max_payload32, EXCLUDED.max_payload32),
        fallback_count = EXCLUDED.fallback_count,
        error_count    = EXCLUDED.error_count;
END;
$$;
COMMENT ON FUNCTION public.rollup_promote_minutes() IS
    'Periodic fold : 1min → 1hr → 1day rollup tables. Idempotent. SECURITY DEFINER.';

-- ─── RLS policies ──────────────────────────────────────────────────
-- Σ-mask discipline : default-deny per-player surfacing. Self can read
-- own rows ; service_role bypasses for ingestion + rollup-job.
-- AGGREGATE views (cap >= 2) are exposed via the metrics endpoint.
ALTER TABLE public.analytics_events ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.analytics_rollup_1min ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.analytics_rollup_1hr ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.analytics_rollup_1day ENABLE ROW LEVEL SECURITY;

-- Self-read on raw events.
DROP POLICY IF EXISTS "analytics_events_select_self" ON public.analytics_events;
CREATE POLICY "analytics_events_select_self"
    ON public.analytics_events FOR SELECT
    USING (auth.uid() = player_id OR auth.role() = 'service_role');

DROP POLICY IF EXISTS "analytics_events_service_write" ON public.analytics_events;
CREATE POLICY "analytics_events_service_write"
    ON public.analytics_events FOR ALL
    USING (auth.role() = 'service_role')
    WITH CHECK (auth.role() = 'service_role');

-- Self-read on rollups (cap >= AggregateRelay surface to others).
DROP POLICY IF EXISTS "analytics_rollup_1min_select" ON public.analytics_rollup_1min;
CREATE POLICY "analytics_rollup_1min_select"
    ON public.analytics_rollup_1min FOR SELECT
    USING (
        auth.uid() = player_id
        OR auth.role() = 'service_role'
        OR sigma_consent_cap >= 2
    );

DROP POLICY IF EXISTS "analytics_rollup_1min_service_write" ON public.analytics_rollup_1min;
CREATE POLICY "analytics_rollup_1min_service_write"
    ON public.analytics_rollup_1min FOR ALL
    USING (auth.role() = 'service_role')
    WITH CHECK (auth.role() = 'service_role');

DROP POLICY IF EXISTS "analytics_rollup_1hr_select" ON public.analytics_rollup_1hr;
CREATE POLICY "analytics_rollup_1hr_select"
    ON public.analytics_rollup_1hr FOR SELECT
    USING (
        auth.uid() = player_id
        OR auth.role() = 'service_role'
        OR sigma_consent_cap >= 2
    );

DROP POLICY IF EXISTS "analytics_rollup_1hr_service_write" ON public.analytics_rollup_1hr;
CREATE POLICY "analytics_rollup_1hr_service_write"
    ON public.analytics_rollup_1hr FOR ALL
    USING (auth.role() = 'service_role')
    WITH CHECK (auth.role() = 'service_role');

DROP POLICY IF EXISTS "analytics_rollup_1day_select" ON public.analytics_rollup_1day;
CREATE POLICY "analytics_rollup_1day_select"
    ON public.analytics_rollup_1day FOR SELECT
    USING (
        auth.uid() = player_id
        OR auth.role() = 'service_role'
        OR sigma_consent_cap >= 2
    );

DROP POLICY IF EXISTS "analytics_rollup_1day_service_write" ON public.analytics_rollup_1day;
CREATE POLICY "analytics_rollup_1day_service_write"
    ON public.analytics_rollup_1day FOR ALL
    USING (auth.role() = 'service_role')
    WITH CHECK (auth.role() = 'service_role');

-- Event-kinds LUT is world-readable (it's static metadata).
ALTER TABLE public.analytics_event_kinds ENABLE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS "analytics_event_kinds_world_read" ON public.analytics_event_kinds;
CREATE POLICY "analytics_event_kinds_world_read"
    ON public.analytics_event_kinds FOR SELECT
    USING (true);
