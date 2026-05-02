-- =====================================================================
-- § T11-W13-9 · 0032_battle_pass.sql · seasonal-pass progression schema
-- ¬ pay-for-power · cosmetic-channel-only · ¬ FOMO · sovereign-revocable
-- Ref : Labyrinth of Apocalypse/systems/battle_pass.csl
--       compiler-rs/crates/cssl-host-battle-pass (Rust serde-stable mirror)
-- PRIME : Free track ALWAYS-included · 14d-pro-rated-refund · default-DENY-fed
-- Idempotent : CREATE TABLE IF NOT EXISTS · DROP-then-CREATE policies · safe re-run
-- =====================================================================
CREATE EXTENSION IF NOT EXISTS pgcrypto;

-- ─── battle_pass_seasons · 60-90d cycle catalog ─────────────────────────
CREATE TABLE IF NOT EXISTS public.battle_pass_seasons (
    season_id   integer     PRIMARY KEY CHECK (season_id >= 0),
    start_at    timestamptz NOT NULL,
    end_at      timestamptz NOT NULL,
    tier_count  integer     NOT NULL DEFAULT 100 CHECK (tier_count = 100),
    status      text        NOT NULL DEFAULT 'upcoming'
        CHECK (status IN ('upcoming','active','archived')),
    created_at  timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT bp_season_window CHECK (end_at > start_at),
    -- 60-90d duration window per spec § seasonal-cycle.
    CONSTRAINT bp_season_duration_60_to_90d CHECK (
        EXTRACT(EPOCH FROM (end_at - start_at)) BETWEEN 60*86400 AND 90*86400
    )
);
CREATE INDEX IF NOT EXISTS bp_seasons_status_idx ON public.battle_pass_seasons (status);
CREATE INDEX IF NOT EXISTS bp_seasons_active_idx
    ON public.battle_pass_seasons (start_at) WHERE status = 'active';
COMMENT ON TABLE public.battle_pass_seasons IS
    '60-90d battle-pass cycle catalog. SELECT public · INSERT/UPDATE service-role-only.';

-- ─── battle_pass_progression · per-player per-season state ──────────────
CREATE TABLE IF NOT EXISTS public.battle_pass_progression (
    id              uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
    player_id       uuid        NOT NULL,
    season_id       integer     NOT NULL REFERENCES public.battle_pass_seasons(season_id) ON DELETE RESTRICT,
    cumulative_xp   bigint      NOT NULL DEFAULT 0 CHECK (cumulative_xp >= 0),
    tier            integer     NOT NULL DEFAULT 1 CHECK (tier BETWEEN 1 AND 100),
    is_premium      boolean     NOT NULL DEFAULT false,
    paused_at       timestamptz NULL,
    created_at      timestamptz NOT NULL DEFAULT now(),
    updated_at      timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT bp_prog_unique_per_player_season UNIQUE (player_id, season_id)
);
CREATE INDEX IF NOT EXISTS bp_prog_player_idx ON public.battle_pass_progression (player_id);
CREATE INDEX IF NOT EXISTS bp_prog_season_idx ON public.battle_pass_progression (season_id);
CREATE INDEX IF NOT EXISTS bp_prog_premium_idx
    ON public.battle_pass_progression (season_id) WHERE is_premium = true;
CREATE INDEX IF NOT EXISTS bp_prog_paused_idx
    ON public.battle_pass_progression (player_id) WHERE paused_at IS NOT NULL;
COMMENT ON TABLE public.battle_pass_progression IS
    'Per-player per-season pass-progression. Free track ALWAYS available · is_premium=true after Stripe unlock. Paused rows do not accumulate XP. SELECT/UPDATE owner-only.';

-- ─── battle_pass_rewards · per-(season,tier,track) cosmetic catalog ─────
CREATE TABLE IF NOT EXISTS public.battle_pass_rewards (
    id                          uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
    season_id                   integer     NOT NULL REFERENCES public.battle_pass_seasons(season_id) ON DELETE RESTRICT,
    tier                        integer     NOT NULL CHECK (tier BETWEEN 1 AND 100),
    track                       text        NOT NULL CHECK (track IN ('free','premium')),
    cosmetic_id                 text        NOT NULL CHECK (char_length(cosmetic_id) BETWEEN 1 AND 128),
    kind                        text        NOT NULL CHECK (kind IN (
        'skin','weapon_skin','pet_cosmetic','home_decor','emote',
        'mycelial_bloom','memorial_aura','echo_shard_gift_pouch'
    )),
    -- Anti-FOMO : when set, the reward is re-purchasable at gift-cost AFTER this timestamp.
    re_purchasable_after        timestamptz NULL,
    -- gift-cost only ; NULL while season is active (purchases gated to in-season XP-progression).
    gift_cost_echo_shards       integer     NULL CHECK (gift_cost_echo_shards IS NULL OR gift_cost_echo_shards >= 0),
    created_at                  timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT bp_reward_unique_per_slot UNIQUE (season_id, tier, track)
);
CREATE INDEX IF NOT EXISTS bp_rewards_season_idx ON public.battle_pass_rewards (season_id);
CREATE INDEX IF NOT EXISTS bp_rewards_track_idx ON public.battle_pass_rewards (track);
CREATE INDEX IF NOT EXISTS bp_rewards_re_purchasable_idx
    ON public.battle_pass_rewards (re_purchasable_after) WHERE re_purchasable_after IS NOT NULL;
COMMENT ON TABLE public.battle_pass_rewards IS
    'Per-tier cosmetic-only rewards. ¬ stat-affixes · ¬ XP-boost. Anti-FOMO : re_purchasable_after gate set on season-archive ; rewards available at gift-cost post-season.';

-- ─── battle_pass_redemptions · audit-trail of redeemed rewards ──────────
CREATE TABLE IF NOT EXISTS public.battle_pass_redemptions (
    id              uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
    player_id       uuid        NOT NULL,
    season_id       integer     NOT NULL REFERENCES public.battle_pass_seasons(season_id) ON DELETE RESTRICT,
    tier            integer     NOT NULL CHECK (tier BETWEEN 1 AND 100),
    track           text        NOT NULL CHECK (track IN ('free','premium')),
    cosmetic_id     text        NOT NULL CHECK (char_length(cosmetic_id) BETWEEN 1 AND 128),
    redeemed_at     timestamptz NOT NULL DEFAULT now(),
    -- Anti-double-claim guard mirrored from progression.redeemed_tiers BTreeMap.
    CONSTRAINT bp_redemption_unique UNIQUE (player_id, season_id, tier)
);
CREATE INDEX IF NOT EXISTS bp_redemptions_player_idx ON public.battle_pass_redemptions (player_id);
CREATE INDEX IF NOT EXISTS bp_redemptions_season_idx ON public.battle_pass_redemptions (season_id);
COMMENT ON TABLE public.battle_pass_redemptions IS
    'Audit-trail of redeemed pass-rewards. UNIQUE per (player, season, tier) — anti-double-claim.';

-- ─── helpers ────────────────────────────────────────────────────────────
CREATE OR REPLACE FUNCTION public.bp_archive_season(p_season_id integer, p_re_purchasable_after timestamptz)
RETURNS integer LANGUAGE plpgsql SECURITY DEFINER AS $$
DECLARE v_role text;
BEGIN
    v_role := COALESCE(current_setting('request.jwt.claim.role', true), '');
    IF v_role <> 'service_role' THEN RAISE EXCEPTION 'bp_archive_season requires service_role'; END IF;
    UPDATE public.battle_pass_seasons SET status = 'archived' WHERE season_id = p_season_id;
    UPDATE public.battle_pass_rewards
        SET re_purchasable_after = p_re_purchasable_after
        WHERE season_id = p_season_id AND re_purchasable_after IS NULL;
    RETURN p_season_id;
END;
$$;

CREATE OR REPLACE FUNCTION public.bp_pause_progression(p_season_id integer)
RETURNS uuid LANGUAGE plpgsql AS $$
DECLARE v_id uuid;
BEGIN
    UPDATE public.battle_pass_progression
        SET paused_at = now(), updated_at = now()
        WHERE player_id = auth.uid() AND season_id = p_season_id
        RETURNING id INTO v_id;
    RETURN v_id;
END;
$$;

CREATE OR REPLACE FUNCTION public.bp_resume_progression(p_season_id integer)
RETURNS uuid LANGUAGE plpgsql AS $$
DECLARE v_id uuid;
BEGIN
    UPDATE public.battle_pass_progression
        SET paused_at = NULL, updated_at = now()
        WHERE player_id = auth.uid() AND season_id = p_season_id
        RETURNING id INTO v_id;
    RETURN v_id;
END;
$$;

-- ─── RLS ────────────────────────────────────────────────────────────────
ALTER TABLE public.battle_pass_seasons       ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.battle_pass_progression   ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.battle_pass_rewards       ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.battle_pass_redemptions   ENABLE ROW LEVEL SECURITY;

DROP POLICY IF EXISTS "bp_seasons_select_public"           ON public.battle_pass_seasons;
DROP POLICY IF EXISTS "bp_seasons_modify_service"          ON public.battle_pass_seasons;
DROP POLICY IF EXISTS "bp_prog_select_owner"               ON public.battle_pass_progression;
DROP POLICY IF EXISTS "bp_prog_insert_owner"               ON public.battle_pass_progression;
DROP POLICY IF EXISTS "bp_prog_update_owner"               ON public.battle_pass_progression;
DROP POLICY IF EXISTS "bp_rewards_select_public"           ON public.battle_pass_rewards;
DROP POLICY IF EXISTS "bp_rewards_modify_service"          ON public.battle_pass_rewards;
DROP POLICY IF EXISTS "bp_redemptions_select_owner"        ON public.battle_pass_redemptions;
DROP POLICY IF EXISTS "bp_redemptions_insert_owner"        ON public.battle_pass_redemptions;

-- battle_pass_seasons : public read · service-role write.
CREATE POLICY "bp_seasons_select_public"  ON public.battle_pass_seasons FOR SELECT USING (true);
CREATE POLICY "bp_seasons_modify_service" ON public.battle_pass_seasons FOR ALL
    USING (auth.role() = 'service_role') WITH CHECK (auth.role() = 'service_role');

-- battle_pass_progression : owner-only access (default-DENY for others).
-- Mycelium federation default-DENY by structural absence of public-aggregate policy.
CREATE POLICY "bp_prog_select_owner" ON public.battle_pass_progression FOR SELECT
    USING (auth.uid() = player_id OR auth.role() = 'service_role');
CREATE POLICY "bp_prog_insert_owner" ON public.battle_pass_progression FOR INSERT
    WITH CHECK ((auth.uid() IS NOT NULL AND auth.uid() = player_id) OR auth.role() = 'service_role');
CREATE POLICY "bp_prog_update_owner" ON public.battle_pass_progression FOR UPDATE
    USING (auth.uid() = player_id OR auth.role() = 'service_role')
    WITH CHECK (auth.uid() = player_id OR auth.role() = 'service_role');
-- N! NO DELETE policy · service-role only · sovereign-pause is via paused_at NOT delete.

-- battle_pass_rewards : public catalog · service-role write.
CREATE POLICY "bp_rewards_select_public"  ON public.battle_pass_rewards FOR SELECT USING (true);
CREATE POLICY "bp_rewards_modify_service" ON public.battle_pass_rewards FOR ALL
    USING (auth.role() = 'service_role') WITH CHECK (auth.role() = 'service_role');

-- battle_pass_redemptions : owner-read + owner-insert ; UNIQUE constraint enforces anti-double-claim.
CREATE POLICY "bp_redemptions_select_owner" ON public.battle_pass_redemptions FOR SELECT
    USING (auth.uid() = player_id OR auth.role() = 'service_role');
CREATE POLICY "bp_redemptions_insert_owner" ON public.battle_pass_redemptions FOR INSERT
    WITH CHECK ((auth.uid() IS NOT NULL AND auth.uid() = player_id) OR auth.role() = 'service_role');
-- N! NO UPDATE policy · redemptions are append-only.
-- N! NO DELETE policy · service-role only.

-- ─── grants ─────────────────────────────────────────────────────────────
GRANT SELECT                          ON public.battle_pass_seasons     TO authenticated, anon;
GRANT SELECT, INSERT, UPDATE          ON public.battle_pass_progression TO authenticated;
GRANT SELECT                          ON public.battle_pass_rewards     TO authenticated, anon;
GRANT SELECT, INSERT                  ON public.battle_pass_redemptions TO authenticated;
GRANT ALL                             ON public.battle_pass_seasons     TO service_role;
GRANT ALL                             ON public.battle_pass_progression TO service_role;
GRANT ALL                             ON public.battle_pass_rewards     TO service_role;
GRANT ALL                             ON public.battle_pass_redemptions TO service_role;
GRANT EXECUTE ON FUNCTION public.bp_archive_season(integer, timestamptz)  TO service_role;
GRANT EXECUTE ON FUNCTION public.bp_pause_progression(integer)            TO authenticated;
GRANT EXECUTE ON FUNCTION public.bp_resume_progression(integer)           TO authenticated;
