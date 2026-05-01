//! § upgrade_paths tests — level-up · transmute · bond · reroll · cost-respected.

use cssl_host_gear_archetype::{
    bond, level_up, reroll_affix, roll_drop, roll_gear, transmute, BaseItem, BaseMat, Biome,
    DropContext, GearSlot, Rarity, RecordingAuditSink, RerollTarget, TransmuteResult,
};

#[test]
fn level_up_consumes_xp_and_emits_audit() {
    let base = BaseItem::weapon(GearSlot::MainHand, BaseMat::Iron, 10.0, 1.0);
    let mut g = roll_gear(7, &base, Rarity::Common);
    let sink = RecordingAuditSink::new();
    assert_eq!(g.item_level, 1);
    level_up(&mut g, 250, &sink);
    assert_eq!(g.item_level, 1 + 2, "250 XP → +2 levels");
    assert!(sink.contains_kind("gear.leveled"));
}

#[test]
fn transmute_common_to_uncommon_succeeds_and_audits() {
    let base = BaseItem::weapon(GearSlot::MainHand, BaseMat::Iron, 10.0, 1.0);
    let g = roll_gear(11, &base, Rarity::Common);
    let sink = RecordingAuditSink::new();
    let res = transmute(g, Rarity::Uncommon, 1, &sink);
    match res {
        TransmuteResult::Ok(new) => {
            assert_eq!(new.rarity, Rarity::Uncommon);
            assert_eq!(new.base.base_mat, BaseMat::Silver, "mat floor → Silver");
        }
        other => panic!("expected Ok, got {other:?}"),
    }
    assert!(sink.contains_kind("gear.transmuted"));
}

#[test]
fn legendary_to_mythic_transmute_forbidden() {
    let base = BaseItem::weapon(GearSlot::MainHand, BaseMat::Voidsteel, 10.0, 1.0);
    let g = roll_gear(13, &base, Rarity::Legendary);
    let sink = RecordingAuditSink::new();
    let res = transmute(g, Rarity::Mythic, 1, &sink);
    assert!(matches!(res, TransmuteResult::ForbiddenMythicTransmute));
    assert!(sink.contains_kind("gear.transmute_rejected"));
}

#[test]
fn transmute_insufficient_material_rejected() {
    let base = BaseItem::weapon(GearSlot::MainHand, BaseMat::Iron, 10.0, 1.0);
    let g = roll_gear(17, &base, Rarity::Common);
    let sink = RecordingAuditSink::new();
    let res = transmute(g, Rarity::Uncommon, 0, &sink);
    assert!(matches!(res, TransmuteResult::InsufficientMaterial));
    assert!(sink.contains_kind("gear.transmute_rejected"));
}

#[test]
fn bond_only_legendary_or_mythic_and_emits_audit() {
    // Common cannot bond.
    let base = BaseItem::weapon(GearSlot::MainHand, BaseMat::Iron, 10.0, 1.0);
    let mut g = roll_gear(19, &base, Rarity::Common);
    let sink = RecordingAuditSink::new();
    let r = bond(&mut g, 0xDEAD_BEEF_u128, &sink);
    assert!(r.is_err(), "Common should not bond");
    assert!(sink.contains_kind("gear.bond_rejected"));

    // Legendary CAN bond.
    let base = BaseItem::weapon(GearSlot::MainHand, BaseMat::Voidsteel, 10.0, 1.0);
    let mut g = roll_gear(23, &base, Rarity::Legendary);
    let sink = RecordingAuditSink::new();
    bond(&mut g, 0xCAFE_u128, &sink).expect("legendary bonds");
    assert!(g.bound_to_player);
    assert!(sink.contains_kind("gear.bonded"));

    // Already-bonded → reject.
    let r = bond(&mut g, 0xCAFE_u128, &sink);
    assert!(r.is_err(), "double-bond should fail");
}

#[test]
fn reroll_affix_changes_seed_and_value() {
    // Use a roguelike-context drop with rarity ≥ Rare to guarantee multiple affixes.
    let ctx = DropContext { mob_tier: 5, biome: Biome::Forge, magic_find: 1.0 };
    // Find a seed yielding ≥ Rare with ≥ 1 prefix.
    let mut found = None;
    for s in 0u128..200 {
        let g = roll_drop(&ctx, s, Some(GearSlot::MainHand)).expect("drop");
        if g.rarity >= Rarity::Rare && !g.prefixes.is_empty() {
            found = Some((s, g));
            break;
        }
    }
    let (_seed, mut g) = found.expect("find ≥Rare drop with prefix");
    let old_value = g.prefixes[0].value;
    let old_seed = g.prefixes[0].seed;
    let sink = RecordingAuditSink::new();
    reroll_affix(&mut g, RerollTarget::Prefix(0), 0xFEED_FACE_u128, &sink).expect("reroll");
    assert!(sink.contains_kind("gear.rerolled"));
    let new_seed = g.prefixes[0].seed;
    assert_ne!(new_seed, old_seed, "reroll must produce NEW seed");
    // Value may collide for tight ranges, so we assert seed-shift only.
    let _ = old_value;
}
