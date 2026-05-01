// § tests/cocreative_overlay.rs — SIG0003 Sensitive<*> ban + bias overlay
// ════════════════════════════════════════════════════════════════════
// § I> 4 tests : structural-rejection · runtime audit-emit · bias-determinism ·
//   mood-mapping. NEVER reads Sensitive<biometric|gaze|face|body>.
// ════════════════════════════════════════════════════════════════════

use cssl_host_npc_bt::DetRng;
use cssl_host_npc_bt::audit::{RecordingAuditSink, kind};
use cssl_host_npc_bt::cocreative_overlay::{
    Mood, SensitiveScopeViolation, bias_modulate_dialogue_choice, bias_mood_color,
    record_scope_violation,
};

#[test]
fn sensitive_input_emits_scope_violation_audit() {
    let rec = RecordingAuditSink::new();
    record_scope_violation(SensitiveScopeViolation::Biometric, &rec);
    record_scope_violation(SensitiveScopeViolation::Gaze, &rec);
    record_scope_violation(SensitiveScopeViolation::Face, &rec);
    record_scope_violation(SensitiveScopeViolation::Body, &rec);
    assert_eq!(rec.count_kind(kind::SCOPE_VIOLATION), 4);
    let evs = rec.events();
    let sigs: Vec<_> = evs
        .iter()
        .filter_map(|e| e.attribs.get("sig"))
        .map(String::as_str)
        .collect();
    assert!(sigs.iter().all(|s| *s == "SIG0003"));
}

#[test]
fn structural_rejection_at_type_level() {
    // The fact that this test compiles + the public API of
    // `bias_modulate_dialogue_choice` / `bias_mood_color` accepts NO
    // Sensitive<biometric|gaze|face|body> typed inputs IS the structural
    // proof. Here we exercise the public surface and verify it accepts only :
    //  - bias-vec [f32; 16]            ← player-authored cocreative-bias
    //  - reputation f32                ← derived rep-score
    //  - rng + dialogue-pool           ← runtime-deterministic select
    // NO sensor-feed ; NO camera ; NO biometric.
    let mut rng = DetRng::new(0);
    let pool = [1_u32, 2, 3];
    let bias = [0.5_f32; 16];
    let pick = bias_modulate_dialogue_choice(&pool, &bias, &mut rng);
    assert!(pool.contains(&pick));
    let _m = bias_mood_color(0.5);
}

#[test]
fn bias_modulate_deterministic() {
    let pool = [10_u32, 20, 30, 40, 50];
    let bias = [
        1.0, 0.0, 1.0, 0.0, 1.0, 0.0, 1.0, 0.0, 1.0, 0.0, 1.0, 0.0, 1.0, 0.0, 1.0, 0.0,
    ];
    let mut r1 = DetRng::new(123);
    let mut r2 = DetRng::new(123);
    for _ in 0..100 {
        assert_eq!(
            bias_modulate_dialogue_choice(&pool, &bias, &mut r1),
            bias_modulate_dialogue_choice(&pool, &bias, &mut r2)
        );
    }
}

#[test]
fn mood_color_full_spectrum() {
    assert_eq!(bias_mood_color(-1.0), Mood::Aloof);
    assert_eq!(bias_mood_color(-0.5), Mood::Terse);
    assert_eq!(bias_mood_color(0.0), Mood::Plain);
    assert_eq!(bias_mood_color(0.5), Mood::Warm);
    assert_eq!(bias_mood_color(1.0), Mood::Poetic);
    // Out-of-range clamps :
    assert_eq!(bias_mood_color(-99.0), Mood::Aloof);
    assert_eq!(bias_mood_color(99.0), Mood::Poetic);
}
