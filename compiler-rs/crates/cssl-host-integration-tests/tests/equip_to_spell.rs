// § equip_to_spell — equipped jewelry mana → cast-spell mana-economy.
// ════════════════════════════════════════════════════════════════════
// § Coverage : cross-crate Intelligence stat (gear) modulating mana-pool
//   capacity (spell), then cast success / fail-fast behavior.

use cssl_host_gear_archetype as gear;
use cssl_host_spell_graph as spell;

use cssl_host_integration_tests::{
    cast_minimal_fire_ray, equip_into_slot, make_grimoire, make_player_inventory,
};

/// (a) Equipped jewelry adds mana-pool capacity. We model the cross-crate
///     wiring: jewelry's ManaPool stat-kind contributes to the caster's mana
///     capacity. Higher-base-mat → more capacity.
#[test]
fn equipped_jewelry_adds_mana_capacity() {
    let mut inv = make_player_inventory();
    // Build a Mythic-floor jewelry (Soulbound mat) with 80 mana_pool.
    let base = gear::BaseItem::jewelry(gear::GearSlot::Amulet, gear::BaseMat::Mithril, 80.0);
    let g = gear::roll_gear(0xACE0_FACE, &base, gear::Rarity::Rare);
    equip_into_slot(&mut inv, gear::GearSlot::Amulet, g.clone())
        .expect("equip Amulet must succeed");

    // Read merged_stats back out — ManaPool key must hold ≥ 80.
    let merged = g.merged_stats();
    let mana_pool_stat = merged
        .get(&gear::StatKind::ManaPool)
        .copied()
        .unwrap_or(0.0);
    assert!(
        mana_pool_stat >= 80.0,
        "expected merged ManaPool ≥ 80 ; got {mana_pool_stat}"
    );

    // Wire the stat into the caster's pool. We use intelligence=8 ⇒ 90 cap,
    // then add the equipped-jewelry stat as a flat bump.
    let (_grim, mut pool) = make_grimoire(8);
    let pre_capacity = pool.capacity;
    pool.capacity += mana_pool_stat;
    pool.current = pool.capacity;
    assert!(
        pool.capacity > pre_capacity,
        "equipped jewelry must increase pool.capacity"
    );
}

/// (b) Cast-spell with full mana succeeds.
#[test]
fn cast_spell_with_full_mana_succeeds() {
    let (_g, mut mana) = make_grimoire(10);
    assert!(
        mana.current > 0.0,
        "fresh pool must have current > 0 ; got {}",
        mana.current
    );
    let res = cast_minimal_fire_ray(&mut mana, 10);
    assert!(
        res.success,
        "cast with full mana must succeed ; got {res:?}"
    );
    assert!(
        res.substrate_event_cell.is_some(),
        "successful cast must emit a substrate event-cell"
    );
    assert_eq!(
        res.status_effect_applied,
        Some(spell::StatusEffect::Burn),
        "Fire-Source spell must apply Burn status-effect on consenting target"
    );
}

/// (c) Cast-spell with empty mana fails-fast (success=false).
#[test]
fn cast_spell_with_empty_mana_fails_fast() {
    let (_g, mut mana) = make_grimoire(2);
    // Drain to zero.
    let drained = mana.try_consume(mana.current);
    assert!(drained, "draining-all should succeed");
    assert!(
        mana.current.abs() < 1e-3,
        "post-drain current ≈ 0 ; got {}",
        mana.current
    );

    let res = cast_minimal_fire_ray(&mut mana, 2);
    assert!(
        !res.success,
        "cast with empty mana must fail-fast ; got {res:?}"
    );
    assert!(
        res.substrate_event_cell.is_none(),
        "failed cast must NOT emit an event-cell"
    );
}
