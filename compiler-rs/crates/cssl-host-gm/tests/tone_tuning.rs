//! § tone_tuning — exercises cocreative-bias-vec → ToneAxis mapping.

#![allow(clippy::float_cmp)]

use cssl_host_cocreative::bias::BiasVector;
use cssl_host_gm::{
    GameMaster, GmCapTable, NullAuditSink, Stage0PacingPolicy, TemplateTable, ToneAxis,
};

fn build_gm(caps: GmCapTable) -> GameMaster {
    GameMaster::new(
        caps,
        TemplateTable::default_stage0(),
        Box::new(Stage0PacingPolicy),
        Box::new(NullAuditSink),
        1,
    )
}

#[test]
fn tone_tune_maps_first_three_theta_to_axes() {
    let gm = build_gm(GmCapTable::all());
    let bias = BiasVector::from_slice(&[0.3_f32, -0.2, 0.4]);
    let tone = gm.tune_tone(&bias);
    // warm = 0.5 + 0.3 = 0.8 ; terse = 0.5 - 0.2 = 0.3 ; poetic = 0.9
    assert!((tone.warm - 0.8).abs() < 1e-5);
    assert!((tone.terse - 0.3).abs() < 1e-5);
    assert!((tone.poetic - 0.9).abs() < 1e-5);
}

#[test]
fn tone_tune_clamps_extreme_bias() {
    let gm = build_gm(GmCapTable::all());
    let bias = BiasVector::from_slice(&[5.0_f32, -5.0, 5.0]);
    let tone = gm.tune_tone(&bias);
    assert_eq!(tone.warm, 1.0);
    assert_eq!(tone.terse, 0.0);
    assert_eq!(tone.poetic, 1.0);
}

#[test]
fn tone_tune_short_bias_leaves_missing_axes_neutral() {
    let gm = build_gm(GmCapTable::all());
    let bias = BiasVector::from_slice(&[0.2_f32]);
    let tone = gm.tune_tone(&bias);
    assert!((tone.warm - 0.7).abs() < 1e-5);
    assert!((tone.terse - 0.5).abs() < 1e-5);
    assert!((tone.poetic - 0.5).abs() < 1e-5);
}

#[test]
fn tone_tune_zero_dim_bias_stays_neutral() {
    let gm = build_gm(GmCapTable::all());
    let bias = BiasVector::new(0);
    let tone = gm.tune_tone(&bias);
    assert_eq!(tone, ToneAxis::neutral());
}

#[test]
fn tone_tune_deterministic_across_calls() {
    let gm = build_gm(GmCapTable::all());
    let bias = BiasVector::from_slice(&[0.1, 0.2, 0.3, 0.4]);
    let a = gm.tune_tone(&bias);
    let b = gm.tune_tone(&bias);
    assert_eq!(a, b);
}
