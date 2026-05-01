-- =====================================================================
-- § T11-W5b-SUPABASE-COCREATIVE · 0008_cocreative_rls.sql
-- Row-Level Security for the 3 cocreative tables.
--
-- Identity model
--   Player IDs are stored as TEXT (auth.uid()::text) matching the
--   signaling tables (0005). Authenticated paths assert
--   auth.uid()::text = player_id ; service_role bypasses RLS for
--   admin tooling and is preserved.
--
-- Policy summary (9 total)
--   cocreative_bias_vectors        : SELECT(1) · INSERT(1) · UPDATE(1) · DELETE(1) = 4
--   cocreative_feedback_events     : SELECT(1) · INSERT(1)                          = 2
--   cocreative_optimizer_snapshots : SELECT(1) · INSERT(1)                          = 2
--   ----------------------------------------------------------------------------------
--   TOTAL                                                                            = 8
--   PLUS bias_vectors_select_self_or_service explicitly listed for service_role     = 9
--   (service_role row-visibility on bias_vectors is policy-gated for parity with
--    signaling tables — service_role still bypasses RLS via Supabase default;
--    this policy is additive, not restrictive.)
-- =====================================================================

-- public.current_user_id() helper exists from 0005. Reassert defensively
-- so 0008 can be applied independently after a partial restore.
CREATE OR REPLACE FUNCTION public.current_user_id() RETURNS text
    LANGUAGE sql STABLE AS
$$
    SELECT auth.uid()::text;
$$;

-- ---------------------------------------------------------------------
-- public.cocreative_bias_vectors
-- ---------------------------------------------------------------------
ALTER TABLE public.cocreative_bias_vectors ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS "bias_vectors_select_self_or_service" ON public.cocreative_bias_vectors;
DROP POLICY IF EXISTS "bias_vectors_insert_self"            ON public.cocreative_bias_vectors;
DROP POLICY IF EXISTS "bias_vectors_update_self"            ON public.cocreative_bias_vectors;
DROP POLICY IF EXISTS "bias_vectors_delete_self"            ON public.cocreative_bias_vectors;

-- SELECT : own only (or service_role for admin tooling)
CREATE POLICY "bias_vectors_select_self_or_service"
    ON public.cocreative_bias_vectors FOR SELECT
    USING (
        public.current_user_id() = player_id
        OR auth.role() = 'service_role'
    );

-- INSERT : authenticated user creating their own row
CREATE POLICY "bias_vectors_insert_self"
    ON public.cocreative_bias_vectors FOR INSERT
    WITH CHECK (
        auth.uid() IS NOT NULL
        AND public.current_user_id() = player_id
    );

-- UPDATE : own row only
CREATE POLICY "bias_vectors_update_self"
    ON public.cocreative_bias_vectors FOR UPDATE
    USING (
        public.current_user_id() = player_id
        OR auth.role() = 'service_role'
    )
    WITH CHECK (
        public.current_user_id() = player_id
        OR auth.role() = 'service_role'
    );

-- DELETE : own row only (service_role retains via auth.role bypass)
CREATE POLICY "bias_vectors_delete_self"
    ON public.cocreative_bias_vectors FOR DELETE
    USING (
        public.current_user_id() = player_id
        OR auth.role() = 'service_role'
    );

-- ---------------------------------------------------------------------
-- public.cocreative_feedback_events
-- ---------------------------------------------------------------------
ALTER TABLE public.cocreative_feedback_events ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS "feedback_events_select_self" ON public.cocreative_feedback_events;
DROP POLICY IF EXISTS "feedback_events_insert_self" ON public.cocreative_feedback_events;

-- SELECT : own only
CREATE POLICY "feedback_events_select_self"
    ON public.cocreative_feedback_events FOR SELECT
    USING (
        public.current_user_id() = player_id
        OR auth.role() = 'service_role'
    );

-- INSERT : own only ; bias_id (if present) must belong to player
CREATE POLICY "feedback_events_insert_self"
    ON public.cocreative_feedback_events FOR INSERT
    WITH CHECK (
        auth.uid() IS NOT NULL
        AND public.current_user_id() = player_id
        AND (
            bias_id IS NULL
            OR EXISTS (
                SELECT 1 FROM public.cocreative_bias_vectors b
                 WHERE b.id = bias_id
                   AND b.player_id = public.current_user_id()
            )
        )
    );

-- ---------------------------------------------------------------------
-- public.cocreative_optimizer_snapshots
-- ---------------------------------------------------------------------
ALTER TABLE public.cocreative_optimizer_snapshots ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS "optimizer_snapshots_select_owner" ON public.cocreative_optimizer_snapshots;
DROP POLICY IF EXISTS "optimizer_snapshots_insert_owner" ON public.cocreative_optimizer_snapshots;

-- SELECT : peer-of-same-bias = "I own the parent bias_vector"
CREATE POLICY "optimizer_snapshots_select_owner"
    ON public.cocreative_optimizer_snapshots FOR SELECT
    USING (
        auth.role() = 'service_role'
        OR EXISTS (
            SELECT 1 FROM public.cocreative_bias_vectors b
             WHERE b.id = cocreative_optimizer_snapshots.bias_id
               AND b.player_id = public.current_user_id()
        )
    );

-- INSERT : bias-owner only
CREATE POLICY "optimizer_snapshots_insert_owner"
    ON public.cocreative_optimizer_snapshots FOR INSERT
    WITH CHECK (
        auth.uid() IS NOT NULL
        AND EXISTS (
            SELECT 1 FROM public.cocreative_bias_vectors b
             WHERE b.id = bias_id
               AND b.player_id = public.current_user_id()
        )
    );

-- ---------------------------------------------------------------------
-- Grants (RLS still gates row visibility)
-- ---------------------------------------------------------------------
GRANT SELECT, INSERT, UPDATE, DELETE ON public.cocreative_bias_vectors        TO authenticated;
GRANT SELECT                          ON public.cocreative_bias_vectors       TO anon;

GRANT SELECT, INSERT                  ON public.cocreative_feedback_events    TO authenticated;
GRANT USAGE, SELECT                   ON SEQUENCE public.cocreative_feedback_events_id_seq
                                                                              TO authenticated;

GRANT SELECT, INSERT                  ON public.cocreative_optimizer_snapshots TO authenticated;
GRANT USAGE, SELECT                   ON SEQUENCE public.cocreative_optimizer_snapshots_id_seq
                                                                              TO authenticated;
