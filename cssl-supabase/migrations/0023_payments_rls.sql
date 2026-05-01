-- =====================================================================
-- § T11-W9-PAYMENTS-RLS · 0023_payments_rls.sql
-- Row-Level Security for the 5 W9 payments tables.
--
-- Identity model (matches 0020_player_progression) :
--   player_id is uuid · auth.uid() = player_id for self-rows · service_role
--   bypasses RLS for webhook-handler + admin tooling.
--
-- Policy summary (10 total, 2 per table) :
--   stripe_customers          : SELECT(self)              + service_role
--   stripe_checkout_sessions  : SELECT(self)              + service_role
--   entitlements              : SELECT(self)              + service_role
--   stripe_refunds            : SELECT(self)              + service_role
--   stripe_webhook_events     : ALL = service_role only · INSERT-ONLY (no UPDATE/DELETE)
--
-- Apply order : after 0022.
-- =====================================================================

-- ─── stripe_customers · self-read · service_role bypass ────────────────
ALTER TABLE public.stripe_customers ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS "stripe_customers_select_self" ON public.stripe_customers;
CREATE POLICY "stripe_customers_select_self"
    ON public.stripe_customers FOR SELECT
    USING (auth.uid() = player_id OR auth.role() = 'service_role');

-- INSERT/UPDATE only via webhook (service_role) · default-deny everything else.
DROP POLICY IF EXISTS "stripe_customers_service_write" ON public.stripe_customers;
CREATE POLICY "stripe_customers_service_write"
    ON public.stripe_customers FOR ALL
    USING (auth.role() = 'service_role')
    WITH CHECK (auth.role() = 'service_role');

-- ─── stripe_checkout_sessions · self-read · service_role bypass ────────
ALTER TABLE public.stripe_checkout_sessions ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS "stripe_checkout_sessions_select_self" ON public.stripe_checkout_sessions;
CREATE POLICY "stripe_checkout_sessions_select_self"
    ON public.stripe_checkout_sessions FOR SELECT
    USING (auth.uid() = player_id OR auth.role() = 'service_role');

DROP POLICY IF EXISTS "stripe_checkout_sessions_service_write" ON public.stripe_checkout_sessions;
CREATE POLICY "stripe_checkout_sessions_service_write"
    ON public.stripe_checkout_sessions FOR ALL
    USING (auth.role() = 'service_role')
    WITH CHECK (auth.role() = 'service_role');

-- ─── entitlements · self-read · service_role bypass ────────────────────
ALTER TABLE public.entitlements ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS "entitlements_select_self" ON public.entitlements;
CREATE POLICY "entitlements_select_self"
    ON public.entitlements FOR SELECT
    USING (auth.uid() = player_id OR auth.role() = 'service_role');

DROP POLICY IF EXISTS "entitlements_service_write" ON public.entitlements;
CREATE POLICY "entitlements_service_write"
    ON public.entitlements FOR ALL
    USING (auth.role() = 'service_role')
    WITH CHECK (auth.role() = 'service_role');

-- ─── stripe_refunds · self-read · service_role bypass ──────────────────
ALTER TABLE public.stripe_refunds ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS "stripe_refunds_select_self" ON public.stripe_refunds;
CREATE POLICY "stripe_refunds_select_self"
    ON public.stripe_refunds FOR SELECT
    USING (auth.uid() = player_id OR auth.role() = 'service_role');

DROP POLICY IF EXISTS "stripe_refunds_service_write" ON public.stripe_refunds;
CREATE POLICY "stripe_refunds_service_write"
    ON public.stripe_refunds FOR ALL
    USING (auth.role() = 'service_role')
    WITH CHECK (auth.role() = 'service_role');

-- ─── stripe_webhook_events · service_role only · INSERT-ONLY ──────────
-- This table holds Stripe's full event payloads; players have NO access. Only
-- the webhook-handler (running as service_role) writes here, and rows are
-- IMMUTABLE — no UPDATE/DELETE policy means defaults-deny for those.
ALTER TABLE public.stripe_webhook_events ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS "stripe_webhook_events_service_select" ON public.stripe_webhook_events;
CREATE POLICY "stripe_webhook_events_service_select"
    ON public.stripe_webhook_events FOR SELECT
    USING (auth.role() = 'service_role');

DROP POLICY IF EXISTS "stripe_webhook_events_service_insert" ON public.stripe_webhook_events;
CREATE POLICY "stripe_webhook_events_service_insert"
    ON public.stripe_webhook_events FOR INSERT
    WITH CHECK (auth.role() = 'service_role');

-- N! UPDATE policy · N! DELETE policy → idempotency log is immutable.

-- ─── grants for helper functions ──────────────────────────────────────
-- grant_entitlement / revoke_entitlement are SECURITY DEFINER · service_role
-- already has access by default; we restrict authenticated principals from
-- calling these directly (they should never need to · webhook is the only
-- caller).
REVOKE EXECUTE ON FUNCTION public.grant_entitlement(uuid, text, text) FROM PUBLIC;
REVOKE EXECUTE ON FUNCTION public.grant_entitlement(uuid, text, text) FROM authenticated;
GRANT  EXECUTE ON FUNCTION public.grant_entitlement(uuid, text, text) TO service_role;

REVOKE EXECUTE ON FUNCTION public.revoke_entitlement(uuid, text) FROM PUBLIC;
REVOKE EXECUTE ON FUNCTION public.revoke_entitlement(uuid, text) FROM authenticated;
GRANT  EXECUTE ON FUNCTION public.revoke_entitlement(uuid, text) TO service_role;
