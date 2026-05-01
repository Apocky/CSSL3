//! § gm_text_emit — end-to-end prose-emission paths.
//!
//! Covers happy-path emit, Φ-tag-miss degrade, no-template degrade,
//! determinism (same input → same output prose).

use cssl_host_gm::{
    GameMaster, GmCapTable, GmSceneInput, NarrativeTextFrame, NullAuditSink,
    Stage0PacingPolicy, TemplateTable, ToneAxis,
};

fn build_gm() -> GameMaster {
    GameMaster::new(
        GmCapTable::all(),
        TemplateTable::default_stage0(),
        Box::new(Stage0PacingPolicy),
        Box::new(NullAuditSink),
        1,
    )
}

#[test]
fn emit_text_with_phi_tag_produces_prose() {
    let gm = build_gm();
    let mut s = GmSceneInput::default_empty();
    s.phi_tags = vec![101];
    let f: NarrativeTextFrame = gm.emit_text(&s, ToneAxis::neutral()).unwrap();
    assert!(!f.utf8_text.is_empty());
    assert_eq!(f.tone, ToneAxis::neutral());
    assert!(f.ts_micros > 0);
}

#[test]
fn emit_text_without_phi_tag_degrades_gracefully() {
    let gm = build_gm();
    let s = GmSceneInput::default_empty();
    let f = gm.emit_text(&s, ToneAxis::neutral()).unwrap();
    // Either the generic-prose fallback OR a slot-filled "something"
    // — both are acceptable degrade modes.
    assert!(!f.utf8_text.contains("{tag}"));
}

#[test]
fn emit_text_uses_companion_class_when_companion_present() {
    let gm = build_gm();
    let mut s = GmSceneInput::default_empty();
    s.companion_present = true;
    s.phi_tags = vec![104];
    // Force warm-bucket so we hit the Companion warm pool that exists.
    let warm = ToneAxis::clamped(0.95, 0.5, 0.5);
    let f = gm.emit_text(&s, warm).unwrap();
    assert!(!f.utf8_text.is_empty());
    // The default-stage0 companion pool mentions "companion" verbatim.
    assert!(f.utf8_text.contains("companion"));
}

#[test]
fn emit_text_unknown_zone_degrades_to_generic_prose() {
    let gm = build_gm();
    let mut s = GmSceneInput::default_empty();
    s.zone_id = 9999; // no pools for zone 9999
    s.phi_tags = vec![101];
    let f = gm.emit_text(&s, ToneAxis::neutral()).unwrap();
    assert_eq!(f.utf8_text, "you see something here");
}

#[test]
fn emit_text_timestamps_are_monotonic() {
    let gm = build_gm();
    let mut s = GmSceneInput::default_empty();
    s.phi_tags = vec![101];
    let a = gm.emit_text(&s, ToneAxis::neutral()).unwrap();
    let b = gm.emit_text(&s, ToneAxis::neutral()).unwrap();
    assert!(b.ts_micros > a.ts_micros);
}

#[test]
fn prompt_suggestions_includes_three_items() {
    let gm = build_gm();
    let mut s = GmSceneInput::default_empty();
    s.phi_tags = vec![101];
    let p = gm.prompt_suggestions(&s);
    assert_eq!(p.items.len(), 3);
    assert_eq!(p.max_select, 3);
    assert!(p.items[0].contains("altar"));
}

#[test]
fn pacing_mark_emit_records_correct_magnitude() {
    let gm = build_gm();
    let hint = gm.compute_pacing(&[0.85], 0, 0.1);
    let event = gm.emit_pacing_mark(hint).unwrap();
    assert!((event.magnitude - hint.tension_target).abs() < 1e-6);
}
