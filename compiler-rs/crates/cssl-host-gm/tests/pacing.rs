//! § pacing — exercises stage-0 + stage-1-stub pacing-policy impls.

use cssl_host_gm::{
    PacingKind, PacingPolicy, Stage0PacingPolicy, Stage1KanStubPacingPolicy,
};

#[test]
fn stage0_high_fatigue_emits_fall() {
    let p = Stage0PacingPolicy;
    let h = p.compute_pacing(&[0.7], 0, 0.85);
    assert_eq!(h.kind, PacingKind::Fall);
    assert!(h.idle_allow);
    assert!(h.beat_spacing_ms >= 2000);
}

#[test]
fn stage0_low_fatigue_high_tension_emits_rise() {
    let p = Stage0PacingPolicy;
    let h = p.compute_pacing(&[0.85], 0, 0.1);
    assert_eq!(h.kind, PacingKind::Rise);
    assert!(!h.idle_allow);
    assert!(h.beat_spacing_ms <= 1000);
}

#[test]
fn stage0_default_emits_hold() {
    let p = Stage0PacingPolicy;
    let h = p.compute_pacing(&[0.5], 0, 0.5);
    assert_eq!(h.kind, PacingKind::Hold);
    assert!(h.idle_allow);
}

#[test]
fn stage0_empty_tension_vec_holds_at_default() {
    let p = Stage0PacingPolicy;
    let h = p.compute_pacing(&[], 0, 0.5);
    assert_eq!(h.kind, PacingKind::Hold);
}

#[test]
fn stage1_stub_no_handle_delegates_to_fallback() {
    let s1 = Stage1KanStubPacingPolicy::new(Box::new(Stage0PacingPolicy));
    let h = s1.compute_pacing(&[0.5], 0, 0.5);
    assert_eq!(h.kind, PacingKind::Hold);
    assert_eq!(s1.name(), "stage1-kan-stub");
}

#[test]
fn stage1_stub_with_handle_returns_canned_rise() {
    let s1 = Stage1KanStubPacingPolicy::with_handle(
        Box::new(Stage0PacingPolicy),
        String::from("kan-v0-mock"),
    );
    let h = s1.compute_pacing(&[0.4], 0, 0.5);
    assert_eq!(h.kind, PacingKind::Rise);
    assert!(!h.idle_allow);
}
