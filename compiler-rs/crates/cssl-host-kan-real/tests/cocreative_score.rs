//! § Integration tests : RealCocreativeKanClassifier end-to-end.
//!
//! 6 tests per task spec.

#![allow(clippy::manual_range_contains)]
#![allow(clippy::redundant_clone)]

use cssl_host_kan_real::RealCocreativeKanClassifier;
use cssl_host_kan_substrate_bridge::cocreative_classifier::{
    CocreativeClassifier, Stage0DotProductClassifier,
};

fn fresh_fb() -> Box<dyn CocreativeClassifier> {
    Box::new(Stage0DotProductClassifier::default_dim4())
}

#[test]
fn score_in_unit_interval() {
    let c = RealCocreativeKanClassifier::with_baked_seed(11, fresh_fb());
    let s = c.score_features(&[0.1, 0.2, 0.3, 0.4]);
    assert!(s >= 0.0 && s <= 1.0);
    assert!(s.is_finite());
}

#[test]
fn determinism_same_seed_same_score() {
    let c1 = RealCocreativeKanClassifier::with_baked_seed(11, fresh_fb());
    let c2 = RealCocreativeKanClassifier::with_baked_seed(11, fresh_fb());
    let s1 = c1.score_features(&[0.5; 16]);
    let s2 = c2.score_features(&[0.5; 16]);
    assert_eq!(s1, s2);
}

#[test]
fn nan_input_produces_finite_score() {
    let c = RealCocreativeKanClassifier::with_baked_seed(11, fresh_fb());
    let s = c.score_features(&[f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 0.0]);
    assert!(s.is_finite());
    assert!(s >= 0.0 && s <= 1.0);
}

#[test]
fn empty_features_safe() {
    let c = RealCocreativeKanClassifier::with_baked_seed(11, fresh_fb());
    let s = c.score_features(&[]);
    assert!(s.is_finite());
    assert!(s >= 0.0 && s <= 1.0);
}

#[test]
fn rank_indices_form_permutation() {
    let c = RealCocreativeKanClassifier::with_baked_seed(11, fresh_fb());
    let cands = vec![
        vec![0.9, 0.9, 0.9, 0.9],
        vec![0.1, 0.1, 0.1, 0.1],
        vec![0.5, 0.5, 0.5, 0.5],
        vec![0.0; 4],
    ];
    let r = c.rank_candidates(&cands);
    assert_eq!(r.len(), 4);
    let mut sorted = r.clone();
    sorted.sort_unstable();
    assert_eq!(sorted, vec![0, 1, 2, 3]);
}

#[test]
fn pure_fallback_path_uses_stage0() {
    let c = RealCocreativeKanClassifier::pure_fallback(fresh_fb());
    // Stage-0 dot([0.5,0.5,0.5,0.5], [1,1,1,1]) = 2.0 ; clamped to 1.0.
    let s = c.score_features(&[1.0, 1.0, 1.0, 1.0]);
    assert!((s - 1.0).abs() < 1e-6);
}
