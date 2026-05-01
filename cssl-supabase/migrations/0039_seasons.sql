-- =====================================================================
-- § T11-W8-E1 · 0039_seasons.sql · seasonal-hard-perma · 90-day-cycle
-- gift-economy-only · cosmetic-channel-only · NO leaderboards · NO pay-for-power
-- Ref : GDDs/ROGUELIKE_LOOP.csl § DEATH-PENALTY · specs/grand-vision/19 § W8-E1
-- Mirrors : cssl-host-roguelike-run::season (Rust serde-stable schema)
-- PRIME : sovereignty preserved · DELETE = service-role · permadeath structural
-- Idempotent : CREATE TABLE IF NOT EXISTS · DROP-then-CREATE policies · safe re-run
-- =====================================================================
CREATE EXTENSION IF NOT EXISTS pgcrypto;

-- ─── season_index · 90-day-cycle catalog ────────────────────────────────
CREATE TABLE IF NOT EXISTS public.season_index (
    season_id     integer     PRIMARY KEY,
    cycle_index   integer     NOT NULL CHECK (cycle_index >= 0),
    started_at    timestamptz NOT NULL,
    ends_at       timestamptz NOT NULL,
    status        text        NOT NULL DEFAULT 'upcoming'
        CHECK (status IN ('upcoming','active','ended')),
    created_at    timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT season_index_window CHECK (ends_at > started_at),
    CONSTRAINT season_index_id_nonneg CHECK (season_id >= 0)
);
CREATE INDEX IF NOT EXISTS season_index_status_idx ON public.season_index (status);
CREATE INDEX IF NOT EXISTS season_index_active_idx
    ON public.season_index (started_at) WHERE status = 'active';
COMMENT ON TABLE public.season_index IS
    '90-day-cycle catalog. SELECT public · INSERT/UPDATE service-role-only.';

-- ─── season_characters · per-season character records ───────────────────
CREATE TABLE IF NOT EXISTS public.season_characters (
    id                    uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
    season_id             integer     NOT NULL REFERENCES public.season_index(season_id) ON DELETE RESTRICT,
    player_id             uuid        NOT NULL,
    player_pubkey         bytea       NOT NULL,
    mode                  text        NOT NULL DEFAULT 'soft' CHECK (mode IN ('soft','hard')),
    alive                 boolean     NOT NULL DEFAULT true,
    cause_of_death        text        NULL,
    coherence_score_final real        NULL,
    biography             text        NULL,
    created_at            timestamptz NOT NULL DEFAULT now(),
    died_at               timestamptz NULL,
    CONSTRAINT season_chars_pubkey_len CHECK (octet_length(player_pubkey) BETWEEN 16 AND 256),
    CONSTRAINT season_chars_cause_len CHECK (cause_of_death IS NULL OR char_length(cause_of_death) BETWEEN 1 AND 64),
    CONSTRAINT season_chars_coherence_range CHECK (coherence_score_final IS NULL OR (coherence_score_final >= 0.0 AND coherence_score_final <= 1.0)),
    CONSTRAINT season_chars_bio_len CHECK (biography IS NULL OR char_length(biography) BETWEEN 1 AND 8192),
    CONSTRAINT season_chars_died_after_created CHECK (died_at IS NULL OR died_at >= created_at),
    CONSTRAINT season_chars_dead_has_cause CHECK (alive = true OR cause_of_death IS NOT NULL)
);
CREATE INDEX IF NOT EXISTS season_chars_player_idx ON public.season_characters (player_id);
CREATE INDEX IF NOT EXISTS season_chars_season_idx ON public.season_characters (season_id);
CREATE INDEX IF NOT EXISTS season_chars_alive_idx ON public.season_characters (player_id) WHERE alive = true;
CREATE INDEX IF NOT EXISTS season_chars_fallen_idx ON public.season_characters (season_id, died_at DESC) WHERE alive = false;
COMMENT ON TABLE public.season_characters IS
    'Per-season characters. mode=hard → permadeath. SELECT owner-or-fallen-public · INSERT/UPDATE owner-while-alive · DELETE service-role-only.';
COMMENT ON COLUMN public.season_characters.player_pubkey IS
    'Σ-Chain Ed25519 pubkey (32-byte). Distinct from auth.uid (player_id).';

-- ─── season_memorials · gift-economy memorial-imprint ledger ────────────
CREATE TABLE IF NOT EXISTS public.season_memorials (
    id                 uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
    character_id       uuid        NOT NULL UNIQUE REFERENCES public.season_characters(id) ON DELETE RESTRICT,
    imprint_blake3     bytea       NOT NULL CHECK (octet_length(imprint_blake3) = 32),
    imprinted_at       timestamptz NOT NULL DEFAULT now(),
    attribution_pubkey bytea       NULL CHECK (attribution_pubkey IS NULL OR octet_length(attribution_pubkey) BETWEEN 16 AND 256)
);
CREATE INDEX IF NOT EXISTS season_memorials_imprinted_idx ON public.season_memorials (imprinted_at DESC);
COMMENT ON TABLE public.season_memorials IS
    'Gift-economy memorial-imprint ledger. NO leaderboards · cosmetic-channel-only · UNIQUE per character_id.';

-- ─── helpers ────────────────────────────────────────────────────────────
CREATE OR REPLACE FUNCTION public.current_season()
RETURNS integer LANGUAGE plpgsql STABLE AS $$
DECLARE v_id integer;
BEGIN
    SELECT season_id INTO v_id FROM public.season_index
        WHERE status = 'active' ORDER BY started_at DESC LIMIT 1;
    RETURN v_id;
END;
$$;

CREATE OR REPLACE FUNCTION public.start_season(p_season_id integer, p_cycle_index integer, p_started_at timestamptz, p_ends_at timestamptz)
RETURNS integer LANGUAGE plpgsql SECURITY DEFINER AS $$
DECLARE v_role text;
BEGIN
    v_role := COALESCE(current_setting('request.jwt.claim.role', true), '');
    IF v_role <> 'service_role' THEN RAISE EXCEPTION 'start_season requires service_role'; END IF;
    IF p_ends_at <= p_started_at THEN RAISE EXCEPTION 'ends_at must be after started_at'; END IF;
    INSERT INTO public.season_index (season_id, cycle_index, started_at, ends_at, status)
        VALUES (p_season_id, p_cycle_index, p_started_at, p_ends_at, 'active')
        ON CONFLICT (season_id) DO UPDATE
            SET cycle_index = EXCLUDED.cycle_index, started_at = EXCLUDED.started_at,
                ends_at = EXCLUDED.ends_at, status = 'active';
    RETURN p_season_id;
END;
$$;

CREATE OR REPLACE FUNCTION public.end_season(p_season_id integer)
RETURNS integer LANGUAGE plpgsql SECURITY DEFINER AS $$
DECLARE v_role text;
BEGIN
    v_role := COALESCE(current_setting('request.jwt.claim.role', true), '');
    IF v_role <> 'service_role' THEN RAISE EXCEPTION 'end_season requires service_role'; END IF;
    UPDATE public.season_index SET status = 'ended' WHERE season_id = p_season_id;
    RETURN p_season_id;
END;
$$;

CREATE OR REPLACE FUNCTION public.imprint_memorial(p_character_id uuid, p_blake3 bytea, p_attribution bytea)
RETURNS uuid LANGUAGE plpgsql SECURITY DEFINER AS $$
DECLARE v_role text; v_id uuid;
BEGIN
    v_role := COALESCE(current_setting('request.jwt.claim.role', true), '');
    IF v_role <> 'service_role' THEN RAISE EXCEPTION 'imprint_memorial requires service_role'; END IF;
    IF p_character_id IS NULL OR p_blake3 IS NULL THEN RAISE EXCEPTION 'character_id and blake3 required'; END IF;
    IF octet_length(p_blake3) <> 32 THEN RAISE EXCEPTION 'blake3 must be 32 bytes'; END IF;
    INSERT INTO public.season_memorials (character_id, imprint_blake3, attribution_pubkey)
        VALUES (p_character_id, p_blake3, p_attribution)
        ON CONFLICT (character_id) DO UPDATE
            SET imprint_blake3 = EXCLUDED.imprint_blake3,
                attribution_pubkey = EXCLUDED.attribution_pubkey,
                imprinted_at = now()
        RETURNING id INTO v_id;
    RETURN v_id;
END;
$$;

-- ─── RLS ────────────────────────────────────────────────────────────────
ALTER TABLE public.season_index      ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.season_characters ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.season_memorials  ENABLE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS "season_index_select_public"           ON public.season_index;
DROP POLICY IF EXISTS "season_index_modify_service"          ON public.season_index;
DROP POLICY IF EXISTS "season_chars_select_owner_or_fallen"  ON public.season_characters;
DROP POLICY IF EXISTS "season_chars_insert_owner"            ON public.season_characters;
DROP POLICY IF EXISTS "season_chars_update_owner_alive"      ON public.season_characters;
DROP POLICY IF EXISTS "season_memorials_select_public"       ON public.season_memorials;
DROP POLICY IF EXISTS "season_memorials_insert_service"      ON public.season_memorials;

CREATE POLICY "season_index_select_public" ON public.season_index FOR SELECT USING (true);
CREATE POLICY "season_index_modify_service" ON public.season_index FOR ALL
    USING (auth.role() = 'service_role') WITH CHECK (auth.role() = 'service_role');

CREATE POLICY "season_chars_select_owner_or_fallen" ON public.season_characters FOR SELECT
    USING (auth.uid() = player_id OR alive = false OR auth.role() = 'service_role');
CREATE POLICY "season_chars_insert_owner" ON public.season_characters FOR INSERT
    WITH CHECK ((auth.uid() IS NOT NULL AND auth.uid() = player_id) OR auth.role() = 'service_role');
CREATE POLICY "season_chars_update_owner_alive" ON public.season_characters FOR UPDATE
    USING ((auth.uid() = player_id AND alive = true) OR auth.role() = 'service_role')
    WITH CHECK (auth.uid() = player_id OR auth.role() = 'service_role');
-- N! NO DELETE policy · service-role only

CREATE POLICY "season_memorials_select_public" ON public.season_memorials FOR SELECT USING (true);
CREATE POLICY "season_memorials_insert_service" ON public.season_memorials FOR INSERT
    WITH CHECK (auth.role() = 'service_role');
-- N! NO UPDATE policy · use imprint_memorial helper · N! NO DELETE policy · service-role only

-- ─── grants ─────────────────────────────────────────────────────────────
GRANT SELECT                 ON public.season_index      TO authenticated, anon;
GRANT SELECT, INSERT, UPDATE ON public.season_characters TO authenticated;
GRANT SELECT                 ON public.season_memorials  TO authenticated, anon;
GRANT ALL                    ON public.season_index      TO service_role;
GRANT ALL                    ON public.season_characters TO service_role;
GRANT ALL                    ON public.season_memorials  TO service_role;
GRANT EXECUTE ON FUNCTION public.current_season()                                         TO authenticated, anon, service_role;
GRANT EXECUTE ON FUNCTION public.start_season(integer, integer, timestamptz, timestamptz) TO service_role;
GRANT EXECUTE ON FUNCTION public.end_season(integer)                                      TO service_role;
GRANT EXECUTE ON FUNCTION public.imprint_memorial(uuid, bytea, bytea)                     TO service_role;
