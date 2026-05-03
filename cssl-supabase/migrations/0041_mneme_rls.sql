-- =====================================================================
-- § T11-WAVE-MNEME · 0041_mneme_rls.sql
-- ════════════════════════════════════════════════════════════════════
-- Row-Level Security policies for MNEME tables.
--
-- Model :
--   - service_role : full access (cssl-edge runs as service-role for writes)
--   - authenticated: scoped to own-profile via JWT claim 'sovereign_pk_hex'
--   - anon         : denied (default-deny once RLS is enabled)
--
-- Caller-PK source (in order of preference) :
--   1. JWT claim   request.jwt.claim.sovereign_pk_hex  (Supabase auth path)
--   2. App-level header forwarding by cssl-edge route handlers (fallback)
--
-- Spec : ../specs/45_MNEME_SCHEMA.csl § RLS
-- =====================================================================

-- Enable RLS on all four tables.
ALTER TABLE public.mneme_profiles ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.mneme_messages ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.mneme_memories ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.mneme_audit    ENABLE ROW LEVEL SECURITY;

-- ─── Helper : safely read JWT-claim sovereign_pk_hex as bytea ──────────
CREATE OR REPLACE FUNCTION public.mneme_caller_pk()
RETURNS bytea
LANGUAGE sql STABLE
AS $$
    SELECT CASE
        WHEN current_setting('request.jwt.claim.sovereign_pk_hex', true) IS NULL THEN NULL
        WHEN current_setting('request.jwt.claim.sovereign_pk_hex', true) = '' THEN NULL
        ELSE decode(current_setting('request.jwt.claim.sovereign_pk_hex', true), 'hex')
    END
$$;

COMMENT ON FUNCTION public.mneme_caller_pk() IS
    'Returns Ed25519 PK from request JWT, or NULL if absent. Used in RLS predicates.';

-- =====================================================================
-- Profiles
-- =====================================================================

-- service_role bypass (full access)
DROP POLICY IF EXISTS mneme_profile_service_all ON public.mneme_profiles;
CREATE POLICY mneme_profile_service_all
    ON public.mneme_profiles
    FOR ALL TO service_role
    USING (true) WITH CHECK (true);

-- authenticated users can SELECT only their own profile
DROP POLICY IF EXISTS mneme_profile_owner_read ON public.mneme_profiles;
CREATE POLICY mneme_profile_owner_read
    ON public.mneme_profiles
    FOR SELECT TO authenticated
    USING (sovereign_pk = public.mneme_caller_pk());

-- authenticated users can INSERT a profile only if the row's sovereign_pk
-- matches their own caller-PK (prevents creating profiles owned by others).
DROP POLICY IF EXISTS mneme_profile_owner_insert ON public.mneme_profiles;
CREATE POLICY mneme_profile_owner_insert
    ON public.mneme_profiles
    FOR INSERT TO authenticated
    WITH CHECK (sovereign_pk = public.mneme_caller_pk());

-- authenticated users can UPDATE only their own profile, and cannot change ownership
DROP POLICY IF EXISTS mneme_profile_owner_update ON public.mneme_profiles;
CREATE POLICY mneme_profile_owner_update
    ON public.mneme_profiles
    FOR UPDATE TO authenticated
    USING (sovereign_pk = public.mneme_caller_pk())
    WITH CHECK (sovereign_pk = public.mneme_caller_pk());

-- =====================================================================
-- Messages
-- =====================================================================

DROP POLICY IF EXISTS mneme_message_service_all ON public.mneme_messages;
CREATE POLICY mneme_message_service_all
    ON public.mneme_messages
    FOR ALL TO service_role
    USING (true) WITH CHECK (true);

-- authenticated read: must own the parent profile AND the row mask must not be revoked
DROP POLICY IF EXISTS mneme_message_owner_read ON public.mneme_messages;
CREATE POLICY mneme_message_owner_read
    ON public.mneme_messages
    FOR SELECT TO authenticated
    USING (
        public.mneme_mask_revoked_at(sigma_mask) = 0
        AND EXISTS (
            SELECT 1 FROM public.mneme_profiles p
             WHERE p.profile_id   = mneme_messages.profile_id
               AND p.sovereign_pk = public.mneme_caller_pk()
        )
    );

-- authenticated insert: must own the parent profile
DROP POLICY IF EXISTS mneme_message_owner_insert ON public.mneme_messages;
CREATE POLICY mneme_message_owner_insert
    ON public.mneme_messages
    FOR INSERT TO authenticated
    WITH CHECK (
        EXISTS (
            SELECT 1 FROM public.mneme_profiles p
             WHERE p.profile_id   = mneme_messages.profile_id
               AND p.sovereign_pk = public.mneme_caller_pk()
        )
    );

-- No UPDATE policy (raw messages are immutable).
-- No DELETE policy at v1 (forget = sigma-mask revoke, not row deletion).

-- =====================================================================
-- Memories
-- =====================================================================

DROP POLICY IF EXISTS mneme_memory_service_all ON public.mneme_memories;
CREATE POLICY mneme_memory_service_all
    ON public.mneme_memories
    FOR ALL TO service_role
    USING (true) WITH CHECK (true);

-- authenticated read: own profile AND not revoked
DROP POLICY IF EXISTS mneme_memory_owner_read ON public.mneme_memories;
CREATE POLICY mneme_memory_owner_read
    ON public.mneme_memories
    FOR SELECT TO authenticated
    USING (
        public.mneme_mask_revoked_at(sigma_mask) = 0
        AND EXISTS (
            SELECT 1 FROM public.mneme_profiles p
             WHERE p.profile_id   = mneme_memories.profile_id
               AND p.sovereign_pk = public.mneme_caller_pk()
        )
    );

-- authenticated insert: own profile
DROP POLICY IF EXISTS mneme_memory_owner_insert ON public.mneme_memories;
CREATE POLICY mneme_memory_owner_insert
    ON public.mneme_memories
    FOR INSERT TO authenticated
    WITH CHECK (
        EXISTS (
            SELECT 1 FROM public.mneme_profiles p
             WHERE p.profile_id   = mneme_memories.profile_id
               AND p.sovereign_pk = public.mneme_caller_pk()
        )
    );

-- authenticated update: own profile only; canonical fields are immutable.
-- The route handler enforces "only superseded_by + sigma_mask are mutable"
-- at app level; defense-in-depth here is to require ownership at minimum.
DROP POLICY IF EXISTS mneme_memory_owner_update ON public.mneme_memories;
CREATE POLICY mneme_memory_owner_update
    ON public.mneme_memories
    FOR UPDATE TO authenticated
    USING (
        EXISTS (
            SELECT 1 FROM public.mneme_profiles p
             WHERE p.profile_id   = mneme_memories.profile_id
               AND p.sovereign_pk = public.mneme_caller_pk()
        )
    )
    WITH CHECK (
        EXISTS (
            SELECT 1 FROM public.mneme_profiles p
             WHERE p.profile_id   = mneme_memories.profile_id
               AND p.sovereign_pk = public.mneme_caller_pk()
        )
    );

-- No DELETE policy at v1 (forget = sigma-mask revoke, not row deletion).

-- =====================================================================
-- Audit
-- =====================================================================

DROP POLICY IF EXISTS mneme_audit_service_all ON public.mneme_audit;
CREATE POLICY mneme_audit_service_all
    ON public.mneme_audit
    FOR ALL TO service_role
    USING (true) WITH CHECK (true);

-- authenticated read: own profile only
DROP POLICY IF EXISTS mneme_audit_owner_read ON public.mneme_audit;
CREATE POLICY mneme_audit_owner_read
    ON public.mneme_audit
    FOR SELECT TO authenticated
    USING (
        EXISTS (
            SELECT 1 FROM public.mneme_profiles p
             WHERE p.profile_id   = mneme_audit.profile_id
               AND p.sovereign_pk = public.mneme_caller_pk()
        )
    );

-- No INSERT policy for authenticated (audit writes via service-role only).
-- No UPDATE / DELETE policies (append-only).

-- =====================================================================
-- Done.
-- =====================================================================
