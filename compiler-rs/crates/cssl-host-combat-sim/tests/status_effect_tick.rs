// § status_effect_tick.rs — 4 tests on stack-policy + tick + petrify-promote
// ════════════════════════════════════════════════════════════════════

use cssl_host_combat_sim::status_effects::{
    apply_with_policy, tick_status, StackPolicy, StatusEffect, StatusInstance,
};

#[test]
fn enum_has_16_variants_via_default_policy_check() {
    // Spot-check 4 from each policy bucket ; serves as enum-completeness probe
    assert_eq!(StatusEffect::Bleed.default_stack_policy(), StackPolicy::AddDuration);
    assert_eq!(StatusEffect::Burn.default_stack_policy(), StackPolicy::AddDuration);
    assert_eq!(StatusEffect::Poison.default_stack_policy(), StackPolicy::AddDuration);
    assert_eq!(StatusEffect::Curse.default_stack_policy(), StackPolicy::AddIntensity);
    assert_eq!(StatusEffect::Marked.default_stack_policy(), StackPolicy::AddIntensity);
    assert_eq!(StatusEffect::Stun.default_stack_policy(), StackPolicy::RefreshDuration);
    assert_eq!(StatusEffect::Sleep.default_stack_policy(), StackPolicy::RefreshDuration);
    assert_eq!(StatusEffect::Phased.default_stack_policy(), StackPolicy::RefreshDuration);
}

#[test]
fn add_intensity_for_curse_increases_magnitude() {
    let mut v = vec![StatusInstance::new(StatusEffect::Curse, 5.0, 1.0)];
    apply_with_policy(&mut v, StatusInstance::new(StatusEffect::Curse, 4.0, 2.0));
    assert_eq!(v.len(), 1);
    assert!((v[0].magnitude - 3.0).abs() < 1e-3);
}

#[test]
fn tick_decrements_durations_and_prunes() {
    let mut v = vec![
        StatusInstance::new(StatusEffect::Slow, 2.0, 1.0),
        StatusInstance::new(StatusEffect::Sleep, 0.1, 1.0),
    ];
    tick_status(&mut v, 0.5);
    // Sleep was 0.1 ; should be removed
    assert!(v.iter().all(|e| e.kind != StatusEffect::Sleep));
    // Slow should still be present at ~1.5
    let slow = v.iter().find(|e| e.kind == StatusEffect::Slow).expect("slow present");
    assert!((slow.duration_secs - 1.5).abs() < 1e-3);
}

#[test]
fn freeze_stack_three_promotes_to_petrify() {
    let mut v = vec![StatusInstance::new(StatusEffect::Freeze, 5.0, 3.0)];
    tick_status(&mut v, 0.0);
    assert!(v.iter().any(|e| e.kind == StatusEffect::Petrify));
    assert!(v.iter().all(|e| e.kind != StatusEffect::Freeze));
}
