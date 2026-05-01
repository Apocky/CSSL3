-- =====================================================================
-- § T11-W7-RD-D4-MIGRATIONS · 0016_gear_inventory.sql
-- Player-scoped gear inventory + loadouts (gift-economy framing).
-- Ref : GDDs/GEAR_RARITY_SYSTEM.csl § BONDING + DROP-RATES + SLOTS.
-- § PRIME_DIRECTIVE : sovereignty preserved · gift-economy (DELETE = service-role)
-- · bonded items cannot unbond (audit-immutable bond commitment) · all helper
-- fns clamp inputs · audit-emit-compatible. Apply after 0015.
-- =====================================================================
CREATE EXTENSION IF NOT EXISTS pgcrypto;

-- § gear_inventory · per-player owned-items (bonded ⊕ unbonded)
CREATE TABLE IF NOT EXISTS public.gear_inventory (
    inventory_id      uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
    player_id         uuid        NOT NULL,
    gear_seed         bytea       NOT NULL,
    slot              text        NOT NULL,
    item_class        text        NOT NULL,
    rarity            text        NOT NULL,
    item_level        smallint    NOT NULL DEFAULT 1,
    is_bonded         boolean     NOT NULL DEFAULT false,
    bonded_at         timestamptz NULL,
    equipped_to_slot  text        NULL,
    created_at        timestamptz NOT NULL DEFAULT now(),
    last_modified     timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT gear_inventory_slot_enum
        CHECK (slot IN ('helm','chest','legs','boots','gloves','main_hand','off_hand','ring1','ring2','amulet','cloak')),
    CONSTRAINT gear_inventory_rarity_enum
        CHECK (rarity IN ('common','uncommon','rare','epic','legendary','mythic','sovereign')),
    CONSTRAINT gear_inventory_item_class_length
        CHECK (char_length(item_class) BETWEEN 1 AND 200),
    CONSTRAINT gear_inventory_item_level_range
        CHECK (item_level BETWEEN 1 AND 999),
    CONSTRAINT gear_inventory_seed_length
        CHECK (octet_length(gear_seed) BETWEEN 1 AND 256),
    CONSTRAINT gear_inventory_bonded_at_set_iff_bonded
        CHECK ((is_bonded = false AND bonded_at IS NULL) OR (is_bonded = true AND bonded_at IS NOT NULL)),
    CONSTRAINT gear_inventory_equipped_slot_enum
        CHECK (equipped_to_slot IS NULL OR equipped_to_slot IN ('helm','chest','legs','boots','gloves','main_hand','off_hand','ring1','ring2','amulet','cloak'))
);
CREATE INDEX IF NOT EXISTS gear_inventory_player_idx        ON public.gear_inventory (player_id);
CREATE INDEX IF NOT EXISTS gear_inventory_player_slot_idx   ON public.gear_inventory (player_id, slot);
CREATE INDEX IF NOT EXISTS gear_inventory_unbonded_idx      ON public.gear_inventory (player_id) WHERE is_bonded = false;
CREATE INDEX IF NOT EXISTS gear_inventory_equipped_idx      ON public.gear_inventory (player_id, equipped_to_slot) WHERE equipped_to_slot IS NOT NULL;
COMMENT ON TABLE  public.gear_inventory IS
    'Player-owned gear (gift-economy). DELETE = service_role only ; players cannot destroy gear (transfer instead). Bonded items immutable on bonding. PRIME_DIRECTIVE : sovereignty preserved.';
COMMENT ON COLUMN public.gear_inventory.gear_seed        IS 'Deterministic procgen seed (≤256 bytes). Drives stat-roll regeneration on load.';
COMMENT ON COLUMN public.gear_inventory.slot             IS 'Item slot category (helm/chest/legs/boots/gloves/main_hand/off_hand/ring1/ring2/amulet/cloak).';
COMMENT ON COLUMN public.gear_inventory.rarity           IS 'common < uncommon < rare < epic < legendary < mythic < sovereign.';
COMMENT ON COLUMN public.gear_inventory.is_bonded        IS 'TRUE iff bonded to player. Bonding immutable (audit-trail invariant) — see gear_equip_to_slot helper.';
COMMENT ON COLUMN public.gear_inventory.equipped_to_slot IS 'NULL = inventory ; non-NULL = currently equipped slot. Set via gear_equip_to_slot helper.';

-- § gear_loadouts · player-named slot-assignment presets (jsonb)
CREATE TABLE IF NOT EXISTS public.gear_loadouts (
    loadout_id          uuid        PRIMARY KEY DEFAULT gen_random_uuid(),
    player_id           uuid        NOT NULL,
    loadout_name        text        NOT NULL DEFAULT 'default',
    slot_assignments    jsonb       NOT NULL DEFAULT '{}'::jsonb,
    created_at          timestamptz NOT NULL DEFAULT now(),
    last_modified       timestamptz NOT NULL DEFAULT now(),
    CONSTRAINT gear_loadouts_name_length
        CHECK (char_length(loadout_name) BETWEEN 1 AND 100),
    CONSTRAINT gear_loadouts_slot_assignments_is_object
        CHECK (jsonb_typeof(slot_assignments) = 'object'),
    UNIQUE (player_id, loadout_name)
);
CREATE INDEX IF NOT EXISTS gear_loadouts_player_idx ON public.gear_loadouts (player_id);
COMMENT ON TABLE  public.gear_loadouts IS
    'Named loadout presets per player. slot_assignments = {slot:inventory_id} jsonb mapping. Players UPDATE freely ; DELETE = service-role only.';

-- § helper · gear_equip_to_slot — atomic equip + bond-on-equip + audit-emit
CREATE OR REPLACE FUNCTION public.gear_equip_to_slot(
    p_player_id    uuid,
    p_inventory_id uuid,
    p_slot         text
) RETURNS boolean LANGUAGE plpgsql SECURITY DEFINER AS $$
DECLARE
    v_owns boolean;
    v_slot text;
BEGIN
    -- clamp + validate
    IF p_player_id IS NULL OR p_inventory_id IS NULL OR p_slot IS NULL THEN
        RETURN false;
    END IF;
    IF p_slot NOT IN ('helm','chest','legs','boots','gloves','main_hand','off_hand','ring1','ring2','amulet','cloak') THEN
        RETURN false;
    END IF;
    -- ownership check
    SELECT slot INTO v_slot FROM public.gear_inventory
     WHERE inventory_id = p_inventory_id AND player_id = p_player_id;
    IF v_slot IS NULL OR v_slot <> p_slot THEN
        RETURN false;
    END IF;
    -- unequip prior occupant of slot
    UPDATE public.gear_inventory
       SET equipped_to_slot = NULL, last_modified = now()
     WHERE player_id = p_player_id AND equipped_to_slot = p_slot;
    -- equip + bond (bonding is one-way · audit-immutable)
    UPDATE public.gear_inventory
       SET equipped_to_slot = p_slot,
           is_bonded        = true,
           bonded_at        = COALESCE(bonded_at, now()),
           last_modified    = now()
     WHERE inventory_id = p_inventory_id AND player_id = p_player_id;
    RETURN true;
END;
$$;
COMMENT ON FUNCTION public.gear_equip_to_slot(uuid, uuid, text) IS
    'Atomic equip-to-slot. Bonds-on-equip (one-way · audit-immutable). Validates ownership + slot match. Returns TRUE on success, FALSE on validation fail.';

-- § trigger · enforce bonded-cannot-unbond invariant (audit-immutable)
CREATE OR REPLACE FUNCTION public.gear_inventory_enforce_bond_immutability()
RETURNS trigger LANGUAGE plpgsql AS $$
BEGIN
    IF OLD.is_bonded = true AND NEW.is_bonded = false THEN
        RAISE EXCEPTION 'gear_inventory: bonded items cannot be unbonded (audit-trail invariant)';
    END IF;
    IF OLD.is_bonded = true AND NEW.bonded_at IS DISTINCT FROM OLD.bonded_at THEN
        RAISE EXCEPTION 'gear_inventory: bonded_at is immutable once bonded';
    END IF;
    RETURN NEW;
END;
$$;
DROP TRIGGER IF EXISTS gear_inventory_bond_immutability_trg ON public.gear_inventory;
CREATE TRIGGER gear_inventory_bond_immutability_trg
    BEFORE UPDATE ON public.gear_inventory
    FOR EACH ROW EXECUTE FUNCTION public.gear_inventory_enforce_bond_immutability();
COMMENT ON FUNCTION public.gear_inventory_enforce_bond_immutability() IS
    'Trigger fn · blocks unbond + bonded_at mutation · audit-trail invariant. service_role bypasses via SECURITY DEFINER admin tools if needed.';

-- § RLS · player-scoped · gift-economy DELETE = service-role only
ALTER TABLE public.gear_inventory ENABLE ROW LEVEL SECURITY;
ALTER TABLE public.gear_loadouts  ENABLE ROW LEVEL SECURITY;
DROP POLICY IF EXISTS "gear_inventory_select_self" ON public.gear_inventory;
DROP POLICY IF EXISTS "gear_inventory_insert_self" ON public.gear_inventory;
DROP POLICY IF EXISTS "gear_inventory_update_self" ON public.gear_inventory;
DROP POLICY IF EXISTS "gear_loadouts_select_self"  ON public.gear_loadouts;
DROP POLICY IF EXISTS "gear_loadouts_insert_self"  ON public.gear_loadouts;
DROP POLICY IF EXISTS "gear_loadouts_update_self"  ON public.gear_loadouts;

CREATE POLICY "gear_inventory_select_self"
    ON public.gear_inventory FOR SELECT
    USING (auth.uid() = player_id OR auth.role() = 'service_role');

CREATE POLICY "gear_inventory_insert_self"
    ON public.gear_inventory FOR INSERT
    WITH CHECK ((auth.uid() IS NOT NULL AND auth.uid() = player_id) OR auth.role() = 'service_role');

-- UPDATE : self-allowed · bond-immutability enforced by trigger (above)
CREATE POLICY "gear_inventory_update_self"
    ON public.gear_inventory FOR UPDATE
    USING      (auth.uid() = player_id OR auth.role() = 'service_role')
    WITH CHECK (auth.uid() = player_id OR auth.role() = 'service_role');

-- N! NO DELETE policy on gear_inventory · service_role only (gift-economy invariant)

CREATE POLICY "gear_loadouts_select_self"
    ON public.gear_loadouts FOR SELECT
    USING (auth.uid() = player_id OR auth.role() = 'service_role');
CREATE POLICY "gear_loadouts_insert_self"
    ON public.gear_loadouts FOR INSERT
    WITH CHECK ((auth.uid() IS NOT NULL AND auth.uid() = player_id) OR auth.role() = 'service_role');
CREATE POLICY "gear_loadouts_update_self"
    ON public.gear_loadouts FOR UPDATE
    USING      (auth.uid() = player_id OR auth.role() = 'service_role')
    WITH CHECK (auth.uid() = player_id OR auth.role() = 'service_role');

-- § grants
GRANT SELECT, INSERT, UPDATE ON public.gear_inventory TO authenticated;
GRANT SELECT, INSERT, UPDATE ON public.gear_loadouts  TO authenticated;
GRANT ALL                    ON public.gear_inventory TO service_role;
GRANT ALL                    ON public.gear_loadouts  TO service_role;
GRANT EXECUTE ON FUNCTION public.gear_equip_to_slot(uuid, uuid, text) TO authenticated, service_role;
