// § damage_falloff.rs — falloff curve correctness across full range
// ════════════════════════════════════════════════════════════════════

use cssl_host_weapons::{damage_falloff, ArmorClass, DamageType, HitZone, compute_damage};

#[test]
fn falloff_full_strength_at_min_range() {
    let m = damage_falloff(0.0, 10.0, 50.0, 0.4);
    assert!((m - 1.0).abs() < f32::EPSILON);
    let m2 = damage_falloff(10.0, 10.0, 50.0, 0.4);
    assert!((m2 - 1.0).abs() < f32::EPSILON);
}

#[test]
fn falloff_floor_at_max_range_and_beyond() {
    let m = damage_falloff(50.0, 10.0, 50.0, 0.4);
    assert!((m - 0.4).abs() < f32::EPSILON);
    let m2 = damage_falloff(1000.0, 10.0, 50.0, 0.4);
    assert!((m2 - 0.4).abs() < f32::EPSILON);
}

#[test]
fn falloff_monotone_decreasing_in_range() {
    let mut prev = 1.0_f32;
    let mut d = 0.0;
    while d <= 100.0 {
        let m = damage_falloff(d, 10.0, 50.0, 0.4);
        assert!(m <= prev + 1e-5);
        prev = m;
        d += 1.0;
    }
}

#[test]
fn falloff_compounds_with_armor_modifier() {
    // Far + plate = significantly reduced from near + flesh.
    let near = compute_damage(
        100.0 * damage_falloff(5.0, 10.0, 50.0, 0.4),
        HitZone::Body,
        DamageType::Kinetic,
        ArmorClass::Flesh,
        false,
    );
    let far = compute_damage(
        100.0 * damage_falloff(80.0, 10.0, 50.0, 0.4),
        HitZone::Body,
        DamageType::Kinetic,
        ArmorClass::Plate,
        false,
    );
    assert!(near.final_dmg > far.final_dmg);
}

#[test]
fn falloff_handles_nan_and_negative_distance() {
    let m_neg = damage_falloff(-10.0, 10.0, 50.0, 0.4);
    assert!((m_neg - 1.0).abs() < f32::EPSILON);

    let m_nan = damage_falloff(f32::NAN, 10.0, 50.0, 0.4);
    assert!(m_nan.is_finite());
}
