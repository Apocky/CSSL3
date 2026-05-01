-- =====================================================================
-- § T11-WAVE3-SUPABASE · 0002_rls_policies.sql
-- Row-Level Security policies for all 4 tables
--
-- Policy summary
--   public.assets          : SELECT public · INSERT/UPDATE/DELETE service-role only
--   public.scenes          : SELECT own OR is_public=true · INSERT/UPDATE/DELETE own only
--   public.history         : SELECT own OR user_id IS NULL · INSERT own (or anon)
--                          · DELETE own
--   public.companion_logs  : SELECT/INSERT own only · UPDATE forbidden
--                          · DELETE service-role only (audit-immutable for users)
-- =====================================================================

-- ---------------------------------------------------------------------
-- public.assets
-- ---------------------------------------------------------------------
ALTER TABLE public.assets ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.assets FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS "assets_select_public"        ON public.assets;
DROP POLICY IF EXISTS "assets_insert_service_role"  ON public.assets;
DROP POLICY IF EXISTS "assets_update_service_role"  ON public.assets;
DROP POLICY IF EXISTS "assets_delete_service_role"  ON public.assets;

CREATE POLICY "assets_select_public"
    ON public.assets FOR SELECT
    USING (true);

CREATE POLICY "assets_insert_service_role"
    ON public.assets FOR INSERT
    WITH CHECK (auth.role() = 'service_role');

CREATE POLICY "assets_update_service_role"
    ON public.assets FOR UPDATE
    USING (auth.role() = 'service_role')
    WITH CHECK (auth.role() = 'service_role');

CREATE POLICY "assets_delete_service_role"
    ON public.assets FOR DELETE
    USING (auth.role() = 'service_role');

-- ---------------------------------------------------------------------
-- public.scenes
-- ---------------------------------------------------------------------
ALTER TABLE public.scenes ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.scenes FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS "scenes_select_own_or_public" ON public.scenes;
DROP POLICY IF EXISTS "scenes_insert_own"           ON public.scenes;
DROP POLICY IF EXISTS "scenes_update_own"           ON public.scenes;
DROP POLICY IF EXISTS "scenes_delete_own"           ON public.scenes;

CREATE POLICY "scenes_select_own_or_public"
    ON public.scenes FOR SELECT
    USING (
        is_public = true
        OR user_id = auth.uid()
        OR auth.role() = 'service_role'
    );

CREATE POLICY "scenes_insert_own"
    ON public.scenes FOR INSERT
    WITH CHECK (user_id = auth.uid());

CREATE POLICY "scenes_update_own"
    ON public.scenes FOR UPDATE
    USING (user_id = auth.uid())
    WITH CHECK (user_id = auth.uid());

CREATE POLICY "scenes_delete_own"
    ON public.scenes FOR DELETE
    USING (user_id = auth.uid());

-- ---------------------------------------------------------------------
-- public.history
-- ---------------------------------------------------------------------
ALTER TABLE public.history ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.history FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS "history_select_own_or_anon" ON public.history;
DROP POLICY IF EXISTS "history_insert_own_or_anon" ON public.history;
DROP POLICY IF EXISTS "history_delete_own"         ON public.history;
DROP POLICY IF EXISTS "history_update_forbidden"   ON public.history;

-- SELECT: own rows OR fully anonymous rows OR service_role
CREATE POLICY "history_select_own_or_anon"
    ON public.history FOR SELECT
    USING (
        user_id = auth.uid()
        OR user_id IS NULL
        OR auth.role() = 'service_role'
    );

-- INSERT: row.user_id MUST equal auth.uid() OR be NULL (anonymous opt-in)
CREATE POLICY "history_insert_own_or_anon"
    ON public.history FOR INSERT
    WITH CHECK (
        user_id IS NULL
        OR user_id = auth.uid()
    );

-- DELETE: own only (anonymous rows = service-role only)
CREATE POLICY "history_delete_own"
    ON public.history FOR DELETE
    USING (
        user_id = auth.uid()
        OR auth.role() = 'service_role'
    );
-- No UPDATE policy → all UPDATEs denied (history is append-mostly)

-- ---------------------------------------------------------------------
-- public.companion_logs · audit-immutable for users
-- ---------------------------------------------------------------------
ALTER TABLE public.companion_logs ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.companion_logs FORCE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS "companion_logs_select_own"           ON public.companion_logs;
DROP POLICY IF EXISTS "companion_logs_insert_own"           ON public.companion_logs;
DROP POLICY IF EXISTS "companion_logs_delete_service_role"  ON public.companion_logs;

CREATE POLICY "companion_logs_select_own"
    ON public.companion_logs FOR SELECT
    USING (
        user_id = auth.uid()
        OR auth.role() = 'service_role'
    );

CREATE POLICY "companion_logs_insert_own"
    ON public.companion_logs FOR INSERT
    WITH CHECK (user_id = auth.uid());

CREATE POLICY "companion_logs_delete_service_role"
    ON public.companion_logs FOR DELETE
    USING (auth.role() = 'service_role');
-- No UPDATE policy → all UPDATEs denied
-- No user-DELETE policy → users cannot delete their own audit trail

-- ---------------------------------------------------------------------
-- Grants (RLS still gates row visibility)
-- ---------------------------------------------------------------------
GRANT SELECT                          ON public.assets         TO anon, authenticated;
GRANT SELECT, INSERT, UPDATE, DELETE  ON public.scenes         TO authenticated;
GRANT SELECT                          ON public.scenes         TO anon;
GRANT SELECT, INSERT, DELETE          ON public.history        TO authenticated;
GRANT SELECT, INSERT                  ON public.history        TO anon;
GRANT SELECT, INSERT                  ON public.companion_logs TO authenticated;
