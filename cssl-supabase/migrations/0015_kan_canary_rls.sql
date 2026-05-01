-- =====================================================================
-- § T11-W7-F-MIGRATIONS · 0015_kan_canary_rls.sql
-- RLS for kan_canary_enrollment + kan_canary_disagreements.
-- Ref : specs/grand-vision/11_KAN_RIDE.csl § ROLL-BACK T-5 · § ATTESTATION-PER-SESSION.
-- § PRIME_DIRECTIVE : sovereignty preserved · player revokes via UI ·
-- revoked_at = unilateral · NO surveillance · audit-trail immutable
-- (UPDATE/DELETE on disagreements blocked entirely). service_role bypasses
-- RLS via Supabase default — application code MUST NEVER use service_role
-- for normal request paths. Identity : player_id is uuid (= auth.uid()).
-- Policies : enroll[SELECT,INSERT,UPDATE]=3 · disagree[SELECT,INSERT]=2 = 5 total.
-- =====================================================================

-- § enrollment
ALTER TABLE public.kan_canary_enrollment ENABLE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS "kan_canary_enrollment_select_self" ON public.kan_canary_enrollment;
DROP POLICY IF EXISTS "kan_canary_enrollment_insert_self" ON public.kan_canary_enrollment;
DROP POLICY IF EXISTS "kan_canary_enrollment_update_self" ON public.kan_canary_enrollment;
DROP POLICY IF EXISTS "kan_canary_enrollment_delete_self" ON public.kan_canary_enrollment;

CREATE POLICY "kan_canary_enrollment_select_self"
    ON public.kan_canary_enrollment FOR SELECT
    USING (auth.uid() = player_id OR auth.role() = 'service_role');

CREATE POLICY "kan_canary_enrollment_insert_self"
    ON public.kan_canary_enrollment FOR INSERT
    WITH CHECK (
        (auth.uid() IS NOT NULL AND auth.uid() = player_id)
        OR auth.role() = 'service_role'
    );

-- UPDATE = primary path for player to set revoked_at (unilateral revoke)
CREATE POLICY "kan_canary_enrollment_update_self"
    ON public.kan_canary_enrollment FOR UPDATE
    USING      (auth.uid() = player_id OR auth.role() = 'service_role')
    WITH CHECK (auth.uid() = player_id OR auth.role() = 'service_role');

-- N! NO DELETE policy. Players REVOKE via UPDATE revoked_at ; service_role only hard-deletes.

-- § disagreements · service-role-INSERT only · UPDATE/DELETE blocked
ALTER TABLE public.kan_canary_disagreements ENABLE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS "kan_canary_disagreements_select_self"    ON public.kan_canary_disagreements;
DROP POLICY IF EXISTS "kan_canary_disagreements_insert_service" ON public.kan_canary_disagreements;
DROP POLICY IF EXISTS "kan_canary_disagreements_update_self"    ON public.kan_canary_disagreements;
DROP POLICY IF EXISTS "kan_canary_disagreements_delete_self"    ON public.kan_canary_disagreements;

CREATE POLICY "kan_canary_disagreements_select_self"
    ON public.kan_canary_disagreements FOR SELECT
    USING (auth.uid() = player_id OR auth.role() = 'service_role');

CREATE POLICY "kan_canary_disagreements_insert_service"
    ON public.kan_canary_disagreements FOR INSERT
    WITH CHECK (auth.role() = 'service_role');

-- N! NO UPDATE/DELETE policy · audit-trail invariant — once written, immutable.

-- § grants (RLS gates row visibility)
GRANT SELECT, INSERT, UPDATE ON public.kan_canary_enrollment              TO authenticated;
GRANT SELECT                 ON public.kan_canary_enrollment              TO anon;
GRANT SELECT                 ON public.kan_canary_disagreements           TO authenticated;
GRANT SELECT                 ON public.kan_canary_metrics_per_player_per_sp TO authenticated;
GRANT ALL                    ON public.kan_canary_enrollment              TO service_role;
GRANT ALL                    ON public.kan_canary_disagreements           TO service_role;
GRANT EXECUTE ON FUNCTION public.kan_canary_is_enrolled(uuid, text)
                                                                          TO authenticated, service_role;
GRANT EXECUTE ON FUNCTION public.kan_canary_record_disagreement(
                              uuid,uuid,text,bigint,jsonb,jsonb,text,integer,integer,uuid)
                                                                          TO service_role;
