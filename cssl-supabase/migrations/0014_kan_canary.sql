-- =====================================================================
-- § T11-W7-F-MIGRATIONS · 0014_kan_canary.sql
-- KAN-RIDE canary enrollment + per-call disagreement tracking.
-- Ref : specs/grand-vision/11_KAN_RIDE.csl § A/B-TESTING + ATTESTATION-PER-SESSION.
-- § PRIME_DIRECTIVE : sovereignty preserved · player revokes via UI
-- (revoked_at = unilateral · no-confirm) · NO surveillance · audit-trail
-- immutable (UPDATE/DELETE blocked on disagreements in 0015). Apply after 0013.
-- =====================================================================
CREATE EXTENSION IF NOT EXISTS pgcrypto;

-- § kan_canary_enrollment · per-(player,classifier-handle) canary participation
CREATE TABLE IF NOT EXISTS public.kan_canary_enrollment (
    enrollment_id      uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
    player_id          uuid        NOT NULL,
    classifier_handle  text        NOT NULL,
    swap_point         text        NOT NULL,
    canary_pct         smallint    NOT NULL DEFAULT 10,
    enrolled_at        timestamptz NOT NULL DEFAULT now(),
    revoked_at         timestamptz NULL,
    CONSTRAINT kan_canary_enrollment_pct_range
        CHECK (canary_pct BETWEEN 0 AND 100),
    CONSTRAINT kan_canary_enrollment_swap_point_enum
        CHECK (swap_point IN ('SP-1','SP-2','SP-3','SP-4','SP-5')),
    CONSTRAINT kan_canary_enrollment_handle_length
        CHECK (char_length(classifier_handle) BETWEEN 1 AND 200),
    UNIQUE (player_id, classifier_handle)
);
CREATE INDEX IF NOT EXISTS kan_canary_enrollment_player_id_idx
    ON public.kan_canary_enrollment (player_id);
CREATE INDEX IF NOT EXISTS kan_canary_enrollment_active_idx
    ON public.kan_canary_enrollment (player_id) WHERE revoked_at IS NULL;
COMMENT ON TABLE  public.kan_canary_enrollment IS
    'KAN-RIDE A/B-test enrollment per (player,handle). revoked_at = unilateral player revoke (PRIME_DIRECTIVE). Players UPDATE revoked_at ; service_role DELETE.';
COMMENT ON COLUMN public.kan_canary_enrollment.classifier_handle IS
    'Stage-1 classifier id (e.g. ''intent-real-v1''). Cross-refs ClassifierRegistry impl-id.';
COMMENT ON COLUMN public.kan_canary_enrollment.swap_point  IS 'SP-1..SP-5 (intent_router, cocreative, spontaneous, dm_arbiter, gm_pacing).';
COMMENT ON COLUMN public.kan_canary_enrollment.canary_pct  IS '0..100 — pct of classify-calls routed to stage-1.';
COMMENT ON COLUMN public.kan_canary_enrollment.revoked_at  IS 'Unilateral player revoke timestamp. NULL = active enrollment.';

-- § kan_canary_disagreements · per-call divergence stage-0 ⊕ stage-1 (audit-immutable)
CREATE TABLE IF NOT EXISTS public.kan_canary_disagreements (
    disagreement_id    uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
    session_id         uuid        NOT NULL,
    player_id          uuid        NOT NULL,
    swap_point         text        NOT NULL,
    ts_micros          bigint      NOT NULL,
    stage0_output      jsonb       NOT NULL,
    stage1_output      jsonb       NOT NULL,
    disagreement_kind  text        NOT NULL,
    perf_stage0_us     integer     NULL,
    perf_stage1_us     integer     NULL,
    audit_event_id     uuid        NULL,
    CONSTRAINT kan_canary_disagreements_swap_point_enum
        CHECK (swap_point IN ('SP-1','SP-2','SP-3','SP-4','SP-5')),
    CONSTRAINT kan_canary_disagreements_kind_enum
        CHECK (disagreement_kind IN ('kind-mismatch','confidence-delta>0.2','args-mismatch','latency-overshoot'))
);
CREATE INDEX IF NOT EXISTS kan_canary_disagreements_session_idx     ON public.kan_canary_disagreements (session_id, ts_micros);
CREATE INDEX IF NOT EXISTS kan_canary_disagreements_player_idx      ON public.kan_canary_disagreements (player_id, ts_micros);
CREATE INDEX IF NOT EXISTS kan_canary_disagreements_swap_point_idx  ON public.kan_canary_disagreements (swap_point);
COMMENT ON TABLE  public.kan_canary_disagreements IS
    'Per-classify-call divergence (stage-0 vs stage-1). INSERT-only by service_role · UPDATE/DELETE blocked entirely (audit-trail invariant). Feeds rollback trigger T-2 + graduate-metric M-1.';
COMMENT ON COLUMN public.kan_canary_disagreements.ts_micros         IS 'Monotonic frame-id (microseconds since session start).';
COMMENT ON COLUMN public.kan_canary_disagreements.disagreement_kind IS 'kind-mismatch | confidence-delta>0.2 | args-mismatch | latency-overshoot.';
COMMENT ON COLUMN public.kan_canary_disagreements.audit_event_id    IS 'Cross-link to cssl-host-attestation row (I-3 audit-emit invariant).';

-- § helper : is-enrolled?
CREATE OR REPLACE FUNCTION public.kan_canary_is_enrolled(p_player_id uuid, p_handle text)
RETURNS boolean LANGUAGE sql STABLE AS $$
    SELECT EXISTS (
        SELECT 1 FROM public.kan_canary_enrollment
         WHERE player_id = p_player_id AND classifier_handle = p_handle AND revoked_at IS NULL
    );
$$;
COMMENT ON FUNCTION public.kan_canary_is_enrolled(uuid, text) IS 'TRUE iff (player,handle) has an active (non-revoked) enrollment. STABLE ; safe in policy clauses.';

-- § helper : record disagreement (SECURITY DEFINER · service-role insert path)
CREATE OR REPLACE FUNCTION public.kan_canary_record_disagreement(
    p_session_id uuid, p_player_id uuid, p_swap_point text, p_ts_micros bigint,
    p_stage0_output jsonb, p_stage1_output jsonb, p_disagreement_kind text,
    p_perf_stage0_us integer, p_perf_stage1_us integer, p_audit_event_id uuid
) RETURNS uuid LANGUAGE plpgsql SECURITY DEFINER AS $$
DECLARE v_id uuid;
BEGIN
    INSERT INTO public.kan_canary_disagreements (
        session_id, player_id, swap_point, ts_micros, stage0_output, stage1_output,
        disagreement_kind, perf_stage0_us, perf_stage1_us, audit_event_id
    ) VALUES (
        p_session_id, p_player_id, p_swap_point, p_ts_micros, p_stage0_output, p_stage1_output,
        p_disagreement_kind, p_perf_stage0_us, p_perf_stage1_us, p_audit_event_id
    ) RETURNING disagreement_id INTO v_id;
    RETURN v_id;
END;
$$;
COMMENT ON FUNCTION public.kan_canary_record_disagreement(uuid,uuid,text,bigint,jsonb,jsonb,text,integer,integer,uuid) IS
    'SECURITY DEFINER insert path for host populator. Returns disagreement_id. Caller must pass the correct player_id.';

-- § view : per-(session,SP,player) disagreement rollup · feeds T-2 + M-1 (RLS propagates)
DROP VIEW IF EXISTS public.kan_canary_metrics_per_player_per_sp;
CREATE VIEW public.kan_canary_metrics_per_player_per_sp AS
    SELECT player_id, session_id, swap_point,
           count(*)                                                            AS disagreements,
           count(*) FILTER (WHERE disagreement_kind = 'kind-mismatch')         AS kind_mismatches,
           count(*) FILTER (WHERE disagreement_kind = 'confidence-delta>0.2') AS confidence_deltas,
           count(*) FILTER (WHERE disagreement_kind = 'args-mismatch')         AS args_mismatches,
           count(*) FILTER (WHERE disagreement_kind = 'latency-overshoot')     AS latency_overshoots,
           avg(perf_stage0_us)                                                 AS avg_stage0_us,
           avg(perf_stage1_us)                                                 AS avg_stage1_us,
           min(ts_micros)                                                      AS first_ts,
           max(ts_micros)                                                      AS last_ts
      FROM public.kan_canary_disagreements
     GROUP BY player_id, session_id, swap_point;
COMMENT ON VIEW public.kan_canary_metrics_per_player_per_sp IS
    'Per-(session,SP,player) disagreement rollup. Feeds rollback trigger T-2 + graduate-criterion M-1.';
