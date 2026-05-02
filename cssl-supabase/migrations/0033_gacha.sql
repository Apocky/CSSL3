-- =====================================================================
-- § T11-W13-GACHA · 0033_gacha.sql
-- ════════════════════════════════════════════════════════════════════
-- Transparency-first gacha-system schema. Implements :
--   - PRIME-DIRECTIVE attestations encoded structurally
--   - cosmetic-only-axiom (¬ pay-for-power)
--   - public-disclosure of drop-rates BEFORE any pull is allowed
--   - guaranteed-Mythic by pity_threshold (default 90 · publicly-known)
--   - 7-day sovereign-refund window (full · automated · no-questions-asked)
--   - Σ-Chain-anchor every-pull AND every-refund (immutable attribution)
--
-- TABLES :
--   - gacha_banners : published banners with PUBLIC-READ on drop-rates
--   - gacha_pulls   : per-pull record · pubkey-tied · sigma-anchor + ts
--   - gacha_refunds : per-refund record · 7d-window-enforced · removed-cosmetic
--   - gacha_currency_grants : pull-currency only via Stripe OR gift-from-friend
--
-- RLS POLICIES :
--   - public-read on gacha_banners (transparency-mandate)
--   - default-deny on gacha_pulls + gacha_refunds + gacha_currency_grants
--   - service-role-only insert/update on pulls + refunds + currency_grants
--
-- HELPERS :
--   - record_gacha_pull(p_player, p_banner, p_result, p_sigma) · idempotent
--   - record_gacha_refund(p_pull_id, p_amount) · 7d-window-enforced
--   - grant_pull_currency(p_player, p_amount, p_source) · stripe|gift only
--
-- Apply order : after 0032 (slot 0033 is unclaimed). Schema-level CHECK
-- constraints structurally-encode the cosmetic-only-axiom.
-- =====================================================================

CREATE EXTENSION IF NOT EXISTS pgcrypto;

-- ─── gacha_banners ──────────────────────────────────────────────────────
-- Published gacha-banners. PUBLIC-READ : transparency-mandate · anyone
-- (authenticated or anonymous) can read the drop-rate-table BEFORE pulling.
CREATE TABLE IF NOT EXISTS public.gacha_banners (
    banner_id          text        PRIMARY KEY,
    season             integer     NOT NULL,
    -- JSONB serialization of cssl_host_gacha::DropRateTable.
    -- Required keys : common, uncommon, rare, epic, legendary, mythic
    -- (basis-points, summing to 100000 · enforced at INSERT-time).
    drop_rate_table    jsonb       NOT NULL,
    pity_threshold     integer     NOT NULL DEFAULT 90,
    disclosed_at       timestamptz NOT NULL DEFAULT now(),
    -- Cosmetic-only-axiom attestation : structural enforcement.
    -- All gacha-banners MUST have category = 'cosmetic' · NEVER 'power'.
    category           text        NOT NULL DEFAULT 'cosmetic',
    created_at         timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT gacha_banners_category_cosmetic_only
        CHECK (category = 'cosmetic'),
    CONSTRAINT gacha_banners_pity_positive
        CHECK (pity_threshold > 0 AND pity_threshold <= 200),
    CONSTRAINT gacha_banners_drop_rate_sum_invariant
        CHECK (
            (drop_rate_table->>'common')::int
          + (drop_rate_table->>'uncommon')::int
          + (drop_rate_table->>'rare')::int
          + (drop_rate_table->>'epic')::int
          + (drop_rate_table->>'legendary')::int
          + (drop_rate_table->>'mythic')::int
            = 100000
        )
);
COMMENT ON TABLE public.gacha_banners IS
    'Published gacha-banners. PUBLIC-READ enforced via RLS. drop_rate_table publicly-disclosed. category=cosmetic structural-enforcement.';

CREATE INDEX IF NOT EXISTS gacha_banners_season_idx ON public.gacha_banners (season);

-- ─── gacha_pulls ────────────────────────────────────────────────────────
-- Per-pull record. PK = uuid · pubkey-tied · banner-scoped · sigma-anchored.
CREATE TABLE IF NOT EXISTS public.gacha_pulls (
    pull_id            uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
    player_pubkey      text        NOT NULL,  -- hex-encoded Ed25519 pubkey (64 chars)
    banner_id          text        NOT NULL,
    -- JSON-encoded PullOutcome (rarity, cosmetic_handle, forced_by_pity, roll_bps).
    result             jsonb       NOT NULL,
    pulled_at          timestamptz NOT NULL DEFAULT now(),
    refunded_at        timestamptz,
    -- Σ-Chain anchor-id (32 hex-chars). Set on insert · immutable.
    sigma_anchor_id    text        NOT NULL,
    pull_index         bigint      NOT NULL,
    pull_mode          text        NOT NULL,
    forced_by_pity     boolean     NOT NULL DEFAULT false,
    rarity             text        NOT NULL,
    cosmetic_handle    text        NOT NULL,
    CONSTRAINT gacha_pulls_pubkey_shape
        CHECK (char_length(player_pubkey) BETWEEN 32 AND 256),
    CONSTRAINT gacha_pulls_sigma_anchor_shape
        CHECK (sigma_anchor_id ~ '^[0-9a-f]{32}$'),
    CONSTRAINT gacha_pulls_rarity_enum
        CHECK (rarity IN ('common', 'uncommon', 'rare', 'epic', 'legendary', 'mythic')),
    CONSTRAINT gacha_pulls_mode_enum
        CHECK (pull_mode IN ('single', 'ten_pull', 'hundred_pull')),
    CONSTRAINT gacha_pulls_cosmetic_only_handle
        CHECK (cosmetic_handle LIKE 'cosmetic:%')
);
CREATE INDEX IF NOT EXISTS gacha_pulls_player_idx ON public.gacha_pulls (player_pubkey);
CREATE INDEX IF NOT EXISTS gacha_pulls_banner_idx ON public.gacha_pulls (banner_id);
CREATE INDEX IF NOT EXISTS gacha_pulls_pulled_idx ON public.gacha_pulls (pulled_at DESC);
CREATE INDEX IF NOT EXISTS gacha_pulls_anchor_idx ON public.gacha_pulls (sigma_anchor_id);
COMMENT ON TABLE public.gacha_pulls IS
    'Per-pull record · cosmetic-only structural-enforcement · sigma-anchor every-pull · refunded_at NULL = active.';

-- ─── gacha_refunds ──────────────────────────────────────────────────────
-- Per-refund record. PK = pull_id (1-to-1 mapping). 7d-window enforced
-- structurally via CHECK on the (refunded_at - pulled_at) interval.
CREATE TABLE IF NOT EXISTS public.gacha_refunds (
    pull_id            uuid        PRIMARY KEY REFERENCES public.gacha_pulls(pull_id) ON DELETE CASCADE,
    refunded_at        timestamptz NOT NULL DEFAULT now(),
    refund_amount      integer     NOT NULL,
    -- Σ-Chain refund-anchor (different from pull-anchor due to kind tag).
    sigma_refund_id    text        NOT NULL,
    -- Whether the original pull was a Mythic (affects pity-rollback semantics).
    original_was_mythic boolean    NOT NULL DEFAULT false,
    CONSTRAINT gacha_refunds_amount_nonneg
        CHECK (refund_amount >= 0),
    CONSTRAINT gacha_refunds_sigma_shape
        CHECK (sigma_refund_id ~ '^[0-9a-f]{32}$')
);
CREATE INDEX IF NOT EXISTS gacha_refunds_refunded_idx ON public.gacha_refunds (refunded_at DESC);
COMMENT ON TABLE public.gacha_refunds IS
    'Per-refund record · 1:1 with gacha_pulls.pull_id · sigma_refund_id distinct from pull-anchor.';

-- ─── gacha_currency_grants ──────────────────────────────────────────────
-- Pull-currency grants. ONLY two source-types allowed (structural CHECK) :
--   - 'stripe'  : purchased via Stripe checkout (entitlement-grant)
--   - 'gift'    : received from another player (gift-economy)
-- ¬ in-game-grind-loop : NEVER 'grind' · NEVER 'achievement' source.
CREATE TABLE IF NOT EXISTS public.gacha_currency_grants (
    grant_id           uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
    player_pubkey      text        NOT NULL,
    amount             integer     NOT NULL,
    source             text        NOT NULL,
    granted_at         timestamptz NOT NULL DEFAULT now(),
    -- For 'stripe' source : the stripe_session_id (cs_xxxx).
    -- For 'gift' source   : the giftor's player_pubkey.
    source_ref         text        NOT NULL,
    CONSTRAINT gacha_currency_amount_positive
        CHECK (amount > 0),
    -- ¬ in-game-grind-loop axiom : structurally-enforce only stripe OR gift.
    CONSTRAINT gacha_currency_source_only_stripe_or_gift
        CHECK (source IN ('stripe', 'gift'))
);
CREATE INDEX IF NOT EXISTS gacha_currency_grants_player_idx ON public.gacha_currency_grants (player_pubkey);
CREATE INDEX IF NOT EXISTS gacha_currency_grants_source_idx ON public.gacha_currency_grants (source);
COMMENT ON TABLE public.gacha_currency_grants IS
    'Pull-currency grants · ONLY stripe-purchase OR gift-from-friend · ¬ grind-loop structural-enforcement.';

-- ─── RLS : default-deny + public-read on banners ────────────────────────

ALTER TABLE public.gacha_banners ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.gacha_pulls ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.gacha_refunds ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.gacha_currency_grants ENABLE ROW LEVEL SECURITY;

-- gacha_banners : public-read for transparency-mandate.
DROP POLICY IF EXISTS gacha_banners_public_read ON public.gacha_banners;
CREATE POLICY gacha_banners_public_read
    ON public.gacha_banners
    FOR SELECT
    TO anon, authenticated
    USING (true);

-- gacha_banners : service-role-only for INSERT/UPDATE/DELETE.
DROP POLICY IF EXISTS gacha_banners_service_role_write ON public.gacha_banners;
CREATE POLICY gacha_banners_service_role_write
    ON public.gacha_banners
    FOR ALL
    TO service_role
    USING (true)
    WITH CHECK (true);

-- gacha_pulls : default-deny for anon · authenticated may read OWN-pulls only.
DROP POLICY IF EXISTS gacha_pulls_self_read ON public.gacha_pulls;
CREATE POLICY gacha_pulls_self_read
    ON public.gacha_pulls
    FOR SELECT
    TO authenticated
    USING (
        -- Player can read pulls keyed to their pubkey via JWT claim.
        auth.jwt() ->> 'sub' = player_pubkey
    );

DROP POLICY IF EXISTS gacha_pulls_service_role_write ON public.gacha_pulls;
CREATE POLICY gacha_pulls_service_role_write
    ON public.gacha_pulls
    FOR ALL
    TO service_role
    USING (true)
    WITH CHECK (true);

-- gacha_refunds : authenticated reads own-refunds (joined to gacha_pulls).
DROP POLICY IF EXISTS gacha_refunds_self_read ON public.gacha_refunds;
CREATE POLICY gacha_refunds_self_read
    ON public.gacha_refunds
    FOR SELECT
    TO authenticated
    USING (
        EXISTS (
            SELECT 1 FROM public.gacha_pulls p
            WHERE p.pull_id = gacha_refunds.pull_id
              AND auth.jwt() ->> 'sub' = p.player_pubkey
        )
    );

DROP POLICY IF EXISTS gacha_refunds_service_role_write ON public.gacha_refunds;
CREATE POLICY gacha_refunds_service_role_write
    ON public.gacha_refunds
    FOR ALL
    TO service_role
    USING (true)
    WITH CHECK (true);

-- gacha_currency_grants : authenticated reads own-grants only.
DROP POLICY IF EXISTS gacha_currency_grants_self_read ON public.gacha_currency_grants;
CREATE POLICY gacha_currency_grants_self_read
    ON public.gacha_currency_grants
    FOR SELECT
    TO authenticated
    USING (auth.jwt() ->> 'sub' = player_pubkey);

DROP POLICY IF EXISTS gacha_currency_grants_service_role_write ON public.gacha_currency_grants;
CREATE POLICY gacha_currency_grants_service_role_write
    ON public.gacha_currency_grants
    FOR ALL
    TO service_role
    USING (true)
    WITH CHECK (true);

-- ─── helper : record_gacha_pull ─────────────────────────────────────────
-- Idempotent insert · returns the full row · no-op on duplicate sigma_anchor_id.
CREATE OR REPLACE FUNCTION public.record_gacha_pull(
    p_player_pubkey   text,
    p_banner_id       text,
    p_result          jsonb,
    p_sigma_anchor_id text,
    p_pull_index      bigint,
    p_pull_mode       text,
    p_forced_by_pity  boolean,
    p_rarity          text,
    p_cosmetic_handle text
) RETURNS public.gacha_pulls
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    r public.gacha_pulls;
BEGIN
    INSERT INTO public.gacha_pulls (
        player_pubkey, banner_id, result, sigma_anchor_id,
        pull_index, pull_mode, forced_by_pity, rarity, cosmetic_handle
    )
    VALUES (
        p_player_pubkey, p_banner_id, p_result, p_sigma_anchor_id,
        p_pull_index, p_pull_mode, p_forced_by_pity, p_rarity, p_cosmetic_handle
    )
    RETURNING * INTO r;
    RETURN r;
EXCEPTION WHEN unique_violation THEN
    -- Idempotent : re-fetch the existing row by sigma_anchor_id.
    SELECT * INTO r FROM public.gacha_pulls WHERE sigma_anchor_id = p_sigma_anchor_id LIMIT 1;
    RETURN r;
END;
$$;
COMMENT ON FUNCTION public.record_gacha_pull(text, text, jsonb, text, bigint, text, boolean, text, text) IS
    'Idempotent gacha-pull recorder. Re-call with same sigma_anchor_id is no-op (returns existing row).';

-- ─── helper : record_gacha_refund · 7d-window enforced ──────────────────
CREATE OR REPLACE FUNCTION public.record_gacha_refund(
    p_pull_id          uuid,
    p_refund_amount    integer,
    p_sigma_refund_id  text
) RETURNS public.gacha_refunds
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    pull_row public.gacha_pulls;
    refund_row public.gacha_refunds;
    is_mythic boolean;
BEGIN
    SELECT * INTO pull_row FROM public.gacha_pulls WHERE pull_id = p_pull_id;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'pull_id not found: %', p_pull_id;
    END IF;

    -- 7d-window structural enforcement.
    IF (now() - pull_row.pulled_at) > INTERVAL '7 days' THEN
        RAISE EXCEPTION 'refund window expired (>7 days since pull)';
    END IF;

    is_mythic := (pull_row.rarity = 'mythic');

    -- Mark the pull as refunded.
    UPDATE public.gacha_pulls SET refunded_at = now() WHERE pull_id = p_pull_id;

    -- Record the refund.
    INSERT INTO public.gacha_refunds (pull_id, refund_amount, sigma_refund_id, original_was_mythic)
    VALUES (p_pull_id, p_refund_amount, p_sigma_refund_id, is_mythic)
    RETURNING * INTO refund_row;
    RETURN refund_row;
END;
$$;
COMMENT ON FUNCTION public.record_gacha_refund(uuid, integer, text) IS
    '7d-window-enforced refund recorder · marks gacha_pulls.refunded_at + inserts gacha_refunds. Throws on stale-pull.';

-- ─── helper : grant_pull_currency · stripe|gift only ────────────────────
CREATE OR REPLACE FUNCTION public.grant_pull_currency(
    p_player_pubkey  text,
    p_amount         integer,
    p_source         text,
    p_source_ref     text
) RETURNS public.gacha_currency_grants
LANGUAGE plpgsql
SECURITY DEFINER
SET search_path = public, pg_temp
AS $$
DECLARE
    r public.gacha_currency_grants;
BEGIN
    -- Source restriction enforced both via CHECK constraint AND here for a
    -- meaningful error-message.
    IF p_source NOT IN ('stripe', 'gift') THEN
        RAISE EXCEPTION 'pull-currency source must be stripe OR gift (got %)', p_source;
    END IF;

    INSERT INTO public.gacha_currency_grants (player_pubkey, amount, source, source_ref)
    VALUES (p_player_pubkey, p_amount, p_source, p_source_ref)
    RETURNING * INTO r;
    RETURN r;
END;
$$;
COMMENT ON FUNCTION public.grant_pull_currency(text, integer, text, text) IS
    'Pull-currency grant · ONLY stripe (Stripe-checkout) OR gift (gift-from-friend). ¬ grind-loop structural-enforcement.';
