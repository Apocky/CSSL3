-- =====================================================================
-- § T11-W12-SUBSCRIBE · 0029_content_subscriptions.sql
-- Follow-creator + auto-pull-new-content schema.
-- Reuses cssl-hotfix mechanism (channel name 'content.subscribed.realtime').
--
-- Tables :
--   - content_subscriptions          · who follows whom · sovereign-revocable
--   - content_notifications          · per-subscriber feed · 5 kinds · Σ-mask
--   - content_subscription_aggregates · k-anon ≥ 10 trending counts (no PII)
--
-- Helpers :
--   - rebuild_content_subscription_aggregates() · recomputes the
--     aggregate row-set from active subscriptions, dropping rows with
--     subscriber_count < 10 (k-anon enforcement).
--
-- All tables RLS-policied · default-deny-everything · self-row only.
--
-- Apply order : after 0026_hotfix (renumber to 0029 keeps room for 0027/0028
-- siblings : W12-rating + W12-publish-pipeline ; this slice does not depend
-- on those landing first).
-- =====================================================================

CREATE EXTENSION IF NOT EXISTS pgcrypto;

-- ─── content_subscriptions ────────────────────────────────────────────
-- Authoritative subscription record · 1:1 with cssl_content_subscription
-- crate's Subscription struct. id is BLAKE3(pubkey · target · ts_ns) hex.
CREATE TABLE IF NOT EXISTS public.content_subscriptions (
    id                   text        PRIMARY KEY,        -- blake3 hex (64 chars)
    subscriber_pubkey    text        NOT NULL,           -- ed25519 hex (64 chars)
    target_kind          text        NOT NULL,           -- 'creator' | 'tag' | 'content-chain'
    target_id            text        NOT NULL,           -- pubkey-hex / tag / chain-hex
    sigma_mask           uuid        NOT NULL DEFAULT gen_random_uuid(),
    frequency            text        NOT NULL DEFAULT 'realtime',
    created_at_ns        bigint      NOT NULL DEFAULT (EXTRACT(EPOCH FROM now())::bigint * 1000000000),
    revoked_at_ns        bigint      NULL,
    created_at           timestamptz NOT NULL DEFAULT now(),
    updated_at           timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT content_subscriptions_target_kind_enum
        CHECK (target_kind IN ('creator', 'tag', 'content-chain')),
    CONSTRAINT content_subscriptions_frequency_enum
        CHECK (frequency IN ('realtime', 'daily', 'manual')),
    CONSTRAINT content_subscriptions_id_shape
        CHECK (length(id) = 64 AND id ~ '^[0-9a-f]+$'),
    CONSTRAINT content_subscriptions_subscriber_shape
        CHECK (length(subscriber_pubkey) = 64 AND subscriber_pubkey ~ '^[0-9a-f]+$'),
    CONSTRAINT content_subscriptions_target_id_length
        CHECK (char_length(target_id) BETWEEN 1 AND 256),
    CONSTRAINT content_subscriptions_revoke_after_create
        CHECK (revoked_at_ns IS NULL OR revoked_at_ns >= created_at_ns)
);
COMMENT ON TABLE public.content_subscriptions IS
    'Per-subscriber follow-target rows. Sovereign-revocable (revoked_at_ns set).';

CREATE INDEX IF NOT EXISTS content_subscriptions_subscriber_idx
    ON public.content_subscriptions (subscriber_pubkey)
    WHERE revoked_at_ns IS NULL;
CREATE INDEX IF NOT EXISTS content_subscriptions_target_idx
    ON public.content_subscriptions (target_kind, target_id)
    WHERE revoked_at_ns IS NULL;

-- ─── content_notifications ────────────────────────────────────────────
-- Per-subscriber feed row · 1:1 with ContentNotification struct.
-- READ-by-subscriber drives mark-read ; service-write only.
CREATE TABLE IF NOT EXISTS public.content_notifications (
    id                   text        PRIMARY KEY,        -- blake3 hex (64 chars)
    subscription_id      text        NOT NULL REFERENCES public.content_subscriptions(id) ON DELETE CASCADE,
    subscriber_pubkey    text        NOT NULL,
    kind                 text        NOT NULL,
    content_id           text        NOT NULL,           -- 32-byte content-id hex (64 chars)
    reason               text        NULL,               -- ≤ 200 chars · for revoke-* kinds
    sigma_mask           uuid        NOT NULL DEFAULT gen_random_uuid(),
    created_at_ns        bigint      NOT NULL,
    read_at_ns           bigint      NULL,
    created_at           timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT content_notifications_kind_enum
        CHECK (kind IN ('new-published', 'remix-created', 'update-available',
                        'revoked-by-creator', 'revoked-by-moderation')),
    CONSTRAINT content_notifications_id_shape
        CHECK (length(id) = 64 AND id ~ '^[0-9a-f]+$'),
    CONSTRAINT content_notifications_content_id_shape
        CHECK (length(content_id) = 64 AND content_id ~ '^[0-9a-f]+$'),
    CONSTRAINT content_notifications_reason_length
        CHECK (reason IS NULL OR char_length(reason) BETWEEN 1 AND 200),
    CONSTRAINT content_notifications_read_after_create
        CHECK (read_at_ns IS NULL OR read_at_ns >= created_at_ns)
);
COMMENT ON TABLE public.content_notifications IS
    'Per-subscriber notification feed (5 kinds · Σ-mask-gated · no auto-resurface).';

CREATE INDEX IF NOT EXISTS content_notifications_subscriber_unread_idx
    ON public.content_notifications (subscriber_pubkey, created_at_ns DESC)
    WHERE read_at_ns IS NULL;
CREATE INDEX IF NOT EXISTS content_notifications_subscription_idx
    ON public.content_notifications (subscription_id);

-- ─── content_subscription_aggregates ──────────────────────────────────
-- Trending-via-subscription-count · k-anon ≥ 10 · NO individual rows.
CREATE TABLE IF NOT EXISTS public.content_subscription_aggregates (
    target_kind          text        NOT NULL,
    target_id            text        NOT NULL,
    subscriber_count     bigint      NOT NULL,
    sigma_mask           uuid        NOT NULL DEFAULT gen_random_uuid(),
    last_updated_at      timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (target_kind, target_id),
    CONSTRAINT content_subscription_aggregates_kind_enum
        CHECK (target_kind IN ('creator', 'tag', 'content-chain')),
    CONSTRAINT content_subscription_aggregates_k_anon
        CHECK (subscriber_count >= 10)
);
COMMENT ON TABLE public.content_subscription_aggregates IS
    'k-anon ≥ 10 aggregate counts per target. Rows below threshold MUST be deleted.';

-- ─── helper · rebuild_content_subscription_aggregates ─────────────────
CREATE OR REPLACE FUNCTION public.rebuild_content_subscription_aggregates()
RETURNS void AS $$
BEGIN
    -- Drop every row ; rebuild from active subs. k-anon < 10 is filtered by HAVING.
    DELETE FROM public.content_subscription_aggregates;
    INSERT INTO public.content_subscription_aggregates (target_kind, target_id, subscriber_count)
    SELECT target_kind, target_id, COUNT(*) AS subscriber_count
    FROM public.content_subscriptions
    WHERE revoked_at_ns IS NULL
    GROUP BY target_kind, target_id
    HAVING COUNT(*) >= 10;
END;
$$ LANGUAGE plpgsql SECURITY DEFINER;
COMMENT ON FUNCTION public.rebuild_content_subscription_aggregates IS
    'Recompute aggregate row-set from active subscriptions. k-anon ≥ 10 gate enforced.';

-- =====================================================================
-- § Row-Level Security
-- =====================================================================

-- ─── content_subscriptions · self-read · service-write ────────────────
ALTER TABLE public.content_subscriptions ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS "content_subscriptions_self_read" ON public.content_subscriptions;
CREATE POLICY "content_subscriptions_self_read"
    ON public.content_subscriptions FOR SELECT
    USING (auth.uid()::text = subscriber_pubkey OR auth.role() = 'service_role');

DROP POLICY IF EXISTS "content_subscriptions_service_write" ON public.content_subscriptions;
CREATE POLICY "content_subscriptions_service_write"
    ON public.content_subscriptions FOR ALL
    USING (auth.role() = 'service_role')
    WITH CHECK (auth.role() = 'service_role');

-- ─── content_notifications · self-read · service-write ────────────────
ALTER TABLE public.content_notifications ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS "content_notifications_self_read" ON public.content_notifications;
CREATE POLICY "content_notifications_self_read"
    ON public.content_notifications FOR SELECT
    USING (auth.uid()::text = subscriber_pubkey OR auth.role() = 'service_role');

DROP POLICY IF EXISTS "content_notifications_self_mark_read" ON public.content_notifications;
CREATE POLICY "content_notifications_self_mark_read"
    ON public.content_notifications FOR UPDATE
    USING (auth.uid()::text = subscriber_pubkey)
    WITH CHECK (auth.uid()::text = subscriber_pubkey);

DROP POLICY IF EXISTS "content_notifications_service_write" ON public.content_notifications;
CREATE POLICY "content_notifications_service_write"
    ON public.content_notifications FOR ALL
    USING (auth.role() = 'service_role')
    WITH CHECK (auth.role() = 'service_role');

-- ─── content_subscription_aggregates · public-read · service-write ────
-- k-anon ≥ 10 makes rows safe to expose ; aggregates are not PII.
ALTER TABLE public.content_subscription_aggregates ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS "content_subscription_aggregates_public_read" ON public.content_subscription_aggregates;
CREATE POLICY "content_subscription_aggregates_public_read"
    ON public.content_subscription_aggregates FOR SELECT
    USING (true);

DROP POLICY IF EXISTS "content_subscription_aggregates_service_write" ON public.content_subscription_aggregates;
CREATE POLICY "content_subscription_aggregates_service_write"
    ON public.content_subscription_aggregates FOR ALL
    USING (auth.role() = 'service_role')
    WITH CHECK (auth.role() = 'service_role');

-- =====================================================================
-- § ATTESTATION
-- ¬ harm · sovereign-unsubscribe · cascade-revoke-consent-gated
-- ¬ engagement-bait · no-auto-resurface · k-anon-aggregate-≥-10
-- ¬ DRM · ¬ rootkit · rate-limit-default-1-per-min · default-deny RLS
-- =====================================================================
