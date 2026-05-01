-- =====================================================================
-- § T11-W7-RD-D4-MIGRATIONS · 0019_multiplayer_shards.sql
-- Multiplayer hub-city shards + membership tracking.
-- Ref : GDDs/MULTIPLAYER_MATRIX.csl § HUB-CITIES + SHARD-CAPACITY + ROLES.
-- § PRIME_DIRECTIVE : sovereignty preserved · shard membership = player-
-- owned (member can leave unilaterally) · shards are world-state (host-
-- managed) · public-SELECT for shard discovery. Apply after 0018.
-- =====================================================================
CREATE EXTENSION IF NOT EXISTS pgcrypto;

-- § multiplayer_shards · per-hub-city instance
CREATE TABLE IF NOT EXISTS public.multiplayer_shards (
    shard_id            uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
    hub_city            text        NOT NULL,
    capacity            smallint    NOT NULL DEFAULT 256,
    current_population  smallint    NOT NULL DEFAULT 0,
    instance_index      smallint    NOT NULL DEFAULT 0,
    created_at          timestamptz NOT NULL DEFAULT now(),
    last_heartbeat      timestamptz NOT NULL DEFAULT now(),
    status              text        NOT NULL DEFAULT 'active',
    CONSTRAINT multiplayer_shards_hub_city_length
        CHECK (char_length(hub_city) BETWEEN 1 AND 200),
    CONSTRAINT multiplayer_shards_capacity_range
        CHECK (capacity BETWEEN 1 AND 4096),
    CONSTRAINT multiplayer_shards_population_range
        CHECK (current_population BETWEEN 0 AND 4096),
    CONSTRAINT multiplayer_shards_population_le_capacity
        CHECK (current_population <= capacity),
    CONSTRAINT multiplayer_shards_instance_range
        CHECK (instance_index BETWEEN 0 AND 999),
    CONSTRAINT multiplayer_shards_status_enum
        CHECK (status IN ('active','draining','closed','quarantined')),
    UNIQUE (hub_city, instance_index)
);
CREATE INDEX IF NOT EXISTS multiplayer_shards_hub_city_idx ON public.multiplayer_shards (hub_city);
CREATE INDEX IF NOT EXISTS multiplayer_shards_status_idx   ON public.multiplayer_shards (status);
CREATE INDEX IF NOT EXISTS multiplayer_shards_active_idx
    ON public.multiplayer_shards (hub_city) WHERE status = 'active';
COMMENT ON TABLE  public.multiplayer_shards IS
    'Hub-city shard instances. Public-SELECT for discovery. INSERT/UPDATE = service_role only (host manages capacity + heartbeats).';
COMMENT ON COLUMN public.multiplayer_shards.hub_city IS 'Hub-city id (e.g. ''Aetherspire'',''Verdant-Hollow'',''Ashen-Reach'').';
COMMENT ON COLUMN public.multiplayer_shards.capacity IS 'Soft-cap for concurrent members (default 256).';
COMMENT ON COLUMN public.multiplayer_shards.status   IS 'active | draining | closed | quarantined.';

-- § multiplayer_shard_members · per-(shard,player) membership
CREATE TABLE IF NOT EXISTS public.multiplayer_shard_members (
    membership_id   uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
    shard_id        uuid        NOT NULL REFERENCES public.multiplayer_shards(shard_id) ON DELETE CASCADE,
    player_id       uuid        NOT NULL,
    joined_at       timestamptz NOT NULL DEFAULT now(),
    last_seen_at    timestamptz NOT NULL DEFAULT now(),
    role            text        NOT NULL DEFAULT 'visitor',
    CONSTRAINT multiplayer_shard_members_role_enum
        CHECK (role IN ('visitor','resident','steward','moderator','founder')),
    UNIQUE (shard_id, player_id)
);
CREATE INDEX IF NOT EXISTS multiplayer_shard_members_shard_idx  ON public.multiplayer_shard_members (shard_id);
CREATE INDEX IF NOT EXISTS multiplayer_shard_members_player_idx ON public.multiplayer_shard_members (player_id);
COMMENT ON TABLE  public.multiplayer_shard_members IS
    'Membership rows per (shard,player). UNIQUE(shard_id,player_id) — one membership per pair. Player owns their own membership row · service_role admin.';

-- § helper · shard_join — atomic capacity-check + membership-row create
CREATE OR REPLACE FUNCTION public.shard_join(p_shard_id uuid, p_player_id uuid)
RETURNS uuid LANGUAGE plpgsql SECURITY DEFINER AS $$
DECLARE
    v_capacity smallint;
    v_pop      smallint;
    v_status   text;
    v_existing uuid;
    v_new      uuid;
BEGIN
    IF p_shard_id IS NULL OR p_player_id IS NULL THEN RETURN NULL; END IF;
    SELECT capacity, current_population, status
      INTO v_capacity, v_pop, v_status
      FROM public.multiplayer_shards
     WHERE shard_id = p_shard_id;
    IF v_capacity IS NULL OR v_status <> 'active' THEN RETURN NULL; END IF;
    -- idempotent : if already a member · return existing membership_id
    SELECT membership_id INTO v_existing
      FROM public.multiplayer_shard_members
     WHERE shard_id = p_shard_id AND player_id = p_player_id;
    IF v_existing IS NOT NULL THEN
        UPDATE public.multiplayer_shard_members SET last_seen_at = now()
         WHERE membership_id = v_existing;
        RETURN v_existing;
    END IF;
    IF v_pop >= v_capacity THEN RETURN NULL; END IF;
    INSERT INTO public.multiplayer_shard_members (shard_id, player_id)
        VALUES (p_shard_id, p_player_id)
     RETURNING membership_id INTO v_new;
    UPDATE public.multiplayer_shards
       SET current_population = current_population + 1, last_heartbeat = now()
     WHERE shard_id = p_shard_id;
    RETURN v_new;
END;
$$;
COMMENT ON FUNCTION public.shard_join(uuid, uuid) IS
    'SECURITY DEFINER atomic shard-join. Validates capacity + status=active. Idempotent (returns existing membership_id). Increments current_population.';

-- § RLS
ALTER TABLE public.multiplayer_shards         ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.multiplayer_shard_members  ENABLE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS "multiplayer_shards_select_public"   ON public.multiplayer_shards;
DROP POLICY IF EXISTS "multiplayer_shards_mutate_service"  ON public.multiplayer_shards;
DROP POLICY IF EXISTS "shard_members_select_party"         ON public.multiplayer_shard_members;
DROP POLICY IF EXISTS "shard_members_insert_self"          ON public.multiplayer_shard_members;
DROP POLICY IF EXISTS "shard_members_update_self"          ON public.multiplayer_shard_members;
DROP POLICY IF EXISTS "shard_members_delete_self"          ON public.multiplayer_shard_members;

CREATE POLICY "multiplayer_shards_select_public"
    ON public.multiplayer_shards FOR SELECT
    USING (true);
CREATE POLICY "multiplayer_shards_mutate_service"
    ON public.multiplayer_shards FOR ALL
    USING      (auth.role() = 'service_role')
    WITH CHECK (auth.role() = 'service_role');

-- members : self-SELECT + co-shard-member SELECT (presence) · player INSERT/UPDATE/DELETE own row
CREATE POLICY "shard_members_select_party"
    ON public.multiplayer_shard_members FOR SELECT
    USING (
        auth.uid() = player_id
        OR auth.role() = 'service_role'
        OR EXISTS (
            SELECT 1 FROM public.multiplayer_shard_members m
             WHERE m.shard_id = multiplayer_shard_members.shard_id
               AND m.player_id = auth.uid()
        )
    );
CREATE POLICY "shard_members_insert_self"
    ON public.multiplayer_shard_members FOR INSERT
    WITH CHECK ((auth.uid() IS NOT NULL AND auth.uid() = player_id) OR auth.role() = 'service_role');
CREATE POLICY "shard_members_update_self"
    ON public.multiplayer_shard_members FOR UPDATE
    USING      (auth.uid() = player_id OR auth.role() = 'service_role')
    WITH CHECK (auth.uid() = player_id OR auth.role() = 'service_role');
CREATE POLICY "shard_members_delete_self"
    ON public.multiplayer_shard_members FOR DELETE
    USING (auth.uid() = player_id OR auth.role() = 'service_role');

-- § grants
GRANT SELECT                 ON public.multiplayer_shards         TO authenticated, anon;
GRANT ALL                    ON public.multiplayer_shards         TO service_role;
GRANT SELECT, INSERT, UPDATE, DELETE ON public.multiplayer_shard_members TO authenticated;
GRANT ALL                    ON public.multiplayer_shard_members  TO service_role;
GRANT EXECUTE ON FUNCTION public.shard_join(uuid, uuid) TO authenticated, service_role;
