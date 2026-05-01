-- =====================================================================
-- § T11-W5c-SUPABASE-GAMESTATE · 0012_game_state_rls.sql
-- Row-Level Security for the 3 game-state tables.
--
-- Identity model (matches signaling 0005 + cocreative 0008)
--   Player IDs are stored as TEXT (auth.uid()::text). Authenticated paths
--   assert auth.uid()::text = player_id ; service_role bypasses RLS for
--   admin tooling and is preserved.
--
-- Policy summary (8 total)
--   game_state_snapshots  : SELECT(1) · INSERT(1) · DELETE(1)              = 3
--   game_session_index    : SELECT(1) · INSERT(1) · UPDATE(1)              = 3
--   sovereign_cap_audit   : SELECT(1) · INSERT(1)                          = 2
--   ----------------------------------------------------------------------------
--   TOTAL                                                                  = 8
--
-- Transparency invariant for sovereign_cap_audit :
--   N! UPDATE policy · N! DELETE policy · once written, immutable to all
--   non-service-role principals. service_role can administer via default
--   RLS-bypass. No exceptions.
-- =====================================================================

-- public.current_user_id() helper exists from 0005 (and reasserted in 0008).
-- Reassert again so 0012 can be applied independently after a partial restore.
CREATE OR REPLACE FUNCTION public.current_user_id() RETURNS text
    LANGUAGE sql STABLE AS
$$
    SELECT auth.uid()::text;
$$;

-- ---------------------------------------------------------------------
-- public.game_state_snapshots
-- ---------------------------------------------------------------------
ALTER TABLE public.game_state_snapshots ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS "game_state_snapshots_select_self" ON public.game_state_snapshots;
DROP POLICY IF EXISTS "game_state_snapshots_insert_self" ON public.game_state_snapshots;
DROP POLICY IF EXISTS "game_state_snapshots_delete_self" ON public.game_state_snapshots;

-- SELECT : own only (or service_role for admin tooling)
CREATE POLICY "game_state_snapshots_select_self"
    ON public.game_state_snapshots FOR SELECT
    USING (
        public.current_user_id() = player_id
        OR auth.role() = 'service_role'
    );

-- INSERT : authenticated user appending their own snapshot
CREATE POLICY "game_state_snapshots_insert_self"
    ON public.game_state_snapshots FOR INSERT
    WITH CHECK (
        auth.uid() IS NOT NULL
        AND public.current_user_id() = player_id
    );

-- DELETE : own only — players retain GDPR-style erasure rights
CREATE POLICY "game_state_snapshots_delete_self"
    ON public.game_state_snapshots FOR DELETE
    USING (
        public.current_user_id() = player_id
        OR auth.role() = 'service_role'
    );

-- ---------------------------------------------------------------------
-- public.game_session_index
-- ---------------------------------------------------------------------
ALTER TABLE public.game_session_index ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS "game_session_index_select_self" ON public.game_session_index;
DROP POLICY IF EXISTS "game_session_index_insert_self" ON public.game_session_index;
DROP POLICY IF EXISTS "game_session_index_update_self" ON public.game_session_index;

-- SELECT : own only (or service_role)
CREATE POLICY "game_session_index_select_self"
    ON public.game_session_index FOR SELECT
    USING (
        public.current_user_id() = player_id
        OR auth.role() = 'service_role'
    );

-- INSERT : authenticated user creating their own session
CREATE POLICY "game_session_index_insert_self"
    ON public.game_session_index FOR INSERT
    WITH CHECK (
        auth.uid() IS NOT NULL
        AND public.current_user_id() = player_id
    );

-- UPDATE : own row only (latest_seq / total_snapshots / ended_at / meta)
-- Note : player_id is NOT in the UPDATE-able set ; the WITH CHECK clause
-- prevents reassigning ownership (matches USING).
CREATE POLICY "game_session_index_update_self"
    ON public.game_session_index FOR UPDATE
    USING (
        public.current_user_id() = player_id
        OR auth.role() = 'service_role'
    )
    WITH CHECK (
        public.current_user_id() = player_id
        OR auth.role() = 'service_role'
    );

-- ---------------------------------------------------------------------
-- public.sovereign_cap_audit · INSERT-ONLY · transparency invariant
-- ---------------------------------------------------------------------
-- N! NO UPDATE policy · N! NO DELETE policy. The absence is intentional
-- and load-bearing : with RLS enabled, the absence of a policy = denial
-- of that operation for all non-service-role principals. service_role
-- bypasses RLS by default for admin tooling (e.g. retention sweeps with
-- explicit consent) — application code must NEVER use service_role for
-- normal request paths.
-- ---------------------------------------------------------------------
ALTER TABLE public.sovereign_cap_audit ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS "sovereign_cap_audit_select_self" ON public.sovereign_cap_audit;
DROP POLICY IF EXISTS "sovereign_cap_audit_insert_self" ON public.sovereign_cap_audit;
-- Defensive : if a previous migration ever introduced UPDATE/DELETE
-- policies, drop them here so the transparency invariant is enforced.
DROP POLICY IF EXISTS "sovereign_cap_audit_update_self" ON public.sovereign_cap_audit;
DROP POLICY IF EXISTS "sovereign_cap_audit_delete_self" ON public.sovereign_cap_audit;

-- SELECT : own only (or service_role for compliance audits)
CREATE POLICY "sovereign_cap_audit_select_self"
    ON public.sovereign_cap_audit FOR SELECT
    USING (
        public.current_user_id() = player_id
        OR auth.role() = 'service_role'
    );

-- INSERT : own only — sovereign-cap attestation must always be self-attributable
CREATE POLICY "sovereign_cap_audit_insert_self"
    ON public.sovereign_cap_audit FOR INSERT
    WITH CHECK (
        auth.uid() IS NOT NULL
        AND public.current_user_id() = player_id
    );

-- ---------------------------------------------------------------------
-- Grants (RLS still gates row visibility)
-- ---------------------------------------------------------------------
GRANT SELECT, INSERT, DELETE         ON public.game_state_snapshots TO authenticated;
GRANT USAGE, SELECT                  ON SEQUENCE public.game_state_snapshots_id_seq
                                                                     TO authenticated;

GRANT SELECT, INSERT, UPDATE         ON public.game_session_index   TO authenticated;

-- sovereign_cap_audit grants are issued in 0011 ; do not re-grant UPDATE/DELETE.
