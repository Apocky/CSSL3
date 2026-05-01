//! § fixtures — cross-crate test-builders + type-coercion wrappers.
//! ════════════════════════════════════════════════════════════════════
//!
//! § DESIGN
//!   Source-crates are NEVER modified. Cross-crate coercions live here as
//!   small free fns ([`material_to_basemat`] · [`item_class_coerce`]).
//!   Test-builders construct minimal-but-realistic instances so each
//!   integration-test stays small + readable.
//!
//! § DETERMINISM
//!   Every builder takes an explicit seed (u64 or u128) so tests can assert
//!   bit-equal replay across runs. No hidden RNG.

use std::collections::BTreeMap;

// ─── POD-2 substrate ─────────────────────────────────────────────────────
use cssl_host_combat_sim as combat;
use cssl_host_craft_graph as craft;
use cssl_host_gear_archetype as gear;
use cssl_host_npc_bt as npc;
use cssl_host_spell_graph as spell;
// `cssl_host_roguelike_run` is consumed directly by run-lifecycle tests in
// `tests/run_lifecycle.rs` ; not needed here.

// ─── W7 ──────────────────────────────────────────────────────────────────
use cssl_host_dm as dm;
use cssl_host_gm as gm;

// ════════════════════════════════════════════════════════════════════════
// § Type-coercion : craft Material → gear BaseMat
// ════════════════════════════════════════════════════════════════════════

/// Map a `cssl-host-craft-graph::Material` to the closest
/// `cssl-host-gear-archetype::BaseMat` mat (METALS pool).
///
/// Non-METAL inputs (woods/cloths/etc.) coerce to the tier-equivalent METAL
/// per `material_tier(...)` so the tier-floor invariant holds. This is a
/// LOSSY coercion ; tests that need exact-mat preservation should construct
/// `BaseMat` directly.
#[must_use]
pub fn material_to_basemat(m: craft::Material) -> gear::BaseMat {
    use craft::Material as M;
    use gear::BaseMat as B;
    // Direct METALS-to-METALS mapping where the names line up.
    match m {
        M::Iron => B::Iron,
        M::Silver => B::Silver,
        M::Mithril => B::Mithril,
        M::Adamant => B::Adamant,
        M::Voidsteel => B::Voidsteel,
        M::Soulalloy => B::Soulbound,
        // Non-METAL : coerce by tier.
        other => match craft::material_tier(other) {
            1 => B::Iron,
            2 => B::Silver,
            3 => B::Mithril,
            4 => B::Adamant,
            5 => B::Voidsteel,
            _ => B::Soulbound,
        },
    }
}

/// Map gear::ItemClass → craft::recipe::ItemClass (consume Trinket → Trinket).
#[must_use]
pub fn item_class_coerce(c: gear::ItemClass) -> craft::ItemClass {
    match c {
        gear::ItemClass::Weapon => craft::ItemClass::Weapon,
        gear::ItemClass::Armor => craft::ItemClass::Armor,
        gear::ItemClass::Jewelry => craft::ItemClass::Jewelry,
        gear::ItemClass::Trinket => craft::ItemClass::Trinket,
    }
}

// ════════════════════════════════════════════════════════════════════════
// § CombatSession — paired actor + target + roll-fixture
// ════════════════════════════════════════════════════════════════════════

/// Self-contained combat-pair fixture for tick-driven tests.
#[derive(Debug)]
pub struct CombatSession {
    /// Attacker tick-state.
    pub attacker: combat::CombatTick,
    /// Defender tick-state.
    pub defender: combat::CombatTick,
    /// Pre-computed damage-roll the caller hands the tick-fn.
    pub damage_roll: combat::DamageRoll,
    /// Defender's armor-class (for affinity application).
    pub defender_armor: combat::ArmorClass,
    /// The seed used to construct ; useful for replay assertions.
    pub seed: u64,
}

/// Build a deterministic 1v1 combat session at the given seed.
#[must_use]
pub fn make_combat_session(seed: u64) -> CombatSession {
    let attacker = combat::CombatTick::new(
        combat::WeaponArchetype::Sword,
        combat::ArmorClass::LeatherMed,
        seed,
    );
    let defender = combat::CombatTick::new(
        combat::WeaponArchetype::Hammer,
        combat::ArmorClass::FleshBeast,
        seed.wrapping_add(1),
    );
    // 30-damage Slash vs FleshBeast (1.0× per affinity-table). The exact f32
    // value is bit-equal across runs because affinity-table is `pub const`.
    let damage_roll = combat::DamageRoll::single(combat::DamageType::Slash, 30.0);
    CombatSession {
        attacker,
        defender,
        damage_roll,
        defender_armor: combat::ArmorClass::FleshBeast,
        seed,
    }
}

// ════════════════════════════════════════════════════════════════════════
// § PlayerInventory — 13-slot equipped + bond-state
// ════════════════════════════════════════════════════════════════════════

/// Player's equipped-gear inventory + bond/loadout-state.
#[derive(Debug, Default)]
pub struct PlayerInventory {
    /// 13 slots ; None = empty.
    pub slots: BTreeMap<gear::GearSlot, gear::Gear>,
    /// Saved-loadout snapshot for switch-tests.
    pub saved_loadout: BTreeMap<gear::GearSlot, gear::Gear>,
}

impl PlayerInventory {
    /// True iff the slot is currently equipped.
    #[must_use]
    pub fn is_equipped(&self, slot: gear::GearSlot) -> bool {
        self.slots.contains_key(&slot)
    }

    /// Lookup the equipped gear at `slot`, if any.
    #[must_use]
    pub fn get(&self, slot: gear::GearSlot) -> Option<&gear::Gear> {
        self.slots.get(&slot)
    }

    /// Snapshot the current loadout into `saved_loadout`.
    pub fn snapshot_loadout(&mut self) {
        self.saved_loadout = self.slots.clone();
    }

    /// Restore from `saved_loadout` (overwrites current).
    pub fn restore_loadout(&mut self) {
        self.slots = self.saved_loadout.clone();
    }
}

/// Build an empty player inventory.
#[must_use]
pub fn make_player_inventory() -> PlayerInventory {
    PlayerInventory::default()
}

/// Equip a gear into its slot. Returns
/// - `Ok(true)` if slot was previously empty (fresh equip)
/// - `Ok(false)` if it overwrote an existing equipped item
/// - `Err(reason)` if the gear's slot doesn't match the requested slot.
pub fn equip_into_slot(
    inv: &mut PlayerInventory,
    slot: gear::GearSlot,
    g: gear::Gear,
) -> Result<bool, &'static str> {
    if g.slot != slot {
        return Err("slot_mismatch");
    }
    let was_empty = !inv.slots.contains_key(&slot);
    inv.slots.insert(slot, g);
    Ok(was_empty)
}

// ════════════════════════════════════════════════════════════════════════
// § RecipeBook — default-graph + skill state + audit-sink
// ════════════════════════════════════════════════════════════════════════

/// Recipe-book bundle for craft-tests.
#[derive(Debug)]
pub struct RecipeBook {
    /// 40-recipe default DAG.
    pub graph: craft::RecipeGraph,
    /// Caller's craft-skill state.
    pub craft_skill: craft::CraftSkill,
    /// Caller's deconstruct-skill state.
    pub decon_skill: craft::CraftSkill,
    /// Recording-sink for deterministic audit-trace assertions.
    pub audit: craft::RecordingAuditSink,
}

/// Build a recipe-book with skill-curve preset to `skill_level`.
#[must_use]
pub fn make_recipe_book(skill_level: u8) -> RecipeBook {
    RecipeBook {
        graph: craft::default_recipe_graph(),
        craft_skill: craft::CraftSkill::with_level(skill_level),
        decon_skill: craft::CraftSkill::with_level(skill_level),
        audit: craft::RecordingAuditSink::default(),
    }
}

/// Hand-crafted T1 weapon helper. Returns the `gear::Gear` at the requested
/// rarity using the fixture-coerced base-mat.
#[must_use]
pub fn craft_a_t1_weapon(seed: u128, rarity: gear::Rarity) -> gear::Gear {
    let mat = gear::default_base_for_rarity(rarity);
    let base = gear::BaseItem::weapon(gear::GearSlot::MainHand, mat, 12.0, 1.0);
    gear::roll_gear(seed, &base, rarity)
}

/// Deconstruct a synthetic `CraftedItem` reproducing the gear's base-mat
/// lineage. Returns the decon-result + recovered (Material, count) pairs.
#[must_use]
pub fn deconstruct_a_crafted_item(
    base_mat_used: craft::Material,
    count: u8,
    skill: u8,
    tool_q: u8,
) -> craft::DeconstructResult {
    let item = craft::CraftedItem {
        recipe_id: 5, // R05 sword-T1 per default_recipe_graph
        class: craft::ItemClass::Weapon,
        tier: 1,
        lineage: vec![(base_mat_used, count)],
        glyph_slots: Vec::new(),
    };
    craft::deconstruct(
        &item,
        craft::DeconstructTool::Voidsmelter,
        skill,
        tool_q,
        0.99, // glyph_roll : > 0.50 ⇒ no shard (we have no filled slots anyway)
    )
}

// ════════════════════════════════════════════════════════════════════════
// § Grimoire / Mana / Spell-cast
// ════════════════════════════════════════════════════════════════════════

/// Build a 6-slot grimoire + mana-pool sized to the caster's Intelligence.
///
/// `intelligence` follows the `caster_intelligence` field of `spell::Target` ;
/// each Int-point yields +10 capacity (matches GDD scaling).
///
/// Returns (grimoire, mana-pool).
#[must_use]
pub fn make_grimoire(intelligence: u8) -> (spell::Grimoire, spell::ManaPool) {
    let g = spell::Grimoire::new();
    let cap = 10.0f32.mul_add(intelligence as f32, 10.0);
    let pool = spell::ManaPool::new(cap);
    (g, pool)
}

/// Build a minimal valid Fire-Ray spell-graph (Source → Shape → Trigger).
#[must_use]
pub fn minimal_fire_ray() -> spell::SpellGraph {
    let mut g = spell::SpellGraph::new();
    let s = g.add_node(spell::SpellNode::Source(spell::Element::Fire));
    let sh = g.add_node(spell::SpellNode::Shape(spell::ShapeKind::Ray));
    let tr = g.add_node(spell::SpellNode::Trigger(spell::TriggerKind::OnCast));
    g.add_edge(s, sh);
    g.add_edge(sh, tr);
    g
}

/// Cast the minimal-fire-ray against a fixed target, debiting `mana`.
pub fn cast_minimal_fire_ray(
    mana: &mut spell::ManaPool,
    intelligence: u8,
) -> spell::CastResult {
    let g = minimal_fire_ray();
    let target = spell::Target {
        x: 1,
        y: 2,
        z: 3,
        consent_present: true,
        caster_intelligence: intelligence,
    };
    spell::cast(&g, target, mana)
}

// ════════════════════════════════════════════════════════════════════════
// § Full-loop driver
// ════════════════════════════════════════════════════════════════════════

/// Composite outcome of `run_full_loop` — used by full_loop tests for
/// determinism + step-by-step assertions.
#[derive(Debug, Clone)]
pub struct FullLoopOutcome {
    /// Damage dealt by the attacker on the chosen tick.
    pub damage_dealt: f32,
    /// Rarity of the looted gear.
    pub looted_rarity: gear::Rarity,
    /// Mat returned from deconstructing the lootedgear's base-lineage.
    pub recovered_mats: Vec<(craft::Material, u8)>,
    /// Was the new T1 weapon equipped successfully ?
    pub equipped_ok: bool,
    /// Did the spell-cast succeed ?
    pub cast_ok: bool,
    /// Final mana-pool current value.
    pub mana_after: f32,
    /// Display-name of the looted gear (for human-readable assertions).
    pub looted_display_name: String,
}

/// Drive the full loop : combat → loot → craft → equip → cast.
///
/// Deterministic for given (seed, intelligence). Bit-equal across runs.
pub fn run_full_loop(seed: u128, intelligence: u8) -> FullLoopOutcome {
    // 1) Combat-tick : attacker swings, deals damage.
    let mut session = make_combat_session(seed as u64);
    // Press attack to enter Windup.
    let _ = session.attacker.tick(combat::CombatInput::AttackPress, 0.016, None, None);
    // Bring attacker to Active by simulating windup-completion (driver-fn).
    session.attacker.state = combat::CombatState::Active;
    // Deal damage on the active-tick.
    let out = session.attacker.tick(
        combat::CombatInput::None,
        0.016,
        Some(session.damage_roll),
        Some(session.defender_armor),
    );
    let damage_dealt = out.damage_dealt;

    // 2) Loot-drop : roll a gear from the drop-table.
    let ctx = gear::DropContext {
        mob_tier: 3, // mid-tier mob
        biome: gear::Biome::Dungeon,
        magic_find: 0.5,
    };
    let looted = gear::roll_drop(&ctx, seed, Some(gear::GearSlot::MainHand))
        .expect("roll_drop never returns None for valid context");
    let looted_rarity = looted.rarity;
    let looted_display_name = looted.display_name();

    // 3) Craft : feed the looted gear's base-lineage through deconstruct.
    let craft_mat = match looted.base.base_mat {
        gear::BaseMat::Iron => craft::Material::Iron,
        gear::BaseMat::Silver => craft::Material::Silver,
        gear::BaseMat::Mithril => craft::Material::Mithril,
        gear::BaseMat::Adamant => craft::Material::Adamant,
        gear::BaseMat::Voidsteel => craft::Material::Voidsteel,
        gear::BaseMat::Soulbound => craft::Material::Soulalloy,
    };
    let decon = deconstruct_a_crafted_item(craft_mat, 5, 50, 50);
    let recovered_mats = decon.returned_mats;

    // 4) Equip : craft a T1 weapon from the recovered tier-mat ; equip it.
    let crafted = craft_a_t1_weapon(seed, gear::Rarity::Common);
    let mut inv = make_player_inventory();
    let equipped_ok = equip_into_slot(&mut inv, gear::GearSlot::MainHand, crafted).is_ok();

    // 5) Cast : minimal-fire-ray with full mana ; success expected.
    let (_grim, mut mana) = make_grimoire(intelligence);
    let cast_res = cast_minimal_fire_ray(&mut mana, intelligence);

    FullLoopOutcome {
        damage_dealt,
        looted_rarity,
        recovered_mats,
        equipped_ok,
        cast_ok: cast_res.success,
        mana_after: mana.current,
        looted_display_name,
    }
}

// ════════════════════════════════════════════════════════════════════════
// § NPC-BT + DM/GM bridge helpers
// ════════════════════════════════════════════════════════════════════════

/// Stub world-ref for NPC-BT tick-tests. Reads constant scalars.
pub struct StubNpcWorld;

impl npc::NpcWorldRef for StubNpcWorld {
    fn current_zone(&self) -> u32 {
        1
    }
    fn hp_ratio(&self) -> f32 {
        0.5
    }
    fn mana_ratio(&self) -> f32 {
        0.5
    }
    fn nearby_ally(&self) -> bool {
        false
    }
    fn target_is_hostile(&self, _t: u64) -> bool {
        false
    }
    fn game_hour_block(&self) -> u8 {
        12
    }
    fn dialogue_open(&self) -> bool {
        false
    }
    fn resource_count(&self, _k: u32) -> u32 {
        0
    }
    fn is_idle(&self) -> bool {
        true
    }
}

/// Build a tiny BT : Selector { Idle? , LowHP? }.
/// Idle? Succeeds against StubNpcWorld ⇒ Selector returns Success.
#[must_use]
pub fn tiny_bt() -> npc::BtNode {
    npc::BtNode::Selector(vec![
        npc::BtNode::Condition(npc::ConditionKind::Idle),
        npc::BtNode::Condition(npc::ConditionKind::LowHP),
    ])
}

/// Build a default DM scaffold (heuristic-arbiter + all-caps + noop-audit).
///
/// Argument-order per `cssl-host-dm::DirectorMaster::new` :
///     (cap_table, arbiter, audit_sink).
#[must_use]
pub fn make_dm() -> dm::DirectorMaster {
    dm::DirectorMaster::new(
        dm::DmCapTable::all_granted(),
        Box::new(dm::Stage0HeuristicArbiter::new()),
        Box::new(dm::NoopAuditSink),
    )
}

/// Build a default GM scaffold (stage-0 pacing-policy + all-caps + null-audit).
///
/// Argument-order per `cssl-host-gm::GameMaster::new` :
///     (caps, templates, pacing, audit, micros_per_tick).
#[must_use]
pub fn make_gm() -> gm::GameMaster {
    gm::GameMaster::new(
        gm::GmCapTable::all(),
        gm::TemplateTable::default(),
        Box::new(gm::Stage0PacingPolicy),
        Box::new(gm::NullAuditSink),
        1,
    )
}
