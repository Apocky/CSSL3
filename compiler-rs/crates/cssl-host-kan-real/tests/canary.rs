//! § Integration tests : CanaryGate enrollment + disagreement classification.
//!
//! 5 tests per task spec.

#![allow(clippy::manual_range_contains)]

use cssl_host_kan_real::{CanaryGate, DisagreementKind};
use cssl_host_kan_substrate_bridge::{IntentClass, SeedCell};

#[test]
fn enrollment_pct_zero_excludes_all() {
    let g = CanaryGate::with_pct(0);
    for s in 0..1000 {
        let id = format!("session-{s}");
        assert!(!g.enrolled(&id));
    }
}

#[test]
fn enrollment_pct_one_hundred_includes_all() {
    let g = CanaryGate::with_pct(100);
    for s in 0..1000 {
        let id = format!("session-{s}");
        assert!(g.enrolled(&id));
    }
}

#[test]
fn enrollment_default_ten_pct_roughly_matches() {
    let g = CanaryGate::default();
    let mut hits = 0;
    let n = 1000_u32;
    for s in 0..n {
        let id = format!("session-{s}");
        if g.enrolled(&id) {
            hits += 1;
        }
    }
    // Expect ≈ 10% of sessions enrolled. Tolerance band [5%, 15%] for
    // hash-spread variance.
    let pct = (hits as f32) / (n as f32) * 100.0;
    assert!(
        pct >= 5.0 && pct <= 15.0,
        "expected ~10%, got {pct}% ({hits} / {n})"
    );
}

#[test]
fn intent_disagreement_classifies_kind_mismatch() {
    let stage0 = IntentClass {
        kind: "move".to_string(),
        confidence: 0.9,
        args: vec![],
    };
    let stage1 = IntentClass {
        kind: "examine".to_string(),
        confidence: 0.8,
        args: vec![],
    };
    let d = CanaryGate::intent_disagreement(&stage0, &stage1);
    matches!(d, DisagreementKind::IntentKind { .. });
    if let DisagreementKind::IntentKind {
        stage0_kind,
        stage1_kind,
    } = d
    {
        assert_eq!(stage0_kind, "move");
        assert_eq!(stage1_kind, "examine");
    } else {
        panic!("expected IntentKind disagreement");
    }
}

#[test]
fn seed_disagreement_cardinality_branch() {
    let stage0 = vec![SeedCell::new(1, 0, 0, 0, 0.5, 10)];
    let stage1 = vec![
        SeedCell::new(1, 0, 0, 0, 0.5, 10),
        SeedCell::new(2, 1, 1, 1, 0.6, 20),
    ];
    let d = CanaryGate::seed_disagreement(&stage0, &stage1);
    if let DisagreementKind::Cardinality { stage0, stage1 } = d {
        assert_eq!(stage0, 1);
        assert_eq!(stage1, 2);
    } else {
        panic!("expected Cardinality disagreement");
    }
    // Score-disagreement above threshold.
    let d2 = CanaryGate::score_disagreement(0.1, 0.9);
    matches!(d2, DisagreementKind::Score { .. });
    // Score-disagreement below threshold.
    let d3 = CanaryGate::score_disagreement(0.5, 0.51);
    assert_eq!(d3, DisagreementKind::Agree);
}
