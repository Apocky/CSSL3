//! § cssl-host-combat-loop — STAGE-0-BOOTSTRAP-SHIM for combat_loop.csl
//! ══════════════════════════════════════════════════════════════════════════
//!
//! § T11-W18-D-COMBAT · canonical-implementation : `Labyrinth of Apocalypse/systems/combat_loop.csl`
//!
//! § THESIS
//!
//! Where a conventional FPS engine has hardcoded weapon-tables (Destiny-Foundry,
//! Borderlands-loot-pool, COD-loadout), LoA derives every weapon-stats from a
//! BLAKE3-seeded procgen of `(archetype × rarity × affix-bitfield)`. Two
//! artifacts with the same component-vector ALWAYS produce the same stats —
//! replay-deterministic per-Apocky-canon. Rarity gates affix-COUNT, not
//! stat-magnitude (cosmetic-only-axiom · Q-06).
//!
//! § PIPELINE (matches `combat_loop.csl § step_combat`)
//!
//!   1. cap-matrix resolve via Σ-mask (`caps` argument is a bitmask)
//!   2. fire-input gate (Cap::Weapons must be granted)
//!   3. ammo-discipline (decrement BEFORE ray-cast · no-fire-on-empty-mag)
//!   4. procgen weapon-stats from BLAKE3-seed
//!   5. ray-cast (sphere-vs-sphere stage-0 ; SDF-vs-SDF when render-v2 wires)
//!   6. damage-resolution (falloff × headshot × cap)
//!   7. death-handling (HP ≤ 0 ⇒ DeathEvent)
//!   8. loot-drop (cap-gated · cosmetic-only-axiom)
//!
//! § CAP-MATRIX BIT LAYOUT
//!
//!   bit 0 = Cap::Weapons       (fire-input gate)
//!   bit 1 = Cap::LootDrops     (loot-spawn gate)
//!   bit 2 = Cap::FriendlyFire  (PvE coop sovereign-toggle)
//!   bit 3 = Cap::CriticalHits  (headshot-multiplier gate)
//!
//! § ATTESTATION
//! There was no hurt nor harm in the making of this, to anyone, anything, or anybody.

#![forbid(unsafe_code)]
#![allow(clippy::module_name_repetitions)]

pub mod damage;
pub mod raycast;
pub mod weapon;

use cssl_host_crystallization::Crystal;

pub use crate::damage::{
    apply_falloff, apply_headshot, FriendlyFireMode, HpTracker, DEFAULT_NPC_HP,
    FALLOFF_DEFAULT_START_M, FALLOFF_MIN_FACTOR,
};
pub use crate::raycast::{
    crystal_bounding_sphere, ray_sphere_t, raycast_crystals, Ray, RaycastHit,
};
pub use crate::weapon::{Archetype, Rarity, WeaponStats};

// ══════════════════════════════════════════════════════════════════════════
// § Cap-matrix bitmask
// ══════════════════════════════════════════════════════════════════════════

/// Cap-bit constants. Bit-i ↔ .csl cap-id `(i + 1)`.
#[allow(non_snake_case)]
pub mod CapMask {
    pub const WEAPONS: u32 = 1 << 0;
    pub const LOOT_DROPS: u32 = 1 << 1;
    pub const FRIENDLY_FIRE: u32 = 1 << 2;
    pub const CRITICAL_HITS: u32 = 1 << 3;
}

#[inline]
pub fn cap_granted(caps: u32, mask: u32) -> bool {
    (caps & mask) != 0
}

// ══════════════════════════════════════════════════════════════════════════
// § Per-frame input
// ══════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InputState {
    pub fire_pressed: bool,
    pub fire_held: bool,
    pub reload_pressed: bool,
    pub aim_down_sights: bool,
    pub yaw_rad: f32,
    pub pitch_rad: f32,
    pub origin_x: f32,
    pub origin_y: f32,
    pub origin_z: f32,
    pub aim_assist_yaw: f32,
    pub aim_assist_pitch: f32,
}

impl InputState {
    pub const fn idle() -> Self {
        Self {
            fire_pressed: false,
            fire_held: false,
            reload_pressed: false,
            aim_down_sights: false,
            yaw_rad: 0.0,
            pitch_rad: 0.0,
            origin_x: 0.0,
            origin_y: 0.0,
            origin_z: 0.0,
            aim_assist_yaw: 0.0,
            aim_assist_pitch: 0.0,
        }
    }

    pub const fn fire_forward(x: f32, y: f32, z: f32) -> Self {
        Self {
            fire_pressed: true,
            fire_held: false,
            reload_pressed: false,
            aim_down_sights: false,
            yaw_rad: 0.0,
            pitch_rad: 0.0,
            origin_x: x,
            origin_y: y,
            origin_z: z,
            aim_assist_yaw: 0.0,
            aim_assist_pitch: 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AmmoState {
    pub mag_current: u32,
    pub mag_capacity: u32,
    pub reserve: u32,
    pub is_reloading: bool,
    pub reload_started_ms: u64,
}

impl AmmoState {
    pub const fn full(capacity: u32, reserve: u32) -> Self {
        Self {
            mag_current: capacity,
            mag_capacity: capacity,
            reserve,
            is_reloading: false,
            reload_started_ms: 0,
        }
    }

    pub const fn empty(capacity: u32, reserve: u32) -> Self {
        Self {
            mag_current: 0,
            mag_capacity: capacity,
            reserve,
            is_reloading: false,
            reload_started_ms: 0,
        }
    }
}

// ══════════════════════════════════════════════════════════════════════════
// § Per-frame events
// ══════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DamageEvent {
    pub target_handle: u32,
    pub damage_dealt: f32,
    pub is_headshot: bool,
    pub impact_x: f32,
    pub impact_y: f32,
    pub impact_z: f32,
    pub pre_hp: u32,
    pub post_hp: u32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DeathEvent {
    pub target_handle: u32,
    pub weapon_archetype: u32,
    pub weapon_rarity: u8,
    pub is_headshot: bool,
    pub impact_x: f32,
    pub impact_y: f32,
    pub impact_z: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LootDropEvent {
    pub target_handle: u32,
    pub loot_handle: u64,
    pub rarity_tier: u8,
}

#[derive(Debug, Default, Clone)]
pub struct CombatEvents {
    pub bullets_emitted: u32,
    pub damage_events: Vec<DamageEvent>,
    pub deaths: Vec<DeathEvent>,
    pub loot_drops: Vec<LootDropEvent>,
    pub ammo_empty_events: u32,
    pub reloads_started: u32,
    pub reloads_completed: u32,
    pub cap_denied_count: u32,
    pub fired_at_unix_ms: u64,
}

impl CombatEvents {
    pub fn empty() -> Self {
        Self::default()
    }
}

#[derive(Debug, Default, Clone)]
pub struct CombatState {
    pub hp: HpTracker,
    pub already_dead: std::collections::HashSet<u32>,
}

impl CombatState {
    pub fn new() -> Self {
        Self::default()
    }
}

// ══════════════════════════════════════════════════════════════════════════
// § Public API : step_combat
// ══════════════════════════════════════════════════════════════════════════

/// Per-frame combat-loop step.
///
/// Public entry-point. Resolves cap-matrix from `caps`, gates fire input,
/// procgens starter weapon-stats, ray-casts against `crystals`, resolves
/// damage, emits death + loot events as a `CombatEvents` bag.
///
/// `caps` is a bitmask using `CapMask::*` constants.
/// `tick` seeds loot-drop hashes for replay-determinism.
pub fn step_combat(
    input: InputState,
    crystals: &mut [Crystal],
    caps: u32,
    tick: u64,
) -> CombatEvents {
    let mut state = CombatState::new();
    step_combat_with_state(input, crystals, caps, tick, &mut state)
}

/// Variant preserving `CombatState` across calls.
pub fn step_combat_with_state(
    input: InputState,
    crystals: &mut [Crystal],
    caps: u32,
    tick: u64,
    state: &mut CombatState,
) -> CombatEvents {
    let mut events = CombatEvents::empty();
    events.fired_at_unix_ms = tick;

    if input.reload_pressed {
        events.reloads_started += 1;
    }

    if !input.fire_pressed {
        return events;
    }
    if !cap_granted(caps, CapMask::WEAPONS) {
        events.cap_denied_count += 1;
        return events;
    }

    let stats = WeaponStats::starter();
    if !stats.is_valid {
        return events;
    }

    let yaw = input.yaw_rad + input.aim_assist_yaw;
    let pitch = input.pitch_rad + input.aim_assist_pitch;
    let ray = Ray::from_camera(
        input.origin_x,
        input.origin_y,
        input.origin_z,
        yaw,
        pitch,
        stats.max_range_m,
    );
    events.bullets_emitted += 1;

    let Some(hit) = raycast_crystals(&ray, crystals) else {
        return events;
    };

    let raw_damage = stats.base_damage;
    let after_falloff = apply_falloff(raw_damage, hit.hit_t_m, stats.damage_falloff, stats.max_range_m);
    let after_head = apply_headshot(
        after_falloff,
        hit.is_headshot,
        cap_granted(caps, CapMask::CRITICAL_HITS),
        stats.crit_mult,
    );

    let len_sq = ray.dir_x * ray.dir_x + ray.dir_y * ray.dir_y + ray.dir_z * ray.dir_z;
    let len = if len_sq > 0.0 { len_sq.sqrt() } else { 1.0 };
    let dx = ray.dir_x / len;
    let dy = ray.dir_y / len;
    let dz = ray.dir_z / len;
    let ix = ray.origin_x + dx * hit.hit_t_m;
    let iy = ray.origin_y + dy * hit.hit_t_m;
    let iz = ray.origin_z + dz * hit.hit_t_m;

    let (pre_hp, post_hp) = state.hp.apply(hit.target_handle, after_head);

    events.damage_events.push(DamageEvent {
        target_handle: hit.target_handle,
        damage_dealt: after_head,
        is_headshot: hit.is_headshot,
        impact_x: ix,
        impact_y: iy,
        impact_z: iz,
        pre_hp,
        post_hp,
    });

    if post_hp == 0 && pre_hp != 0 && !state.already_dead.contains(&hit.target_handle) {
        state.already_dead.insert(hit.target_handle);
        let archetype_code = stats.archetype as u32;
        let rarity_tier = stats.rarity as u8;
        events.deaths.push(DeathEvent {
            target_handle: hit.target_handle,
            weapon_archetype: archetype_code,
            weapon_rarity: rarity_tier,
            is_headshot: hit.is_headshot,
            impact_x: ix,
            impact_y: iy,
            impact_z: iz,
        });

        if cap_granted(caps, CapMask::LOOT_DROPS) {
            let loot_handle = derive_loot_handle(hit.target_handle, tick, stats.affix_bitfield);
            events.loot_drops.push(LootDropEvent {
                target_handle: hit.target_handle,
                loot_handle,
                rarity_tier,
            });
        }
    }

    events
}

/// Integration variant : caller specifies `WeaponStats` + `AmmoState`.
pub fn step_combat_with_weapon_and_ammo(
    input: InputState,
    crystals: &mut [Crystal],
    weapon: WeaponStats,
    ammo: &mut AmmoState,
    caps: u32,
    tick: u64,
    state: &mut CombatState,
) -> CombatEvents {
    let mut events = CombatEvents::empty();
    events.fired_at_unix_ms = tick;

    if input.reload_pressed
        && !ammo.is_reloading
        && ammo.mag_current < ammo.mag_capacity
        && ammo.reserve > 0
    {
        ammo.is_reloading = true;
        ammo.reload_started_ms = tick;
        events.reloads_started += 1;
    }

    if ammo.is_reloading {
        let elapsed_ms = tick.saturating_sub(ammo.reload_started_ms);
        let secs = if weapon.is_valid && weapon.reload_secs > 0.0 {
            weapon.reload_secs
        } else {
            1.5
        };
        let secs_ms = (secs * 1000.0) as u64;
        if elapsed_ms >= secs_ms {
            let needed = ammo.mag_capacity - ammo.mag_current;
            let take = needed.min(ammo.reserve);
            ammo.mag_current += take;
            ammo.reserve -= take;
            ammo.is_reloading = false;
            ammo.reload_started_ms = 0;
            events.reloads_completed += 1;
        }
    }

    if !input.fire_pressed {
        return events;
    }
    if !cap_granted(caps, CapMask::WEAPONS) {
        events.cap_denied_count += 1;
        return events;
    }

    if ammo.is_reloading || ammo.mag_current == 0 {
        events.ammo_empty_events += 1;
        return events;
    }
    ammo.mag_current -= 1;

    if !weapon.is_valid {
        ammo.mag_current += 1;
        return events;
    }

    let yaw = input.yaw_rad + input.aim_assist_yaw;
    let pitch = input.pitch_rad + input.aim_assist_pitch;
    let ray = Ray::from_camera(
        input.origin_x,
        input.origin_y,
        input.origin_z,
        yaw,
        pitch,
        weapon.max_range_m,
    );
    events.bullets_emitted += 1;

    let Some(hit) = raycast_crystals(&ray, crystals) else {
        return events;
    };

    let raw_damage = weapon.base_damage;
    let after_falloff = apply_falloff(raw_damage, hit.hit_t_m, weapon.damage_falloff, weapon.max_range_m);
    let after_head = apply_headshot(
        after_falloff,
        hit.is_headshot,
        cap_granted(caps, CapMask::CRITICAL_HITS),
        weapon.crit_mult,
    );

    let len_sq = ray.dir_x * ray.dir_x + ray.dir_y * ray.dir_y + ray.dir_z * ray.dir_z;
    let len = if len_sq > 0.0 { len_sq.sqrt() } else { 1.0 };
    let dx = ray.dir_x / len;
    let dy = ray.dir_y / len;
    let dz = ray.dir_z / len;
    let ix = ray.origin_x + dx * hit.hit_t_m;
    let iy = ray.origin_y + dy * hit.hit_t_m;
    let iz = ray.origin_z + dz * hit.hit_t_m;

    let (pre_hp, post_hp) = state.hp.apply(hit.target_handle, after_head);

    events.damage_events.push(DamageEvent {
        target_handle: hit.target_handle,
        damage_dealt: after_head,
        is_headshot: hit.is_headshot,
        impact_x: ix,
        impact_y: iy,
        impact_z: iz,
        pre_hp,
        post_hp,
    });

    if post_hp == 0 && pre_hp != 0 && !state.already_dead.contains(&hit.target_handle) {
        state.already_dead.insert(hit.target_handle);
        let archetype_code = weapon.archetype as u32;
        let rarity_tier = weapon.rarity as u8;
        events.deaths.push(DeathEvent {
            target_handle: hit.target_handle,
            weapon_archetype: archetype_code,
            weapon_rarity: rarity_tier,
            is_headshot: hit.is_headshot,
            impact_x: ix,
            impact_y: iy,
            impact_z: iz,
        });
        if cap_granted(caps, CapMask::LOOT_DROPS) {
            let loot_handle = derive_loot_handle(hit.target_handle, tick, weapon.affix_bitfield);
            events.loot_drops.push(LootDropEvent {
                target_handle: hit.target_handle,
                loot_handle,
                rarity_tier,
            });
        }
    }

    events
}

/// Derive deterministic loot-handle from `(target × tick × affix-bitfield)`.
pub fn derive_loot_handle(target_handle: u32, tick: u64, affix_bitfield: u64) -> u64 {
    let mut h = blake3::Hasher::new();
    h.update(b"loot-handle-v1");
    h.update(&target_handle.to_le_bytes());
    h.update(&tick.to_le_bytes());
    h.update(&affix_bitfield.to_le_bytes());
    let d: [u8; 32] = h.finalize().into();
    u64::from_le_bytes(d[0..8].try_into().unwrap())
}

// ══════════════════════════════════════════════════════════════════════════
// § Integration tests
// ══════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;
    use cssl_host_crystallization::{Crystal, CrystalClass, WorldPos};

    fn entity_at(x_m: f32, y_m: f32, z_m: f32, seed: u64) -> Crystal {
        Crystal::allocate(
            CrystalClass::Entity,
            seed,
            WorldPos::new((x_m * 1000.0) as i32, (y_m * 1000.0) as i32, (z_m * 1000.0) as i32),
        )
    }

    #[test]
    fn cap_denied_emits_no_bullets() {
        let input = InputState::fire_forward(0.0, 0.0, 0.0);
        let mut crystals = vec![entity_at(0.0, 0.0, 5.0, 1)];
        let events = step_combat(input, &mut crystals, 0u32, 0);
        assert_eq!(events.bullets_emitted, 0);
        assert_eq!(events.cap_denied_count, 1);
        assert!(events.damage_events.is_empty());
    }

    #[test]
    fn cap_granted_no_fire_input_no_bullets() {
        let input = InputState::idle();
        let mut crystals = vec![entity_at(0.0, 0.0, 5.0, 1)];
        let events = step_combat(input, &mut crystals, CapMask::WEAPONS, 0);
        assert_eq!(events.bullets_emitted, 0);
        assert_eq!(events.cap_denied_count, 0);
    }

    #[test]
    fn cap_granted_miss_emits_bullet_no_damage() {
        let input = InputState {
            fire_pressed: true,
            origin_y: 100.0,
            ..InputState::idle()
        };
        let mut crystals = vec![entity_at(0.0, 0.0, 5.0, 1)];
        let events = step_combat(input, &mut crystals, CapMask::WEAPONS, 0);
        assert_eq!(events.bullets_emitted, 1);
        assert!(events.damage_events.is_empty());
    }

    #[test]
    fn cap_granted_hit_emits_damage_event() {
        let input = InputState::fire_forward(0.0, 0.0, 0.0);
        let mut crystals = vec![entity_at(0.0, 0.0, 5.0, 42)];
        let events = step_combat(input, &mut crystals, CapMask::WEAPONS, 0);
        assert_eq!(events.bullets_emitted, 1);
        assert_eq!(events.damage_events.len(), 1);
        let de = &events.damage_events[0];
        assert!(de.damage_dealt > 0.0);
        assert!(de.pre_hp > de.post_hp);
        assert_eq!(de.pre_hp, DEFAULT_NPC_HP);
    }

    #[test]
    fn lethal_damage_emits_death_event() {
        let mut stats = WeaponStats::starter();
        stats.base_damage = 200.0;
        stats.archetype = Archetype::Sniper;
        let mut ammo = AmmoState::full(30, 90);
        let mut state = CombatState::new();
        let input = InputState::fire_forward(0.0, 0.0, 0.0);
        let mut crystals = vec![entity_at(0.0, 0.0, 5.0, 99)];
        let events = step_combat_with_weapon_and_ammo(
            input,
            &mut crystals,
            stats,
            &mut ammo,
            CapMask::WEAPONS,
            0,
            &mut state,
        );
        assert_eq!(events.damage_events.len(), 1);
        assert_eq!(events.deaths.len(), 1);
        assert_eq!(events.deaths[0].target_handle, crystals[0].handle);
    }

    #[test]
    fn death_with_loot_cap_emits_loot_drop() {
        let mut stats = WeaponStats::starter();
        stats.base_damage = 200.0;
        let mut ammo = AmmoState::full(30, 90);
        let mut state = CombatState::new();
        let input = InputState::fire_forward(0.0, 0.0, 0.0);
        let mut crystals = vec![entity_at(0.0, 0.0, 5.0, 99)];
        let caps = CapMask::WEAPONS | CapMask::LOOT_DROPS;
        let events = step_combat_with_weapon_and_ammo(
            input,
            &mut crystals,
            stats,
            &mut ammo,
            caps,
            7,
            &mut state,
        );
        assert_eq!(events.loot_drops.len(), 1);
        let again = derive_loot_handle(crystals[0].handle, 7, stats.affix_bitfield);
        assert_eq!(events.loot_drops[0].loot_handle, again);
    }

    #[test]
    fn death_without_loot_cap_no_drop() {
        let mut stats = WeaponStats::starter();
        stats.base_damage = 200.0;
        let mut ammo = AmmoState::full(30, 90);
        let mut state = CombatState::new();
        let input = InputState::fire_forward(0.0, 0.0, 0.0);
        let mut crystals = vec![entity_at(0.0, 0.0, 5.0, 99)];
        let events = step_combat_with_weapon_and_ammo(
            input,
            &mut crystals,
            stats,
            &mut ammo,
            CapMask::WEAPONS,
            0,
            &mut state,
        );
        assert_eq!(events.deaths.len(), 1);
        assert!(events.loot_drops.is_empty());
    }

    #[test]
    fn ammo_decrements_on_fire() {
        let stats = WeaponStats::starter();
        let mut ammo = AmmoState::full(30, 90);
        let mut state = CombatState::new();
        let input = InputState::fire_forward(0.0, 0.0, 0.0);
        let mut crystals = vec![entity_at(100.0, 100.0, 100.0, 1)];
        let _ = step_combat_with_weapon_and_ammo(
            input,
            &mut crystals,
            stats,
            &mut ammo,
            CapMask::WEAPONS,
            0,
            &mut state,
        );
        assert_eq!(ammo.mag_current, 29);
    }

    #[test]
    fn empty_mag_emits_ammo_empty_no_bullet() {
        let stats = WeaponStats::starter();
        let mut ammo = AmmoState::empty(30, 90);
        let mut state = CombatState::new();
        let input = InputState::fire_forward(0.0, 0.0, 0.0);
        let mut crystals = vec![entity_at(0.0, 0.0, 5.0, 1)];
        let events = step_combat_with_weapon_and_ammo(
            input,
            &mut crystals,
            stats,
            &mut ammo,
            CapMask::WEAPONS,
            0,
            &mut state,
        );
        assert_eq!(events.bullets_emitted, 0);
        assert_eq!(events.ammo_empty_events, 1);
    }

    #[test]
    fn death_event_idempotent_across_frames() {
        let mut stats = WeaponStats::starter();
        stats.base_damage = 200.0;
        let mut ammo = AmmoState::full(30, 90);
        let mut state = CombatState::new();
        let input = InputState::fire_forward(0.0, 0.0, 0.0);
        let mut crystals = vec![entity_at(0.0, 0.0, 5.0, 99)];

        let f1 = step_combat_with_weapon_and_ammo(
            input,
            &mut crystals,
            stats,
            &mut ammo,
            CapMask::WEAPONS,
            0,
            &mut state,
        );
        assert_eq!(f1.deaths.len(), 1);

        let f2 = step_combat_with_weapon_and_ammo(
            input,
            &mut crystals,
            stats,
            &mut ammo,
            CapMask::WEAPONS,
            1,
            &mut state,
        );
        assert!(f2.deaths.is_empty());
    }

    #[test]
    fn replay_determinism_same_inputs_same_events() {
        let input = InputState::fire_forward(0.0, 0.0, 0.0);
        let mut crystals_a = vec![entity_at(0.0, 0.0, 5.0, 7)];
        let mut crystals_b = vec![entity_at(0.0, 0.0, 5.0, 7)];
        let ev_a = step_combat(input, &mut crystals_a, CapMask::WEAPONS, 42);
        let ev_b = step_combat(input, &mut crystals_b, CapMask::WEAPONS, 42);
        assert_eq!(ev_a.bullets_emitted, ev_b.bullets_emitted);
        assert_eq!(ev_a.damage_events.len(), ev_b.damage_events.len());
        if !ev_a.damage_events.is_empty() {
            let a = &ev_a.damage_events[0];
            let b = &ev_b.damage_events[0];
            assert_eq!(a.damage_dealt.to_bits(), b.damage_dealt.to_bits());
            assert_eq!(a.post_hp, b.post_hp);
        }
    }

    #[test]
    fn caps_bitmask_constants_canonical() {
        assert_eq!(CapMask::WEAPONS, 1);
        assert_eq!(CapMask::LOOT_DROPS, 2);
        assert_eq!(CapMask::FRIENDLY_FIRE, 4);
        assert_eq!(CapMask::CRITICAL_HITS, 8);
        assert!(cap_granted(CapMask::WEAPONS | CapMask::LOOT_DROPS, CapMask::WEAPONS));
        assert!(!cap_granted(CapMask::LOOT_DROPS, CapMask::WEAPONS));
    }
}
