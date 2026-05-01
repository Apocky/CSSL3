//! § Integration tests : RealIntentKanClassifier end-to-end.
//!
//! 8 tests per task spec.

#![allow(clippy::manual_range_contains)]

use cssl_host_kan_real::{
    IntentLabel, RealIntentKanClassifier, INTENT_LABEL_COUNT,
};
use cssl_host_kan_substrate_bridge::{
    intent_classifier::Stage0HeuristicClassifier, IntentClassifier,
};

fn fresh_fb() -> Box<dyn IntentClassifier> {
    Box::new(Stage0HeuristicClassifier::default_rules())
}

#[test]
fn classifies_known_intent_returns_canonical_label() {
    let c = RealIntentKanClassifier::with_baked_seed(101, fresh_fb());
    let r = c.classify("examine the altar");
    let known = [
        "move", "talk", "examine", "cocreate", "use", "drop", "wait", "unknown",
    ];
    assert!(known.contains(&r.kind.as_str()), "unexpected kind: {}", r.kind);
}

#[test]
fn classifies_handles_empty_string() {
    let c = RealIntentKanClassifier::with_baked_seed(101, fresh_fb());
    let r = c.classify("");
    assert!(r.confidence >= 0.0 && r.confidence <= 1.0);
    assert!(r.confidence.is_finite());
}

#[test]
fn determinism_same_input_same_output() {
    let c1 = RealIntentKanClassifier::with_baked_seed(7, fresh_fb());
    let c2 = RealIntentKanClassifier::with_baked_seed(7, fresh_fb());
    let r1 = c1.classify("walk forward please");
    let r2 = c2.classify("walk forward please");
    assert_eq!(r1.kind, r2.kind);
    assert_eq!(r1.confidence, r2.confidence);
    assert_eq!(r1.args, r2.args);
}

#[test]
fn confidence_clamped_unit_interval() {
    let c = RealIntentKanClassifier::with_baked_seed(13, fresh_fb());
    for input in [
        "",
        "a",
        "the quick brown fox",
        "ALL CAPS HERE",
        "punctuation!!!",
        "1234 56789",
    ] {
        let r = c.classify(input);
        assert!(
            r.confidence >= 0.0 && r.confidence <= 1.0,
            "conf out of range for '{input}' : {}",
            r.confidence
        );
        assert!(r.confidence.is_finite());
    }
}

#[test]
fn pure_fallback_uses_stage0_keyword_rules() {
    let c = RealIntentKanClassifier::pure_fallback(fresh_fb());
    // Stage-0 has rules for "walk" → "move" + "examine" → "examine".
    assert_eq!(c.classify("walk forward").kind, "move");
    assert_eq!(c.classify("examine altar").kind, "examine");
    assert_eq!(c.classify("zzz nonsense").kind, "unknown");
}

#[test]
fn args_carry_backend_and_label_index() {
    let c = RealIntentKanClassifier::with_baked_seed(31, fresh_fb());
    let r = c.classify("hello world");
    let backend = r.args.iter().find(|(k, _)| k == "backend");
    assert!(backend.is_some());
    let label_idx = r.args.iter().find(|(k, _)| k == "label_index");
    assert!(label_idx.is_some());
    let idx: usize = label_idx.unwrap().1.parse().unwrap();
    assert!(idx < INTENT_LABEL_COUNT);
}

#[test]
fn name_contract_stable() {
    let c = RealIntentKanClassifier::with_baked_seed(0, fresh_fb());
    assert_eq!(c.name(), "stage1-kan-real-intent");
}

#[test]
fn intent_label_enum_covers_all_indices() {
    for i in 0..INTENT_LABEL_COUNT {
        let lbl = IntentLabel::from_index(i);
        assert!(!lbl.as_str().is_empty());
    }
    // Out-of-range index ⇒ Unknown.
    let oor = IntentLabel::from_index(999);
    assert_eq!(oor, IntentLabel::Unknown);
    assert_eq!(oor.as_str(), "unknown");
}
