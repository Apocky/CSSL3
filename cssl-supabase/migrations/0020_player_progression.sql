-- =====================================================================
-- § T11-W7-RD-D4-MIGRATIONS · 0020_player_progression.sql
-- Cross-run player progression : echoes · classes · perks · skill XP.
-- Ref : GDDs/ROGUELIKE_LOOP.csl § META-PROGRESSION + GDDs/GEAR_RARITY_SYSTEM.csl
-- § CRAFT-SKILL-LADDER. § PRIME_DIRECTIVE : sovereignty preserved · player
-- owns their progression · DELETE = service-role only · 1M-XP-cap clamp ·
-- audit-emit-compatible. Apply after 0019.
-- =====================================================================
CREATE EXTENSION IF NOT EXISTS pgcrypto;

-- § player_progression · per-player meta-progression rollup (UNIQUE on player_id)
CREATE TABLE IF NOT EXISTS public.player_progression (
    progression_id     uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
    player_id          uuid        NOT NULL UNIQUE,
    total_echoes       bigint      NOT NULL DEFAULT 0,
    total_runs         integer     NOT NULL DEFAULT 0,
    classes_unlocked   smallint[]  NOT NULL DEFAULT ARRAY[]::smallint[],
    perks_unlocked     smallint[]  NOT NULL DEFAULT ARRAY[]::smallint[],
    craft_skill        smallint    NOT NULL DEFAULT 0,
    alchemy_skill      smallint    NOT NULL DEFAULT 0,
    magic_skill        smallint    NOT NULL DEFAULT 0,
    last_session_at    timestamptz NOT NULL DEFAULT now(),
    created_at         timestamptz NOT NULL DEFAULT now(),
    last_modified      timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT player_progression_echoes_range
        CHECK (total_echoes >= 0 AND total_echoes <= 1000000000),
    CONSTRAINT player_progression_runs_range
        CHECK (total_runs >= 0 AND total_runs <= 1000000),
    CONSTRAINT player_progression_craft_range
        CHECK (craft_skill   BETWEEN 0 AND 999),
    CONSTRAINT player_progression_alchemy_range
        CHECK (alchemy_skill BETWEEN 0 AND 999),
    CONSTRAINT player_progression_magic_range
        CHECK (magic_skill   BETWEEN 0 AND 999)
);
CREATE INDEX IF NOT EXISTS player_progression_player_idx
    ON public.player_progression (player_id);
COMMENT ON TABLE  public.player_progression IS
    'Per-player meta-progression rollup. UNIQUE(player_id). DELETE = service-role only · sovereignty-preserved.';
COMMENT ON COLUMN public.player_progression.total_echoes     IS 'Soft-currency lifetime total · clamped 0..1e9.';
COMMENT ON COLUMN public.player_progression.classes_unlocked IS 'Unlocked-class IDs (smallint[]). Append-only via player_grant.';
COMMENT ON COLUMN public.player_progression.perks_unlocked   IS 'Unlocked-perk IDs (smallint[]). Append-only via player_grant.';

-- § player_class_xp · per-(player,class) XP ledger (UNIQUE pair)
CREATE TABLE IF NOT EXISTS public.player_class_xp (
    class_xp_id     uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
    player_id       uuid        NOT NULL,
    class_id        smallint    NOT NULL,
    xp              bigint      NOT NULL DEFAULT 0,
    last_gain_at    timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT player_class_xp_class_range
        CHECK (class_id BETWEEN 0 AND 1023),
    CONSTRAINT player_class_xp_xp_range
        CHECK (xp >= 0 AND xp <= 1000000),
    UNIQUE (player_id, class_id)
);
CREATE INDEX IF NOT EXISTS player_class_xp_player_idx ON public.player_class_xp (player_id);
CREATE INDEX IF NOT EXISTS player_class_xp_class_idx  ON public.player_class_xp (class_id);
COMMENT ON TABLE  public.player_class_xp IS
    'Per-(player,class) XP ledger. UNIQUE(player_id,class_id). XP cap = 1M (1000000) per class · clamped in helper fns.';

-- § helper · player_grant_echoes — clamped echoes-grant + audit-trail
CREATE OR REPLACE FUNCTION public.player_grant_echoes(p_player_id uuid, p_amount bigint)
RETURNS bigint LANGUAGE plpgsql SECURITY DEFINER AS $$
DECLARE
    v_new_total bigint;
BEGIN
    IF p_player_id IS NULL OR p_amount IS NULL OR p_amount = 0 THEN RETURN 0; END IF;
    -- ensure progression row exists
    INSERT INTO public.player_progression (player_id) VALUES (p_player_id)
        ON CONFLICT (player_id) DO NOTHING;
    -- clamp + apply
    UPDATE public.player_progression
       SET total_echoes  = LEAST(GREATEST(total_echoes + p_amount, 0::bigint), 1000000000::bigint),
           last_modified = now(),
           last_session_at = now()
     WHERE player_id = p_player_id
     RETURNING total_echoes INTO v_new_total;
    RETURN COALESCE(v_new_total, 0);
END;
$$;
COMMENT ON FUNCTION public.player_grant_echoes(uuid, bigint) IS
    'SECURITY DEFINER · grant (or deduct) echoes · clamped 0..1e9. Returns new total. Idempotent ensure-row via ON CONFLICT.';

-- § helper · player_grant_class_xp — clamped per-class XP grant (1M cap)
CREATE OR REPLACE FUNCTION public.player_grant_class_xp(
    p_player_id uuid,
    p_class_id  smallint,
    p_amount    bigint
) RETURNS bigint LANGUAGE plpgsql SECURITY DEFINER AS $$
DECLARE
    v_new_xp bigint;
BEGIN
    IF p_player_id IS NULL OR p_class_id IS NULL OR p_amount IS NULL OR p_amount = 0 THEN RETURN 0; END IF;
    IF p_class_id < 0 OR p_class_id > 1023 THEN RETURN 0; END IF;
    INSERT INTO public.player_class_xp (player_id, class_id, xp)
        VALUES (p_player_id, p_class_id, GREATEST(LEAST(p_amount, 1000000::bigint), 0::bigint))
        ON CONFLICT (player_id, class_id) DO UPDATE
            SET xp           = LEAST(GREATEST(public.player_class_xp.xp + p_amount, 0::bigint), 1000000::bigint),
                last_gain_at = now()
     RETURNING xp INTO v_new_xp;
    RETURN COALESCE(v_new_xp, 0);
END;
$$;
COMMENT ON FUNCTION public.player_grant_class_xp(uuid, smallint, bigint) IS
    'SECURITY DEFINER · grant (or deduct) class-XP · clamped 0..1M. Returns new XP. Upserts player_class_xp row idempotently.';

-- § RLS
ALTER TABLE public.player_progression ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.player_class_xp    ENABLE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS "player_progression_select_self" ON public.player_progression;
DROP POLICY IF EXISTS "player_progression_insert_self" ON public.player_progression;
DROP POLICY IF EXISTS "player_progression_update_self" ON public.player_progression;
DROP POLICY IF EXISTS "player_class_xp_select_self"    ON public.player_class_xp;
DROP POLICY IF EXISTS "player_class_xp_insert_self"    ON public.player_class_xp;
DROP POLICY IF EXISTS "player_class_xp_update_self"    ON public.player_class_xp;

CREATE POLICY "player_progression_select_self"
    ON public.player_progression FOR SELECT
    USING (auth.uid() = player_id OR auth.role() = 'service_role');
CREATE POLICY "player_progression_insert_self"
    ON public.player_progression FOR INSERT
    WITH CHECK ((auth.uid() IS NOT NULL AND auth.uid() = player_id) OR auth.role() = 'service_role');
CREATE POLICY "player_progression_update_self"
    ON public.player_progression FOR UPDATE
    USING      (auth.uid() = player_id OR auth.role() = 'service_role')
    WITH CHECK (auth.uid() = player_id OR auth.role() = 'service_role');
-- N! NO DELETE policy · service-role only

CREATE POLICY "player_class_xp_select_self"
    ON public.player_class_xp FOR SELECT
    USING (auth.uid() = player_id OR auth.role() = 'service_role');
CREATE POLICY "player_class_xp_insert_self"
    ON public.player_class_xp FOR INSERT
    WITH CHECK ((auth.uid() IS NOT NULL AND auth.uid() = player_id) OR auth.role() = 'service_role');
CREATE POLICY "player_class_xp_update_self"
    ON public.player_class_xp FOR UPDATE
    USING      (auth.uid() = player_id OR auth.role() = 'service_role')
    WITH CHECK (auth.uid() = player_id OR auth.role() = 'service_role');

-- § grants
GRANT SELECT, INSERT, UPDATE ON public.player_progression TO authenticated;
GRANT SELECT, INSERT, UPDATE ON public.player_class_xp    TO authenticated;
GRANT ALL                    ON public.player_progression TO service_role;
GRANT ALL                    ON public.player_class_xp    TO service_role;
GRANT EXECUTE ON FUNCTION public.player_grant_echoes(uuid, bigint)            TO authenticated, service_role;
GRANT EXECUTE ON FUNCTION public.player_grant_class_xp(uuid, smallint, bigint) TO authenticated, service_role;
