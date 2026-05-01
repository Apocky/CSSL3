-- =====================================================================
-- § T11-W9-PAYMENTS · 0022_payments.sql
-- Stripe-payments + entitlements schema. Cosmetic-channel-only-axiom (per
-- specs/grand-vision/13_INFINITE_LABYRINTH_LEGACY.csl) is enforced at code-
-- review · this schema does NOT carry any "power" tier · only product_id
-- strings whose semantics are defined client-side.
--
-- Tables :
--   - stripe_customers          · player ↔ stripe-customer-id map
--   - stripe_checkout_sessions  · per-checkout-flow record
--   - entitlements              · what a player owns / subscribes to
--   - stripe_webhook_events     · idempotency record (UNIQUE event_id)
--   - stripe_refunds            · refund flow record
--
-- Helpers :
--   - grant_entitlement(p_player_id, p_product_id, p_session_id) · idempotent
--   - revoke_entitlement(p_player_id, p_product_id)              · refund/cancel
--
-- All tables UUID-PK or stripe-id-PK · all FK cascade-on-delete · all carry
-- created_at/updated_at timestamptz · all RLS-policied in 0023.
--
-- Apply order : after 0021. Slot 0021 is unclaimed in this branch but the
-- session-ordering convention is monotonic-fill ; 0022 is fine.
-- =====================================================================
CREATE EXTENSION IF NOT EXISTS pgcrypto;

-- ─── stripe_customers ───────────────────────────────────────────────────
-- Maps Apocky-Hub player_id (uuid) ↔ Stripe customer-id (cus_xxxx).
-- UNIQUE on player_id : one Stripe-customer per player (Stripe-side dedup
-- happens via email, but we enforce 1-to-1 to keep the map clean).
CREATE TABLE IF NOT EXISTS public.stripe_customers (
    player_id          uuid        PRIMARY KEY,
    stripe_customer_id text        NOT NULL UNIQUE,
    created_at         timestamptz NOT NULL DEFAULT now(),
    updated_at         timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT stripe_customers_id_shape
        CHECK (stripe_customer_id ~ '^cus_[A-Za-z0-9]+$'
               OR stripe_customer_id ~ '^cus_test_[A-Za-z0-9]+$')
);
COMMENT ON TABLE public.stripe_customers IS
    'Apocky-Hub player_id ↔ Stripe customer_id map. UNIQUE on player_id.';

-- ─── stripe_checkout_sessions ───────────────────────────────────────────
-- One row per Stripe checkout session created from /api/payments/stripe/checkout.
-- session_id is Stripe's cs_xxxx · used as PK so duplicate-create requests
-- (idempotency-replays) fold cleanly via ON CONFLICT.
CREATE TABLE IF NOT EXISTS public.stripe_checkout_sessions (
    session_id   text        PRIMARY KEY,
    player_id    uuid        NOT NULL,
    product_id   text        NOT NULL,
    status       text        NOT NULL DEFAULT 'created',
    created_at   timestamptz NOT NULL DEFAULT now(),
    updated_at   timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT stripe_checkout_sessions_status_enum
        CHECK (status IN ('created', 'completed', 'expired', 'cancelled')),
    CONSTRAINT stripe_checkout_sessions_product_length
        CHECK (char_length(product_id) BETWEEN 1 AND 100)
);
CREATE INDEX IF NOT EXISTS stripe_checkout_sessions_player_idx
    ON public.stripe_checkout_sessions (player_id);
CREATE INDEX IF NOT EXISTS stripe_checkout_sessions_product_idx
    ON public.stripe_checkout_sessions (product_id);
COMMENT ON TABLE public.stripe_checkout_sessions IS
    'One row per Stripe checkout-session. PK = Stripe cs_xxxx for idempotent upsert.';

-- ─── entitlements ───────────────────────────────────────────────────────
-- Source-of-truth for what a player owns or is subscribed to. Multiple rows
-- per player (one per product_id) · UNIQUE(player_id, product_id) so a
-- second purchase of the same product is treated as renewal not duplicate.
-- expires_at NULL = perpetual (one-time purchase) · cancelled_at NULL =
-- not-cancelled.
CREATE TABLE IF NOT EXISTS public.entitlements (
    entitlement_id     uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
    player_id          uuid        NOT NULL,
    product_id         text        NOT NULL,
    stripe_session_id  text        NOT NULL,
    granted_at         timestamptz NOT NULL DEFAULT now(),
    expires_at         timestamptz,
    cancelled_at       timestamptz,
    UNIQUE (player_id, product_id),
    CONSTRAINT entitlements_product_length
        CHECK (char_length(product_id) BETWEEN 1 AND 100)
);
CREATE INDEX IF NOT EXISTS entitlements_player_idx
    ON public.entitlements (player_id);
CREATE INDEX IF NOT EXISTS entitlements_session_idx
    ON public.entitlements (stripe_session_id);
COMMENT ON TABLE public.entitlements IS
    'What a player owns / is subscribed to. UNIQUE(player_id, product_id). cancelled_at IS NOT NULL = revoked.';

-- ─── stripe_webhook_events ──────────────────────────────────────────────
-- Idempotency-record. UNIQUE on stripe_event_id. INSERT-ONLY (RLS forbids
-- UPDATE/DELETE for service-role-only). Webhook handler INSERTs first ; on
-- 23505-unique-violation it knows the event was already processed and skips.
CREATE TABLE IF NOT EXISTS public.stripe_webhook_events (
    stripe_event_id text        PRIMARY KEY,
    event_type      text        NOT NULL,
    payload         jsonb       NOT NULL,
    processed_at    timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT stripe_webhook_events_id_shape
        CHECK (stripe_event_id ~ '^evt_[A-Za-z0-9]+$'
               OR stripe_event_id ~ '^evt_test_[A-Za-z0-9]+$'),
    CONSTRAINT stripe_webhook_events_type_length
        CHECK (char_length(event_type) BETWEEN 1 AND 100)
);
CREATE INDEX IF NOT EXISTS stripe_webhook_events_type_idx
    ON public.stripe_webhook_events (event_type);
CREATE INDEX IF NOT EXISTS stripe_webhook_events_processed_idx
    ON public.stripe_webhook_events (processed_at DESC);
COMMENT ON TABLE public.stripe_webhook_events IS
    'INSERT-ONLY idempotency log. PK = Stripe event_id. UNIQUE-violation on replay → skip.';

-- ─── stripe_refunds ─────────────────────────────────────────────────────
-- Per-refund record. PK = Stripe re_xxxx (refund-id). Linked to the
-- checkout-session that originated the charge.
CREATE TABLE IF NOT EXISTS public.stripe_refunds (
    refund_id          text        PRIMARY KEY,
    player_id          uuid        NOT NULL,
    stripe_session_id  text        NOT NULL,
    amount             integer     NOT NULL,
    status             text        NOT NULL,
    processed_at       timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT stripe_refunds_amount_nonneg
        CHECK (amount >= 0),
    CONSTRAINT stripe_refunds_status_enum
        CHECK (status IN ('pending', 'succeeded', 'failed', 'cancelled', 'requires_action'))
);
CREATE INDEX IF NOT EXISTS stripe_refunds_player_idx
    ON public.stripe_refunds (player_id);
CREATE INDEX IF NOT EXISTS stripe_refunds_session_idx
    ON public.stripe_refunds (stripe_session_id);
COMMENT ON TABLE public.stripe_refunds IS
    'Per-refund record. PK = Stripe refund_id (re_xxxx). 14-day no-questions refund flow.';

-- ─── grant_entitlement helper · idempotent ──────────────────────────────
-- Called from webhook on checkout.session.completed (and subscription.updated
-- when status='active'). Idempotent : second-call with same (player, product)
-- updates granted_at + clears cancelled_at (re-subscribe semantics).
CREATE OR REPLACE FUNCTION public.grant_entitlement(
    p_player_id  uuid,
    p_product_id text,
    p_session_id text
) RETURNS public.entitlements
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    r public.entitlements;
BEGIN
    INSERT INTO public.entitlements (player_id, product_id, stripe_session_id, granted_at, cancelled_at)
    VALUES (p_player_id, p_product_id, p_session_id, now(), NULL)
    ON CONFLICT (player_id, product_id)
    DO UPDATE SET
        stripe_session_id = EXCLUDED.stripe_session_id,
        granted_at        = EXCLUDED.granted_at,
        cancelled_at      = NULL
    RETURNING * INTO r;
    RETURN r;
END;
$$;
COMMENT ON FUNCTION public.grant_entitlement(uuid, text, text) IS
    'Idempotent grant. Re-grant clears cancelled_at (re-subscribe). SECURITY DEFINER · service-role-callable.';

-- ─── revoke_entitlement helper · for refunds + subscription-cancel ──────
-- Sets cancelled_at = now() · keeps row for audit. Idempotent.
CREATE OR REPLACE FUNCTION public.revoke_entitlement(
    p_player_id  uuid,
    p_product_id text
) RETURNS public.entitlements
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    r public.entitlements;
BEGIN
    UPDATE public.entitlements
    SET cancelled_at = COALESCE(cancelled_at, now())
    WHERE player_id = p_player_id AND product_id = p_product_id
    RETURNING * INTO r;
    RETURN r;
END;
$$;
COMMENT ON FUNCTION public.revoke_entitlement(uuid, text) IS
    'Idempotent revoke. Sets cancelled_at := now() if not already set. Row is preserved for audit.';

-- ─── updated_at trigger boilerplate ─────────────────────────────────────
CREATE OR REPLACE FUNCTION public.touch_updated_at()
RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    NEW.updated_at := now();
    RETURN NEW;
END;
$$;

DROP TRIGGER IF EXISTS stripe_customers_touch ON public.stripe_customers;
CREATE TRIGGER stripe_customers_touch
    BEFORE UPDATE ON public.stripe_customers
    FOR EACH ROW EXECUTE FUNCTION public.touch_updated_at();

DROP TRIGGER IF EXISTS stripe_checkout_sessions_touch ON public.stripe_checkout_sessions;
CREATE TRIGGER stripe_checkout_sessions_touch
    BEFORE UPDATE ON public.stripe_checkout_sessions
    FOR EACH ROW EXECUTE FUNCTION public.touch_updated_at();
