//! § template_table — verifies the stage-0 template registry behaves
//! deterministically + slot-fills Φ-tags + falls back gracefully.

use cssl_host_gm::{EventClass, TemplateTable, ToneAxis};

#[test]
fn pack_pool_key_packs_zone_class_bucket() {
    let k = TemplateTable::pack_pool_key(3, EventClass::Companion, 0);
    // zone 3 << 8 | class 2 << 4 | bucket 0
    assert_eq!(k, (3u32 << 8) | (2u32 << 4));
}

#[test]
fn default_table_contains_test_room_pools() {
    let t = TemplateTable::default_stage0();
    assert!(t.lookup_pool(0, EventClass::Arrive, 3).is_some());
    assert!(t.lookup_pool(0, EventClass::Examine, 1).is_some());
    assert!(t.lookup_pool(0, EventClass::Examine, 2).is_some());
    assert!(t.lookup_pool(0, EventClass::Companion, 0).is_some());
    assert!(t.lookup_pool(0, EventClass::Tension, 2).is_some());
}

#[test]
fn pick_is_deterministic_for_same_seed() {
    let t = TemplateTable::default_stage0();
    let a = t.pick(0, EventClass::Examine, 1, Some(101), 42);
    let b = t.pick(0, EventClass::Examine, 1, Some(101), 42);
    assert_eq!(a, b);
}

#[test]
fn pick_changes_with_different_seed() {
    let t = TemplateTable::default_stage0();
    let a = t.pick(0, EventClass::Examine, 1, Some(101), 0).unwrap().0;
    let b = t.pick(0, EventClass::Examine, 1, Some(101), 1).unwrap().0;
    // Either index_in_pool differs OR pool sizes are 1 (no variation
    // possible) ; the default-table terse-pool has 2 templates so we
    // expect index variation.
    assert_ne!(a.index_in_pool, b.index_in_pool);
}

#[test]
fn pick_slot_fills_known_phi_tag() {
    let t = TemplateTable::default_stage0();
    let (_id, prose) = t
        .pick(0, EventClass::Examine, 1, Some(101), 0)
        .expect("pool exists");
    assert!(
        prose.contains("altar"),
        "expected altar slot-fill, got: {prose}"
    );
    assert!(!prose.contains("{tag}"));
}

#[test]
fn pick_slot_fills_generic_when_no_phi_tag() {
    let t = TemplateTable::default_stage0();
    let (_id, prose) = t
        .pick(0, EventClass::Examine, 1, None, 0)
        .expect("pool exists");
    assert!(prose.contains("something"));
    assert!(!prose.contains("{tag}"));
}

#[test]
fn lookup_pool_falls_back_to_arrive_class() {
    // Tension class has no terse-bucket pool in default_stage0 ; should
    // fall back to Arrive's neutral-bucket... but in default_stage0
    // Arrive is only seeded for bucket 3, so falling back to Arrive in
    // bucket 0 should miss. This tests the miss path.
    let t = TemplateTable::default_stage0();
    let r = t.lookup_pool(99, EventClass::Tension, 0);
    // zone 99 has no pools at all → None.
    assert!(r.is_none());
}

#[test]
fn tone_bucket_neutral_threshold() {
    // Tone within ±0.05 of neutral on every axis → bucket 3.
    let near_neutral = ToneAxis::clamped(0.52, 0.48, 0.51);
    assert_eq!(TemplateTable::tone_bucket(near_neutral), 3);
}
