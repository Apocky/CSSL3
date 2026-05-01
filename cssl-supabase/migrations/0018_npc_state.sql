-- =====================================================================
-- § T11-W7-RD-D4-MIGRATIONS · 0018_npc_state.sql
-- NPC runtime state + economy-state · per-shard.
-- Ref : GDDs/AI_NPC_BEHAVIOR.csl § LOD-TIERS + ARCHETYPES + ECONOMY-MODEL.
-- § PRIME_DIRECTIVE : sovereignty preserved · NPC mutations = host/service-role
-- only (NPCs are world-state, not player-owned) · public-shard SELECT for
-- visibility · audit-emit-compatible. Apply after 0017.
-- =====================================================================
CREATE EXTENSION IF NOT EXISTS pgcrypto;

-- § npc_state · per-NPC runtime state (host owns mutations)
CREATE TABLE IF NOT EXISTS public.npc_state (
    npc_id         uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
    shard_id       uuid        NOT NULL,
    archetype      text        NOT NULL,
    position_xyz   jsonb       NOT NULL DEFAULT '{"x":0,"y":0,"z":0}'::jsonb,
    hp             smallint    NOT NULL DEFAULT 100,
    mp             smallint    NOT NULL DEFAULT 0,
    faction        smallint    NOT NULL DEFAULT 0,
    last_tick_ms   bigint      NOT NULL DEFAULT 0,
    lod_tier       smallint    NOT NULL DEFAULT 3,
    created_at     timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT npc_state_archetype_length
        CHECK (char_length(archetype) BETWEEN 1 AND 100),
    CONSTRAINT npc_state_hp_range
        CHECK (hp BETWEEN 0 AND 32000),
    CONSTRAINT npc_state_mp_range
        CHECK (mp BETWEEN 0 AND 32000),
    CONSTRAINT npc_state_faction_range
        CHECK (faction BETWEEN 0 AND 31),
    CONSTRAINT npc_state_lod_tier_range
        CHECK (lod_tier BETWEEN 0 AND 5),
    CONSTRAINT npc_state_position_object
        CHECK (jsonb_typeof(position_xyz) = 'object')
);
CREATE INDEX IF NOT EXISTS npc_state_shard_idx     ON public.npc_state (shard_id);
CREATE INDEX IF NOT EXISTS npc_state_archetype_idx ON public.npc_state (archetype);
CREATE INDEX IF NOT EXISTS npc_state_faction_idx   ON public.npc_state (faction);
CREATE INDEX IF NOT EXISTS npc_state_shard_lod_idx ON public.npc_state (shard_id, lod_tier);
COMMENT ON TABLE  public.npc_state IS
    'Per-NPC runtime state. INSERT/UPDATE = service_role only (host owns NPC mutations). SELECT = public (any authenticated user can observe shard NPCs).';
COMMENT ON COLUMN public.npc_state.archetype    IS 'Behavior archetype id (e.g. ''merchant'',''guard'',''wildlife'',''cultist'').';
COMMENT ON COLUMN public.npc_state.lod_tier     IS '0=culled · 1=ambient-only · 2=physics-low · 3=physics-full · 4=full+AI · 5=director-attention.';
COMMENT ON COLUMN public.npc_state.faction      IS '0..31 · faction-id for diplomacy/aggro tables.';
COMMENT ON COLUMN public.npc_state.last_tick_ms IS 'Monotonic ms since shard-epoch · last AI tick application.';

-- § npc_economy_state · per-shard market goods · supply/demand/price
CREATE TABLE IF NOT EXISTS public.npc_economy_state (
    market_id     uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
    shard_id      uuid        NOT NULL,
    good_id       smallint    NOT NULL,
    current_price float8      NOT NULL DEFAULT 1.0,
    supply_units  integer     NOT NULL DEFAULT 0,
    demand_units  integer     NOT NULL DEFAULT 0,
    last_update   timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT npc_economy_state_good_id_range
        CHECK (good_id BETWEEN 0 AND 4095),
    CONSTRAINT npc_economy_state_price_range
        CHECK (current_price >= 0 AND current_price <= 1e9),
    CONSTRAINT npc_economy_state_supply_nonneg
        CHECK (supply_units >= 0),
    CONSTRAINT npc_economy_state_demand_nonneg
        CHECK (demand_units >= 0),
    UNIQUE (shard_id, good_id)
);
CREATE INDEX IF NOT EXISTS npc_economy_state_shard_idx   ON public.npc_economy_state (shard_id);
CREATE INDEX IF NOT EXISTS npc_economy_state_good_idx    ON public.npc_economy_state (good_id);
COMMENT ON TABLE  public.npc_economy_state IS
    'Per-shard market state for goods (merchant supply/demand/price). UNIQUE(shard_id,good_id) — one row per market×good. Host-owned.';

-- § helper · npc_apply_lod — distance-driven LOD-tier (clamped 0..5)
CREATE OR REPLACE FUNCTION public.npc_apply_lod(p_npc_id uuid, p_distance_m float8)
RETURNS smallint LANGUAGE plpgsql SECURITY DEFINER AS $$
DECLARE
    v_tier smallint;
BEGIN
    IF p_npc_id IS NULL OR p_distance_m IS NULL OR p_distance_m < 0 THEN
        RETURN 0;
    END IF;
    -- distance-tier mapping (clamped 0..5)
    v_tier := CASE
        WHEN p_distance_m >  500.0 THEN 0
        WHEN p_distance_m >  200.0 THEN 1
        WHEN p_distance_m >   80.0 THEN 2
        WHEN p_distance_m >   30.0 THEN 3
        WHEN p_distance_m >   10.0 THEN 4
        ELSE                            5
    END;
    UPDATE public.npc_state SET lod_tier = v_tier, last_tick_ms = (extract(epoch from now())*1000)::bigint
     WHERE npc_id = p_npc_id;
    RETURN v_tier;
END;
$$;
COMMENT ON FUNCTION public.npc_apply_lod(uuid, float8) IS
    'Distance-driven LOD-tier mutation (0..5 clamp). Returns applied tier. Inputs sanitized (NULL/negative → 0).';

-- § RLS · public-SELECT for shard observation · host-only mutations
ALTER TABLE public.npc_state          ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.npc_economy_state  ENABLE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS "npc_state_select_public"          ON public.npc_state;
DROP POLICY IF EXISTS "npc_state_mutate_service"         ON public.npc_state;
DROP POLICY IF EXISTS "npc_economy_select_public"        ON public.npc_economy_state;
DROP POLICY IF EXISTS "npc_economy_mutate_service"       ON public.npc_economy_state;

CREATE POLICY "npc_state_select_public"
    ON public.npc_state FOR SELECT
    USING (true);
CREATE POLICY "npc_state_mutate_service"
    ON public.npc_state FOR ALL
    USING      (auth.role() = 'service_role')
    WITH CHECK (auth.role() = 'service_role');

CREATE POLICY "npc_economy_select_public"
    ON public.npc_economy_state FOR SELECT
    USING (true);
CREATE POLICY "npc_economy_mutate_service"
    ON public.npc_economy_state FOR ALL
    USING      (auth.role() = 'service_role')
    WITH CHECK (auth.role() = 'service_role');

-- § grants
GRANT SELECT ON public.npc_state         TO authenticated, anon;
GRANT SELECT ON public.npc_economy_state TO authenticated, anon;
GRANT ALL    ON public.npc_state         TO service_role;
GRANT ALL    ON public.npc_economy_state TO service_role;
GRANT EXECUTE ON FUNCTION public.npc_apply_lod(uuid, float8) TO service_role;
